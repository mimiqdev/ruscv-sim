//! Blocking TLM U2: atomic-extension parity and negative controls.

use super::*;
use crate::core::{ExceptionCause, RiscvCore, StepOutcome, Trap};
use crate::csr::machine;
use crate::hart_amods;
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
use crate::mmu::physical::PhysicalMemoryInterface;
use crate::mmu::pte::{PagePermissions, PageTableEntry};
use crate::mmu::translator::AddressTranslator;
use crate::mmu::{AccessType, MmuConfig};
use crate::physical::{
    AmoWidth, AtomicAccessResult, AtomicOrdering, AtomicRequest, AtomicReservationContext,
    ConditionalStatus, NativeRamBackend, PhysicalRequest, PhysicalSpan, PhysicalWidth,
    ValidatedAtomicAccess, ValidatedPhysicalAccess,
};
use crate::ram::RamSlot;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const BASE: u64 = 0x8000_0000;
const SIZE: usize = 0x10_000;
const ORDERING: AtomicOrdering = AtomicOrdering {
    aq: false,
    rl: false,
};

fn native_slot() -> RamSlot {
    RamSlot::native(Arc::new(Mutex::new(SimpleMemory::new(SIZE))), BASE, SIZE)
}

/// A zero-delay target that counts blocking and atomic transactions, and can
/// refuse the atomic extension with the contract's command error.
struct CountingTarget {
    inner: ZeroDelayRam,
    transactions: AtomicUsize,
    atomics: AtomicUsize,
    reject_atomics: bool,
}

impl CountingTarget {
    fn new(reject_atomics: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: ZeroDelayRam::new(),
            transactions: AtomicUsize::new(0),
            atomics: AtomicUsize::new(0),
            reject_atomics,
        })
    }

    fn transactions(&self) -> usize {
        self.transactions.load(Ordering::SeqCst)
    }

    fn atomics(&self) -> usize {
        self.atomics.load(Ordering::SeqCst)
    }
}

impl BlockingTarget for CountingTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::ZERO
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        if transaction.atomic_extension().is_some() {
            self.atomics.fetch_add(1, Ordering::SeqCst);
            if self.reject_atomics {
                transaction.set_status(BlockingResponseStatus::CommandError);
                return;
            }
        }
        self.inner.b_transport(transaction);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.inner.transport_reset()
    }
}

fn tlm_slot(target: &Arc<CountingTarget>) -> RamSlot {
    let slot = RamSlot::tlm(target.clone(), BASE, SIZE);
    slot.begin_window().unwrap();
    slot
}

fn value_bytes(value: u64, width: AmoWidth) -> Vec<u8> {
    value.to_le_bytes()[..width.bytes()].to_vec()
}

fn read_u64(slot: &RamSlot, offset: u64, width: AmoWidth) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..width.bytes()].copy_from_slice(&slot.host_read(offset, width.bytes()).unwrap());
    u64::from_le_bytes(bytes)
}

#[test]
fn every_amo_matches_native_for_word_and_doubleword() {
    let old_patterns = [0x0000_0000u64, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff];
    let operand_patterns = [0u64, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff];
    for width in [AmoWidth::Word, AmoWidth::Doubleword] {
        for operation in hart_amods::ALL_OPERATIONS {
            for &old in &old_patterns {
                for &operand in &operand_patterns {
                    let old_bytes = value_bytes(old, width);
                    let operand_bytes = value_bytes(operand, width);
                    let expected =
                        hart_amods::apply(operation, width, &old_bytes, &operand_bytes).unwrap();

                    let native = native_slot();
                    let target = CountingTarget::new(false);
                    let tlm = tlm_slot(&target);
                    native.program(0x20, &old_bytes).unwrap();
                    tlm.program(0x20, &old_bytes).unwrap();
                    let mut native_port = ValidatedAtomicAccess::new(native.clone());
                    let mut tlm_port = ValidatedAtomicAccess::new(tlm.clone());

                    let request = AtomicRequest::rmw(
                        BASE + 0x20,
                        width.physical_width(),
                        ORDERING,
                        &operand_bytes,
                        hart_amods::transform(operation, width),
                    )
                    .unwrap();
                    let native_response = native_port.access_atomic(request).unwrap();
                    let request = AtomicRequest::rmw(
                        BASE + 0x20,
                        width.physical_width(),
                        ORDERING,
                        &operand_bytes,
                        hart_amods::transform(operation, width),
                    )
                    .unwrap();
                    let tlm_response = tlm_port.access_atomic(request).unwrap();

                    assert_eq!(
                        native_response.old_bytes(),
                        tlm_response.old_bytes(),
                        "{operation} {width:?} old={old:#x} operand={operand:#x}: old value differs"
                    );
                    assert_eq!(
                        tlm_response.old_bytes(),
                        Some(old_bytes.as_slice()),
                        "{operation} {width:?} old={old:#x} operand={operand:#x}: wrong old value"
                    );
                    let native_after = native.host_read(0x20, width.bytes()).unwrap();
                    let tlm_after = tlm.host_read(0x20, width.bytes()).unwrap();
                    assert_eq!(
                        tlm_after, native_after,
                        "{operation} {width:?} old={old:#x} operand={operand:#x}: memory differs"
                    );
                    assert_eq!(
                        tlm_after,
                        expected[..width.bytes()].to_vec(),
                        "{operation} {width:?} old={old:#x} operand={operand:#x}: wrong result"
                    );
                    assert_eq!(
                        target.atomics(),
                        1,
                        "{operation} {width:?}: an AMO must be exactly one atomic transaction"
                    );
                    assert_eq!(
                        target.transactions(),
                        1,
                        "{operation} {width:?}: an AMO must be exactly one transaction"
                    );
                }
            }
        }
    }
}

fn lr_context(
    port: &mut ValidatedAtomicAccess<RamSlot>,
    address: u64,
) -> (Vec<u8>, AtomicReservationContext) {
    let request =
        AtomicRequest::load_reserved(address, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(request).unwrap();
    let old = response.old_bytes().unwrap().to_vec();
    let snapshot = response.snapshot().expect("load-reserved snapshot");
    (
        old,
        AtomicReservationContext {
            reserved: PhysicalSpan {
                paddr: address,
                width: PhysicalWidth::Doubleword,
            },
            snapshot,
        },
    )
}

fn sc(
    port: &mut ValidatedAtomicAccess<RamSlot>,
    address: u64,
    width: PhysicalWidth,
    payload: &[u8],
    reservation: Option<AtomicReservationContext>,
) -> ConditionalStatus {
    let request =
        AtomicRequest::store_conditional(address, width, ORDERING, payload, reservation).unwrap();
    port.access_atomic(request)
        .unwrap()
        .conditional_status()
        .expect("store-conditional status")
}

fn lr_sc_scenario(slot: &RamSlot, scenario: usize) -> (Vec<ConditionalStatus>, Vec<u8>) {
    const ADDRESS: u64 = BASE + 0x40;
    let mut port = ValidatedAtomicAccess::new(slot.clone());
    let initial = 0x1111_2222_3333_4444u64.to_le_bytes();
    slot.program(0x40, &initial).unwrap();
    let (old, context) = lr_context(&mut port, ADDRESS);
    assert_eq!(old, initial);
    let payload = 0xaaaa_bbbb_cccc_ddddu64.to_le_bytes();
    let mut statuses = Vec::new();
    match scenario {
        0 => statuses.push(sc(
            &mut port,
            ADDRESS,
            PhysicalWidth::Doubleword,
            &payload,
            Some(context),
        )),
        1 => {
            slot.program(0x40, &0xdead_beefu64.to_le_bytes()).unwrap();
            statuses.push(sc(
                &mut port,
                ADDRESS,
                PhysicalWidth::Doubleword,
                &payload,
                Some(context),
            ));
        }
        2 => {
            slot.program(0x48, &0x99u8.to_le_bytes()).unwrap();
            statuses.push(sc(
                &mut port,
                ADDRESS,
                PhysicalWidth::Doubleword,
                &payload,
                Some(context),
            ));
        }
        3 => statuses.push(sc(
            &mut port,
            ADDRESS,
            PhysicalWidth::Doubleword,
            &payload,
            None,
        )),
        4 => statuses.push(sc(
            &mut port,
            ADDRESS,
            PhysicalWidth::Word,
            &payload[..4],
            Some(context),
        )),
        5 => {
            let lr = AtomicRequest::load_reserved(ADDRESS, PhysicalWidth::Word, ORDERING).unwrap();
            let response = port.access_atomic(lr).unwrap();
            let context = AtomicReservationContext {
                reserved: PhysicalSpan {
                    paddr: ADDRESS,
                    width: PhysicalWidth::Word,
                },
                snapshot: response.snapshot().unwrap(),
            };
            statuses.push(sc(
                &mut port,
                ADDRESS,
                PhysicalWidth::Doubleword,
                &payload,
                Some(context),
            ));
        }
        _ => unreachable!(),
    }
    (statuses, slot.host_read(0x40, 8).unwrap())
}

#[test]
fn lr_and_sc_outcomes_match_native() {
    let cases = [
        (
            "reserved same-span SC succeeds",
            vec![ConditionalStatus::Success],
        ),
        (
            "overlapping committed write fails SC",
            vec![ConditionalStatus::Failure],
        ),
        (
            "non-overlapping committed write allows SC",
            vec![ConditionalStatus::Success],
        ),
        ("no reservation fails SC", vec![ConditionalStatus::Failure]),
        (
            "wider reservation covers a word SC",
            vec![ConditionalStatus::Success],
        ),
        ("uncovered wider SC fails", vec![ConditionalStatus::Failure]),
    ];
    for (index, (label, expected)) in cases.into_iter().enumerate() {
        let native = native_slot();
        let target = CountingTarget::new(false);
        let tlm = tlm_slot(&target);
        let (native_statuses, native_memory) = lr_sc_scenario(&native, index);
        let (tlm_statuses, tlm_memory) = lr_sc_scenario(&tlm, index);
        assert_eq!(native_statuses, expected, "{label}: native outcome drifted");
        assert_eq!(tlm_statuses, expected, "{label}: TLM outcome differs");
        assert_eq!(tlm_memory, native_memory, "{label}: final memory differs");
        assert!(
            target.atomics() >= 2,
            "{label}: LR/SC bypassed the atomic extension"
        );
    }
}

#[test]
fn one_transaction_per_atomic_operation() {
    for width in [AmoWidth::Word, AmoWidth::Doubleword] {
        for operation in hart_amods::ALL_OPERATIONS {
            let target = CountingTarget::new(false);
            let slot = tlm_slot(&target);
            slot.program(0x20, &value_bytes(0x1234_5678, width))
                .unwrap();
            let mut port = ValidatedAtomicAccess::new(slot);
            let operand = value_bytes(7, width);
            let request = AtomicRequest::rmw(
                BASE + 0x20,
                width.physical_width(),
                ORDERING,
                &operand,
                hart_amods::transform(operation, width),
            )
            .unwrap();
            port.access_atomic(request).unwrap();
            assert_eq!(target.transactions(), 1, "{operation} {width:?}: split AMO");
            assert_eq!(
                target.atomics(),
                1,
                "{operation} {width:?}: not one atomic transaction"
            );
        }
    }

    let target = CountingTarget::new(false);
    let slot = tlm_slot(&target);
    let mut port = ValidatedAtomicAccess::new(slot);
    let lr =
        AtomicRequest::load_reserved(BASE + 0x20, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(lr).unwrap();
    assert_eq!(target.transactions(), 1, "LR was not one transaction");
    assert_eq!(target.atomics(), 1, "LR was not one atomic transaction");
    let context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: BASE + 0x20,
            width: PhysicalWidth::Doubleword,
        },
        snapshot: response.snapshot().unwrap(),
    };
    let payload = 1u64.to_le_bytes();
    let sc = AtomicRequest::store_conditional(
        BASE + 0x20,
        PhysicalWidth::Doubleword,
        ORDERING,
        &payload,
        context,
    )
    .unwrap();
    port.access_atomic(sc).unwrap();
    assert_eq!(target.transactions(), 2, "SC was not one transaction");
    assert_eq!(target.atomics(), 2, "SC was not one atomic transaction");
}

#[test]
fn completed_atomics_are_recorded_for_committed_write_visibility() {
    let target = CountingTarget::new(false);
    let slot = tlm_slot(&target);
    let mut port = ValidatedAtomicAccess::new(slot.clone());
    assert_eq!(slot.committed_write_version(), Some(0));

    let lr =
        AtomicRequest::load_reserved(BASE + 0x60, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(lr).unwrap();
    assert_eq!(
        slot.committed_write_version(),
        Some(0),
        "a load-reserved is not a committed write"
    );

    let operand = 1u64.to_le_bytes();
    let request = AtomicRequest::rmw(
        BASE + 0x60,
        PhysicalWidth::Doubleword,
        ORDERING,
        &operand,
        hart_amods::transform(hart_amods::AmoOperation::Add, AmoWidth::Doubleword),
    )
    .unwrap();
    port.access_atomic(request).unwrap();
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a completed AMO must be recorded"
    );

    let context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: BASE + 0x60,
            width: PhysicalWidth::Doubleword,
        },
        snapshot: response.snapshot().unwrap(),
    };
    let payload = 2u64.to_le_bytes();
    let sc = AtomicRequest::store_conditional(
        BASE + 0x60,
        PhysicalWidth::Doubleword,
        ORDERING,
        &payload,
        Some(context),
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(sc).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure),
        "the AMO must invalidate the earlier reservation"
    );
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a failed SC is not a committed write"
    );

    let lr =
        AtomicRequest::load_reserved(BASE + 0x60, PhysicalWidth::Doubleword, ORDERING).unwrap();
    let response = port.access_atomic(lr).unwrap();
    let context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: BASE + 0x60,
            width: PhysicalWidth::Doubleword,
        },
        snapshot: response.snapshot().unwrap(),
    };
    let sc = AtomicRequest::store_conditional(
        BASE + 0x60,
        PhysicalWidth::Doubleword,
        ORDERING,
        &payload,
        Some(context),
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(sc).unwrap().conditional_status(),
        Some(ConditionalStatus::Success)
    );
    assert_eq!(
        slot.committed_write_version(),
        Some(2),
        "a successful SC must be recorded"
    );
}

#[test]
fn a_target_without_the_atomic_extension_yields_the_native_access_fault() {
    for (instruction, setup, expected) in [
        (
            lr_w(3, 1),
            (|core: &mut RiscvCore| {
                core.state_mut().regs[1] = BASE + 0x20;
            }) as fn(&mut RiscvCore),
            ExceptionCause::LoadAccessFault,
        ),
        (
            sc_w(3, 1, 2),
            (|core: &mut RiscvCore| {
                core.state_mut().regs[1] = BASE + 0x20;
                core.state_mut().regs[2] = 0x55;
            }) as fn(&mut RiscvCore),
            ExceptionCause::StoreAccessFault,
        ),
        (
            amo_w(0b00000, 3, 1, 2),
            (|core: &mut RiscvCore| {
                core.state_mut().regs[1] = BASE + 0x20;
                core.state_mut().regs[2] = 1;
            }) as fn(&mut RiscvCore),
            ExceptionCause::StoreAccessFault,
        ),
    ] {
        let target = CountingTarget::new(true);
        let slot = tlm_slot(&target);
        let mut core = core_with_slot(slot, instruction);
        setup(&mut core);
        let outcome = core.step_outcome();
        assert!(
            matches!(
                outcome,
                StepOutcome::TrapEntered(trap)
                    if trap.cause == Trap::Exception(expected)
                        && trap.mtval == BASE + 0x20
            ),
            "an extension-less target must give {expected:?}, got {outcome:?}"
        );
        assert_eq!(target.atomics(), 1, "the atomic never reached the target");
    }
}

#[test]
fn word_amo_old_value_sign_extends_through_the_tlm_route() {
    for tlm in [false, true] {
        let slot = if tlm {
            let target = CountingTarget::new(false);
            tlm_slot(&target)
        } else {
            native_slot()
        };
        slot.program(0x20, &0x8000_0000u32.to_le_bytes()).unwrap();
        let mut core = core_with_slot(slot.clone(), amo_w(0b00000, 3, 1, 2));
        core.state_mut().regs[1] = BASE + 0x20;
        core.state_mut().regs[2] = 0;
        let outcome = core.step_outcome();
        assert!(
            matches!(outcome, StepOutcome::InstructionRetired(_)),
            "{tlm}: AMO.W did not retire: {outcome:?}"
        );
        assert_eq!(
            core.state().regs[3],
            0xffff_ffff_8000_0000,
            "{tlm}: AMO.W did not sign-extend its old value"
        );
        assert_eq!(read_u64(&slot, 0x20, AmoWidth::Word), 0x8000_0000);
    }
}

#[test]
fn sv39_ad_update_uses_one_lr_and_one_sc_transaction() {
    const ROOT_PPN: u64 = 0x80000;
    const DATA_PPN: u64 = 0x80004;
    const LEAF_OFFSET: u64 = 0x2008;
    const VA: u64 = 0x1000;
    let target = CountingTarget::new(false);
    let slot = tlm_slot(&target);
    let pointer = |ppn: u64| PageTableEntry::new_pointer(ppn).bits().to_le_bytes();
    slot.program(0, &pointer(ROOT_PPN + 1)).unwrap();
    slot.program(0x1000, &pointer(ROOT_PPN + 2)).unwrap();
    let leaf = PageTableEntry::new_leaf(DATA_PPN, PagePermissions::rw(), false);
    slot.program(LEAF_OFFSET, &leaf.bits().to_le_bytes())
        .unwrap();

    let mut memory = SlotMemory { slot: slot.clone() };
    let satp = (8 << 60) | ROOT_PPN;
    let translated = AddressTranslator::new(MmuConfig::default())
        .translate_sv39_walk(
            VA,
            satp,
            AccessType::Write,
            crate::core::PrivilegeMode::Supervisor,
            0,
            &mut memory,
        )
        .unwrap();
    assert_eq!(translated, (DATA_PPN << 12) | (VA & 0xfff));
    assert_eq!(
        target.atomics(),
        2,
        "A/D must use exactly one LR and one SC"
    );
    assert_eq!(
        target.transactions(),
        5,
        "A/D walk issued an extra transaction"
    );
    let updated = PageTableEntry::from_raw(memory.read_dword(LEAF_OFFSET + BASE).unwrap());
    assert!(updated.is_accessed(), "A was not set through the TLM route");
    assert!(updated.is_dirty(), "D was not set through the TLM route");
}

struct SlotMemory {
    slot: RamSlot,
}

impl PhysicalMemoryInterface for SlotMemory {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        let request = PhysicalRequest::data_read(paddr, PhysicalWidth::Byte).unwrap();
        let response = ValidatedPhysicalAccess::new(self.slot.clone())
            .access(request)
            .map_err(|error| MemoryError::Backend(error.to_string()))?;
        Ok(response.read_bytes().unwrap()[0])
    }

    fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        let bytes = [value];
        let request = PhysicalRequest::data_write(paddr, PhysicalWidth::Byte, &bytes).unwrap();
        ValidatedPhysicalAccess::new(self.slot.clone())
            .access(request)
            .map_err(|error| MemoryError::Backend(error.to_string()))?;
        Ok(())
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        let request = PhysicalRequest::data_read(paddr, PhysicalWidth::Doubleword).unwrap();
        let response = ValidatedPhysicalAccess::new(self.slot.clone())
            .access(request)
            .map_err(|error| MemoryError::Backend(error.to_string()))?;
        Ok(u64::from_le_bytes(
            response.read_bytes().unwrap().try_into().unwrap(),
        ))
    }

    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        let bytes = value.to_le_bytes();
        let request =
            PhysicalRequest::data_write(paddr, PhysicalWidth::Doubleword, &bytes).unwrap();
        ValidatedPhysicalAccess::new(self.slot.clone())
            .access(request)
            .map_err(|error| MemoryError::Backend(error.to_string()))?;
        Ok(())
    }

    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        ValidatedAtomicAccess::new(self.slot.clone()).access_atomic(request)
    }
}

fn core_with_slot(slot: RamSlot, instruction: u32) -> RiscvCore {
    let code = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    code.lock().unwrap().write_word(0, instruction).unwrap();
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(code.clone(), 0, 0x1000),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(slot)));
    let mut core =
        RiscvCore::new_with_physical_ports(code.clone(), code, instruction_port, data_port);
    core.reset(0, 0);
    core.state_mut().csr.write(machine::MTVEC, 0x100).unwrap();
    core
}

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

fn sc_w(rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(0b00011, 0b010, rd, rs1, rs2)
}

fn amo_w(funct5: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo_raw(funct5, 0b010, rd, rs1, rs2)
}
