//! A6 Task 3 regressions for the integrated Hart outcome and trap boundary.

use ruscv_sim::core::{
    ExceptionCause, PrivilegeMode, RiscvCore, SimulatorFailureKind, StepOutcome,
};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::RiscVSimulator;
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::MemoryInterface;
use std::sync::{Arc, Mutex};

const MEMORY_SIZE: usize = 0x200;
const MTVEC: u64 = 0x80;

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
