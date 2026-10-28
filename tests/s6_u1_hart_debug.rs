#[allow(dead_code)]
mod common;
use common::public_elf as elf;

use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::trap::{ExceptionCause, InterruptCause, Trap};
use ruscv_sim::core::{DebugCause, StepOutcome};
use ruscv_sim::csr::{machine, mstatus, unprivileged};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::executor::RiscVSimulator;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::{MemoryInterface, PrivilegeMode, RiscvCore, SimpleMemory};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const EBREAK: u32 = 0x0010_0073;
const ECALL: u32 = 0x0000_0073;
const DRET: u32 = 0x7b20_0073;
const WFI: u32 = 0x1050_0073;
const EBREAKM: u64 = 1 << 15;
const EBREAKS: u64 = 1 << 13;
const EBREAKU: u64 = 1 << 12;
const STEP: u64 = 1 << 2;
const MTIE: i32 = 0x80;

fn dcsr_cause(dcsr: u64) -> u64 {
    (dcsr >> 6) & 0b111
}

fn dcsr_prv(dcsr: u64) -> u64 {
    dcsr & 0b11
}

fn csrw(csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | 0x73
}

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn lr_d(rd: u8, rs1: u8) -> u32 {
    (0b00010 << 27) | (u32::from(rs1) << 15) | (0b011 << 12) | (u32::from(rd) << 7) | 0b010_1111
}

fn sc_d(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (0b00011 << 27)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (0b011 << 12)
        | (u32::from(rd) << 7)
        | 0b010_1111
}

fn core_with(words: &[u32]) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    {
        let mut store = memory.lock().expect("memory");
        for (index, word) in words.iter().enumerate() {
            store
                .write_word(index as u64 * 4, *word)
                .expect("plant instruction");
        }
    }
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    (core, memory)
}

fn machine_with(code: &[u32]) -> Machine {
    let bytes = elf::elf_with_code(code, 0, true, false, elf::BSS_MEMORY_SIZE);
    let image = Arc::new(LoadImage::parse(&bytes).unwrap());
    let machine = Machine::new(MachineConfig::new(PlatformKind::Native));
    machine.install(image).unwrap();
    machine.resume().unwrap();
    machine
}

fn pc_at(index: usize) -> u64 {
    elf::BASE + (index as u64) * 4
}

fn reg(machine: &Machine, index: usize) -> u64 {
    machine.inspect().unwrap().hart.regs[index]
}

fn csr_of(machine: &Machine, addr: u16) -> u64 {
    let mut csr = machine.inspect().unwrap().hart.csr;
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(addr).unwrap()
}

fn core_csr(core: &RiscvCore, addr: u16) -> u64 {
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(addr).unwrap()
}

fn retire(machine: &Machine, count: usize) {
    for index in 0..count {
        let turn = machine.step(false).expect("step");
        assert!(
            turn.hart().control.retired,
            "instruction {index} did not retire: {:?}",
            turn.hart().outcome
        );
        drop(turn);
    }
}

fn take_turn(machine: &Machine) -> StepOutcome {
    let turn = machine.step(false).expect("step");
    let outcome = turn.hart().outcome.clone();
    drop(turn);
    outcome
}

fn add(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x33
}

fn future_wfi() -> Vec<u32> {
    vec![
        elf::lui(9, 0x0200C),
        elf::addi(9, 9, -8),
        elf::ld(5, 9, 0),
        elf::lui(9, 0x02004),
        elf::lui(6, 1),
        add(5, 5, 6),
        elf::sd(5, 9, 0),
        elf::addi(5, 0, MTIE),
        csrw(machine::MIE, 5),
        elf::addi(24, 0, 0x22),
        WFI,
        elf::addi(24, 24, 1),
    ]
}

fn due_timer_then_target() -> Vec<u32> {
    vec![
        elf::addi(5, 0, 8),
        csrrs(0, machine::MSTATUS, 5),
        elf::lui(6, 0x02004),
        elf::sd(0, 6, 0),
        elf::addi(5, 0, MTIE),
        csrw(machine::MIE, 5),
        elf::addi(24, 0, 7),
        elf::addi(24, 24, 1),
    ]
}

/// Debug 1.0 `dcsr` field table and `dpc` / scratch registers. §4: the four
/// registers exist, with the D5 reset and WARL values.
#[test]
fn dcsr_dpc_and_scratch_exist_with_the_decided_warl_rules() {
    let (mut core, _) = core_with(&[]);
    let reset = core.dcsr();
    assert_eq!(reset >> 28, 4, "debugver");
    assert_eq!(dcsr_cause(reset), 0);
    assert_eq!(dcsr_prv(reset), PrivilegeMode::Machine as u64);
    assert_eq!(reset & (1 << 17), 0, "ebreakvs");
    assert_eq!(reset & (1 << 16), 0, "ebreakvu");
    assert_eq!(reset & EBREAKM, 0);
    assert_eq!(reset & EBREAKS, 0);
    assert_eq!(reset & EBREAKU, 0);
    assert_eq!(reset & (1 << 11), 0, "stepie");
    assert_ne!(reset & (1 << 10), 0, "stopcount");
    assert_ne!(reset & (1 << 9), 0, "stoptime");
    assert_eq!(reset & (1 << 5), 0, "v");
    assert_ne!(reset & (1 << 4), 0, "mprven");
    assert_eq!(reset & (1 << 3), 0, "nmip");
    assert_eq!(reset & STEP, 0);
    assert_eq!(core.dpc(), 0);
    assert_eq!(core.dscratch0(), 0);
    assert_eq!(core.dscratch1(), 0);

    let written = core.write_dcsr(
        EBREAKM | EBREAKS | EBREAKU | STEP | (1 << 11) | (1 << 17) | (1 << 16) | (1 << 5) | 3,
    );
    assert_eq!(written & EBREAKM, EBREAKM);
    assert_eq!(written & EBREAKS, EBREAKS);
    assert_eq!(written & EBREAKU, EBREAKU);
    assert_eq!(written & STEP, STEP);
    assert_eq!(written & (1 << 11), 0, "stepie stays 0");
    assert_eq!(written & (1 << 17), 0, "ebreakvs stays 0");
    assert_eq!(written & (1 << 16), 0, "ebreakvu stays 0");
    assert_eq!(written & (1 << 5), 0, "v stays 0");
    assert_ne!(written & (1 << 10), 0, "stopcount stays 1");
    assert_ne!(written & (1 << 9), 0, "stoptime stays 1");
    assert_ne!(written & (1 << 4), 0, "mprven stays 1");
    assert_eq!(dcsr_cause(written), 0, "cause is read-only");

    assert_eq!(dcsr_prv(core.write_dcsr(2)), PrivilegeMode::Machine as u64);
    assert_eq!(dcsr_prv(core.write_dcsr(0)), PrivilegeMode::User as u64);
    assert_eq!(dcsr_prv(core.write_dcsr(2)), PrivilegeMode::User as u64);
    assert_eq!(
        dcsr_prv(core.write_dcsr(1)),
        PrivilegeMode::Supervisor as u64
    );
    assert_eq!(dcsr_prv(core.write_dcsr(3)), PrivilegeMode::Machine as u64);

    assert_eq!(core.write_dpc(0x1003), 0x1000);
    core.write_dscratch0(0x1111);
    core.write_dscratch1(0x2222);
    assert_eq!(core.dscratch0(), 0x1111);
    assert_eq!(core.dscratch1(), 0x2222);

    core.set_haltreq(true);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::DebugHalted { .. }
    ));
    assert_eq!(dcsr_cause(core.dcsr()), DebugCause::HaltReq.code());
    let written = core.write_dcsr((DebugCause::Ebreak.code() << 6) | 3);
    assert_eq!(dcsr_cause(written), DebugCause::HaltReq.code());
}

/// Debug 1.0 Halt, and the `dpc` column for ebreak, halt request, and single
/// step. §7 item 1: U1 shows each halt cause's `dcsr.cause`, `dcsr.prv`, and
/// `dpc`.
#[test]
fn each_halt_cause_reports_dcsr_cause_prv_and_dpc() {
    let (mut halt, _) = core_with(&[elf::addi(1, 0, 1)]);
    halt.state_mut().privilege = PrivilegeMode::Supervisor;
    halt.state_mut().pc = 0x200;
    halt.set_haltreq(true);
    match halt.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            completed_turn: false,
        } => {}
        other => panic!("haltreq did not halt: {other:?}"),
    }
    let dcsr = halt.dcsr();
    assert_eq!(dcsr_cause(dcsr), DebugCause::HaltReq.code());
    assert_eq!(dcsr_prv(dcsr), PrivilegeMode::Supervisor as u64);
    assert_eq!(halt.dpc(), 0x200);

    let (mut ebreak, _) = core_with(&[EBREAK]);
    ebreak.state_mut().privilege = PrivilegeMode::User;
    ebreak.state_mut().csr.set_privilege(PrivilegeMode::User);
    ebreak.write_dcsr(EBREAKU);
    match ebreak.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            completed_turn: false,
        } => {}
        other => panic!("ebreak did not halt: {other:?}"),
    }
    let dcsr = ebreak.dcsr();
    assert_eq!(dcsr_cause(dcsr), DebugCause::Ebreak.code());
    assert_eq!(dcsr_prv(dcsr), PrivilegeMode::User as u64);
    assert_eq!(ebreak.dpc(), 0);

    let (mut step, _) = core_with(&[elf::addi(1, 0, 5)]);
    step.write_dcsr(STEP);
    match step.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        } => {}
        other => panic!("step did not halt: {other:?}"),
    }
    let dcsr = step.dcsr();
    assert_eq!(dcsr_cause(dcsr), DebugCause::Step.code());
    assert_eq!(dcsr_prv(dcsr), PrivilegeMode::Machine as u64);
    assert_eq!(step.dpc(), 4);
    assert_eq!(step.state().regs[1], 5);
}

/// Debug 1.0 `dcsr.ebreaks`. An S-mode EBREAK redirects only when `ebreaks`
/// is 1. `ebreaku` does not.
#[test]
fn supervisor_ebreak_redirects_only_when_ebreaks_is_set() {
    let (mut core, _) = core_with(&[EBREAK]);
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.write_dcsr(EBREAKU);
    match core.step_outcome() {
        StepOutcome::TrapEntered(trap) => {
            assert_eq!(trap.cause, Trap::Exception(ExceptionCause::Breakpoint));
        }
        other => panic!("ebreaku redirected an S-mode EBREAK: {other:?}"),
    }

    let (mut core, _) = core_with(&[EBREAK]);
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.write_dcsr(EBREAKS);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            completed_turn: false,
        } => {}
        other => panic!("ebreaks did not redirect an S-mode EBREAK: {other:?}"),
    }
    assert_eq!(dcsr_prv(core.dcsr()), PrivilegeMode::Supervisor as u64);
}

/// Privileged §3.1.15 cause 3. §7 item 1: with the ebreak bit clear, EBREAK
/// still traps with cause 3.
#[test]
fn ebreak_with_the_bit_clear_traps_with_cause_3() {
    let (mut core, _) = core_with(&[EBREAK]);
    core.state_mut()
        .csr
        .write(machine::MTVEC, 0x80)
        .expect("mtvec");
    match core.step_outcome() {
        StepOutcome::TrapEntered(trap) => {
            assert_eq!(trap.cause, Trap::Exception(ExceptionCause::Breakpoint));
            assert_eq!(trap.faulting_pc, 0);
        }
        other => panic!("clear ebreakm trapped as {other:?}"),
    }
    assert_eq!(
        core_csr(&core, machine::MCAUSE),
        ExceptionCause::Breakpoint as u64
    );
    assert_eq!(core_csr(&core, machine::MEPC), 0);
    assert!(!core.in_debug_mode());
}

/// Debug 1.0 `dcsr.ebreakm` and `dcsr.stopcount`. §7 item 1: with the bit set,
/// `mepc` and `mcause` stay unchanged and `mcycle` and `minstret` do not
/// increase.
#[test]
fn ebreak_with_the_bit_set_leaves_mepc_mcause_mcycle_and_minstret() {
    let (mut core, _) = core_with(&[elf::addi(1, 0, 1), EBREAK]);
    core.state_mut().csr.write(machine::MEPC, 0x80).unwrap();
    core.state_mut()
        .csr
        .write(
            machine::MCAUSE,
            (1 << 63) | InterruptCause::MachineTimer as u64,
        )
        .unwrap();
    core.step_outcome();
    let mepc = core_csr(&core, machine::MEPC);
    let mcause = core_csr(&core, machine::MCAUSE);
    assert_ne!(mepc, 0);
    assert_ne!(mcause, 0);
    let mcycle = core_csr(&core, machine::MCYCLE);
    let minstret = core_csr(&core, machine::MINSTRET);
    core.write_dcsr(EBREAKM);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            completed_turn: false,
        } => {}
        other => panic!("ebreakm did not halt: {other:?}"),
    }
    assert_eq!(core.dpc(), 4);
    assert_eq!(core.state().pc, 4);
    assert_eq!(core_csr(&core, machine::MEPC), mepc);
    assert_eq!(core_csr(&core, machine::MCAUSE), mcause);
    assert_eq!(core_csr(&core, machine::MCYCLE), mcycle);
    assert_eq!(core_csr(&core, machine::MINSTRET), minstret);
}

/// Debug 1.0 Sdext, Wait for Interrupt, and the halt-request row of the `dpc`
/// table. §7 item 1: a halt during a WFI wait sets `dpc` to the WFI address
/// plus 4 and clears the wait.
#[test]
fn halt_during_wfi_sets_dpc_to_wfi_plus_four_and_clears_the_wait() {
    let code = future_wfi();
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    let machine = machine_with(&code);
    retire(&machine, wfi + 1);
    assert_eq!(machine.inspect().unwrap().hart.pc, pc_at(wfi + 1));
    let waiting = take_turn(&machine);
    assert!(
        matches!(waiting, StepOutcome::Waiting),
        "the hart was not waiting before haltreq: {waiting:?}"
    );
    machine.set_haltreq(true).unwrap();
    let halted = take_turn(&machine);
    assert!(matches!(
        halted,
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            completed_turn: false,
        }
    ));
    assert_eq!(machine.dpc().unwrap(), pc_at(wfi + 1));
    machine.set_haltreq(false).unwrap();
    assert!(machine.debug_resume().unwrap());
    let next = take_turn(&machine);
    assert!(
        matches!(next, StepOutcome::InstructionRetired(_)),
        "the wait was still set: {next:?}"
    );
    assert_eq!(reg(&machine, 24), 0x23);
}

/// Debug 1.0 Sdext, Single Step: a stepped WFI is a nop and does not stall.
/// §7 item 1: a single-step of WFI does not wait.
#[test]
fn single_step_of_wfi_does_not_wait() {
    let code = future_wfi();
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    let machine = machine_with(&code);
    retire(&machine, wfi);
    let before = csr_of(&machine, machine::MINSTRET);
    machine.write_dcsr(STEP).unwrap();
    let stepped = take_turn(&machine);
    assert!(matches!(
        stepped,
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        }
    ));
    assert_eq!(machine.dpc().unwrap(), pc_at(wfi + 1));
    assert_eq!(csr_of(&machine, machine::MINSTRET), before + 1);
    assert!(
        !matches!(stepped, StepOutcome::Waiting),
        "the stepped WFI waited"
    );
    machine.write_dcsr(0).unwrap();
    assert!(machine.debug_resume().unwrap());
    let next = take_turn(&machine);
    assert!(
        matches!(next, StepOutcome::InstructionRetired(_)),
        "resume after a stepped WFI waited: {next:?}"
    );
}

/// Debug 1.0 `dcsr.stepie` value 0. §7 item 1: a single-step with a takeable
/// timer pending executes one instruction, increases `minstret` by one, and
/// does not take the interrupt.
#[test]
fn single_step_with_a_takeable_timer_pending_executes_one_and_skips_the_interrupt() {
    let code = due_timer_then_target();
    let target = code.len() - 2;
    let machine = machine_with(&code);
    retire(&machine, target);
    let minstret = csr_of(&machine, machine::MINSTRET);
    let mcause = csr_of(&machine, machine::MCAUSE);
    let mtime = csr_of(&machine, unprivileged::TIME);
    machine.write_dcsr(STEP).unwrap();
    let stepped = take_turn(&machine);
    assert!(matches!(
        stepped,
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        }
    ));
    assert_eq!(reg(&machine, 24), 7);
    assert_eq!(csr_of(&machine, machine::MINSTRET), minstret + 1);
    assert_eq!(csr_of(&machine, machine::MCAUSE), mcause);
    assert_eq!(machine.dpc().unwrap(), pc_at(target + 1));
    assert_eq!(
        csr_of(&machine, unprivileged::TIME),
        mtime + 1,
        "the stepped instruction counts once"
    );
    let again = take_turn(&machine);
    assert!(matches!(
        again,
        StepOutcome::DebugHalted {
            completed_turn: false,
            ..
        }
    ));
    assert_eq!(csr_of(&machine, unprivileged::TIME), mtime + 1);
}

/// Debug 1.0 Sdext, Single Step: a trap re-enters Debug Mode at the handler
/// with `mepc` and `mcause` updated, and the handler does not execute.
/// §7 item 1: a single-step into `ecall` stops at the handler.
#[test]
fn single_step_into_ecall_stops_at_the_handler() {
    let (mut core, _) = core_with(&[ECALL, elf::addi(1, 0, 99)]);
    core.state_mut()
        .csr
        .write(machine::MTVEC, 4)
        .expect("mtvec");
    let minstret = core_csr(&core, machine::MINSTRET);
    core.write_dcsr(STEP);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        } => {}
        other => panic!("ecall step did not halt: {other:?}"),
    }
    assert_eq!(core.dpc(), 4);
    assert_eq!(core.state().pc, 4);
    assert_eq!(core.state().regs[1], 0, "the handler executed");
    assert_eq!(core_csr(&core, machine::MEPC), 0);
    assert_eq!(
        core_csr(&core, machine::MCAUSE),
        ExceptionCause::EcallM as u64
    );
    assert_eq!(core_csr(&core, machine::MINSTRET), minstret);
    assert_eq!(dcsr_prv(core.dcsr()), PrivilegeMode::Machine as u64);
}

/// Debug 1.0 cause priority (`haltreq` above the instruction) and privileged
/// §3.1.9 after resume. Contract D6. §7 item 1: `haltreq` beside a takeable
/// interrupt stops at the current PC, and a later resume takes the interrupt
/// with `mepc` at that PC.
#[test]
fn haltreq_beside_a_takeable_interrupt_stops_at_that_pc_and_resume_takes_it() {
    let code = due_timer_then_target();
    let target = code.len() - 2;
    let machine = machine_with(&code);
    retire(&machine, target);
    let pc = pc_at(target);
    let minstret = csr_of(&machine, machine::MINSTRET);
    let mepc = csr_of(&machine, machine::MEPC);
    machine.set_haltreq(true).unwrap();
    let halted = take_turn(&machine);
    assert!(matches!(
        halted,
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            completed_turn: false,
        }
    ));
    assert_eq!(machine.dpc().unwrap(), pc);
    assert_eq!(machine.inspect().unwrap().hart.pc, pc);
    assert_eq!(reg(&machine, 24), 0);
    assert_eq!(csr_of(&machine, machine::MINSTRET), minstret);
    assert_eq!(csr_of(&machine, machine::MEPC), mepc);
    assert_eq!(
        dcsr_cause(machine.dcsr().unwrap()),
        DebugCause::HaltReq.code()
    );

    machine.set_haltreq(false).unwrap();
    assert!(machine.debug_resume().unwrap());
    let taken = take_turn(&machine);
    match &taken {
        StepOutcome::TrapEntered(trap) => {
            assert_eq!(trap.cause, Trap::Interrupt(InterruptCause::MachineTimer));
        }
        other => panic!("resume did not take the timer: {other:?}"),
    }
    assert_eq!(csr_of(&machine, machine::MEPC), pc);
}

/// Debug 1.0 Sdext, Resume, and privileged `mstatus.MPRV`. MPRV is cleared
/// only when the resumed privilege is below M. Resume to M keeps it.
#[test]
fn resume_to_machine_keeps_mprv() {
    let (mut core, _) = core_with(&[]);
    core.state_mut()
        .csr
        .write(machine::MSTATUS, mstatus::MPRV)
        .expect("mprv");
    core.set_haltreq(true);
    core.step_outcome();
    core.set_haltreq(false);
    core.write_dcsr(PrivilegeMode::Machine as u64);
    assert!(core.debug_resume());
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
    assert_ne!(core_csr(&core, machine::MSTATUS) & mstatus::MPRV, 0);
}

/// Debug 1.0 Sdext, Resume: below M, `mstatus.MPRV` is cleared. §7 item 1:
/// resume to U clears MPRV. The same sentence in the spec covers S.
#[test]
fn resume_to_user_clears_mprv() {
    for privilege in [PrivilegeMode::User, PrivilegeMode::Supervisor] {
        let (mut core, _) = core_with(&[]);
        core.state_mut()
            .csr
            .write(machine::MSTATUS, mstatus::MPRV)
            .expect("mprv");
        assert_ne!(core_csr(&core, machine::MSTATUS) & mstatus::MPRV, 0);
        core.set_haltreq(true);
        core.step_outcome();
        core.set_haltreq(false);
        core.write_dcsr(privilege as u64);
        assert!(core.debug_resume());
        assert_eq!(core.state().privilege, privilege);
        assert_eq!(core_csr(&core, machine::MSTATUS) & mstatus::MPRV, 0);
    }
}

/// Debug 1.0 Sdext: core debug registers are accessible only from Debug Mode,
/// and an absent Sdtrig CSR is an illegal instruction. §7 item 1: a guest CSR
/// access to `0x7b0` through `0x7b3`, and to `0x7a0`, is illegal. Contract §2
/// keeps `dret` illegal as well.
#[test]
fn guest_csr_access_to_debug_and_tselect_is_illegal() {
    let addresses = [0x7b0u16, 0x7b1, 0x7b2, 0x7b3, 0x7a0];
    for address in addresses {
        let (mut core, _) = core_with(&[csrrs(5, address, 0)]);
        match core.step_outcome() {
            StepOutcome::TrapEntered(trap) => {
                assert_eq!(
                    trap.cause,
                    Trap::Exception(ExceptionCause::IllegalInstruction)
                );
            }
            other => panic!("csr {address:#x} was not illegal: {other:?}"),
        }
        assert_eq!(
            core_csr(&core, machine::MCAUSE),
            ExceptionCause::IllegalInstruction as u64
        );
        assert_eq!(core.state().regs[5], 0);
        assert!(!core.in_debug_mode());
    }
    let (mut core, _) = core_with(&[DRET]);
    match core.step_outcome() {
        StepOutcome::TrapEntered(trap) => {
            assert_eq!(
                trap.cause,
                Trap::Exception(ExceptionCause::IllegalInstruction)
            );
        }
        other => panic!("dret was not illegal: {other:?}"),
    }
}

/// Debug 1.0 Sdext, Load-Reserved/Store-Conditional. Contract D7. §7 item 1:
/// an SC after a halt fails.
#[test]
fn sc_after_a_halt_fails() {
    let (mut core, memory) = core_with(&[lr_d(11, 10), sc_d(12, 10, 13)]);
    memory
        .lock()
        .expect("memory")
        .write_dword(0x200, 0x1111_2222_3333_4444)
        .expect("seed");
    core.state_mut().regs[10] = 0x200;
    core.state_mut().regs[13] = 0x55;
    match core.step_outcome() {
        StepOutcome::InstructionRetired(_) => {}
        other => panic!("LR did not retire: {other:?}"),
    }
    assert!(core.state().reservation.is_some());
    core.set_haltreq(true);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            completed_turn: false,
        } => {}
        other => panic!("halt after LR failed: {other:?}"),
    }
    assert!(core.state().reservation.is_none());
    core.set_haltreq(false);
    assert!(core.debug_resume());
    match core.step_outcome() {
        StepOutcome::InstructionRetired(_) => {}
        other => panic!("SC after halt did not retire: {other:?}"),
    }
    assert_eq!(core.state().regs[12], 1);
    assert_eq!(
        memory
            .lock()
            .expect("memory")
            .read_dword(0x200)
            .expect("load"),
        0x1111_2222_3333_4444
    );
}

/// Debug 1.0 `dcsr.stoptime`, privileged §3.1.10, contract D9. §7 item 1:
/// `mtime` does not move while the hart is halted.
#[test]
fn mtime_does_not_move_while_the_hart_is_halted() {
    let machine = machine_with(&[elf::addi(1, 0, 1), elf::addi(1, 1, 1)]);
    retire(&machine, 1);
    let mtime = csr_of(&machine, unprivileged::TIME);
    let mcycle = csr_of(&machine, machine::MCYCLE);
    let minstret = csr_of(&machine, machine::MINSTRET);
    machine.set_haltreq(true).unwrap();
    let halted = take_turn(&machine);
    assert!(matches!(
        halted,
        StepOutcome::DebugHalted {
            completed_turn: false,
            ..
        }
    ));
    assert_eq!(csr_of(&machine, unprivileged::TIME), mtime);
    assert_eq!(csr_of(&machine, machine::MCYCLE), mcycle);
    assert_eq!(csr_of(&machine, machine::MINSTRET), minstret);
    let again = take_turn(&machine);
    assert!(matches!(
        again,
        StepOutcome::DebugHalted {
            completed_turn: false,
            ..
        }
    ));
    assert_eq!(csr_of(&machine, unprivileged::TIME), mtime);
    assert_eq!(csr_of(&machine, machine::MCYCLE), mcycle);
    assert_eq!(csr_of(&machine, machine::MINSTRET), minstret);
}

/// D9. A single-step retires the instruction. The outcome stays
/// `DebugHalted`, and the retirement is still the control fact and the
/// commit observation. `minstret` moves from 0 to 1.
#[test]
fn stepped_retirement_is_visible_while_the_outcome_stays_debug_halted() {
    let word = elf::addi(1, 0, 1);
    let (mut core, _) = core_with(&[word]);
    core.write_dcsr(core.dcsr() | STEP);
    let turn = core.step_transition(true);
    assert!(matches!(
        turn.outcome,
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        }
    ));
    assert!(turn.control.retired);
    assert!(!turn.control.trap_entered);
    assert_eq!(turn.control.minstret_before, 0);
    assert_eq!(turn.control.minstret_after, 1);
    match turn.observation {
        Some(Observation::Commit(record)) => assert_eq!(record.retired.instruction, word),
        other => panic!("stepped retirement had no commit observation: {other:?}"),
    }
}

/// D9 and D12. §5 keeps the Stage 5 exit check on a completed turn. A stepped
/// `sd` of the guest exit still exits 0.
#[test]
fn stepped_sd_to_tohost_exits_zero() {
    let code = [
        elf::auipc(5, 1),
        elf::addi(6, 0, 1),
        elf::slli(6, 6, 63),
        elf::sd(6, 5, 0),
    ];
    let bytes = elf::elf_with_code(&code, 0, true, false, elf::BSS_MEMORY_SIZE);
    let mut sim = RiscVSimulator::new(0x20_000);
    sim.load_elf(&bytes).expect("flat load");
    for _ in 0..3 {
        sim.step().expect("setup");
    }
    sim.write_dcsr(sim.dcsr() | STEP);
    let result = sim.run(Some(10)).expect("flat run");
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.timed_out);
}

/// Debug 1.0 `dpc` table, halt request: the address of the next instruction,
/// which is the current PC. §7 item 1 negative mutation: storing `dpc` as the
/// PC plus 4 on `haltreq` must fail.
#[test]
fn haltreq_stores_dpc_as_the_current_pc() {
    let (mut core, _) = core_with(&[elf::addi(1, 0, 1)]);
    core.state_mut().pc = 0x40;
    core.set_haltreq(true);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            ..
        } => {}
        other => panic!("haltreq did not halt: {other:?}"),
    }
    assert_eq!(core.dpc(), 0x40);
    assert_ne!(core.dpc(), 0x44, "haltreq must not store the following PC");
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn assemble(name: &str, dir: &Path) -> PathBuf {
    let prefix = env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{prefix}as");
    let linker = format!("{prefix}ld");
    assert!(
        command_available(&assembler) && command_available(&linker),
        "toolchain checked by the caller"
    );
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = repository
        .join("tests/bare-metal-riscv-test/rv64i")
        .join(format!("{name}.S"));
    let script = repository.join("tests/bare-metal-riscv-test/linker.ld");
    let object = dir.join(format!("{name}.o"));
    let elf_path = dir.join(format!("{name}.elf"));
    let assembled = Command::new(&assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("as");
    assert!(
        assembled.status.success(),
        "assemble {}:\n{}",
        name,
        String::from_utf8_lossy(&assembled.stderr)
    );
    let linked = Command::new(&linker)
        .arg(format!("-T{}", script.display()))
        .arg(&object)
        .arg("-o")
        .arg(&elf_path)
        .output()
        .expect("ld");
    assert!(
        linked.status.success(),
        "link {}:\n{}",
        name,
        String::from_utf8_lossy(&linked.stderr)
    );
    elf_path
}

/// The same `s6_ebreak` guest the CLI runs. With `ebreakm` clear it traps and
/// exits 0. With `ebreakm` set through the library, the run stops in Debug
/// Mode and `ExecutionResult.cycles` does not count that entry (contract D9).
#[test]
fn s6_ebreak_guest_traps_on_the_cli_and_halts_when_ebreakm_is_set() {
    let prefix = env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    if !command_available(&format!("{prefix}as")) || !command_available(&format!("{prefix}ld")) {
        let message = "riscv64-unknown-elf-as/ld unavailable; s6 guest ELF skipped";
        if env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
            panic!("{message}; CI requested a real guest run");
        }
        eprintln!("[SKIP] {message}");
        return;
    }
    let temp = tempfile::TempDir::new().expect("guest build dir");
    let ebreak = assemble("s6_ebreak", temp.path());
    let csr = assemble("s6_debug_csr", temp.path());
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"));

    for guest in [&ebreak, &csr] {
        let ran = Command::new(&cli)
            .args(["run", guest.to_str().expect("utf-8")])
            .args(["--max-cycles", "100000"])
            .output()
            .expect("cargo-built simulator");
        assert!(
            ran.status.success(),
            "CLI {} failed:\nstdout:\n{}\nstderr:\n{}",
            guest.display(),
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        );
        let text = String::from_utf8_lossy(&ran.stdout);
        assert!(
            text.contains("Status:     SUCCESS"),
            "CLI status for {}:\n{text}",
            guest.display()
        );
    }

    let bytes = std::fs::read(&ebreak).expect("elf");
    let image = Arc::new(LoadImage::parse(&bytes).unwrap());
    let trapped = Machine::new(MachineConfig::new(PlatformKind::Native));
    trapped.install(image.clone()).unwrap();
    trapped.resume().unwrap();
    let mut trap_minstret = None;
    let mut trap_mepc = None;
    let mut trap_mcycle = None;
    for _ in 0..10_000 {
        let outcome = take_turn(&trapped);
        if let StepOutcome::TrapEntered(trap) = &outcome {
            assert_eq!(trap.cause, Trap::Exception(ExceptionCause::Breakpoint));
            trap_minstret = Some(csr_of(&trapped, machine::MINSTRET));
            trap_mepc = Some(csr_of(&trapped, machine::MEPC));
            trap_mcycle = Some(csr_of(&trapped, machine::MCYCLE));
            break;
        }
    }
    let trap_minstret = trap_minstret.expect("ebreak did not trap");
    let trap_mepc = trap_mepc.expect("mepc");
    let trap_mcycle = trap_mcycle.expect("mcycle");

    let halted = Machine::new(MachineConfig::new(PlatformKind::Native));
    halted.install(image).unwrap();
    halted.resume().unwrap();
    halted.write_dcsr(EBREAKM).unwrap();
    let mut saw_halt = false;
    for _ in 0..10_000 {
        let outcome = take_turn(&halted);
        if let StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            completed_turn: false,
        } = outcome
        {
            saw_halt = true;
            break;
        }
    }
    assert!(saw_halt, "ebreakm did not halt the guest");
    assert_eq!(halted.dpc().unwrap(), trap_mepc);
    assert_eq!(csr_of(&halted, machine::MINSTRET), trap_minstret);
    assert_eq!(csr_of(&halted, machine::MCYCLE) + 1, trap_mcycle);
    assert_eq!(csr_of(&halted, machine::MEPC), 0);
    assert_eq!(csr_of(&halted, machine::MCAUSE), 0);
    assert_eq!(
        dcsr_cause(halted.dcsr().unwrap()),
        DebugCause::Ebreak.code()
    );

    let mut flat = RiscVSimulator::new(0x20_000);
    flat.load_elf(&bytes).expect("flat load");
    flat.write_dcsr(EBREAKM);
    let result = flat.run(Some(100_000)).expect("flat run");
    assert_eq!(result.error.as_deref(), Some("DebugHalted"));
    assert!(!result.timed_out);
    let mut csr = flat.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    assert_eq!(result.cycles, csr.read(machine::MINSTRET).unwrap());
    assert_eq!(flat.dpc(), flat.state().pc);
    assert_eq!(csr.read(machine::MEPC).unwrap(), 0);
    assert_eq!(csr.read(machine::MCAUSE).unwrap(), 0);
    assert_eq!(dcsr_cause(flat.dcsr()), DebugCause::Ebreak.code());
}
