//! A6 Task 2 component and TrapHandler-composition tests.
//!
//! These tests exercise MRET semantics without wiring a second trap path into
//! the helper.  The public core/Runner mapping of `ExecuteError::IllegalInstruction`
//! remains a Task 3 responsibility.

use ruscv_sim::core::{CoreState, PrivilegeMode, Trap, TrapHandler};
use ruscv_sim::csr::machine;
use ruscv_sim::execute::ExecuteError;
use ruscv_sim::isa::rv64i::exec_system;
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::DecodedInstruction;

fn mret_instruction() -> DecodedInstruction {
    DecodedInstruction {
        raw: 0x3020_0073,
        format: ruscv_sim::InstructionFormat::RType,
        opcode: ruscv_sim::decode::Opcode::System,
        funct3: None,
        funct7: Some(0b001_1000),
        rs1: None,
        rs2: Some(2),
        rs3: None,
        rd: None,
        imm: Some(0x302),
        branch_taken: false,
    }
}

#[test]
fn mret_restores_all_mie_mpie_combinations_and_privilege_modes() {
    for return_mode in [
        PrivilegeMode::User,
        PrivilegeMode::Supervisor,
        PrivilegeMode::Machine,
    ] {
        for mie in [0u64, 1] {
            for mpie in [0u64, 1] {
                for initial_mprv in [false, true] {
                    let mut state = CoreState::default();
                    let initial_status = (mie << 3)
                        | (mpie << 7)
                        | ((return_mode as u64) << 11)
                        | (u64::from(initial_mprv) << 17);
                    state.csr.write(machine::MSTATUS, initial_status).unwrap();
                    state
                        .csr
                        .write(machine::MEPC, 0x1234_5678_9ABC_DEF3)
                        .unwrap();
                    state.pc = 0xDEAD_BEEF;
                    state.regs[0] = 0;

                    let instr = mret_instruction();
                    let mut mem = SimpleMemory::new(0x1000);
                    exec_system(&instr, &mut state, &mut mem).unwrap();

                    assert_eq!(state.pc, 0x1234_5678_9ABC_DEF0);
                    assert!(state.branch_taken);
                    assert_eq!(state.regs[0], 0);
                    assert_eq!(state.privilege, return_mode);
                    assert_eq!(state.csr.get_privilege(), return_mode);

                    // Machine CSRs are not readable from U/S after the
                    // transition.  Temporarily elevate the CSR view only to
                    // inspect the committed value, then restore consistency.
                    state.csr.set_privilege(PrivilegeMode::Machine);
                    let actual_status = state.csr.read(machine::MSTATUS).unwrap();
                    state.csr.set_privilege(return_mode);

                    let mut expected_status = (initial_status & !(1 << 3)) | (mpie << 3);
                    expected_status = (expected_status | (1 << 7)) & !(0b11 << 11);
                    if return_mode != PrivilegeMode::Machine {
                        expected_status &= !(1 << 17);
                    }
                    assert_eq!(actual_status, expected_status);
                }
            }
        }
    }
}

#[test]
fn mret_from_user_or_supervisor_is_typed_illegal_instruction_without_side_effects() {
    for source_mode in [PrivilegeMode::User, PrivilegeMode::Supervisor] {
        let mut state = CoreState::default();
        let initial_status = (1 << 3) | (1 << 7) | (0b11 << 11) | (1 << 17);
        state.csr.write(machine::MSTATUS, initial_status).unwrap();
        state
            .csr
            .write(machine::MEPC, 0x1111_2222_3333_4444)
            .unwrap();
        state.pc = 0x5555_6666_7777_8888;
        state.privilege = source_mode;
        state.csr.set_privilege(source_mode);

        let instr = mret_instruction();
        let mut mem = SimpleMemory::new(0x1000);
        let result = exec_system(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::IllegalInstruction)));
        assert_eq!(state.pc, 0x5555_6666_7777_8888);
        assert!(!state.branch_taken);
        assert_eq!(state.privilege, source_mode);
        assert_eq!(state.csr.get_privilege(), source_mode);

        state.csr.set_privilege(PrivilegeMode::Machine);
        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), initial_status);
        assert_eq!(
            state.csr.read(machine::MEPC).unwrap(),
            0x1111_2222_3333_4444
        );
        state.csr.set_privilege(source_mode);
    }
}

#[test]
fn mret_read_failure_does_not_partially_restore_state() {
    let mut state = CoreState {
        pc: 0x7777,
        ..Default::default()
    };
    state.csr.set_privilege(PrivilegeMode::Supervisor);
    let instr = mret_instruction();
    let mut mem = SimpleMemory::new(0x1000);

    let result = exec_system(&instr, &mut state, &mut mem);

    assert!(matches!(
        result,
        Err(ExecuteError::CsrError(
            ruscv_sim::CsrError::PrivilegeViolation(machine::MSTATUS)
        ))
    ));
    assert_eq!(state.pc, 0x7777);
    assert_eq!(state.privilege, PrivilegeMode::Machine);
    assert_eq!(state.csr.get_privilege(), PrivilegeMode::Supervisor);
    assert!(!state.branch_taken);
}

#[test]
fn lower_privilege_mret_error_composes_with_standard_illegal_trap_entry() {
    for source_mode in [PrivilegeMode::User, PrivilegeMode::Supervisor] {
        let mut state = CoreState::default();
        let initial_mie = 1u64;
        let initial_status = (initial_mie << 3) | (1 << 17);
        state.csr.write(machine::MSTATUS, initial_status).unwrap();
        state.csr.write(machine::MTVEC, 0x8000_1001).unwrap();
        state.pc = 0x4000;
        state.privilege = source_mode;
        state.csr.set_privilege(source_mode);

        let instr = mret_instruction();
        let fault_pc = state.pc;
        let mut mem = SimpleMemory::new(0x1000);
        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(matches!(result, Err(ExecuteError::IllegalInstruction)));

        // The helper itself has not entered a trap or changed return state.
        assert_eq!(state.pc, fault_pc);
        assert_eq!(state.privilege, source_mode);
        assert_eq!(state.csr.get_privilege(), source_mode);

        // Task 3 owns this boundary mapping.  Compose the typed helper error
        // with the existing TrapHandler component to verify the required
        // architectural IllegalInstruction effects without a helper bypass.
        let mut handler = TrapHandler::new();
        let vector = handler.handle_trap(
            Trap::Exception(ruscv_sim::core::ExceptionCause::IllegalInstruction),
            fault_pc,
            instr.raw as u64,
            &mut state,
        );

        assert_eq!(vector, 0x8000_1000);
        assert_eq!(state.pc, 0x8000_1000);
        assert_eq!(state.privilege, PrivilegeMode::Machine);
        assert_eq!(state.csr.get_privilege(), PrivilegeMode::Machine);
        assert_eq!(state.csr.read(machine::MEPC).unwrap(), fault_pc);
        assert_eq!(state.csr.read(machine::MCAUSE).unwrap(), 2);
        assert_eq!(state.csr.read(machine::MTVAL).unwrap(), instr.raw as u64);

        let mstatus = state.csr.read(machine::MSTATUS).unwrap();
        assert_eq!((mstatus >> 3) & 1, 0);
        assert_eq!((mstatus >> 7) & 1, initial_mie);
        assert_eq!((mstatus >> 11) & 0b11, source_mode as u64);
        assert_eq!((mstatus >> 17) & 1, 1);
    }
}
