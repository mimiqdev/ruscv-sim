//! Stage 4 D11 supervisor interrupt bits and delivery.

use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::{
    ExceptionCause, InterruptCause, PrivilegeMode, RiscvCore, StepOutcome, Trap, TrapEntered,
};
use ruscv_sim::csr::{machine, mstatus, supervisor, CsrError, CsrFile, SUPERVISOR_INTERRUPTS};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use ruscv_sim::isa::rv64a::ReservationSet;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::PhysicalWidth;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod elf;

const MIE: u64 = mstatus::MIE;
const MPIE: u64 = mstatus::MPIE;
const MPP: u64 = mstatus::MPP;
const SSIP: u64 = 1 << 1;
const STIP: u64 = 1 << 5;
const SEIP: u64 = 1 << 9;
const SSIE: u64 = SSIP;
const STIE: u64 = STIP;
const SEIE: u64 = SEIP;
const MTVEC: u64 = 0x100;
const NOP: u32 = 0x0000_0013;
const ECALL: u32 = 0x0000_0073;
const MRET: u32 = 0x3020_0073;
const SRET: u32 = 0x1020_0073;

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn csrrw(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | (u32::from(rd) << 7) | 0x73
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
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    core
}

fn machine_csr(core: &RiscvCore, addr: u16) -> u64 {
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(addr).unwrap()
}

fn arm(core: &mut RiscvCore, mode: PrivilegeMode, pending: u64, enable: u64, global: bool) {
    core.state_mut().csr.write(machine::MIP, pending).unwrap();
    core.state_mut().csr.write(machine::MIE, enable).unwrap();
    let preserved = core.state_mut().csr.read(machine::MSTATUS).unwrap() & !MIE;
    let status = preserved | if global { MIE } else { 0 };
    core.state_mut()
        .csr
        .write(machine::MSTATUS, status)
        .unwrap();
    core.state_mut().privilege = mode;
    core.state_mut().csr.set_privilege(mode);
}

fn assert_interrupt(core: &RiscvCore, entered: &TrapEntered, cause: InterruptCause, pc: u64) {
    assert_eq!(entered.cause, Trap::Interrupt(cause));
    assert_eq!(entered.faulting_pc, pc);
    assert_eq!(entered.mtval, 0);
    assert!(entered.instruction.is_none());
    assert_eq!(entered.handler_privilege, PrivilegeMode::Machine);
    assert_eq!(machine_csr(core, machine::MCAUSE), cause.code());
    assert_eq!(machine_csr(core, machine::MEPC), pc);
    assert_eq!(machine_csr(core, machine::MTVAL), 0);
    assert_eq!(machine_csr(core, machine::MINSTRET), 0);
    assert_eq!(core.state().pc, entered.vector_pc);
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
}

#[test]
fn enabled_pending_cause_is_taken_by_privilege_and_mie() {
    let cases = [
        (InterruptCause::SupervisorSoftware, SSIP, SSIE),
        (InterruptCause::SupervisorTimer, STIP, STIE),
        (InterruptCause::SupervisorExternal, SEIP, SEIE),
    ];
    for (cause, pending, enable) in cases {
        for mode in [
            PrivilegeMode::User,
            PrivilegeMode::Supervisor,
            PrivilegeMode::Machine,
        ] {
            for global in [false, true] {
                let mut core = direct_core(&[NOP]);
                arm(&mut core, mode, pending, enable, global);
                let before = machine_csr(&core, machine::MCYCLE);
                let turn = core.step_transition(true);
                let taken = mode != PrivilegeMode::Machine || global;
                if taken {
                    let StepOutcome::TrapEntered(entered) = &turn.outcome else {
                        panic!("{cause:?} {mode:?} MIE={global} was not taken");
                    };
                    assert_interrupt(&core, entered, cause, 0);
                    assert_eq!(entered.source_privilege, mode);
                    assert!(!turn.control.instruction_attempted);
                    assert!(!turn.control.retired);
                    assert!(turn.control.trap_entered);
                    assert_eq!(machine_csr(&core, machine::MCYCLE), before + 1);
                    assert_eq!(machine_csr(&core, machine::MINSTRET), 0);
                    let status = machine_csr(&core, machine::MSTATUS);
                    assert_eq!(status & MIE, 0);
                    assert_eq!((status & MPIE) != 0, global);
                    assert_eq!((status & MPP) >> 11, mode as u64);
                    assert_eq!(core.state().pc, MTVEC);
                    let Observation::Trap(record) = turn.observation.unwrap() else {
                        panic!("observation demand must record the interrupt");
                    };
                    assert!(record.effects.memory.is_empty());
                    assert_eq!(record.saved_pc, 0);
                    assert!(record.instruction_length.is_none());
                } else {
                    assert!(turn.control.retired);
                    assert!(turn.control.instruction_attempted);
                    assert_eq!(core.state().pc, 4);
                    assert_eq!(machine_csr(&core, machine::MCAUSE), 0);
                    assert_eq!(machine_csr(&core, machine::MINSTRET), 1);
                }
            }
        }
    }
}

#[test]
fn absent_or_locally_disabled_interrupt_retires_in_every_mode() {
    for mode in [
        PrivilegeMode::User,
        PrivilegeMode::Supervisor,
        PrivilegeMode::Machine,
    ] {
        for (pending, enable) in [(SSIP, 0), (0, SSIE), (SEIP, STIE)] {
            let mut core = direct_core(&[NOP]);
            arm(&mut core, mode, pending, enable, true);
            let turn = core.step_transition(true);
            assert!(turn.control.instruction_attempted && turn.control.retired);
            assert!(!turn.control.trap_entered);
            assert_eq!(core.state().pc, 4);
            assert_eq!(machine_csr(&core, machine::MIP), pending);
        }
    }
}

#[test]
fn interrupt_precedes_misaligned_pc_and_pending_reenters_after_mret() {
    let mut program = vec![NOP; MTVEC as usize / 4 + 1];
    program[MTVEC as usize / 4] = MRET;
    let mut core = direct_core(&program);
    arm(&mut core, PrivilegeMode::Machine, SSIP, SSIE, true);
    // PC = 2 is injected state. With IALIGN=32, no spec-defined path can
    // misalign the PC: a misaligned branch or jump target raises the
    // exception on that instruction, and neither the PC nor rd is updated.
    // Privileged spec 20250508 §3.1.14 (and §12.1.7 for sepc) says the two
    // low bits of mepc are always zero, including the write at trap entry.
    core.state_mut().pc = 2;
    let first = core.step_transition(true);
    assert!(!first.control.instruction_attempted);
    assert!(matches!(first.outcome, StepOutcome::TrapEntered(trap)
        if trap.cause == Trap::Interrupt(InterruptCause::SupervisorSoftware)
        && trap.faulting_pc == 2));
    assert_eq!(machine_csr(&core, machine::MEPC), 0);
    assert_eq!(core.state().pc, MTVEC);
    assert!(core.step_transition(false).control.retired);
    assert_eq!(core.state().pc, 0);
    let second = core.step_transition(false);
    assert!(!second.control.instruction_attempted);
    assert!(matches!(second.outcome, StepOutcome::TrapEntered(trap)
        if trap.cause == Trap::Interrupt(InterruptCause::SupervisorSoftware)
        && trap.faulting_pc == 0));
    assert_eq!(machine_csr(&core, machine::MCYCLE), 3);
    assert_eq!(machine_csr(&core, machine::MINSTRET), 1);
}

#[test]
fn priority_selects_sei_then_ssi_then_sti() {
    let subsets = [
        (
            SEIP | SSIP | STIP,
            SEIE | SSIE | STIE,
            InterruptCause::SupervisorExternal,
        ),
        (SSIP | STIP, SSIE | STIE, InterruptCause::SupervisorSoftware),
        (STIP, STIE, InterruptCause::SupervisorTimer),
        (SEIP | STIP, SEIE | STIE, InterruptCause::SupervisorExternal),
        (SEIP | SSIP, SEIE, InterruptCause::SupervisorExternal),
        (SSIP | STIP, STIE, InterruptCause::SupervisorTimer),
    ];
    for (pending, enable, expected) in subsets {
        let mut core = direct_core(&[NOP]);
        arm(&mut core, PrivilegeMode::User, pending, enable, false);
        let StepOutcome::TrapEntered(entered) = core.step_outcome() else {
            panic!("subset {pending:#x}/{enable:#x} was not taken");
        };
        assert_eq!(entered.cause, Trap::Interrupt(expected));
        assert_eq!(machine_csr(&core, machine::MIP), pending);
    }
}

#[test]
fn direct_and_vectored_targets_follow_mtvec() {
    let mut direct = direct_core(&[NOP]);
    direct.state_mut().csr.write(machine::MTVEC, 0x200).unwrap();
    arm(&mut direct, PrivilegeMode::Supervisor, SSIP, SSIE, false);
    direct.step_outcome();
    assert_eq!(direct.state().pc, 0x200);

    let mut vectored = direct_core(&[NOP]);
    vectored
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x300 | 1)
        .unwrap();
    arm(&mut vectored, PrivilegeMode::Supervisor, SEIP, SEIE, false);
    vectored.step_outcome();
    assert_eq!(vectored.state().pc, 0x300 + 4 * 9);

    let mut software = direct_core(&[NOP]);
    software
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x400 | 1)
        .unwrap();
    arm(&mut software, PrivilegeMode::User, SSIP, SSIE, false);
    software.step_outcome();
    assert_eq!(software.state().pc, 0x400 + 4);
}

#[test]
fn pending_bit_survives_entry_and_returns_after_it_is_cleared() {
    let mut core = direct_core(&[MRET, NOP]);
    arm(&mut core, PrivilegeMode::Machine, SSIP, SSIE, true);
    core.step_outcome();
    assert_eq!(machine_csr(&core, machine::MIP) & SSIP, SSIP);
    assert_eq!(core.state().pc, MTVEC);

    core.state_mut().pc = 0;
    core.state_mut().csr.write(machine::MEPC, 4).unwrap();
    core.state_mut().csr.write(machine::MIP, 0).unwrap();
    assert!(core.step_transition(false).control.retired);
    assert_eq!(core.state().pc, 4);
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
    assert!(core.step_transition(false).control.retired);
    assert_eq!(core.state().pc, 8);
    assert_eq!(machine_csr(&core, machine::MINSTRET), 2);
}

#[test]
fn csr_write_and_xret_deliver_at_the_next_boundary() {
    let cases = [
        vec![csrrw(0, machine::MSTATUS, 1), NOP],
        vec![csrrs(0, machine::MIE, 1), NOP],
        vec![csrrs(0, machine::MIP, 1), NOP],
    ];
    for program in cases {
        let mut core = direct_core(&program);
        core.state_mut().csr.write(machine::MIP, SSIP).unwrap();
        core.state_mut().regs[1] = SSIE;
        if program[0] == csrrw(0, machine::MSTATUS, 1) {
            core.state_mut().csr.write(machine::MIE, SSIE).unwrap();
            core.state_mut().regs[1] = MIE;
        }
        if program[0] == csrrs(0, machine::MIE, 1) {
            core.state_mut().csr.write(machine::MSTATUS, MIE).unwrap();
        }
        if program[0] == csrrs(0, machine::MIP, 1) {
            core.state_mut().csr.write(machine::MIP, 0).unwrap();
            core.state_mut().csr.write(machine::MSTATUS, MIE).unwrap();
            core.state_mut().csr.write(machine::MIE, SSIE).unwrap();
        }
        assert!(core.step_transition(false).control.retired);
        let turn = core.step_transition(false);
        let StepOutcome::TrapEntered(entered) = turn.outcome else {
            panic!("the boundary after the CSR write did not take SSIP");
        };
        assert_eq!(
            entered.cause,
            Trap::Interrupt(InterruptCause::SupervisorSoftware)
        );
        assert_eq!(entered.faulting_pc, 4);
        assert!(!turn.control.instruction_attempted);
    }

    let mut mret = direct_core(&[MRET]);
    mret.state_mut().csr.write(machine::MIP, SSIP).unwrap();
    mret.state_mut().csr.write(machine::MIE, SSIE).unwrap();
    mret.state_mut().csr.write(machine::MEPC, 0x20).unwrap();
    mret.state_mut().csr.write(machine::MSTATUS, MPIE).unwrap();
    let returned = mret.step_transition(false);
    assert!(returned.control.retired);
    assert_eq!(mret.state().pc, 0x20);
    assert_ne!(machine_csr(&mret, machine::MSTATUS) & MIE, 0);
    let taken = mret.step_transition(false);
    assert!(!taken.control.instruction_attempted);
    assert!(matches!(
        taken.outcome,
        StepOutcome::TrapEntered(entered)
            if entered.cause == Trap::Interrupt(InterruptCause::SupervisorSoftware)
                && entered.faulting_pc == 0x20
    ));

    let mut sret = direct_core(&[SRET]);
    sret.state_mut().csr.write(machine::MIP, STIP).unwrap();
    sret.state_mut().csr.write(machine::MIE, STIE).unwrap();
    sret.state_mut().csr.write(supervisor::SEPC, 0x30).unwrap();
    let preserved = sret.state_mut().csr.read(machine::MSTATUS).unwrap()
        & !(MIE | mstatus::SPP | mstatus::MPRV);
    sret.state_mut()
        .csr
        .write(machine::MSTATUS, preserved | mstatus::SPP)
        .unwrap();
    let returned = sret.step_transition(false);
    assert!(returned.control.retired, "{returned:?}");
    assert_eq!(sret.state().pc, 0x30);
    assert_eq!(sret.state().privilege, PrivilegeMode::Supervisor);
    let taken = sret.step_transition(false);
    assert!(!taken.control.instruction_attempted);
    assert!(matches!(
        taken.outcome,
        StepOutcome::TrapEntered(entered)
            if entered.cause == Trap::Interrupt(InterruptCause::SupervisorTimer)
                && entered.faulting_pc == 0x30
                && entered.source_privilege == PrivilegeMode::Supervisor
    ));
}

#[test]
fn interrupt_entry_probes_for_wfi_and_matches_with_observation_off() {
    let fetches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let memory = Arc::new(Mutex::new(CountingMemory {
        inner: SimpleMemory::new(0x1000),
        fetches: fetches.clone(),
    }));
    memory.lock().unwrap().inner.write_word(0, NOP).unwrap();
    let mut on = RiscvCore::new(memory.clone(), memory.clone());
    on.reset(0, 0);
    arm(
        &mut on,
        PrivilegeMode::User,
        SEIP | SSIP,
        SEIE | SSIE,
        false,
    );
    on.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    on.state_mut().csr.write(machine::MTVEC, 0x180 | 1).unwrap();
    on.state_mut().reservation = Some(ReservationSet::new(0x40, PhysicalWidth::Doubleword, None));
    let observed = on.step_transition(true);

    let mut off = RiscvCore::new(memory.clone(), memory);
    off.reset(0, 0);
    arm(
        &mut off,
        PrivilegeMode::User,
        SEIP | SSIP,
        SEIE | SSIE,
        false,
    );
    off.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    off.state_mut()
        .csr
        .write(machine::MTVEC, 0x180 | 1)
        .unwrap();
    off.state_mut().reservation = Some(ReservationSet::new(0x40, PhysicalWidth::Doubleword, None));
    let silent = off.step_transition(false);

    // Each accepted interrupt reads one word to see whether it is a legal
    // WFI. This NOP is not, so the read does not retire. Two cores, two reads.
    assert_eq!(fetches.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(observed.control, silent.control);
    assert_eq!(observed.outcome, silent.outcome);
    assert!(silent.observation.is_none());
    let reservation = Some(ReservationSet::new(0x40, PhysicalWidth::Doubleword, None));
    assert_eq!(on.state().reservation, reservation);
    assert_eq!(off.state().reservation, reservation);
    assert_eq!(on.state().pc, 0x180 + 4 * 9);
    assert_eq!(off.state().pc, on.state().pc);
    assert_eq!(
        machine_csr(&on, machine::MCAUSE),
        machine_csr(&off, machine::MCAUSE)
    );
    let Observation::Trap(record) = observed.observation.unwrap() else {
        panic!("enabled observation must record the interrupt");
    };
    assert!(record.effects.memory.is_empty());
    assert!(record.effects.reservation.is_none());
    assert!(record.effects.gpr.is_empty());
    assert!(record.effects.fpr.is_empty());
    assert!(record.effects.fcsr.is_none());
    assert_eq!(
        record
            .effects
            .csr
            .iter()
            .map(|write| (write.addr, write.old_value, write.new_value))
            .collect::<Vec<_>>(),
        vec![
            (machine::MEPC, 0, 0),
            (machine::MCAUSE, 0, (1 << 63) | 9),
            (machine::MTVAL, 0, 0),
            (machine::MSTATUS, 0xA_0000_0000, 0xA_0000_0000),
            (machine::MCYCLE, 0, 1),
        ]
    );
}

#[test]
fn masked_interrupt_does_not_prevent_synchronous_exception() {
    let mut core = direct_core(&[ECALL]);
    arm(&mut core, PrivilegeMode::Machine, SSIP, SSIE, false);
    let turn = core.step_transition(false);
    assert!(turn.control.instruction_attempted);
    assert!(matches!(
        turn.outcome,
        StepOutcome::TrapEntered(entered)
            if entered.cause == Trap::Exception(ExceptionCause::EcallM)
    ));
    assert_eq!(
        machine_csr(&core, machine::MCAUSE),
        ExceptionCause::EcallM.code()
    );
}

#[test]
fn masked_bits_and_supervisor_views_ignore_writes() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MIP, u64::MAX).unwrap();
    csr.write(machine::MIE, u64::MAX).unwrap();
    assert_eq!(csr.read(machine::MIP), Ok(SUPERVISOR_INTERRUPTS));
    assert_eq!(csr.read(machine::MIE), Ok(0xAAA));
    csr.set_privilege(PrivilegeMode::Supervisor);
    assert_eq!(csr.read(supervisor::SIP), Ok(0));
    assert_eq!(csr.read(supervisor::SIE), Ok(0));
    assert_eq!(csr.write(supervisor::SIP, u64::MAX), Ok(()));
    assert_eq!(csr.write(supervisor::SIE, u64::MAX), Ok(()));
    assert_eq!(csr.read(supervisor::SIP), Ok(0));
    assert_eq!(csr.read(supervisor::SIE), Ok(0));
    csr.set_privilege(PrivilegeMode::Machine);
    assert_eq!(csr.read(machine::MIP), Ok(SUPERVISOR_INTERRUPTS));
    assert_eq!(csr.read(machine::MIE), Ok(0xAAA));
    csr.set_privilege(PrivilegeMode::User);
    assert_eq!(
        csr.read(machine::MIP),
        Err(CsrError::PrivilegeViolation(machine::MIP))
    );
    assert_eq!(
        csr.read(supervisor::SIP),
        Err(CsrError::PrivilegeViolation(supervisor::SIP))
    );
}

struct CountingMemory {
    inner: SimpleMemory,
    fetches: Arc<std::sync::atomic::AtomicUsize>,
}

impl MemoryInterface for CountingMemory {
    fn read_dword(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_dword(addr)
    }
    fn read_word(&self, addr: u64) -> Result<u32, ruscv_sim::MemoryError> {
        self.fetches
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.read_word(addr)
    }
    fn read_half(&self, addr: u64) -> Result<u16, ruscv_sim::MemoryError> {
        self.inner.read_half(addr)
    }
    fn read_byte(&self, addr: u64) -> Result<u8, ruscv_sim::MemoryError> {
        self.inner.read_byte(addr)
    }
    fn read_word_zext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_word_zext(addr)
    }
    fn read_half_zext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_half_zext(addr)
    }
    fn read_byte_zext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_byte_zext(addr)
    }
    fn read_word_sext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_word_sext(addr)
    }
    fn read_half_sext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_half_sext(addr)
    }
    fn read_byte_sext(&self, addr: u64) -> Result<u64, ruscv_sim::MemoryError> {
        self.inner.read_byte_sext(addr)
    }
    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), ruscv_sim::MemoryError> {
        self.inner.write_dword(addr, value)
    }
    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), ruscv_sim::MemoryError> {
        self.inner.write_word(addr, value)
    }
    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), ruscv_sim::MemoryError> {
        self.inner.write_half(addr, value)
    }
    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), ruscv_sim::MemoryError> {
        self.inner.write_byte(addr, value)
    }
    fn size(&self) -> usize {
        self.inner.size()
    }
}

fn public_program() -> Vec<u32> {
    // PC-relative addresses work with both native and flat image placement.
    vec![
        elf::auipc(1, 0),
        elf::addi(1, 1, 40),
        csrrw(0, machine::MTVEC, 1),
        elf::addi(1, 0, SSIE as i32),
        csrrs(0, machine::MIE, 1),
        elf::addi(1, 0, MIE as i32),
        csrrs(0, machine::MSTATUS, 1),
        elf::addi(1, 0, SSIP as i32),
        csrrs(0, machine::MIP, 1),
        NOP,
        elf::auipc(11, 1),
        elf::addi(11, 11, -40),
        elf::addi(10, 0, 1),
        elf::slli(10, 10, 63),
        elf::sd(10, 11, 0),
    ]
}

#[test]
fn public_routes_take_the_interrupt_before_the_following_instruction() {
    let code = public_program();
    let bytes = elf::elf_with_code(&code, 0, true, false, elf::BSS_MEMORY_SIZE);
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        for observe in [false, true] {
            let machine = Machine::new(MachineConfig::new(kind));
            machine
                .install(Arc::new(LoadImage::parse(&bytes).unwrap()))
                .unwrap();
            machine.resume().unwrap();
            for index in 0..code.len() {
                let turn = machine.step(observe).unwrap();
                if index == 9 {
                    let StepOutcome::TrapEntered(entered) = &turn.hart().outcome else {
                        panic!("{kind:?} retired the instruction after mip");
                    };
                    assert_eq!(
                        entered.cause,
                        Trap::Interrupt(InterruptCause::SupervisorSoftware)
                    );
                    assert!(!turn.hart().control.instruction_attempted);
                    assert_eq!(turn.hart().control.minstret_after, 9);
                    assert!(turn.tohost().unwrap().value.as_ref().unwrap() == &0);
                    break;
                }
                assert!(turn.hart().control.retired, "{kind:?} step {index}");
            }
        }
    }

    let native = load_and_run(&bytes, Some(64), None, None, false).unwrap();
    assert_eq!(native.exit_code, 0, "{native:?}");
    assert!(native.error.is_none());
    assert_eq!(native.cycles, 15);
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&bytes).unwrap();
    let flat_result = flat.run(Some(64)).unwrap();
    assert_eq!(flat_result.exit_code, 0, "{flat_result:?}");
    assert!(flat_result.error.is_none());
    assert_eq!(flat_result.cycles, 15);
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn cli_child_guest_takes_ssip_and_reports_success() {
    let prefix = env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{prefix}as");
    let linker = format!("{prefix}ld");
    if !command_available(&assembler) || !command_available(&linker) {
        let message = "riscv64-unknown-elf-as/ld unavailable; D11 CLI guest skipped";
        if env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
            panic!("{message}; the container requested a real guest run");
        }
        eprintln!("[SKIP] {message}");
        return;
    }
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = repository.join("tests/bare-metal-riscv-test/rv64i/supervisor_interrupt.S");
    let script = repository.join("tests/bare-metal-riscv-test/linker.ld");
    let temp = tempfile::tempdir().unwrap();
    let object = temp.path().join("supervisor_interrupt.o");
    let elf = temp.path().join("supervisor_interrupt.elf");
    let assembled = Command::new(&assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64", "-o"])
        .arg(&object)
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        assembled.status.success(),
        "{}",
        String::from_utf8_lossy(&assembled.stderr)
    );
    let linked = Command::new(&linker)
        .arg(format!("-T{}", script.display()))
        .arg(&object)
        .arg("-o")
        .arg(&elf)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let bytes = std::fs::read(&elf).unwrap();
    let native = load_and_run(&bytes, Some(10_000), None, None, false).unwrap();
    assert_eq!(native.exit_code, 0, "{native:?}");
    let mut flat = RiscVSimulator::new(0x20_000);
    flat.load_elf(&bytes).unwrap();
    let flat_result = flat.run(Some(10_000)).unwrap();
    assert_eq!(flat_result.exit_code, 0, "{flat_result:?}");

    let cli = PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"));
    let child = Command::new(&cli)
        .args(["run", elf.to_str().unwrap(), "--max-cycles", "10000"])
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(stdout.contains("Exit Code:  0"), "{stdout}");
}
