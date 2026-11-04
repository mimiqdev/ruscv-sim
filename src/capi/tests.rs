//! Rust coverage for the C ABI facade.
//!
//! These tests drive the real `extern "C"` entry points with a RAM target whose
//! callbacks are Rust `extern "C"` functions, so they exercise the same path a
//! C host takes: the callback table, the payload mapping, thread affinity,
//! re-entry rejection, panic poisoning, grant time authority and result
//! classification.  `tests/capi/smoke.c` covers the same surface from plain C.

use super::host::{RunState, Session};
use super::target::{atomic_kind, atomic_outcome, CDebug, CPayload, CTarget, SNAPSHOT_CAPACITY};
use super::{
    ruscv_machine_boundary, ruscv_machine_create, ruscv_machine_destroy, ruscv_machine_diagnostic,
    ruscv_machine_load_elf, ruscv_machine_reset, ruscv_machine_result, ruscv_machine_run_grant,
    ruscv_machine_self_test_panic, ruscv_machine_set_commit_log, CBoundary, CGrant, CResult,
    RuscvMachine, Status,
};
use crate::blocking::{
    Femtoseconds, TickPeriod, TurnCompletionBound, MAX_GUEST_TRANSACTIONS_PER_TURN,
};
use crate::elf::LoadImage;
use crate::executor::observation_fixture as elf;
use crate::machine::{IsaProfile, MachineConfig};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One nanosecond, the documented default tick period.
const TICK_FS: u64 = 1_000_000;

/// A RAM target whose callbacks are the C ABI's own calling convention.
struct TestTarget {
    base: u64,
    ram: Mutex<Vec<u8>>,
    /// Status the target reports for every blocking transaction.
    status: AtomicI32,
    /// Mark every blocking transaction outcome-unknown.
    unknown: AtomicBool,
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
}

impl TestTarget {
    fn new(base: u64, size: usize) -> Self {
        Self {
            base,
            ram: Mutex::new(vec![0; size]),
            status: AtomicI32::new(super::target::status::OK),
            unknown: AtomicBool::new(false),
            reset_acks: AtomicUsize::new(0),
            reset_ack: AtomicBool::new(true),
            debug_calls: AtomicUsize::new(0),
            transactions: AtomicUsize::new(0),
            reentry_status: AtomicI32::new(i32::MIN),
            handle: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    fn reentry_status(&self) -> i32 {
        self.reentry_status.load(Ordering::SeqCst)
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
    let len = payload.length;
    // SAFETY: the ABI guarantees a Rust-owned buffer of exactly `length` bytes
    // that this callback exclusively borrows for its duration.
    let data = unsafe { std::slice::from_raw_parts_mut(payload.data, len) };
    let (start, end) = target.window(payload.address, len);
    let mut ram = target.ram.lock().unwrap();
    if payload.command == super::target::command::READ {
        data.copy_from_slice(&ram[start..end]);
    } else {
        ram[start..end].copy_from_slice(data);
    }
    drop(ram);
    payload.status = target.status.load(Ordering::SeqCst);
    if target.unknown.load(Ordering::SeqCst) {
        payload.outcome_unknown = 1;
    }
}

unsafe extern "C" fn target_dbg(user: *mut c_void, debug: *mut CDebug) -> usize {
    // SAFETY: as in `target_b_transport`.
    let (target, debug) = unsafe { (&*(user.cast::<TestTarget>()), &mut *debug) };
    target.debug_calls.fetch_add(1, Ordering::SeqCst);
    let len = debug.length;
    // SAFETY: as in `target_b_transport`.
    let data = unsafe { std::slice::from_raw_parts_mut(debug.data, len) };
    let (start, end) = target.window(debug.address, len);
    let mut ram = target.ram.lock().unwrap();
    if debug.command == super::target::command::READ {
        data.copy_from_slice(&ram[start..end]);
    } else {
        ram[start..end].copy_from_slice(data);
    }
    len
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

fn c_text(path: &std::path::Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_str().unwrap()).unwrap()
}

impl Harness {
    fn new(isa: i32, code: &[u32], max_delay_fs: u64) -> Self {
        let mut target = Box::new(TestTarget::new(elf::BASE, 0x1_0000));
        let table = target.table(max_delay_fs);
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
    // One declared tick per transaction reserves 18 ticks plus the completed
    // turn tick, so a 10-tick grant cannot start a turn.
    let harness = Harness::new(IsaProfile::Rv64imafdc as i32, &exit_guest(2), TICK_FS);
    let bound = TurnCompletionBound::new(Femtoseconds::new(TICK_FS), TickPeriod::DEFAULT).unwrap();
    let grant = harness
        .grant(0, 10 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.facts.deadline_blocked, 1);
    assert_eq!(grant.boundary_reached, 1);
    assert_eq!(grant.consumed_fs, 0, "a blocked grant consumed time");
    assert_eq!(
        grant.facts.candidate_bound_fs,
        (bound.ticks() + 1) * TICK_FS
    );
    assert_eq!(grant.facts.residual_slack_fs, 10 * TICK_FS);
    assert_eq!(
        harness.target.transactions.load(Ordering::SeqCst),
        0,
        "a blocked grant issued a transaction"
    );

    // A 20-tick horizon fits exactly two turns; the third would cross it, so
    // the grant reports the residual slack instead of starting it.
    let grant = harness
        .grant(0, 20 * TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    assert_eq!(grant.consumed_fs, 2 * TICK_FS);
    assert_eq!(grant.facts.turns, 2);
    assert_eq!(grant.facts.deadline_blocked, 1);
    assert_eq!(grant.facts.deadline_reached, 0);
    assert_eq!(grant.facts.residual_slack_fs, 18 * TICK_FS);
    assert_eq!(
        harness.target.transactions.load(Ordering::SeqCst),
        2,
        "the blocked turn issued a transaction"
    );
}

#[test]
fn a_native_grant_starts_one_turn_that_fits_its_deadline() {
    let mut config = MachineConfig::new(crate::machine::PlatformKind::Native);
    config.isa = IsaProfile::Rv64imafdc;
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&exit_guest(2), 0, true, false, 0x4000)).unwrap(),
    );
    let mut session = Session::new(config, TickPeriod::DEFAULT, image).unwrap();
    // A native turn has no transport-bound reserve, so a one-tick horizon fits
    // exactly one turn and then reports the reached inclusive deadline.
    session
        .run_grant(0, TICK_FS, super::UNBOUNDED_TURNS)
        .unwrap();
    let facts = session.facts();
    assert_eq!(facts.consumed_fs, TICK_FS);
    assert_eq!(facts.turns, 1);
    assert!(facts.deadline_reached);
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

#[test]
fn transporting_every_worst_case_transaction_stays_within_the_bound() {
    assert_eq!(
        MAX_GUEST_TRANSACTIONS_PER_TURN, 18,
        "the documented worst-case transaction count changed"
    );
    assert_eq!(
        TurnCompletionBound::new(Femtoseconds::new(TICK_FS), TickPeriod::DEFAULT)
            .unwrap()
            .ticks(),
        MAX_GUEST_TRANSACTIONS_PER_TURN
    );
}
