//! Stage 5 U2 on the public Machine.
//!
//! Privileged specification 20250508 §3.1.9 makes MSIP and MTIP read-only in
//! `mip` and makes MSIE and MTIE writable. §3.2.1 makes a compare or `msip`
//! change visible at the next instruction boundary. MSI is taken before MTI.
//! MEIE and MEIP stay clear. Flat has no CLINT, so this file uses the native
//! machine only.

#[allow(dead_code)]
mod common;
use common::public_elf as elf;
use ruscv_sim::core::trap::{InterruptCause, Trap};
use ruscv_sim::core::StepOutcome;
use ruscv_sim::csr::{machine, mstatus};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::PrivilegeMode;
use std::sync::Arc;

const MSIP_BIT: u64 = 1 << 3;
const MTIP_BIT: u64 = 1 << 7;
const MSIE: u64 = MSIP_BIT;
const MTIE: u64 = MTIP_BIT;
const MIE_ENABLES: u64 = 0x222 | MSIE | MTIE;
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

fn assert_taken(
    machine: &Machine,
    cause: InterruptCause,
    victim: usize,
    sentinel: u64,
) -> PrivilegeMode {
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
    before_privilege
}

#[test]
fn native_msip_and_mtip_are_taken_before_fetch() {
    machine_software_is_taken_before_fetch();
    reset_masks_keep_machine_enables_and_clear_pending_bits();
    msip_follows_the_store_and_survives_a_mip_write();
    pending_software_retires_until_both_enables_are_set();
    mtip_is_visible_after_mtimecmp_and_then_taken();
    msi_precedes_mti_when_both_are_pending();
    supervisor_takes_msip_with_mie_clear();
}

fn machine_software_is_taken_before_fetch() {
    let code = [
        elf::addi(12, 0, 0x11),
        elf::lui(6, 0x02000),
        elf::addi(7, 0, -1),
        store(2, 7, 6, 0),
        elf::addi(8, 0, MSIE as i32),
        csrw(machine::MIE, 8),
        elf::addi(8, 0, mstatus::MIE as i32),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(12, 0, 0x42),
    ];
    let victim = code.len() - 1;
    let machine = machine_with(&code);
    retire_named(&machine, victim, "arm MSIP, MSIE, and MIE");
    let source = assert_taken(&machine, InterruptCause::MachineSoftware, victim, 0x11);
    assert_eq!(source, PrivilegeMode::Machine);
}

fn reset_masks_keep_machine_enables_and_clear_pending_bits() {
    let code = [
        csrr(1, machine::MIE),
        csrr(2, machine::MIP),
        elf::addi(3, 0, -1),
        csrw(machine::MIE, 3),
        csrr(4, machine::MIE),
        csrw(machine::MIP, 3),
        csrr(5, machine::MIP),
    ];
    let machine = machine_with(&code);
    retire_named(&machine, code.len(), "mask readback");
    assert_eq!(reg(&machine, 1), 0, "mie resets to 0");
    assert_eq!(reg(&machine, 2), 0, "mip resets to 0");
    assert_eq!(
        reg(&machine, 4),
        MIE_ENABLES,
        "all-ones mie keeps MSIE and MTIE"
    );
    assert_eq!(
        reg(&machine, 5),
        0x222,
        "all-ones mip keeps the supervisor bits and no machine line"
    );
    assert_eq!(reg(&machine, 5) & (MSIP_BIT | MTIP_BIT), 0);
    assert_eq!(csr_value(&machine, machine::MIE), MIE_ENABLES);
    assert_eq!(csr_value(&machine, machine::MIP), 0x222);
}

fn msip_follows_the_store_and_survives_a_mip_write() {
    let code = [
        elf::lui(6, 0x02000),
        elf::addi(7, 0, -1),
        store(2, 7, 6, 0),
        csrr(9, machine::MIP),
        load(2, 8, 6, 0),
        csrw(machine::MIP, 0),
        csrr(10, machine::MIP),
    ];
    let machine = machine_with(&code);
    retire_named(&machine, 3, "msip store");
    assert_eq!(
        csr_value(&machine, machine::MIP) & MSIP_BIT,
        MSIP_BIT,
        "host inspect after the msip store sees MSIP"
    );
    retire_named(&machine, 1, "csrr mip");
    assert_eq!(reg(&machine, 9) & MSIP_BIT, MSIP_BIT);
    retire_named(&machine, 1, "msip readback");
    assert_eq!(reg(&machine, 8), 1, "sw of -1 to msip reads back 1");
    retire_named(&machine, 1, "csrw mip, x0");
    assert_eq!(
        csr_value(&machine, machine::MIP) & MSIP_BIT,
        MSIP_BIT,
        "a mip write does not clear MSIP"
    );
    retire_named(&machine, 1, "csrr mip after the write");
    assert_eq!(reg(&machine, 10) & MSIP_BIT, MSIP_BIT);
    assert_eq!(reg(&machine, 10) & !MSIP_BIT, 0);
}

fn pending_software_retires_until_both_enables_are_set() {
    let code = [
        elf::lui(6, 0x02000),
        elf::addi(7, 0, -1),
        store(2, 7, 6, 0),
        elf::addi(8, 0, MSIE as i32),
        csrw(machine::MIE, 8),
        elf::addi(10, 0, 1),
        csrw(machine::MIE, 0),
        elf::addi(8, 0, mstatus::MIE as i32),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(11, 0, 1),
    ];
    let machine = machine_with(&code);
    retire_named(
        &machine,
        6,
        "MSIP pending and MSIE set, with MIE clear, still retires in M",
    );
    assert_eq!(reg(&machine, 10), 1);
    assert_eq!(csr_value(&machine, machine::MSTATUS) & mstatus::MIE, 0);
    assert_eq!(csr_value(&machine, machine::MIP) & MSIP_BIT, MSIP_BIT);
    retire_named(
        &machine,
        4,
        "MSIP pending and MIE set, with MSIE clear, still retires in M",
    );
    assert_eq!(reg(&machine, 11), 1);
    assert_eq!(csr_value(&machine, machine::MIE) & MSIE, 0);
    assert_ne!(csr_value(&machine, machine::MSTATUS) & mstatus::MIE, 0);
}

fn mtip_is_visible_after_mtimecmp_and_then_taken() {
    let code = [
        elf::lui(6, 0x02004),
        elf::sd(0, 6, 0),
        csrr(9, machine::MIP),
        elf::addi(12, 0, 0x11),
        elf::addi(8, 0, MTIE as i32),
        csrw(machine::MIE, 8),
        elf::addi(8, 0, mstatus::MIE as i32),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(12, 0, 0x42),
    ];
    let victim = code.len() - 1;
    let machine = machine_with(&code);
    retire_named(&machine, 2, "mtimecmp store retires before MTIE is set");
    assert_eq!(
        csr_value(&machine, machine::MIP) & MTIP_BIT,
        MTIP_BIT,
        "host inspect after writing mtimecmp to 0 sees MTIP"
    );
    assert_eq!(csr_value(&machine, machine::MIE) & MTIE, 0);
    retire_named(&machine, 1, "the following csrr mip");
    assert_eq!(reg(&machine, 9) & MTIP_BIT, MTIP_BIT);
    retire_named(&machine, victim - 3, "arm MTIE and MIE");
    let source = assert_taken(&machine, InterruptCause::MachineTimer, victim, 0x11);
    assert_eq!(source, PrivilegeMode::Machine);
    assert_eq!(csr_value(&machine, machine::MIP) & MSIP_BIT, 0);
}

fn msi_precedes_mti_when_both_are_pending() {
    let code = [
        elf::lui(6, 0x02000),
        elf::addi(7, 0, -1),
        store(2, 7, 6, 0),
        elf::lui(6, 0x02004),
        elf::sd(0, 6, 0),
        elf::addi(12, 0, 0x11),
        elf::addi(8, 0, (MSIE | MTIE) as i32),
        csrw(machine::MIE, 8),
        elf::addi(8, 0, mstatus::MIE as i32),
        csrrs(0, machine::MSTATUS, 8),
        elf::addi(12, 0, 0x42),
    ];
    let victim = code.len() - 1;
    let machine = machine_with(&code);
    retire_named(&machine, victim, "arm both lines, both enables, and MIE");
    assert_eq!(
        csr_value(&machine, machine::MIP) & (MSIP_BIT | MTIP_BIT),
        MSIP_BIT | MTIP_BIT
    );
    assert_eq!(
        csr_value(&machine, machine::MIE) & (MSIE | MTIE),
        MSIE | MTIE
    );
    let source = assert_taken(&machine, InterruptCause::MachineSoftware, victim, 0x11);
    assert_eq!(source, PrivilegeMode::Machine);
}

fn supervisor_takes_msip_with_mie_clear() {
    const AUIPC: usize = 8;
    const VICTIM: usize = 13;
    let distance = i32::try_from((VICTIM - AUIPC) * 4).unwrap();
    let code = [
        elf::addi(12, 0, 0x11),
        elf::lui(6, 0x02000),
        elf::addi(7, 0, -1),
        store(2, 7, 6, 0),
        elf::addi(8, 0, MSIE as i32),
        csrw(machine::MIE, 8),
        elf::addi(9, 0, 1),
        elf::slli(9, 9, 11),
        elf::auipc(5, 0),
        elf::addi(5, 5, distance),
        csrw(machine::MEPC, 5),
        csrw(machine::MSTATUS, 9),
        MRET,
        elf::addi(12, 0, 0x42),
    ];
    assert_eq!(code.len(), VICTIM + 1);
    assert_eq!(code[AUIPC], elf::auipc(5, 0));
    assert_eq!(code[VICTIM], elf::addi(12, 0, 0x42));
    let machine = machine_with(&code);
    retire_named(&machine, VICTIM, "mret into S with MIE clear");
    let hart = machine.inspect().unwrap().hart;
    assert_eq!(hart.privilege, PrivilegeMode::Supervisor);
    assert_eq!(csr_value(&machine, machine::MSTATUS) & mstatus::MIE, 0);
    assert_eq!(csr_value(&machine, machine::MIE) & MSIE, MSIE);
    assert_eq!(csr_value(&machine, machine::MIP) & MSIP_BIT, MSIP_BIT);
    assert_eq!(csr_value(&machine, machine::MIDELEG), 0);
    let source = assert_taken(&machine, InterruptCause::MachineSoftware, VICTIM, 0x11);
    assert_eq!(source, PrivilegeMode::Supervisor);
    assert_eq!(csr_value(&machine, machine::MIDELEG), 0);
}
