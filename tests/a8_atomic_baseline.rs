//! A8 executable baseline, rewritten at T3 (dev-plan §8 T3, §9).
//!
//! At T0 this file pinned the *old* column of the A8 §6 compatibility
//! matrix: the mis-targeted `funct5` dispatch, the process-global
//! reservation, and the §3.4 HTIF typed-call outcomes.  T3 flipped every
//! approved row (the ledger `docs/verification/a8-fixture-reclassification.md`
//! maps each old row to its class and new expected value); this file now
//! asserts the new column:
//!
//! 1. The §2/§5.3 dispatch table for all 32 `funct5` values × W/D: real
//!    AMOSWAP, correct min/max/and/or encodings, LR with `rs2 != 0` and all
//!    reserved `funct5` values as illegal-instruction traps before access.
//! 2. Correct SC widths and the C30 span-containment rows.
//! 3. Per-Hart reservation scope: core isolation, reset/reload clearing,
//!    committed-write-aware SC, and faulting-SC retain.
//! 4. The §3.4 HTIF outcomes on the envelope route (approved D-c: LR/SC are
//!    target rejections, one RMW envelope fires the callback exactly once).
//! 5. Ordinary-path context rows that must not have moved.
//!
//! Rows that live on the typed `RiscvCore::new` route are the labeled
//! non-conforming compatibility adapter (C24); rows through
//! `new_with_physical_ports` and the public facades are the standard route.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{ExceptionCause, InstructionRetired, RiscvCore, StepOutcome, TrapEntered};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{RiscVSimulator, SystemBus, SYSTEM_BUS_HTIF_BASE};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, AtomicRequestDescriptor, NativeRamBackend,
    PhysicalBackend, PhysicalBackendResult, PhysicalRequest, ValidatedPhysicalAccess,
};
use std::sync::{Arc, Mutex};

const UART_BASE: u64 = 0x1000_0000;

type SharedCalls = Arc<Mutex<Vec<String>>>;
type SharedCallbacks = Arc<Mutex<Vec<u64>>>;
type SharedBus = Arc<Mutex<SystemBus>>;
type AtomicCalls = Arc<Mutex<Vec<AtomicRequestDescriptor>>>;

// ---------------------------------------------------------------------------
// Encoding helpers (raw words; the decoder and dispatcher are the subject).
// ---------------------------------------------------------------------------

fn amo_raw(funct5: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

/// Architectural LR encoding (`funct5=00010`, `rs2=0`).
fn lr_encoding(rd: u8, rs1: u8, funct3: u8) -> u32 {
    amo_raw(0b00010, funct3, rd, rs1, 0)
}

/// Reserved LR encoding (`funct5=00010`, `rs2!=0`): an illegal-instruction
/// trap under the §2 table.
fn malformed_lr(rd: u8, rs1: u8, rs2: u8, funct3: u8) -> u32 {
    amo_raw(0b00010, funct3, rd, rs1, rs2)
}

/// Architectural SC encoding (`funct5=00011`).
fn sc_encoding(rd: u8, rs1: u8, rs2: u8, funct3: u8) -> u32 {
    amo_raw(0b00011, funct3, rd, rs1, rs2)
}

fn ld(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn nop() -> u32 {
    0x0000_0013
}

// ---------------------------------------------------------------------------
// Small harnesses.
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

/// Typed-only core (the old `RiscvCore::new` compatibility route): fetch and
/// AMO/LR/SC both use the typed handles — the labeled adapter (C24).
fn typed_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    (core, memory)
}

/// Port-configured core over shared RAM: AMO/LR/SC issue one atomic envelope
/// per instruction through the validated data port.
fn port_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory)
}

fn set_mtvec(core: &mut RiscvCore, address: u64) {
    core.state_mut().csr.write(machine::MTVEC, address).unwrap();
}

fn retired(core: &mut RiscvCore) -> InstructionRetired {
    match core.step_outcome() {
        StepOutcome::InstructionRetired(fact) => fact,
        StepOutcome::TrapEntered(trap) => {
            panic!("expected a retired instruction, got trap {:?}", trap.cause)
        }
        StepOutcome::SimulatorFailure(failure) => {
            panic!("expected a retired instruction, got failure {failure:?}")
        }
    }
}

fn trapped(core: &mut RiscvCore) -> TrapEntered {
    match core.step_outcome() {
        StepOutcome::TrapEntered(trap) => trap,
        StepOutcome::InstructionRetired(fact) => {
            panic!("expected a trap, got retired {:?}", fact.instruction)
        }
        StepOutcome::SimulatorFailure(failure) => {
            panic!("expected a trap, got failure {failure:?}")
        }
    }
}

/// A typed legacy view over the real native `SystemBus` that records the
/// exact `MemoryInterface` calls selected by the adapter route.
struct TracedSystemBus {
    bus: SharedBus,
    calls: SharedCalls,
}

impl TracedSystemBus {
    fn record(&self, operation: &str, address: u64) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{operation}@{address:#x}"));
    }
}

impl MemoryInterface for TracedSystemBus {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_dword", addr);
        self.bus.lock().unwrap().read_dword(addr)
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        self.record("read_word", addr);
        self.bus.lock().unwrap().read_word(addr)
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        self.record("read_half", addr);
        self.bus.lock().unwrap().read_half(addr)
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        self.record("read_byte", addr);
        self.bus.lock().unwrap().read_byte(addr)
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_word_zext", addr);
        self.bus.lock().unwrap().read_word_zext(addr)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_half_zext", addr);
        self.bus.lock().unwrap().read_half_zext(addr)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_zext", addr);
        self.bus.lock().unwrap().read_byte_zext(addr)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_word_sext", addr);
        self.bus.lock().unwrap().read_word_sext(addr)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_half_sext", addr);
        self.bus.lock().unwrap().read_half_sext(addr)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_sext", addr);
        self.bus.lock().unwrap().read_byte_sext(addr)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.record("write_dword", addr);
        self.bus.lock().unwrap().write_dword(addr, value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.record("write_word", addr);
        self.bus.lock().unwrap().write_word(addr, value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.record("write_half", addr);
        self.bus.lock().unwrap().write_half(addr, value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.record("write_byte", addr);
        self.bus.lock().unwrap().write_byte(addr, value)
    }

    fn size(&self) -> usize {
        self.bus.lock().unwrap().size()
    }
}

/// A physical/atomic backend double over the real `SystemBus` that records
/// every raw request and every atomic envelope it serves.
struct TracedBusBackend {
    bus: SharedBus,
    atomic_calls: AtomicCalls,
}

impl PhysicalBackend for TracedBusBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.bus.lock().unwrap().transact(request)
    }
}

impl AtomicBackend for TracedBusBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.atomic_calls.lock().unwrap().push(request.descriptor());
        self.bus.lock().unwrap().transact_atomic(request)
    }
}

/// Standard-facade wiring over the real native bus: raw fetch port over the
/// instruction RAM, validated data port over a bus backend that records
/// atomic envelopes, plus the traced typed view as the labeled adapter.
fn htif_core(
    program: &[u32],
) -> (
    RiscvCore,
    SharedCalls,
    AtomicCalls,
    SharedCallbacks,
    SharedBus,
) {
    let instruction = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    {
        let mut guard = instruction.lock().unwrap();
        for (index, word) in program.iter().copied().enumerate() {
            guard.write_word(index as u64 * 4, word).unwrap();
        }
    }
    let callbacks: SharedCallbacks = Arc::new(Mutex::new(Vec::new()));
    let mut bus_value = SystemBus::new(
        Arc::new(Mutex::new(SimpleMemory::new(0))),
        Arc::new(Mutex::new(Uart16550::new(UART_BASE))),
        0,
        0,
    );
    {
        let sink = callbacks.clone();
        bus_value.set_htif_write_callback(move |value| sink.lock().unwrap().push(value));
    }
    let bus = Arc::new(Mutex::new(bus_value));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let atomic_calls: AtomicCalls = Arc::new(Mutex::new(Vec::new()));
    let data_typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
        Arc::new(Mutex::new(TracedSystemBus {
            bus: bus.clone(),
            calls: calls.clone(),
        }));
    let instruction_typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = instruction.clone();
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(instruction.clone(), 0, 0x40),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(TracedBusBackend {
        bus: bus.clone(),
        atomic_calls: atomic_calls.clone(),
    })));
    let mut core = RiscvCore::new_with_physical_ports(
        instruction_typed,
        data_typed,
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, calls, atomic_calls, callbacks, bus)
}

// ---------------------------------------------------------------------------
// Group 1: the §2/§5.3 funct5 dispatch table (all 32 values × W/D).
// ---------------------------------------------------------------------------

/// What the approved dispatcher does with one `funct5` value under `rs2 != 0`.
enum DispatchExpectation {
    /// A real RMW retires: W writes `w` at the word span and retires the
    /// sign-extended old word; D writes `d` at the dword span and retires
    /// the full old dword.
    Retire { w: u32, d: u64, note: &'static str },
    /// SC with no reservation retires `rd=1` and performs no access.
    ScNoReservation,
    /// A reserved encoding traps illegal-instruction before any access:
    /// every unassigned `funct5` and `funct5=00010` with `rs2 != 0` (C6).
    IllegalTrap,
}

/// Initial memory contents: word value 10, operand 6.
const OLD_WORD: u32 = 10;
const OPERAND: u64 = 6;

fn expected_dispatch(funct5: u8) -> DispatchExpectation {
    let (old, operand) = (OLD_WORD, OPERAND as u32);
    let (old_d, operand_d) = (OLD_WORD as u64, OPERAND);
    let rmw = |w: u32, d: u64, note: &'static str| DispatchExpectation::Retire { w, d, note };
    match funct5 {
        0b00000 => rmw(
            old.wrapping_add(operand),
            old_d.wrapping_add(operand_d),
            "AMOADD",
        ),
        // Real AMOSWAP: memory receives the operand, rd the old value (C2).
        0b00001 => rmw(operand, operand_d, "AMOSWAP"),
        // LR with rs2 != 0 is a reserved encoding, not an SC fallback (§2).
        0b00010 => DispatchExpectation::IllegalTrap,
        0b00011 => DispatchExpectation::ScNoReservation,
        0b00100 => rmw(old ^ operand, old_d ^ operand_d, "AMOXOR"),
        // Spec AMOOR at its real encoding (was AMOMIN at the baseline).
        0b01000 => rmw(old | operand, old_d | operand_d, "AMOOR"),
        // Spec AMOAND at its real encoding.
        0b01100 => rmw(old & operand, old_d & operand_d, "AMOAND"),
        0b10000 => rmw(
            (old as i32).min(operand as i32) as u32,
            (old_d as i64).min(operand_d as i64) as u64,
            "AMOMIN",
        ),
        0b10100 => rmw(
            (old as i32).max(operand as i32) as u32,
            (old_d as i64).max(operand_d as i64) as u64,
            "AMOMAX",
        ),
        // Spec AMOMINU/AMOMAXU are now reachable at their real encodings (C4).
        0b11000 => rmw(old.min(operand), old_d.min(operand_d), "AMOMINU"),
        0b11100 => rmw(old.max(operand), old_d.max(operand_d), "AMOMAXU"),
        // Every other funct5 is a reserved encoding: an illegal-instruction
        // trap, never a simulator failure (C6).
        _ => DispatchExpectation::IllegalTrap,
    }
}

use DispatchExpectation::{IllegalTrap, Retire, ScNoReservation};

#[test]
fn dispatch_matrix_pins_all_32_funct5_values() {
    for funct5 in 0u8..=31 {
        for funct3 in [0b010u8, 0b011] {
            let encoding = amo_raw(funct5, funct3, 3, 1, 2);
            let (mut core, memory) = typed_core(&[(0, encoding), (4, nop())], 0x100);
            core.state_mut().regs[1] = 0x80;
            core.state_mut().regs[2] = OPERAND;
            memory
                .lock()
                .unwrap()
                .write_dword(0x80, OLD_WORD as u64)
                .unwrap();
            set_mtvec(&mut core, 0x40);

            match expected_dispatch(funct5) {
                Retire { w, d, note } => {
                    let fact = retired(&mut core);
                    assert_eq!(
                        fact.instruction, encoding,
                        "funct5={funct5:05b}/{funct3:03b}"
                    );
                    if funct3 == 0b010 {
                        assert_eq!(
                            core.state().regs[3],
                            OLD_WORD as i32 as i64 as u64,
                            "funct5={funct5:05b}/W ({note}): rd is the sign-extended old word"
                        );
                        assert_eq!(
                            memory.lock().unwrap().read_word(0x80).unwrap(),
                            w,
                            "funct5={funct5:05b}/W ({note}): memory word"
                        );
                    } else {
                        assert_eq!(
                            core.state().regs[3],
                            OLD_WORD as u64,
                            "funct5={funct5:05b}/D ({note}): rd is the full old dword"
                        );
                        assert_eq!(
                            memory.lock().unwrap().read_dword(0x80).unwrap(),
                            d,
                            "funct5={funct5:05b}/D ({note}): memory dword"
                        );
                    }
                    assert_eq!(
                        core.state().pc,
                        4,
                        "funct5={funct5:05b}/{funct3:03b} ({note}): pc must advance"
                    );
                }
                ScNoReservation => {
                    let fact = retired(&mut core);
                    assert_eq!(
                        fact.instruction, encoding,
                        "funct5={funct5:05b}/{funct3:03b}"
                    );
                    assert_eq!(
                        core.state().regs[3],
                        1,
                        "funct5={funct5:05b}/{funct3:03b}: SC without a reservation \
                         must retire rd=1"
                    );
                    assert_eq!(
                        memory.lock().unwrap().read_dword(0x80).unwrap(),
                        OLD_WORD as u64,
                        "funct5={funct5:05b}/{funct3:03b}: failed SC must not write"
                    );
                }
                IllegalTrap => {
                    let trap = trapped(&mut core);
                    assert_eq!(
                        trap.cause,
                        ExceptionCause::IllegalInstruction,
                        "funct5={funct5:05b}/{funct3:03b}: reserved encodings are \
                         illegal-instruction traps, not simulator failures"
                    );
                    assert_eq!(trap.mtval, encoding as u64);
                    assert_eq!(
                        memory.lock().unwrap().read_dword(0x80).unwrap(),
                        OLD_WORD as u64,
                        "funct5={funct5:05b}/{funct3:03b}: the trap must precede \
                         any memory access"
                    );
                }
            }
        }
    }
}

/// Two operand pairs discriminate signed and unsigned min/max at their real
/// encodings, and or/and spot-checks pin the correct bit operations.  The
/// old value is the full storage width for each encoding (the mixed pair
/// uses a true signed -10), and assertions read the low result word.
#[test]
fn min_max_and_bitwise_operations_use_their_real_encodings() {
    let observe = |funct5: u8, funct3: u8, old: u64, operand: u64| {
        let encoding = amo_raw(funct5, funct3, 3, 1, 2);
        let (mut core, memory) = typed_core(&[(0, encoding)], 0x100);
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[2] = operand;
        memory.lock().unwrap().write_dword(0x80, old).unwrap();
        let fact = retired(&mut core);
        assert_eq!(fact.instruction, encoding);
        let result = memory.lock().unwrap().read_word(0x80).unwrap();
        result
    };

    for (funct3, mixed, same) in [
        // W rows: old = 0xFFFF_FFF6 is -10 as i32.
        (0b010u8, 0xFFFF_FFF6u64, 10u64),
        // D rows: old must be a real signed -10 across the full width.
        (0b011u8, 0xFFFF_FFFF_FFFF_FFF6u64, 10u64),
    ] {
        let operand = 6u64;
        // 01000 is AMOOR at its real encoding (no longer AMOMIN).
        assert_eq!(
            observe(0b01000, funct3, mixed, operand),
            (mixed | operand) as u32,
            "funct3={funct3:03b}: 01000 must execute OR"
        );
        // 01100 is AMOAND at its real encoding.
        assert_eq!(
            observe(0b01100, funct3, mixed, operand),
            (mixed & operand) as u32,
            "funct3={funct3:03b}: 01100 must execute AND"
        );
        // 10000 is signed MIN: the mixed pair keeps the negative operand.
        assert_eq!(
            observe(0b10000, funct3, mixed, operand),
            0xFFFF_FFF6,
            "funct3={funct3:03b}: 10000 must execute signed min"
        );
        // 10100 is signed MAX.
        assert_eq!(
            observe(0b10100, funct3, mixed, operand),
            6,
            "funct3={funct3:03b}: 10100 must execute signed max"
        );
        assert_eq!(
            observe(0b10100, funct3, same, operand),
            10,
            "funct3={funct3:03b}: 10100 must execute max, not min"
        );
        // 11000 is unsigned MIN (reachable now): the mixed pair yields 6.
        assert_eq!(
            observe(0b11000, funct3, mixed, operand),
            6,
            "funct3={funct3:03b}: 11000 must execute unsigned min"
        );
        // 11100 is unsigned MAX (reachable now): the mixed pair yields the
        // bit-pattern-max whose low word is 0xFFFF_FFF6.
        assert_eq!(
            observe(0b11100, funct3, mixed, operand),
            0xFFFF_FFF6,
            "funct3={funct3:03b}: 11100 must execute unsigned max"
        );
        assert_eq!(
            observe(0b11100, funct3, same, operand),
            10,
            "funct3={funct3:03b}: 11100 must execute max, not min"
        );
    }
}

/// The real AMOSWAP encoding (`funct5=00001`) now swaps: W writes the low
/// word of `rs2`, D writes the full `rs2`; `rd` receives the old value.
#[test]
fn amoswap_encoding_executes_a_real_swap() {
    let (mut core, memory) = typed_core(&[(0, amo_raw(0b00001, 0b010, 3, 1, 2))], 0x100);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 6;
    memory.lock().unwrap().write_word(0x80, 10).unwrap();
    retired(&mut core);
    assert_eq!(core.state().regs[3], 10);
    assert_eq!(
        memory.lock().unwrap().read_word(0x80).unwrap(),
        6,
        "AMOSWAP.W writes rs2's low word, not old + rs2"
    );

    let (mut core, memory) = typed_core(&[(0, amo_raw(0b00001, 0b011, 3, 1, 2))], 0x100);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    memory
        .lock()
        .unwrap()
        .write_dword(0x80, 0x0000_0001_8000_0002)
        .unwrap();
    retired(&mut core);
    assert_eq!(core.state().regs[3], 0x0000_0001_8000_0002);
    assert_eq!(
        memory.lock().unwrap().read_dword(0x80).unwrap(),
        0x1234_5678_9abc_def0,
        "AMOSWAP.D writes the full rs2"
    );
}

/// AMO encodings with `funct3` outside {010, 011} remain illegal-instruction
/// traps before any access.
#[test]
fn amo_funct3_outside_w_d_is_an_illegal_instruction_trap() {
    let encoding = amo_raw(0b00001, 0b000, 3, 1, 2);
    let (mut core, _memory) = typed_core(&[(0, encoding)], 0x100);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 6;
    set_mtvec(&mut core, 0x40);

    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::IllegalInstruction);
    assert_eq!(trap.mtval, encoding as u64);
}

/// `funct5=00010` with `rs2 != 0` is a reserved encoding: an
/// illegal-instruction trap at both widths, never an SC execution.
#[test]
fn lr_with_nonzero_rs2_is_an_illegal_instruction_trap() {
    for funct3 in [0b010u8, 0b011] {
        let encoding = malformed_lr(4, 1, 2, funct3);
        let (mut core, memory) =
            typed_core(&[(0, lr_encoding(3, 1, funct3)), (4, encoding)], 0x100);
        memory.lock().unwrap().write_dword(0x80, 5).unwrap();
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[2] = 9;
        retired(&mut core);
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(trap.cause, ExceptionCause::IllegalInstruction);
        assert_eq!(trap.mtval, encoding as u64);
        assert_eq!(
            memory.lock().unwrap().read_dword(0x80).unwrap(),
            5,
            "the reserved encoding must not write even with a live reservation"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 2: correct SC widths and the C30 span-containment rows.
// ---------------------------------------------------------------------------

/// With a live reservation the SC width follows `funct3`: SC.W writes four
/// bytes, SC.D writes eight — the old dword fallback is gone.
#[test]
fn sc_widths_follow_the_funct3_encoding() {
    const P: u64 = 0x80;
    let value = 0x1234_5678_9abc_def0u64;

    // SC.W writes only the low word; the upper word keeps its bytes.
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    memory
        .lock()
        .unwrap()
        .write_dword(P, 0x0000_00ff_0000_0000)
        .unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(
        memory.lock().unwrap().read_word(P).unwrap(),
        0x9abc_def0,
        "SC.W writes four bytes"
    );
    assert_eq!(
        memory.lock().unwrap().read_word(P + 4).unwrap(),
        0x0000_00ff,
        "SC.W leaves the upper word untouched"
    );

    // SC.D writes the complete dword.
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b011)),
            (4, sc_encoding(4, 1, 2, 0b011)),
        ],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 0).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), value);
}

/// C30 new column (§5.4): the SC span must be contained in the reserved
/// span, so `LR.W@p→SC.D@p` fails and `LR.D@p→SC.W@(p+4)` succeeds — the
/// exact inverse of the old width-less exact-address key.
#[test]
fn cross_width_sc_rows_apply_span_containment() {
    const P: u64 = 0x80;

    // LR.W reserves four bytes; the SC.D span is not contained → rd = 1 and
    // no write, and the reservation is consumed.
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b011)),
        ],
        0x200,
    );
    memory
        .lock()
        .unwrap()
        .write_dword(P, 0x0000_0000_8000_0001)
        .unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0xdead_beef_cafe_f00d;
    retired(&mut core);
    // LR.W now performs the real 32-bit sign-extended read.
    assert_eq!(
        core.state().regs[3],
        0xFFFF_FFFF_8000_0001,
        "LR.W sign-extends its word"
    );
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "C30: an 8-byte SC cannot fit a 4-byte reservation"
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P).unwrap(),
        0x0000_0000_8000_0001,
        "the uncovered SC writes nothing"
    );

    // LR.D reserves eight bytes; SC.W at p+4 is contained → rd = 0.
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b011)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x200,
    );
    memory
        .lock()
        .unwrap()
        .write_dword(P, 0x0000_0000_8000_0001)
        .unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0x66;
    retired(&mut core);
    // LR.D reads the full dword.
    assert_eq!(
        core.state().regs[3],
        0x0000_0000_8000_0001,
        "LR.D returns the full dword"
    );
    core.state_mut().regs[1] = P + 4;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        0,
        "C30: a 4-byte SC inside the reserved dword succeeds"
    );
    assert_eq!(memory.lock().unwrap().read_word(P + 4).unwrap(), 0x66);
    assert_eq!(
        memory.lock().unwrap().read_word(P).unwrap(),
        0x8000_0001,
        "the low word keeps its bytes"
    );
}

// ---------------------------------------------------------------------------
// Group 3: per-Hart reservation scope (the retired global singleton).
// ---------------------------------------------------------------------------

#[test]
fn per_hart_reservation_isolates_cores_and_reset_clears() {
    const P: u64 = 0x90;

    // LR in one core, SC in a different core at the same address: the
    // reservation is Hart-owned, so the second core fails rd = 1.
    let (mut first, first_memory) = port_core(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    first_memory.lock().unwrap().write_dword(P, 7).unwrap();
    first.state_mut().regs[1] = P;
    retired(&mut first);

    let (mut second, second_memory) = port_core(&[(0, sc_encoding(4, 1, 2, 0b010))], 0x200);
    second_memory.lock().unwrap().write_dword(P, 7).unwrap();
    second.state_mut().regs[1] = P;
    second.state_mut().regs[2] = 8;
    retired(&mut second);
    assert_eq!(
        second.state().regs[4],
        1,
        "a second Hart does not share the first Hart's reservation"
    );
    assert_eq!(second_memory.lock().unwrap().read_dword(P).unwrap(), 7);

    // Reset installs a fresh CoreState and clears the reservation.
    let (mut core, memory) = port_core(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    memory.lock().unwrap().write_dword(P, 11).unwrap();
    core.state_mut().regs[1] = P;
    retired(&mut core);
    memory
        .lock()
        .unwrap()
        .write_word(0, sc_encoding(4, 1, 2, 0b010))
        .unwrap();
    core.reset(0, 0);
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 12;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "reset clears the per-Hart reservation"
    );
    assert_eq!(memory.lock().unwrap().read_dword(P).unwrap(), 11);

    // The key is the issued span, not a granule: an SC at a different
    // address fails (unchanged observable).
    let (mut core, memory) = port_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x200,
    );
    memory.lock().unwrap().write_dword(0xb0, 21).unwrap();
    memory.lock().unwrap().write_dword(0xb8, 31).unwrap();
    core.state_mut().regs[1] = 0xb0;
    retired(&mut core);
    core.state_mut().regs[1] = 0xb8;
    core.state_mut().regs[2] = 32;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "an SC outside the reserved span fails"
    );
    assert_eq!(memory.lock().unwrap().read_dword(0xb8).unwrap(), 31);
}

#[test]
fn flat_image_replacement_clears_the_reservation() {
    let first = fixture::elf_with_code(&[lr_encoding(3, 1, 0b010)], 0, false, false, 0x3000);
    let second = fixture::elf_with_code(&[sc_encoding(4, 1, 2, 0b010)], 0, false, false, 0x3000);
    let mut simulator = RiscVSimulator::new(0x100);
    simulator.load_elf(&first).unwrap();
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_dword(0x80, 1).unwrap();
    }
    simulator.state_mut().regs[1] = fixture::BASE + 0x80;
    simulator.state_mut().regs[2] = 22;
    simulator.step().unwrap();

    // The reload replaces the core (and its CoreState): the reservation is
    // gone, so the SC fails with rd = 1 (flipped from the global key).
    simulator.load_elf(&second).unwrap();
    simulator.state_mut().regs[1] = fixture::BASE + 0x80;
    simulator.state_mut().regs[2] = 22;
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        1,
        "the flat reload must clear the per-Hart reservation"
    );
}

/// Approved P2a-precise: a committed overlapping host `write_mem` bumps the
/// committed-write sequence, so the following SC fails `rd = 1`.
#[test]
fn host_write_mem_committed_overlap_invalidates_the_reservation() {
    const P: u64 = 0xe0;
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_encoding(3, 1, 0b010)).unwrap();
        memory.write_word(4, sc_encoding(4, 1, 2, 0b010)).unwrap();
        memory.write_dword(P, 5).unwrap();
    }
    simulator.state_mut().regs[1] = P;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 5u64.to_le_bytes());

    simulator.write_mem(P, &7u64.to_le_bytes()).unwrap();
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        1,
        "a committed overlapping host write invalidates the reservation"
    );
    assert_eq!(
        simulator.read_mem(P, 8).unwrap(),
        7u64.to_le_bytes(),
        "the SC wrote nothing; the host value remains"
    );
}

/// A write-faulting SC traps and the Hart retains the reservation (approved
/// profile rule C15): a retried SC on the same Hart commits, while a
/// different Hart's SC fails `rd = 1`.
#[test]
fn write_faulting_sc_retains_the_reservation_for_a_later_sc() {
    use ruscv_sim::memory::MemoryError;

    struct FailingWriteMemory {
        inner: SimpleMemory,
        fail_writes: bool,
    }

    impl MemoryInterface for FailingWriteMemory {
        fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_dword(addr)
        }
        fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
            self.inner.read_word(addr)
        }
        fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
            self.inner.read_half(addr)
        }
        fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
            self.inner.read_byte(addr)
        }
        fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_word_zext(addr)
        }
        fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_half_zext(addr)
        }
        fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_byte_zext(addr)
        }
        fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_word_sext(addr)
        }
        fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_half_sext(addr)
        }
        fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
            self.inner.read_byte_sext(addr)
        }
        fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
            if self.fail_writes {
                return Err(MemoryError::InvalidAddress(addr));
            }
            self.inner.write_dword(addr, value)
        }
        fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
            if self.fail_writes {
                return Err(MemoryError::InvalidAddress(addr));
            }
            self.inner.write_word(addr, value)
        }
        fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
            if self.fail_writes {
                return Err(MemoryError::InvalidAddress(addr));
            }
            self.inner.write_half(addr, value)
        }
        fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
            if self.fail_writes {
                return Err(MemoryError::InvalidAddress(addr));
            }
            self.inner.write_byte(addr, value)
        }
        fn size(&self) -> usize {
            self.inner.size()
        }
    }

    const P: u64 = 0xf0;
    let instruction = memory_with_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    let data = Arc::new(Mutex::new(FailingWriteMemory {
        inner: SimpleMemory::new(0x100),
        fail_writes: false,
    }));
    data.lock().unwrap().inner.write_dword(P, 13).unwrap();
    let mut core = RiscvCore::new(instruction, data.clone());
    core.reset(0, 0);
    set_mtvec(&mut core, 0x40);
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 14;
    retired(&mut core);

    data.lock().unwrap().fail_writes = true;
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(trap.mtval, P);

    // The trap discarded the staged state, retaining the reservation: the
    // same Hart's retried SC commits once the write side recovers.
    data.lock().unwrap().fail_writes = false;
    core.state_mut().pc = 4;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        0,
        "the retained reservation lets the retried SC commit"
    );
    assert_eq!(data.lock().unwrap().inner.read_dword(P).unwrap(), 14);

    // A different Hart holds no reservation even at the same address.
    let (mut after, after_memory) = port_core(&[(0, sc_encoding(5, 1, 2, 0b010))], 0x100);
    after_memory.lock().unwrap().write_dword(P, 13).unwrap();
    after.state_mut().regs[1] = P;
    after.state_mut().regs[2] = 15;
    retired(&mut after);
    assert_eq!(
        after.state().regs[5],
        1,
        "the retained record is per-Hart, not process-global"
    );
    assert_eq!(after_memory.lock().unwrap().read_dword(P).unwrap(), 13);
}

// ---------------------------------------------------------------------------
// Group 4: §3.4 HTIF per-encoding Hart outcomes on the envelope route.
// ---------------------------------------------------------------------------

/// LR at the HTIF endpoint is a target rejection → load access fault with
/// the original guest address, before any callback or mutation (D-c).
#[test]
fn htif_lr_is_a_target_rejection_at_base_and_interior() {
    let base = SYSTEM_BUS_HTIF_BASE;
    for (funct3, guest) in [(0b010u8, base), (0b011, base), (0b010, base + 4)] {
        let (mut core, _calls, atomic_calls, callbacks, _bus) =
            htif_core(&[lr_encoding(3, 1, funct3)]);
        core.state_mut().regs[1] = guest;
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(
            trap.cause,
            ExceptionCause::LoadAccessFault,
            "LR funct3={funct3:03b} at {guest:#x}"
        );
        assert_eq!(trap.mtval, guest);
        assert_eq!(
            atomic_calls.lock().unwrap().len(),
            1,
            "exactly one load-reserved envelope was issued and rejected"
        );
        assert!(
            callbacks.lock().unwrap().is_empty(),
            "a rejected LR fires no callback"
        );
    }

    // A faulting LR establishes no reservation: the following SC fails
    // Hart-side with rd = 1 and issues no envelope at all.
    let (mut core, _calls, atomic_calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAccessFault);
    core.state_mut().pc = 4;
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(
        atomic_calls.lock().unwrap().len(),
        1,
        "only the LR envelope was issued; the SC retired Hart-side"
    );
    assert!(callbacks.lock().unwrap().is_empty());
}

/// SC with no reservation retires `rd = 1` at the endpoint without issuing
/// an envelope; interior-offset doubleword encodings trap Hart-side
/// misalignment before any envelope (unchanged Hart prechecks).
#[test]
fn htif_sc_hart_side_preconditions_issue_no_envelope() {
    let base = SYSTEM_BUS_HTIF_BASE;

    let (mut core, _calls, atomic_calls, callbacks, _bus) =
        htif_core(&[sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert!(atomic_calls.lock().unwrap().is_empty());
    assert!(callbacks.lock().unwrap().is_empty());

    // SC.D at base+4: the encoded-width misalignment precheck traps before
    // the envelope, exactly as on RAM.
    let interior = base + 4;
    let (mut core, _calls, atomic_calls, callbacks, _bus) =
        htif_core(&[sc_encoding(4, 1, 2, 0b011)]);
    core.state_mut().regs[1] = interior;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAddressMisaligned);
    assert_eq!(trap.mtval, interior);
    assert!(atomic_calls.lock().unwrap().is_empty());
    assert!(callbacks.lock().unwrap().is_empty());

    // LR.D at base+4 traps load-address-misaligned the same way.
    let (mut core, _calls, atomic_calls, callbacks, _bus) = htif_core(&[lr_encoding(3, 1, 0b011)]);
    core.state_mut().regs[1] = interior;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAddressMisaligned);
    assert_eq!(trap.mtval, interior);
    assert!(atomic_calls.lock().unwrap().is_empty());
    assert!(callbacks.lock().unwrap().is_empty());
}

/// One RMW envelope at the endpoint is a single indivisible transaction:
/// AMOSWAP.D at base reads the endpoint's zero and fires the callback
/// exactly once; a word-width RMW is a target rejection → store/AMO access
/// fault (cause 7) with no callback (D-c).
#[test]
fn htif_rmw_envelope_commits_once_and_word_width_is_rejected() {
    let base = SYSTEM_BUS_HTIF_BASE;
    let value = 0x1234_5678_9abc_def0u64;

    let (mut core, _calls, atomic_calls, callbacks, _bus) =
        htif_core(&[amo_raw(0b00001, 0b011, 5, 1, 2)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    assert_eq!(core.state().regs[5], 0, "the endpoint reads zero");
    assert_eq!(*callbacks.lock().unwrap(), vec![value]);
    assert_eq!(
        atomic_calls.lock().unwrap().len(),
        1,
        "exactly one RMW envelope reached the endpoint"
    );

    let (mut core, _calls, atomic_calls, callbacks, _bus) =
        htif_core(&[amo_raw(0b00001, 0b010, 3, 1, 2)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 5;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(trap.mtval, base);
    assert_eq!(atomic_calls.lock().unwrap().len(), 1);
    assert!(callbacks.lock().unwrap().is_empty());
}

/// Legal `funct5` RMW encodings at the endpoint issue their one envelope
/// and fault as store/AMO access faults at word width; reserved `funct5`
/// values trap illegal-instruction with zero envelopes (C1/C4/C6/C21).
#[test]
fn htif_legal_ops_fault_at_endpoint_and_reserved_trap_illegal() {
    let base = SYSTEM_BUS_HTIF_BASE;

    for funct5 in [0b00000u8, 0b01100, 0b10000, 0b10100, 0b11000, 0b11100] {
        let (mut core, _calls, atomic_calls, callbacks, _bus) =
            htif_core(&[amo_raw(funct5, 0b010, 3, 1, 2)]);
        core.state_mut().regs[1] = base;
        core.state_mut().regs[2] = 5;
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(
            trap.cause,
            ExceptionCause::StoreAccessFault,
            "funct5={funct5:05b}: a legal word-width RMW is a target rejection"
        );
        assert_eq!(trap.mtval, base);
        assert_eq!(
            atomic_calls.lock().unwrap().len(),
            1,
            "funct5={funct5:05b}: one envelope was issued and rejected"
        );
        assert!(callbacks.lock().unwrap().is_empty());
    }

    for funct5 in [
        0b00101u8, 0b00110, 0b00111, 0b01101, 0b10001, 0b10101, 0b11111,
    ] {
        let (mut core, _calls, atomic_calls, callbacks, _bus) =
            htif_core(&[amo_raw(funct5, 0b010, 3, 1, 2)]);
        core.state_mut().regs[1] = base;
        core.state_mut().regs[2] = 5;
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(
            trap.cause,
            ExceptionCause::IllegalInstruction,
            "funct5={funct5:05b}: reserved encodings are illegal-instruction traps"
        );
        assert!(
            atomic_calls.lock().unwrap().is_empty(),
            "funct5={funct5:05b}: the trap precedes any envelope"
        );
        assert!(callbacks.lock().unwrap().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Group 5: ordinary-path context rows that the atomic flips must not disturb.
// ---------------------------------------------------------------------------

/// The LR/SC/AMO encoded-width prechecks remain Hart-side and class-split:
/// LR is load-class, SC/AMO store-class, both before any physical request.
#[test]
fn lr_and_sc_amo_class_faults_keep_their_cause_mapping() {
    const GUEST: u64 = 0x82;
    for (encoding, expected_cause) in [
        (
            lr_encoding(3, 1, 0b010),
            ExceptionCause::LoadAddressMisaligned,
        ),
        (
            lr_encoding(3, 1, 0b011),
            ExceptionCause::LoadAddressMisaligned,
        ),
        (
            sc_encoding(4, 1, 2, 0b010),
            ExceptionCause::StoreAddressMisaligned,
        ),
        (
            sc_encoding(4, 1, 2, 0b011),
            ExceptionCause::StoreAddressMisaligned,
        ),
        (
            amo_raw(0b00001, 0b010, 3, 1, 2),
            ExceptionCause::StoreAddressMisaligned,
        ),
    ] {
        let (mut core, _memory) = typed_core(&[(0, encoding)], 0x100);
        core.state_mut().regs[1] = GUEST;
        core.state_mut().regs[2] = 1;
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(trap.cause, expected_cause, "encoding {encoding:#010x}");
        assert_eq!(trap.mtval, GUEST);
    }

    // LR is load-class when the fault happens inside the target: LR.D at a
    // guest-dword-aligned address whose flat storage offset is misaligned
    // enters a load access fault with the original guest address.
    const IMAGE_BASE: u64 = 0x8000_0004;
    const DATA_GUEST: u64 = IMAGE_BASE + 4;
    let (mut core, memory) = typed_core(&[(0, lr_encoding(3, 1, 0b011))], 0x40);
    core.reset(IMAGE_BASE, IMAGE_BASE);
    set_mtvec(&mut core, 0x20);
    core.state_mut().regs[1] = DATA_GUEST;
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAccessFault);
    assert_eq!(trap.mtval, DATA_GUEST);
    drop(memory);
}

/// Sanity guard: the ordinary load path used by the characterization
/// fixtures is unchanged by the atomic work.
#[test]
fn ordinary_load_context_row_still_retires() {
    let (mut core, memory) = typed_core(&[(0, ld(5, 1, 0))], 0x100);
    memory.lock().unwrap().write_dword(0x80, 0x1234).unwrap();
    core.state_mut().regs[1] = 0x80;
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, ld(5, 1, 0));
    assert_eq!(core.state().regs[5], 0x1234);
}
