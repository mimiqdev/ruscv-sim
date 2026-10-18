//! Stage 4 D10 architectural counters and public-route accounting.

use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::trap::ExceptionCause;
use ruscv_sim::core::{PrivilegeMode, RiscvCore, StepOutcome, Trap};
use ruscv_sim::csr::{machine, unprivileged, CsrError, CsrFile};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod elf;

fn csr_op(funct3: u32, rd: u8, csr: u16, source: u8) -> u32 {
    (u32::from(csr) << 20)
        | (u32::from(source) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | 0x73
}

fn direct_core(program: &[u32]) -> RiscvCore {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    for (index, instruction) in program.iter().enumerate() {
        memory
            .lock()
            .unwrap()
            .write_word(index as u64 * 4, *instruction)
            .unwrap();
    }
    let mut core = RiscvCore::new(memory.clone(), memory);
    core.reset(0, 0);
    core
}

fn counters(core: &RiscvCore) -> (u64, u64) {
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    (
        csr.read(machine::MCYCLE).unwrap(),
        csr.read(machine::MINSTRET).unwrap(),
    )
}

#[test]
fn counters_and_shadows_share_storage_and_access_policy() {
    let mut csr = CsrFile::new(0);
    for (counter, shadow) in [
        (machine::MCYCLE, unprivileged::CYCLE),
        (machine::MINSTRET, unprivileged::INSTRET),
    ] {
        assert_eq!(csr.read(counter), Ok(0));
        assert_eq!(csr.read(shadow), Ok(0));
        csr.write(counter, u64::MAX).unwrap();
        assert_eq!(csr.read(shadow), Ok(u64::MAX));
        assert_eq!(csr.write(shadow, 0), Err(CsrError::ReadOnly(shadow)));
        for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
            csr.set_privilege(mode);
            assert_eq!(csr.read(shadow), Err(CsrError::PrivilegeViolation(shadow)));
            assert_eq!(
                csr.read(counter),
                Err(CsrError::PrivilegeViolation(counter))
            );
        }
        csr.set_privilege(PrivilegeMode::Machine);
        assert_eq!(csr.read(counter), Ok(u64::MAX));
    }
    // time exists and is read-only. The platform publishes mtime through
    // the Hart; this file alone still reads the reset shadow, which is 0,
    // and that read does not change mcycle.
    assert_eq!(csr.read(unprivileged::TIME), Ok(0));
    assert_eq!(
        csr.write(unprivileged::TIME, 1),
        Err(CsrError::ReadOnly(unprivileged::TIME))
    );
    assert_eq!(csr.read(machine::MCYCLE), Ok(u64::MAX));
    csr.set_privilege(PrivilegeMode::Supervisor);
    assert_eq!(
        csr.read(unprivileged::TIME),
        Err(CsrError::PrivilegeViolation(unprivileged::TIME))
    );
    for absent in [0xC03u16, 0xC1F, 0x320] {
        csr.set_privilege(PrivilegeMode::Machine);
        assert_eq!(csr.read(absent), Err(CsrError::InvalidAddress(absent)));
    }
}

#[test]
fn every_csr_form_obeys_counter_write_precedence() {
    // funct3, source field, x2 value, explicit write, resulting counter value.
    let cases = [
        (1, 0, 99, true, 0),
        (1, 2, 40, true, 40),
        (2, 0, 99, false, 41),
        (2, 2, 0, true, 40),
        (2, 2, 3, true, 43),
        (3, 0, 99, false, 41),
        (3, 2, 0, true, 40),
        (3, 2, 8, true, 32),
        (5, 0, 99, true, 0),
        (5, 7, 99, true, 7),
        (6, 0, 99, false, 41),
        (6, 3, 99, true, 43),
        (7, 0, 99, false, 41),
        (7, 8, 99, true, 32),
    ];
    for counter in [machine::MCYCLE, machine::MINSTRET] {
        for (funct3, source, value, wrote, expected) in cases {
            for observe in [false, true] {
                let mut core = direct_core(&[csr_op(funct3, 1, counter, source)]);
                core.state_mut().csr.write(machine::MCYCLE, 40).unwrap();
                core.state_mut().csr.write(machine::MINSTRET, 40).unwrap();
                core.state_mut().regs[2] = value;
                let turn = core.step_transition(observe);
                let StepOutcome::InstructionRetired(retired) = turn.outcome else {
                    panic!("CSR form {funct3}/{source} trapped");
                };
                assert_eq!(
                    retired.explicit_minstret_write,
                    counter == machine::MINSTRET && wrote
                );
                assert_eq!(
                    core.state().regs[1],
                    40,
                    "CSR read precedes this instruction's increment"
                );
                assert_eq!(core.state().regs[0], 0);
                assert_eq!(
                    counters(&core),
                    if counter == machine::MCYCLE {
                        (expected, 41)
                    } else {
                        (41, expected)
                    }
                );
                if observe {
                    let Observation::Commit(record) = turn.observation.unwrap() else {
                        panic!("expected commit");
                    };
                    assert_eq!(record.effects.csr.len(), 2);
                    let changes: Vec<_> = record
                        .effects
                        .csr
                        .iter()
                        .filter(|access| access.addr == counter)
                        .collect();
                    assert_eq!(
                        changes.len(),
                        1,
                        "an explicit write replaces the implicit increment"
                    );
                    assert_eq!((changes[0].old_value, changes[0].new_value), (40, expected));
                } else {
                    assert!(turn.observation.is_none());
                }
            }
        }
    }
}

#[test]
fn shadow_guest_reads_observe_old_values_and_writes_trap() {
    for shadow in [unprivileged::CYCLE, unprivileged::INSTRET] {
        for (funct3, source, writes) in [
            (1, 0, true),
            (2, 0, false),
            (2, 2, true),
            (3, 0, false),
            (3, 2, true),
            (5, 0, true),
            (6, 0, false),
            (6, 1, true),
            (7, 0, false),
            (7, 1, true),
        ] {
            let instruction = csr_op(funct3, 1, shadow, source);
            let mut core = direct_core(&[instruction]);
            core.state_mut().regs[1] = 0x5555;
            core.state_mut().csr.write(machine::MCYCLE, 40).unwrap();
            core.state_mut().csr.write(machine::MINSTRET, 40).unwrap();
            let turn = core.step_transition(true);
            if writes {
                let StepOutcome::TrapEntered(trap) = turn.outcome else {
                    panic!("shadow write retired");
                };
                assert_eq!(
                    trap.cause,
                    Trap::Exception(ExceptionCause::IllegalInstruction)
                );
                assert_eq!(trap.mtval, u64::from(instruction));
                assert_eq!(core.state().regs[1], 0x5555);
                assert_eq!(counters(&core), (41, 40));
            } else {
                assert!(turn.control.retired);
                assert_eq!(core.state().regs[1], 40);
                assert_eq!(counters(&core), (41, 41));
            }
        }
    }
}

#[test]
fn lower_mode_shadow_reads_trap_without_destination_or_retirement() {
    for mode in [PrivilegeMode::Supervisor, PrivilegeMode::User] {
        for shadow in [unprivileged::CYCLE, unprivileged::INSTRET] {
            for observe in [false, true] {
                let instruction = csr_op(2, 1, shadow, 0);
                let mut core = direct_core(&[instruction]);
                core.state_mut().regs[1] = 0x5555;
                core.state_mut().privilege = mode;
                let turn = core.step_transition(observe);
                let StepOutcome::TrapEntered(trap) = turn.outcome else {
                    panic!("lower-mode shadow read retired");
                };
                assert_eq!(
                    trap.cause,
                    Trap::Exception(ExceptionCause::IllegalInstruction)
                );
                assert_eq!(trap.mtval, u64::from(instruction));
                assert_eq!(trap.source_privilege, mode);
                assert_eq!(core.state().regs[1], 0x5555);
                assert_eq!(counters(&core), (1, 0));
            }
        }
    }
}

#[test]
fn completed_turns_wrap_counters_and_journal_trap_cycles() {
    for observe in [false, true] {
        for mode in [
            PrivilegeMode::Machine,
            PrivilegeMode::Supervisor,
            PrivilegeMode::User,
        ] {
            let mut core = direct_core(&[elf::addi(0, 0, 0), 0x0000_0073]);
            core.state_mut()
                .csr
                .write(machine::MCYCLE, u64::MAX)
                .unwrap();
            core.state_mut()
                .csr
                .write(machine::MINSTRET, u64::MAX)
                .unwrap();
            core.state_mut().privilege = mode;
            assert!(core.step_transition(observe).control.retired);
            assert_eq!(counters(&core), (0, 0));
            let trap = core.step_transition(observe);
            assert!(trap.control.trap_entered && !trap.control.retired);
            assert_eq!(counters(&core), (1, 0));
            if observe {
                let Observation::Trap(record) = trap.observation.unwrap() else {
                    panic!("expected trap");
                };
                let counter_effects: Vec<_> = record
                    .effects
                    .csr
                    .iter()
                    .filter(|access| matches!(access.addr, machine::MCYCLE | machine::MINSTRET))
                    .collect();
                assert_eq!(counter_effects.len(), 1);
                assert_eq!(
                    (
                        counter_effects[0].addr,
                        counter_effects[0].old_value,
                        counter_effects[0].new_value
                    ),
                    (machine::MCYCLE, 0, 1)
                );
            }
        }
    }
    let mut core = direct_core(&[0x1050_0073]);
    assert!(core.step_transition(true).control.retired);
    assert_eq!(counters(&core), (1, 1));
}

#[test]
fn failed_turn_leaves_both_counters_unchanged() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    core.state_mut().csr.write(machine::MCYCLE, 17).unwrap();
    core.state_mut().csr.write(machine::MINSTRET, 23).unwrap();
    assert!(std::thread::spawn(move || {
        let _guard = memory.lock().unwrap();
        panic!("poison instruction memory");
    })
    .join()
    .is_err());
    for observe in [false, true] {
        let failed = core.step_transition(observe);
        assert!(matches!(failed.outcome, StepOutcome::SimulatorFailure(_)));
        assert!(failed.observation.is_none());
        assert_eq!(counters(&core), (17, 23));
    }
}

#[test]
fn guest_counter_writes_do_not_change_public_turn_budgets() {
    let program = [
        elf::addi(2, 0, 40),
        csr_op(1, 0, machine::MCYCLE, 2),
        csr_op(2, 3, unprivileged::CYCLE, 0),
        csr_op(1, 0, machine::MINSTRET, 2),
        csr_op(2, 4, unprivileged::INSTRET, 0),
    ];
    let bytes = elf::elf_with_code(&program, 0, true, true, elf::BSS_MEMORY_SIZE);
    for platform in [PlatformKind::Flat, PlatformKind::Native] {
        for observe in [false, true] {
            let machine = Machine::new(MachineConfig::new(platform));
            machine
                .install(Arc::new(LoadImage::parse(&bytes).unwrap()))
                .unwrap();
            machine.resume().unwrap();
            for _ in &program {
                assert!(machine.step(observe).unwrap().hart().control.retired);
            }
            let state = machine.inspect().unwrap().hart;
            assert_eq!((state.regs[3], state.regs[4]), (40, 40));
            assert_eq!(state.csr.read(machine::MCYCLE), Ok(43));
            assert_eq!(state.csr.read(machine::MINSTRET), Ok(41));
            machine.request_quiesce().unwrap();
            machine.try_drain().unwrap();
            machine.fresh_reset().unwrap();
            let reset = machine.inspect().unwrap().hart;
            assert_eq!(reset.csr.read(machine::MCYCLE), Ok(0));
            assert_eq!(reset.csr.read(machine::MINSTRET), Ok(0));
        }
    }
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&bytes).unwrap();
    assert_eq!(flat.run(Some(0)).unwrap().cycles, 0);
    assert_eq!(flat.state().csr.read(machine::MCYCLE), Ok(0));
    let result = flat.run(Some(5)).unwrap();
    assert_eq!(result.cycles, 5);
    assert!(result.timed_out);
    assert_eq!((flat.state().regs[3], flat.state().regs[4]), (40, 40));
    assert_eq!(flat.state().csr.read(machine::MCYCLE), Ok(43));
    let native = load_and_run(&bytes, Some(5), None, None, false).unwrap();
    assert_eq!(native.cycles, 5);
    assert!(native.timed_out);
    assert_eq!(native.error.as_deref(), Some("Timeout after 5 cycles"));
}
