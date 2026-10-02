//! T3 public facade integration, not fresh cross-toolchain/ACT4 acceptance.
#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod elf;
use ruscv_sim::core::CoreState;
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use ruscv_sim::memory::MemoryInterface;
use std::sync::{Arc, Mutex};

#[test]
fn constructor_and_state_memory_apis_remain_actual_borrowed_references() {
    let mut simulator = RiscVSimulator::new(32);
    let state: &CoreState = simulator.state();
    assert_eq!(state.pc, 0);
    let state: &mut CoreState = simulator.state_mut();
    state.regs[9] = 41;
    let memory: &Arc<Mutex<dyn MemoryInterface + Send + Sync>> = simulator.memory();
    assert_eq!(memory.lock().unwrap().size(), 32);
    memory.lock().unwrap().write_word(0, elf::nop()).unwrap();
    let zero = simulator.run(Some(0)).unwrap();
    assert!(zero.timed_out);
    assert_eq!(zero.cycles, 0);
    assert_eq!(simulator.state().pc, 0);
    simulator.step().unwrap();
    assert_eq!(simulator.state().pc, 4);
    assert_eq!(simulator.state().regs[9], 41);
    assert_eq!(simulator.state().csr.read(machine::MINSTRET).unwrap(), 1);
}

fn atomic(kind: u32, rd: u32, rs2: u32) -> u32 {
    (kind << 27) | (rs2 << 20) | (1 << 15) | (3 << 12) | (rd << 7) | 0x2f
}
#[test]
fn same_elf_ordinary_amo_lr_sc_exit_and_fresh_rerun_share_machine_semantics() {
    let code = [
        elf::auipc(1, 0),
        elf::addi(1, 1, 0x200),
        elf::addi(2, 0, 3),
        elf::sd(2, 1, 0),
        atomic(0, 3, 2),
        atomic(2, 4, 0),
        atomic(3, 5, 2),
        atomic(3, 6, 2),
        elf::auipc(1, 0),
        elf::addi(1, 1, -32), // recover BASE
        elf::lui(7, 1),
        (7 << 20) | (1 << 15) | (1 << 7) | 0x33, // ADD x1,x1,x7
        elf::addi(2, 0, 7),
        elf::sd(2, 1, 0),
    ];
    let bytes = elf::elf_with_code(&code, 0, true, true, elf::BSS_MEMORY_SIZE);
    let native = load_and_run(&bytes, Some(code.len() as u64), None, None, false).unwrap();
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&bytes).unwrap();
    let first = flat.run(Some(code.len() as u64)).unwrap();
    assert_eq!(native.exit_code, 3);
    assert_eq!(first.exit_code, native.exit_code);
    assert_eq!(first.cycles, native.cycles);
    assert_eq!(first.final_pc, native.final_pc);
    assert_eq!(first.signature_data, native.signature_data);
    assert!(!first.timed_out && !native.timed_out);
    assert!(first.error.is_none() && native.error.is_none());
    assert_eq!(flat.state().regs[3..7], [3, 6, 0, 1]);
    assert_eq!(flat.read_mem(0x200, 8).unwrap(), 3u64.to_le_bytes());
    let stale = flat.memory().clone();
    flat.write_mem(elf::BSS_PROBE_OFFSET as u64, &[0xaa])
        .unwrap();
    flat.fresh_reset().unwrap();
    assert_eq!(flat.state().pc, elf::BASE);
    assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert!(flat.state().reservation.is_none());
    assert_eq!(flat.read_mem(elf::BSS_PROBE_OFFSET as u64, 1).unwrap(), [0]);
    stale.lock().unwrap().write_dword(0x200, 0xee).unwrap();
    stale
        .lock()
        .unwrap()
        .write_dword(elf::TOHOST_SEGMENT_OFFSET, 9)
        .unwrap();
    assert_eq!(flat.read_mem(0x200, 8).unwrap(), [0; 8]);
    flat.step().unwrap(); // genuinely resumed, not auto-reset by run return
    let rerun = flat.run(Some(code.len() as u64 - 1)).unwrap();
    assert_eq!(rerun.exit_code, 3);
    assert_eq!(rerun.cycles, code.len() as u64 - 1);
    assert_eq!(rerun.final_pc, first.final_pc);
    assert_eq!(rerun.signature_data, first.signature_data);
}

#[test]
fn native_uart_htif_are_not_granted_to_flat_facade() {
    let code = [
        elf::lui(1, 0x10000),
        elf::addi(2, 0, i32::from(b'A')),
        elf::sb(2, 1, 0),
        elf::lui(1, 0x40008),
        elf::addi(2, 0, 7),
        elf::sd(2, 1, 0),
    ];
    let bytes = elf::elf_with_code(&code, 0, false, false, 0x4000);
    let native = load_and_run(&bytes, Some(6), None, None, false).unwrap();
    assert_eq!(native.exit_code, 3);
    assert_eq!(native.cycles, 6);
    assert!(!native.timed_out);
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&bytes).unwrap();
    let result = flat.run(Some(6)).unwrap();
    assert!(result.timed_out);
    assert_eq!(result.exit_code, 1);
    assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 2);
}

#[test]
fn source_audit_standard_facades_have_no_core_composition_invocation_or_effect_reconstruction() {
    let executor = std::fs::read_to_string("src/executor.rs").unwrap();
    let native = executor
        .split("fn run_native(")
        .nth(1)
        .unwrap()
        .split("pub fn load_and_run_file")
        .next()
        .unwrap();
    let flat = executor
        .split("pub struct RiscVSimulator")
        .nth(1)
        .unwrap()
        .split("pub(crate) mod observation_fixture")
        .next()
        .unwrap();
    for facade in [native, flat] {
        let facade: String = facade
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        assert!(facade.contains("OwnedMachine::new("));
        assert!(facade.contains("machine.step_request("));
        for forbidden in [
            "RiscvCore::new",
            "core.step",
            "step_outcome(",
            "step_transition(",
            "regs_before",
            "regs_after",
            "InstructionDecoder",
            "install_image_with_physical_ports(",
        ] {
            assert!(!facade.contains(forbidden), "facade retained {forbidden}");
        }
    }
    let shared = std::fs::read_to_string("src/machine/mod.rs").unwrap();
    let owned = std::fs::read_to_string("src/machine/owned.rs").unwrap();
    assert_eq!(
        shared.matches("self.core.step_transition(observe)").count(),
        1
    );
    assert!(shared.contains("installed.turn(lease, observe, true)"));
    assert!(owned.contains(".turn(lease, observe, false)"));
    assert!(!owned.contains("RiscvCore::new") && !owned.contains("step_transition("));
}

#[test]
fn unsupported_legal_failure_still_allows_real_host_patch_and_separate_resumed_request() {
    let bytes = elf::elf_with_code(&[0x0000_100f, elf::nop()], 0, true, false, 0x4000);
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&bytes).unwrap();
    let clone = sim.memory().clone();
    let failed = sim.run(Some(1)).unwrap();
    assert_eq!(failed.cycles, 0);
    assert!(!failed.timed_out);
    assert!(failed
        .error
        .unwrap()
        .contains("UnsupportedLegalInstruction"));
    clone.lock().unwrap().write_word(0, elf::nop()).unwrap();
    let resumed = sim.run(Some(1)).unwrap();
    assert!(resumed.timed_out);
    assert_eq!(resumed.cycles, 1);
    assert_eq!(sim.state().pc, elf::BASE + 4);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 1);
}

#[test]
fn borrowed_control_edit_cannot_import_old_generation_reservation_but_retains_current_one() {
    let bytes = elf::elf_with_code(&[atomic(2, 3, 0), atomic(3, 4, 2)], 0, true, false, 0x4000);
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&bytes).unwrap();
    sim.state_mut().regs[1] = elf::BASE + 0x200;
    sim.state_mut().regs[2] = 55;
    sim.step().unwrap();
    let old = sim.state().reservation.clone();
    sim.state_mut().regs[2] = 77; // retain this domain's legitimate LR context
    sim.step().unwrap();
    assert_eq!(sim.state().regs[4], 0);
    assert_eq!(sim.read_mem(0x200, 8).unwrap(), 77u64.to_le_bytes());
    sim.fresh_reset().unwrap();
    let state = sim.state_mut();
    state.regs[1] = elf::BASE + 0x200;
    state.regs[2] = 99;
    state.pc = elf::BASE + 4;
    state.reservation = old;
    sim.step().unwrap();
    assert_eq!(sim.state().regs[4], 1);
    assert_eq!(sim.read_mem(0x200, 8).unwrap(), [0; 8]);
}
