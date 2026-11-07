//! Rust coverage for the C ABI facade.
//!
//! These tests drive the real `extern "C"` entry points with a RAM target whose
//! callbacks are Rust `extern "C"` functions, so they exercise the same path a
//! C host takes: the callback table, the payload mapping, thread affinity,
//! re-entry rejection, panic poisoning, grant time authority and result
//! classification.  `tests/capi/smoke.c` covers the same surface from plain C.

use super::host::{RunState, Session};
use super::target::{
    atomic_kind, atomic_outcome, device_event, device_window, dmi_access, dmi_result, CDebug,
    CDevice, CDirectMem, CPayload, CTarget, SNAPSHOT_CAPACITY,
};
use super::{
    ruscv_machine_attach_device, ruscv_machine_boundary, ruscv_machine_create,
    ruscv_machine_destroy, ruscv_machine_diagnostic, ruscv_machine_invalidate_direct_mem,
    ruscv_machine_load_elf, ruscv_machine_reset, ruscv_machine_result, ruscv_machine_run_grant,
    ruscv_machine_self_test_panic, ruscv_machine_set_commit_log, CBoundary, CGrant, CResult,
    RuscvMachine, Status,
};
use crate::blocking::{BlockingDeviceTarget, Femtoseconds, TickPeriod, TurnCompletionBound};
use crate::elf::LoadImage;
use crate::executor::observation_fixture as elf;
use crate::machine::{IsaProfile, MachineConfig};
use std::ffi::c_void;
use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicPtr, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::{Arc, Mutex};

/// One nanosecond, the documented default tick period.
const TICK_FS: u64 = 1_000_000;

/// One and a half Platform ticks, the host elapsed time every suspension
/// fixture reports (nb D4/D5).
///
/// Its charge rounds up to two whole ticks, so a total that used `e` where the
/// contract charges the rounded amount, or the reverse, is visible in a tick
/// difference.
const ELAPSED_HALF_TICK: u64 = TICK_FS + TICK_FS / 2;

/// One `b_transport` the C target served, as the ABI published it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AbiCall {
    address: u64,
    access_kind: i32,
    /// The `ruscv_payload.local_offset_fs` request field (quantum D11).
    local_offset_fs: u64,
    /// The `ruscv_payload.may_suspend` request field (nb D3/D6).
    may_suspend: i32,
}

/// A RAM target whose callbacks are the C ABI's own calling convention.
///
/// Its granted storage is an array of `AtomicU8`, because a direct-memory
/// grant promises relaxed byte-wise atomic access (direct-memory D10).
struct TestTarget {
    base: u64,
    ram: Mutex<Vec<AtomicU8>>,
    /// Status the target reports for every blocking transaction.
    status: AtomicI32,
    /// Mark every blocking transaction outcome-unknown.
    unknown: AtomicBool,
    /// Mark a data read outcome-unknown, once the grant is cached.
    unknown_on_data_read: AtomicBool,
    /// Set `dmi_allowed` on every completed non-atomic transaction.
    hint: AtomicBool,
    /// The inclusive range a query grants, when any.
    grant: Mutex<Option<(u64, u64)>>,
    /// Number of direct-memory queries.
    queries: AtomicUsize,
    /// The request half of the most recent direct-memory query.
    last_query: Mutex<Option<(u64, usize, i32)>>,
    /// Invalidate the whole space from inside a data-read `b_transport`.
    invalidate_in_read: AtomicBool,
    /// Status the in-callback invalidation returned, or `i32::MIN`.
    invalidate_status: AtomicI32,
    /// Number of in-callback invalidations attempted.
    invalidations: AtomicUsize,
    /// Number of transport resets the target acknowledged.
    reset_acks: AtomicUsize,
    /// Whether the target acknowledges a transport reset.
    reset_ack: AtomicBool,
    /// Number of debug-transport calls.
    debug_calls: AtomicUsize,
    /// Number of blocking transactions.
    transactions: AtomicUsize,
    /// Status a nested ABI call returned from inside `b_transport`, or
    /// `i32::MIN` before the attempt.
    reentry_status: AtomicI32,
    /// The handle a callback re-enters, when set.
    handle: AtomicPtr<RuscvMachine>,
    /// A second handle a callback enters, when set.
    second_handle: AtomicPtr<RuscvMachine>,
    /// Status the nested call into `second_handle` returned, or `i32::MIN`.
    second_status: AtomicI32,
    /// Make debug-transport reads fail (a short transfer) after installation.
    fail_dbg_reads: AtomicBool,
    /// Make debug-transport writes fail (a short transfer) after installation.
    fail_dbg_writes: AtomicBool,
    /// Delay annotated on every blocking transaction, in femtoseconds.
    delay_fs: AtomicU64,
    /// Change the request payload once, on the guest's first data read.
    mutate_payload: AtomicBool,
    /// The table's `suspends_host_time` declaration (nb D6).
    suspends: AtomicBool,
    /// The host elapsed time `e` every blocking transaction reports (nb D4).
    host_elapsed_fs: AtomicU64,
    /// The device-event tag every blocking transaction reports (device D8).
    event: AtomicI32,
    /// Probe the entries a suspended callback leaves refused (nb D6).
    probe_entries: AtomicBool,
    /// Status `ruscv_machine_run_grant` returned from inside a callback, or
    /// `i32::MIN` before the attempt.
    run_grant_status: AtomicI32,
    /// Status `ruscv_machine_destroy` returned from inside a callback, or
    /// `i32::MIN` before the attempt.
    destroy_status: AtomicI32,
    /// Every blocking transaction in order, with its published local offset.
    calls: Mutex<Vec<AbiCall>>,
}

impl TestTarget {
    fn new(base: u64, size: usize) -> Self {
        Self {
            base,
            ram: Mutex::new((0..size).map(|_| AtomicU8::new(0)).collect()),
            status: AtomicI32::new(super::target::status::OK),
            unknown: AtomicBool::new(false),
            unknown_on_data_read: AtomicBool::new(false),
            hint: AtomicBool::new(false),
            grant: Mutex::new(None),
            queries: AtomicUsize::new(0),
            last_query: Mutex::new(None),
            invalidate_in_read: AtomicBool::new(false),
            invalidate_status: AtomicI32::new(i32::MIN),
            invalidations: AtomicUsize::new(0),
            reset_acks: AtomicUsize::new(0),
            reset_ack: AtomicBool::new(true),
            debug_calls: AtomicUsize::new(0),
            transactions: AtomicUsize::new(0),
            reentry_status: AtomicI32::new(i32::MIN),
            handle: AtomicPtr::new(std::ptr::null_mut()),
            second_handle: AtomicPtr::new(std::ptr::null_mut()),
            second_status: AtomicI32::new(i32::MIN),
            fail_dbg_reads: AtomicBool::new(false),
            fail_dbg_writes: AtomicBool::new(false),
            delay_fs: AtomicU64::new(0),
            mutate_payload: AtomicBool::new(false),
            suspends: AtomicBool::new(false),
            host_elapsed_fs: AtomicU64::new(0),
            event: AtomicI32::new(device_event::NONE),
            probe_entries: AtomicBool::new(false),
            run_grant_status: AtomicI32::new(i32::MIN),
            destroy_status: AtomicI32::new(i32::MIN),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// The recorded transactions, in order.
    fn calls(&self) -> Vec<AbiCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Forgets the recorded transactions.
    fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    fn set_delay(&self, delay_fs: u64) {
        self.delay_fs.store(delay_fs, Ordering::SeqCst);
    }

    fn set_host_elapsed(&self, elapsed_fs: u64) {
        self.host_elapsed_fs.store(elapsed_fs, Ordering::SeqCst);
    }

    fn set_suspends(&self, suspends: bool) {
        self.suspends.store(suspends, Ordering::SeqCst);
    }

    fn mutate_payload(&self) {
        self.mutate_payload.store(true, Ordering::SeqCst);
    }

    fn run_grant_status(&self) -> i32 {
        self.run_grant_status.load(Ordering::SeqCst)
    }

    fn destroy_status(&self) -> i32 {
        self.destroy_status.load(Ordering::SeqCst)
    }

    fn reentry_status(&self) -> i32 {
        self.reentry_status.load(Ordering::SeqCst)
    }

    fn second_status(&self) -> i32 {
        self.second_status.load(Ordering::SeqCst)
    }

    fn queries(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }

    fn transactions(&self) -> usize {
        self.transactions.load(Ordering::SeqCst)
    }

    fn last_query(&self) -> Option<(u64, usize, i32)> {
        *self.last_query.lock().unwrap()
    }

    /// The half-open window of one access, clamped to the modeled RAM.
    ///
    /// A callback must never panic across the C ABI, so an access outside the
    /// window is served as zero bytes instead of aborting the test process.
    fn window(&self, address: u64, len: usize) -> (usize, usize) {
        let offset = (address - self.base) as usize;
        let size = self.ram.lock().unwrap().len();
        let start = offset.min(size);
        (start, (offset + len).min(size))
    }

    /// The host pointer of one granted physical address.
    fn pointer_at(&self, address: u64) -> *mut u8 {
        let offset = (address - self.base) as usize;
        let ram = self.ram.lock().unwrap();
        ram.as_ptr().wrapping_add(offset).cast_mut().cast::<u8>()
    }
}

/// Copies words out of the target's atomic bytes.
fn ram_read(ram: &[AtomicU8], offset: usize, bytes: &mut [u8]) {
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ram[offset + index].load(Ordering::Relaxed);
    }
}

/// Copies words into the target's atomic bytes.
fn ram_write(ram: &[AtomicU8], offset: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        ram[offset + index].store(*byte, Ordering::Relaxed);
    }
}

unsafe extern "C" fn target_b_transport(user: *mut c_void, payload: *mut CPayload) {
    // SAFETY: the ABI guarantees `user` is the live `TestTarget` and `payload`
    // is one live Rust-owned payload struct for this call.
    let (target, payload) = unsafe { (&*(user.cast::<TestTarget>()), &mut *payload) };
    target.transactions.fetch_add(1, Ordering::SeqCst);
    target.calls.lock().unwrap().push(AbiCall {
        address: payload.address,
        access_kind: payload.access_kind,
        local_offset_fs: payload.local_offset_fs,
        may_suspend: payload.may_suspend,
    });
    let handle = target.handle.load(Ordering::SeqCst);
    if target.probe_entries.swap(false, Ordering::SeqCst) && !handle.is_null() {
        // nb D6: this is the state a suspended `b_transport` callback leaves --
        // its thread-local flag stays set -- so run_grant and destroy must be
        // refused as re-entry while the one exempt entry stays accepted.
        let mut grant = CGrant::default();
        // SAFETY: the handle is live and the call is made from this thread.
        let status = unsafe { ruscv_machine_run_grant(handle, 0, u64::MAX / 2, 1, &mut grant) };
        target.run_grant_status.store(status, Ordering::SeqCst);
        // SAFETY: a refused destroy returns without freeing the handle.
        let status = unsafe { ruscv_machine_destroy(handle) };
        target.destroy_status.store(status, Ordering::SeqCst);
        // SAFETY: the handle is live on this thread.
        let status = unsafe { ruscv_machine_invalidate_direct_mem(handle, 0, u64::MAX) };
        target.invalidate_status.store(status, Ordering::SeqCst);
        target.invalidations.fetch_add(1, Ordering::SeqCst);
    }
    if !handle.is_null() && target.reentry_status.load(Ordering::SeqCst) == i32::MIN {
        let mut facts = CBoundary::default();
        // SAFETY: the handle is live and the call is made from this thread.
        let status = unsafe { ruscv_machine_boundary(handle, &mut facts) };
        target.reentry_status.store(status, Ordering::SeqCst);
    }
    let second = target.second_handle.load(Ordering::SeqCst);
    if !second.is_null() && target.second_status.load(Ordering::SeqCst) == i32::MIN {
        let mut result = CResult::default();
        // SAFETY: the second handle is live on the same thread.
        let status = unsafe { ruscv_machine_result(second, &mut result) };
        target.second_status.store(status, Ordering::SeqCst);
    }
    let len = payload.length;
    // SAFETY: the ABI guarantees a Rust-owned buffer of exactly `length` bytes
    // that this callback exclusively borrows for its duration.
    let data = unsafe { std::slice::from_raw_parts_mut(payload.data, len) };
    let (start, _end) = target.window(payload.address, len);
    let ram = target.ram.lock().unwrap();
    if payload.command == super::target::command::READ {
        ram_read(&ram, start, data);
    } else {
        ram_write(&ram, start, data);
    }
    drop(ram);
    payload.status = target.status.load(Ordering::SeqCst);
    // device D8: the field is a response field for every table, so a RAM
    // target that reports one is refused by the RAM adapter, not ignored here.
    payload.device_event = target.event.load(Ordering::SeqCst);
    payload.delay_fs = target.delay_fs.load(Ordering::SeqCst);
    // nb D4: the host time that passed inside this callback.  The fixture
    // reports a scripted value instead of really suspending.
    payload.host_elapsed_fs = target.host_elapsed_fs.load(Ordering::SeqCst);
    if target.unknown.load(Ordering::SeqCst)
        || (target.unknown_on_data_read.load(Ordering::SeqCst)
            && payload.access_kind == super::target::access_kind::DATA_READ)
    {
        payload.outcome_unknown = 1;
    }
    // direct-memory D3: the hint is a response hint on a completed non-atomic
    // transaction, and it is the only licence for one query.
    if target.hint.load(Ordering::SeqCst)
        && payload.status == super::target::status::OK
        && payload.atomic.present == 0
    {
        payload.dmi_allowed = 1;
    }
    if target.invalidate_in_read.load(Ordering::SeqCst)
        && payload.access_kind == super::target::access_kind::DATA_READ
        && payload.atomic.present == 0
        && target.invalidations.load(Ordering::SeqCst) == 0
    {
        // direct-memory D12: this is the one ABI entry a callback may call.  It
        // must be accepted here even though the outer call still holds the
        // adapter's state lock.
        let handle = target.handle.load(Ordering::SeqCst);
        target.invalidations.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the handle is live on this thread.
        let status = unsafe { ruscv_machine_invalidate_direct_mem(handle, 0, u64::MAX) };
        target.invalidate_status.store(status, Ordering::SeqCst);
    }
    // nb D4 step 2 / D6: change the request payload once, after the bytes were
    // served, so the ABI turns this response into `Incomplete` while it keeps
    // copying the reported host elapsed time back.
    if payload.access_kind == super::target::access_kind::DATA_READ
        && target.mutate_payload.swap(false, Ordering::SeqCst)
    {
        payload.address = payload.address.wrapping_add(8);
    }
}

unsafe extern "C" fn target_dbg(user: *mut c_void, debug: *mut CDebug) -> usize {
    // SAFETY: as in `target_b_transport`.
    let (target, debug) = unsafe { (&*(user.cast::<TestTarget>()), &mut *debug) };
    target.debug_calls.fetch_add(1, Ordering::SeqCst);
    let len = debug.length;
    let fail = if debug.command == super::target::command::READ {
        target.fail_dbg_reads.load(Ordering::SeqCst)
    } else {
        target.fail_dbg_writes.load(Ordering::SeqCst)
    };
    if fail {
        // A short debug transport is a host failure, never a guest fault.
        return len.saturating_sub(1);
    }
    // SAFETY: as in `target_b_transport`.
    let data = unsafe { std::slice::from_raw_parts_mut(debug.data, len) };
    let (start, _end) = target.window(debug.address, len);
    let ram = target.ram.lock().unwrap();
    if debug.command == super::target::command::READ {
        ram_read(&ram, start, data);
    } else {
        ram_write(&ram, start, data);
    }
    len
}

/// Answers one direct-memory query from the target's configured grant.
unsafe extern "C" fn target_get_direct_mem_ptr(user: *mut c_void, direct: *mut CDirectMem) -> i32 {
    // SAFETY: as in `target_b_transport`.
    let (target, direct) = unsafe { (&*(user.cast::<TestTarget>()), &mut *direct) };
    target.queries.fetch_add(1, Ordering::SeqCst);
    *target.last_query.lock().unwrap() = Some((direct.address, direct.length, direct.access));
    let grant = *target.grant.lock().unwrap();
    let Some((start, end)) = grant else {
        return dmi_result::DENIED;
    };
    if direct.address < start || direct.address > end {
        // The range must contain the queried address; a target that cannot
        // cover it denies instead of answering with a malformed grant.
        return dmi_result::DENIED;
    }
    direct.start = start;
    direct.end = end;
    direct.pointer = target.pointer_at(start);
    direct.access = dmi_access::READ_WRITE;
    direct.read_latency_fs = 0;
    direct.write_latency_fs = 0;
    dmi_result::GRANTED
}

unsafe extern "C" fn target_reset(user: *mut c_void) -> i32 {
    // SAFETY: as in `target_b_transport`.
    let target = unsafe { &*(user.cast::<TestTarget>()) };
    let ack = target.reset_ack.load(Ordering::SeqCst);
    if ack {
        target.reset_acks.fetch_add(1, Ordering::SeqCst);
    }
    i32::from(ack)
}

impl TestTarget {
    fn table(&mut self, max_delay_fs: u64) -> CTarget {
        self.quantum_table(max_delay_fs, 0)
    }

    /// The same table selecting a quantum (quantum D2/D12).
    fn quantum_table(&mut self, max_delay_fs: u64, quantum: u64) -> CTarget {
        CTarget {
            max_delay_fs,
            user: std::ptr::from_mut(self).cast::<c_void>(),
            b_transport: Some(target_b_transport),
            transport_dbg: Some(target_dbg),
            transport_reset: Some(target_reset),
            get_direct_mem_ptr: None,
            quantum_ticks: quantum,
            // nb D6: the declaration is part of the copied table, so it comes
            // from the fixture's own setting.
            suspends_host_time: i32::from(self.suspends.load(Ordering::SeqCst)),
        }
    }

    /// The same table with a non-NULL direct-memory query, which is what
    /// selects direct memory for the handle (direct-memory D12).
    fn direct_table(&mut self, max_delay_fs: u64) -> CTarget {
        CTarget {
            get_direct_mem_ptr: Some(target_get_direct_mem_ptr),
            ..self.table(max_delay_fs)
        }
    }
}

/// One installed guest and its handle.
struct Harness {
    handle: *mut RuscvMachine,
    target: Box<TestTarget>,
    _elf: tempfile::NamedTempFile,
}

impl Drop for Harness {
    fn drop(&mut self) {
        // SAFETY: the handle is live until this drop and was created here.
        unsafe { ruscv_machine_destroy(self.handle) };
    }
}

/// A guest that reports HTIF exit `code` through the declared `.tohost`.
///
/// The first parcel is two compressed `c.nop`s, so the image only executes
/// under an IALIGN=16 profile; every profile without compression traps on it.
fn exit_guest(code: u32) -> Vec<u32> {
    vec![
        0x0001_0001,
        // The declared `.tohost` sits at 0x8000_1000; LUI alone would
        // sign-extend it, so build the address from a positive shifted value.
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::standard_exit(code),
        elf::sd(5, 4, 0),
        0x0000_006f,
    ]
}

fn elf_file(code: &[u32]) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&elf::elf_with_code(code, 0, true, false, 0x4000))
        .unwrap();
    file.flush().unwrap();
    file
}

/// A guest whose only data access is a load from `.tohost`.
///
/// The load is outside every code-page grant, so it always takes one
/// `b_transport`.  That is the access a fixture can invalidate from inside a
/// callback, and the fetches after it prove the invalidation took effect.
fn direct_guest(code: u32) -> Vec<u32> {
    vec![
        0x0001_0001,
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::ld(6, 4, 0),
        elf::standard_exit(code),
        // A store outside the declared `.tohost` word first: the exit poll
        // clears the signal, so this is the byte a test can inspect afterwards.
        elf::sd(5, 4, 8),
        elf::sd(5, 4, 0),
        0x0000_006f,
    ]
}

fn c_text(path: &std::path::Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_str().unwrap()).unwrap()
}

/// Runs one grant through the real entry point.
fn handle_grant(
    handle: *mut RuscvMachine,
    start_fs: u64,
    max_end_fs: u64,
    budget: u64,
) -> Result<CGrant, i32> {
    let mut grant = CGrant::default();
    // SAFETY: the handle is live.
    let status =
        unsafe { ruscv_machine_run_grant(handle, start_fs, max_end_fs, budget, &mut grant) };
    if status == Status::Ok as i32 {
        Ok(grant)
    } else {
        Err(status)
    }
}

/// Reads the run classification through the real entry point.
fn handle_result(handle: *mut RuscvMachine) -> CResult {
    let mut result = CResult::default();
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_result(handle, &mut result) };
    assert_eq!(status, Status::Ok as i32, "result");
    result
}

/// Reads the last grant's facts through the real entry point.
fn handle_facts(handle: *mut RuscvMachine) -> CBoundary {
    let mut facts = CBoundary::default();
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_boundary(handle, &mut facts) };
    assert_eq!(status, Status::Ok as i32, "boundary");
    facts
}

/// Reads the handle's diagnostic message.
fn handle_diagnostic(handle: *mut RuscvMachine) -> String {
    let mut buffer = vec![0 as std::ffi::c_char; 1024];
    // SAFETY: the handle is live and the buffer is writable.
    unsafe { ruscv_machine_diagnostic(handle, buffer.as_mut_ptr(), buffer.len()) };
    let text: Vec<u8> = buffer
        .iter()
        .map(|byte| *byte as u8)
        .take_while(|byte| *byte != 0)
        .collect();
    String::from_utf8(text).unwrap()
}

/// Runs bounded grants until the guest exits or the grant count runs out.
///
/// Each grant starts at the Machine's committed time the previous grant
/// returned, which is exactly what an external kernel commits (ADR-0004
/// §4.4).
fn handle_run_to_result(handle: *mut RuscvMachine, budget: u64, grants: usize) -> CResult {
    let mut start = 0;
    for _ in 0..grants {
        let Ok(grant) = handle_grant(handle, start, u64::MAX / 2, budget) else {
            break;
        };
        start = grant.committed_end_fs;
        let result = handle_result(handle);
        if result.state != RunState::Running.code() {
            return result;
        }
    }
    handle_result(handle)
}

impl Harness {
    fn new(isa: i32, code: &[u32], max_delay_fs: u64) -> Self {
        Self::build(isa, code, max_delay_fs, false)
    }

    /// One handle whose target answers a direct-memory query.
    fn direct(isa: i32, code: &[u32], max_delay_fs: u64) -> Self {
        Self::build(isa, code, max_delay_fs, true)
    }

    /// One handle whose copied target table selects `quantum` (D12).
    fn quantum(isa: i32, code: &[u32], max_delay_fs: u64, quantum: u64) -> Self {
        Self::build_full(isa, code, max_delay_fs, false, quantum, 0, false, 0)
    }

    /// One handle whose copied target table declares host-time suspension and
    /// scripts the `e` every transaction reports (nb D4/D6).
    fn suspending(
        isa: i32,
        code: &[u32],
        max_delay_fs: u64,
        host_elapsed_fs: u64,
        delay_fs: u64,
    ) -> Self {
        Self::build_full(
            isa,
            code,
            max_delay_fs,
            false,
            0,
            delay_fs,
            true,
            host_elapsed_fs,
        )
    }

    fn build(isa: i32, code: &[u32], max_delay_fs: u64, direct: bool) -> Self {
        Self::build_full(isa, code, max_delay_fs, direct, 0, 0, false, 0)
    }

    fn build_quantum(
        isa: i32,
        code: &[u32],
        max_delay_fs: u64,
        direct: bool,
        quantum: u64,
        delay_fs: u64,
    ) -> Self {
        Self::build_full(isa, code, max_delay_fs, direct, quantum, delay_fs, false, 0)
    }

    // The fixture is one handle with every knob the ABI copies into its
    // configuration at create time, so one constructor carries them all.
    #[allow(clippy::too_many_arguments)]
    fn build_full(
        isa: i32,
        code: &[u32],
        max_delay_fs: u64,
        direct: bool,
        quantum: u64,
        delay_fs: u64,
        suspends: bool,
        host_elapsed_fs: u64,
    ) -> Self {
        let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
        target.hint.store(true, Ordering::SeqCst);
        target.set_delay(delay_fs);
        target.set_host_elapsed(host_elapsed_fs);
        target.set_suspends(suspends);
        let table = if direct {
            target.direct_table(max_delay_fs)
        } else {
            target.quantum_table(max_delay_fs, quantum)
        };
        let mut handle = std::ptr::null_mut();
        // SAFETY: the table and its user pointer stay alive in this harness.
        let status = unsafe { ruscv_machine_create(&table, isa, TICK_FS, &mut handle) };
        assert_eq!(status, Status::Ok as i32, "create");
        let harness = Self {
            handle,
            target,
            _elf: elf_file(code),
        };
        harness.target.handle.store(handle, Ordering::SeqCst);
        let path = c_text(harness._elf.path());
        // SAFETY: the handle is live and the path is NUL-terminated.
        let status = unsafe { ruscv_machine_load_elf(handle, path.as_ptr()) };
        assert_eq!(status, Status::Ok as i32, "load_elf");
        harness
    }

    fn grant(&self, start_fs: u64, max_end_fs: u64, budget: u64) -> Result<CGrant, i32> {
        handle_grant(self.handle, start_fs, max_end_fs, budget)
    }

    fn result(&self) -> CResult {
        handle_result(self.handle)
    }

    fn facts(&self) -> CBoundary {
        handle_facts(self.handle)
    }

    fn diagnostic(&self) -> String {
        handle_diagnostic(self.handle)
    }

    fn run_to_result(&self, budget: u64, grants: usize) -> CResult {
        handle_run_to_result(self.handle, budget, grants)
    }

    /// Configures the inclusive range this target's queries grant.
    fn grant_range(&self, range: Option<(u64, u64)>) {
        *self.target.grant.lock().unwrap() = range;
    }

    fn invalidate(&self, start: u64, end: u64) -> i32 {
        // SAFETY: the handle is live and this is the creating thread.
        unsafe { ruscv_machine_invalidate_direct_mem(self.handle, start, end) }
    }

    fn reset(&self) -> i32 {
        // SAFETY: the handle is live and this is the creating thread.
        unsafe { ruscv_machine_reset(self.handle) }
    }
}

/// The code page above the first two parcels.
///
/// The first two fetches and the `.tohost` load are outside it, so a fixture
/// has a deterministic `b_transport` between cached code fetches.
fn code_grant() -> (u64, u64) {
    (elf::BASE + 8, elf::BASE + 0x1000 - 1)
}

#[test]
fn a_direct_memory_grant_serves_covered_accesses_without_transport() {
    // A whole-RAM read-write grant serves the fetches, the only data load and
    // the exit store, so the guest proves the direct path in one run.
    let direct = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    direct.grant_range(Some((elf::BASE, elf::BASE + 0x1_0000 - 1)));
    let result = direct.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        direct.diagnostic()
    );
    assert_eq!(result.exit_code, 2);
    assert_eq!(
        direct.target.queries(),
        1,
        "one hint is one query, and a grant answers every later access"
    );
    assert_eq!(
        direct.target.last_query(),
        Some((elf::BASE, 2, dmi_access::READ)),
        "the query carries the completed access's address, width and direction; the \
         first parcel is one compressed instruction, so it is 2 bytes wide"
    );
    let direct_transactions = direct.target.transactions();
    assert_eq!(
        direct_transactions, 1,
        "only the first uncovered fetch may reach b_transport"
    );
    // The image was installed through `transport_dbg` and the fetches read it
    // back through the granted pointer, while the direct store landed in the
    // target's own bytes: the pointer, `transport_dbg` and the target share one
    // storage, as direct-memory D10's aliasing promise requires.  The witness
    // is the store outside `.tohost`, because the exit poll clears the signal.
    let offset = (elf::TOHOST + 8 - elf::BASE) as usize;
    let ram = direct.target.ram.lock().unwrap();
    assert_eq!(
        ram[offset].load(Ordering::Relaxed) as u64
            | (ram[offset + 1].load(Ordering::Relaxed) as u64) << 8,
        5,
        "the direct store did not reach the target's own bytes"
    );
    drop(ram);

    // The same guest with a NULL callback: direct memory is off, and every
    // access is one transaction.
    let off = Harness::new(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    let result = off.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(off.target.queries(), 0, "a NULL callback issues no query");
    assert!(
        direct_transactions < off.target.transactions(),
        "a grant did not remove any transaction: {} direct vs {} blocking",
        direct_transactions,
        off.target.transactions()
    );
}

#[test]
fn a_null_direct_memory_callback_behaves_exactly_as_direct_memory_off() {
    // A target that sets the hint but has no query is the same run as a target
    // that never sets it: the NULL callback selected no direct memory.
    let hinted = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let result = hinted.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(hinted.target.queries(), 0);

    let plain = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    plain.target.hint.store(false, Ordering::SeqCst);
    let result = plain.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(plain.target.queries(), 0);
    assert_eq!(
        hinted.target.transactions(),
        plain.target.transactions(),
        "a NULL callback changed the blocking route"
    );
}

#[test]
fn a_denied_direct_memory_grant_caches_nothing() {
    // A non-NULL callback selects direct memory, but every answer here is a
    // denial: each hinted access queries once and then still takes its one
    // blocking transaction.
    let denied = Harness::direct(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    denied.grant_range(None);
    let result = denied.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    assert!(denied.target.queries() > 0, "direct memory issued no query");

    let off = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let result = off.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(
        denied.target.transactions(),
        off.target.transactions(),
        "a denial cached a grant or changed the blocking route"
    );
}

#[test]
fn an_invalidation_between_grants_takes_effect_before_the_next_access() {
    let invalidated = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    invalidated.grant_range(Some(code_grant()));
    let baseline = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    baseline.grant_range(Some(code_grant()));

    // Four turns reach the first granted fetch, so a grant is cached and the
    // next fetch would use it without an invalidation.
    let first = invalidated.grant(0, u64::MAX / 2, 4).unwrap();
    let baseline_first = baseline.grant(0, u64::MAX / 2, 4).unwrap();
    assert_eq!(first.facts.turns, 4);
    assert_eq!(baseline_first.facts.turns, 4);
    assert!(invalidated.target.queries() > 0, "no grant was cached");
    let before = invalidated.target.transactions();
    assert_eq!(before, baseline.target.transactions(), "fixture divergence");

    assert_eq!(invalidated.invalidate(0, u64::MAX), Status::Ok as i32);
    assert_eq!(
        invalidated.target.transactions(),
        before,
        "invalidation itself touches no target"
    );

    // The next one-turn grant starts with a fetch.  With the grant removed it
    // must take exactly one b_transport first; the baseline fetches directly.
    let next = invalidated
        .grant(first.committed_end_fs, u64::MAX / 2, 1)
        .unwrap();
    assert_eq!(next.facts.turns, first.facts.turns + 1);
    assert_eq!(
        invalidated.target.transactions(),
        before + 1,
        "the fetch after an invalidation did not take b_transport exactly once"
    );
    let baseline_next = baseline
        .grant(baseline_first.committed_end_fs, u64::MAX / 2, 1)
        .unwrap();
    assert_eq!(baseline_next.facts.turns, baseline_first.facts.turns + 1);
    assert_eq!(
        baseline.target.transactions(),
        before,
        "the same fetch used the cached grant without an invalidation"
    );

    // The invalidation changed no architectural fact: both runs still exit.
    let remaining = invalidated.grant(next.committed_end_fs, u64::MAX / 2, u64::MAX);
    assert!(remaining.is_ok());
    assert_eq!(
        invalidated.result().state,
        RunState::Exited.code(),
        "diagnostic: {}",
        invalidated.diagnostic()
    );
    let baseline_remaining = baseline.grant(baseline_next.committed_end_fs, u64::MAX / 2, u64::MAX);
    assert!(baseline_remaining.is_ok());
    assert_eq!(baseline.result().state, RunState::Exited.code());
}

#[test]
fn an_invalidation_inside_a_callback_takes_effect_before_the_next_access() {
    let invalidated = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    invalidated.grant_range(Some(code_grant()));
    invalidated
        .target
        .invalidate_in_read
        .store(true, Ordering::SeqCst);
    let baseline = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    baseline.grant_range(Some(code_grant()));

    let invalidated_result = invalidated.run_to_result(1_000, 16);
    let baseline_result = baseline.run_to_result(1_000, 16);
    assert_eq!(invalidated_result.state, RunState::Exited.code());
    assert_eq!(baseline_result.state, RunState::Exited.code());
    assert_eq!(
        invalidated.target.invalidate_status.load(Ordering::SeqCst),
        Status::Ok as i32,
        "the invalidation inside b_transport was rejected"
    );
    assert_eq!(
        invalidated.target.invalidations.load(Ordering::SeqCst),
        1,
        "the callback did not run the invalidation exactly once"
    );
    assert_eq!(
        invalidated.target.transactions(),
        baseline.target.transactions() + 1,
        "the granted fetch after the in-callback invalidation did not take b_transport"
    );
}

#[test]
fn an_invalidation_on_another_thread_is_rejected_and_changes_nothing() {
    let harness = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    harness.grant_range(Some(code_grant()));
    let grant = harness.grant(0, u64::MAX / 2, 4).unwrap();
    assert_eq!(grant.facts.turns, 4);
    let before = harness.target.transactions();
    assert!(harness.target.queries() > 0, "no grant was cached");

    let handle = harness.handle as usize;
    let status = std::thread::spawn(move || {
        let handle = handle as *mut RuscvMachine;
        // SAFETY: the handle is live until the join below, and the callee must
        // reject this thread without dereferencing run state.
        unsafe { ruscv_machine_invalidate_direct_mem(handle, 0, u64::MAX) }
    })
    .join()
    .unwrap();
    assert_eq!(
        status,
        Status::NotCreatorThread as i32,
        "an invalidation from another thread was accepted"
    );

    let start = grant.committed_end_fs;
    let next = harness.grant(start, u64::MAX / 2, 1).unwrap();
    assert_eq!(next.facts.turns, grant.facts.turns + 1);
    assert_eq!(
        harness.target.transactions(),
        before,
        "the rejected invalidation removed a grant"
    );
}

#[test]
fn the_invalidation_entry_is_the_only_call_accepted_inside_a_callback() {
    let harness = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    harness.grant_range(Some(code_grant()));
    harness
        .target
        .invalidate_in_read
        .store(true, Ordering::SeqCst);
    let other = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    harness
        .target
        .second_handle
        .store(other.handle, Ordering::SeqCst);

    let result = harness.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        harness.diagnostic()
    );
    assert_eq!(
        harness.target.invalidate_status.load(Ordering::SeqCst),
        Status::Ok as i32,
        "the invalidation entry is not accepted inside its own callback"
    );
    assert_eq!(
        harness.target.reentry_status(),
        Status::Reentrant as i32,
        "a callback re-entered the owning handle"
    );
    assert_eq!(
        harness.target.second_status(),
        Status::Reentrant as i32,
        "a callback re-entered the C ABI through another handle"
    );
}

#[test]
fn an_invalidation_is_accepted_on_a_poisoned_handle() {
    // A caught panic poisons the handle; invalidation still only touches the
    // target wrapper's registry, so it remains usable.
    let harness = Harness::direct(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    // SAFETY: the handle is live on this thread.
    assert_eq!(
        unsafe { ruscv_machine_self_test_panic(harness.handle) },
        Status::Panic as i32
    );
    assert_eq!(
        harness.invalidate(0, u64::MAX),
        Status::Ok as i32,
        "a panic-poisoned handle refused an invalidation"
    );
}

#[test]
fn an_unknown_completion_with_direct_memory_poisons_and_reset_restores_grants() {
    let harness = Harness::direct(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    harness.grant_range(Some(code_grant()));
    harness
        .target
        .unknown_on_data_read
        .store(true, Ordering::SeqCst);

    let failed = harness.grant(0, u64::MAX / 2, 100).unwrap();
    assert_eq!(
        failed.facts.failed, 1,
        "an unknown completion is not a pass"
    );
    assert_eq!(failed.facts.uncertain, 1);
    assert_eq!(harness.result().state, RunState::Failed.code());
    let queries_before = harness.target.queries();
    assert!(queries_before > 0, "no grant was cached before the failure");
    // The transport is poisoned, not the handle: the direct-memory entry stays
    // accepted because it touches only the wrapper's registry.
    assert_eq!(harness.invalidate(0, u64::MAX), Status::Ok as i32);

    let transactions = harness.target.transactions();
    assert_eq!(
        harness.grant(0, u64::MAX / 2, 100).err(),
        Some(Status::UnknownCompletion as i32)
    );
    assert_eq!(
        harness.target.transactions(),
        transactions,
        "a transaction reached the poisoned transport"
    );

    harness
        .target
        .unknown_on_data_read
        .store(false, Ordering::SeqCst);
    // SAFETY: the handle is live on this thread.
    assert_eq!(
        unsafe { ruscv_machine_reset(harness.handle) },
        Status::Ok as i32
    );
    let result = harness.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "the reset domain did not reach the guest exit: {}",
        harness.diagnostic()
    );
    assert_eq!(result.exit_code, 2);
    assert!(
        harness.target.queries() > queries_before,
        "reset did not restore a direct-memory grant"
    );

    // The restored path removed transactions: the same guest with a NULL
    // callback needs strictly more.
    let off = Harness::new(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    let result = off.run_to_result(1_000, 16);
    assert_eq!(result.state, RunState::Exited.code());
    let after_reset = harness.target.transactions() - transactions;
    assert!(
        after_reset < off.target.transactions(),
        "the post-reset run used no direct memory: {after_reset} vs {}",
        off.target.transactions()
    );
}

#[test]
fn the_direct_memory_fields_are_final_in_their_structs() {
    // The published layout is x86-64 SysV, the ABI's only target.  Appending
    // must not move a pre-existing field; these offsets are the ones the
    // previous header published (direct-memory D12).  The name is the one the
    // direct-memory contract gave this pin; the quantum fields are appended
    // after it (quantum D12), then the suspension fields (nb D6), then the
    // device event (device D8), and every size it publishes grows with them.
    use std::mem::{offset_of, size_of};
    assert_eq!(offset_of!(CTarget, max_delay_fs), 0);
    assert_eq!(offset_of!(CTarget, user), 8);
    assert_eq!(offset_of!(CTarget, b_transport), 16);
    assert_eq!(offset_of!(CTarget, transport_dbg), 24);
    assert_eq!(offset_of!(CTarget, transport_reset), 32);
    assert_eq!(offset_of!(CTarget, get_direct_mem_ptr), 40);
    assert_eq!(offset_of!(CTarget, quantum_ticks), 48);
    assert_eq!(offset_of!(CTarget, suspends_host_time), 56);
    assert_eq!(size_of::<CTarget>(), 64);

    assert_eq!(offset_of!(CPayload, atomic), 56);
    assert_eq!(
        offset_of!(CPayload, dmi_allowed),
        offset_of!(CPayload, atomic) + size_of::<super::target::CAtomic>()
    );
    assert_eq!(offset_of!(CPayload, dmi_allowed), 240);
    assert_eq!(offset_of!(CPayload, local_offset_fs), 248);
    assert_eq!(offset_of!(CPayload, may_suspend), 256);
    assert_eq!(offset_of!(CPayload, host_elapsed_fs), 264);
    assert_eq!(offset_of!(CPayload, device_event), 272);
    assert_eq!(size_of::<CPayload>(), 280);

    // The device table the attach entry copies (device D8).  Its size and the
    // offset of every callback are as published, so a host's layout matches.
    assert_eq!(offset_of!(CDevice, window_base), 0);
    assert_eq!(offset_of!(CDevice, window_size), 8);
    assert_eq!(offset_of!(CDevice, user), 16);
    assert_eq!(offset_of!(CDevice, b_transport), 24);
    assert_eq!(offset_of!(CDevice, receive), 32);
    assert_eq!(offset_of!(CDevice, transport_reset), 40);
    assert_eq!(size_of::<CDevice>(), 48);

    // The three appended `ruscv_boundary` fields, so a host's reads of them
    // match the header's (quantum D12).
    assert_eq!(offset_of!(CBoundary, turns), 56);
    assert_eq!(offset_of!(CBoundary, quantum_ticks), 64);
    assert_eq!(offset_of!(CBoundary, interrupt_latency_bound_ticks), 72);
    assert_eq!(offset_of!(CBoundary, quantum_syncs), 80);
    assert_eq!(offset_of!(CBoundary, quantum_reached), 88);
    assert_eq!(size_of::<CBoundary>(), 96);

    // The grant's own host elapsed total, appended after its facts (nb D6).
    assert_eq!(offset_of!(CGrant, consumed_fs), 0);
    assert_eq!(offset_of!(CGrant, committed_start_fs), 8);
    assert_eq!(offset_of!(CGrant, committed_end_fs), 16);
    assert_eq!(offset_of!(CGrant, boundary_reached), 24);
    assert_eq!(offset_of!(CGrant, facts), 32);
    assert_eq!(offset_of!(CGrant, host_elapsed_fs), 128);
    assert_eq!(size_of::<CGrant>(), 136);
}

#[test]
fn a_guest_access_carries_may_suspend_only_to_a_suspending_table() {
    // nb D3/D6: the adapter's mark reaches the C payload unchanged, so a guest
    // load (and every fetch beside it) carries the flag exactly when the copied
    // table declares suspension.
    let suspending = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &direct_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    let result = suspending.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        suspending.diagnostic()
    );
    let calls = suspending.target.calls();
    assert!(
        calls
            .iter()
            .any(|call| call.access_kind == super::target::access_kind::DATA_READ),
        "the guest issued no data read: {calls:?}"
    );
    for call in &calls {
        assert_eq!(
            call.may_suspend, 1,
            "a guest access to a suspending target carried may_suspend {call:?}"
        );
    }

    let plain = Harness::new(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    let result = plain.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        plain.diagnostic()
    );
    let calls = plain.target.calls();
    assert!(
        calls
            .iter()
            .any(|call| call.access_kind == super::target::access_kind::DATA_READ),
        "the guest issued no data read: {calls:?}"
    );
    for call in &calls {
        assert_eq!(
            call.may_suspend, 0,
            "a guest access to a non-suspending target carried may_suspend {call:?}"
        );
    }
}

#[test]
fn each_grant_reports_only_its_own_host_elapsed_time() {
    // nb D5/D6: `host_elapsed_fs` is the increase over the grant, never the
    // cumulative generation total, so the second grant reports its own `W`.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &ram_only_exit_guest(12, 2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    let first = harness.grant(0, u64::MAX / 2, 4).expect("the first grant");
    let first_accesses = harness.target.transactions() as u64;
    assert!(first_accesses > 0, "the first grant ran no transaction");
    assert_eq!(
        first.host_elapsed_fs,
        first_accesses * ELAPSED_HALF_TICK,
        "the first grant did not report its own host elapsed total"
    );

    let second = harness
        .grant(first.committed_end_fs, u64::MAX / 2, 4)
        .expect("the second grant");
    let second_accesses = harness.target.transactions() as u64 - first_accesses;
    assert!(second_accesses > 0, "the second grant ran no transaction");
    assert_eq!(
        second.host_elapsed_fs,
        second_accesses * ELAPSED_HALF_TICK,
        "the second grant reported the cumulative host elapsed total"
    );

    // A grant that runs no turn reports zero, not the total so far.
    let idle = harness
        .grant(second.committed_end_fs, u64::MAX / 2, 0)
        .expect("the zero-budget grant");
    assert_eq!(idle.consumed_fs, 0, "the zero-budget grant consumed time");
    assert_eq!(
        idle.facts.turns, second.facts.turns,
        "the zero-budget grant ran a turn"
    );
    assert_eq!(
        idle.host_elapsed_fs, 0,
        "a grant that ran no turn reported host elapsed time"
    );
}

#[test]
fn a_changed_payload_keeps_its_host_elapsed_charge() {
    // nb D4 step 2 / D6: the ABI copies `e` back from the response it turns
    // into `Incomplete` for a changed payload, and the adapter charges it.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &direct_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    harness.target.mutate_payload();
    let failed = harness.grant(0, u64::MAX / 2, 1_000).expect("the grant");
    assert_eq!(
        failed.facts.failed, 1,
        "a changed payload was not a protocol failure"
    );
    assert_eq!(
        failed.facts.uncertain, 0,
        "a changed payload became an unknown completion"
    );
    let diagnostic = harness.diagnostic();
    // The ABI never changes the Rust payload, so it maps the C target's changed
    // copy to `Incomplete`: the adapter reports that non-completion, not the
    // `Ok` the target claimed.
    assert!(
        diagnostic.contains("left TLM_INCOMPLETE_RESPONSE"),
        "the changed payload is not the reported classification: {diagnostic}"
    );
    let accesses = harness.target.transactions() as u64;
    assert!(accesses > 0, "the failed grant ran no transaction");
    assert_eq!(
        failed.host_elapsed_fs,
        accesses * ELAPSED_HALF_TICK,
        "the changed payload's host elapsed time was not totalled"
    );
    assert_eq!(
        failed.consumed_fs - failed.host_elapsed_fs,
        failed.facts.turns * TICK_FS + accesses * (TICK_FS / 2),
        "the changed payload's host elapsed time was not charged"
    );
}

#[test]
fn consumed_minus_host_elapsed_is_the_turn_ticks_plus_half_a_tick_per_access() {
    // nb D5: `e` is 1.5 ticks and its charge rounds up to 2, so beside the turn
    // ticks a grant leaves exactly half a tick per access outside W.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &exit_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    let grant = harness.grant(0, u64::MAX / 2, 8).expect("the grant");
    let turns = grant.facts.turns;
    let accesses = harness.target.transactions() as u64;
    assert!(
        turns > 0 && accesses > 0,
        "the grant ran {turns} turns and {accesses} transactions"
    );
    assert_eq!(
        grant.host_elapsed_fs,
        accesses * ELAPSED_HALF_TICK,
        "the grant did not total the host elapsed time of its transactions"
    );
    assert_eq!(
        grant.consumed_fs - grant.host_elapsed_fs,
        turns * TICK_FS + accesses * (TICK_FS / 2),
        "consumed_fs - host_elapsed_fs is not the turn ticks plus half a tick per access"
    );
    assert_eq!(
        grant.committed_start_fs + grant.consumed_fs,
        grant.committed_end_fs,
        "the grant's consumed amount does not end at its committed end"
    );
}

#[test]
fn a_failed_grant_still_reports_its_host_elapsed_time() {
    // nb D5: `W` is set for every grant, including one a classified failure
    // ends, and it never exceeds the amount that grant consumed.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &direct_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    harness
        .target
        .unknown_on_data_read
        .store(true, Ordering::SeqCst);
    let failed = harness
        .grant(0, u64::MAX / 2, 1_000)
        .expect("the failed grant");
    assert_eq!(
        failed.facts.failed, 1,
        "the unknown completion did not fail the run"
    );
    assert_eq!(failed.facts.uncertain, 1);
    let accesses = harness.target.transactions() as u64;
    assert!(accesses > 0, "the failed grant ran no transaction");
    assert_eq!(
        failed.host_elapsed_fs,
        accesses * ELAPSED_HALF_TICK,
        "the failed grant did not report the host elapsed time it counted"
    );
    assert!(
        failed.host_elapsed_fs <= failed.consumed_fs,
        "the failed grant reported more host time than it consumed"
    );
}

#[test]
fn a_suspending_table_with_a_quantum_is_refused_at_create() {
    // nb D6: host time passing inside a grant would stop the kernel's clock
    // from being the quantum anchor for the whole grant.
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    target.set_suspends(true);
    let table = target.quantum_table(0, 4);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table and its user pointer stay alive for this test.
    let status = unsafe {
        ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, TICK_FS, &mut handle)
    };
    assert_eq!(
        status,
        Status::Config as i32,
        "a suspending table with quantum_ticks 4 was admitted"
    );
    assert!(handle.is_null(), "a refused create published a handle");
}

#[test]
fn the_suspension_declaration_persists_across_reset() {
    // nb D6: the table is copied at create, so the declaration survives a
    // reset's fresh generation.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &direct_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        harness.diagnostic()
    );
    assert_eq!(harness.reset(), Status::Ok as i32, "reset");
    harness.target.clear_calls();
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        harness.diagnostic()
    );
    let calls = harness.target.calls();
    assert!(!calls.is_empty(), "the reset generation ran no transaction");
    for call in &calls {
        assert_eq!(
            call.may_suspend, 1,
            "the reset generation lost the suspension declaration: {call:?}"
        );
    }
}

#[test]
fn a_suspended_callback_leaves_run_grant_and_destroy_refused() {
    // nb D6: a `may_suspend` callback may suspend its thread while the
    // thread-local callback flag stays set, so the entries that would disturb
    // the frame are refused as re-entry and the exempt one stays accepted.
    let harness = Harness::suspending(
        IsaProfile::Rv64imafdc as i32,
        &exit_guest(2),
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
        ELAPSED_HALF_TICK,
    );
    harness.target.probe_entries.store(true, Ordering::SeqCst);
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        harness.diagnostic()
    );
    assert_eq!(
        harness.target.run_grant_status(),
        Status::Reentrant as i32,
        "a callback started a grant on the handle it was serving"
    );
    assert_eq!(
        harness.target.destroy_status(),
        Status::Reentrant as i32,
        "a callback destroyed the handle it was serving"
    );
    assert_eq!(
        harness.target.invalidate_status.load(Ordering::SeqCst),
        Status::Ok as i32,
        "the invalidation entry is not accepted inside a callback"
    );
}

#[test]
fn a_handle_creates_loads_runs_and_reads_a_boundary() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        harness.diagnostic()
    );
    assert_eq!(result.exit_code, 2, "the guest exit code must survive");
    assert!(result.turns > 0, "the run retired no Hart turn");
    assert!(result.pc > elf::BASE, "the boundary PC must be reported");
    let facts = harness.facts();
    assert_eq!(facts.failed, 0);
    assert_eq!(facts.uncertain, 0);
    assert!(
        harness.target.transactions.load(Ordering::SeqCst) > 0,
        "the guest did not reach the selected TLM RAM route"
    );
    assert!(
        harness.target.debug_calls.load(Ordering::SeqCst) > 0,
        "image installation and host views must use transport_dbg"
    );
}

#[test]
fn the_selected_profile_reaches_the_hart() {
    // The first parcel is compressed, so only an IALIGN=16 profile executes it;
    // a profile without compression traps instead of reaching the exit store.
    let code = exit_guest(3);
    let compressed = Harness::new(IsaProfile::Rv64imafdc as i32, &code, 0);
    let result = compressed.run_to_result(2_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "rv64imafdc did not execute the compressed guest: {}",
        compressed.diagnostic()
    );
    assert_eq!(result.exit_code, 3);

    let word_only = Harness::new(IsaProfile::Rv64ima as i32, &code, 0);
    let result = word_only.run_to_result(2_000, 8);
    assert_ne!(
        result.state,
        RunState::Exited.code(),
        "rv64ima executed an IALIGN=16 guest, so the profile did not reach the Hart"
    );
}

#[test]
fn the_commit_log_is_written_to_the_named_path() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let log = tempfile::NamedTempFile::new().unwrap();
    let path = c_text(log.path());
    // SAFETY: the handle is live and the path is NUL-terminated.
    let status = unsafe { ruscv_machine_set_commit_log(harness.handle, path.as_ptr()) };
    assert_eq!(status, Status::Ok as i32, "set_commit_log");
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(result.state, RunState::Exited.code());
    let text = std::fs::read_to_string(log.path()).unwrap();
    assert!(
        text.contains("core   0: 3 0x0000000080000000"),
        "the commit log does not start at the entry PC: {text}"
    );
    assert!(
        text.contains("x4  0x0000000080001000"),
        "the commit log lost the tohost register effect: {text}"
    );
    assert!(
        text.lines().count() >= 4,
        "the commit log is shorter than the guest: {text}"
    );
}

#[test]
fn a_grant_reports_consumed_time_and_rejects_an_earlier_start() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness.grant(0, 100 * TICK_FS, 1).unwrap();
    assert_eq!(grant.committed_start_fs, 0);
    assert_eq!(grant.consumed_fs, grant.committed_end_fs);
    assert!(
        grant.consumed_fs > 0,
        "a completed turn consumed no modeled time"
    );
    assert_eq!(grant.boundary_reached, 1, "the one-turn budget did not end");

    // ADR-0004 §4.4: an authoritative start earlier than the Machine's last
    // committed time is an adapter failure, and the cursor cannot rewind.
    assert_eq!(
        harness.grant(0, u64::MAX / 2, 10).err(),
        Some(Status::TimeAuthority as i32)
    );
    assert!(
        harness.diagnostic().contains("earlier than"),
        "the earlier-start diagnostic is missing: {}",
        harness.diagnostic()
    );
    let committed = harness.facts();
    assert_eq!(
        committed.transport_delay_fs, 0,
        "a zero-delay target must report zero transport delay"
    );

    // A reversed horizon is malformed for the same reason.
    assert_eq!(
        harness
            .grant(grant.committed_end_fs, grant.committed_end_fs - 1, 10)
            .err(),
        Some(Status::TimeAuthority as i32)
    );
    // The cursor did not move.
    let _ = harness.grant(
        grant.committed_end_fs,
        grant.committed_end_fs + 100 * TICK_FS,
        10,
    );
}

#[test]
fn a_call_from_another_thread_is_rejected() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let address = harness.handle as usize;
    let status = std::thread::spawn(move || {
        let mut facts = CBoundary::default();
        // SAFETY: the handle outlives this join, and the callee must reject the
        // call before touching any state.
        unsafe { ruscv_machine_boundary(address as *mut RuscvMachine, &mut facts) }
    })
    .join()
    .unwrap();
    assert_eq!(status, Status::NotCreatorThread as i32);
}

#[test]
fn a_nested_call_is_rejected_as_reentry() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    // Enter through the target callback: the callback runs inside a ruscv-sim
    // call, so its own ABI call must be refused and the run must continue.
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        harness.target.reentry_status(),
        Status::Reentrant as i32,
        "a callback re-entered the C ABI instead of being rejected"
    );
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "a rejected re-entry disturbed the outer run: {}",
        harness.diagnostic()
    );
}

#[test]
fn a_call_made_while_a_call_is_active_is_rejected_as_reentry() {
    // The same guard without a live callback: while an ABI call owns the
    // handle, a nested entry is refused rather than served.
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = target.table(0);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table stays alive for the whole test.
    let status = unsafe {
        ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, TICK_FS, &mut handle)
    };
    assert_eq!(status, Status::Ok as i32);
    // SAFETY: the handle is live.
    let machine = unsafe { &*handle };
    let outer = machine.gate.enter().expect("the handle is idle");
    let mut facts = CBoundary::default();
    // SAFETY: the handle is live and the call is made on the creating thread.
    let nested = unsafe { ruscv_machine_boundary(handle, &mut facts) };
    assert_eq!(
        nested,
        Status::Reentrant as i32,
        "a nested call was served instead of rejected"
    );
    drop(outer);
    // SAFETY: the handle is live.
    let nested = unsafe { ruscv_machine_boundary(handle, &mut facts) };
    assert_eq!(
        nested,
        Status::NoImage as i32,
        "the handle stayed unusable after the outer call"
    );
    // SAFETY: the handle is live and unused from now on.
    assert_eq!(unsafe { ruscv_machine_destroy(handle) }, Status::Ok as i32);
}

#[test]
fn a_caught_panic_poisons_the_handle() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_self_test_panic(harness.handle) };
    assert_eq!(status, Status::Panic as i32, "the panic was not caught");
    let mut facts = CBoundary::default();
    // SAFETY: the handle is live, but poisoned.
    let status = unsafe { ruscv_machine_boundary(harness.handle, &mut facts) };
    assert_eq!(status, Status::Poisoned as i32);
    let mut grant = CGrant::default();
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_run_grant(harness.handle, 0, 1, 1, &mut grant) };
    assert_eq!(status, Status::Poisoned as i32);
}

/// Encode `csrrs rd, csr, rs1` (setting bits from `rs1`).
fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

/// A guest that arms the timer, waits, then reports an exit.
///
/// Eight turns complete before the wait, so the Machine's cursor is 8 when the
/// `WFI` retires; `mtimecmp` is 0x1000.
fn waiting_guest(compare: u32) -> Vec<u32> {
    let load_compare = if compare < 2048 {
        elf::addi(6, 0, compare as i32)
    } else {
        elf::lui(6, compare >> 12)
    };
    vec![
        elf::addi(5, 0, 0x80),
        csrrs(0, crate::csr::machine::MIE, 5),
        elf::lui(9, 0x02004),
        load_compare,
        elf::sd(6, 9, 0),
        elf::addi(5, 0, 8),
        csrrs(0, crate::csr::machine::MSTATUS, 5),
        0x1050_0073,
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::standard_exit(2),
        elf::sd(5, 4, 0),
    ]
}

#[test]
fn a_wait_wake_at_the_deadline_does_not_cross_the_grant_horizon() {
    // The timer fires exactly at the inclusive deadline.  A turn may not begin
    // at the deadline (ADR-0004 §7.2), so the grant must stop there instead of
    // taking the wake turn and reporting more time than its horizon.
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(
            &waiting_guest(0x1000),
            0,
            true,
            false,
            0x4000,
        ))
        .unwrap(),
    );
    let mut session = Session::new(config, image).unwrap();
    session
        .run_grant(0, 0x1000 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    let facts = session.facts();
    assert!(
        facts.consumed_fs <= 0x1000 * TICK_FS,
        "the grant consumed {} fs beyond its {} fs horizon",
        facts.consumed_fs,
        0x1000 * TICK_FS
    );
    assert!(
        facts.deadline_reached,
        "a wait that stopped at the deadline did not report it"
    );
}

#[test]
fn a_start_before_the_callers_committed_time_is_rejected() {
    // ADR-0004 §4.4: the kernel commits global time only from the amount a
    // grant returned, so its own clock cannot rewind either.  The caller starts
    // far ahead of the Machine's cursor here, which makes the Machine-cursor
    // check alone vacuous.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness.grant(100 * TICK_FS, 110 * TICK_FS, 1).unwrap();
    assert_eq!(grant.consumed_fs, TICK_FS);
    assert_eq!(
        harness.grant(50 * TICK_FS, 200 * TICK_FS, 1).err(),
        Some(Status::TimeAuthority as i32),
        "the caller's clock rewound"
    );
    assert!(
        harness.diagnostic().contains("caller's committed time"),
        "wrong diagnostic: {}",
        harness.diagnostic()
    );
    // A later start (the caller's own idle step) stays legal.
    let grant = harness
        .grant(102 * TICK_FS, 200 * TICK_FS, 1)
        .expect("a forward start is legal");
    assert_eq!(grant.consumed_fs, TICK_FS);
}

#[test]
fn the_declared_tick_period_drives_delay_conversion() {
    // A 2 ns tick: one compressed fetch annotated at 2 ns is one delay tick, and
    // a completed turn is one more, so one retired instruction consumes 4 ns.
    // The period must reach the RAM adapter, not only the caller-facing
    // arithmetic, and the completion bound must use it too.
    const PERIOD_FS: u64 = 2_000_000;
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    config.tick_period = TickPeriod::new(Femtoseconds::new(PERIOD_FS));
    config.transport = crate::blocking::Transport::tlm(Arc::new(DelayedTarget::new(PERIOD_FS)));
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&exit_guest(2), 0, true, false, 0x4000)).unwrap(),
    );
    let mut session = Session::new(config, image).unwrap();
    session
        .run_grant(0, 100 * PERIOD_FS, 1)
        .expect("one turn fits");
    let facts = session.facts();
    assert_eq!(facts.consumed_fs, 2 * PERIOD_FS);
    assert_eq!(facts.transport_delay_fs, PERIOD_FS);
    assert_eq!(facts.turns, 1);

    // The bound converts the declared per-transaction maximum under the same
    // period: two turns of one tick each, plus two transactions at one tick.
    session
        .run_grant(2 * PERIOD_FS, 10 * PERIOD_FS, super::UNBOUNDED_TURNS)
        .expect("a stopped grant is not an API error");
    let facts = session.facts();
    assert!(facts.deadline_blocked);
    // Two turns, each reserving its own tick plus 18 transactions at one tick.
    assert_eq!(facts.candidate_bound_fs, 38 * PERIOD_FS);
    assert_eq!(facts.residual_slack_fs, 8 * PERIOD_FS);
}

#[test]
fn a_polling_debug_transport_failure_is_a_host_failure() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(0), 0);
    // Fail only after the image is installed, so the load still succeeds.
    harness.target.fail_dbg_reads.store(true, Ordering::SeqCst);
    let grant = harness.grant(0, u64::MAX / 2, 1000).unwrap();
    let result = harness.result();
    assert_eq!(
        grant.facts.failed, 1,
        "a failed polling read was not a host failure"
    );
    assert_eq!(result.state, RunState::Failed.code());
    assert!(
        harness.diagnostic().contains("tohost read failed"),
        "wrong diagnostic: {}",
        harness.diagnostic()
    );
}

#[test]
fn a_failed_tohost_clear_never_reports_a_guest_exit() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(0), 0);
    harness.target.fail_dbg_writes.store(true, Ordering::SeqCst);
    let grant = harness.grant(0, u64::MAX / 2, 1000).unwrap();
    let result = harness.result();
    assert_eq!(
        grant.facts.failed, 1,
        "a failed clear was accepted as a guest exit"
    );
    assert_ne!(result.state, RunState::Exited.code());
    assert_eq!(result.state, RunState::Failed.code());
    assert!(
        harness.diagnostic().contains("tohost clear failed"),
        "wrong diagnostic: {}",
        harness.diagnostic()
    );
}

#[test]
fn a_callback_call_into_another_handle_is_rejected_as_reentry() {
    // D8 rejects any call back into the ABI from a callback, not only one into
    // the handle the callback belongs to.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let other = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    harness
        .target
        .second_handle
        .store(other.handle, Ordering::SeqCst);
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        harness.target.second_status(),
        Status::Reentrant as i32,
        "a callback re-entered the C ABI through another handle"
    );
    assert_eq!(result.state, RunState::Exited.code());
}

#[test]
fn an_oversized_atomic_snapshot_is_not_a_completion() {
    use crate::blocking::{
        BlockingAtomicExtension, BlockingCommand, BlockingInitiator, BlockingTarget,
        BlockingTransaction,
    };
    use crate::physical::{
        AmoWidth, AtomicOrdering, PhysicalAccessKind, MAX_COMMITTED_WRITE_SNAPSHOT_BYTES,
    };

    struct SnapshotTarget;
    unsafe extern "C" fn snapshot_transport(_: *mut c_void, payload: *mut CPayload) {
        // SAFETY: the ABI guarantees a live payload.
        let payload = unsafe { &mut *payload };
        payload.status = super::target::status::OK;
        payload.atomic.outcome = atomic_outcome::LOAD_RESERVED;
        payload.atomic.snapshot_length = (MAX_COMMITTED_WRITE_SNAPSHOT_BYTES + 1) as u32;
    }
    unsafe extern "C" fn snapshot_debug(_: *mut c_void, debug: *mut CDebug) -> usize {
        // SAFETY: the ABI guarantees a live request.
        unsafe { &*debug }.length
    }
    unsafe extern "C" fn snapshot_reset(_: *mut c_void) -> i32 {
        1
    }
    let table = CTarget {
        max_delay_fs: 0,
        user: std::ptr::null_mut(),
        b_transport: Some(snapshot_transport),
        transport_dbg: Some(snapshot_debug),
        transport_reset: Some(snapshot_reset),
        get_direct_mem_ptr: None,
        quantum_ticks: 0,
        suspends_host_time: 0,
    };
    let _ = SnapshotTarget;
    let adapter = super::target::CBlockingTarget::new(table).unwrap();
    let mut bytes = [0u8; 4];
    let mut transaction = BlockingTransaction::new(
        0x8000_0100,
        BlockingCommand::Read,
        &mut bytes,
        crate::blocking::BlockingExtension::atomic(
            PhysicalAccessKind::DataRead,
            BlockingInitiator::Hart,
            BlockingAtomicExtension::load_reserved(AmoWidth::Word, AtomicOrdering::default()),
        ),
    );
    adapter.b_transport(&mut transaction);
    assert_eq!(
        transaction.atomic_extension().unwrap().outcome(),
        None,
        "an oversized snapshot was truncated into a completion"
    );
}

#[test]
fn a_fractional_horizon_reports_its_residual_slack() {
    // A 2.5-tick horizon leaves a half tick that no whole tick can use: the
    // grant must report DeadlineBlocked with the real femtosecond residual, not
    // claim the horizon was reached.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness
        .grant(0, 5 * TICK_FS / 2, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.facts.deadline_blocked, 1);
    assert_eq!(grant.facts.deadline_reached, 0);
    assert_eq!(grant.consumed_fs, TICK_FS);
    assert_eq!(grant.facts.residual_slack_fs, TICK_FS + TICK_FS / 2);
    assert_eq!(grant.facts.candidate_bound_fs, 2 * TICK_FS);
}

#[test]
fn a_terminal_exit_keeps_coincident_budget_facts() {
    // The exit lands on the last granted slot, so the boundary facts may not
    // drop `budget_exhausted` (ADR-0004 §7.1, no return path discards a
    // coincident fact).
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness
        .grant(0, u64::MAX / 2, 7)
        .expect("the guest exits on its seventh turn");
    let result = harness.result();
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(result.exit_code, 2);
    assert_eq!(grant.facts.turns, 7);
    assert_eq!(grant.facts.budget_exhausted, 1);
}

/// A guest that arms the timer, waits, then keeps executing after the wake.
///
/// The wake is a normal instruction turn (the interrupt is not taken), so it
/// fetches with the target's annotated delay.
fn wake_guest(compare: u32) -> Vec<u32> {
    let load_compare = if compare < 2048 {
        elf::addi(6, 0, compare as i32)
    } else {
        elf::lui(6, compare >> 12)
    };
    vec![
        elf::addi(5, 0, 0x80),
        csrrs(0, crate::csr::machine::MIE, 5),
        elf::lui(9, 0x02004),
        load_compare,
        elf::sd(6, 9, 0),
        0x1050_0073,
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::standard_exit(2),
        elf::sd(5, 4, 0),
    ]
}

#[test]
fn an_idle_jump_leaves_no_slack_for_a_wake_turn_that_cannot_fit() {
    // The timer fires one tick before the horizon.  The idle jump consumes the
    // slack the step was granted, so the wake turn's own conservative bound no
    // longer fits and must not start (ADR-0004 §7.2).
    const PERIOD_FS: u64 = 2_000_000;
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    config.tick_period = TickPeriod::new(Femtoseconds::new(PERIOD_FS));
    config.transport = crate::blocking::Transport::tlm(Arc::new(DelayedTarget::new(PERIOD_FS)));
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&wake_guest(59), 0, true, false, 0x4000)).unwrap(),
    );
    let mut session = Session::new(config, image).unwrap();
    session
        .run_grant(0, 60 * PERIOD_FS, super::UNBOUNDED_TURNS)
        .expect("a blocked grant is not an API error");
    let facts = session.facts();
    assert!(
        facts.consumed_fs <= 60 * PERIOD_FS,
        "the wake turn consumed {} fs against a {} fs horizon",
        facts.consumed_fs,
        60 * PERIOD_FS
    );
    assert!(
        facts.deadline_blocked,
        "the refused wake turn was not reported"
    );
    assert!(!facts.deadline_reached);
    assert_eq!(facts.candidate_bound_fs, 19 * PERIOD_FS);
}

#[test]
fn horizon_facts_stay_in_the_callers_time_base() {
    // The caller's clock runs far ahead of the Machine's cursor, so the residual
    // slack must be measured from the caller's own committed time.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness.grant(100 * TICK_FS, 110 * TICK_FS, 1).unwrap();
    assert_eq!(grant.consumed_fs, TICK_FS);
    assert_eq!(grant.facts.budget_exhausted, 1);
    assert_eq!(
        grant.facts.residual_slack_fs,
        9 * TICK_FS,
        "the residual mixed the caller's horizon with the Machine cursor"
    );

    // A fractional horizon is not reached by the floored internal deadline.
    let waiting = Harness::new(IsaProfile::Rv64imafdc as i32, &waiting_guest(0x1000), 0);
    let grant = waiting
        .grant(0, 4096 * TICK_FS + TICK_FS / 2, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.consumed_fs, 4096 * TICK_FS);
    assert_eq!(
        grant.facts.deadline_blocked, 1,
        "the internal deadline stop was reported as the caller's horizon"
    );
    assert_eq!(grant.facts.deadline_reached, 0);
    assert_eq!(grant.facts.residual_slack_fs, TICK_FS / 2);
}

#[test]
fn an_unbounded_completion_bound_is_a_configuration_failure() {
    // A period of 1 fs with a maximum of UINT64_MAX/18 passes create-time
    // validation, but doubling the step bound cannot be represented.  It must be
    // an explicit failure, never a wrap or a panic (ADR-0004 §5.2).
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), u64::MAX / 18);
    // Harness uses a 1 ns period, so rebuild with a 1 fs period by hand.
    drop(harness);
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = target.table(u64::MAX / 18);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table outlives the handle.
    let status =
        unsafe { ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, 1, &mut handle) };
    assert_eq!(status, Status::Ok as i32, "create with a 1 fs period");
    let file = elf_file(&exit_guest(2));
    let path = c_text(file.path());
    // SAFETY: the handle is live and the path is NUL-terminated.
    assert_eq!(
        unsafe { ruscv_machine_load_elf(handle, path.as_ptr()) },
        Status::Ok as i32
    );
    let mut grant = CGrant::default();
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_run_grant(handle, 0, u64::MAX / 2, 1, &mut grant) };
    assert_eq!(
        status,
        Status::Config as i32,
        "an unrepresentable step bound was not a configuration failure"
    );
    let mut facts = CBoundary::default();
    // SAFETY: the handle is live and must not be poisoned.
    assert_eq!(
        unsafe { ruscv_machine_boundary(handle, &mut facts) },
        Status::Ok as i32,
        "the failure poisoned the handle"
    );
    // SAFETY: the handle is live.
    assert_eq!(unsafe { ruscv_machine_destroy(handle) }, Status::Ok as i32);
}

#[test]
fn a_refused_reset_keeps_the_failed_machine_in_place() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    harness.target.unknown.store(true, Ordering::SeqCst);
    let grant = harness.grant(0, u64::MAX / 2, 100).unwrap();
    assert_eq!(grant.facts.uncertain, 1);

    harness.target.reset_ack.store(false, Ordering::SeqCst);
    // SAFETY: the handle is live.
    assert_eq!(
        unsafe { ruscv_machine_reset(harness.handle) },
        Status::Machine as i32,
        "a reset without the acknowledgement must fail"
    );
    // The failed Machine stays installed, so the session still reports facts and
    // the same unknown-completion refusal instead of `NO_IMAGE`.
    let _ = harness.facts();
    assert_eq!(
        harness.grant(0, u64::MAX / 2, 100).err(),
        Some(Status::UnknownCompletion as i32)
    );

    harness.target.reset_ack.store(true, Ordering::SeqCst);
    // SAFETY: the handle is live.
    assert_eq!(
        unsafe { ruscv_machine_reset(harness.handle) },
        Status::Ok as i32,
        "the retried reset was refused"
    );
    harness.target.unknown.store(false, Ordering::SeqCst);
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(result.state, RunState::Exited.code());
}

#[test]
fn an_overflowing_reserve_is_refused_at_the_maximum_horizon() {
    // A 1 fs period with 1 fs transaction delays: after the idle jump to
    // `mtimecmp = UINT64_MAX - 1`, the wake turn's reserve cannot be added to
    // the cursor.  Overflow is refusal, never a saturated "fits", so the grant
    // must report `deadline_blocked` instead of starting the turn and failing on
    // a clock overflow (ADR-0004 §7.2).
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    config.tick_period = TickPeriod::new(Femtoseconds::new(1));
    config.transport = crate::blocking::Transport::tlm(Arc::new(DelayedTarget::new(1)));
    let mut code = wake_guest(0);
    // mtimecmp = UINT64_MAX - 1: x6 = -1 then x6 -= 1.
    let compare = code
        .iter()
        .position(|word| *word == elf::addi(6, 0, 0))
        .expect("the compare setup");
    code[compare] = elf::addi(6, 0, -1);
    code.insert(compare + 1, elf::addi(6, 6, -1));
    let image =
        Arc::new(LoadImage::parse(&elf::elf_with_code(&code, 0, true, false, 0x4000)).unwrap());
    let mut session = Session::new(config, image).unwrap();
    session
        .run_grant(0, u64::MAX, super::UNBOUNDED_TURNS)
        .expect("a blocked grant is not an API error");
    let facts = session.facts();
    assert!(!facts.failed, "the grant failed instead of refusing");
    assert!(
        facts.deadline_blocked,
        "an overflowing reserve was treated as fitting the horizon"
    );
    assert!(
        facts.consumed_fs < u64::MAX,
        "the wake turn ran past the maximum horizon"
    );
}

#[test]
fn a_terminal_exit_reports_the_remaining_slack() {
    // The guest exits on its seventh and last budgeted turn, so the boundary
    // reports both the exhausted budget and the caller's remaining slack.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness.grant(100 * TICK_FS, 200 * TICK_FS, 7).unwrap();
    let result = harness.result();
    assert_eq!(result.state, RunState::Exited.code());
    assert_eq!(grant.facts.turns, 7);
    assert_eq!(grant.facts.budget_exhausted, 1);
    assert_eq!(
        grant.facts.residual_slack_fs,
        93 * TICK_FS,
        "a terminal exit dropped the caller's residual slack"
    );
}

#[test]
fn a_host_view_failure_keeps_the_completed_final_slot() {
    // The turn completed before polling failed, so the final budget slot and its
    // residual slack are coincident facts, not casualties of the failure
    // (ADR-0004 §4.3; §7.1's exception is a failed attempt, not a completed one).
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    harness.target.fail_dbg_reads.store(true, Ordering::SeqCst);
    let grant = harness.grant(100 * TICK_FS, 200 * TICK_FS, 1).unwrap();
    let result = harness.result();
    assert_eq!(result.state, RunState::Failed.code());
    assert_eq!(grant.facts.turns, 1, "the completed turn was dropped");
    assert_eq!(grant.consumed_fs, TICK_FS);
    assert_eq!(
        grant.facts.budget_exhausted, 1,
        "a completed final slot lost its budget fact"
    );
    assert_eq!(grant.facts.residual_slack_fs, 99 * TICK_FS);
}

#[test]
fn a_commit_log_failure_keeps_the_completed_turn() {
    // /dev/full accepts the open and fails every write, so delivery fails after
    // the Hart already completed its transition.  The device node is a
    // prerequisite of this case on the supported Linux x86-64 platform.
    assert!(
        std::path::Path::new("/dev/full").exists(),
        "the /dev/full prerequisite is missing"
    );
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let path = c_text(std::path::Path::new("/dev/full"));
    // SAFETY: the handle is live and the path is NUL-terminated.
    assert_eq!(
        unsafe { ruscv_machine_set_commit_log(harness.handle, path.as_ptr()) },
        Status::Ok as i32,
        "routing the commit log to /dev/full failed"
    );
    let grant = harness.grant(0, 100 * TICK_FS, 1).unwrap();
    let result = harness.result();
    assert_eq!(result.state, RunState::Failed.code());
    assert_eq!(
        grant.facts.turns, 1,
        "a commit-log failure unretired the completed turn"
    );
    assert_eq!(grant.consumed_fs, TICK_FS);
    assert_eq!(
        grant.facts.budget_exhausted, 1,
        "the completed final slot lost its budget fact"
    );
    assert_eq!(grant.facts.residual_slack_fs, 99 * TICK_FS);
    assert!(
        harness.diagnostic().contains("commit log"),
        "wrong diagnostic: {}",
        harness.diagnostic()
    );
}

#[test]
fn a_grant_boundary_is_not_a_terminal_result() {
    // The C-ABI stand-in for `sc_stop`: a host that ends a grant at its budget
    // and stops granting has not produced a terminal result, and a zero exit
    // code alone must not read as one.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    let grant = harness.grant(0, 1000 * TICK_FS, 1).unwrap();
    let result = harness.result();
    assert_eq!(
        result.state,
        RunState::Running.code(),
        "a host stop before a terminal result was reported as a passing result"
    );
    assert_eq!(result.exit_code, 0, "a boundary invented a guest exit code");
    assert_eq!(grant.facts.failed, 0);
    assert_eq!(grant.facts.uncertain, 0);
    assert_eq!(grant.boundary_reached, 1);

    // The run is still resumable from the committed time it returned.
    let grant = harness
        .grant(grant.committed_end_fs, u64::MAX / 2, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.committed_start_fs, TICK_FS);
    let result = harness.result();
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "the resumed run did not reach the guest exit: {}",
        harness.diagnostic()
    );
    assert_eq!(result.exit_code, 2);
}

#[test]
fn an_unknown_completion_blocks_the_transport_until_reset() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    harness.target.unknown.store(true, Ordering::SeqCst);
    let grant = harness.grant(0, u64::MAX / 2, 100).unwrap();
    assert_eq!(grant.facts.failed, 1, "an unknown completion is not a pass");
    assert_eq!(
        grant.facts.uncertain, 1,
        "the grant must report uncertainty"
    );
    let result = harness.result();
    assert_eq!(result.state, RunState::Failed.code());
    assert_eq!(
        result.exit_code, 0,
        "a failed run must not invent a guest code"
    );

    // No further access may be issued before the target acknowledges a reset.
    assert_eq!(
        harness.grant(0, u64::MAX / 2, 100).err(),
        Some(Status::UnknownCompletion as i32)
    );
    assert_eq!(
        harness.target.transactions.load(Ordering::SeqCst),
        1,
        "a transaction reached the poisoned transport"
    );

    harness.target.unknown.store(false, Ordering::SeqCst);
    let resets = harness.target.reset_acks.load(Ordering::SeqCst);
    // SAFETY: the handle is live.
    assert_eq!(
        unsafe { ruscv_machine_reset(harness.handle) },
        Status::Ok as i32
    );
    assert!(
        harness.target.reset_acks.load(Ordering::SeqCst) > resets,
        "reset did not obtain the target's transport-reset acknowledgement"
    );
    let result = harness.run_to_result(1_000, 8);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "the reset domain did not run to the guest exit: {}",
        harness.diagnostic()
    );
    assert_eq!(result.exit_code, 2);
}

#[test]
fn an_unacknowledged_transport_reset_keeps_the_handle_unusable() {
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = target.table(0);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table stays alive for the whole test.
    let status = unsafe {
        ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, TICK_FS, &mut handle)
    };
    assert_eq!(status, Status::Ok as i32);
    target.reset_ack.store(false, Ordering::SeqCst);
    let file = elf_file(&exit_guest(2));
    let path = c_text(file.path());
    // SAFETY: the handle is live and the path is NUL-terminated.
    let status = unsafe { ruscv_machine_load_elf(handle, path.as_ptr()) };
    assert_ne!(
        status,
        Status::Ok as i32,
        "installation without a transport-reset acknowledgement reported success"
    );
    let mut grant = CGrant::default();
    // SAFETY: the handle is live.
    let status = unsafe { ruscv_machine_run_grant(handle, 0, u64::MAX / 2, 10, &mut grant) };
    assert_eq!(status, Status::NoImage as i32);
    // SAFETY: the handle is live.
    assert_eq!(unsafe { ruscv_machine_destroy(handle) }, Status::Ok as i32);
}

#[test]
fn a_host_or_protocol_transport_status_is_not_a_passing_result() {
    for status in [
        super::target::status::GENERIC_ERROR,
        super::target::status::INCOMPLETE,
        super::target::status::COMMAND_ERROR | 0x40,
    ] {
        let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(0), 0);
        harness.target.status.store(status, Ordering::SeqCst);
        let grant = harness.grant(0, u64::MAX / 2, 100).unwrap();
        let result = harness.result();
        assert_eq!(
            grant.facts.failed, 1,
            "TLM status {status} was reported as a passing result"
        );
        assert_ne!(
            result.state,
            RunState::Exited.code(),
            "a transport status became a terminal guest exit"
        );
        assert_eq!(result.state, RunState::Failed.code());
        assert_eq!(result.exit_code, 0);
        assert!(
            !harness.diagnostic().is_empty(),
            "a transport failure left no diagnostic"
        );
    }
}

#[test]
fn a_target_status_is_a_guest_fault_never_a_pass() {
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(0), 0);
    harness
        .target
        .status
        .store(super::target::status::ADDRESS_ERROR, Ordering::SeqCst);
    let grant = harness.grant(0, u64::MAX / 2, 100).unwrap();
    let result = harness.result();
    assert_ne!(
        result.state,
        RunState::Exited.code(),
        "a target fault became a terminal guest exit"
    );
    assert!(
        grant.facts.turns > 0,
        "the Hart did not take the access fault"
    );
    assert_eq!(
        grant.facts.failed, 0,
        "a guest-visible target fault was reported as a host failure"
    );
}

#[test]
fn a_deadline_blocks_a_turn_it_cannot_finish() {
    // One declared tick per transaction reserves one turn tick plus 18
    // transaction ticks, and a step may complete a waiting turn and its wake, so
    // the lookahead reserves two of those: 38 ticks.  A 10-tick grant therefore
    // cannot start a turn.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), TICK_FS);
    let bound = TurnCompletionBound::new(Femtoseconds::new(TICK_FS), TickPeriod::DEFAULT).unwrap();
    let step_bound = 2 * (bound.ticks() + 1);
    let grant = harness
        .grant(0, 10 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.facts.deadline_blocked, 1);
    assert_eq!(grant.boundary_reached, 1);
    assert_eq!(grant.consumed_fs, 0, "a blocked grant consumed time");
    assert_eq!(grant.facts.candidate_bound_fs, step_bound * TICK_FS);
    assert_eq!(grant.facts.residual_slack_fs, 10 * TICK_FS);
    assert_eq!(
        harness.target.transactions.load(Ordering::SeqCst),
        0,
        "a blocked grant issued a transaction"
    );

    // A 40-tick horizon fits three one-tick turns; the fourth would cross it, so
    // the grant reports the residual slack instead of starting it.
    let grant = harness
        .grant(0, 40 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.consumed_fs, 3 * TICK_FS);
    assert_eq!(grant.facts.turns, 3);
    assert_eq!(grant.facts.deadline_blocked, 1);
    assert_eq!(grant.facts.deadline_reached, 0);
    assert_eq!(grant.facts.residual_slack_fs, 37 * TICK_FS);
    assert_eq!(
        harness.target.transactions.load(Ordering::SeqCst),
        4,
        "the blocked turn issued a transaction (two 2-byte parcels, then one \
         two-parcel 32-bit parcel read)"
    );
}

#[test]
fn a_native_grant_reserves_its_two_turn_step_bound() {
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&exit_guest(2), 0, true, false, 0x4000)).unwrap(),
    );
    let mut session = Session::new(config, image).unwrap();
    // A native step has no transport reserve, so its bound is the two turns a
    // waiting wake can complete.  A two-tick horizon fits one one-tick turn and
    // then refuses the next, which would need the whole step again.
    session
        .run_grant(0, 2 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    let facts = session.facts();
    assert_eq!(facts.consumed_fs, TICK_FS);
    assert_eq!(facts.turns, 1);
    assert!(facts.deadline_blocked);
    assert_eq!(facts.candidate_bound_fs, 2 * TICK_FS);
    assert_eq!(facts.residual_slack_fs, TICK_FS);
    assert_eq!(facts.transport_delay_fs, 0);
}

#[test]
fn the_atomic_extension_maps_both_directions() {
    use crate::blocking::{
        BlockingAtomicExtension, BlockingAtomicOperation, BlockingCommand, BlockingInitiator,
        BlockingTarget, BlockingTransaction,
    };
    use crate::physical::{
        AmoKind, AmoWidth, AtomicAccessKind, AtomicOrdering, AtomicReservationContext,
        CommittedWriteSnapshot, ConditionalStatus, PhysicalAccessKind, PhysicalSpan,
    };

    struct AtomicTarget {
        outcome: AtomicI32,
        seen_operation: AtomicI32,
        seen_width: AtomicI32,
        seen_reserved: AtomicUsize,
    }
    unsafe extern "C" fn atomic_transport(user: *mut c_void, payload: *mut CPayload) {
        // SAFETY: the ABI guarantees a live payload and user pointer.
        let (target, payload) = unsafe { (&*(user.cast::<AtomicTarget>()), &mut *payload) };
        target
            .seen_operation
            .store(payload.atomic.operation, Ordering::SeqCst);
        target
            .seen_width
            .store(payload.atomic.width, Ordering::SeqCst);
        target.seen_reserved.store(
            payload.atomic.reserved_snapshot_length as usize,
            Ordering::SeqCst,
        );
        payload.atomic.outcome = target.outcome.load(Ordering::SeqCst);
        payload.atomic.snapshot_length = 2;
        payload.atomic.snapshot[0] = 0x34;
        payload.atomic.snapshot[1] = 0x12;
        payload.status = super::target::status::OK;
    }
    unsafe extern "C" fn atomic_debug(_: *mut c_void, debug: *mut CDebug) -> usize {
        // SAFETY: the ABI guarantees a live request.
        let debug = unsafe { &*debug };
        debug.length
    }
    unsafe extern "C" fn atomic_reset(_: *mut c_void) -> i32 {
        1
    }

    let target = Box::new(AtomicTarget {
        outcome: AtomicI32::new(atomic_outcome::LOAD_RESERVED),
        seen_operation: AtomicI32::new(-1),
        seen_width: AtomicI32::new(-1),
        seen_reserved: AtomicUsize::new(0),
    });
    let table = CTarget {
        max_delay_fs: 0,
        user: (&*target as *const AtomicTarget)
            .cast_mut()
            .cast::<c_void>(),
        b_transport: Some(atomic_transport),
        transport_dbg: Some(atomic_debug),
        transport_reset: Some(atomic_reset),
        get_direct_mem_ptr: None,
        quantum_ticks: 0,
        suspends_host_time: 0,
    };
    let adapter = super::target::CBlockingTarget::new(table).unwrap();

    let mut bytes = [0u8; 4];
    let mut transaction = BlockingTransaction::new(
        0x8000_0100,
        BlockingCommand::Write,
        &mut bytes,
        crate::blocking::BlockingExtension::atomic(
            PhysicalAccessKind::DataWrite,
            BlockingInitiator::Hart,
            BlockingAtomicExtension::store_conditional(
                AmoWidth::Word,
                AtomicOrdering {
                    aq: true,
                    rl: false,
                },
                Some(AtomicReservationContext {
                    reserved: PhysicalSpan {
                        paddr: 0x8000_0100,
                        width: crate::physical::PhysicalWidth::Word,
                    },
                    snapshot: CommittedWriteSnapshot::from_bytes(&[1, 2, 3, 4]).unwrap(),
                }),
            ),
        ),
    );
    adapter.b_transport(&mut transaction);
    assert_eq!(
        transaction.status(),
        crate::blocking::BlockingResponseStatus::Ok
    );
    assert_eq!(
        transaction.atomic_extension().unwrap().operation(),
        BlockingAtomicOperation::StoreConditional,
        "the request kind did not cross the ABI"
    );
    assert_eq!(target.seen_operation.load(Ordering::SeqCst), -1);
    assert_eq!(target.seen_width.load(Ordering::SeqCst), 4);
    assert_eq!(
        target.seen_reserved.load(Ordering::SeqCst),
        4,
        "the SC reservation snapshot did not cross the ABI"
    );
    assert_eq!(
        transaction.atomic_extension().unwrap().outcome(),
        Some(crate::blocking::BlockingAtomicOutcome::LoadReserved {
            snapshot: CommittedWriteSnapshot::from_bytes(&[0x34, 0x12]).unwrap()
        }),
        "the LR snapshot did not cross back"
    );

    // An RMW carries its operation code and produces a completion.
    target.outcome.store(atomic_outcome::RMW, Ordering::SeqCst);
    let mut transaction = BlockingTransaction::new(
        0x8000_0200,
        BlockingCommand::Write,
        &mut bytes,
        crate::blocking::BlockingExtension::atomic(
            PhysicalAccessKind::DataWrite,
            BlockingInitiator::Hart,
            BlockingAtomicExtension::rmw(AmoKind::Maxu, AmoWidth::Word, AtomicOrdering::default()),
        ),
    );
    adapter.b_transport(&mut transaction);
    assert_eq!(
        target.seen_operation.load(Ordering::SeqCst),
        i32::from(AmoKind::Maxu.code())
    );
    assert_eq!(
        transaction.atomic_extension().unwrap().outcome(),
        Some(crate::blocking::BlockingAtomicOutcome::Rmw)
    );
    assert_eq!(
        transaction.atomic_extension().unwrap().operation().kind(),
        AtomicAccessKind::Rmw
    );

    // A store-conditional outcome code maps to both conditional results.
    for (code, expected) in [
        (
            atomic_outcome::STORE_CONDITIONAL_SUCCESS,
            ConditionalStatus::Success,
        ),
        (
            atomic_outcome::STORE_CONDITIONAL_FAILURE,
            ConditionalStatus::Failure,
        ),
    ] {
        target.outcome.store(code, Ordering::SeqCst);
        let mut transaction = BlockingTransaction::new(
            0x8000_0300,
            BlockingCommand::Write,
            &mut bytes,
            crate::blocking::BlockingExtension::atomic(
                PhysicalAccessKind::DataWrite,
                BlockingInitiator::Hart,
                BlockingAtomicExtension::store_conditional(
                    AmoWidth::Word,
                    AtomicOrdering::default(),
                    None,
                ),
            ),
        );
        adapter.b_transport(&mut transaction);
        assert_eq!(
            transaction.atomic_extension().unwrap().outcome(),
            Some(crate::blocking::BlockingAtomicOutcome::StoreConditional(
                expected
            ))
        );
    }

    // An unrecognized outcome leaves no completion, so the adapter rejects an
    // apparently successful atomic response as a protocol failure.
    target.outcome.store(99, Ordering::SeqCst);
    let mut transaction = BlockingTransaction::new(
        0x8000_0400,
        BlockingCommand::Read,
        &mut bytes,
        crate::blocking::BlockingExtension::atomic(
            PhysicalAccessKind::DataRead,
            BlockingInitiator::Hart,
            BlockingAtomicExtension::load_reserved(AmoWidth::Word, AtomicOrdering::default()),
        ),
    );
    adapter.b_transport(&mut transaction);
    assert_eq!(
        transaction.atomic_extension().unwrap().outcome(),
        None,
        "an unknown C outcome code produced a completion"
    );
    assert_eq!(SNAPSHOT_CAPACITY, 64);
    assert_eq!(atomic_kind::RMW, 0);
}

#[test]
fn create_rejects_unusable_configurations() {
    let mut out = std::ptr::null_mut();
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = target.table(0);
    // SAFETY: `out` is writable and the table is complete.
    unsafe {
        assert_eq!(
            ruscv_machine_create(&table, 7, TICK_FS, &mut out),
            Status::InvalidArgument as i32
        );
        assert!(out.is_null(), "a rejected create published a handle");
        assert_eq!(
            ruscv_machine_create(&table, 0, 0, &mut out),
            Status::InvalidArgument as i32
        );
        let incomplete = CTarget {
            max_delay_fs: 0,
            user: std::ptr::null_mut(),
            b_transport: Some(target_b_transport),
            transport_dbg: None,
            transport_reset: Some(target_reset),
            get_direct_mem_ptr: None,
            quantum_ticks: 0,
            suspends_host_time: 0,
        };
        assert_eq!(
            ruscv_machine_create(&incomplete, 0, TICK_FS, &mut out),
            Status::InvalidArgument as i32
        );
        // A declared maximum that no tick period can represent is a
        // configuration failure, not a silently truncated bound.
        let overflow = target.table(u64::MAX);
        assert_eq!(
            ruscv_machine_create(&overflow, 0, 1, &mut out),
            Status::Config as i32
        );
    }
    assert!(out.is_null());
}

#[test]
fn calls_before_an_image_is_installed_report_no_image() {
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = target.table(0);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table and `handle` slot are live.
    unsafe {
        assert_eq!(
            ruscv_machine_create(&table, 3, TICK_FS, &mut handle),
            Status::Ok as i32
        );
        let mut facts = CBoundary::default();
        assert_eq!(
            ruscv_machine_boundary(handle, &mut facts),
            Status::NoImage as i32
        );
        assert_eq!(ruscv_machine_reset(handle), Status::NoImage as i32);
        let mut result = CResult::default();
        assert_eq!(
            ruscv_machine_result(handle, &mut result),
            Status::NoImage as i32
        );
        let mut grant = CGrant::default();
        assert_eq!(
            ruscv_machine_run_grant(handle, 0, 1, 1, &mut grant),
            Status::NoImage as i32
        );
        assert_eq!(
            ruscv_machine_load_elf(handle, std::ptr::null()),
            Status::InvalidArgument as i32
        );
        assert_eq!(ruscv_machine_destroy(handle), Status::Ok as i32);
    }
}

/// A RAM target that annotates one fixed delay for every transaction.
struct DelayedTarget {
    ram: Mutex<Vec<u8>>,
    delay: Femtoseconds,
}

impl DelayedTarget {
    fn new(delay_fs: u64) -> Self {
        Self {
            ram: Mutex::new(vec![0; 0x1_0000]),
            delay: Femtoseconds::new(delay_fs),
        }
    }
}

impl crate::blocking::BlockingTarget for DelayedTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        self.delay
    }

    fn b_transport(&self, transaction: &mut crate::blocking::BlockingTransaction<'_>) {
        let offset = (transaction.address() - elf::BASE) as usize;
        let len = transaction.data_length();
        let mut ram = self.ram.lock().unwrap();
        if transaction.command() == crate::blocking::BlockingCommand::Read {
            transaction
                .data_mut()
                .copy_from_slice(&ram[offset..offset + len]);
        } else {
            ram[offset..offset + len].copy_from_slice(transaction.data());
        }
        transaction.set_status(crate::blocking::BlockingResponseStatus::Ok);
        transaction.set_response_delay(self.delay);
    }

    fn transport_dbg(&self, transaction: &mut crate::blocking::BlockingDebug<'_>) -> usize {
        let offset = (transaction.address() - elf::BASE) as usize;
        let len = transaction.len();
        let mut ram = self.ram.lock().unwrap();
        if transaction.command() == crate::blocking::BlockingCommand::Read {
            transaction
                .data_mut()
                .copy_from_slice(&ram[offset..offset + len]);
        } else {
            ram[offset..offset + len].copy_from_slice(transaction.data());
        }
        len
    }

    fn transport_reset(&self) -> bool {
        true
    }
}

/// quantum F9: the in-crate `Session::reset` builds a fresh Platform, so no
/// local offset crosses the generation. `Q` = 1 syncs every turn, so a
/// one-turn grant after reset reports exactly that turn's committed time; a
/// seeded or inherited offset would show up as a larger committed end.
#[test]
fn quantum_session_reset_starts_with_no_local_offset() {
    let target = std::sync::Arc::new(crate::blocking::test_support::ScriptTarget::new(
        elf::BASE,
        0x1_0000,
    ));
    target.set_declared_max_delay(0);
    target.set_delay(0);
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.transport = crate::blocking::Transport::tlm(target);
    // In-crate selection only: the C ABI's quantum field arrives in U4.
    config.quantum = 1;
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(
            &[elf::nop(), elf::nop(), elf::nop(), elf::nop()],
            0,
            true,
            false,
            0x4000,
        ))
        .unwrap(),
    );
    let mut session = Session::new(config, image).unwrap();
    // Leave the first generation with a non-zero local offset.
    session.run_grant(0, 0x1000 * TICK_FS, 2).unwrap();
    assert!(session.facts().committed_end_fs > 0);
    session.reset().unwrap();
    session.run_grant(0, 0x1000 * TICK_FS, 1).unwrap();
    assert_eq!(
        session.facts().committed_end_fs,
        TICK_FS,
        "the reset generation committed an inherited local offset"
    );
}

// ---- Quantum U4: the C ABI surface (issue 204) ----
//
// The contract's U4 row (docs/dev-plan.md §4) drives these tests: D11's
// transaction offset and D12's header, `capi` and grant facts, with the grant
// side of F7.

/// `wfi`.
const WFI: u32 = 0x1050_0073;
/// `mret`.
const MRET: u32 = 0x3020_0073;

/// `csrrw rd, csr, rs1`.
fn csrrw(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | (u32::from(rd) << 7) | 0x73
}

/// `or rd, rs1, rs2`.
fn or(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (6 << 12) | (u32::from(rd) << 7) | 0x33
}

/// A RAM-only guest: `nops` no-ops followed by the HTIF exit `code`.
fn ram_only_exit_guest(nops: usize, code: u32) -> Vec<u32> {
    let mut program = vec![elf::nop(); nops];
    program.extend(exit_guest(code));
    program
}

/// The `mtimecmp` value the idle-jump guest arms, in Platform ticks.
const WAKE_TICKS: u64 = 40;

/// `wfi_wake_guest`'s `mtimecmp` store: the device access of its F1 turn.
///
/// A device store reaches the native CLINT, never the C target, so a test can
/// only name the turn by the instruction it executes.
const WFI_MTIMECMP_STORE: usize = 5;

/// A guest that arms `mtimecmp`, enables `MTIE` with `mstatus.MIE` clear,
/// waits, and then stores to RAM.
///
/// The wait is a time-derived crossing with no trap: `mie.MTIE` is set, so the
/// wake is due, and `mstatus.MIE` is clear, so the wake runs at the next PC
/// instead of entering a trap handler.  The store after `wfi` is the wake
/// turn's first data transaction.
fn wfi_wake_guest() -> Vec<u32> {
    vec![
        // x4 = the declared `.tohost` word, the wake turn's store target.
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        // x5 = the compare value, x6 = hart 0's `mtimecmp`.
        elf::addi(5, 0, WAKE_TICKS as i32),
        elf::lui(6, 0x2004),
        elf::sd(5, 6, 0),
        // MTIE on, MIE left clear.
        elf::addi(7, 0, 0x80),
        csrrs(0, crate::csr::machine::MIE, 7),
        WFI,
        // The wake turn: a RAM store, never a trap.
        elf::sd(0, 4, 0),
        // Spin so a bounded grant can sample further turns.
        elf::beq(0, 0, 0),
    ]
}

/// `sv39_clint_pte_guest`'s trap-handler index, so the test can name it.
const SV39_TRAP: usize = 35;

/// A guest whose Sv39 walk reads a CLINT-resident PTE, then touches RAM.
///
/// The root table lives at `0x8000_3000` and its entry 2 points at
/// `mtimecmp`, so the level-1 walk of the guest's identity megapage reads its
/// PTE from the CLINT device window.  That read is an F1 flush; the fetch and
/// the data access after it are RAM transactions of the same turn, so their
/// published local offset is `L - E` plus the ticks already charged.  The
/// megapage leaf value is `mtimecmp` itself, which stays far above `mtime`, so
/// MTIP stays low and no timer crossing is involved.
fn sv39_clint_pte_guest() -> Vec<u32> {
    let mut code = vec![
        // x1 = &trap_handler.
        elf::auipc(1, 0),
        elf::addi(1, 1, (SV39_TRAP * 4) as i32),
        csrrw(0, crate::csr::machine::MTVEC, 1),
        // x4 = the data VA and PA: the declared `.tohost` word.
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        // x2 = the root table at 0x8000_3000.
        elf::lui(2, 0x80),
        elf::addi(2, 2, 3),
        elf::slli(2, 2, 12),
        // x3 = hart 0's `mtimecmp`.
        elf::lui(3, 0x2004),
        // root[2] = non-leaf PTE at the CLINT's `mtimecmp` word: PPN 0x02004.
        elf::lui(5, 2),
        elf::addi(5, 5, 4),
        elf::slli(5, 5, 10),
        elf::ori(5, 5, 1),
        elf::sd(5, 2, 16),
        // `mtimecmp` = the level-1 leaf: PA 0x8000_0000, R|W|X|A|D.
        elf::lui(6, 0x80),
        elf::slli(6, 6, 10),
        elf::ori(6, 6, 0xcf),
        elf::sd(6, 3, 0),
        // satp = Sv39 with PPN 0x80003 (the root table).
        elf::addi(7, 0, 8),
        elf::slli(7, 7, 60),
        elf::lui(8, 0x80),
        elf::addi(8, 8, 3),
        or(7, 7, 8),
        csrrw(0, crate::csr::supervisor::SATP, 7),
        // mstatus = MPP = S, MPRV = 0.
        elf::addi(1, 0, 1),
        elf::slli(1, 1, 11),
        csrrw(0, crate::csr::machine::MSTATUS, 1),
        // mepc = the S-mode entry, then enter it.
        elf::auipc(1, 0),
        elf::addi(1, 1, ((SV39_ENTRY - 28) * 4) as i32),
        csrrw(0, crate::csr::machine::MEPC, 1),
        MRET,
        // The S-mode entry: two no-ops, then the data load whose walk reads the
        // CLINT-resident PTE before the RAM access.
        elf::nop(),
        elf::nop(),
        elf::ld(5, 4, 0),
    ];
    debug_assert_eq!(code.len(), SV39_ENTRY + 3);
    code.extend(trap_exit_from_htif(1));
    debug_assert_eq!(code.len() - 7, SV39_TRAP);
    code
}

/// The index of the first S-mode instruction in `sv39_clint_pte_guest`.
const SV39_ENTRY: usize = 32;

/// A trap handler that reports a failure through the HTIF exit.
fn trap_exit_from_htif(code: u32) -> Vec<u32> {
    let payload = code
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .unwrap();
    vec![
        elf::lui(9, 0x80),
        elf::addi(9, 9, 1),
        elf::slli(9, 9, 12),
        elf::addi(10, 0, payload as i32),
        elf::sd(10, 9, 0),
        0x0000_006f,
        elf::nop(),
    ]
}

/// quantum D12: the four `ruscv_boundary` quantum fields report the selection,
/// its declared bound and the cumulative sync count, and stay at the defaults
/// until the first grant of a generation.
#[test]
fn quantum_grant_facts_report_the_selection_and_its_bound() {
    // Before the first grant, `GrantFacts::default()` applies even with a
    // quantum selected (quantum D12).
    let pending = Harness::quantum(
        IsaProfile::Rv64imafdc as i32,
        &ram_only_exit_guest(2, 2),
        0,
        4,
    );
    let facts = pending.facts();
    assert_eq!(
        (
            facts.quantum_ticks,
            facts.interrupt_latency_bound_ticks,
            facts.quantum_syncs,
            facts.quantum_reached,
        ),
        (0, 0, 0, 0),
        "a boundary before the first grant did not report the D12 defaults"
    );

    for (quantum, bound) in [(0, 0), (1, 0), (4, 3)] {
        let harness = Harness::quantum(
            IsaProfile::Rv64imafdc as i32,
            &ram_only_exit_guest(2, 2),
            0,
            quantum,
        );
        // Three turns keep a larger quantum below its threshold, so only the
        // return sync is counted and only Q = 1 reports the test as reached.
        let grant = harness.grant(0, u64::MAX / 2, 3).expect("grant");
        let facts = grant.facts;
        assert_eq!(facts.quantum_ticks, quantum, "the quantum was misreported");
        assert_eq!(
            facts.interrupt_latency_bound_ticks, bound,
            "Q = {quantum}: the declared interrupt-latency bound is wrong"
        );
        // Q = 1 syncs at every boundary; a larger quantum does not reach its
        // threshold inside a four-turn grant with no forced sync.  With the
        // quantum off there is no offset and no sync to count.
        assert_eq!(
            facts.quantum_syncs,
            u64::from(quantum != 0),
            "Q = {quantum}: the return sync was not counted exactly once"
        );
        assert_eq!(
            facts.quantum_reached,
            i32::from(quantum == 1),
            "Q = {quantum}: the threshold result is wrong"
        );
        // The sync count is cumulative for the generation, so a second grant
        // continues from it (quantum D10/D12).
        let before = facts.quantum_syncs;
        let second = harness
            .grant(grant.committed_end_fs, u64::MAX / 2, 3)
            .expect("second grant");
        assert!(
            second.facts.quantum_syncs >= before,
            "the cumulative sync count went backwards"
        );
    }
}

/// quantum D12: a counted sync at a turn boundary ends a grant, and a forced
/// sync alone leaves `quantum_reached` false while a coincident threshold test
/// keeps it true.
#[test]
fn quantum_grant_ends_at_a_sync_without_reaching_the_threshold() {
    // The guest stores to `mtimecmp`: an F1 device access, so the turn ends in
    // a sync that is not D5's.
    let forced = Harness::quantum(IsaProfile::Rv64imafdc as i32, &wfi_wake_guest(), 0, 64);
    let first = forced.grant(0, u64::MAX / 2, 32).expect("grant");
    assert_eq!(
        first.facts.quantum_reached, 0,
        "a forced sync alone must not report the threshold as reached"
    );
    assert_eq!(
        first.facts.quantum_syncs, 1,
        "the forced sync was not counted exactly once"
    );
    assert_eq!(
        first.boundary_reached, 1,
        "a counted sync did not end the grant with boundary_reached"
    );
    assert_eq!(
        (first.facts.budget_exhausted, first.facts.deadline_blocked),
        (0, 0),
        "the grant ended on a bound it did not report"
    );

    // Q = 1 makes the same boundary's D5 test pass, so the coincident forced
    // sync cannot clear `reached` (quantum D5/D12).  Every boundary syncs
    // there, so each grant retires one turn: advance to the device store's own
    // turn, whose fetch names it.
    let coincident = Harness::quantum(IsaProfile::Rv64imafdc as i32, &wfi_wake_guest(), 0, 1);
    let mut start = 0;
    let mut store_grant = None;
    for index in 0..=WFI_MTIMECMP_STORE {
        // One slot spare, so the turn's own sync ends the grant and not its
        // budget: the two facts are reported apart.
        let grant = coincident.grant(start, u64::MAX / 2, 2).expect("grant");
        let calls = coincident.target.calls();
        coincident.target.clear_calls();
        start = grant.committed_end_fs;
        if index == WFI_MTIMECMP_STORE {
            assert!(
                calls
                    .iter()
                    .any(|call| call.address == elf::BASE + (WFI_MTIMECMP_STORE * 4) as u64),
                "the grant did not execute the device store's own turn: {calls:?}"
            );
            store_grant = Some(grant);
        }
    }
    let grant = store_grant.expect("the device store's turn was never reached");
    assert_eq!(
        grant.facts.quantum_ticks, 1,
        "the device store's turn ran under another quantum"
    );
    assert_eq!(
        grant.facts.quantum_reached, 1,
        "a coincident forced sync cleared the threshold result"
    );
    assert_eq!(
        grant.facts.budget_exhausted, 0,
        "the device store's turn ended on its budget, not its own sync"
    );
    assert_eq!(grant.boundary_reached, 1);
    // Q = 1 syncs every completed turn, so the cumulative count is its turns.
    assert_eq!(
        grant.facts.quantum_syncs, grant.facts.turns,
        "Q = 1 did not count one sync per completed turn"
    );
}

/// quantum D11: the request field carries the turn's `L - E` plus that turn's
/// charged ticks, in the cycle the quantum imposes, and is zero when off.
#[test]
fn quantum_local_offset_publishes_the_turn_sequence_and_is_zero_when_off() {
    // Three turns per grant keep every grant below the threshold, so the F7
    // return sync is the only sync and the cycle restarts at each grant.
    for (quantum, expected) in [
        (0, vec![0]),
        (1, vec![0]),
        // Three grants of three turns each: one group per turn, because the
        // turn's offset changes at every boundary.  The expectation is fixed,
        // so an implementation that publishes zero everywhere fails here.
        (4, vec![0, 1, 2, 0, 1, 2, 0, 1, 2]),
    ] {
        let harness = Harness::quantum(
            IsaProfile::Rv64imafdc as i32,
            &ram_only_exit_guest(9, 2),
            0,
            quantum,
        );
        let mut start = 0;
        let mut observed = Vec::new();
        for _ in 0..3 {
            let grant = harness.grant(start, u64::MAX / 2, 3).expect("grant");
            for call in harness.target.calls() {
                let offset = call.local_offset_fs / TICK_FS;
                if observed.last() != Some(&offset) {
                    observed.push(offset);
                }
            }
            harness.target.clear_calls();
            start = grant.committed_end_fs;
        }
        assert_eq!(
            observed, expected,
            "Q = {quantum}: the published local-offset sequence is wrong"
        );
    }
}

/// quantum D8/D12 with the F7 grant return: a grant near its horizon stops
/// inside it, reports `deadline_blocked` at the same turn as the parity run's
/// committed time, and the return sync makes `committed_end_fs` and
/// `consumed_fs` match that parity run.
#[test]
fn quantum_grant_near_its_horizon_never_overshoots() {
    // Every turn costs one tick, so the parity run's committed time after `n`
    // turns is `n` ticks; the completion bound is two ticks (18 worst-case
    // transactions of a zero-delay target plus the turn tick, twice).
    let guest = ram_only_exit_guest(24, 2);
    let parity_harness = Harness::new(IsaProfile::Rv64imafdc as i32, &guest, 0);
    let parity = parity_harness
        .grant(0, 5 * TICK_FS, 32)
        .expect("the parity grant");
    // The horizon is five ticks and the two-tick completion bound fits a turn
    // only while the cursor is at or below three, so both routes stop after
    // four completed turns and report the fifth as blocked.
    assert_eq!(
        parity.committed_end_fs,
        4 * TICK_FS,
        "the parity run did not stop at its horizon"
    );
    assert_eq!(
        parity.facts.deadline_blocked, 1,
        "the parity run did not report the blocked turn"
    );

    let harness = Harness::quantum(IsaProfile::Rv64imafdc as i32, &guest, 0, 64);
    let grant = harness
        .grant(0, 5 * TICK_FS, 32)
        .expect("the quantum grant");
    assert_eq!(
        grant.facts.deadline_blocked, 1,
        "the grant near its horizon did not report the blocked turn"
    );
    assert!(
        grant.committed_end_fs <= 5 * TICK_FS,
        "the grant overshot its horizon: committed {} fs against {} fs",
        grant.committed_end_fs,
        5 * TICK_FS
    );
    // Four completed turns have committed at the parity run's rate, and the F7
    // sync is what makes `committed_end_fs` (and therefore `consumed_fs`) equal
    // the parity run's at the same turn.
    assert_eq!(
        grant.facts.turns, 4,
        "the quantum grant completed a different number of turns"
    );
    assert_eq!(
        grant.committed_end_fs, parity.committed_end_fs,
        "the grant return did not carry the parity run's committed time"
    );
    assert_eq!(
        grant.consumed_fs, parity.consumed_fs,
        "the grant return did not carry the parity run's consumed time"
    );
}

/// quantum D11/D12 with F3: an idle jump ends its grant, and the wake turn's
/// transactions are at the jump target with a zero local offset.
#[test]
fn quantum_idle_jump_ends_the_grant_and_the_wake_starts_at_zero_offset() {
    let harness = Harness::quantum(IsaProfile::Rv64imafdc as i32, &wfi_wake_guest(), 0, 64);
    // First grant: the `mtimecmp` store's forced sync ends it.
    let first = harness.grant(0, u64::MAX / 2, 32).expect("first grant");
    assert_eq!(first.boundary_reached, 1);
    harness.target.clear_calls();
    // Second grant: turns to the wait, whose idle jump ends it.
    let jump = harness
        .grant(first.committed_end_fs, u64::MAX / 2, 32)
        .expect("jump");
    assert_eq!(
        jump.facts.waiting, 1,
        "the idle jump did not report the waiting state"
    );
    assert_eq!(
        jump.facts.deadline_blocked, 0,
        "the jump-ended grant reported a blocked deadline"
    );
    assert_eq!(
        jump.boundary_reached, 1,
        "an idle jump did not end the grant with boundary_reached"
    );
    let target_fs = WAKE_TICKS * TICK_FS;
    assert_eq!(
        jump.committed_end_fs, target_fs,
        "the grant ended at a time other than the jump target"
    );
    harness.target.clear_calls();
    // The wake turn's first transaction is the local time the jump landed on,
    // and its own offset is zero: F3's sync already made `C` and `E` the target.
    let wake = harness
        .grant(jump.committed_end_fs, u64::MAX / 2, 1)
        .expect("wake");
    assert_eq!(
        wake.committed_start_fs, target_fs,
        "the wake grant did not start at the jump target"
    );
    let calls = harness.target.calls();
    let first = calls.first().expect("the wake turn issued no transaction");
    assert_eq!(
        first.local_offset_fs, 0,
        "the wake turn's first transaction carried a stale local offset"
    );
    assert_eq!(
        wake.committed_start_fs + first.local_offset_fs,
        target_fs,
        "the wake transaction's local time is not the jump target"
    );
    // The store after `wfi` ran, so the wake took no trap.
    assert!(
        calls.iter().any(|call| call.address == elf::TOHOST
            && call.access_kind == super::target::access_kind::DATA_WRITE),
        "the wake turn did not execute the store after `wfi`: {calls:?}"
    );
}

/// quantum D11 with F1: a Sv39 walk that reads a CLINT-resident PTE publishes
/// `L - E` plus the ticks already charged, and an annotated delay is charged
/// once per transaction.
#[test]
fn quantum_sv39_walk_through_the_clint_publishes_the_local_time() {
    /// The root table `sv39_clint_pte_guest` builds, and its entry 2.
    const ROOT_ENTRY: u64 = elf::BASE + 0x3010;
    let harness = Harness::build_quantum(
        IsaProfile::Rv64imafdc as i32,
        &sv39_clint_pte_guest(),
        2 * TICK_FS,
        false,
        64,
        TICK_FS,
    );
    let mut start = 0;
    let mut sdkept = None;
    for _ in 0..8 {
        let grant = harness.grant(start, u64::MAX / 2, 32).expect("grant");
        let calls = harness.target.calls();
        harness.target.clear_calls();
        // The first S-mode turn: its fetch walks the page table, which reads
        // the CLINT-resident level-1 PTE (an F1 flush) before the RAM fetch.
        if sdkept.is_none() {
            if let Some(index) = calls.iter().position(|call| {
                call.address == ROOT_ENTRY
                    && call.access_kind == super::target::access_kind::DATA_READ
                    && call.local_offset_fs > 0
            }) {
                let base = calls[index].local_offset_fs;
                assert!(
                    base > 0,
                    "the walk read the root entry at a zero local offset"
                );
                assert_eq!(
                    calls[index + 1].local_offset_fs,
                    base + TICK_FS,
                    "the fetch after the walk did not add its one charged tick"
                );
                assert_eq!(
                    calls[index + 2].address,
                    ROOT_ENTRY,
                    "the second walk did not read the root entry"
                );
                assert_eq!(
                    calls[index + 2].local_offset_fs,
                    base + 2 * TICK_FS,
                    "the F1 flush lost the local time between the two walks"
                );
                assert_eq!(
                    calls[index + 3].local_offset_fs,
                    base + 3 * TICK_FS,
                    "the fetch after the flushed offset lost a charged tick"
                );
                sdkept = Some(base);
            }
        }
        start = grant.committed_end_fs;
        if harness.result().state != RunState::Running.code() {
            break;
        }
    }
    assert!(
        sdkept.is_some(),
        "the S-mode walk never published a non-zero local offset"
    );

    // The data load's turn: two walks and the data access, each charging the
    // target's one-tick delay exactly once.
    let harness = Harness::build_quantum(
        IsaProfile::Rv64imafdc as i32,
        &sv39_clint_pte_guest(),
        2 * TICK_FS,
        false,
        64,
        TICK_FS,
    );
    let mut start = 0;
    let mut load = None;
    for _ in 0..16 {
        let grant = harness.grant(start, u64::MAX / 2, 32).expect("grant");
        let calls = harness.target.calls();
        harness.target.clear_calls();
        if let Some(index) = calls.iter().position(|call| {
            call.address == elf::TOHOST && call.access_kind == super::target::access_kind::DATA_READ
        }) {
            let turn = &calls[index.saturating_sub(5)..=index];
            assert_eq!(
                turn.iter()
                    .map(|call| call.local_offset_fs)
                    .collect::<Vec<_>>(),
                vec![
                    0,
                    TICK_FS,
                    2 * TICK_FS,
                    3 * TICK_FS,
                    4 * TICK_FS,
                    5 * TICK_FS
                ],
                "the load's turn did not charge its delay once per transaction"
            );
            assert_eq!(
                turn.iter().map(|call| call.address).collect::<Vec<_>>(),
                vec![
                    ROOT_ENTRY,
                    elf::BASE + 0x88,
                    ROOT_ENTRY,
                    elf::BASE + 0x8a,
                    ROOT_ENTRY,
                    elf::TOHOST,
                ],
                "the load's turn did not walk twice and then read RAM"
            );
            load = Some(index);
        }
        start = grant.committed_end_fs;
        if harness.result().state != RunState::Running.code() {
            break;
        }
    }
    load.expect("the data load never reached RAM");
}

/// quantum D12 with F6: an unknown completion at non-zero `O` syncs its offset,
/// and a reset restores the configured quantum.
#[test]
fn quantum_unknown_completion_syncs_and_reset_restores_the_quantum() {
    let harness = Harness::build_quantum(
        IsaProfile::Rv64imafdc as i32,
        &direct_guest(2),
        0,
        false,
        8,
        0,
    );
    harness
        .target
        .unknown_on_data_read
        .store(true, Ordering::SeqCst);
    let failed = harness.grant(0, u64::MAX / 2, 16).expect("grant");
    let result = harness.result();
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "the unknown completion did not fail the run: {}",
        harness.diagnostic()
    );
    assert_eq!(failed.facts.uncertain, 1);
    assert_eq!(
        harness
            .target
            .calls()
            .last()
            .expect("a transaction")
            .local_offset_fs,
        5 * TICK_FS,
        "the failing turn did not start at a non-zero local offset"
    );
    // F6 committed the offset the failed turn knew, which is the parity run's
    // committed time at the same turn.
    assert_eq!(
        failed.committed_end_fs,
        5 * TICK_FS,
        "F6 did not commit the offset of the unknown completion"
    );
    let parity = Harness::new(IsaProfile::Rv64imafdc as i32, &direct_guest(2), 0);
    parity
        .target
        .unknown_on_data_read
        .store(true, Ordering::SeqCst);
    let parity_failed = parity.grant(0, u64::MAX / 2, 16).expect("parity grant");
    assert_eq!(
        failed.committed_end_fs, parity_failed.committed_end_fs,
        "the failing turn's committed time diverged from the parity run"
    );

    // Reset is the acknowledged recovery path and keeps the selection.
    harness
        .target
        .unknown_on_data_read
        .store(false, Ordering::SeqCst);
    // SAFETY: the handle is live on this thread.
    assert_eq!(
        unsafe { ruscv_machine_reset(harness.handle) },
        Status::Ok as i32,
        "reset did not recover the unknown completion"
    );
    harness.target.clear_calls();
    let after = harness.grant(0, 4 * TICK_FS, 4).expect("post-reset grant");
    assert_eq!(
        after.facts.quantum_ticks, 8,
        "reset did not restore the configured quantum"
    );
    assert_eq!(
        after.committed_start_fs, 0,
        "the reset generation did not restart the epoch"
    );
    assert!(
        after.facts.quantum_syncs > 0,
        "the reset generation did not sync through the quantum path"
    );
}

/// quantum D2/D12: a refused quantum is `RUSCV_ERROR_CONFIG`, and a NULL target
/// with no quantum stays native.
#[test]
fn quantum_config_refusals_are_config_and_a_null_target_stays_native() {
    let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    // `(Q + t + 1) x tick_period` overflows u64 femtoseconds.
    let table = target.quantum_table(0, u64::MAX - 100);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table and the handle slot are live.
    let status = unsafe { ruscv_machine_create(&table, 3, TICK_FS, &mut handle) };
    assert_eq!(
        status,
        Status::Config as i32,
        "an unrepresentable quantum was not a configuration failure"
    );
    assert!(handle.is_null(), "a rejected create published a handle");

    // A NULL target selects the native route, so no quantum can be selected and
    // a grant reports no quantum facts at all.
    let elf = elf_file(&exit_guest(2));
    let path = c_text(elf.path());
    let mut native = std::ptr::null_mut();
    // SAFETY: the handle slot and the path are live.
    unsafe {
        assert_eq!(
            ruscv_machine_create(std::ptr::null(), 3, TICK_FS, &mut native),
            Status::Ok as i32,
            "a native create failed"
        );
        assert_eq!(
            ruscv_machine_load_elf(native, path.as_ptr()),
            Status::Ok as i32,
            "the native handle did not load the image"
        );
        let mut grant = CGrant::default();
        assert_eq!(
            ruscv_machine_run_grant(native, 0, u64::MAX / 2, 8, &mut grant),
            Status::Ok as i32
        );
        assert_eq!(
            (
                grant.facts.quantum_ticks,
                grant.facts.quantum_syncs,
                grant.facts.quantum_reached,
            ),
            (0, 0, 0),
            "a native handle reported quantum facts"
        );
        let mut facts = CBoundary::default();
        assert_eq!(
            ruscv_machine_boundary(native, &mut facts),
            Status::Ok as i32
        );
        assert_eq!(facts.quantum_ticks, 0);
        assert_eq!(ruscv_machine_destroy(native), Status::Ok as i32);
    }
}

/// quantum D11: with the quantum off, no transaction carries a local offset,
/// even in a turn that has already charged delay before it.
#[test]
fn quantum_local_offset_is_zero_when_off_even_after_charged_delay() {
    // A one-tick response delay makes a multi-access turn's *second*
    // transaction see accumulated delay, which is exactly the value a guard on
    // "quantum off" must drop.  Without it the two fetch parcels of one
    // instruction publish 0 and 1,000,000 fs.
    let harness = Harness::build_quantum(
        IsaProfile::Rv64imafdc as i32,
        &ram_only_exit_guest(3, 2),
        TICK_FS,
        false,
        0,
        TICK_FS,
    );
    let grant = harness.grant(0, u64::MAX / 2, 3).expect("grant");
    let calls = harness.target.calls();
    assert!(
        calls.len() > 2,
        "the off-mode turn issued too few transactions to expose charged delay"
    );
    assert_eq!(
        calls
            .iter()
            .map(|call| call.local_offset_fs)
            .collect::<Vec<_>>(),
        vec![0; calls.len()],
        "a transaction published a local offset with the quantum off"
    );
    // The delay is still charged to committed time exactly once.
    assert_eq!(
        grant.consumed_fs,
        grant.facts.turns * TICK_FS + grant.facts.transport_delay_fs,
        "the off-mode grant did not charge its delay exactly once"
    );
}

/// quantum D12 with F2: a sync that a non-zero RAM `tohost` sample triggers
/// ends its own grant, so no later turn starts in that grant.
#[test]
fn quantum_tohost_sync_ends_its_grant() {
    // The guest writes a non-zero, non-terminal value to RAM `tohost`, which F2
    // syncs before the exit rule decodes it, and then keeps running.
    let code = vec![
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::addi(5, 0, 2),
        elf::sd(5, 4, 0),
        elf::nop(),
        elf::nop(),
        elf::nop(),
    ];
    let harness = Harness::quantum(IsaProfile::Rv64imafdc as i32, &code, 0, 64);
    let grant = harness.grant(0, u64::MAX / 2, 16).expect("grant");
    let calls = harness.target.calls();
    assert_eq!(
        grant.facts.quantum_syncs, 1,
        "the tohost sync was not counted exactly once"
    );
    assert_eq!(
        grant.boundary_reached, 1,
        "the tohost sync did not end its grant"
    );
    assert_eq!(
        grant.facts.turns, 5,
        "the grant ran past the tohost store's turn"
    );
    let last = calls.last().expect("a transaction");
    assert_eq!(
        (last.address, last.access_kind),
        (elf::TOHOST, super::target::access_kind::DATA_WRITE),
        "the grant did not end on the tohost store: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.address == elf::BASE + (5 * 4) as u64),
        "the grant fetched the instruction after the tohost sync: {calls:?}"
    );
    assert_eq!(
        last.local_offset_fs,
        (grant.facts.turns - 1) * TICK_FS,
        "the store's turn did not publish its own local offset"
    );
}

// ---- TLM-Modeled Devices U3: the C ABI device table and attach ----

/// The UART window the C device table names (device D8).
const DEVICE_WINDOW_BASE: u64 = crate::executor::SYSTEM_BUS_UART_BASE;
/// The UART window's published size, in bytes.
const DEVICE_WINDOW_SIZE: u64 = crate::executor::SYSTEM_BUS_UART_WINDOW as u64;
/// The window's line-status register offset.
const DEVICE_LSR: u64 = crate::peripherals::uart16550::reg_offset::LSR;
/// The window's receive/transmit holding register offset.
const DEVICE_RBR_THR: u64 = crate::peripherals::uart16550::reg_offset::RBR_THR;
/// The bit a polling guest needs in `LSR` so it stops waiting.
const DEVICE_LSR_TRANSMIT_EMPTY: u8 = 0x20;

/// One device transaction as the C payload published it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceCall {
    address: u64,
    access_kind: i32,
    length: usize,
    /// The request's `device_event`, which is always `NONE` (device D8).
    device_event: i32,
    delay_fs: u64,
    host_elapsed_fs: u64,
    may_suspend: i32,
    local_offset_fs: u64,
}

/// A device target whose callbacks are the C ABI's own calling convention.
///
/// It models the two UART registers the fixture guest touches: `LSR` answers
/// the transmit-empty bit and a `THR` write records its byte.  Every other
/// answer is scripted per test, which is how a test makes the target report a
/// protocol violation the Rust side must classify.
struct TestDevice {
    /// The window this table declares.
    window_base: u64,
    window_size: u64,
    /// Status every blocking transaction reports.
    status: AtomicI32,
    /// The tag every completed write reports (device D8).
    event: AtomicI32,
    /// Delay annotated on every transaction, in femtoseconds.
    delay_fs: AtomicU64,
    /// The host elapsed time `e` every transaction reports (nb D4).
    host_elapsed_fs: AtomicU64,
    /// Set `dmi_allowed` on every completed transaction.
    hint: AtomicBool,
    /// Mark every transaction outcome-unknown.
    unknown: AtomicBool,
    /// Number of blocking transactions.
    transactions: AtomicUsize,
    /// Number of transport resets the target acknowledged.
    resets: AtomicUsize,
    /// Whether the target acknowledges a transport reset.
    reset_ack: AtomicBool,
    /// The bytes a `THR` write carried, in order.
    transmitted: Mutex<Vec<u8>>,
    /// The bytes `receive` delivered, in order.
    receives: Mutex<Vec<u8>>,
    /// Every blocking transaction in order.
    calls: Mutex<Vec<DeviceCall>>,
    /// The handle a callback re-enters, when set.
    handle: AtomicPtr<RuscvMachine>,
    /// Probe the entries a callback leaves refused, once.
    probe_entries: AtomicBool,
    /// Status `ruscv_machine_run_grant` returned from inside a callback, or
    /// `i32::MIN` before the attempt.
    run_grant_status: AtomicI32,
    /// Status `ruscv_machine_attach_device` returned from inside a callback, or
    /// `i32::MIN` before the attempt.
    attach_status: AtomicI32,
    /// Status `ruscv_machine_invalidate_direct_mem` returned from inside a
    /// callback, or `i32::MIN` before the attempt.
    invalidate_status: AtomicI32,
}

impl TestDevice {
    fn new() -> Self {
        Self {
            window_base: DEVICE_WINDOW_BASE,
            window_size: DEVICE_WINDOW_SIZE,
            status: AtomicI32::new(super::target::status::OK),
            event: AtomicI32::new(device_event::NONE),
            delay_fs: AtomicU64::new(0),
            host_elapsed_fs: AtomicU64::new(0),
            hint: AtomicBool::new(false),
            unknown: AtomicBool::new(false),
            transactions: AtomicUsize::new(0),
            resets: AtomicUsize::new(0),
            reset_ack: AtomicBool::new(true),
            transmitted: Mutex::new(Vec::new()),
            receives: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            handle: AtomicPtr::new(std::ptr::null_mut()),
            probe_entries: AtomicBool::new(false),
            run_grant_status: AtomicI32::new(i32::MIN),
            attach_status: AtomicI32::new(i32::MIN),
            invalidate_status: AtomicI32::new(i32::MIN),
        }
    }

    fn table(&self) -> CDevice {
        CDevice {
            window_base: self.window_base,
            window_size: self.window_size,
            user: std::ptr::from_ref(self).cast_mut().cast::<c_void>(),
            b_transport: Some(device_b_transport),
            receive: Some(device_receive),
            transport_reset: Some(device_reset),
        }
    }

    fn calls(&self) -> Vec<DeviceCall> {
        self.calls.lock().unwrap().clone()
    }

    fn transmitted(&self) -> Vec<u8> {
        self.transmitted.lock().unwrap().clone()
    }

    fn receives(&self) -> Vec<u8> {
        self.receives.lock().unwrap().clone()
    }

    fn transactions(&self) -> usize {
        self.transactions.load(Ordering::SeqCst)
    }

    fn resets(&self) -> usize {
        self.resets.load(Ordering::SeqCst)
    }

    fn set_event(&self, tag: i32) {
        self.event.store(tag, Ordering::SeqCst);
    }
}

unsafe extern "C" fn device_b_transport(user: *mut c_void, payload: *mut CPayload) {
    // SAFETY: the ABI guarantees `user` is the live `TestDevice` and `payload`
    // is one live Rust-owned payload struct for this call.
    let (device, payload) = unsafe { (&*(user.cast::<TestDevice>()), &mut *payload) };
    device.transactions.fetch_add(1, Ordering::SeqCst);
    device.calls.lock().unwrap().push(DeviceCall {
        address: payload.address,
        access_kind: payload.access_kind,
        length: payload.length,
        device_event: payload.device_event,
        delay_fs: payload.delay_fs,
        host_elapsed_fs: payload.host_elapsed_fs,
        may_suspend: payload.may_suspend,
        local_offset_fs: payload.local_offset_fs,
    });
    let handle = device.handle.load(Ordering::SeqCst);
    if device.probe_entries.swap(false, Ordering::SeqCst) && !handle.is_null() {
        // device D8: a device callback obeys the RAM target's rules, so every
        // entry that reaches the callback check is re-entry while the one
        // exemption stays accepted.
        let mut grant = CGrant::default();
        // SAFETY: the handle is live and the call is made from this thread.
        let status = unsafe { ruscv_machine_run_grant(handle, 0, u64::MAX / 2, 1, &mut grant) };
        device.run_grant_status.store(status, Ordering::SeqCst);
        let table = device.table();
        // SAFETY: the handle and the copied table are live on this thread.
        let status = unsafe { ruscv_machine_attach_device(handle, device_window::UART, &table) };
        device.attach_status.store(status, Ordering::SeqCst);
        // SAFETY: the handle is live on this thread.
        let status = unsafe { ruscv_machine_invalidate_direct_mem(handle, 0, u64::MAX) };
        device.invalidate_status.store(status, Ordering::SeqCst);
    }
    let len = payload.length;
    // SAFETY: the ABI guarantees a Rust-owned buffer of exactly `length` bytes
    // that this callback exclusively borrows for its duration.
    let data = unsafe { std::slice::from_raw_parts_mut(payload.data, len) };
    let offset = payload.address.wrapping_sub(device.window_base);
    match payload.command {
        super::target::command::READ => {
            // `LSR` reports the transmit-empty bit so a polling guest makes
            // progress; every other register reads as zero.
            if offset == DEVICE_LSR {
                if let Some(byte) = data.first_mut() {
                    *byte = DEVICE_LSR_TRANSMIT_EMPTY;
                }
            }
        }
        _ => {
            if offset == DEVICE_RBR_THR {
                if let Some(byte) = data.first() {
                    device.transmitted.lock().unwrap().push(*byte);
                }
            }
        }
    }
    payload.status = device.status.load(Ordering::SeqCst);
    payload.delay_fs = device.delay_fs.load(Ordering::SeqCst);
    payload.host_elapsed_fs = device.host_elapsed_fs.load(Ordering::SeqCst);
    if device.unknown.load(Ordering::SeqCst) {
        payload.outcome_unknown = 1;
    }
    if device.hint.load(Ordering::SeqCst) && payload.status == super::target::status::OK {
        payload.dmi_allowed = 1;
    }
    if payload.command == super::target::command::WRITE {
        payload.device_event = device.event.load(Ordering::SeqCst);
    }
}

unsafe extern "C" fn device_receive(user: *mut c_void, byte: u8) {
    // SAFETY: as in `device_b_transport`; the callback takes no Rust reference
    // beyond the live fixture and returns nothing.
    let device = unsafe { &*(user.cast::<TestDevice>()) };
    device.receives.lock().unwrap().push(byte);
}

unsafe extern "C" fn device_reset(user: *mut c_void) -> i32 {
    // SAFETY: as in `device_b_transport`.
    let device = unsafe { &*(user.cast::<TestDevice>()) };
    let ack = device.reset_ack.load(Ordering::SeqCst);
    if ack {
        device.resets.fetch_add(1, Ordering::SeqCst);
    }
    i32::from(ack)
}

/// A guest that polls the UART's `LSR` and writes one byte to `THR`.
///
/// The first parcel is two compressed `c.nop`s, so the image only executes
/// under an IALIGN=16 profile, as the other fixtures require.
fn uart_guest(code: u32) -> Vec<u32> {
    vec![
        0x0001_0001,
        elf::lui(5, 0x10000), // 0x1000_0000, the UART window
        elf::lbu(7, 5, DEVICE_LSR as i32),
        elf::addi(6, 0, i32::from(b'H')),
        elf::sb(6, 5, DEVICE_RBR_THR as i32),
        elf::lui(4, 0x80),
        elf::addi(4, 4, 1),
        elf::slli(4, 4, 12),
        elf::standard_exit(code),
        elf::sd(5, 4, 0),
        0x0000_006f,
    ]
}

/// One handle with a C device table attached to the UART window.
struct DeviceHarness {
    handle: *mut RuscvMachine,
    device: Box<TestDevice>,
    _ram: Box<TestTarget>,
    _elf: tempfile::NamedTempFile,
}

impl Drop for DeviceHarness {
    fn drop(&mut self) {
        // SAFETY: the handle is live until this drop and was created here.
        unsafe { ruscv_machine_destroy(self.handle) };
    }
}

impl DeviceHarness {
    fn new(code: &[u32]) -> Self {
        let mut ram = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
        let table = ram.table(0);
        let mut handle = std::ptr::null_mut();
        // SAFETY: the table and its user pointer stay alive in this harness.
        let status = unsafe {
            ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, TICK_FS, &mut handle)
        };
        assert_eq!(status, Status::Ok as i32, "create");
        let device = Box::new(TestDevice::new());
        device.handle.store(handle, Ordering::SeqCst);
        let device_table = device.table();
        // SAFETY: the handle and the table are live and this is the creating
        // thread.
        let status =
            unsafe { ruscv_machine_attach_device(handle, device_window::UART, &device_table) };
        assert_eq!(status, Status::Ok as i32, "attach_device");
        let harness = Self {
            handle,
            device,
            _ram: ram,
            _elf: elf_file(code),
        };
        let path = c_text(harness._elf.path());
        // SAFETY: the handle is live and the path is NUL-terminated.
        let status = unsafe { ruscv_machine_load_elf(handle, path.as_ptr()) };
        assert_eq!(status, Status::Ok as i32, "load_elf");
        harness
    }

    fn run_to_result(&self, budget: u64, grants: usize) -> CResult {
        handle_run_to_result(self.handle, budget, grants)
    }

    fn facts(&self) -> CBoundary {
        handle_facts(self.handle)
    }

    fn diagnostic(&self) -> String {
        handle_diagnostic(self.handle)
    }
}

/// One TLM-route handle over a plain C RAM table, and the fixture behind it.
fn tlm_handle() -> (*mut RuscvMachine, Box<TestTarget>) {
    let mut ram = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
    let table = ram.table(0);
    let mut handle = std::ptr::null_mut();
    // SAFETY: the table and its user pointer outlive the returned handle.
    let status = unsafe {
        ruscv_machine_create(&table, IsaProfile::Rv64imafdc as i32, TICK_FS, &mut handle)
    };
    assert_eq!(status, Status::Ok as i32, "create");
    (handle, ram)
}

#[test]
fn an_attached_c_device_target_serves_the_uart_window() {
    // device D8: the attach succeeds before the first load and the attached
    // target's own count is non-zero for a guest that touches the window.
    let device = DeviceHarness::new(&uart_guest(2));
    let result = device.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        device.diagnostic()
    );
    assert_eq!(result.exit_code, 2);
    assert!(
        device.device.transactions() > 0,
        "the attached C device target served no transaction"
    );
    assert_eq!(
        device.device.transmitted(),
        vec![b'H'],
        "the copied table did not serve the guest's THR write"
    );
    assert!(
        device.device.resets() > 0,
        "the attached target was never reset at a Platform build"
    );
    let calls = device.device.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.access_kind == super::target::access_kind::DATA_READ)
            .count(),
        1,
        "the LSR poll did not reach the device target exactly once: {calls:?}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.access_kind == super::target::access_kind::DATA_WRITE)
            .count(),
        1,
        "the THR write did not reach the device target exactly once: {calls:?}"
    );

    // The same guest on the in-tree window: the run result is its parity
    // result, and only the target that served it differs.
    let in_tree = Harness::new(IsaProfile::Rv64imafdc as i32, &uart_guest(2), 0);
    let parity = in_tree.run_to_result(1_000, 16);
    assert_eq!(
        (result.state, result.exit_code),
        (parity.state, parity.exit_code),
        "the host device route is not its parity result"
    );
}

#[test]
fn every_documented_attach_refusal_returns_its_code() {
    use Status::{Config, InvalidArgument};

    // A NULL handle, a NULL table and an unknown window are argument refusals,
    // so they precede every configuration refusal (device D8).
    let (handle, _ram) = tlm_handle();
    let mut device = Box::new(TestDevice::new());
    let table = device.table();
    // SAFETY: every pointer here is either null or live on this thread.
    unsafe {
        assert_eq!(
            ruscv_machine_attach_device(std::ptr::null_mut(), device_window::UART, &table),
            InvalidArgument as i32,
            "a NULL handle is an argument refusal"
        );
        assert_eq!(
            ruscv_machine_attach_device(handle, device_window::UART, std::ptr::null()),
            InvalidArgument as i32,
            "a NULL table is an argument refusal"
        );
        assert_eq!(
            ruscv_machine_attach_device(handle, device_window::UART + 1, &table),
            InvalidArgument as i32,
            "an unknown window is an argument refusal"
        );
        let incomplete = CDevice {
            b_transport: None,
            ..device.table()
        };
        assert_eq!(
            ruscv_machine_attach_device(handle, device_window::UART, &incomplete),
            InvalidArgument as i32,
            "a table without its callbacks is an argument refusal"
        );
    }

    // A window the table does not name is a configuration refusal.
    device.window_base = DEVICE_WINDOW_BASE + 4;
    // SAFETY: the handle and the table are live on this thread.
    assert_eq!(
        unsafe { ruscv_machine_attach_device(handle, device_window::UART, &device.table()) },
        Config as i32,
        "a mismatched window base is a configuration refusal"
    );
    device.window_base = DEVICE_WINDOW_BASE;
    device.window_size = DEVICE_WINDOW_SIZE + 1;
    // SAFETY: as above.
    assert_eq!(
        unsafe { ruscv_machine_attach_device(handle, device_window::UART, &device.table()) },
        Config as i32,
        "a mismatched window size is a configuration refusal"
    );
    device.window_size = DEVICE_WINDOW_SIZE;

    // The first attach succeeds; the second is refused.
    // SAFETY: as above.
    assert_eq!(
        unsafe { ruscv_machine_attach_device(handle, device_window::UART, &device.table()) },
        Status::Ok as i32,
        "the first attach is accepted"
    );
    // SAFETY: as above.
    assert_eq!(
        unsafe { ruscv_machine_attach_device(handle, device_window::UART, &device.table()) },
        Config as i32,
        "a second attach for the same window is a configuration refusal"
    );
    // SAFETY: the handle is destroyed exactly once, at the end of the test.
    unsafe { ruscv_machine_destroy(handle) };

    // A late attach is refused: the table must be attached before the first
    // load.
    let (handle, _ram) = tlm_handle();
    let elf = elf_file(&exit_guest(2));
    let path = c_text(elf.path());
    // SAFETY: the handle and the path are live on this thread.
    unsafe {
        assert_eq!(
            ruscv_machine_load_elf(handle, path.as_ptr()),
            Status::Ok as i32,
            "load_elf"
        );
        assert_eq!(
            ruscv_machine_attach_device(handle, device_window::UART, &device.table()),
            Config as i32,
            "an attach after the first load is a configuration refusal"
        );
    }
    // SAFETY: the handle is destroyed exactly once.
    unsafe { ruscv_machine_destroy(handle) };

    // A handle created with a NULL ruscv_target has no TLM route, so it cannot
    // serve a device window.
    let mut native = std::ptr::null_mut();
    // SAFETY: `native` is writable and the route needs no table.
    let status = unsafe {
        ruscv_machine_create(
            std::ptr::null(),
            IsaProfile::Rv64imafdc as i32,
            TICK_FS,
            &mut native,
        )
    };
    assert_eq!(status, Status::Ok as i32, "create a native handle");
    // SAFETY: the handle and the table are live on this thread.
    assert_eq!(
        unsafe { ruscv_machine_attach_device(native, device_window::UART, &device.table()) },
        Config as i32,
        "a handle without a TLM route is a configuration refusal"
    );
    // SAFETY: the handle is destroyed exactly once.
    unsafe { ruscv_machine_destroy(native) };
}

#[test]
fn a_device_payload_carries_zero_annotations_and_no_event_request() {
    // device D8: the host device receives `device_event` zero, zero delay, zero
    // host elapsed time and `may_suspend` zero, with the window's own width.
    let device = DeviceHarness::new(&uart_guest(2));
    let result = device.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        device.diagnostic()
    );
    let calls = device.device.calls();
    assert!(!calls.is_empty(), "the device target served no transaction");
    for call in &calls {
        assert_eq!(
            call.device_event,
            device_event::NONE,
            "a device request carried a response tag: {call:?}"
        );
        assert_eq!(
            call.delay_fs, 0,
            "a device request carried a delay: {call:?}"
        );
        assert_eq!(
            call.host_elapsed_fs, 0,
            "a device request carried host elapsed time: {call:?}"
        );
        assert_eq!(
            call.may_suspend, 0,
            "a device request carried the suspension flag: {call:?}"
        );
        assert_eq!(
            call.local_offset_fs, 0,
            "a device request carried a local offset: {call:?}"
        );
        assert_eq!(call.length, 1, "the window is byte-only: {call:?}");
        assert!(
            call.address >= DEVICE_WINDOW_BASE
                && call.address < DEVICE_WINDOW_BASE + DEVICE_WINDOW_SIZE,
            "a request outside the window reached the target: {call:?}"
        );
    }
    // A device transaction never charges the transport totals (device D4).
    assert_eq!(device.facts().transport_delay_fs, 0);
}

#[test]
fn a_uart_transmit_tag_is_accepted_while_an_htif_write_tag_fails_the_run() {
    // device D8: the tag is copied from the C response, so a matching tag is
    // the access's one event and a tag from the other window is a protocol
    // failure the run reports.
    let accepted = DeviceHarness::new(&uart_guest(2));
    accepted.device.set_event(device_event::UART_TRANSMIT);
    let result = accepted.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        accepted.diagnostic()
    );
    assert_eq!(result.exit_code, 2);

    let refused = DeviceHarness::new(&uart_guest(2));
    refused.device.set_event(device_event::HTIF_WRITE);
    let result = refused.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "an HTIF event on the UART window is not a failure: {}",
        refused.diagnostic()
    );
    assert_ne!(result.exit_code, 2);
    let diagnostic = refused.diagnostic();
    assert!(
        diagnostic.contains("htif-write") && diagnostic.contains("UART window"),
        "the failure does not name the copied tag: {diagnostic}"
    );

    // An out-of-contract tag is not a value the ABI can carry, so it is a
    // protocol failure too rather than a silently dropped event.
    let unknown = DeviceHarness::new(&uart_guest(2));
    unknown.device.set_event(device_event::HTIF_WRITE + 1);
    let result = unknown.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "an unrecognized tag was accepted: {}",
        unknown.diagnostic()
    );
}

#[test]
fn a_host_device_delay_elapsed_time_or_hint_is_a_protocol_failure() {
    // device D2/D4: a device transaction annotates nothing, so each of the three
    // response fields a host can set is a protocol failure of that access.
    let delay = DeviceHarness::new(&uart_guest(2));
    delay.device.delay_fs.store(TICK_FS, Ordering::SeqCst);
    let result = delay.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "an annotated device delay was accepted: {}",
        delay.diagnostic()
    );
    assert!(
        delay.diagnostic().contains("delay"),
        "the failure does not name the delay: {}",
        delay.diagnostic()
    );

    let elapsed = DeviceHarness::new(&uart_guest(2));
    elapsed
        .device
        .host_elapsed_fs
        .store(TICK_FS, Ordering::SeqCst);
    let result = elapsed.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "host time inside a device callback was accepted: {}",
        elapsed.diagnostic()
    );
    assert!(
        elapsed.diagnostic().contains("host elapsed"),
        "the failure does not name the host elapsed time: {}",
        elapsed.diagnostic()
    );

    let hinted = DeviceHarness::new(&uart_guest(2));
    hinted.device.hint.store(true, Ordering::SeqCst);
    let result = hinted.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "a device direct-memory hint was accepted: {}",
        hinted.diagnostic()
    );
    assert!(
        hinted.diagnostic().contains("direct-memory hint"),
        "the failure does not name the hint: {}",
        hinted.diagnostic()
    );
    // device D5: a device window never offers direct memory, so no grant is
    // cached and nothing is charged.
    assert_eq!(hinted.facts().transport_delay_fs, 0);
}

#[test]
fn the_device_callback_cannot_reenter_the_abi() {
    // device D8: a device callback obeys the RAM target's rules, so every entry
    // that reaches the callback check is re-entry while direct-memory
    // invalidation stays accepted.
    let device = DeviceHarness::new(&uart_guest(2));
    device.device.probe_entries.store(true, Ordering::SeqCst);
    let result = device.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Exited.code(),
        "diagnostic: {}",
        device.diagnostic()
    );
    assert_eq!(
        device.device.run_grant_status.load(Ordering::SeqCst),
        Status::Reentrant as i32,
        "a device callback re-entered run_grant"
    );
    assert_eq!(
        device.device.attach_status.load(Ordering::SeqCst),
        Status::Reentrant as i32,
        "a device callback re-entered attach_device"
    );
    assert_eq!(
        device.device.invalidate_status.load(Ordering::SeqCst),
        Status::Ok as i32,
        "the invalidation entry is not accepted inside a device callback"
    );
}

#[test]
fn a_c_ram_table_that_reports_a_device_event_is_a_protocol_failure() {
    // device D2: only a device window publishes an event.  The copied field is
    // therefore live for a RAM table too, and the RAM adapter refuses it instead
    // of the ABI dropping it silently.
    let ram = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), 0);
    ram.target
        .event
        .store(device_event::UART_TRANSMIT, Ordering::SeqCst);
    let result = ram.run_to_result(1_000, 16);
    assert_eq!(
        result.state,
        RunState::Failed.code(),
        "a RAM target's device event was accepted: {}",
        ram.diagnostic()
    );
    assert!(
        ram.diagnostic().contains("only a device window"),
        "the failure does not name the RAM rule: {}",
        ram.diagnostic()
    );
}

#[test]
fn the_device_wrapper_delivers_a_pushed_byte_to_receive() {
    // device D8: `receive` returns nothing and reaches the copied callback with
    // the byte.  No entry in this ABI version pushes a byte, so the wrapper's
    // own mapping is what proves the copied pointer and callback are used.
    let device = Box::new(TestDevice::new());
    let wrapper = super::target::CDeviceTarget::new(device.table()).unwrap();
    wrapper.receive(b'Z');
    assert_eq!(device.receives(), vec![b'Z']);
}
