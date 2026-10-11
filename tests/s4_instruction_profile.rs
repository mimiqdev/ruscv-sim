use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::{CoreState, PrivilegeMode, RiscvCore, StepOutcome};
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::decode::InstructionDecoder;
use ruscv_sim::execute::{ExecuteError, Executor};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use std::sync::{Arc, Mutex};

const MODES: [PrivilegeMode; 3] = [
    PrivilegeMode::Machine,
    PrivilegeMode::Supervisor,
    PrivilegeMode::User,
];

fn core(raw: u32, mode: PrivilegeMode, status: u64) -> RiscvCore {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    ram.lock().unwrap().write_word(0, raw).unwrap();
    let mut core = RiscvCore::new(ram.clone(), ram);
    let state = core.state_mut();
    state.csr.write(machine::MSTATUS, status).unwrap();
    state.csr.write(machine::MTVEC, 0x80).unwrap();
    state.csr.write(supervisor::SEPC, 0x43).unwrap();
    state.regs[1] = u64::MAX;
    state.regs[2] = 0xdead_beef;
    state.privilege = mode;
    state.csr.set_privilege(mode);
    core
}

fn assert_state_eq(left: &CoreState, right: &CoreState) {
    assert_eq!(left.regs, right.regs);
    for index in 0..32 {
        assert_eq!(left.fpr.read(index).bits(), right.fpr.read(index).bits());
    }
    assert_eq!(left.fcsr.read(), right.fcsr.read());
    assert_eq!(left.pc, right.pc);
    assert_eq!(left.privilege, right.privilege);
    assert_eq!(left.branch_taken, right.branch_taken);
    assert_eq!(left.reservation, right.reservation);
    assert_eq!(left.csr.get_privilege(), right.csr.get_privilege());
    let mut left = left.csr.clone();
    let mut right = right.csr.clone();
    left.set_privilege(PrivilegeMode::Machine);
    right.set_privilege(PrivilegeMode::Machine);
    for addr in [
        machine::MSTATUS,
        machine::MTVEC,
        machine::MEPC,
        machine::MCAUSE,
        machine::MTVAL,
        machine::MINSTRET,
        supervisor::SEPC,
        supervisor::SSTATUS,
    ] {
        assert_eq!(left.read(addr), right.read(addr));
    }
}

fn check(raw: u32, mode: PrivilegeMode, status: u64, legal: bool) -> CoreState {
    let mut on = core(raw, mode, status);
    let mut off = core(raw, mode, status);
    let observed = on.step_transition(true);
    let silent = off.step_transition(false);
    assert_eq!(observed.control, silent.control);
    assert_eq!(observed.outcome, silent.outcome);
    assert!(silent.observation.is_none());
    assert_state_eq(on.state(), off.state());
    assert_eq!(observed.control.retired, legal, "{raw:#x} {mode:?}");
    if !legal {
        let StepOutcome::TrapEntered(trap) = observed.outcome else {
            panic!("expected trap")
        };
        assert_eq!(trap.cause as u64, 2);
        assert_eq!(trap.mtval, raw as u64);
        assert_eq!(on.state().pc, 0x80);
        assert_eq!(on.state().csr.read(machine::MEPC).unwrap(), 0);
        let Some(Observation::Trap(record)) = observed.observation else {
            panic!("expected trap record")
        };
        assert!(record.effects.memory.is_empty());
        assert!(record.effects.fpr.is_empty());
        assert!(record.effects.fcsr.is_none());
        assert!(record.effects.gpr.is_empty());
    }
    on.state().clone()
}

#[test]
fn wfi_privilege_tw_matrix_and_fixed_encoding() {
    for mode in MODES {
        for tw in [0, mstatus::TW] {
            let legal =
                mode == PrivilegeMode::Machine || (mode == PrivilegeMode::Supervisor && tw == 0);
            let mut state = check(0x1050_0073, mode, tw, legal);
            if legal {
                assert_eq!(state.pc, 4);
                assert_eq!(state.privilege, mode);
                state.csr.set_privilege(PrivilegeMode::Machine);
                assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), 1);
                assert_eq!(
                    state.csr.read(machine::MSTATUS).unwrap(),
                    0xa_0000_0000 | tw
                );
            }
            for malformed in [0x1050_00f3, 0x1050_8073] {
                check(malformed, mode, tw, false);
            }
        }
    }
}

#[test]
fn satp_read_and_zero_write_honor_privilege_and_tvm() {
    for mode in MODES {
        for tvm in [0, mstatus::TVM] {
            for funct3 in [1, 2] {
                let raw = (u32::from(supervisor::SATP) << 20) | (funct3 << 12) | (3 << 7) | 0x73;
                let legal = mode == PrivilegeMode::Machine
                    || (mode == PrivilegeMode::Supervisor && tvm == 0);
                let mut state = check(raw, mode, tvm, legal);
                assert_eq!(state.regs[3], 0);
                state.csr.set_privilege(PrivilegeMode::Machine);
                assert_eq!(state.csr.read(supervisor::SATP).unwrap(), 0);
                assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), u64::from(legal));
            }
        }
    }
}

#[test]
fn sret_privilege_tsr_spie_spp_and_mprv_matrix() {
    for mode in MODES {
        for tsr in [0, mstatus::TSR] {
            for spie in [0, mstatus::SPIE] {
                for spp in [0, mstatus::SPP] {
                    let status = tsr | spie | spp | mstatus::MPRV | mstatus::SIE;
                    let legal = mode == PrivilegeMode::Machine
                        || (mode == PrivilegeMode::Supervisor && tsr == 0);
                    let mut state = check(0x1020_0073, mode, status, legal);
                    if legal {
                        let target = if spp == 0 {
                            PrivilegeMode::User
                        } else {
                            PrivilegeMode::Supervisor
                        };
                        assert_eq!(state.pc, 0x40);
                        assert_eq!(state.privilege, target);
                        assert_eq!(state.csr.get_privilege(), target);
                        assert!(state.csr.read(machine::MSTATUS).is_err());
                        assert_eq!(
                            state.csr.read(supervisor::SEPC).is_ok(),
                            target == PrivilegeMode::Supervisor
                        );
                        state.csr.set_privilege(PrivilegeMode::Machine);
                        let expected = 0xa_0000_0000
                            | tsr
                            | mstatus::SPIE
                            | if spie != 0 { mstatus::SIE } else { 0 };
                        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), expected);
                        assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), 1);
                    }
                }
            }
        }
    }
}

#[test]
fn sfence_all_forms_privilege_tvm_and_reserved_fields() {
    for mode in MODES {
        for tvm in [0, mstatus::TVM] {
            for rs1 in [0, 1] {
                for rs2 in [0, 2] {
                    let raw = 0x1200_0073 | (rs1 << 15) | (rs2 << 20);
                    let legal = mode == PrivilegeMode::Machine
                        || (mode == PrivilegeMode::Supervisor && tvm == 0);
                    let state = check(raw, mode, tvm, legal);
                    if legal {
                        assert_eq!(state.pc, 4);
                    }
                    check(raw | (1 << 7), mode, tvm, false);
                    check(raw ^ (1 << 26), mode, tvm, false);
                }
            }
        }
    }
}

#[test]
fn direct_executor_rejects_malformed_fixed_system_encodings_without_effects() {
    for raw in [
        0x1050_0073,
        0x1200_0073,
        0x0000_0073,
        0x0010_0073,
        0x1020_0073,
        0x3020_0073,
    ] {
        for malformed in [raw | (1 << 7), raw | (1 << 15)] {
            if raw == 0x1200_0073 && malformed == raw | (1 << 15) {
                continue;
            }
            let decoded = InstructionDecoder::new().decode(malformed).unwrap();
            let mut state = CoreState::default();
            let before = state.clone();
            let mut ram = SimpleMemory::new(16);
            let result = Executor::new().execute(&decoded, &mut state, &mut ram);
            assert!(
                matches!(result, Err(ExecuteError::IllegalInstruction)),
                "raw={malformed:#010x}: {result:?}"
            );
            assert_state_eq(&state, &before);
        }
    }
}

#[test]
fn word_m_dispatch_corner_cases_and_x0() {
    for (funct3, a, b, expected) in [
        (0, 0xffff_ffff_0000_0003, 7, 21),
        (0, 0x8000_0000, 1, 0xffff_ffff_8000_0000),
        (0, 0x10000, 0x10000, 0),
        (4, (-7i64) as u64, 2, (-3i64) as u64),
        (4, 0x8000_0000, u64::MAX, 0xffff_ffff_8000_0000),
        (4, 5, 0, u64::MAX),
        (5, 0xffff_ffff, 2, 0x7fff_ffff),
        (5, 0x8000_0000, 1, 0xffff_ffff_8000_0000),
        (5, 9, 0, u64::MAX),
        (6, (-7i64) as u64, 2, u64::MAX),
        (6, 0x8000_0000, u64::MAX, 0),
        (6, 0x8000_0001, 0, 0xffff_ffff_8000_0001),
        (7, 0xffff_ffff, 2, 1),
        (7, 0x8000_0001, 0, 0xffff_ffff_8000_0001),
    ] {
        for rd in [0, 3] {
            let raw = 0x0220_803b | (funct3 << 12) | (rd << 7);
            let mut controls = Vec::new();
            for observed in [false, true] {
                let mut hart = core(raw, PrivilegeMode::Machine, 0);
                hart.state_mut().regs[1] = a;
                hart.state_mut().regs[2] = b;
                let turn = hart.step_transition(observed);
                assert!(turn.control.retired);
                assert_eq!(hart.state().regs[0], 0);
                assert_eq!(hart.state().regs[3], if rd == 0 { 0 } else { expected });
                assert_eq!(hart.state().pc, 4);
                controls.push((turn.control, turn.outcome, hart.state().clone()));
            }
            assert_eq!(controls[0].0, controls[1].0);
            assert_eq!(controls[0].1, controls[1].1);
            assert_state_eq(&controls[0].2, &controls[1].2);
        }
    }
    for funct3 in [1, 2, 3] {
        check(
            0x0220_81bb | (funct3 << 12),
            PrivilegeMode::Machine,
            0,
            false,
        );
    }
}
