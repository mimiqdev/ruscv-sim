//! A8 T3 Hart atomic dispatch, per-Hart reservation, and writer-visibility
//! suite (dev-plan §5.2–§5.6, §11 items 2–4).
//!
//! These tests exercise the standard route — cores configured with the
//! validated data port — where every admitted AMO/LR/SC issues exactly one
//! atomic envelope.  They prove:
//!
//! * the complete §2/§5.3 operation matrix at both widths with Hart-owned
//!   arithmetic (`hart_amods`) and exactly-one-envelope issue;
//! * Hart-side preconditions that issue zero physical requests (reserved
//!   `funct5`, LR `rs2 != 0`, bad `funct3`, uncovered SC span);
//! * the per-Hart reservation lifecycle (recorded span + snapshot in
//!   `CoreState`, consume/replace/retain rules, reset/reload clearing);
//! * the writer-visibility contract W1–W3: a committed overlapping write —
//!   guest store, FP store, AMO write side, host `write_mem`, or the
//!   run-boundary `tohost` clear — fails the SC; a committed non-overlapping
//!   or a rejected write does not; the check is committed writes, never
//!   "any write"; and a competing writer is excluded only during the
//!   envelope critical section, never across Hart steps.
//!
//! The typed `RiscvCore::new` route appears only in the labeled-adapter
//! rows, which pin its documented non-conformance (no committed-write
//! visibility).

use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::RiscVSimulator;
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AccessCategory, AtomicAccessKind, AtomicBackend, AtomicBackendResult, AtomicRequest,
    AtomicRequestDescriptor, NativeRamBackend, PhysicalBackend, PhysicalBackendResult,
    PhysicalRequest, PhysicalTargetRejectionReason, PhysicalWidth, ValidatedPhysicalAccess,
};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

// ---------------------------------------------------------------------------
// Encoders.
// ---------------------------------------------------------------------------

fn amo_raw(funct5: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn lr_w(rd: u8, rs1: u8) -> u32 {
    amo_raw(0b00010, 0b010, rd, rs1, 0)
}

fn lr_d(rd: u8, rs1: u8) -> u32 {
    amo_raw(0b00010, 0b011, rd, rs1, 0)
}

fn sc_w(rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(0b00011, 0b010, rd, rs1, rs2)
}

fn sc_d(rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(0b00011, 0b011, rd, rs1, rs2)
}

fn amo_w(funct5: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(funct5, 0b010, rd, rs1, rs2)
}

fn amo_d(funct5: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(funct5, 0b011, rd, rs1, rs2)
}

fn sd(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn fsd(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x27
}

fn nop() -> u32 {
    0x0000_0013
}

// ---------------------------------------------------------------------------
// Harnesses.
// ---------------------------------------------------------------------------

fn memory_with_words(words: &[(u64, u32)], size: usize) -> Arc<Mutex<SimpleMemory>> {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    {
        let mut guard = memory.lock().unwrap();
        for &(address, word) in words {
            guard.write_word(address, word).unwrap();
        }
    }
    memory
}

fn retired(core: &mut RiscvCore) {
    let outcome = core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "expected a retired instruction, got {outcome:?}"
    );
}

/// A data-port backend that forwards every request to RAM while recording
/// each atomic envelope and each ordinary raw request it serves.
struct SpyBackend {
    inner: NativeRamBackend,
    atomics: Arc<Mutex<Vec<AtomicRequestDescriptor>>>,
    raws: Arc<Mutex<Vec<(AccessCategory, u64)>>>,
}

impl SpyBackend {
    fn new(memory: Arc<Mutex<SimpleMemory>>, size: usize) -> (Self, Spy) {
        let spy = Spy {
            atomics: Arc::new(Mutex::new(Vec::new())),
            raws: Arc::new(Mutex::new(Vec::new())),
        };
        (
            Self {
                inner: NativeRamBackend::new(memory, 0, size),
                atomics: spy.atomics.clone(),
                raws: spy.raws.clone(),
            },
            spy,
        )
    }
}

struct Spy {
    atomics: Arc<Mutex<Vec<AtomicRequestDescriptor>>>,
    raws: Arc<Mutex<Vec<(AccessCategory, u64)>>>,
}

impl Spy {
    fn atomic_kinds(&self) -> Vec<AtomicAccessKind> {
        self.atomics
            .lock()
            .unwrap()
            .iter()
            .map(|descriptor| descriptor.kind)
            .collect()
    }

    fn atomic_count(&self) -> usize {
        self.atomics.lock().unwrap().len()
    }

    fn atomic_paddrs(&self) -> Vec<u64> {
        self.atomics
            .lock()
            .unwrap()
            .iter()
            .map(|descriptor| descriptor.paddr)
            .collect()
    }

    fn data_write_count(&self) -> usize {
        self.raws
            .lock()
            .unwrap()
            .iter()
            .filter(|(category, _)| *category == AccessCategory::DataWrite)
            .count()
    }
}

impl PhysicalBackend for SpyBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.raws
            .lock()
            .unwrap()
            .push((request.category(), request.paddr()));
        self.inner.transact(request)
    }
}

impl AtomicBackend for SpyBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.atomics.lock().unwrap().push(request.descriptor());
        self.inner.transact_atomic(request)
    }
}

/// A core whose data port is the spy backend over shared RAM.
fn spy_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>, Spy) {
    let memory = memory_with_words(words, size);
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let (backend, spy) = SpyBackend::new(memory.clone(), size);
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(backend)));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory, spy)
}

/// A data backend that target-rejects only store-conditional envelopes;
/// every other request forwards to RAM.  It exists so a *covered* SC can be
/// driven into a store-AMO access fault (cause 7).
struct ScRejectBackend {
    inner: NativeRamBackend,
}

impl PhysicalBackend for ScRejectBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.inner.transact(request)
    }
}

impl AtomicBackend for ScRejectBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        if request.descriptor().kind == AtomicAccessKind::StoreConditional {
            return Err(ruscv_sim::physical::PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                "store-conditional envelope rejected by the target",
            ));
        }
        self.inner.transact_atomic(request)
    }
}

fn sc_reject_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(ScRejectBackend {
        inner: NativeRamBackend::new(memory.clone(), 0, size),
    })));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory)
}

/// A backend whose every access — ordinary and atomic — serializes through
/// an extra domain mutex it shares with a competing writer thread.  It
/// models the contract boundary precisely: a writer is excluded only while
/// an envelope executes, and can interpose between Hart steps.
struct DomainGatedBackend {
    inner: NativeRamBackend,
    domain: Arc<Mutex<()>>,
}

impl PhysicalBackend for DomainGatedBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let _domain = self.domain.lock().unwrap();
        self.inner.transact(request)
    }
}

impl AtomicBackend for DomainGatedBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        let _domain = self.domain.lock().unwrap();
        self.inner.transact_atomic(request)
    }
}

fn gated_core(
    words: &[(u64, u32)],
    size: usize,
) -> (RiscvCore, Arc<Mutex<SimpleMemory>>, Arc<Mutex<()>>) {
    let memory = memory_with_words(words, size);
    let domain = Arc::new(Mutex::new(()));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        DomainGatedBackend {
            inner: NativeRamBackend::new(memory.clone(), 0, size),
            domain: domain.clone(),
        },
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory, domain)
}

fn set_mtvec(core: &mut RiscvCore, address: u64) {
    core.state_mut().csr.write(machine::MTVEC, address).unwrap();
}

// ---------------------------------------------------------------------------
// The §2/§5.3 operation matrix on the envelope route.
// ---------------------------------------------------------------------------

/// Every RMW funct5 at both widths: one envelope per instruction, Hart-owned
/// arithmetic, correct rd and memory at the encoded width.
#[test]
fn rmw_matrix_issues_one_envelope_with_hart_arithmetic() {
    let old_w: u32 = 10;
    let old_d: u64 = 0x0000_0001_8000_0002;
    let operand: u64 = 6;

    // (funct5, expected W memory, expected D memory)
    let rows: [(u8, u32, u64); 9] = [
        (0b00000, 16, old_d + operand),        // AMOADD
        (0b00001, 6, operand),                 // AMOSWAP
        (0b00100, old_w ^ 6, old_d ^ operand), // AMOXOR
        (0b01000, old_w | 6, old_d | operand), // AMOOR
        (0b01100, old_w & 6, old_d & operand), // AMOAND
        (
            0b10000,
            (old_w as i32).min(6) as u32,
            (old_d as i64).min(operand as i64) as u64,
        ), // AMOMIN
        (
            0b10100,
            (old_w as i32).max(6) as u32,
            (old_d as i64).max(operand as i64) as u64,
        ), // AMOMAX
        (0b11000, old_w.min(6), old_d.min(operand)), // AMOMINU
        (0b11100, old_w.max(6), old_d.max(operand)), // AMOMAXU
    ];

    for (funct5, want_w, want_d) in rows {
        for (encode, expected_memory, expected_rd, width) in [
            (
                amo_w(funct5, 3, 1, 2),
                want_w as u64,
                old_w as i32 as i64 as u64,
                PhysicalWidth::Word,
            ),
            (
                amo_d(funct5, 3, 1, 2),
                want_d,
                old_d,
                PhysicalWidth::Doubleword,
            ),
        ] {
            let (mut core, memory, spy) = spy_core(&[(0, encode), (4, nop())], 0x100);
            memory.lock().unwrap().write_dword(0x80, old_d).unwrap();
            if width == PhysicalWidth::Word {
                memory.lock().unwrap().write_word(0x80, old_w).unwrap();
            }
            core.state_mut().regs[1] = 0x80;
            core.state_mut().regs[2] = operand;
            retired(&mut core);
            assert_eq!(
                core.state().regs[3],
                expected_rd,
                "funct5={funct5:05b}/{width:?}: rd must be the old value"
            );
            match width {
                PhysicalWidth::Word => assert_eq!(
                    memory.lock().unwrap().read_word(0x80).unwrap(),
                    expected_memory as u32,
                    "funct5={funct5:05b}/W: memory"
                ),
                PhysicalWidth::Doubleword => assert_eq!(
                    memory.lock().unwrap().read_dword(0x80).unwrap(),
                    expected_memory,
                    "funct5={funct5:05b}/D: memory"
                ),
                _ => unreachable!(),
            }
            let kinds = spy.atomic_kinds();
            assert_eq!(
                kinds,
                [AtomicAccessKind::Rmw],
                "funct5={funct5:05b}/{width:?}: exactly one RMW envelope"
            );
            assert_eq!(
                spy.atomic_paddrs(),
                vec![0x80],
                "funct5={funct5:05b}/{width:?}: the envelope issued the \
                 guest address after the single base conversion"
            );
        }
    }
}

/// LR and SC issue one envelope each; the envelope kind, span, and paddr
/// match the encoding, and aq/rl bits are legal in all four combinations
/// with no behavioral change in this profile.
#[test]
fn lr_sc_issue_one_envelope_each_and_aq_rl_are_informational() {
    const P: u64 = 0x80;
    for aq in [0u8, 1] {
        for rl in [0u8, 1] {
            let lr = lr_d(3, 1) | ((aq as u32) << 26) | ((rl as u32) << 25);
            let sc = sc_d(4, 1, 2) | ((aq as u32) << 26) | ((rl as u32) << 25);
            let (mut core, memory, spy) = spy_core(&[(0, lr), (4, sc)], 0x100);
            memory.lock().unwrap().write_dword(P, 0x1234).unwrap();
            core.state_mut().regs[1] = P;
            core.state_mut().regs[2] = 0xbeef;
            retired(&mut core);
            retired(&mut core);
            assert_eq!(core.state().regs[3], 0x1234, "aq={aq} rl={rl}: LR read");
            assert_eq!(core.state().regs[4], 0, "aq={aq} rl={rl}: SC succeeded");
            assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0xbeef);
            let kinds = spy.atomic_kinds();
            assert_eq!(
                kinds,
                [
                    AtomicAccessKind::LoadReserved,
                    AtomicAccessKind::StoreConditional
                ],
                "aq={aq} rl={rl}: exactly one envelope each"
            );
        }
    }
}

/// Every Hart-side rejection issues zero physical requests: reserved
/// funct5 values, LR with rs2 != 0, bad funct3, and an SC span the
/// reservation does not cover.
#[test]
fn hart_side_rejections_issue_zero_requests() {
    const P: u64 = 0x80;

    // Reserved funct5 values → illegal instruction, no envelope.
    for funct5 in [0b00101u8, 0b00110, 0b00111, 0b10001, 0b10101, 0b11111] {
        let (mut core, _memory, spy) = spy_core(&[(0, amo_w(funct5, 3, 1, 2))], 0x100);
        core.state_mut().regs[1] = P;
        core.state_mut().regs[2] = 1;
        set_mtvec(&mut core, 0x40);
        let outcome = core.step_outcome();
        assert!(
            matches!(
                outcome,
                StepOutcome::TrapEntered(trap)
                    if trap.cause == ExceptionCause::IllegalInstruction
            ),
            "funct5={funct5:05b} must trap illegal-instruction"
        );
        assert_eq!(
            spy.atomic_count(),
            0,
            "funct5={funct5:05b} issued a request"
        );
        assert_eq!(spy.data_write_count(), 0);
    }

    // LR with rs2 != 0 → illegal instruction, no envelope.
    let (mut core, _memory, spy) = spy_core(&[(0, amo_raw(0b00010, 0b010, 3, 1, 2))], 0x100);
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 1;
    set_mtvec(&mut core, 0x40);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap) if trap.cause == ExceptionCause::IllegalInstruction
    ));
    assert_eq!(spy.atomic_count(), 0);

    // funct3 outside {010, 011} → illegal instruction, no envelope.
    let (mut core, _memory, spy) = spy_core(&[(0, amo_raw(0b00001, 0b001, 3, 1, 2))], 0x100);
    core.state_mut().regs[1] = P;
    set_mtvec(&mut core, 0x40);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap) if trap.cause == ExceptionCause::IllegalInstruction
    ));
    assert_eq!(spy.atomic_count(), 0);

    // SC with no reservation → rd = 1, no envelope, no write.
    let (mut core, memory, spy) = spy_core(&[(0, sc_w(4, 1, 2))], 0x100);
    memory.lock().unwrap().write_dword(P, 5).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 9;
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 5);
    assert_eq!(spy.atomic_count(), 0);

    // LR.W reserves four bytes; SC.D's wider span is uncovered → rd = 1,
    // and the SC issued no envelope (only the LR's one).
    let (mut core, memory, spy) = spy_core(&[(0, lr_w(3, 1)), (4, sc_d(4, 1, 2))], 0x100);
    memory.lock().unwrap().write_dword(P, 5).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 9;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 5);
    assert_eq!(spy.atomic_kinds(), [AtomicAccessKind::LoadReserved]);
}

// ---------------------------------------------------------------------------
// Per-Hart reservation lifecycle.
// ---------------------------------------------------------------------------

/// The reservation record lives in `CoreState`: a successful LR installs the
/// issued span plus the committed-write snapshot; an executed SC consumes
/// it; a faulting LR preserves the prior record.
#[test]
fn reservation_record_lifecycle_is_hart_owned() {
    const P: u64 = 0x80;
    let (mut core, memory, _spy) = spy_core(
        &[(0, lr_d(3, 1)), (4, sc_d(4, 1, 2)), (8, lr_d(5, 1))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 7).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 8;

    assert!(core.state().reservation.is_none());
    retired(&mut core);
    let record = core
        .state()
        .reservation
        .clone()
        .expect("a successful LR installs the per-Hart reservation");
    assert_eq!(record.reserved.paddr, P);
    assert_eq!(record.reserved.width, PhysicalWidth::Doubleword);
    assert!(
        record.snapshot.is_some(),
        "the envelope route records the committed-write snapshot"
    );

    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert!(
        core.state().reservation.is_none(),
        "an executed SC consumes the reservation"
    );

    // A second successful LR replaces the record.
    retired(&mut core);
    let record = core.state().reservation.clone().expect("LR replaced");
    assert_eq!(record.reserved.paddr, P);

    // A faulting LR preserves the prior reservation (staged state is
    // discarded on the trap): the out-of-range LR leaves the record intact.
    core.state_mut().regs[1] = 0x800;
    set_mtvec(&mut core, 0x40);
    core.state_mut().pc = 8;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap) if trap.cause == ExceptionCause::LoadAccessFault
    ));
    let record = core
        .state()
        .reservation
        .clone()
        .expect("a faulting LR preserves the prior reservation");
    assert_eq!(record.reserved.paddr, P);
}

/// Two cores sharing storage keep independent reservations: labeled
/// test-object evidence for the per-Hart record (dev-plan §8 T3: the
/// current single-Hart API carries the record in `CoreState`, so two
/// `RiscvCore` instances are the per-Hart isolation probe).
#[test]
fn two_cores_on_one_domain_hold_independent_reservations() {
    const P: u64 = 0x80;
    let (mut first, memory, _spy_a) = spy_core(&[(0, lr_d(3, 1)), (4, sc_d(4, 1, 2))], 0x100);
    let (mut second, _other, _spy_b) = spy_core(
        &[(0, sc_d(4, 1, 2)), (4, lr_d(5, 1)), (8, sc_d(6, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    first.state_mut().regs[1] = P;
    first.state_mut().regs[2] = 7;
    second.state_mut().regs[1] = P;
    second.state_mut().regs[2] = 9;

    // First Hart reserves on its own storage.
    retired(&mut first);
    assert!(first.state().reservation.is_some());
    assert!(second.state().reservation.is_none());

    // The second Hart's SC at the same address fails: reservations are
    // per-Hart, so no record covers its span (and no envelope is issued).
    retired(&mut second);
    assert_eq!(second.state().regs[4], 1);

    // Each Hart's own reservation still governs only its own SC.
    second.state_mut().pc = 4;
    retired(&mut second); // second's LR installs its own record
    second.state_mut().pc = 8;
    retired(&mut second);
    assert_eq!(
        second.state().regs[6],
        0,
        "the second Hart's own LR/SC pair works"
    );

    first.state_mut().pc = 4;
    retired(&mut first);
    assert_eq!(
        first.state().regs[4],
        0,
        "the first Hart's reservation was never shared"
    );
}

// ---------------------------------------------------------------------------
// Writer-visibility contract (W1–W3).
// ---------------------------------------------------------------------------

/// A committed overlapping guest store between LR and SC invalidates the
/// reservation; a committed non-overlapping store does not.  The check is
/// committed writes to the reserved span, never "any write".
#[test]
fn committed_guest_store_overlap_fails_sc_and_non_overlap_succeeds() {
    const P: u64 = 0x80;

    // Overlapping SD at P.
    let (mut core, memory, spy) = spy_core(
        &[(0, lr_d(3, 1)), (4, sd(2, 1, 0)), (8, sc_d(4, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0x55;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x55);
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "the committed overlapping store fails the SC"
    );
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x55);
    assert_eq!(
        spy.atomic_kinds(),
        [
            AtomicAccessKind::LoadReserved,
            AtomicAccessKind::StoreConditional
        ],
        "a covered SC still issues its one envelope; the backend check fails it"
    );

    // Non-overlapping SD at P+0x20.
    let (mut core, memory, _spy) = spy_core(
        &[(0, lr_d(3, 1)), (4, sd(2, 4, 0)), (8, sc_d(4, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0x66;
    core.state_mut().regs[4] = P + 0x20;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(memory.lock().unwrap().read_dword(P + 0x20).unwrap(), 0x66);
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        0,
        "a committed write outside the reserved span leaves SC successful"
    );
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x66);
}

/// A committed overlapping FSD invalidates identically (C17: guest ordinary
/// writes, integer or FP).
#[test]
fn committed_fp_store_overlap_fails_sc() {
    const P: u64 = 0x80;
    let (mut core, memory, _spy) = spy_core(
        &[(0, lr_d(3, 1)), (4, fsd(2, 1, 0)), (8, sc_d(4, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 9;
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(0x44));
    retired(&mut core);
    retired(&mut core);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x44);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x44);
}

/// An AMO whose write side overlaps the reserved span invalidates — the
/// check sees the envelope's committed write exactly like an ordinary
/// store.
#[test]
fn committed_amo_write_overlap_fails_sc() {
    const P: u64 = 0x80;
    let (mut core, memory, spy) = spy_core(
        &[
            (0, lr_d(3, 1)),
            (4, amo_d(0b00001, 5, 1, 2)), // AMOSWAP.D x5, (x1), x2
            (8, sc_d(4, 1, 2)),
        ],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0x77;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[5], 1, "the AMO returned the old value");
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x77);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(
        spy.atomic_kinds(),
        [
            AtomicAccessKind::LoadReserved,
            AtomicAccessKind::Rmw,
            AtomicAccessKind::StoreConditional,
        ],
        "three instructions, three envelopes"
    );
}

/// A *rejected* ordinary store commits nothing, so the reservation survives:
/// target rejection ≠ committed write.
#[test]
fn rejected_store_does_not_invalidate_the_reservation() {
    const P: u64 = 0x80;
    let (mut core, memory, _spy) = spy_core(
        &[(0, lr_d(3, 1)), (4, sd(2, 4, 0)), (8, sc_d(5, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 9).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 10;
    core.state_mut().regs[4] = 0x1000; // outside the RAM: target rejection
    set_mtvec(&mut core, 0x40);
    retired(&mut core);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x1000
    ));
    core.state_mut().pc = 8;
    retired(&mut core);
    assert_eq!(
        core.state().regs[5],
        0,
        "the rejected store committed nothing; the SC succeeds"
    );
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 10);
}

/// Host-side committed writes invalidate by byte overlap only (P2a-precise):
/// a fully committed overlapping write fails the SC, a committed prefix
/// from a partially failed write invalidates iff its committed bytes
/// overlap, and a non-overlapping committed write does not.
#[test]
fn host_write_mem_invalidation_is_committed_byte_precise() {
    const P: u64 = 0xe0;

    // Fully committed overlapping host write ⇒ rd = 1.
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    simulator.write_mem(P, &7u64.to_le_bytes()).unwrap();
    simulator.step().unwrap();
    assert_eq!(simulator.state().regs[4], 1);
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 7u64.to_le_bytes());

    // Committed prefix overlapping ⇒ rd = 1 (P2a: bookkeeping is written
    // bytes, not attempted bytes).
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    // A write that overflows the RAM end commits only its prefix; the
    // prefix covers P..P+8, so the reservation is invalidated (P2a:
    // bookkeeping is written bytes, not attempted bytes).
    simulator.write_mem(P, &[0xaau8; 0x200]).unwrap_err();
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        1,
        "the committed prefix overlapped the reservation"
    );

    // A partially failed write whose committed prefix does NOT overlap
    // leaves the reservation intact.
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    // Commits 0x1f8..0x200 (8 bytes) then fails: no committed byte covers P.
    simulator.write_mem(0x1f8, &[0xaau8; 0x20]).unwrap_err();
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        0,
        "no committed bytes overlapped the reservation"
    );

    // Committed non-overlapping write ⇒ rd = 0.
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    simulator.write_mem(P + 0x20, &9u64.to_le_bytes()).unwrap();
    simulator.step().unwrap();
    assert_eq!(simulator.state().regs[4], 0);
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 6u64.to_le_bytes());
}

/// The host `memory()` handle writes through the same committed-write
/// bookkeeping: an overlapping typed `write_dword` between run segments
/// invalidates the reservation.
#[test]
fn memory_handle_typed_write_between_run_segments_invalidates() {
    const P: u64 = 0xe0;
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    {
        // Between guest-visible steps, the facade's memory() handle is the
        // host's inspection/mutation surface; its committed writes are
        // tracked identically (§5.5).
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_dword(P, 0xdead).unwrap();
    }
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        1,
        "the committed memory() write invalidates the reservation"
    );
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 0xdeadu64.to_le_bytes());
}

/// The run-boundary `tohost` clear is a committed write: when it overlaps
/// the reserved span the resumed SC fails; a budget-exhausted run followed
/// by a resume sees no such commit and the SC succeeds (§5.5 run-boundary
/// visibility).
#[test]
fn run_boundary_writes_and_resume_visibility() {
    const P: u64 = 0x80;

    // Variant A: the exit path's clear_tohost write overlaps the
    // reservation taken inside the same program run — but here the resume
    // scenario uses two run() calls, so first prove a budget-resume SC.
    let mut simulator = RiscVSimulator::new(0x200);
    simulator.set_tohost(0x180); // exit signal lives far from P
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 1).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 9;
    // Budget covers only the LR; the SC waits for the resumed run.
    let first = simulator.run(Some(1)).unwrap();
    assert!(first.timed_out, "the LR consumed the whole first budget");
    let second = simulator.run(Some(1)).unwrap();
    assert!(second.timed_out, "the SC consumed the resumed budget");
    assert_eq!(
        simulator.state().regs[4],
        0,
        "no write landed between run segments; the SC still succeeds"
    );
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 9u64.to_le_bytes());

    // Variant B: a host write between run segments lands inside the
    // boundary and invalidates (the visibility boundary is any committed
    // write, here the facade's own write_mem between run() calls).
    let mut simulator = RiscVSimulator::new(0x200);
    simulator.set_tohost(0x180);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_d(3, 1)).unwrap();
        memory.write_word(4, sc_d(4, 1, 2)).unwrap();
        memory.write_dword(P, 1).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 9;
    simulator.run(Some(1)).unwrap();
    simulator.write_mem(P, &0xabu64.to_le_bytes()).unwrap();
    simulator.run(Some(1)).unwrap();
    assert_eq!(
        simulator.state().regs[4],
        1,
        "a committed write between run segments fails the resumed SC"
    );
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 0xabu64.to_le_bytes());
}

/// A competing writer thread shares the domain mutex with the backend: it
/// can commit between the LR and SC envelopes (SC fails), but when it
/// arrives only after the SC envelope completes, the SC commits (rd = 0)
/// and the writer's later value is the final state.  Exclusion is bounded
/// to each envelope's critical section — there is no cross-step window.
#[test]
fn cross_thread_writer_is_excluded_only_inside_the_envelope() {
    const P: u64 = 0x80;

    // Variant A: the writer commits between LR and SC → SC fails.
    let (mut core, memory, domain) = gated_core(&[(0, lr_d(3, 1)), (4, sc_d(4, 1, 2))], 0x100);
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 7;
    retired(&mut core); // LR committed

    let writer_memory = memory.clone();
    let writer_domain = domain.clone();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = thread::spawn(move || {
        writer_barrier.wait();
        // The domain mutex serializes this write with backend envelopes;
        // between envelopes it wins freely.
        let _domain = writer_domain.lock().unwrap();
        writer_memory.lock().unwrap().write_dword(P, 0x5a).unwrap();
    });
    barrier.wait();
    writer.join().unwrap();

    retired(&mut core); // SC: the committed write invalidates
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 0x5a);

    // Variant B: the writer is gated until after the SC envelope → the SC
    // commits, and the writer's later value is the final memory.
    let (mut core, memory, domain) = gated_core(&[(0, lr_d(3, 1)), (4, sc_d(4, 1, 2))], 0x100);
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 7;
    retired(&mut core);

    let writer_memory = memory.clone();
    let writer_domain = domain.clone();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = thread::spawn(move || {
        writer_barrier.wait();
        let _domain = writer_domain.lock().unwrap();
        writer_memory.lock().unwrap().write_dword(P, 0x6b).unwrap();
    });
    retired(&mut core); // SC commits first (deterministically)
    barrier.wait();
    writer.join().unwrap();
    assert_eq!(
        core.state().regs[4],
        0,
        "the SC committed before the writer"
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P).unwrap(),
        0x6b,
        "the later writer's value is the final state"
    );
}

// ---------------------------------------------------------------------------
// Target rejections and labeled adapter boundaries.
// ---------------------------------------------------------------------------

/// Target rejections map to the access-class faults with the original guest
/// address: LR and RMW → cause 5/7 at an unmapped span, and a covered SC
/// envelope rejected by the backend → cause 7.
#[test]
fn target_rejections_map_to_access_faults_with_guest_mtval() {
    // LR at an unmapped span → load access fault (cause 5).
    let (mut core, _memory, _spy) = spy_core(&[(0, lr_d(3, 1))], 0x100);
    core.state_mut().regs[1] = 0x400;
    set_mtvec(&mut core, 0x40);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::LoadAccessFault && trap.mtval == 0x400
    ));

    // AMO at an unmapped span → store/AMO access fault (cause 7).
    let (mut core, _memory, _spy) = spy_core(&[(0, amo_w(0b00001, 3, 1, 2))], 0x100);
    core.state_mut().regs[1] = 0x400;
    core.state_mut().regs[2] = 1;
    set_mtvec(&mut core, 0x40);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x400
    ));

    // A covered SC envelope rejected by the target → store access fault,
    // and the approved faulting-SC retain keeps the reservation: retrying
    // the same SC faults again (the backend always rejects), never rd=1.
    let (mut core, _memory) = sc_reject_core(
        &[(0, lr_d(3, 1)), (4, sc_d(4, 1, 2)), (8, sc_d(5, 1, 2))],
        0x100,
    );
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 9;
    set_mtvec(&mut core, 0x40);
    retired(&mut core);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x80
    ));
    core.state_mut().pc = 8;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x80
    ));
}

/// `rd = x0` suppresses only the destination write: the RMW still mutates
/// memory, and an SC with `rd = x0` still commits its store.
#[test]
fn rd_x0_suppresses_only_the_destination_write() {
    const P: u64 = 0x80;

    let (mut core, memory, spy) = spy_core(&[(0, amo_w(0b00001, 0, 1, 2)), (4, nop())], 0x100);
    memory.lock().unwrap().write_word(P, 10).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 6;
    retired(&mut core);
    assert_eq!(core.state().regs[0], 0);
    assert_eq!(memory.lock().unwrap().read_word(P).unwrap(), 6);
    assert_eq!(spy.atomic_count(), 1);

    let (mut core, memory, _spy) = spy_core(&[(0, lr_d(0, 1)), (4, sc_d(0, 1, 2))], 0x100);
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 9;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[0], 0);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 9);
}

/// The typed `RiscvCore::new` route is the labeled non-conforming adapter:
/// it shares the per-Hart reservation record but carries no snapshot, so a
/// committed overlapping write through the same typed memory cannot be
/// observed and the SC still succeeds.  This is the documented boundary of
/// the compatibility surface (dev-plan C24/§7.1), asserted explicitly.
#[test]
fn typed_adapter_cannot_observe_committed_writes() {
    const P: u64 = 0x80;
    let memory = memory_with_words(
        &[(0, lr_d(3, 1)), (4, sd(2, 1, 0)), (8, sc_d(4, 1, 2))],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 1).unwrap();
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 9;
    retired(&mut core);
    assert!(core.state().reservation.is_some());
    assert!(
        core.state()
            .reservation
            .as_ref()
            .unwrap()
            .snapshot
            .is_none(),
        "the adapter route records no committed-write snapshot"
    );
    retired(&mut core); // overlapping typed store
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        0,
        "labeled non-conformance: the typed route cannot see the committed \
         write, so its conditional check is span containment only"
    );
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 9);
}
