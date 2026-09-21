//! A8 T0 executable baseline (dev-plan §8 T0): fixtures that pin the verified
//! pre-A8 behavior of the AMO/LR/SC `funct5` dispatch, the process-global
//! reservation, and the §3.4 HTIF per-encoding Hart outcomes, on unchanged
//! code.
//!
//! Every assertion was verified against the source inspected at the T0 base
//! commit (`6c0a6df`, dev-plan evidence baseline) and against the running
//! tests; together with `docs/verification/a8-fixture-reclassification.md`
//! they form the "old" column of the A8 §6 compatibility matrix.  Each later
//! contract flip is a diff against this baseline.  Zero behavior change is
//! intended and required: this task modifies no `src/` file, and the existing
//! A7 fixtures must stay green alongside this one.
//!
//! Fixture groups:
//!
//! 1. `funct5` dispatch status quo for all 32 values × W/D, including the
//!    mis-targeted spec encodings (AMOMINU `11000`/AMOMAXU `11100` are not
//!    dispatched), the reserved values that currently succeed with a wrong
//!    operation, and the missing AMOSWAP (`00001` executes AMOADD).
//! 2. C30 cross-width reservation rows: `LR.W@p→SC.D@p` succeeds today and
//!    `LR.D@p→SC.W@(p+4)` fails today under the width-less exact-address key.
//! 3. `GLOBAL_RESERVATION` process-global scope rows.
//! 4. The §3.4 HTIF per-encoding Hart-outcome baseline through the real
//!    native `SystemBus`, including the LR.W dword-read bug and the dword
//!    SC callback successes.
//!
//! All rows are characterized observations of deliberate pre-A8 debt, not
//! conformance claims; the A8 contract (§2, §5, §6, §11) is the authority for
//! how each row flips.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{
    ExceptionCause, InstructionRetired, RiscvCore, SimulatorFailure, SimulatorFailureKind,
    StepOutcome, TrapEntered,
};
use ruscv_sim::csr::machine;
use ruscv_sim::execute::clear_reservation;
use ruscv_sim::executor::{RiscVSimulator, SystemBus, SYSTEM_BUS_HTIF_BASE};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{NativeRamBackend, NativeSystemBusBackend, ValidatedPhysicalAccess};
use std::sync::{Arc, Mutex};

const UART_BASE: u64 = 0x1000_0000;

/// AMO/LR/SC helpers share one process-global reservation singleton.
/// Every test in this file that executes an LR or SC serializes on this lock
/// and clears the singleton at entry, so rows cannot invalidate each other.
static RESERVATION_SERIAL: Mutex<()> = Mutex::new(());

type SharedCalls = Arc<Mutex<Vec<String>>>;
type SharedCallbacks = Arc<Mutex<Vec<u64>>>;
type SharedBus = Arc<Mutex<SystemBus>>;

/// Panic-resilient acquire: a failed baseline row must not poison the
/// serialization lock for the remaining rows in this process.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    RESERVATION_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

/// Malformed LR encoding (`funct5=00010`, `rs2!=0`) that the current
/// dispatcher executes as SC with reversed widths.
fn malformed_lr_as_sc(rd: u8, rs1: u8, rs2: u8, funct3: u8) -> u32 {
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
/// AMO/LR/SC both use the typed handles, like the A7 characterization rows.
fn typed_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
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

fn failure(core: &mut RiscvCore) -> SimulatorFailure {
    match core.step_outcome() {
        StepOutcome::SimulatorFailure(failure) => failure,
        StepOutcome::InstructionRetired(fact) => {
            panic!(
                "expected a simulator failure, got retired {:?}",
                fact.instruction
            )
        }
        StepOutcome::TrapEntered(trap) => {
            panic!("expected a simulator failure, got trap {:?}", trap.cause)
        }
    }
}

/// A typed legacy view over the real native `SystemBus` that records the
/// exact `MemoryInterface` calls selected by the dispatcher.  The HTIF
/// callback is registered on the inner bus, so typed and raw writes are both
/// observed.
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

/// Standard-facade wiring over the real native bus: raw fetch port over the
/// instruction RAM, raw data port over the bus, and the traced typed view as
/// the legacy AMO/LR/SC route.  This is exactly the A7 legacy-MMIO harness.
fn htif_core(program: &[u32]) -> (RiscvCore, SharedCalls, SharedCallbacks, SharedBus) {
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
    let data_typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
        Arc::new(Mutex::new(TracedSystemBus {
            bus: bus.clone(),
            calls: calls.clone(),
        }));
    let instruction_typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = instruction.clone();
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(instruction.clone(), 0, 0x40),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        instruction_typed,
        data_typed,
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, calls, callbacks, bus)
}

// ---------------------------------------------------------------------------
// Group 1: funct5 dispatch status quo (all 32 values × W/D).
// ---------------------------------------------------------------------------

/// What the current dispatcher does with one `funct5` value under `rs2 != 0`.
enum DispatchExpectation {
    /// Retires through the named helper; the 32-bit memory word becomes
    /// `new_word` (word helpers only, both for funct3=010 and funct3=011).
    Retire { new_word: u32, note: &'static str },
    /// Retires as SC with `rd=1` and no memory call (no live reservation).
    ScNoReservation,
    /// `ExecuteError::InvalidOperation` → `SimulatorFailure`
    /// (`UnsupportedLegalInstruction`) before any memory access.
    Unsupported,
}

/// Initial memory word 10, operand 6; AMO helper results below are the
/// 32-bit values produced by the currently dispatched helper.
const OLD_WORD: u32 = 10;
const OPERAND: u64 = 6;

fn expected_dispatch(funct5: u8) -> DispatchExpectation {
    match funct5 {
        // Real AMOADD (spec AMOSWAP): dispatches to exec_amoadd — no swap
        // helper exists (C2 old column).
        0b00001 => Retire {
            new_word: OLD_WORD.wrapping_add(OPERAND as u32),
            note: "exec_amoadd (AMOSWAP missing)",
        },
        // LR/SC family: with rs2 != 0 this is the malformed-LR-as-SC path;
        // without a reservation both widths retire rd=1 with no write.
        0b00010 => DispatchExpectation::ScNoReservation,
        // Real SC (funct5=00011 → exec_sc fallback for both widths).
        0b00011 => DispatchExpectation::ScNoReservation,
        // Correct dispatch arm (C3 old column: word helper for W and D).
        0b00100 => Retire {
            new_word: OLD_WORD ^ OPERAND as u32,
            note: "exec_amoxor",
        },
        // Unassigned values matched to wrong operations (C6 old column).
        0b00110 => Retire {
            new_word: OLD_WORD | OPERAND as u32,
            note: "exec_amoor (unassigned funct5)",
        },
        0b00111 => Retire {
            new_word: OLD_WORD & OPERAND as u32,
            note: "exec_amoand (unassigned funct5)",
        },
        // Spec AMOOR executes the AMOMIN helper (C5 old column).
        0b01000 => Retire {
            new_word: if (OLD_WORD as i32) < (OPERAND as u32 as i32) {
                OLD_WORD
            } else {
                OPERAND as u32
            },
            note: "exec_amomin (spec AMOOR mis-target)",
        },
        0b01001 => Retire {
            new_word: if OLD_WORD < OPERAND as u32 {
                OLD_WORD
            } else {
                OPERAND as u32
            },
            note: "exec_amominu (unassigned funct5)",
        },
        0b01010 => Retire {
            new_word: if (OLD_WORD as i32) > (OPERAND as u32 as i32) {
                OLD_WORD
            } else {
                OPERAND as u32
            },
            note: "exec_amomax (unassigned funct5)",
        },
        0b01011 => Retire {
            new_word: if OLD_WORD > OPERAND as u32 {
                OLD_WORD
            } else {
                OPERAND as u32
            },
            note: "exec_amomaxu (unassigned funct5)",
        },
        // Spec AMOADD, AMOAND, AMOMIN, AMOMAX, AMOMINU, AMOMAXU encodings and
        // every other value have no dispatch arm: simulator failure before
        // any access (C1/C4 old column).  This includes 11000/11100 (the
        // spec AMOMINU/AMOMAXU encodings, currently unreachable) and the
        // draft's mis-listed reserved values 10001/10101.
        _ => DispatchExpectation::Unsupported,
    }
}

use DispatchExpectation::{Retire, ScNoReservation, Unsupported};

#[test]
fn dispatch_matrix_pins_all_32_funct5_values() {
    let _serial = serial();
    clear_reservation();

    for funct5 in 0u8..=31 {
        for funct3 in [0b010u8, 0b011] {
            let encoding = amo_raw(funct5, funct3, 3, 1, 2);
            let (mut core, memory) = typed_core(&[(0, encoding), (4, nop())], 0x100);
            core.state_mut().regs[1] = 0x80;
            core.state_mut().regs[2] = OPERAND;
            memory.lock().unwrap().write_word(0x80, OLD_WORD).unwrap();

            match expected_dispatch(funct5) {
                Retire { new_word, note } => {
                    let fact = retired(&mut core);
                    assert_eq!(
                        fact.instruction, encoding,
                        "funct5={funct5:05b}/{funct3:03b}"
                    );
                    assert_eq!(
                        core.state().regs[3],
                        OLD_WORD as i32 as i64 as u64,
                        "funct5={funct5:05b}/{funct3:03b} ({note}): rd must be the \
                         sign-extended old word"
                    );
                    assert_eq!(
                        memory.lock().unwrap().read_word(0x80).unwrap(),
                        new_word,
                        "funct5={funct5:05b}/{funct3:03b} ({note}): memory word"
                    );
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
                        memory.lock().unwrap().read_word(0x80).unwrap(),
                        OLD_WORD,
                        "funct5={funct5:05b}/{funct3:03b}: failed SC must not write"
                    );
                }
                Unsupported => {
                    let failure = failure(&mut core);
                    assert_eq!(
                        failure.kind,
                        SimulatorFailureKind::UnsupportedLegalInstruction,
                        "funct5={funct5:05b}/{funct3:03b}: expected an unsupported \
                         legal-instruction failure, got {failure:?}"
                    );
                    assert_eq!(
                        memory.lock().unwrap().read_word(0x80).unwrap(),
                        OLD_WORD,
                        "funct5={funct5:05b}/{funct3:03b}: failure must precede any \
                         memory access"
                    );
                    assert_eq!(
                        core.state().pc,
                        0,
                        "funct5={funct5:05b}/{funct3:03b}: pc must not advance"
                    );
                }
            }
        }
    }
}

/// Discriminating rows for the mis-targeted helpers: one mixed-sign and one
/// same-sign operand pair together pin *which* helper each unassigned or
/// mis-targeted `funct5` reaches (a single pair cannot separate signed from
/// unsigned min/max because their results coincide structurally).
#[test]
fn min_max_helper_mistargets_are_discriminated_by_two_operand_pairs() {
    let _serial = serial();
    clear_reservation();

    // (old, operand) → (mixed-sign result, same-sign result)
    //   AMOMIN:  (0xFFFF_FFF6, 6)   AMOMINU: (6, 6)
    //   AMOMAX:  (6, 10)            AMOMAXU: (0xFFFF_FFF6, 10)
    let mixed = (0xFFFF_FFF6u64, 6u64); // old = -10 signed
    let same = (10u64, 6u64);

    let observe = |funct5: u8, funct3: u8, old: u64, operand: u64| {
        clear_reservation();
        let encoding = amo_raw(funct5, funct3, 3, 1, 2);
        let (mut core, memory) = typed_core(&[(0, encoding)], 0x100);
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[2] = operand;
        memory.lock().unwrap().write_word(0x80, old as u32).unwrap();
        let fact = retired(&mut core);
        assert_eq!(fact.instruction, encoding);
        let word = memory.lock().unwrap().read_word(0x80).unwrap();
        word
    };

    for funct3 in [0b010u8, 0b011] {
        // 01000 (spec AMOOR) executes AMOMIN arithmetic, not OR: mixed-sign
        // pair yields the signed minimum 0xFFFF_FFF6 (the old -10), whereas
        // AMOOR would produce 0xFFFF_FFFE.
        assert_eq!(
            observe(0b01000, funct3, mixed.0, mixed.1),
            0xFFFF_FFF6,
            "funct3={funct3:03b}: 01000 must reach the signed-min helper"
        );
        assert_eq!(
            observe(0b01000, funct3, same.0, same.1),
            6,
            "funct3={funct3:03b}: 01000 must reach min, not a max helper"
        );
        // 01001 executes the *unsigned* min helper: mixed-sign pair yields 6
        // (the signed-min helper would yield 0xFFFF_FFFA).
        assert_eq!(
            observe(0b01001, funct3, mixed.0, mixed.1),
            6,
            "funct3={funct3:03b}: 01001 must reach the unsigned-min helper"
        );
        // 01010 executes the *signed* max helper: mixed-sign pair yields 6.
        assert_eq!(
            observe(0b01010, funct3, mixed.0, mixed.1),
            6,
            "funct3={funct3:03b}: 01010 must reach the signed-max helper"
        );
        // 01011 executes the *unsigned* max helper: mixed-sign pair yields
        // 0xFFFF_FFF6 and the same-sign pair yields 10.
        assert_eq!(
            observe(0b01011, funct3, mixed.0, mixed.1),
            0xFFFF_FFF6,
            "funct3={funct3:03b}: 01011 must reach the unsigned-max helper"
        );
        assert_eq!(
            observe(0b01011, funct3, same.0, same.1),
            10,
            "funct3={funct3:03b}: 01011 must reach max, not a min helper"
        );
    }
}

/// The real AMOSWAP encoding (`funct5=00001`) executes AMOADD arithmetic:
/// the memory result is `old + rs2`, not `rs2`, and no swap helper exists.
#[test]
fn amoswap_encoding_executes_amoadd_arithmetic() {
    let _serial = serial();
    clear_reservation();

    for funct3 in [0b010u8, 0b011] {
        let encoding = amo_raw(0b00001, funct3, 3, 1, 2);
        let (mut core, memory) = typed_core(&[(0, encoding)], 0x100);
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[2] = 6;
        memory.lock().unwrap().write_word(0x80, 10).unwrap();

        let fact = retired(&mut core);
        assert_eq!(fact.instruction, encoding);
        assert_eq!(core.state().regs[3], 10, "rd must be the old value");
        assert_eq!(
            memory.lock().unwrap().read_word(0x80).unwrap(),
            16,
            "funct3={funct3:03b}: memory must be old+operand (AMOADD), not the \
             operand (a swap would leave 6)"
        );
    }
}

/// AMO encodings with `funct3` outside {010, 011} are illegal-instruction
/// traps before any access (pre-existing Hart rule, unchanged by A8).
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

// ---------------------------------------------------------------------------
// Group 2: LR/SC widths and the C30 cross-width rows.
// ---------------------------------------------------------------------------

/// Width reversal on RAM with a live reservation (C9/C11 old column):
/// the real SC encodings both perform a 64-bit dword write (SC.W wrongly),
/// and the malformed LR encodings execute SC with reversed widths.
#[test]
fn sc_width_reversal_on_ram_pins_dword_and_word_helpers() {
    let _serial = serial();
    const P: u64 = 0x80;
    let value = 0x1234_5678_9abc_def0u64;

    // Malformed LR (00010, rs2!=0, funct3=010) → exec_sc → dword write.
    clear_reservation();
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, malformed_lr_as_sc(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 0).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, malformed_lr_as_sc(4, 1, 2, 0b010));
    assert_eq!(
        core.state().regs[4],
        0,
        "SC must succeed with a live reservation"
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P).unwrap(),
        value,
        "malformed funct3=010 encoding must perform a 64-bit write"
    );

    // Malformed LR (00010, rs2!=0, funct3=011) → exec_sc_w → 32-bit write.
    clear_reservation();
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, malformed_lr_as_sc(4, 1, 2, 0b011)),
        ],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 0).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(
        memory.lock().unwrap().read_word(P).unwrap(),
        0x9abc_def0,
        "malformed funct3=011 encoding must perform a 32-bit write"
    );
    assert_eq!(
        memory.lock().unwrap().read_word(P + 4).unwrap(),
        0,
        "the upper word must be unchanged"
    );

    // Real SC.W (00011, funct3=010) → exec_sc → dword write (width debt C9).
    clear_reservation();
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    memory.lock().unwrap().write_dword(P, 0).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(
        memory.lock().unwrap().read_dword(P).unwrap(),
        value,
        "real SC.W must write all eight bytes through the exec_sc fallback"
    );

    // Real SC.D (00011, funct3=011) → exec_sc → dword write: already the
    // correct width via the same fallback (C10 old column).
    clear_reservation();
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
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

/// C30 old column: the reservation key is the exact starting address with no
/// recorded width, so `LR.W@p→SC.D@p` succeeds today while
/// `LR.D@p→SC.W@(p+4)` fails today.  Both flip under the approved
/// span-containment rule (§5.4/C30); these rows are the executable baseline.
#[test]
fn cross_width_sc_rows_pin_the_width_less_exact_address_key() {
    let _serial = serial();
    const P: u64 = 0x80;

    // Direction 1: LR.W@p → SC.D@p succeeds.
    clear_reservation();
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
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, lr_encoding(3, 1, 0b010));
    // Width reversal: the W encoding performs the 64-bit read.
    assert_eq!(
        core.state().regs[3],
        0x0000_0000_8000_0001,
        "LR.W must read a full dword (bug), not a sign-extended word"
    );
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, sc_encoding(4, 1, 2, 0b011));
    assert_eq!(
        core.state().regs[4],
        0,
        "C30 old: SC.D at the exact reserved address must succeed"
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P).unwrap(),
        0xdead_beef_cafe_f00d
    );

    // Direction 2: LR.D@p → SC.W@(p+4) fails.
    clear_reservation();
    let (mut core, memory) = typed_core(
        &[
            (0, lr_encoding(3, 1, 0b011)),
            (4, sc_encoding(4, 1, 4, 0b010)),
        ],
        0x200,
    );
    memory
        .lock()
        .unwrap()
        .write_dword(P, 0x0000_0000_8000_0001)
        .unwrap();
    memory.lock().unwrap().write_dword(P + 8, 0x55).unwrap();
    core.state_mut().regs[1] = P;
    core.state_mut().regs[2] = 0x66;
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, lr_encoding(3, 1, 0b011));
    // Width reversal: the D encoding performs the 32-bit sign-extended read.
    assert_eq!(
        core.state().regs[3],
        0xFFFF_FFFF_8000_0001,
        "LR.D must read a sign-extended word (bug), not a full dword"
    );
    core.state_mut().regs[1] = P + 4;
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, sc_encoding(4, 1, 4, 0b010));
    assert_eq!(
        core.state().regs[4],
        1,
        "C30 old: SC.W at p+4 must fail under the exact-address key even \
         though its span is inside the LR.D dword"
    );
    assert_eq!(memory.lock().unwrap().read_word(P + 4).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Group 3: GLOBAL_RESERVATION process-global scope.
// ---------------------------------------------------------------------------

#[test]
fn global_reservation_is_shared_across_cores_and_survives_reset() {
    let _serial = serial();
    const P: u64 = 0x90;

    // LR in one core instance, SC in a different core instance at the same
    // exact address: the singleton key is process-global.
    clear_reservation();
    let (mut first, first_memory) = typed_core(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    first_memory.lock().unwrap().write_dword(P, 7).unwrap();
    first.state_mut().regs[1] = P;
    retired(&mut first);

    let (mut second, second_memory) = typed_core(&[(0, sc_encoding(4, 1, 2, 0b010))], 0x200);
    second_memory.lock().unwrap().write_dword(P, 7).unwrap();
    second.state_mut().regs[1] = P;
    second.state_mut().regs[2] = 8;
    retired(&mut second);
    assert_eq!(
        second.state().regs[4],
        0,
        "the reservation key must be shared across independent Core instances"
    );

    // Reset does not clear the singleton.
    clear_reservation();
    let (mut core, memory) = typed_core(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
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
        0,
        "reset must not clear the process-global reservation"
    );

    // The key is the exact address, not a granule.
    clear_reservation();
    let (mut core, memory) = typed_core(
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
        "an SC at a different exact address must fail"
    );
    assert_eq!(memory.lock().unwrap().read_dword(0xb8).unwrap(), 31);
}

#[test]
fn global_reservation_survives_flat_image_replacement() {
    let _serial = serial();
    clear_reservation();
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

    // The reload replaces the core and the RAM while the singleton guest
    // address key survives (C16 old column; flips under per-Hart state).
    simulator.load_elf(&second).unwrap();
    simulator.state_mut().regs[1] = fixture::BASE + 0x80;
    simulator.state_mut().regs[2] = 22;
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        0,
        "the flat reload must retain the legacy reservation key"
    );
}

#[test]
fn host_write_mem_does_not_invalidate_the_global_reservation() {
    let _serial = serial();
    clear_reservation();
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

    // Host write_mem is not an invalidation source today (C20 old column;
    // flips under approved P2a-precise).
    simulator.write_mem(P, &7u64.to_le_bytes()).unwrap();
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    assert_eq!(
        simulator.state().regs[4],
        0,
        "the host write must not invalidate the global reservation"
    );
    assert_eq!(simulator.read_mem(P, 8).unwrap(), 6u64.to_le_bytes());
}

/// A write-faulting SC retains the reservation because the helper returns
/// before its clear (C15 old column: same observable the contract relabels
/// as the approved faulting-SC retain profile).
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

    let _serial = serial();
    clear_reservation();
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

    // The faulting SC returned before its reservation clear, so a following
    // SC at the same address can still succeed.  The reservation must NOT be
    // cleared here: the retained singleton key is exactly what the second
    // core consumes.
    let (mut after, after_memory) = typed_core(&[(0, sc_encoding(5, 1, 2, 0b010))], 0x100);
    after_memory.lock().unwrap().write_dword(P, 13).unwrap();
    after.state_mut().regs[1] = P;
    after.state_mut().regs[2] = 15;
    retired(&mut after);
    assert_eq!(
        after.state().regs[5],
        0,
        "the faulting SC must retain the reservation (early return before clear)"
    );
    assert_eq!(after_memory.lock().unwrap().read_dword(P).unwrap(), 15);
}

// ---------------------------------------------------------------------------
// Group 4: §3.4 HTIF per-encoding Hart-outcome baseline (native SystemBus).
// ---------------------------------------------------------------------------

/// LR.W at 4-aligned endpoint addresses retires through the dword-read bug
/// and sets the reservation (C21 old column; LR.D has no A7 fixture).
#[test]
fn htif_lr_w_retires_via_dword_read_bug_and_reserves() {
    let _serial = serial();
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = SYSTEM_BUS_HTIF_BASE;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;

    let fact = retired(&mut core);
    assert_eq!(fact.instruction, lr_encoding(3, 1, 0b010));
    assert_eq!(core.state().regs[3], 0, "HTIF dword reads return zero");
    assert_eq!(
        *calls.lock().unwrap(),
        vec![format!("read_dword@{:#x}", SYSTEM_BUS_HTIF_BASE)],
        "LR.W must perform the buggy 64-bit typed read"
    );
    assert!(callbacks.lock().unwrap().is_empty(), "reads never callback");

    // The reservation was set by the faulting-free LR.W: the following SC.W
    // at the same endpoint address succeeds through the dword write.
    let fact = retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![0x1234_5678_9abc_def0]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{:#x}", SYSTEM_BUS_HTIF_BASE),
            format!("write_dword@{:#x}", SYSTEM_BUS_HTIF_BASE),
        ]
    );
    assert_eq!(fact.instruction, sc_encoding(4, 1, 2, 0b010));

    // The same rows at the interior 4-aligned address base+4: the typed HTIF
    // check is start-address-only, so the dword read and write both succeed.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b010)]);
    let interior = SYSTEM_BUS_HTIF_BASE + 4;
    core.state_mut().regs[1] = interior;
    core.state_mut().regs[2] = 0x0fed_cba9_8765_4321;
    retired(&mut core);
    assert_eq!(core.state().regs[3], 0);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![format!("read_dword@{interior:#x}")]
    );
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![0x0fed_cba9_8765_4321]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{interior:#x}"),
            format!("write_dword@{interior:#x}"),
        ]
    );
}

/// Real LR.D at the endpoint base faults as a load access fault (the typed
/// `read_word` has no HTIF branch) and establishes no reservation.
#[test]
fn htif_real_lr_d_at_base_load_access_fault_and_no_reservation() {
    let _serial = serial();
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b011), sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = SYSTEM_BUS_HTIF_BASE;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    set_mtvec(&mut core, 0x40);

    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAccessFault);
    assert_eq!(trap.mtval, SYSTEM_BUS_HTIF_BASE);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![format!("read_word@{:#x}", SYSTEM_BUS_HTIF_BASE)],
        "LR.D must perform the reversed 32-bit typed read"
    );
    assert!(callbacks.lock().unwrap().is_empty());

    // The faulting LR established no reservation: the following SC fails
    // without a callback.  (The trap advanced PC to mtvec; the second
    // encoding sits at offset 4.)
    core.state_mut().pc = 4;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "no reservation must be established"
    );
    assert!(callbacks.lock().unwrap().is_empty());
}

/// Real LR.D at the interior offset (≡4 mod 8) traps encoded misalignment
/// before any access; the same precheck traps SC.D at base+4 even with a
/// live reservation at that address.
#[test]
fn htif_interior_doubleword_encodings_trap_before_any_access() {
    let _serial = serial();
    let interior = SYSTEM_BUS_HTIF_BASE + 4;

    // LR.D at base+4: width-8 precheck fails → load-address-misaligned.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) = htif_core(&[lr_encoding(3, 1, 0b011)]);
    core.state_mut().regs[1] = interior;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAddressMisaligned);
    assert_eq!(trap.mtval, interior);
    assert!(
        calls.lock().unwrap().is_empty(),
        "no access may precede the trap"
    );
    assert!(callbacks.lock().unwrap().is_empty());

    // SC.D at base+4 with a live reservation at base+4: the encoded-width
    // precheck still fires first, before both the access and the
    // reservation logic inside the helper.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b011)]);
    core.state_mut().regs[1] = interior;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    retired(&mut core);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![format!("read_dword@{interior:#x}")],
        "the LR.W reservation source must have accessed the endpoint"
    );
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAddressMisaligned);
    assert_eq!(trap.mtval, interior);
    assert_eq!(
        calls.lock().unwrap().len(),
        1,
        "SC.D at base+4 must trap before any access despite the live reservation"
    );
    assert!(callbacks.lock().unwrap().is_empty());
}

/// The SC family succeeds at encoded-aligned endpoint addresses through the
/// dword callback only with a live reservation at the same exact address
/// (SC.W at base and base+4, SC.D at base; SC.D at base+4 is covered by the
/// misalignment row above).
#[test]
fn htif_sc_family_dword_callback_needs_alignment_and_live_reservation() {
    let _serial = serial();
    let value = 0x1234_5678_9abc_def0u64;
    let base = SYSTEM_BUS_HTIF_BASE;

    // No reservation: SC.W at base retires rd=1 with no access, no callback.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) = htif_core(&[sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert!(calls.lock().unwrap().is_empty());
    assert!(callbacks.lock().unwrap().is_empty());

    // SC.W at base with a live reservation: dword callback + rd=0.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![value]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{base:#x}"),
            format!("write_dword@{base:#x}")
        ]
    );

    // SC.W at base+4 with a reservation at base+4 (the characterized
    // sc.w@htif+4 row).
    clear_reservation();
    let interior = base + 4;
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = interior;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![value]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{interior:#x}"),
            format!("write_dword@{interior:#x}")
        ]
    );

    // SC.D at base with a reservation at base: width-8 precheck passes, the
    // helper is the same dword write, so the full 64-bit value reaches the
    // callback (no A7 fixture covers real SC.D at the endpoint).
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), sc_encoding(4, 1, 2, 0b011)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = value;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![value]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{base:#x}"),
            format!("write_dword@{base:#x}")
        ]
    );
}

/// Dispatched AMO helpers reach the endpoint through the typed word read,
/// which has no HTIF branch: store/AMO access fault (cause 7) with the
/// original guest address and no callback (no A7 fixture covers AMO at
/// HTIF).
#[test]
fn htif_dispatched_amo_helpers_fault_store_access_cause_7() {
    let _serial = serial();
    let base = SYSTEM_BUS_HTIF_BASE;

    // AMOADD.W encoding at base: read_word → InvalidAddress → cause 7.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) = htif_core(&[amo_raw(0b00001, 0b010, 3, 1, 2)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 5;
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(trap.mtval, base);
    assert_eq!(*calls.lock().unwrap(), vec![format!("read_word@{base:#x}")]);
    assert!(callbacks.lock().unwrap().is_empty());

    // Malformed LR encoding (00010, rs2!=0, funct3=011) executes SC.W
    // (reversed width) → write_word → InvalidAddress → cause 7, but only
    // once a reservation is live; without one it retires rd=1 silently.
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), malformed_lr_as_sc(4, 1, 2, 0b011)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    retired(&mut core);
    set_mtvec(&mut core, 0x40);
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(trap.mtval, base);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{base:#x}"),
            format!("write_word@{base:#x}"),
        ],
        "the malformed funct3=011 encoding must attempt the reversed 32-bit write"
    );
    assert!(callbacks.lock().unwrap().is_empty());

    // The same malformed encoding with funct3=010 mirrors SC.W at base:
    // dword write → callback success (§3.4 row).
    clear_reservation();
    let (mut core, calls, callbacks, _bus) =
        htif_core(&[lr_encoding(3, 1, 0b010), malformed_lr_as_sc(4, 1, 2, 0b010)]);
    core.state_mut().regs[1] = base;
    core.state_mut().regs[2] = 0x0fed_cba9_8765_4321;
    retired(&mut core);
    retired(&mut core);
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![0x0fed_cba9_8765_4321]);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            format!("read_dword@{base:#x}"),
            format!("write_dword@{base:#x}")
        ]
    );
}

/// Unsupported `funct5` AMO encodings are simulator failures before any
/// memory access, at RAM and at the endpoint alike (C1/C4 old column).
#[test]
fn htif_unsupported_funct5_fails_before_any_access() {
    let _serial = serial();
    clear_reservation();
    let base = SYSTEM_BUS_HTIF_BASE;

    for funct5 in [0b00000u8, 0b01100, 0b10000, 0b10100, 0b11000, 0b11100] {
        let (mut core, calls, callbacks, _bus) = htif_core(&[amo_raw(funct5, 0b010, 3, 1, 2)]);
        core.state_mut().regs[1] = base;
        core.state_mut().regs[2] = 5;
        let failure = failure(&mut core);
        assert_eq!(
            failure.kind,
            SimulatorFailureKind::UnsupportedLegalInstruction
        );
        assert!(
            failure.message.contains("legal extension instruction"),
            "funct5={funct5:05b}: unexpected failure message {failure:?}"
        );
        assert!(
            calls.lock().unwrap().is_empty(),
            "funct5={funct5:05b}: the dispatch failure must precede any access"
        );
        assert!(
            callbacks.lock().unwrap().is_empty(),
            "funct5={funct5:05b}: no callback may fire"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 5: ordinary-path context rows that the atomic flips must not disturb.
// ---------------------------------------------------------------------------

/// The LR/SC width prechecks are Hart-side and class-split: LR is
/// load-class and SC/AMO store-class, both before any physical request
/// (C26 old column, unchanged by A8).
#[test]
fn lr_and_sc_amo_class_faults_keep_their_cause_mapping() {
    let _serial = serial();
    clear_reservation();

    // Encoded-width precheck classes at a 4-mod-8 guest address: LR enters
    // load-address-misaligned, SC and AMO enter store-address-misaligned,
    // and none of them reaches memory.
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
        clear_reservation();
        let (mut core, _memory) = typed_core(&[(0, encoding)], 0x100);
        core.state_mut().regs[1] = GUEST;
        core.state_mut().regs[2] = 1;
        set_mtvec(&mut core, 0x40);
        let trap = trapped(&mut core);
        assert_eq!(trap.cause, expected_cause, "encoding {encoding:#010x}");
        assert_eq!(trap.mtval, GUEST);
    }

    // LR is load-class even when the fault happens inside the backend: the
    // buggy dword read at a dword-unaligned flat backend offset faults as a
    // load access fault with the original guest address (§3.1 consequence).
    clear_reservation();
    const IMAGE_BASE: u64 = 0x8000_0004;
    const DATA_GUEST: u64 = IMAGE_BASE + 4;
    let (mut core, memory) = typed_core(&[(0, lr_encoding(3, 1, 0b010))], 0x40);
    core.reset(IMAGE_BASE, IMAGE_BASE);
    set_mtvec(&mut core, 0x20);
    core.state_mut().regs[1] = DATA_GUEST;
    let trap = trapped(&mut core);
    assert_eq!(trap.cause, ExceptionCause::LoadAccessFault);
    assert_eq!(trap.mtval, DATA_GUEST);
    drop(memory);
}

/// Sanity guard: the executable baseline links the same `ld`-shaped
/// ordinary load used by the characterization fixtures, so a regression in
/// the ordinary path surfaces here too.
#[test]
fn ordinary_load_context_row_still_retires() {
    let (mut core, memory) = typed_core(&[(0, ld(5, 1, 0))], 0x100);
    memory.lock().unwrap().write_dword(0x80, 0x1234).unwrap();
    core.state_mut().regs[1] = 0x80;
    let fact = retired(&mut core);
    assert_eq!(fact.instruction, ld(5, 1, 0));
    assert_eq!(core.state().regs[5], 0x1234);
}
