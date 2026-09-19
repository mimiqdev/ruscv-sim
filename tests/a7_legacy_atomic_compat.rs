//! A7 T3 compatibility bridge tests.
//!
//! AMO/LR/SC remains on the original Executor/helpers and the outer typed
//! memory lock.  These fixtures deliberately assert the characterized global
//! reservation behavior rather than architectural per-Hart semantics.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::elf::SignatureInfo;
use ruscv_sim::executor::{dump_signature, RiscVSimulator, SystemBus, SYSTEM_BUS_HTIF_BASE};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AccessCategory, AccessWidth, NativeRamBackend, NativeSystemBusBackend, PhysicalBackend,
    PhysicalBackendResult, PhysicalRequest, ValidatedPhysicalAccess,
};
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "RUSCV_A7_LEGACY_LOCK_CHILD";
const CHILD_TEST: &str = "legacy_lock_reentry_child";
const CHILD_TIMEOUT: Duration = Duration::from_secs(5);

static RESERVATION_FIXTURE_LOCK: Mutex<()> = Mutex::new(());

type SharedRam = Arc<Mutex<SimpleMemory>>;
type SharedMemory = Arc<Mutex<dyn MemoryInterface + Send + Sync>>;
type RawCall = (AccessCategory, AccessWidth, u64, Option<Vec<u8>>);
type RawCalls = Arc<Mutex<Vec<RawCall>>>;

fn amo_raw(funct5: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn lr(rd: u8, rs1: u8) -> u32 {
    amo_raw(0b00010, 0b010, rd, rs1, 0)
}

fn sc(rd: u8, rs1: u8, rs2: u8) -> u32 {
    // The existing dispatcher intentionally retains its SC width fallback.
    amo_raw(0b00011, 0b010, rd, rs1, rs2)
}

fn amoadd_d(rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(0b00001, 0b011, rd, rs1, rs2)
}

fn load(rd: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn store(rs2: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn typed_handle<M: MemoryInterface + Send + Sync + 'static>(memory: Arc<Mutex<M>>) -> SharedMemory {
    memory
}

/// A typed legacy view over the same backing RAM.  It records the helper call
/// order while the core owns this wrapper's outer mutex.
struct LegacyMemory {
    backing: SharedRam,
    calls: Arc<Mutex<Vec<String>>>,
    fail_writes: bool,
}

impl LegacyMemory {
    fn new(backing: SharedRam, calls: Arc<Mutex<Vec<String>>>, fail_writes: bool) -> Self {
        Self {
            backing,
            calls,
            fail_writes,
        }
    }

    fn record(&self, operation: &str, address: u64) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{operation}@{address:#x}"));
    }

    fn set_fail_writes(&mut self, fail_writes: bool) {
        self.fail_writes = fail_writes;
    }
}

impl MemoryInterface for LegacyMemory {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_dword", addr);
        self.backing.lock().unwrap().read_dword(addr)
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        self.record("read_word", addr);
        self.backing.lock().unwrap().read_word(addr)
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        self.record("read_half", addr);
        self.backing.lock().unwrap().read_half(addr)
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        self.record("read_byte", addr);
        self.backing.lock().unwrap().read_byte(addr)
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_word_zext", addr);
        self.backing.lock().unwrap().read_word_zext(addr)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_half_zext", addr);
        self.backing.lock().unwrap().read_half_zext(addr)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_zext", addr);
        self.backing.lock().unwrap().read_byte_zext(addr)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_word_sext", addr);
        self.backing.lock().unwrap().read_word_sext(addr)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_half_sext", addr);
        self.backing.lock().unwrap().read_half_sext(addr)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_sext", addr);
        self.backing.lock().unwrap().read_byte_sext(addr)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.record("write_dword", addr);
        if self.fail_writes {
            return Err(MemoryError::InvalidAddress(addr));
        }
        self.backing.lock().unwrap().write_dword(addr, value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.record("write_word", addr);
        if self.fail_writes {
            return Err(MemoryError::InvalidAddress(addr));
        }
        self.backing.lock().unwrap().write_word(addr, value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.record("write_half", addr);
        if self.fail_writes {
            return Err(MemoryError::InvalidAddress(addr));
        }
        self.backing.lock().unwrap().write_half(addr, value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.record("write_byte", addr);
        if self.fail_writes {
            return Err(MemoryError::InvalidAddress(addr));
        }
        self.backing.lock().unwrap().write_byte(addr, value)
    }

    fn size(&self) -> usize {
        self.backing.lock().unwrap().size()
    }
}

struct RawTrace {
    inner: NativeRamBackend,
    calls: RawCalls,
}

impl PhysicalBackend for RawTrace {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.lock().unwrap().push((
            request.category(),
            request.width(),
            request.paddr(),
            request.payload().map(ToOwned::to_owned),
        ));
        self.inner.transact(request)
    }
}

fn legacy_core(
    backing: SharedRam,
    program: &[u32],
    legacy_calls: Arc<Mutex<Vec<String>>>,
    fail_writes: bool,
    raw_calls: RawCalls,
) -> (RiscvCore, Arc<Mutex<LegacyMemory>>, SharedRam) {
    {
        let mut ram = backing.lock().unwrap();
        for (index, instruction) in program.iter().copied().enumerate() {
            ram.write_word(index as u64 * 4, instruction).unwrap();
        }
    }
    let instruction_legacy = Arc::new(Mutex::new(LegacyMemory::new(
        backing.clone(),
        Arc::new(Mutex::new(Vec::new())),
        false,
    )));
    let data_legacy = Arc::new(Mutex::new(LegacyMemory::new(
        backing.clone(),
        legacy_calls,
        fail_writes,
    )));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(RawTrace {
        inner: NativeRamBackend::new(backing.clone(), 0, backing.lock().unwrap().size()),
        calls: raw_calls.clone(),
    })));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(RawTrace {
        inner: NativeRamBackend::new(backing.clone(), 0, backing.lock().unwrap().size()),
        calls: raw_calls,
    })));
    let core = RiscvCore::new_with_physical_ports(
        typed_handle(instruction_legacy),
        typed_handle(data_legacy.clone()),
        instruction_port,
        data_port,
    );
    (core, data_legacy, backing)
}

fn clear_reservation() {
    ruscv_sim::execute::clear_reservation();
}

#[test]
fn ordinary_store_and_fp_store_interleave_with_unchanged_legacy_lr_sc() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 1).unwrap();
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut core, _legacy, backing) = legacy_core(
        backing,
        &[lr(3, 1), store(2, 1, 3, 0), sc(4, 1, 2)],
        legacy_calls.clone(),
        false,
        raw_calls.clone(),
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 7;

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(legacy_calls.lock().unwrap().as_slice(), ["read_dword@0x80"]);
    assert!(raw_calls
        .lock()
        .unwrap()
        .iter()
        .all(|call| { call.0 == AccessCategory::Fetch }));

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        backing.lock().unwrap().read_dword(0x80).unwrap(),
        7,
        "raw ordinary store is visible to the legacy domain"
    );
    assert_eq!(
        raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1
    );

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        core.state().regs[4],
        0,
        "baseline reservation survives scalar store"
    );
    assert_eq!(
        legacy_calls.lock().unwrap().as_slice(),
        ["read_dword@0x80", "write_dword@0x80"]
    );
    assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 7);

    // A separate FP store follows the same ordinary raw route and still does
    // not add an invalidation mechanism to the retained legacy singleton.
    clear_reservation();
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 3).unwrap();
    let (mut core, _legacy, backing) = legacy_core(
        backing,
        &[
            lr(3, 1),
            // FSD f2, 0(x1)
            (2u32 << 20) | (1u32 << 15) | (0b011 << 12) | 0x27,
            sc(4, 1, 2),
        ],
        legacy_calls,
        false,
        raw_calls.clone(),
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().fpr.write(2, ruscv_sim::Fpr::from_bits(5));
    core.state_mut().regs[2] = 5;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 5);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[4], 0);
    assert_eq!(
        raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1,
        "FSD is ordinary raw DataWrite, not a legacy helper call"
    );
}

#[test]
fn ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 1).unwrap();
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut core, _legacy, backing) = legacy_core(
        backing,
        &[store(2, 1, 3, 0), amoadd_d(3, 1, 4), lr(5, 1)],
        legacy_calls.clone(),
        false,
        raw_calls.clone(),
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 7;
    core.state_mut().regs[4] = 3;

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[3], 7);
    assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 10);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 10);
    assert_eq!(
        legacy_calls.lock().unwrap().as_slice(),
        ["read_word@0x80", "write_word@0x80", "read_dword@0x80"]
    );
    assert_eq!(
        raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1,
        "only the ordinary store uses the raw data port"
    );
}

#[test]
fn rejected_ordinary_write_and_faulting_sc_preserve_characterized_reservation() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    backing.lock().unwrap().write_dword(0x80, 9).unwrap();
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut core, _legacy, backing) = legacy_core(
        backing,
        &[
            lr(3, 1),
            store(2, 4, 3, 0), // x4 is the rejected 0x1000 address
            sc(5, 1, 2),
        ],
        legacy_calls,
        false,
        raw_calls,
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 10;
    core.state_mut().regs[4] = 0x1000;
    core.state_mut().csr.write(machine::MTVEC, 0x40).unwrap();
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x1000
    ));
    assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 9);
    core.state_mut().pc = 8;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0);
    assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 10);

    // A write-faulting legacy SC returns before its historical clear, and the
    // next independent facade instance can still consume that global key.
    clear_reservation();
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    backing.lock().unwrap().write_dword(0x80, 11).unwrap();
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut faulting, legacy, _backing) = legacy_core(
        backing,
        &[lr(3, 1), sc(4, 1, 2)],
        legacy_calls,
        false,
        raw_calls,
    );
    faulting.reset(0, 0);
    faulting.state_mut().regs[1] = 0x80;
    faulting.state_mut().regs[2] = 12;
    faulting
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x40)
        .unwrap();
    assert!(matches!(
        faulting.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    legacy.lock().unwrap().set_fail_writes(true);
    assert!(matches!(
        faulting.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x80
    ));
    assert_eq!(faulting.state().regs[4], 0);

    let other_backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    other_backing.lock().unwrap().write_dword(0x80, 11).unwrap();
    let other_legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let other_raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut after_fault, _other_legacy, other_backing) = legacy_core(
        other_backing,
        &[sc(5, 1, 2)],
        other_legacy_calls,
        false,
        other_raw_calls,
    );
    after_fault.reset(0, 0);
    after_fault.state_mut().regs[1] = 0x80;
    after_fault.state_mut().regs[2] = 13;
    assert!(matches!(
        after_fault.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(after_fault.state().regs[5], 0);
    assert_eq!(other_backing.lock().unwrap().read_dword(0x80).unwrap(), 13);
}

#[test]
fn legacy_lr_survives_reset_and_replacement_storage_with_global_key_behavior() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let first_ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    first_ram.lock().unwrap().write_dword(0x80, 1).unwrap();
    let first_calls = Arc::new(Mutex::new(Vec::new()));
    let first_raw = Arc::new(Mutex::new(Vec::new()));
    let (mut first, _legacy, first_ram) =
        legacy_core(first_ram, &[lr(3, 1)], first_calls, false, first_raw);
    first.reset(0, 0);
    first.state_mut().regs[1] = 0x80;
    assert!(matches!(
        first.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));

    // Reset does not clear the characterized singleton reservation.  A
    // replacement image/core can still observe the exact address key.
    first.reset(0, 0);
    first_ram
        .lock()
        .unwrap()
        .write_word(0, 0x0000_0013)
        .unwrap();
    assert!(matches!(
        first.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));

    let second_ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    second_ram.lock().unwrap().write_dword(0x80, 2).unwrap();
    let second_calls = Arc::new(Mutex::new(Vec::new()));
    let second_raw = Arc::new(Mutex::new(Vec::new()));
    let (mut second, _legacy, second_ram) =
        legacy_core(second_ram, &[sc(4, 1, 2)], second_calls, false, second_raw);
    second.reset(0, 0);
    second.state_mut().regs[1] = 0x80;
    second.state_mut().regs[2] = 22;
    assert!(matches!(
        second.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        second.state().regs[4],
        0,
        "reservation key remains global/address-only"
    );
    assert_eq!(first_ram.lock().unwrap().read_dword(0x80).unwrap(), 1);
    assert_eq!(second_ram.lock().unwrap().read_dword(0x80).unwrap(), 22);
}

#[test]
fn public_flat_reload_replaces_storage_but_retains_legacy_lr_key() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let first = fixture::elf_with_code(&[lr(3, 1)], 0, false, false, 0x3000);
    let second = fixture::elf_with_code(&[sc(4, 1, 2)], 0, false, false, 0x3000);
    let mut simulator = RiscVSimulator::new(0x100);
    simulator.load_elf(&first).unwrap();
    let old_memory = simulator.memory().clone();
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_dword(0x80, 1).unwrap();
    }
    simulator.state_mut().regs[1] = fixture::BASE + 0x80;
    simulator.state_mut().regs[2] = 22;
    simulator.step().unwrap();

    simulator.load_elf(&second).unwrap();
    assert!(!Arc::ptr_eq(&old_memory, simulator.memory()));
    simulator.state_mut().regs[1] = fixture::BASE + 0x80;
    simulator.state_mut().regs[2] = 22;
    simulator.step().unwrap();
    assert_eq!(simulator.state().regs[4], 0);
    assert_eq!(old_memory.lock().unwrap().read_dword(0x80).unwrap(), 1);
    assert_eq!(
        simulator.memory().lock().unwrap().read_dword(0x80).unwrap(),
        22
    );
}

#[test]
fn legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    clear_reservation();
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls = Arc::new(Mutex::new(Vec::new()));
    let (mut core, instruction_legacy, backing) = legacy_core(
        backing,
        &[load(5, 1, 3, 0), 0x0000_0013],
        legacy_calls,
        false,
        raw_calls,
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    {
        let mut instruction = instruction_legacy.lock().unwrap();
        instruction.write_word(0, 0x0070_0293).unwrap(); // ADDI x5, x0, 7
    }
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        core.state().regs[5],
        7,
        "legacy instruction write reaches raw fetch"
    );

    {
        let mut instruction = instruction_legacy.lock().unwrap();
        instruction.write_word(0, load(5, 1, 3, 0)).unwrap();
    }
    backing.lock().unwrap().write_dword(0x80, 0x1234).unwrap();
    core.state_mut().pc = 0;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        core.state().regs[5],
        0x1234,
        "legacy RAM write reaches raw load"
    );

    let signature_value = 0x8877_6655_4433_2211u64;
    instruction_legacy
        .lock()
        .unwrap()
        .write_dword(0x40, signature_value)
        .unwrap();
    core.state_mut().regs[1] = 0x40;
    core.state_mut().pc = 0;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], signature_value);
    let signature_memory: SharedMemory = typed_handle(instruction_legacy.clone());
    let signature = SignatureInfo {
        vaddr: 0x40,
        size: 8,
        file_offset: 0,
    };
    assert_eq!(
        dump_signature(&signature_memory, Some(&signature)).unwrap(),
        Some(signature_value.to_le_bytes().to_vec()),
        "signature inspection sees the same legacy write as the raw load"
    );

    let simulator = RiscVSimulator::new(0x100);
    simulator
        .write_mem(0x80, &[0xaa, 0xbb, 0xcc, 0xdd])
        .unwrap();
    assert_eq!(
        simulator.read_mem(0x80, 4).unwrap(),
        vec![0xaa, 0xbb, 0xcc, 0xdd]
    );
}

fn legacy_mmio_scenario() {
    clear_reservation();
    let instruction = Arc::new(Mutex::new(SimpleMemory::new(0x20)));
    instruction.lock().unwrap().write_word(0, lr(3, 1)).unwrap();
    instruction
        .lock()
        .unwrap()
        .write_word(4, sc(4, 1, 2))
        .unwrap();
    let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let copy = observed.clone();
    let bus = Arc::new(Mutex::new(SystemBus::new(
        Arc::new(Mutex::new(SimpleMemory::new(0))),
        uart,
        0,
        0,
    )));
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| copy.lock().unwrap().push(value));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(instruction.clone(), 0, 0x20),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        typed_handle(instruction.clone()),
        typed_handle(bus.clone()),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = SYSTEM_BUS_HTIF_BASE + 4;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(*observed.lock().unwrap(), vec![0x1234_5678_9abc_def0]);
}

#[test]
fn successful_legacy_sc_width_debt_mmio_callback_is_retained() {
    let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
    legacy_mmio_scenario();
}

struct ChildResult {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn read_child_output(child: &mut Child) -> (String, String) {
    let mut stdout = Vec::new();
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let mut stderr = Vec::new();
    child
        .stderr
        .as_mut()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    (
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn run_child() -> ChildResult {
    let executable = std::env::current_exe().unwrap();
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg(CHILD_TEST)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD_ENV, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let (stdout, stderr) = read_child_output(&mut child);
            return ChildResult {
                status,
                stdout,
                stderr,
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().unwrap();
            let (stdout, stderr) = read_child_output(&mut child);
            panic!("legacy lock child exceeded {CHILD_TIMEOUT:?}: {status}\n{stdout}\n{stderr}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn legacy_lock_reentry_child() {
    if std::env::var_os(CHILD_ENV).is_some() {
        let _serial = RESERVATION_FIXTURE_LOCK.lock().unwrap();
        let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
        backing.lock().unwrap().write_dword(0x80, 1).unwrap();
        let instruction = Arc::new(Mutex::new(SimpleMemory::new(0x20)));
        instruction
            .lock()
            .unwrap()
            .write_word(0, store(2, 1, 3, 0))
            .unwrap();
        instruction.lock().unwrap().write_word(4, lr(3, 1)).unwrap();
        instruction
            .lock()
            .unwrap()
            .write_word(8, sc(4, 1, 2))
            .unwrap();
        let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
        let bus = Arc::new(Mutex::new(SystemBus::new(backing.clone(), uart, 0, 0x100)));
        let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(instruction.clone(), 0, 0x20),
        )));
        let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeSystemBusBackend::new(bus.clone()),
        )));
        let mut core = RiscvCore::new_with_physical_ports(
            typed_handle(instruction),
            typed_handle(bus),
            instruction_port,
            data_port,
        );
        core.reset(0, 0);
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[2] = 2;
        for _ in 0..3 {
            assert!(matches!(
                core.step_outcome(),
                StepOutcome::InstructionRetired(_)
            ));
        }
        assert_eq!(core.state().regs[4], 0);
        assert_eq!(backing.lock().unwrap().read_dword(0x80).unwrap(), 2);
        return;
    }

    let result = run_child();
    assert!(
        result.status.success(),
        "lock-domain child failed: {}\n{}",
        result.stdout,
        result.stderr
    );
}
