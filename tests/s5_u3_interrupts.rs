//! Stage 5 U3 on the public Machine.
//!
//! Privileged specification 20250508 §3.1.9 makes MEIP read-only in `mip`
//! and makes MEIE writable. The machine PLIC context presents MEIP. PLIC
//! specification 1.0.0: a 4-byte claim read returns the source ID and clears
//! pending, and a 4-byte complete ends service. A level that stays high
//! makes source 1 pending again. Trap entry does not claim. Flat has no
//! PLIC, so this file uses the native machine only.

#[allow(dead_code)]
mod common;
use common::public_elf as elf;
use ruscv_sim::core::trap::{ExceptionCause, InterruptCause, Trap};
use ruscv_sim::core::StepOutcome;
use ruscv_sim::csr::{machine, mstatus};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::PrivilegeMode;
use std::sync::Arc;

const MEIP_BIT: u64 = 1 << 11;
const SEIP_BIT: u64 = 1 << 9;
const MIE_BIT: i32 = mstatus::MIE as i32;
const MRET: u32 = 0x3020_0073;

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn csrr(rd: u8, csr: u16) -> u32 {
    csrrs(rd, csr, 0)
}

fn csrw(csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | 0x73
}

fn store(funct3: u32, rs2: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32;
    ((immediate >> 5) & 0x7f) << 25
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn load(funct3: u32, rd: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0xfff;
    (immediate << 20) | (u32::from(rs1) << 15) | (funct3 << 12) | (u32::from(rd) << 7) | 0x03
}

fn sw(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    store(2, rs2, rs1, immediate)
}

fn lw(rd: u8, rs1: u8, immediate: i32) -> u32 {
    load(2, rd, rs1, immediate)
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

fn retire_named(machine: &Machine, count: usize, why: &str) {
    for index in 0..count {
        let turn = machine.step(false).expect("step");
        assert!(
            turn.hart().control.retired,
            "{why}: instruction {index} did not retire: {:?}",
            turn.hart().outcome
        );
    }
}

fn meie(rd: u8) -> [u32; 2] {
    [elf::addi(rd, 0, 1), elf::slli(rd, rd, 11)]
}

/// Priority, context-0 enable, threshold 0, then gateway level 1.
/// `x6` is left holding the context-0 claim base `0x0C20_0000`.
fn arm_machine_source(priority: i32) -> Vec<u32> {
    let mut code = vec![
        elf::lui(6, 0x0C000),
        elf::addi(7, 0, priority),
        sw(7, 6, 4),
        elf::lui(6, 0x0C002),
        elf::addi(7, 0, 2),
        sw(7, 6, 0),
        elf::lui(6, 0x0C200),
        sw(0, 6, 0),
        elf::lui(8, 0x0C400),
        elf::addi(7, 0, 1),
        sw(7, 8, 0),
    ];
    code.extend(meie(8));
    code
}

#[test]
fn mei_is_taken_before_fetch_and_claim_still_returns_one() {
    const AUIPC: usize = 14;
    const VICTIM: usize = 20;
    const CLAIM: usize = 21;
    let distance = i32::try_from((CLAIM - AUIPC) * 4).unwrap();
    let mut code = vec![elf::addi(12, 0, 0x11)];
    code.extend(arm_machine_source(1));
    code.extend([
        elf::auipc(5, 0),
        elf::addi(5, 5, distance),
        csrw(machine::MTVEC, 5),
        csrw(machine::MIE, 8),
        elf::addi(8, 0, MIE_BIT),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(12, 0, 0x42),
        lw(10, 6, 4),
        csrr(11, machine::MIP),
        elf::lui(9, 0x0C001),
        lw(13, 9, 0),
    ]);
    assert_eq!(code[AUIPC], elf::auipc(5, 0));
    assert_eq!(code[VICTIM], elf::addi(12, 0, 0x42));
    assert_eq!(code[CLAIM], lw(10, 6, 4));
    let machine = machine_with(&code);
    retire_named(&machine, VICTIM, "arm source 1, MEIE, and MIE");
    assert_eq!(csr_value(&machine, machine::MIP) & MEIP_BIT, MEIP_BIT);
    let source = assert_taken(&machine, InterruptCause::MachineExternal, VICTIM, 0x11);
    assert_eq!(source, PrivilegeMode::Machine);
    retire_named(&machine, 4, "claim, then read mip and pending");
    assert_eq!(
        reg(&machine, 10),
        1,
        "claim after the trap returns source 1"
    );
    assert_eq!(
        reg(&machine, 11) & MEIP_BIT,
        0,
        "MEIP clears while in service"
    );
    assert_eq!(reg(&machine, 13) & 2, 0, "claim clears the pending bit");
}

fn assert_taken(
    machine: &Machine,
    cause: InterruptCause,
    victim: usize,
    sentinel: u64,
) -> PrivilegeMode {
    let before_status = csr_value(machine, machine::MSTATUS);
    let turn = machine.step(false).expect("step");
    let attempted = turn.hart().control.instruction_attempted;
    let retired = turn.hart().control.retired;
    let before_pc = turn.hart().control.before_pc;
    let before_privilege = turn.hart().control.before_privilege;
    let StepOutcome::TrapEntered(entered) = turn.hart().outcome.clone() else {
        panic!(
            "{cause:?} was not taken before fetch at {}: {:?}",
            pc_at(victim),
            turn.hart().outcome
        );
    };
    drop(turn);
    assert_eq!(entered.cause, Trap::Interrupt(cause));
    assert!(!attempted, "the interrupted instruction was fetched");
    assert!(!retired, "the interrupted instruction retired");
    assert_eq!(entered.faulting_pc, pc_at(victim));
    assert_eq!(before_pc, pc_at(victim));
    assert_eq!(entered.mtval, 0);
    assert_eq!(csr_value(machine, machine::MEPC), pc_at(victim));
    assert_eq!(csr_value(machine, machine::MTVAL), 0);
    assert_eq!(csr_value(machine, machine::MCAUSE), cause.code());
    assert_eq!(reg(machine, 12), sentinel);
    assert_eq!(
        machine.inspect().unwrap().hart.privilege,
        PrivilegeMode::Machine
    );
    let after_status = csr_value(machine, machine::MSTATUS);
    assert_eq!(after_status & mstatus::MIE, 0, "trap entry clears MIE");
    assert_eq!(
        (after_status & mstatus::MPIE) >> 7,
        (before_status & mstatus::MIE) >> 3,
        "MPIE receives the MIE from before the trap"
    );
    assert_eq!(
        (after_status & mstatus::MPP) >> mstatus::MPP_SHIFT,
        before_privilege as u64,
        "MPP records the interrupted privilege"
    );
    before_privilege
}

#[test]
fn claim_clears_pending_and_complete_repends_while_level_stays_high() {
    let mut code = arm_machine_source(1);
    code.extend([
        elf::lui(9, 0x0C001),
        lw(10, 9, 0),
        csrr(11, machine::MIP),
        lw(12, 6, 4),
        lw(13, 9, 0),
        csrr(14, machine::MIP),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        lw(15, 9, 0),
        csrr(16, machine::MIP),
    ]);
    let machine = machine_with(&code);
    retire_named(
        &machine,
        code.len(),
        "claim then complete with the level high",
    );
    assert_eq!(reg(&machine, 10) & 2, 2, "gateway sets pending");
    assert_eq!(reg(&machine, 11) & MEIP_BIT, MEIP_BIT);
    assert_eq!(reg(&machine, 12), 1, "claim returns 1");
    assert_eq!(reg(&machine, 13) & 2, 0, "claim clears pending");
    assert_eq!(reg(&machine, 14) & MEIP_BIT, 0);
    assert_eq!(
        reg(&machine, 15) & 2,
        2,
        "complete with level 1 sets pending again"
    );
    assert_eq!(reg(&machine, 16) & MEIP_BIT, MEIP_BIT);
}

#[test]
fn clearing_the_level_and_completing_leaves_meip_clear() {
    let mut code = arm_machine_source(1);
    code.extend([
        lw(10, 6, 4),
        elf::lui(8, 0x0C400),
        sw(0, 8, 0),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        elf::lui(9, 0x0C001),
        lw(11, 9, 0),
        csrr(12, machine::MIP),
    ]);
    let machine = machine_with(&code);
    retire_named(&machine, code.len(), "deassert, then complete");
    assert_eq!(reg(&machine, 10), 1);
    assert_eq!(reg(&machine, 11) & 2, 0);
    assert_eq!(reg(&machine, 12) & MEIP_BIT, 0);
}

#[test]
fn mei_precedes_msi_and_mti() {
    let mut code = vec![
        elf::addi(12, 0, 0x11),
        elf::lui(5, 0x02000),
        elf::addi(7, 0, 1),
        sw(7, 5, 0),
        elf::lui(5, 0x02004),
        elf::sd(0, 5, 0),
    ];
    code.extend(arm_machine_source(1));
    code.extend([
        elf::ori(8, 8, 0x88),
        csrw(machine::MIE, 8),
        elf::addi(8, 0, MIE_BIT),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(12, 0, 0x42),
    ]);
    let victim = code.len() - 1;
    let machine = machine_with(&code);
    retire_named(&machine, victim, "arm MEI, MSI, and MTI");
    assert_eq!(csr_value(&machine, machine::MIP) & MEIP_BIT, MEIP_BIT);
    assert_eq!(csr_value(&machine, machine::MIP) & (1 << 3), 1 << 3);
    assert_eq!(csr_value(&machine, machine::MIP) & (1 << 7), 1 << 7);
    assert_taken(&machine, InterruptCause::MachineExternal, victim, 0x11);
}

#[test]
fn priority_zero_and_a_blocking_threshold_do_not_present_meip() {
    let mut blocked = arm_machine_source(0);
    blocked.extend([
        csrw(machine::MIE, 8),
        elf::addi(8, 0, MIE_BIT),
        csrrs(0, machine::MSTATUS, 8),
        elf::lui(9, 0x0C001),
        lw(10, 9, 0),
        csrr(11, machine::MIP),
        elf::addi(12, 0, 0x42),
    ]);
    let machine = machine_with(&blocked);
    retire_named(
        &machine,
        blocked.len(),
        "priority 0 stays pending and retires",
    );
    assert_eq!(reg(&machine, 10) & 2, 2);
    assert_eq!(reg(&machine, 11) & MEIP_BIT, 0);
    assert_eq!(reg(&machine, 12), 0x42);

    let mut code = vec![
        elf::lui(6, 0x0C000),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        elf::lui(6, 0x0C002),
        elf::addi(7, 0, 2),
        sw(7, 6, 0),
        elf::lui(6, 0x0C200),
        elf::addi(7, 0, 1),
        sw(7, 6, 0),
        elf::lui(8, 0x0C400),
        elf::addi(7, 0, 1),
        sw(7, 8, 0),
    ];
    code.extend(meie(8));
    code.extend([
        csrw(machine::MIE, 8),
        elf::addi(8, 0, MIE_BIT),
        csrrs(0, machine::MSTATUS, 8),
        csrr(11, machine::MIP),
        elf::addi(12, 0, 0x42),
    ]);
    let machine = machine_with(&code);
    retire_named(
        &machine,
        code.len(),
        "threshold equal to the priority blocks MEIP",
    );
    assert_eq!(reg(&machine, 11) & MEIP_BIT, 0);
    assert_eq!(reg(&machine, 12), 0x42);
}

#[test]
fn supervisor_line_ors_into_seip_without_sticking_the_software_bit() {
    let code = [
        elf::lui(6, 0x0C000),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        elf::lui(6, 0x0C002),
        elf::addi(7, 0, 2),
        sw(7, 6, 0x80),
        elf::lui(8, 0x0C400),
        elf::addi(7, 0, 1),
        sw(7, 8, 0),
        csrr(10, machine::MIP),
        elf::addi(7, 0, 2),
        csrrs(0, machine::MIP, 7),
        elf::lui(6, 0x0C201),
        lw(0, 6, 4),
        csrr(11, machine::MIP),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        csrw(machine::MIP, 0),
        csrr(12, machine::MIP),
        lw(0, 6, 4),
        csrr(13, machine::MIP),
    ];
    let machine = machine_with(&code);
    retire_named(&machine, 10, "present the supervisor line only");
    assert_eq!(reg(&machine, 10) & SEIP_BIT, SEIP_BIT);
    assert_eq!(reg(&machine, 10) & MEIP_BIT, 0, "context 0 is disabled");
    assert_eq!(csr_value(&machine, machine::MIP) & SEIP_BIT, SEIP_BIT);
    retire_named(&machine, 5, "CSRRS of SSIP, then claim context 1");
    assert_eq!(reg(&machine, 11) & 2, 2, "CSRRS sets the software SSIP bit");
    assert_eq!(
        reg(&machine, 11) & SEIP_BIT,
        0,
        "CSRRS does not copy the external line into the software SEIP bit"
    );
    retire_named(&machine, 6, "complete, CSRW 0, then claim again");
    assert_eq!(
        reg(&machine, 12) & SEIP_BIT,
        SEIP_BIT,
        "a mip write of 0 leaves the external OR in place"
    );
    assert_eq!(reg(&machine, 13) & SEIP_BIT, 0);
    assert_eq!(
        reg(&machine, 13) & 2,
        0,
        "CSRW of 0 clears the software SSIP bit"
    );
}

#[test]
fn supervisor_takes_meip_with_mie_clear() {
    const AUIPC: usize = 17;
    const VICTIM: usize = 22;
    let distance = i32::try_from((VICTIM - AUIPC) * 4).unwrap();
    let mut code = vec![elf::addi(12, 0, 0x11)];
    code.extend(arm_machine_source(1));
    code.extend([
        csrw(machine::MIE, 8),
        elf::addi(9, 0, 1),
        elf::slli(9, 9, 11),
        elf::auipc(5, 0),
        elf::addi(5, 5, distance),
        csrw(machine::MEPC, 5),
        csrw(machine::MSTATUS, 9),
        MRET,
        elf::addi(12, 0, 0x42),
    ]);
    assert_eq!(code.len(), VICTIM + 1);
    assert_eq!(code[AUIPC], elf::auipc(5, 0));
    let machine = machine_with(&code);
    retire_named(&machine, VICTIM, "mret into S with MEIP pending");
    assert_eq!(
        machine.inspect().unwrap().hart.privilege,
        PrivilegeMode::Supervisor
    );
    assert_eq!(csr_value(&machine, machine::MSTATUS) & mstatus::MIE, 0);
    assert_eq!(csr_value(&machine, machine::MIP) & MEIP_BIT, MEIP_BIT);
    let source = assert_taken(&machine, InterruptCause::MachineExternal, VICTIM, 0x11);
    assert_eq!(source, PrivilegeMode::Supervisor);
}

#[test]
fn threshold_blocks_meip_but_claim_still_returns_one() {
    let mut code = vec![
        elf::lui(6, 0x0C000),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        elf::lui(6, 0x0C002),
        elf::addi(7, 0, 2),
        sw(7, 6, 0),
        elf::lui(6, 0x0C200),
        elf::addi(7, 0, 1),
        sw(7, 6, 0),
        elf::lui(8, 0x0C400),
        elf::addi(7, 0, 1),
        sw(7, 8, 0),
    ];
    code.extend(meie(9));
    code.extend([
        csrw(machine::MIE, 9),
        elf::addi(9, 0, MIE_BIT),
        csrrs(0, machine::MSTATUS, 9),
        csrr(10, machine::MIP),
        lw(11, 6, 4),
        elf::lui(9, 0x0C001),
        lw(12, 9, 0),
        csrr(13, machine::MIP),
    ]);
    let machine = machine_with(&code);
    retire_named(
        &machine,
        code.len(),
        "threshold masks MEIP and the claim still returns source 1",
    );
    assert_eq!(reg(&machine, 10) & MEIP_BIT, 0);
    assert_eq!(reg(&machine, 11), 1);
    assert_eq!(reg(&machine, 12) & 2, 0);
    assert_eq!(reg(&machine, 13) & MEIP_BIT, 0);
}

#[test]
fn a_falling_level_does_not_retract_a_forwarded_request() {
    let mut code = arm_machine_source(1);
    code.extend([
        elf::lui(8, 0x0C400),
        sw(0, 8, 0),
        elf::lui(9, 0x0C001),
        lw(10, 9, 0),
        csrr(11, machine::MIP),
        lw(12, 6, 4),
        lw(13, 9, 0),
        csrr(14, machine::MIP),
        elf::addi(7, 0, 1),
        sw(7, 6, 4),
        lw(15, 9, 0),
    ]);
    let machine = machine_with(&code);
    retire_named(&machine, code.len(), "drop the level before claim");
    assert_eq!(
        reg(&machine, 10) & 2,
        2,
        "pending stays after the level falls"
    );
    assert_eq!(reg(&machine, 11) & MEIP_BIT, MEIP_BIT);
    assert_eq!(reg(&machine, 12), 1);
    assert_eq!(reg(&machine, 13) & 2, 0);
    assert_eq!(reg(&machine, 14) & MEIP_BIT, 0);
    assert_eq!(
        reg(&machine, 15) & 2,
        0,
        "complete with the level low does not forward again"
    );
}

#[test]
fn a_byte_load_of_the_claim_register_does_not_claim() {
    let mut code = arm_machine_source(1);
    code.push(load(0, 10, 6, 4));
    let machine = machine_with(&code);
    retire_named(&machine, code.len() - 1, "arm source 1 without taking it");
    assert_eq!(csr_value(&machine, machine::MIP) & MEIP_BIT, MEIP_BIT);
    let turn = machine.step(false).expect("step");
    let StepOutcome::TrapEntered(entered) = turn.hart().outcome.clone() else {
        panic!("byte claim load retired: {:?}", turn.hart().outcome);
    };
    assert_eq!(
        entered.cause,
        Trap::Exception(ExceptionCause::LoadAccessFault)
    );
    drop(turn);
    assert_eq!(
        csr_value(&machine, machine::MIP) & MEIP_BIT,
        MEIP_BIT,
        "a rejected byte read does not claim"
    );
}
