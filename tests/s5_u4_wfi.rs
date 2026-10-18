//! Stage 5 U4 driver checks on the public Machine.
//!
//! Privileged specification 20250508 §3.3.3. A single-step of a waiting hart
//! does not move `mtime` (ADR-0004 §8.3). A virtual-time deadline earlier
//! than the next `mtimecmp` stops the idle jump and retires nothing. The
//! public CLI has no deadline flag.

#[allow(dead_code)]
mod common;
use common::public_elf as elf;
use ruscv_sim::core::trap::{ExceptionCause, InterruptCause, Trap};
use ruscv_sim::core::{RiscvCore, StepOutcome};
use ruscv_sim::csr::{machine, mstatus, supervisor, unprivileged};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind, WaitSettlement};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::mmu::pte::flags;
use ruscv_sim::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use ruscv_sim::PrivilegeMode;
use std::sync::{Arc, Mutex};

const WFI: u32 = 0x1050_0073;
const MRET: u32 = 0x3020_0073;
const MTIE: i32 = 0x80;

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn csrw(csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | 0x73
}

fn add(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x33
}

fn or(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (6 << 12) | (u32::from(rd) << 7) | 0x33
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

fn csr_value(machine: &Machine, addr: u16) -> u64 {
    let mut csr = machine.inspect().unwrap().hart.csr;
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
    }
}

/// Far compare, MTIE, MIE clear. Returns the WFI index.
fn arm_future_timer() -> Vec<u32> {
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

#[test]
fn a_single_step_of_a_waiting_hart_does_not_move_mtime() {
    let code = arm_future_timer();
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    let machine = machine_with(&code);
    retire(&machine, wfi);
    let retired = machine.step(false).expect("WFI");
    assert!(
        matches!(retired.hart().outcome, StepOutcome::InstructionRetired(_)),
        "WFI did not retire: {:?}",
        retired.hart().outcome
    );
    assert_eq!(retired.hart().control.after_pc, pc_at(wfi + 1));
    drop(retired);
    let time = csr_value(&machine, unprivileged::TIME);
    let minstret = csr_value(&machine, machine::MINSTRET);
    assert_eq!(reg(&machine, 24), 0x22);
    assert_eq!(machine.inspect().unwrap().hart.pc, pc_at(wfi + 1));

    let waiting = machine.step(false).expect("wait");
    assert!(
        matches!(waiting.hart().outcome, StepOutcome::Waiting),
        "the next single-step did not wait: {:?}",
        waiting.hart().outcome
    );
    assert!(waiting.boundary().waiting);
    assert!(!waiting.hart().control.retired);
    assert!(!waiting.hart().control.instruction_attempted);
    drop(waiting);
    assert_eq!(csr_value(&machine, unprivileged::TIME), time);
    assert_eq!(csr_value(&machine, machine::MINSTRET), minstret);
    assert_eq!(reg(&machine, 24), 0x22);
    assert_eq!(machine.inspect().unwrap().hart.pc, pc_at(wfi + 1));
}

#[test]
fn a_deadline_before_mtimecmp_stops_without_a_retirement() {
    let code = arm_future_timer();
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    let machine = machine_with(&code);
    retire(&machine, wfi + 1);
    let time = csr_value(&machine, unprivileged::TIME);
    let minstret = csr_value(&machine, machine::MINSTRET);
    let deadline = time + 10;
    machine.set_virtual_deadline(Some(deadline)).unwrap();
    assert_eq!(machine.settle_wait().unwrap(), WaitSettlement::Deadline);
    assert_eq!(csr_value(&machine, unprivileged::TIME), deadline);
    assert_eq!(csr_value(&machine, machine::MINSTRET), minstret);
    assert_eq!(machine.inspect().unwrap().hart.pc, pc_at(wfi + 1));
    assert_eq!(reg(&machine, 24), 0x22);

    let waiting = machine.step(false).expect("still waiting");
    assert!(matches!(waiting.hart().outcome, StepOutcome::Waiting));
    drop(waiting);
    assert_eq!(
        csr_value(&machine, unprivileged::TIME),
        deadline,
        "a waiting single-step moved mtime off the deadline"
    );
    assert_eq!(csr_value(&machine, machine::MINSTRET), minstret);
}

#[test]
fn a_deadline_already_due_does_not_rewind_mtime() {
    let mut code = arm_future_timer();
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    // Keep the stored compare. The arming sequence reuses x5 for MTIE.
    code.insert(wfi - 3, add(19, 5, 0));
    let wfi = code.iter().position(|word| *word == WFI).unwrap();
    let machine = machine_with(&code);
    retire(&machine, wfi + 1);
    let time = csr_value(&machine, unprivileged::TIME);
    let minstret = csr_value(&machine, machine::MINSTRET);
    let compare = reg(&machine, 19);
    assert!(compare > time);

    machine
        .set_virtual_deadline(Some(time.saturating_sub(1)))
        .unwrap();
    assert_eq!(machine.settle_wait().unwrap(), WaitSettlement::Deadline);
    assert_eq!(csr_value(&machine, unprivileged::TIME), time);

    machine.set_virtual_deadline(Some(time)).unwrap();
    assert_eq!(machine.settle_wait().unwrap(), WaitSettlement::Deadline);
    assert_eq!(csr_value(&machine, unprivileged::TIME), time);
    assert_eq!(csr_value(&machine, machine::MINSTRET), minstret);

    machine.set_virtual_deadline(Some(compare)).unwrap();
    assert_eq!(machine.settle_wait().unwrap(), WaitSettlement::Jumped);
    assert_eq!(csr_value(&machine, unprivileged::TIME), compare);
    assert_eq!(csr_value(&machine, machine::MINSTRET), minstret);
}

#[test]
fn a_pending_local_interrupt_retires_wfi_then_traps_at_the_next_word() {
    let code = [
        elf::addi(24, 0, 0x22),
        elf::addi(5, 0, MTIE),
        csrw(machine::MIE, 5),
        elf::addi(5, 0, mstatus::MIE as i32),
        csrrs(0, machine::MSTATUS, 5),
        elf::lui(9, 0x02004),
        elf::sd(0, 9, 0),
        WFI,
        elf::addi(24, 0, 0x99),
    ];
    let wfi = 7;
    let machine = machine_with(&code);
    retire(&machine, wfi);
    let retired = machine.step(false).expect("WFI");
    assert!(
        matches!(retired.hart().outcome, StepOutcome::InstructionRetired(_)),
        "a pending interrupt suppressed the WFI retirement: {:?}",
        retired.hart().outcome
    );
    drop(retired);
    assert_eq!(csr_value(&machine, machine::MINSTRET), wfi as u64 + 1);
    let taken = machine.step(false).expect("timer");
    let StepOutcome::TrapEntered(entered) = taken.hart().outcome.clone() else {
        panic!(
            "the following boundary did not trap: {:?}",
            taken.hart().outcome
        );
    };
    assert!(!taken.hart().control.instruction_attempted);
    assert!(!taken.hart().control.retired);
    assert_eq!(entered.cause, Trap::Interrupt(InterruptCause::MachineTimer));
    assert_eq!(entered.faulting_pc, pc_at(wfi + 1));
    drop(taken);
    assert_eq!(csr_value(&machine, machine::MEPC), pc_at(wfi + 1));
    assert_eq!(csr_value(&machine, machine::MTVAL), 0);
    assert_eq!(reg(&machine, 24), 0x22, "the victim retired");
}

#[test]
fn an_s_mode_wfi_with_tw_set_traps_and_does_not_wait() {
    // auipc at index 6, plus 16 bytes, is the WFI.
    let code = [
        elf::addi(5, 0, 1),
        elf::slli(5, 5, 11),
        elf::addi(6, 0, 1),
        elf::slli(6, 6, 21),
        or(5, 5, 6),
        csrw(machine::MSTATUS, 5),
        elf::auipc(5, 0),
        elf::addi(5, 5, 16),
        csrw(machine::MEPC, 5),
        MRET,
        WFI,
    ];
    let machine = machine_with(&code);
    retire(&machine, 10);
    assert_eq!(
        machine.inspect().unwrap().hart.privilege,
        PrivilegeMode::Supervisor
    );
    let turn = machine.step(false).expect("illegal WFI");
    let StepOutcome::TrapEntered(entered) = turn.hart().outcome.clone() else {
        panic!("TW did not trap WFI: {:?}", turn.hart().outcome);
    };
    assert_eq!(
        entered.cause,
        Trap::Exception(ExceptionCause::IllegalInstruction)
    );
    assert_eq!(entered.faulting_pc, pc_at(10));
    drop(turn);
    assert_eq!(csr_value(&machine, machine::MEPC), pc_at(10));
    assert_eq!(
        machine.inspect().unwrap().hart.privilege,
        PrivilegeMode::Machine
    );
}

const SV_PAGE: u64 = 0x1000;
const SV_RAM: usize = 0x10_0000;
const SV_BASE: u64 = 0x8000_0000;
const SV_VA: u64 = 0x1000;
const SV_PA: u64 = 0x8000_5000;
const SV_ROOT: u64 = 0x80000;
const SSIP: u64 = 1 << 1;
const PTE_X: u64 = 0x08;
const PTE_R: u64 = 0x02;

fn sv_write(ram: &Arc<Mutex<SimpleMemory>>, guest: u64, value: u64) {
    ram.lock()
        .unwrap()
        .write_dword(guest - SV_BASE, value)
        .unwrap();
}

fn sv_read(ram: &Arc<Mutex<SimpleMemory>>, guest: u64) -> u64 {
    ram.lock().unwrap().read_dword(guest - SV_BASE).unwrap()
}

fn install_sv39_leaf(ram: &Arc<Mutex<SimpleMemory>>, perm: u64) -> u64 {
    let vpn0 = (SV_VA >> 12) & 0x1ff;
    sv_write(ram, SV_ROOT * SV_PAGE, ((SV_ROOT + 1) << 10) | 1);
    sv_write(ram, (SV_ROOT + 1) * SV_PAGE, ((SV_ROOT + 2) << 10) | 1);
    let pte_addr = (SV_ROOT + 2) * SV_PAGE + vpn0 * 8;
    sv_write(ram, pte_addr, ((SV_PA / SV_PAGE) << 10) | perm | 1);
    pte_addr
}

/// Supervisor, Sv39, SSIP and SSIE pending. The leaf has no A bit.
fn sv39_supervisor(word: u32, perm: u64) -> (RiscvCore, Arc<Mutex<SimpleMemory>>, u64) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(SV_RAM)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, SV_RAM),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, SV_RAM),
    )));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.reset(0, SV_BASE);
    let pte_addr = install_sv39_leaf(&ram, perm);
    ram.lock()
        .unwrap()
        .write_word(SV_PA - SV_BASE, word)
        .unwrap();
    core.state_mut()
        .csr
        .write(supervisor::SATP, (8u64 << 60) | SV_ROOT)
        .unwrap();
    core.state_mut().csr.write(machine::MIP, SSIP).unwrap();
    core.state_mut().csr.write(machine::MIE, SSIP).unwrap();
    core.state_mut().csr.write(machine::MTVEC, 0x100).unwrap();
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.state_mut().pc = SV_VA;
    assert_eq!(sv_read(&ram, pte_addr) & flags::A, 0);
    (core, ram, pte_addr)
}

fn core_csr(core: &RiscvCore, addr: u16) -> u64 {
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(addr).unwrap()
}

#[test]
fn an_sv39_supervisor_wfi_retires_before_the_pending_interrupt() {
    let (mut core, ram, pte_addr) = sv39_supervisor(WFI, PTE_X);
    let retired = core.step_transition(false);
    let StepOutcome::InstructionRetired(done) = retired.outcome else {
        panic!("Sv39 WFI did not retire: {:?}", retired.outcome);
    };
    assert_eq!(done.instruction, WFI);
    assert_eq!(done.next_pc, SV_VA + 4);
    assert_eq!(core.state().pc, SV_VA + 4);
    assert_ne!(sv_read(&ram, pte_addr) & flags::A, 0);

    let taken = core.step_transition(false);
    let StepOutcome::TrapEntered(entered) = taken.outcome else {
        panic!("the following boundary did not trap: {:?}", taken.outcome);
    };
    assert!(!taken.control.retired);
    assert!(!taken.control.instruction_attempted);
    assert_eq!(
        entered.cause,
        Trap::Interrupt(InterruptCause::SupervisorSoftware)
    );
    assert_eq!(entered.faulting_pc, SV_VA + 4);
    assert_eq!(core_csr(&core, machine::MEPC), SV_VA + 4);
    assert_eq!(core_csr(&core, machine::MTVAL), 0);
}

#[test]
fn an_sv39_non_wfi_interrupt_does_not_set_accessed() {
    let (mut core, ram, pte_addr) = sv39_supervisor(0x0000_0013, PTE_X);
    let taken = core.step_transition(false);
    let StepOutcome::TrapEntered(entered) = taken.outcome else {
        panic!("expected the interrupt, got {:?}", taken.outcome);
    };
    assert!(!taken.control.retired);
    assert_eq!(
        entered.cause,
        Trap::Interrupt(InterruptCause::SupervisorSoftware)
    );
    assert_eq!(entered.faulting_pc, SV_VA);
    assert_eq!(core_csr(&core, machine::MEPC), SV_VA);
    assert_eq!(sv_read(&ram, pte_addr) & flags::A, 0);
}

#[test]
fn an_sv39_faulting_fetch_takes_the_interrupt_and_leaves_the_pte() {
    let (mut core, ram, pte_addr) = sv39_supervisor(WFI, PTE_R);
    let before = sv_read(&ram, pte_addr);
    let taken = core.step_transition(false);
    let StepOutcome::TrapEntered(entered) = taken.outcome else {
        panic!("expected the interrupt, got {:?}", taken.outcome);
    };
    assert_eq!(
        entered.cause,
        Trap::Interrupt(InterruptCause::SupervisorSoftware)
    );
    assert_eq!(entered.faulting_pc, SV_VA);
    assert_eq!(sv_read(&ram, pte_addr), before);
}
