//! A7 T3 compatibility bridge tests, rewritten for the A8 T3 atomic profile.
//!
//! Port-configured cores now route AMO/LR/SC through the validated data port
//! as one atomic envelope per instruction (dev-plan §7.1/§7.2); the old
//! `MemoryInterface` helper route and the process-global reservation
//! singleton are gone.  These fixtures assert the characterized *new*
//! behavior of the formerly legacy rows: exactly-one-envelope dispatch,
//! committed-write-aware SC outcomes, per-Hart reservation clearing on
//! reset/reload, and the approved HTIF atomic (D-c) policy.  The ledger
//! `docs/verification/a8-fixture-reclassification.md` lists which rows
//! flipped and which harnesses retired.

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
    AccessCategory, AccessWidth, AtomicAccessKind, AtomicBackend, AtomicBackendResult,
    AtomicRequest, AtomicRequestDescriptor, NativeRamBackend, NativeSystemBusBackend,
    PhysicalBackend, PhysicalBackendError, PhysicalBackendResult, PhysicalRequest,
    PhysicalTargetRejectionReason, ValidatedPhysicalAccess,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type SharedRam = Arc<Mutex<SimpleMemory>>;
type SharedMemory = Arc<Mutex<dyn MemoryInterface + Send + Sync>>;
type RawCall = (AccessCategory, AccessWidth, u64, Option<Vec<u8>>);
type RawCalls = Arc<Mutex<Vec<RawCall>>>;
type AtomicCalls = Arc<Mutex<Vec<AtomicRequestDescriptor>>>;

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
    // funct5 = 00011 is SC; `funct3` selects the span width.
    amo_raw(0b00011, 0b010, rd, rs1, rs2)
}

fn amoswap_d(rd: u8, rs1: u8, rs2: u8) -> u32 {
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
/// order while the core owns this wrapper's outer mutex.  Port-configured
/// atomics no longer reach it, but ordinary legacy handles still can.
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

/// A physical backend double that records ordinary raw calls and atomic
/// envelope descriptors, then forwards to the RAM target.  `fail_atomic`
/// injects one target rejection for the next atomic envelope without
/// touching storage, modeling an SC write-side failure for fault-retention
/// coverage.
struct RawTrace {
    inner: NativeRamBackend,
    calls: RawCalls,
    atomic_calls: AtomicCalls,
    fail_atomic: Arc<AtomicBool>,
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

impl AtomicBackend for RawTrace {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.atomic_calls.lock().unwrap().push(request.descriptor());
        if self.fail_atomic.swap(false, Ordering::SeqCst) {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                "injected atomic target rejection",
            ));
        }
        self.inner.transact_atomic(request)
    }
}

struct LegacyFixture {
    core: RiscvCore,
    legacy_calls: Arc<Mutex<Vec<String>>>,
    raw_calls: RawCalls,
    atomic_calls: AtomicCalls,
    fail_atomic: Arc<AtomicBool>,
    backing: SharedRam,
}

/// Builds the standard port-configured Hart with a traced legacy typed view
/// for host inspection. Despite the historical helper name, guest atomics use
/// the physical envelope; `RiscvCore::new` remains the separate, explicitly
/// non-conforming typed compatibility route.
fn legacy_core(backing: SharedRam, program: &[u32]) -> LegacyFixture {
    {
        let mut ram = backing.lock().unwrap();
        for (index, instruction) in program.iter().copied().enumerate() {
            ram.write_word(index as u64 * 4, instruction).unwrap();
        }
    }
    let legacy_calls = Arc::new(Mutex::new(Vec::new()));
    let raw_calls: RawCalls = Arc::new(Mutex::new(Vec::new()));
    let atomic_calls: AtomicCalls = Arc::new(Mutex::new(Vec::new()));
    let fail_atomic = Arc::new(AtomicBool::new(false));
    let instruction_legacy = Arc::new(Mutex::new(LegacyMemory::new(
        backing.clone(),
        Arc::new(Mutex::new(Vec::new())),
        false,
    )));
    let data_legacy = Arc::new(Mutex::new(LegacyMemory::new(
        backing.clone(),
        legacy_calls.clone(),
        false,
    )));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(RawTrace {
        inner: NativeRamBackend::new(backing.clone(), 0, backing.lock().unwrap().size()),
        calls: raw_calls.clone(),
        atomic_calls: Arc::new(Mutex::new(Vec::new())),
        fail_atomic: Arc::new(AtomicBool::new(false)),
    })));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(RawTrace {
        inner: NativeRamBackend::new(backing.clone(), 0, backing.lock().unwrap().size()),
        calls: raw_calls.clone(),
        atomic_calls: atomic_calls.clone(),
        fail_atomic: fail_atomic.clone(),
    })));
    let core = RiscvCore::new_with_physical_ports(
        typed_handle(instruction_legacy),
        typed_handle(data_legacy),
        instruction_port,
        data_port,
    );
    LegacyFixture {
        core,
        legacy_calls,
        raw_calls,
        atomic_calls,
        fail_atomic,
        backing,
    }
}

fn retired(core: &mut RiscvCore) {
    assert!(
        matches!(core.step_outcome(), StepOutcome::InstructionRetired(_)),
        "expected a retired instruction"
    );
}

#[test]
fn ordinary_store_and_fp_store_interleave_with_port_route_lr_sc() {
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 1).unwrap();
    let mut fixture = legacy_core(backing, &[lr(3, 1), store(2, 1, 3, 0), sc(4, 1, 2)]);
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    fixture.core.state_mut().regs[2] = 7;

    retired(&mut fixture.core);
    // The LR issued exactly one atomic envelope on the data port and never
    // touched the legacy typed handle.
    assert!(fixture.legacy_calls.lock().unwrap().is_empty());
    assert_eq!(fixture.atomic_calls.lock().unwrap().len(), 1);
    assert!(fixture
        .raw_calls
        .lock()
        .unwrap()
        .iter()
        .all(|call| call.0 == AccessCategory::Fetch));

    retired(&mut fixture.core);
    assert_eq!(fixture.backing.lock().unwrap().read_dword(0x80).unwrap(), 7);
    assert_eq!(
        fixture
            .raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1
    );

    retired(&mut fixture.core);
    // The interleaving committed store bumped the snapshot, so the SC
    // envelope returns conditional failure: rd = 1 and no write.
    assert_eq!(
        fixture.core.state().regs[4],
        1,
        "a committed overlapping ordinary store fails the conditional SC"
    );
    assert_eq!(fixture.backing.lock().unwrap().read_dword(0x80).unwrap(), 7);
    assert_eq!(
        fixture.atomic_calls.lock().unwrap().len(),
        2,
        "LR and the executed SC each issue exactly one envelope"
    );

    // An FP store follows the same ordinary raw route and also commits, so
    // it likewise breaks a reservation taken across it.
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 3).unwrap();
    let mut fixture = legacy_core(
        backing,
        &[
            lr(3, 1),
            // FSD f2, 0(x1)
            (2u32 << 20) | (1u32 << 15) | (0b011 << 12) | 0x27,
            sc(4, 1, 2),
        ],
    );
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    fixture
        .core
        .state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(5));
    fixture.core.state_mut().regs[2] = 5;
    retired(&mut fixture.core);
    retired(&mut fixture.core);
    assert_eq!(fixture.backing.lock().unwrap().read_dword(0x80).unwrap(), 5);
    retired(&mut fixture.core);
    assert_eq!(
        fixture.core.state().regs[4],
        1,
        "a committed overlapping FP store fails the conditional SC"
    );
    assert_eq!(
        fixture
            .raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1,
        "FSD is ordinary raw DataWrite, not an atomic envelope"
    );
}

#[test]
fn ordinary_store_then_port_route_amo_and_lr_share_one_domain() {
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x200)));
    backing.lock().unwrap().write_dword(0x80, 1).unwrap();
    let mut fixture = legacy_core(backing, &[store(2, 1, 3, 0), amoswap_d(3, 1, 4), lr(5, 1)]);
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    fixture.core.state_mut().regs[2] = 7;
    fixture.core.state_mut().regs[4] = 3;

    retired(&mut fixture.core);
    retired(&mut fixture.core);

    // AMOSWAP.D returned the committed store value and swapped in rs2.
    assert_eq!(fixture.core.state().regs[3], 7);
    assert_eq!(fixture.backing.lock().unwrap().read_dword(0x80).unwrap(), 3);
    retired(&mut fixture.core);
    assert_eq!(fixture.core.state().regs[5], 3);
    assert!(
        fixture.legacy_calls.lock().unwrap().is_empty(),
        "atomics stay on the port; the legacy typed handle is untouched"
    );
    let kinds: Vec<AtomicAccessKind> = fixture
        .atomic_calls
        .lock()
        .unwrap()
        .iter()
        .map(|call| call.kind)
        .collect();
    assert_eq!(
        kinds,
        [AtomicAccessKind::Rmw, AtomicAccessKind::LoadReserved]
    );
    assert_eq!(
        fixture
            .raw_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.0 == AccessCategory::DataWrite)
            .count(),
        1,
        "only the ordinary store uses a raw data-write request"
    );
}

#[test]
fn rejected_write_keeps_reservation_and_faulting_sc_retains_it() {
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    backing.lock().unwrap().write_dword(0x80, 9).unwrap();
    let mut fixture = legacy_core(
        backing,
        &[
            lr(3, 1),
            store(2, 4, 3, 0), // x4 is the rejected 0x1000 address
            sc(5, 1, 2),
        ],
    );
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    fixture.core.state_mut().regs[2] = 10;
    fixture.core.state_mut().regs[4] = 0x1000;
    fixture
        .core
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x40)
        .unwrap();
    retired(&mut fixture.core);
    assert!(matches!(
        fixture.core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x1000
    ));
    assert_eq!(
        fixture.backing.lock().unwrap().read_dword(0x80).unwrap(),
        9,
        "a target-rejected ordinary write commits nothing"
    );
    fixture.core.state_mut().pc = 8;
    retired(&mut fixture.core);
    assert_eq!(
        fixture.core.state().regs[5],
        0,
        "a rejected write does not bump the snapshot, so SC still succeeds"
    );
    assert_eq!(
        fixture.backing.lock().unwrap().read_dword(0x80).unwrap(),
        10
    );

    // A write-faulting SC keeps the Hart's reservation (approved
    // faulting-SC retain): the trap discards the staged consumption, so the
    // next SC on the same Hart still succeeds.
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    backing.lock().unwrap().write_dword(0x80, 11).unwrap();
    let mut fixture = legacy_core(backing, &[lr(3, 1), sc(4, 1, 2)]);
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    fixture.core.state_mut().regs[2] = 12;
    fixture
        .core
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x40)
        .unwrap();
    retired(&mut fixture.core);
    fixture.fail_atomic.store(true, Ordering::SeqCst);
    assert!(matches!(
        fixture.core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x80
    ));
    assert_eq!(fixture.core.state().regs[4], 0);
    fixture.core.state_mut().pc = 4;
    retired(&mut fixture.core);
    assert_eq!(
        fixture.core.state().regs[4],
        0,
        "the retained reservation lets the retried SC commit"
    );
    assert_eq!(
        fixture.backing.lock().unwrap().read_dword(0x80).unwrap(),
        12
    );

    // A different Hart holds no reservation: SC at valid RAM still submits
    // one envelope, then fails conditionally even at the same address.
    let other_backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    other_backing.lock().unwrap().write_dword(0x80, 11).unwrap();
    let mut after_fault = legacy_core(other_backing, &[sc(5, 1, 2)]);
    after_fault.core.reset(0, 0);
    after_fault.core.state_mut().regs[1] = 0x80;
    after_fault.core.state_mut().regs[2] = 13;
    retired(&mut after_fault.core);
    assert_eq!(
        after_fault.core.state().regs[5],
        1,
        "reservations are per-Hart; another core's SC fails"
    );
    assert_eq!(
        after_fault
            .atomic_calls
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.kind)
            .collect::<Vec<_>>(),
        [AtomicAccessKind::StoreConditional],
        "the unreserved SC reaches the valid RAM target once"
    );
    assert_eq!(
        after_fault
            .backing
            .lock()
            .unwrap()
            .read_dword(0x80)
            .unwrap(),
        11
    );
}

#[test]
fn per_hart_reservation_clears_on_reset_and_does_not_cross_facades() {
    let first_ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    first_ram.lock().unwrap().write_dword(0x80, 1).unwrap();
    let mut first = legacy_core(first_ram, &[lr(3, 1), sc(4, 1, 2)]);
    first.core.reset(0, 0);
    first.core.state_mut().regs[1] = 0x80;
    first.core.state_mut().regs[2] = 22;
    retired(&mut first.core);

    // Reset installs a fresh CoreState, clearing the reservation.
    first.core.reset(0, 0);
    first.core.state_mut().regs[1] = 0x80;
    first.core.state_mut().regs[2] = 22;
    first.core.state_mut().pc = 4;
    retired(&mut first.core);
    assert_eq!(
        first.core.state().regs[4],
        1,
        "reset clears the per-Hart reservation; SC fails with rd = 1"
    );
    assert_eq!(
        first
            .atomic_calls
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.kind)
            .collect::<Vec<_>>(),
        [
            AtomicAccessKind::LoadReserved,
            AtomicAccessKind::StoreConditional
        ],
        "reset does not suppress the later SC target check"
    );
    assert_eq!(first.backing.lock().unwrap().read_dword(0x80).unwrap(), 1);

    // A replacement core on different storage holds no reservation either:
    // the record lives in this Hart's CoreState, not in a global singleton.
    let second_ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    second_ram.lock().unwrap().write_dword(0x80, 2).unwrap();
    let mut second = legacy_core(second_ram, &[sc(4, 1, 2)]);
    second.core.reset(0, 0);
    second.core.state_mut().regs[1] = 0x80;
    second.core.state_mut().regs[2] = 22;
    retired(&mut second.core);
    assert_eq!(
        second.core.state().regs[4],
        1,
        "the reservation record is Hart-owned, not address-keyed global"
    );
    assert_eq!(
        second
            .atomic_calls
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.kind)
            .collect::<Vec<_>>(),
        [AtomicAccessKind::StoreConditional],
        "the fresh Hart's SC still validates the valid RAM target"
    );
    assert_eq!(second.backing.lock().unwrap().read_dword(0x80).unwrap(), 2);
}

#[test]
fn public_flat_reload_replaces_storage_and_clears_the_reservation() {
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
    assert_eq!(
        simulator.state().regs[4],
        1,
        "image reload installs a fresh core, clearing the reservation"
    );
    assert_eq!(old_memory.lock().unwrap().read_dword(0x80).unwrap(), 1);
}

#[test]
fn legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection() {
    let backing = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let program = [load(5, 1, 3, 0), 0x0000_0013];
    let mut fixture = legacy_core(backing, &program);
    fixture.core.reset(0, 0);
    fixture.core.state_mut().regs[1] = 0x80;
    let instruction_legacy = {
        // Recover the typed instruction handle by wrapping the same RAM.
        fixture.backing.clone()
    };
    {
        let mut ram = instruction_legacy.lock().unwrap();
        ram.write_word(0, 0x0070_0293).unwrap(); // ADDI x5, x0, 7
    }
    retired(&mut fixture.core);
    assert_eq!(
        fixture.core.state().regs[5],
        7,
        "backing-RAM instruction write reaches raw fetch"
    );

    {
        let mut ram = instruction_legacy.lock().unwrap();
        ram.write_word(0, load(5, 1, 3, 0)).unwrap();
    }
    instruction_legacy
        .lock()
        .unwrap()
        .write_dword(0x80, 0x1234)
        .unwrap();
    fixture.core.state_mut().pc = 0;
    retired(&mut fixture.core);
    assert_eq!(
        fixture.core.state().regs[5],
        0x1234,
        "backing-RAM write reaches raw load"
    );

    let signature_value = 0x8877_6655_4433_2211u64;
    instruction_legacy
        .lock()
        .unwrap()
        .write_dword(0x40, signature_value)
        .unwrap();
    fixture.core.state_mut().regs[1] = 0x40;
    fixture.core.state_mut().pc = 0;
    retired(&mut fixture.core);
    assert_eq!(fixture.core.state().regs[5], signature_value);
    let signature_memory: SharedMemory = typed_handle(instruction_legacy.clone());
    let signature = SignatureInfo {
        vaddr: 0x40,
        size: 8,
        file_offset: 0,
    };
    assert_eq!(
        dump_signature(&signature_memory, Some(&signature)).unwrap(),
        Some(signature_value.to_le_bytes().to_vec()),
        "signature inspection sees the same write as the raw load"
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

#[test]
fn htif_atomic_policy_rejects_lr_sc_and_commits_one_rmw_callback() {
    // Approved D-c on the native bus: LR and SC at the HTIF endpoint are
    // target rejections before any callback or mutation, while one RMW
    // envelope fires the callback exactly once inside the critical section.
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
    let instruction = Arc::new(Mutex::new(SimpleMemory::new(0x20)));
    {
        let mut ram = instruction.lock().unwrap();
        ram.write_word(0, lr(3, 1)).unwrap();
        ram.write_word(4, sc(4, 1, 2)).unwrap();
        ram.write_word(8, amoswap_d(5, 1, 2)).unwrap();
    }
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
    core.state_mut().csr.write(machine::MTVEC, 0x40).unwrap();

    // LR at the HTIF endpoint: the backend rejects the load-reserved kind
    // before any mutation; the Hart enters a load access fault.
    core.state_mut().regs[1] = SYSTEM_BUS_HTIF_BASE;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::LoadAccessFault
                && trap.mtval == SYSTEM_BUS_HTIF_BASE
    ));
    assert!(observed.lock().unwrap().is_empty());

    // SC with no reservation still submits its StoreConditional envelope;
    // D-c rejects the HTIF target before conditional failure or callback.
    core.state_mut().pc = 4;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    core.state_mut().regs[4] = 0xbeef;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ExceptionCause::StoreAccessFault
                && trap.mtval == SYSTEM_BUS_HTIF_BASE
    ));
    assert_eq!(
        core.state().regs[4],
        0xbeef,
        "faulting SC does not write rd"
    );
    assert!(observed.lock().unwrap().is_empty());

    // AMOSWAP.D at the endpoint: one indivisible envelope whose old value is
    // the endpoint's zero read and whose write fires the callback once.
    core.state_mut().pc = 8;
    retired(&mut core);
    assert_eq!(core.state().regs[5], 0, "the HTIF endpoint reads zero");
    assert_eq!(
        *observed.lock().unwrap(),
        vec![0x1234_5678_9abc_def0],
        "one RMW envelope fires exactly one callback with the transformed value"
    );
}
