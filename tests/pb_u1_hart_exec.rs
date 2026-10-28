//! Program-buffer U1: Hart debug execution and Zifencei.

#[allow(dead_code)]
mod common;
use common::public_elf as elf;

use ruscv_sim::core::trap::{ExceptionCause, Trap};
use ruscv_sim::core::{DebugExecOutcome, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::{MemoryInterface, RiscvCore, SimpleMemory};
use std::sync::{Arc, Mutex};

const EBREAK: u32 = 0x0010_0073;
const FENCE_I: u32 = 0x0000_100f;
const WFI: u32 = 0x1050_0073;

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}
fn csrrw(rd: u32, csr: u32, rs1: u32) -> u32 {
    (csr << 20) | (rs1 << 15) | (1 << 12) | (rd << 7) | 0x73
}
fn sd(rs2: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32 >> 5) & 0x7f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (3 << 12)
        | ((imm as u32 & 0x1f) << 7)
        | 0x23
}
fn ld(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (3 << 12) | (rd << 7) | 0x03
}
fn amoadd_d(rd: u32, rs1: u32, rs2: u32) -> u32 {
    (rs2 << 20) | (rs1 << 15) | (3 << 12) | (rd << 7) | 0x2f
}
fn auipc(rd: u32) -> u32 {
    (rd << 7) | 0x17
}
fn beq(rs1: u32, rs2: u32, imm: i32) -> u32 {
    let i = imm as u32;
    ((i >> 12 & 1) << 31)
        | ((i >> 5 & 0x3f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | ((i >> 1 & 0xf) << 8)
        | ((i >> 11 & 1) << 7)
        | 0x63
}

fn halted_core(words: &[u32]) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    {
        let mut m = memory.lock().unwrap();
        for (i, w) in words.iter().enumerate() {
            m.write_word(i as u64 * 4, *w).unwrap();
        }
    }
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    core.set_haltreq(true);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::DebugHalted { .. }
    ));
    (core, memory)
}

#[test]
fn arithmetic_csr_memory_atomic_and_fence_i_run_and_preserve_dpc() {
    let (mut core, mem) = halted_core(&[0x13]);
    let dpc = core.dpc();
    core.state_mut().regs[2] = 0x100;
    core.state_mut().regs[3] = 5;
    assert_eq!(
        core.debug_execute(addi(1, 0, 7)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.state().regs[1], 7);
    assert_eq!(core.debug_execute(sd(1, 2, 0)), DebugExecOutcome::Completed);
    assert_eq!(mem.lock().unwrap().read_dword(0x100).unwrap(), 7);
    assert_eq!(
        core.debug_execute(amoadd_d(4, 2, 3)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.state().regs[4], 7);
    assert_eq!(mem.lock().unwrap().read_dword(0x100).unwrap(), 12);
    assert_eq!(core.debug_execute(ld(5, 2, 0)), DebugExecOutcome::Completed);
    assert_eq!(core.state().regs[5], 12);
    assert_eq!(
        core.debug_execute(csrrw(0, machine::MSCRATCH as u32, 1)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.state().csr.read(machine::MSCRATCH).unwrap(), 7);
    assert_eq!(core.debug_execute(FENCE_I), DebugExecOutcome::Completed);
    assert_eq!(
        core.debug_execute(addi(0, 0, 9)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.state().regs[0], 0, "x0 stays zero");
    assert_eq!(core.dpc(), dpc, "buffer sequencing leaves dpc alone");
    assert!(core.in_debug_mode());
}

#[test]
fn wfi_is_nop_and_ebreak_terminates() {
    let (mut core, _) = halted_core(&[0x13]);
    assert_eq!(core.debug_execute(WFI), DebugExecOutcome::Completed);
    assert_eq!(core.debug_execute(EBREAK), DebugExecOutcome::Ebreak);
    assert!(core.in_debug_mode());
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::DebugHalted { .. }
    ));
}

#[test]
fn explicit_dpc_write_changes_resume_pc() {
    let (mut core, _) = halted_core(&[0x13]);
    core.write_dpc(0x40);
    assert_eq!(
        core.debug_execute(addi(1, 0, 1)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.dpc(), 0x40);
    assert!(core.debug_resume());
    assert_eq!(core.state().pc, 0x40);
}

#[test]
fn control_transfers_pc_dependents_and_privilege_ops_are_rejected() {
    let (mut core, _) = halted_core(&[0x13]);
    let pc = core.state().pc;
    for word in [
        beq(0, 1, 8), // taken
        beq(0, 2, 8), // not taken (x2 != 0 below)
        0x0000_006f,  // jal x0, 0
        0x0000_0067,  // jalr x0, 0(x0)
        auipc(1),
        0x0000_0073, // ecall
        0x3020_0073, // mret
        0x1020_0073, // sret
        0x7b20_0073, // dret
    ] {
        core.state_mut().regs[2] = 1;
        core.state_mut().regs[1] = 0x55;
        assert_eq!(
            core.debug_execute(word),
            DebugExecOutcome::Exception,
            "{word:#010x}"
        );
        assert_eq!(core.state().regs[1], 0x55, "no effect for {word:#010x}");
        assert_eq!(core.state().pc, pc);
    }
}

#[test]
fn exception_keeps_guest_trap_csrs_and_counters_and_earlier_effects() {
    let (mut core, _) = halted_core(&[0x13]);
    core.state_mut().csr.write(machine::MEPC, 0x1234).unwrap();
    core.state_mut().csr.write(machine::MCAUSE, 9).unwrap();
    core.state_mut().csr.write(machine::MTVAL, 0x77).unwrap();
    let cycle = core.state().csr.read(machine::MCYCLE).unwrap();
    let instret = core.state().csr.read(machine::MINSTRET).unwrap();
    assert_eq!(
        core.debug_execute(addi(1, 0, 3)),
        DebugExecOutcome::Completed
    );
    // Misaligned load faults inside the buffer.
    core.state_mut().regs[2] = 0x101;
    assert_eq!(core.debug_execute(ld(3, 2, 0)), DebugExecOutcome::Exception);
    assert_eq!(core.state().regs[1], 3, "earlier effect retained");
    let csr = &core.state().csr;
    assert_eq!(csr.read(machine::MEPC).unwrap(), 0x1234);
    assert_eq!(csr.read(machine::MCAUSE).unwrap(), 9);
    assert_eq!(csr.read(machine::MTVAL).unwrap(), 0x77);
    assert_eq!(csr.read(machine::MCYCLE).unwrap(), cycle);
    assert_eq!(csr.read(machine::MINSTRET).unwrap(), instret);
    assert!(core.in_debug_mode());
}

#[test]
fn explicit_counter_write_is_kept_but_work_adds_no_charge() {
    let (mut core, _) = halted_core(&[0x13]);
    let instret = core.state().csr.read(machine::MINSTRET).unwrap();
    core.debug_execute(addi(1, 0, 100));
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), instret);
    core.debug_execute(csrrw(0, machine::MINSTRET as u32, 1));
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 100);
}

#[test]
fn debug_execute_outside_debug_mode_fails() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let mut core = RiscvCore::new(memory.clone(), memory);
    assert!(matches!(
        core.debug_execute(addi(1, 0, 1)),
        DebugExecOutcome::Failure(_)
    ));
}

/// Ordinary guest path: FENCE.I makes a store-modified instruction visible.
#[test]
fn guest_fence_i_self_modifying_code_executes_new_instruction() {
    // 0: addi x2,x0,0x13 ; 4: sw x2,16(x0)...: patch word at 16 with `addi x0,x0,0`-like
    // value (0x13 = nop). Word at 16 starts as addi x5,x0,1, becomes nop.
    let sw: u32 = (2 << 20) | (2 << 12) | (16 << 7) | 0x23;
    let words = [addi(2, 0, 0x13), sw, FENCE_I, addi(6, 0, 1), addi(5, 0, 1)];
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    {
        let mut m = memory.lock().unwrap();
        for (i, w) in words.iter().enumerate() {
            m.write_word(i as u64 * 4, *w).unwrap();
        }
    }
    let mut core = RiscvCore::new(memory.clone(), memory);
    core.reset(0, 0);
    for _ in 0..5 {
        assert!(matches!(
            core.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }
    assert_eq!(core.state().regs[6], 1);
    assert_eq!(core.state().regs[5], 0, "patched instruction is a nop");
}

#[test]
fn guest_fence_i_does_not_trap() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    memory.lock().unwrap().write_word(0, FENCE_I).unwrap();
    let mut core = RiscvCore::new(memory.clone(), memory);
    core.reset(0, 0);
    match core.step_outcome() {
        StepOutcome::InstructionRetired(r) => assert_eq!(r.next_pc, 4),
        StepOutcome::TrapEntered(t) => {
            assert_ne!(t.cause, Trap::Exception(ExceptionCause::IllegalInstruction))
        }
        other => panic!("{other:?}"),
    }
}

/// Debug path: patch the guest instruction at dpc, FENCE.I, then resume.
#[test]
fn debug_fence_i_self_modifying_code_is_seen_after_resume() {
    let (mut core, mem) = halted_core(&[addi(5, 0, 1)]);
    core.state_mut().regs[2] = 0x13; // nop
    let sw0: u32 = (2 << 20) | (2 << 12) | 0x23; // sw x2, 0(x0)
    assert_eq!(core.debug_execute(sw0), DebugExecOutcome::Completed);
    assert_eq!(core.debug_execute(FENCE_I), DebugExecOutcome::Completed);
    assert_eq!(mem.lock().unwrap().read_word(0).unwrap(), 0x13);
    core.set_haltreq(false);
    assert!(core.debug_resume());
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0);
}

/// Ordinary public ELF path (flat Runner): FENCE.I is legal and the patched
/// instruction executes.
#[test]
fn public_elf_guest_fence_i_self_modifying_code() {
    use ruscv_sim::executor::RiscVSimulator;
    // 0x0: auipc x1,0 ; 0x4: addi x2,x0,0x13 ; 0x8: sw x2,16(x1)
    // 0xc: fence.i ; 0x10: addi x5,x0,1 (patched to nop) ; 0x14: nop
    let sw = (2u32 << 20) | (1 << 15) | (2 << 12) | (16 << 7) | 0x23;
    let code = [
        elf::auipc(1, 0),
        addi(2, 0, 0x13),
        sw,
        FENCE_I,
        addi(5, 0, 1),
        elf::nop(),
    ];
    let image = elf::elf_with_code(&code, 0, false, false, 0x4000);
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&image).unwrap();
    sim.run(Some(6)).unwrap();
    assert_eq!(sim.state().regs[5], 0, "patched instruction executed");
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 6);
}
