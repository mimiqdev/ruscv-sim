//! A6 Task 3 regressions for the integrated Hart outcome and trap boundary.

use ruscv_sim::core::{
    ExceptionCause, PrivilegeMode, RiscvCore, SimulatorFailureKind, StepOutcome,
};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::RiscVSimulator;
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::{MemoryError, MemoryInterface};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const MEMORY_SIZE: usize = 0x200;
const MTVEC: u64 = 0x80;

struct CountingMemory {
    inner: SimpleMemory,
    transactions: AtomicUsize,
}

impl CountingMemory {
    fn new(size: usize) -> Self {
        Self {
            inner: SimpleMemory::new(size),
            transactions: AtomicUsize::new(0),
        }
    }

    fn transaction_count(&self) -> usize {
        self.transactions.load(Ordering::Relaxed)
    }

    fn snapshot_word(&self, address: u64) -> Result<u32, MemoryError> {
        self.inner.read_word(address)
    }

    fn count(&self) {
        self.transactions.fetch_add(1, Ordering::Relaxed);
    }
}

impl MemoryInterface for CountingMemory {
    fn read_dword(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_dword(address)
    }

    fn read_word(&self, address: u64) -> Result<u32, MemoryError> {
        self.count();
        self.inner.read_word(address)
    }

    fn read_half(&self, address: u64) -> Result<u16, MemoryError> {
        self.count();
        self.inner.read_half(address)
    }

    fn read_byte(&self, address: u64) -> Result<u8, MemoryError> {
        self.count();
        self.inner.read_byte(address)
    }

    fn read_word_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_word_zext(address)
    }

    fn read_half_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_half_zext(address)
    }

    fn read_byte_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_byte_zext(address)
    }

    fn read_word_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_word_sext(address)
    }

    fn read_half_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_half_sext(address)
    }

    fn read_byte_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_byte_sext(address)
    }

    fn write_dword(&mut self, address: u64, value: u64) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_dword(address, value)
    }

    fn write_word(&mut self, address: u64, value: u32) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_word(address, value)
    }

    fn write_half(&mut self, address: u64, value: u16) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_half(address, value)
    }

    fn write_byte(&mut self, address: u64, value: u8) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_byte(address, value)
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}

fn core_with_program(program: &[(usize, u32)]) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(MEMORY_SIZE)));
    for &(address, instruction) in program {
        memory
            .lock()
            .unwrap()
            .write_word(address as u64, instruction)
            .unwrap();
    }
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    (core, memory)
}

fn core_with_program_and_counting_data(
    program: &[(usize, u32)],
) -> (RiscvCore, Arc<Mutex<CountingMemory>>) {
    let instruction_memory = Arc::new(Mutex::new(SimpleMemory::new(MEMORY_SIZE)));
    for &(address, instruction) in program {
        instruction_memory
            .lock()
            .unwrap()
            .write_word(address as u64, instruction)
            .unwrap();
    }
    let data_memory = Arc::new(Mutex::new(CountingMemory::new(MEMORY_SIZE)));
    let mut core = RiscvCore::new(instruction_memory, data_memory.clone());
    core.reset(0, 0);
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    (core, data_memory)
}

fn addi(rd: u8, rs1: u8, immediate: i32) -> u32 {
    (((immediate as u32) & 0xfff) << 20) | ((rs1 as u32) << 15) | ((rd as u32) << 7) | 0x13
}

fn load_double(rd: u8, rs1: u8, immediate: i32) -> u32 {
    (((immediate as u32) & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn store_double(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = (immediate as u32) & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn amo_word(rd: u8, rs1: u8, rs2: u8, funct5: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (0b010 << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn lr_word(rd: u8, rs1: u8) -> u32 {
    amo_word(rd, rs1, 0, 0b00010)
}

fn op32(rd: u8, rs1: u8, rs2: u8, funct3: u8, funct7: u8) -> u32 {
    ((funct7 as u32) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x3b
}

fn jalr(rd: u8, rs1: u8, immediate: i32) -> u32 {
    (((immediate as u32) & 0xfff) << 20) | ((rs1 as u32) << 15) | ((rd as u32) << 7) | 0x67
}

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b010 << 12) | ((rd as u32) << 7) | 0x73
}

fn csrrw(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b001 << 12) | ((rd as u32) << 7) | 0x73
}

#[test]
fn normal_and_explicit_minstret_retirement_are_distinct() {
    let (mut core, _) = core_with_program(&[
        (0x00, addi(1, 0, 1)),
        (0x04, csrrs(2, machine::MINSTRET, 0)),
        (0x08, csrrw(0, machine::MINSTRET, 3)),
    ]);
    core.state_mut().regs[3] = 41;

    let first = core.step_outcome();
    assert!(
        matches!(first, StepOutcome::InstructionRetired(fact) if fact.minstret == 1 && !fact.explicit_minstret_write)
    );

    let second = core.step_outcome();
    assert!(
        matches!(second, StepOutcome::InstructionRetired(fact) if fact.minstret == 2 && !fact.explicit_minstret_write)
    );
    assert_eq!(
        core.state().regs[2],
        1,
        "CSR read observes the pre-retirement count"
    );

    let explicit = core.step_outcome();
    assert!(
        matches!(explicit, StepOutcome::InstructionRetired(fact) if fact.minstret == 41 && fact.explicit_minstret_write)
    );
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 41);
}

#[test]
fn trap_entry_then_handler_and_mret_have_typed_boundaries() {
    let (mut core, _) = core_with_program(&[
        (0x00, 0x0000_0073), // ECALL from M-mode
        (0x04, addi(7, 0, 9)),
        (MTVEC as usize, addi(5, 0, 7)),
        (MTVEC as usize + 4, csrrs(6, machine::MEPC, 0)),
        (MTVEC as usize + 8, addi(6, 6, 4)),
        (MTVEC as usize + 12, csrrw(0, machine::MEPC, 6)),
        (MTVEC as usize + 16, 0x3020_0073), // MRET
    ]);
    core.state_mut()
        .csr
        .write(machine::MSTATUS, 1 << 3)
        .unwrap();

    let entered = core.step_outcome();
    let StepOutcome::TrapEntered(entered) = entered else {
        panic!("ECALL must enter a typed trap")
    };
    assert_eq!(entered.cause, ExceptionCause::EcallM);
    assert_eq!(entered.vector_pc, MTVEC);
    assert_eq!(entered.mtval, 0);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    let mstatus = core.state().csr.read(machine::MSTATUS).unwrap();
    assert_eq!((mstatus >> 3) & 1, 0, "MIE is cleared on entry");
    assert_eq!((mstatus >> 7) & 1, 1, "MPIE receives MIE");
    assert_eq!((mstatus >> 11) & 0b11, 0b11, "MPP records M-mode");

    for _ in 0..4 {
        assert!(matches!(
            core.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }
    let mret = core.step_outcome();
    assert!(matches!(mret, StepOutcome::InstructionRetired(fact) if fact.minstret == 5));
    assert_eq!(core.state().pc, 4);
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
    let after_mret = core.state().csr.read(machine::MSTATUS).unwrap();
    assert_eq!((after_mret >> 3) & 1, 1, "MRET restores MIE from MPIE");
    assert_eq!((after_mret >> 7) & 1, 1, "MRET sets MPIE");
    assert_eq!((after_mret >> 11) & 0b11, 0, "MRET clears MPP");

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[7], 9);
}

#[test]
fn recursive_and_heterogeneous_traps_replace_the_saved_record_without_retiring() {
    let (mut core, _) = core_with_program(&[(0x00, 0x0000_0073), (MTVEC as usize, 0xffff_ffff)]);

    let first = core.step_outcome();
    assert!(
        matches!(first, StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::EcallM)
    );
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let second = core.step_outcome();
    assert!(
        matches!(second, StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::IllegalInstruction)
    );
    assert_eq!(core.state().csr.read(machine::MEPC).unwrap(), MTVEC);
    assert_eq!(core.state().csr.read(machine::MCAUSE).unwrap(), 2);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn ecall_cause_tracks_the_privilege_at_the_fault_boundary() {
    for (privilege, expected) in [
        (PrivilegeMode::User, ExceptionCause::EcallU),
        (PrivilegeMode::Supervisor, ExceptionCause::EcallS),
        (PrivilegeMode::Machine, ExceptionCause::EcallM),
    ] {
        let (mut core, _) = core_with_program(&[(0, 0x0000_0073)]);
        if privilege != PrivilegeMode::Machine {
            core.state_mut().privilege = privilege;
            core.state_mut().csr.set_privilege(privilege);
        }
        let outcome = core.step_outcome();
        assert!(matches!(outcome, StepOutcome::TrapEntered(fact) if fact.cause == expected));
        assert_eq!(
            core.state().csr.read(machine::MCAUSE).unwrap(),
            expected.code()
        );
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    }
}

#[test]
fn illegal_mret_and_bad_encoding_enter_illegal_instruction_traps() {
    let (mut lower, _) = core_with_program(&[(0, 0x3020_0073)]);
    lower.state_mut().privilege = PrivilegeMode::User;
    lower.state_mut().csr.set_privilege(PrivilegeMode::User);
    assert!(
        matches!(lower.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::IllegalInstruction)
    );
    assert_eq!(lower.state().pc, MTVEC);
    assert_eq!(lower.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let (mut malformed, _) = core_with_program(&[(0, 0x3020_00f3)]);
    assert!(
        matches!(malformed.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::IllegalInstruction)
    );
    assert_eq!(malformed.state().pc, MTVEC);
}

#[test]
fn fetch_load_store_and_control_alignment_faults_have_no_partial_side_effects() {
    let (mut fetch, _) = core_with_program(&[]);
    fetch.reset(MEMORY_SIZE as u64, 0);
    fetch.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    assert!(
        matches!(fetch.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::InstructionAccessFault)
    );

    let (mut load, _) = core_with_program(&[(0, load_double(5, 0, 1))]);
    load.state_mut().regs[5] = 0xdead_beef;
    assert!(
        matches!(load.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::LoadAddressMisaligned)
    );
    assert_eq!(load.state().regs[5], 0xdead_beef);

    let (mut store, memory) = core_with_program(&[(0, store_double(5, 0, 1))]);
    store.state_mut().regs[5] = 0x1122_3344_5566_7788;
    assert!(
        matches!(store.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::StoreAddressMisaligned)
    );
    assert_eq!(memory.lock().unwrap().read_dword(0x08).unwrap(), 0);

    let (mut access, memory) = core_with_program(&[(0, load_double(5, 0, 0x200))]);
    access.state_mut().regs[5] = 0xfeed_face;
    assert!(
        matches!(access.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::LoadAccessFault)
    );
    assert_eq!(access.state().regs[5], 0xfeed_face);
    assert_eq!(
        memory.lock().unwrap().read_word(0x00).unwrap(),
        load_double(5, 0, 0x200)
    );

    let (mut store_access, memory) = core_with_program(&[(0, store_double(5, 0, 0x200))]);
    store_access.state_mut().regs[5] = 0x8877_6655_4433_2211;
    assert!(
        matches!(store_access.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::StoreAccessFault)
    );
    assert_eq!(
        memory.lock().unwrap().read_word(0x00).unwrap(),
        store_double(5, 0, 0x200)
    );

    let (mut misaligned_pc, _) = core_with_program(&[(0, addi(1, 0, 1))]);
    misaligned_pc.reset(2, 0);
    misaligned_pc
        .state_mut()
        .csr
        .write(machine::MTVEC, MTVEC)
        .unwrap();
    assert!(
        matches!(misaligned_pc.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::InstructionAddressMisaligned)
    );

    let (mut jump, _) = core_with_program(&[(0, jalr(5, 1, 0))]);
    jump.state_mut().regs[1] = 2;
    jump.state_mut().regs[5] = 0x1234;
    assert!(
        matches!(jump.step_outcome(), StepOutcome::TrapEntered(fact) if fact.cause == ExceptionCause::InstructionAddressMisaligned)
    );
    assert_eq!(jump.state().regs[5], 0x1234);
}

#[test]
fn atomic_misalignment_enters_store_trap_before_any_data_transaction() {
    let raw = amo_word(2, 1, 3, 0b00001);
    let (mut core, data_memory) = core_with_program_and_counting_data(&[(0, raw)]);
    data_memory
        .lock()
        .unwrap()
        .inner
        .write_word(0x20, 0xa5a5_a5a5)
        .unwrap();
    core.state_mut().regs[1] = 1;
    core.state_mut().regs[2] = 0xfeed_face;
    core.state_mut().regs[3] = 7;

    let outcome = core.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("misaligned AMO must enter a trap")
    };
    assert_eq!(fact.cause, ExceptionCause::StoreAddressMisaligned);
    assert_eq!(fact.mtval, 1);
    assert_eq!(core.state().pc, MTVEC);
    assert_eq!(core.state().regs[2], 0xfeed_face);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    let data_memory = data_memory.lock().unwrap();
    assert_eq!(data_memory.transaction_count(), 0);
    assert_eq!(data_memory.snapshot_word(0x20).unwrap(), 0xa5a5_a5a5);
}

#[test]
fn unmapped_atomic_access_enters_store_fault_without_retirement_or_partial_state() {
    let raw = amo_word(2, 1, 3, 0b00001);
    let (mut core, data_memory) = core_with_program_and_counting_data(&[(0, raw)]);
    data_memory
        .lock()
        .unwrap()
        .inner
        .write_word(0x20, 0x5a5a_5a5a)
        .unwrap();
    core.state_mut().regs[1] = MEMORY_SIZE as u64;
    core.state_mut().regs[2] = 0xfeed_face;
    core.state_mut().regs[3] = 7;

    let outcome = core.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("unmapped AMO must enter a trap")
    };
    assert_eq!(fact.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(fact.mtval, MEMORY_SIZE as u64);
    assert_eq!(core.state().pc, MTVEC);
    assert_eq!(core.state().regs[2], 0xfeed_face);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    let data_memory = data_memory.lock().unwrap();
    assert_eq!(data_memory.transaction_count(), 1);
    assert_eq!(data_memory.snapshot_word(0x20).unwrap(), 0x5a5a_5a5a);
}

#[test]
fn load_reserved_faults_use_load_causes_and_preserve_state() {
    let raw = lr_word(2, 1);
    let (mut misaligned, misaligned_memory) = core_with_program_and_counting_data(&[(0, raw)]);
    misaligned.state_mut().regs[1] = 1;
    misaligned.state_mut().regs[2] = 0xfeed_face;

    let outcome = misaligned.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("misaligned LR.W must enter a trap")
    };
    assert_eq!(fact.cause, ExceptionCause::LoadAddressMisaligned);
    assert_eq!(fact.mtval, 1);
    assert_eq!(misaligned.state().pc, MTVEC);
    assert_eq!(misaligned.state().regs[2], 0xfeed_face);
    assert_eq!(misaligned.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(misaligned_memory.lock().unwrap().transaction_count(), 0);

    let (mut unmapped, unmapped_memory) = core_with_program_and_counting_data(&[(0, raw)]);
    unmapped.state_mut().regs[1] = MEMORY_SIZE as u64;
    unmapped.state_mut().regs[2] = 0xfeed_face;

    let outcome = unmapped.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("unmapped LR.W must enter a trap")
    };
    assert_eq!(fact.cause, ExceptionCause::LoadAccessFault);
    assert_eq!(fact.mtval, MEMORY_SIZE as u64);
    assert_eq!(unmapped.state().pc, MTVEC);
    assert_eq!(unmapped.state().regs[2], 0xfeed_face);
    assert_eq!(unmapped.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(unmapped_memory.lock().unwrap().transaction_count(), 1);
}

#[test]
fn reserved_op32_sllw_encoding_traps_while_legal_word_controls_retire() {
    const RESERVED_SLLW: u32 = 0x4000_10bb;
    let (mut reserved, _) = core_with_program(&[(0, RESERVED_SLLW)]);
    reserved.state_mut().regs[1] = 0xfeed_face;

    let outcome = reserved.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("reserved SLLW encoding must enter a trap")
    };
    assert_eq!(fact.cause, ExceptionCause::IllegalInstruction);
    assert_eq!(fact.mtval, RESERVED_SLLW as u64);
    assert_eq!(reserved.state().pc, MTVEC);
    assert_eq!(reserved.state().regs[1], 0xfeed_face);
    assert_eq!(reserved.state().csr.read(machine::MINSTRET).unwrap(), 0);

    for (instruction, rs1, rs2, expected) in [
        (op32(5, 1, 2, 1, 0), 1, 1, 2),
        (op32(5, 1, 2, 0, 0x20), 7, 3, 4),
        (
            op32(5, 1, 2, 5, 0x20),
            0x8000_0000,
            1,
            0xffff_ffff_c000_0000,
        ),
    ] {
        let (mut core, _) = core_with_program(&[(0, instruction)]);
        core.state_mut().regs[1] = rs1;
        core.state_mut().regs[2] = rs2;
        assert!(matches!(
            core.step_outcome(),
            StepOutcome::InstructionRetired(fact) if fact.minstret == 1
        ));
        assert_eq!(core.state().regs[5], expected);
        assert_eq!(core.state().pc, 4);
    }
}

#[test]
fn reserved_op32_and_misc_mem_encodings_enter_illegal_instruction_traps() {
    for raw in [0x0000_203b, 0x0000_700f] {
        let (mut core, _) = core_with_program(&[(0, raw)]);
        core.state_mut().regs[1] = 0x1234;

        let outcome = core.step_outcome();
        let StepOutcome::TrapEntered(fact) = outcome else {
            panic!("reserved encoding {raw:#010x} must enter a trap")
        };
        assert_eq!(fact.cause, ExceptionCause::IllegalInstruction);
        assert_eq!(fact.mtval, raw as u64);
        assert_eq!(core.state().pc, MTVEC);
        assert_eq!(core.state().csr.read(machine::MCAUSE).unwrap(), 2);
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
        assert_eq!(core.state().regs[1], 0x1234);
    }
}

#[test]
fn legal_but_unsupported_fence_i_remains_a_simulator_failure() {
    let (mut core, _) = core_with_program(&[(0, 0x0000_100f)]);
    let outcome = core.step_outcome();
    assert!(matches!(
        outcome,
        StepOutcome::SimulatorFailure(failure)
            if failure.kind == SimulatorFailureKind::UnsupportedLegalInstruction
    ));
    assert_eq!(core.state().pc, 0);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn runner_counts_trap_entry_as_a_completed_turn_but_not_a_retirement() {
    let mut simulator = RiscVSimulator::new(MEMORY_SIZE);
    simulator
        .memory()
        .lock()
        .unwrap()
        .write_word(0, 0x0000_0073)
        .unwrap();
    simulator
        .state_mut()
        .csr
        .write(machine::MTVEC, MTVEC)
        .unwrap();

    let result = simulator.run(Some(1)).unwrap();
    assert_eq!(result.cycles, 1);
    assert!(result.timed_out);
    assert_eq!(result.final_pc, MTVEC);
    assert_eq!(simulator.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn zero_budget_does_not_start_a_turn_or_retire() {
    let mut simulator = RiscVSimulator::new(MEMORY_SIZE);
    simulator
        .memory()
        .lock()
        .unwrap()
        .write_word(0, addi(1, 0, 1))
        .unwrap();

    let result = simulator.run(Some(0)).unwrap();
    assert_eq!(result.cycles, 0);
    assert!(result.timed_out);
    assert_eq!(result.error.as_deref(), Some("Timeout after 0 cycles"));
    assert_eq!(simulator.state().pc, 0);
    assert_eq!(simulator.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn runner_consumes_each_recursive_trap_slot_until_timeout() {
    let mut simulator = RiscVSimulator::new(MEMORY_SIZE);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, 0x0000_0073).unwrap();
        memory.write_word(MTVEC, 0xffff_ffff).unwrap();
    }
    simulator
        .state_mut()
        .csr
        .write(machine::MTVEC, MTVEC)
        .unwrap();

    let result = simulator.run(Some(3)).unwrap();
    assert_eq!(result.cycles, 3, "each recursive trap completes one turn");
    assert!(result.timed_out);
    assert_eq!(result.error.as_deref(), Some("Timeout after 3 cycles"));
    assert_eq!(result.final_pc, MTVEC);
    assert_eq!(simulator.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn last_slot_host_failure_consumes_the_slot_without_a_completed_turn() {
    let mut simulator = RiscVSimulator::new(0x100);
    let memory = simulator.memory().clone();
    let poisoned = std::thread::spawn(move || {
        let _guard = memory.lock().unwrap();
        panic!("poison the host backend for the next slot");
    });
    assert!(poisoned.join().is_err());

    let result = simulator.run(Some(1)).unwrap();
    assert_eq!(result.cycles, 0, "host failure does not complete a turn");
    assert!(!result.timed_out);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("HostBackend")));
    assert!(matches!(simulator.state().pc, 0));
}

#[test]
fn unsupported_legal_instruction_is_not_inferred_from_guest_trap_text() {
    let (mut core, _) = core_with_program(&[(0, 0x1050_0073)]); // WFI
    let outcome = core.step_outcome();
    assert!(matches!(
        outcome,
        StepOutcome::SimulatorFailure(failure)
            if failure.kind == SimulatorFailureKind::UnsupportedLegalInstruction
    ));
    assert_eq!(core.state().pc, 0);
}
