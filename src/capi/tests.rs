//! Rust coverage for the C ABI facade.
//!
//! These tests drive the real `extern "C"` entry points with a RAM target whose
//! callbacks are Rust `extern "C"` functions, so they exercise the same path a
//! C host takes: the callback table, the payload mapping, thread affinity,
//! re-entry rejection, panic poisoning, grant time authority and result
//! classification.  `tests/capi/smoke.c` covers the same surface from plain C.

use super::host::{RunState, Session};
use super::target::{
    atomic_kind, atomic_outcome, dmi_access, dmi_result, CDebug, CDirectMem, CPayload, CTarget,
    SNAPSHOT_CAPACITY,
};
use super::{
    ruscv_machine_boundary, ruscv_machine_create, ruscv_machine_destroy, ruscv_machine_diagnostic,
    ruscv_machine_invalidate_direct_mem, ruscv_machine_load_elf, ruscv_machine_reset,
    ruscv_machine_result, ruscv_machine_run_grant, ruscv_machine_self_test_panic,
    ruscv_machine_set_commit_log, CBoundary, CGrant, CResult, RuscvMachine, Status,
};
use crate::blocking::{Femtoseconds, TickPeriod, TurnCompletionBound};
use crate::elf::LoadImage;
use crate::executor::observation_fixture as elf;
use crate::machine::{IsaProfile, MachineConfig};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One nanosecond, the documented default tick period.
const TICK_FS: u64 = 1_000_000;

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
        }
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
    let handle = target.handle.load(Ordering::SeqCst);
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
        CTarget {
            max_delay_fs,
            user: std::ptr::from_mut(self).cast::<c_void>(),
            b_transport: Some(target_b_transport),
            transport_dbg: Some(target_dbg),
            transport_reset: Some(target_reset),
            get_direct_mem_ptr: None,
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

impl Harness {
    fn new(isa: i32, code: &[u32], max_delay_fs: u64) -> Self {
        Self::build(isa, code, max_delay_fs, false)
    }

    /// One handle whose target answers a direct-memory query.
    fn direct(isa: i32, code: &[u32], max_delay_fs: u64) -> Self {
        Self::build(isa, code, max_delay_fs, true)
    }

    fn build(isa: i32, code: &[u32], max_delay_fs: u64, direct: bool) -> Self {
        let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
        target.hint.store(true, Ordering::SeqCst);
        let table = if direct {
            target.direct_table(max_delay_fs)
        } else {
            target.table(max_delay_fs)
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
        let mut grant = CGrant::default();
        // SAFETY: the handle is live.
        let status = unsafe {
            ruscv_machine_run_grant(self.handle, start_fs, max_end_fs, budget, &mut grant)
        };
        if status == Status::Ok as i32 {
            Ok(grant)
        } else {
            Err(status)
        }
    }

    fn result(&self) -> CResult {
        let mut result = CResult::default();
        // SAFETY: the handle is live.
        let status = unsafe { ruscv_machine_result(self.handle, &mut result) };
        assert_eq!(status, Status::Ok as i32, "result");
        result
    }

    fn facts(&self) -> CBoundary {
        let mut facts = CBoundary::default();
        // SAFETY: the handle is live.
        let status = unsafe { ruscv_machine_boundary(self.handle, &mut facts) };
        assert_eq!(status, Status::Ok as i32, "boundary");
        facts
    }

    fn diagnostic(&self) -> String {
        let mut buffer = vec![0 as std::ffi::c_char; 512];
        // SAFETY: the handle is live and the buffer is writable.
        unsafe { ruscv_machine_diagnostic(self.handle, buffer.as_mut_ptr(), buffer.len()) };
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
    fn run_to_result(&self, budget: u64, grants: usize) -> CResult {
        let mut start = 0;
        for _ in 0..grants {
            let Ok(grant) = self.grant(start, u64::MAX / 2, budget) else {
                break;
            };
            start = grant.committed_end_fs;
            let result = self.result();
            if result.state != RunState::Running.code() {
                return result;
            }
        }
        self.result()
    }

    /// Configures the inclusive range this target's queries grant.
    fn grant_range(&self, range: Option<(u64, u64)>) {
        *self.target.grant.lock().unwrap() = range;
    }

    fn invalidate(&self, start: u64, end: u64) -> i32 {
        // SAFETY: the handle is live and this is the creating thread.
        unsafe { ruscv_machine_invalidate_direct_mem(self.handle, start, end) }
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
    // previous header published (direct-memory D12).
    use std::mem::{offset_of, size_of};
    assert_eq!(offset_of!(CTarget, max_delay_fs), 0);
    assert_eq!(offset_of!(CTarget, user), 8);
    assert_eq!(offset_of!(CTarget, b_transport), 16);
    assert_eq!(offset_of!(CTarget, transport_dbg), 24);
    assert_eq!(offset_of!(CTarget, transport_reset), 32);
    assert_eq!(offset_of!(CTarget, get_direct_mem_ptr), 40);
    assert_eq!(size_of::<CTarget>(), 48);

    assert_eq!(offset_of!(CPayload, atomic), 56);
    assert_eq!(
        offset_of!(CPayload, dmi_allowed),
        offset_of!(CPayload, atomic) + size_of::<super::target::CAtomic>()
    );
    assert_eq!(offset_of!(CPayload, dmi_allowed), 240);
    assert_eq!(size_of::<CPayload>(), 248);
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
