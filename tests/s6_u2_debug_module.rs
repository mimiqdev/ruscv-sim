//! Stage 6 U2: Debug Module behind a DMI-level API.
//!
//! Debug Specification 1.0, tag `5695c0a`, chapter Debug Module. Each test
//! names one contract sentence from `docs/dev-plan.md` §7 item 2 or one row
//! of the U2 test table. The flat and native run loops both step through
//! `Machine`, so they cannot host this module without U3. These tests drive
//! a bare [`RiscvCore`] and [`RiscvCore::step_outcome`], which is the hart
//! turn both loops call.

use ruscv_sim::core::{DebugCause, PrivilegeMode, RiscvCore, StepOutcome};
use ruscv_sim::csr::{self, machine, mstatus, supervisor};
use ruscv_sim::executor::{
    SystemBus, SYSTEM_BUS_PLIC_BASE, SYSTEM_BUS_PLIC_GATEWAY, SYSTEM_BUS_UART_BASE,
};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{NativeSystemBusBackend, ValidatedPhysicalAccess};
use ruscv_sim::DebugModule;
use std::sync::{Arc, Mutex};

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;

/// Debug Spec 1.0 §3.14 register addresses. `dtmcs.abits` is 7 (U4).
const DATA0: u8 = 0x04;
const DMCONTROL: u8 = 0x10;
const DMSTATUS: u8 = 0x11;
const HARTINFO: u8 = 0x12;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const ABSTRACTAUTO: u8 = 0x18;
const PROGBUF0: u8 = 0x20;
const SBCS: u8 = 0x38;
const HALTSUM0: u8 = 0x40;

const HALTREQ: u32 = 1 << 31;
const RESUMEREQ: u32 = 1 << 30;
const ACKHAVERESET: u32 = 1 << 28;
const NDMRESET: u32 = 1 << 1;
const DMACTIVE: u32 = 1;

const PLIC_PRIORITY_SRC1: u64 = SYSTEM_BUS_PLIC_BASE + 0x4;
const PLIC_ENABLE0: u64 = SYSTEM_BUS_PLIC_BASE + 0x2000;
const PLIC_CLAIM0: u64 = SYSTEM_BUS_PLIC_BASE + 0x20_0004;

fn typed_handle<M: MemoryInterface + Send + Sync + 'static>(
    memory: Arc<Mutex<M>>,
) -> Arc<Mutex<dyn MemoryInterface + Send + Sync>> {
    memory
}

fn harness() -> (DebugModule<RiscvCore>, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        RAM_BASE,
        RAM_SIZE,
    )));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let core = RiscvCore::new_with_physical_ports(
        typed_handle(bus.clone()),
        typed_handle(bus),
        instruction_port,
        data_port,
    );
    (DebugModule::new(core), ram)
}

fn ready() -> (DebugModule<RiscvCore>, Arc<Mutex<SimpleMemory>>) {
    let (mut dm, ram) = harness();
    // `addi x0, x0, 0`. A step that is not halted retires instead of trapping.
    ram.lock().unwrap().write_word(0, 0x0000_0013).unwrap();
    dm.hart_mut().state_mut().pc = RAM_BASE;
    (dm, ram)
}

fn hartsel_field(hartsel: u32) -> u32 {
    let lo = (hartsel & 0x3ff) << 16;
    let hi = ((hartsel >> 10) & 0x3ff) << 6;
    lo | hi
}

fn hartsel_of(dmcontrol: u32) -> u32 {
    let lo = (dmcontrol >> 16) & 0x3ff;
    let hi = (dmcontrol >> 6) & 0x3ff;
    lo | (hi << 10)
}

fn bit(word: u32, index: u32) -> bool {
    word & (1 << index) != 0
}

fn dmstatus(dm: &mut DebugModule<RiscvCore>) -> u32 {
    dm.dmi_read(DMSTATUS)
}

fn cmderr(dm: &mut DebugModule<RiscvCore>) -> u32 {
    (dm.dmi_read(ABSTRACTCS) >> 8) & 0b111
}

fn clear_cmderr(dm: &mut DebugModule<RiscvCore>) {
    // Debug Spec 1.0 §1.3.3 and §3.14: `cmderr` is bitwise write-1-to-clear.
    dm.dmi_write(ABSTRACTCS, 0b111 << 8);
    assert_eq!(cmderr(dm), 0);
}

fn activate(dm: &mut DebugModule<RiscvCore>) {
    dm.dmi_write(DMCONTROL, DMACTIVE);
    assert_eq!(dmstatus(dm) & 0xf, 3, "dmstatus.version");
}

fn enter_halt(dm: &mut DebugModule<RiscvCore>) {
    activate(dm);
    dm.dmi_write(DMCONTROL, DMACTIVE | HALTREQ);
    assert!(
        !dm.hart().in_debug_mode(),
        "a haltreq write must not enter Debug Mode by itself"
    );
    let outcome = dm.hart_mut().step_outcome();
    assert!(matches!(
        outcome,
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            completed_turn: false,
        }
    ));
    assert!(dm.hart().in_debug_mode());
}

fn set_word(dm: &mut DebugModule<RiscvCore>, index: u8, value: u32) {
    dm.dmi_write(DATA0 + index, value);
}

fn word(dm: &mut DebugModule<RiscvCore>, index: u8) -> u32 {
    dm.dmi_read(DATA0 + index)
}

fn set_arg0(dm: &mut DebugModule<RiscvCore>, value: u64) {
    set_word(dm, 0, value as u32);
    set_word(dm, 1, (value >> 32) as u32);
}

fn arg0(dm: &mut DebugModule<RiscvCore>) -> u64 {
    u64::from(word(dm, 0)) | (u64::from(word(dm, 1)) << 32)
}

fn set_arg1(dm: &mut DebugModule<RiscvCore>, value: u64) {
    set_word(dm, 2, value as u32);
    set_word(dm, 3, (value >> 32) as u32);
}

fn arg1(dm: &mut DebugModule<RiscvCore>) -> u64 {
    u64::from(word(dm, 2)) | (u64::from(word(dm, 3)) << 32)
}

/// Access Register, `cmdtype` 0. Debug Spec 1.0 §3.7.1.1.
fn access_register(
    aarsize: u32,
    postincrement: bool,
    postexec: bool,
    transfer: bool,
    write: bool,
    regno: u32,
) -> u32 {
    (aarsize << 20)
        | (u32::from(postincrement) << 19)
        | (u32::from(postexec) << 18)
        | (u32::from(transfer) << 17)
        | (u32::from(write) << 16)
        | (regno & 0xffff)
}

/// Access Memory, `cmdtype` 2. Debug Spec 1.0 §3.7.1.3.
fn access_memory(aamvirtual: bool, aamsize: u32, postincrement: bool, write: bool) -> u32 {
    (2 << 24)
        | (u32::from(aamvirtual) << 23)
        | (aamsize << 20)
        | (u32::from(postincrement) << 19)
        | (u32::from(write) << 16)
}

fn issue(dm: &mut DebugModule<RiscvCore>, command: u32) {
    dm.dmi_write(COMMAND, command);
}

fn mem_write(dm: &mut DebugModule<RiscvCore>, addr: u64, aamsize: u32, value: u64) {
    set_arg0(dm, value);
    set_arg1(dm, addr);
    issue(dm, access_memory(false, aamsize, false, true));
    assert_eq!(cmderr(dm), 0, "store to {addr:#x} aamsize {aamsize}");
}

fn mem_read(dm: &mut DebugModule<RiscvCore>, addr: u64, aamsize: u32) -> u64 {
    set_arg0(dm, 0);
    set_arg1(dm, addr);
    issue(dm, access_memory(false, aamsize, false, false));
    assert_eq!(cmderr(dm), 0, "load from {addr:#x} aamsize {aamsize}");
    arg0(dm)
}

fn ram_word(ram: &Mutex<SimpleMemory>, offset: u64) -> u32 {
    ram.lock().unwrap().read_word(offset).unwrap()
}

/// §7 item 2: U2 runs the §3.13 version discovery.
#[test]
fn version_discovery_reads_dmstatus_version_3() {
    let (mut dm, _) = harness();
    assert_eq!(dm.dmi_read(DMCONTROL), 0);
    assert_eq!(dm.dmi_read(DMSTATUS), 0, "dmactive 0 reads as 0");

    // §3.13: preserve hartsel, set dmactive, clear every other bit.
    let hartsel = 0x15;
    dm.dmi_write(DMCONTROL, DMACTIVE | hartsel_field(hartsel));
    let control = dm.dmi_read(DMCONTROL);
    assert_eq!(control & 1, 1, "dmactive");
    assert_eq!(hartsel_of(control), hartsel);
    assert_eq!(
        control & !(hartsel_field(0x3ff) | DMACTIVE),
        0,
        "discovery clears haltreq, resumereq, and ndmreset"
    );

    let status = dmstatus(&mut dm);
    assert_eq!(status & 0xf, 3, "version");
    assert!(bit(status, 7), "authenticated");
    assert!(!bit(status, 22), "impebreak");
}

/// §7 item 2: `aarsize` 4 returns `cmderr` 2. Also aarsize 0, 1, and 5–7,
/// `aarpostincrement`, and the same command while the hart is running.
/// Unsupported is checked before not-halted (Debug Spec 1.0 §3.7.1.1).
#[test]
fn aarsize_4_is_cmderr_2() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    let before = dm.hart().state().regs[1];
    issue(
        &mut dm,
        access_register(4, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().regs[1], before);

    clear_cmderr(&mut dm);
    for size in [0, 1, 5, 6, 7] {
        issue(
            &mut dm,
            access_register(size, false, false, true, true, 0x1001),
        );
        assert_eq!(cmderr(&mut dm), 2, "aarsize {size}");
        clear_cmderr(&mut dm);
    }
}

/// §7 item 2: `postexec` returns `cmderr` 2. `progbufsize` is 0, so 0 is the
/// only supported value (Debug Spec 1.0 §3.7.1.1).
#[test]
fn postexec_is_cmderr_2() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    let pc = dm.hart().state().pc;
    issue(
        &mut dm,
        access_register(3, false, true, false, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().pc, pc);
    assert!(dm.hart().in_debug_mode());
}

/// `aarpostincrement` is optional and not implemented. Debug Spec 1.0 §3.7.1.1.
#[test]
fn aarpostincrement_is_cmderr_2() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    dm.hart_mut().state_mut().regs[1] = 0x11;
    issue(
        &mut dm,
        access_register(3, true, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().regs[1], 0x11);
    assert_eq!(arg0(&mut dm), 0, "failed command does not write data");
}

/// §7 item 2: an FPR regno returns `cmderr` 3 because the register does not
/// exist (Debug Spec 1.0 §3.7.1.1). `transfer` 0 does not check regno.
#[test]
fn fpr_regno_is_cmderr_3() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    for regno in [0x1020, 0x103f] {
        issue(
            &mut dm,
            access_register(3, false, false, true, false, regno),
        );
        assert_eq!(cmderr(&mut dm), 3, "regno {regno:#x}");
        clear_cmderr(&mut dm);
    }
    issue(
        &mut dm,
        access_register(3, false, false, false, false, 0x1020),
    );
    assert_eq!(cmderr(&mut dm), 0, "transfer 0 skips the regno check");
}

/// §7 item 2: `aamvirtual` 1 returns `cmderr` 2 (Debug Spec 1.0 §3.7.1.3).
#[test]
fn aamvirtual_is_cmderr_2() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    ram.lock().unwrap().write_word(0x1000, 0x89ab_cdef).unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MSTATUS, mstatus::MPRV)
        .unwrap();
    set_arg0(&mut dm, 0x1111_1111);
    set_arg1(&mut dm, RAM_BASE + 0x1000);
    issue(&mut dm, access_memory(true, 2, false, true));
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(ram_word(&ram, 0x1000), 0x89ab_cdef);
}

/// A physical read with `MPRV` and Sv39 still uses the physical address.
/// Debug Spec 1.0 §3.7.1.3. Translating would miss the planted word.
#[test]
fn physical_read_ignores_mprv() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    ram.lock().unwrap().write_word(0x1000, 0x89ab_cdef).unwrap();
    let satp = (csr::SATP_MODE_SV39 << 60) | 1;
    dm.hart_mut()
        .state_mut()
        .csr
        .write_as_machine(supervisor::SATP, satp)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write_as_machine(machine::MSTATUS, mstatus::MPRV)
        .unwrap();
    assert_eq!(mem_read(&mut dm, RAM_BASE + 0x1000, 2), 0x89ab_cdef);
}

/// §7 item 2: a supported command while running returns `cmderr` 4.
/// An unsupported command while running is still `cmderr` 2.
#[test]
fn supported_command_while_running_is_cmderr_4() {
    let (mut dm, ram) = ready();
    activate(&mut dm);
    assert!(!dm.hart().in_debug_mode());
    dm.hart_mut().state_mut().regs[1] = 0x55;
    ram.lock().unwrap().write_word(0x1000, 0x0102_0304).unwrap();

    issue(
        &mut dm,
        access_register(4, false, false, true, true, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2, "unsupported wins over not-halted");
    clear_cmderr(&mut dm);

    set_arg0(&mut dm, 0x99);
    issue(
        &mut dm,
        access_register(3, false, false, true, true, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 4);
    assert_eq!(dm.hart().state().regs[1], 0x55);
    clear_cmderr(&mut dm);

    set_arg0(&mut dm, 0x1111_1111);
    set_arg1(&mut dm, RAM_BASE + 0x1000);
    issue(&mut dm, access_memory(false, 2, false, true));
    assert_eq!(cmderr(&mut dm), 4);
    assert_eq!(ram_word(&ram, 0x1000), 0x0102_0304);
}

/// §7 item 2: writing 1 to `cmderr` clears that bit. The field is bitwise
/// write-1-to-clear (Debug Spec 1.0 §3.14, Spike's per-bit clear). Writing 7
/// clears every code. Code 1 is not produced because `busy` stays 0.
#[test]
fn cmderr_w1c_clears_written_bits() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);

    issue(
        &mut dm,
        access_register(4, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    dm.dmi_write(ABSTRACTCS, 1 << 8);
    assert_eq!(cmderr(&mut dm), 2, "bit 0 is already clear");
    dm.dmi_write(ABSTRACTCS, 0);
    assert_eq!(cmderr(&mut dm), 2, "writing 0 clears nothing");
    dm.dmi_write(ABSTRACTCS, 2 << 8);
    assert_eq!(cmderr(&mut dm), 0, "writing the set bit clears it");

    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1020),
    );
    assert_eq!(cmderr(&mut dm), 3);
    dm.dmi_write(ABSTRACTCS, 1 << 8);
    assert_eq!(
        cmderr(&mut dm),
        2,
        "writing 1 clears only bit 0 of cmderr 3"
    );
    dm.dmi_write(ABSTRACTCS, 7 << 8);
    assert_eq!(cmderr(&mut dm), 0);

    clear_cmderr(&mut dm);
    // Drop out of Debug Mode is unnecessary: a running command needs the
    // latch clear. Leave the hart halted and use hart 1 for code 4.
    dm.dmi_write(DMCONTROL, DMACTIVE | hartsel_field(1));
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 4);
    dm.dmi_write(ABSTRACTCS, 1 << 8);
    assert_eq!(cmderr(&mut dm), 4, "cmderr 4 has bit 0 clear");
    dm.dmi_write(ABSTRACTCS, 7 << 8);
    assert_eq!(cmderr(&mut dm), 0);
}

/// §7 item 2: GPR and CSR round-trips honor WARL, including `misa`.
/// The hart is halted from U-mode, so a privilege-checked CSR write would
/// reject `mstatus` (Debug Spec 1.0 §3.7.1.1).
#[test]
fn gpr_and_csr_round_trip_honors_warl() {
    let (mut dm, _) = ready();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MEPC, 0x1000)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MCAUSE, 0x42)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MSTATUS, 3 << mstatus::MPP_SHIFT)
        .unwrap();
    let mstatus_before = dm
        .hart()
        .state()
        .csr
        .read_as_machine(machine::MSTATUS)
        .unwrap();
    assert_eq!((mstatus_before & mstatus::MPP) >> mstatus::MPP_SHIFT, 3);
    dm.hart_mut().state_mut().privilege = PrivilegeMode::User;
    dm.hart_mut()
        .state_mut()
        .csr
        .set_privilege(PrivilegeMode::User);
    enter_halt(&mut dm);
    assert_eq!(dm.hart().state().privilege, PrivilegeMode::User);

    let gpr = 0x1234_5678_9abc_def0;
    set_arg0(&mut dm, gpr);
    issue(
        &mut dm,
        access_register(3, false, false, true, true, 0x1005),
    );
    assert_eq!(cmderr(&mut dm), 0);
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1005),
    );
    assert_eq!(arg0(&mut dm), gpr);
    assert_eq!(dm.hart().state().regs[5], gpr);

    set_arg0(&mut dm, u64::MAX);
    issue(&mut dm, access_register(3, false, false, true, true, 0x301));
    assert_eq!(cmderr(&mut dm), 0, "misa write retires");
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x301),
    );
    assert_eq!(arg0(&mut dm), csr::MISA_VALUE);

    set_arg0(
        &mut dm,
        (mstatus_before & !mstatus::MPP) | (2 << mstatus::MPP_SHIFT),
    );
    issue(&mut dm, access_register(3, false, false, true, true, 0x300));
    assert_eq!(cmderr(&mut dm), 0, "mstatus write from U-mode halt");
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x300),
    );
    let mstatus_after = arg0(&mut dm);
    assert_eq!(
        (mstatus_after & mstatus::MPP) >> mstatus::MPP_SHIFT,
        3,
        "MPP 2 keeps the previous legal mode"
    );

    set_arg0(&mut dm, mstatus::SIE);
    issue(
        &mut dm,
        access_register(3, false, false, true, true, u32::from(supervisor::SSTATUS)),
    );
    assert_eq!(cmderr(&mut dm), 0);
    let merged = dm
        .hart()
        .state()
        .csr
        .read_as_machine(machine::MSTATUS)
        .unwrap();
    assert_ne!(merged & mstatus::SIE, 0, "sstatus merges into mstatus");

    let dcsr = dm.hart().dcsr();
    set_arg0(&mut dm, dcsr | (1 << 15) | (1 << 2));
    issue(
        &mut dm,
        access_register(3, false, false, true, true, 0x7b1 - 1),
    );
    assert_eq!(cmderr(&mut dm), 0);
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x7b0),
    );
    let dcsr_after = arg0(&mut dm);
    assert_eq!(dcsr_after, dcsr | (1 << 15) | (1 << 2));
    assert_eq!(dcsr_after & 0b11, 0, "prv stays U");
    assert_eq!((dcsr_after >> 28) & 0xf, 4, "debugver");

    set_arg0(&mut dm, RAM_BASE | 0b11);
    issue(&mut dm, access_register(3, false, false, true, true, 0x7b1));
    assert_eq!(cmderr(&mut dm), 0);
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x7b1),
    );
    assert_eq!(arg0(&mut dm), RAM_BASE, "dpc bits 1:0 are read-only zero");

    set_arg0(&mut dm, 0xffff_ffff);
    issue(&mut dm, access_register(3, false, false, true, true, 0xf11));
    assert_eq!(cmderr(&mut dm), 3, "mvendorid is read-only");
    clear_cmderr(&mut dm);

    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MEPC)
            .unwrap(),
        0x1000
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MCAUSE)
            .unwrap(),
        0x42
    );
    assert_eq!(dm.hart().state().pc, RAM_BASE);
    assert_eq!(dm.hart().state().privilege, PrivilegeMode::User);
}

/// §7 item 2: an Access Memory read of the PLIC claim register consumes the
/// claim (Debug Spec 1.0 §3.7.1.3). A 1-byte access is rejected by the PLIC.
/// An 8-byte access at the claim address is misaligned.
#[test]
fn plic_claim_read_consumes_the_claim() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    mem_write(&mut dm, PLIC_PRIORITY_SRC1, 2, 1);
    mem_write(&mut dm, PLIC_ENABLE0, 2, 1 << 1);
    mem_write(&mut dm, SYSTEM_BUS_PLIC_GATEWAY, 2, 1);

    set_arg1(&mut dm, PLIC_CLAIM0);
    issue(&mut dm, access_memory(false, 3, false, false));
    assert_eq!(cmderr(&mut dm), 3, "8-byte claim address is misaligned");
    clear_cmderr(&mut dm);

    set_arg1(&mut dm, PLIC_CLAIM0);
    issue(&mut dm, access_memory(false, 0, false, false));
    assert_eq!(cmderr(&mut dm), 3, "PLIC rejects a 1-byte access");
    clear_cmderr(&mut dm);

    assert_eq!(
        mem_read(&mut dm, PLIC_CLAIM0, 2),
        1,
        "claim consumes source 1"
    );
    assert_eq!(
        mem_read(&mut dm, PLIC_CLAIM0, 2),
        0,
        "the claim is now empty"
    );
}

/// §7 item 2: a misaligned access that M-mode would reject returns `cmderr` 3
/// and does not trap (contract D15, Debug Spec 1.0 §3.7.1.3).
#[test]
fn misaligned_access_is_cmderr_3_and_does_not_trap() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    ram.lock()
        .unwrap()
        .write_dword(0x1000, 0x1122_3344_5566_7788)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MEPC, 0x40)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MCAUSE, 0x7)
        .unwrap();
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MTVAL, 0x9)
        .unwrap();
    let pc = dm.hart().state().pc;
    let privilege = dm.hart().state().privilege;
    let minstret = dm
        .hart()
        .state()
        .csr
        .read_as_machine(machine::MINSTRET)
        .unwrap();
    let mcycle = dm
        .hart()
        .state()
        .csr
        .read_as_machine(machine::MCYCLE)
        .unwrap();

    set_arg0(&mut dm, 0xffff_ffff_ffff_ffff);
    set_arg1(&mut dm, RAM_BASE + 0x1004);
    issue(&mut dm, access_memory(false, 3, true, true));
    assert_eq!(cmderr(&mut dm), 3);
    assert_eq!(
        arg1(&mut dm),
        RAM_BASE + 0x1004,
        "no postincrement on failure"
    );
    clear_cmderr(&mut dm);

    set_arg1(&mut dm, RAM_BASE + 0x1001);
    issue(&mut dm, access_memory(false, 2, false, false));
    assert_eq!(cmderr(&mut dm), 3);

    assert_eq!(
        ram.lock().unwrap().read_dword(0x1000).unwrap(),
        0x1122_3344_5566_7788
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MEPC)
            .unwrap(),
        0x40
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MCAUSE)
            .unwrap(),
        0x7
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MTVAL)
            .unwrap(),
        0x9
    );
    assert_eq!(dm.hart().state().pc, pc);
    assert_eq!(dm.hart().state().privilege, privilege);
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MINSTRET)
            .unwrap(),
        minstret
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MCYCLE)
            .unwrap(),
        mcycle
    );
    assert!(dm.hart().in_debug_mode());
}

/// §7 item 2: selecting hart 1 sets `anynonexistent`.
#[test]
fn hart_1_sets_anynonexistent_and_clears_other_status() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    dm.dmi_write(
        DMCONTROL,
        DMACTIVE | hartsel_field(1) | ACKHAVERESET | HALTREQ,
    );
    let status = dmstatus(&mut dm);
    assert!(bit(status, 15) && bit(status, 14), "nonexistent");
    for index in [8, 9, 10, 11, 12, 13, 16, 17, 18, 19] {
        assert!(
            !bit(status, index),
            "bit {index} clear for a nonexistent hart"
        );
    }
    assert!(dm.hart().haltreq(), "hart 0 latch is left alone");
    assert!(dm.hart().in_debug_mode());

    dm.dmi_write(DMCONTROL, DMACTIVE);
    let back = dmstatus(&mut dm);
    assert!(
        bit(back, 18) && bit(back, 19),
        "ackhavereset for hart 1 does not clear hart 0"
    );
    assert!(!bit(back, 14) && !bit(back, 15));

    dm.dmi_write(DMCONTROL, DMACTIVE | hartsel_field(1 << 10));
    let high = dmstatus(&mut dm);
    assert!(bit(high, 14) && bit(high, 15), "any nonzero hartsel");
}

/// §7 item 2: `havereset` sticks until `ackhavereset`.
///
/// The bit is set when the module is constructed, for the hart's power-on
/// reset (Debug Spec 1.0 §3.2). It is not set on `dmactive` or `ndmreset`
/// edges, and `dmactive` 1 to 0 does not clear it. §3.14 allows either
/// choice while `dmactive` is low. This module keeps the bit.
#[test]
fn havereset_sticks_until_ackhavereset() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    let status = dmstatus(&mut dm);
    assert!(bit(status, 18) && bit(status, 19));
    assert!(bit(dmstatus(&mut dm), 18) && bit(dmstatus(&mut dm), 19));

    dm.dmi_write(DMCONTROL, 0);
    dm.dmi_write(DMCONTROL, DMACTIVE);
    assert!(
        bit(dmstatus(&mut dm), 18) && bit(dmstatus(&mut dm), 19),
        "dmactive edges do not clear or set havereset"
    );

    dm.dmi_write(DMCONTROL, DMACTIVE | ACKHAVERESET);
    assert!(!bit(dmstatus(&mut dm), 18) && !bit(dmstatus(&mut dm), 19));
    assert!(!bit(dmstatus(&mut dm), 18));

    dm.dmi_write(DMCONTROL, DMACTIVE | NDMRESET);
    dm.dmi_write(DMCONTROL, DMACTIVE);
    assert!(
        !bit(dmstatus(&mut dm), 18) && !bit(dmstatus(&mut dm), 19),
        "ndmreset falling edge does not set havereset"
    );
}

/// §7 item 2: `datacount` is 4, and a 64-bit address uses `data3`.
/// Reporting `datacount` 2, or ignoring `data3`, fails this test.
#[test]
fn datacount_is_4_and_address_uses_data3() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    let abstractcs = dm.dmi_read(ABSTRACTCS);
    assert_eq!(abstractcs & 0xf, 4, "datacount");
    assert_eq!((abstractcs >> 24) & 0x1f, 0, "progbufsize");
    assert!(!bit(abstractcs, 12), "busy");
    dm.dmi_write(ABSTRACTCS, 1 << 11);
    assert_eq!(dm.dmi_read(ABSTRACTCS) & (1 << 11), 0, "relaxedpriv");
    let info = dm.dmi_read(HARTINFO);
    assert_eq!((info >> 20) & 0xf, 2, "nscratch");
    assert!(!bit(info, 16), "dataaccess");

    ram.lock().unwrap().write_word(0x1000, 0xa5a5_5a5a).unwrap();
    set_word(&mut dm, 2, (RAM_BASE + 0x1000) as u32);
    set_word(&mut dm, 3, 0);
    set_word(&mut dm, 0, 0);
    issue(&mut dm, access_memory(false, 2, false, false));
    assert_eq!(cmderr(&mut dm), 0);
    assert_eq!(word(&mut dm, 0), 0xa5a5_5a5a);

    set_word(&mut dm, 3, 1);
    set_word(&mut dm, 0, 0);
    issue(&mut dm, access_memory(false, 2, false, false));
    assert_ne!(word(&mut dm, 0), 0xa5a5_5a5a);
    assert_eq!(cmderr(&mut dm), 3, "data3 selects a different address");
}

/// A `command` write is ignored while `cmderr` is not 0 (Debug Spec 1.0 §3.14).
#[test]
fn command_is_ignored_while_cmderr_is_set() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    issue(
        &mut dm,
        access_register(4, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    dm.hart_mut().state_mut().regs[1] = 5;
    set_arg0(&mut dm, 99);
    issue(
        &mut dm,
        access_register(3, false, false, true, true, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().regs[1], 5);
    assert_eq!(arg0(&mut dm), 99);
}

/// `abstractauto` is unimplemented. A `data0` access does not replay `command`.
#[test]
fn abstractauto_is_unimplemented() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    dm.hart_mut().state_mut().regs[1] = 5;
    dm.dmi_write(ABSTRACTAUTO, 1);
    assert_eq!(dm.dmi_read(ABSTRACTAUTO), 0);
    set_arg0(&mut dm, 99);
    assert_eq!(arg0(&mut dm), 99);
    assert_eq!(dm.hart().state().regs[1], 5);

    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1001),
    );
    assert_eq!(arg0(&mut dm), 5);
    set_arg0(&mut dm, 7);
    assert_eq!(
        dm.hart().state().regs[1],
        5,
        "writing data0 does not replay"
    );
    assert_eq!(dm.dmi_read(DATA0), 7);
}

/// Quick Access is `cmdtype` 1 and returns `cmderr` 2 (Debug Spec 1.0 §3.7.1).
#[test]
fn quick_access_is_cmderr_2() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    dm.hart_mut().state_mut().regs[1] = 5;
    issue(&mut dm, 1 << 24);
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().regs[1], 5);
    assert!(!dm.hart().in_debug_mode());
    clear_cmderr(&mut dm);
    enter_halt(&mut dm);
    issue(&mut dm, 1 << 24);
    assert_eq!(cmderr(&mut dm), 2);
    assert_eq!(dm.hart().state().regs[1], 5);
}

/// CSR `0x7a0` is not implemented. The failure is `cmderr` 3 and not a trap.
#[test]
fn unimplemented_csr_is_cmderr_3() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    let pc = dm.hart().state().pc;
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x7a0),
    );
    assert_eq!(cmderr(&mut dm), 3);
    assert_eq!(arg0(&mut dm), 0);
    assert_eq!(dm.hart().state().pc, pc);
    assert!(dm.hart().in_debug_mode());
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MEPC)
            .unwrap(),
        0
    );
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MCAUSE)
            .unwrap(),
        0
    );
}

/// `x0` reads as 0 and ignores writes (Debug Spec 1.0 §3.7.1.1).
#[test]
fn x0_read_is_zero_and_write_is_ignored() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    set_arg0(&mut dm, 0x1111_1111_2222_2222);
    issue(
        &mut dm,
        access_register(3, false, false, true, true, 0x1000),
    );
    assert_eq!(cmderr(&mut dm), 0);
    assert_eq!(dm.hart().state().regs[0], 0);
    set_arg0(&mut dm, 0xffff);
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1000),
    );
    assert_eq!(arg0(&mut dm), 0);
}

/// `aarsize` 2 updates bits 31:0 and preserves bits 63:32. The spec leaves
/// the high half UNSPECIFIED (Debug Spec 1.0 §3.7.1.1). This module preserves it.
#[test]
fn aarsize_2_preserves_high_half() {
    let (mut dm, _) = ready();
    enter_halt(&mut dm);
    dm.hart_mut().state_mut().regs[1] = 0xaaaa_bbbb_cccc_dddd;
    set_word(&mut dm, 0, 0x1111_1111);
    set_word(&mut dm, 1, 0x7777_7777);
    issue(
        &mut dm,
        access_register(2, false, false, true, true, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 0);
    assert_eq!(dm.hart().state().regs[1], 0xaaaa_bbbb_1111_1111);
    assert_eq!(word(&mut dm, 1), 0x7777_7777);

    issue(
        &mut dm,
        access_register(2, false, false, true, false, 0x1001),
    );
    assert_eq!(word(&mut dm, 0), 0x1111_1111);
    assert_eq!(word(&mut dm, 1), 0x7777_7777, "a size-2 read leaves data1");
}

/// `aampostincrement` adds the access size only after a successful access
/// (Debug Spec 1.0 §3.7.1.3).
#[test]
fn aampostincrement_advances_data2_only_on_success() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    ram.lock().unwrap().write_word(0x1000, 0x0102_0304).unwrap();
    set_arg1(&mut dm, RAM_BASE + 0x1000);
    set_arg0(&mut dm, 0);
    issue(&mut dm, access_memory(false, 2, true, false));
    assert_eq!(cmderr(&mut dm), 0);
    assert_eq!(arg0(&mut dm), 0x0102_0304);
    assert_eq!(arg1(&mut dm), RAM_BASE + 0x1004);

    set_arg1(&mut dm, RAM_BASE + 0x1001);
    issue(&mut dm, access_memory(false, 2, true, false));
    assert_eq!(cmderr(&mut dm), 3);
    assert_eq!(arg1(&mut dm), RAM_BASE + 0x1001);
}

/// `ndmreset` is stored. It does not reset the hart (that is U3).
/// While it is 1 the hart is unavailable and a command returns `cmderr` 4
/// (Debug Spec 1.0 §3.2, §3.14 `ndmresetpending`).
#[test]
fn ndmreset_does_not_change_the_hart() {
    let (mut dm, ram) = ready();
    enter_halt(&mut dm);
    dm.hart_mut().state_mut().regs[1] = 0x77;
    dm.hart_mut().write_dpc(RAM_BASE + 0x20);
    dm.hart_mut()
        .state_mut()
        .csr
        .write(machine::MEPC, 0x30)
        .unwrap();
    ram.lock().unwrap().write_word(0x1000, 0xabc).unwrap();
    let dcsr = dm.hart().dcsr();

    dm.dmi_write(DMCONTROL, DMACTIVE | NDMRESET | HALTREQ);
    assert_eq!(dm.hart().state().regs[1], 0x77);
    assert_eq!(dm.hart().dpc(), RAM_BASE + 0x20);
    assert_eq!(dm.hart().dcsr(), dcsr);
    assert_eq!(
        dm.hart()
            .state()
            .csr
            .read_as_machine(machine::MEPC)
            .unwrap(),
        0x30
    );
    assert_eq!(ram_word(&ram, 0x1000), 0xabc);
    assert!(dm.hart().in_debug_mode(), "the hart latch stays halted");
    let status = dmstatus(&mut dm);
    assert!(bit(status, 24), "ndmresetpending");
    assert!(bit(status, 12) && bit(status, 13), "unavailable");
    assert!(
        !bit(status, 8) && !bit(status, 9),
        "unavailable is not halted"
    );
    assert!(!bit(status, 10) && !bit(status, 11));
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 4);
    assert_eq!(dm.hart().state().regs[1], 0x77);

    let resumeack = bit(dmstatus(&mut dm), 16);
    dm.dmi_write(DMCONTROL, DMACTIVE | NDMRESET | RESUMEREQ);
    assert!(
        dm.hart().in_debug_mode(),
        "resumereq while ndmreset does not resume"
    );
    assert_eq!(bit(dmstatus(&mut dm), 16), resumeack);

    dm.dmi_write(DMCONTROL, DMACTIVE | HALTREQ);
    assert_eq!(dm.hart().state().regs[1], 0x77);
    assert_eq!(dm.hart().dpc(), RAM_BASE + 0x20);
    assert_eq!(ram_word(&ram, 0x1000), 0xabc);
    assert!(!bit(dmstatus(&mut dm), 24));
    assert!(bit(dmstatus(&mut dm), 8), "halted once ndmreset drops");
}

/// `dmactive` 0 hides the version and drops a write that leaves it 0.
/// 1 to 0 resets the module and leaves the hart halted (Debug Spec 1.0 §3.14, §3.5).
#[test]
fn dmactive_low_hides_version_and_ignores_haltreq() {
    let (mut dm, _) = ready();
    assert_eq!(dmstatus(&mut dm) & 0xf, 0);
    dm.dmi_write(DMCONTROL, HALTREQ);
    assert!(!dm.hart().haltreq());
    let outcome = dm.hart_mut().step_outcome();
    assert!(matches!(outcome, StepOutcome::InstructionRetired(_)));
    assert!(!dm.hart().in_debug_mode());

    enter_halt(&mut dm);
    set_word(&mut dm, 0, 0x1234);
    issue(
        &mut dm,
        access_register(4, false, false, true, false, 0x1001),
    );
    assert_eq!(cmderr(&mut dm), 2);
    dm.dmi_write(DMCONTROL, HALTREQ);
    assert_eq!(dm.dmi_read(DMSTATUS), 0);
    assert_eq!(dm.dmi_read(DATA0), 0);
    assert_eq!(dm.dmi_read(DMCONTROL), 0);
    assert!(!dm.hart().haltreq(), "the reset clears the haltreq latch");
    assert!(dm.hart().in_debug_mode(), "the hart stays halted");

    dm.dmi_write(DMCONTROL, DMACTIVE);
    assert_eq!(dmstatus(&mut dm) & 0xf, 3);
    assert_eq!(cmderr(&mut dm), 0, "module reset clears cmderr");
    assert_eq!(word(&mut dm, 0), 0);
    assert!(dm.hart().in_debug_mode());
    assert!(!dm.hart().haltreq());
}

/// `resumereq` is applied to `haltreq` after the same write (Debug Spec 1.0 §3.14).
/// Resume ack resets to 1 (§3.5).
#[test]
fn resume_requires_haltreq_clear_in_the_same_write() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    let running = dmstatus(&mut dm);
    assert!(
        bit(running, 16) && bit(running, 17),
        "resumeack resets to 1"
    );
    assert!(bit(running, 10) && bit(running, 11), "running");

    enter_halt(&mut dm);
    assert!(bit(dmstatus(&mut dm), 8));
    dm.dmi_write(DMCONTROL, DMACTIVE | HALTREQ | RESUMEREQ);
    assert!(
        dm.hart().in_debug_mode(),
        "haltreq set in this write blocks resume"
    );
    assert!(dm.hart().haltreq());

    dm.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ);
    assert!(!dm.hart().in_debug_mode());
    assert!(!dm.hart().haltreq());
    assert_eq!(dm.hart().state().pc, RAM_BASE);
    let resumed = dmstatus(&mut dm);
    assert!(bit(resumed, 16) && bit(resumed, 17), "resumeack");
    assert!(bit(resumed, 10) && bit(resumed, 11));
    assert!(!bit(resumed, 8) && !bit(resumed, 9));

    let pc = dm.hart().state().pc;
    dm.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ);
    assert!(!dm.hart().in_debug_mode());
    assert_eq!(dm.hart().state().pc, pc, "a running hart ignores resumereq");
    assert!(bit(dmstatus(&mut dm), 16));
}

/// `haltsum0` bit 0 follows hart 0 while `hartsel` is below 32.
/// Debug Spec 1.0 §3.14: the window base is `{hartsel[19:5], 5'b0}`.
#[test]
fn haltsum0_bit0_follows_hart_0_for_hartsel_below_32() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    assert_eq!(dm.dmi_read(HALTSUM0), 0);
    enter_halt(&mut dm);
    assert_eq!(dm.dmi_read(HALTSUM0), 1);
    dm.dmi_write(DMCONTROL, DMACTIVE | hartsel_field(1));
    assert_eq!(dm.dmi_read(HALTSUM0), 1, "hartsel 1 still covers hart 0");
    dm.dmi_write(DMCONTROL, DMACTIVE | hartsel_field(32));
    assert_eq!(dm.dmi_read(HALTSUM0), 0);
    dm.dmi_write(DMCONTROL, DMACTIVE | NDMRESET);
    assert_eq!(dm.dmi_read(HALTSUM0), 0, "unavailable is not halted");
}

/// `sbcs` and `progbuf0` are absent, so they read 0 (Debug Spec 1.0 §3.14).
#[test]
fn sbcs_and_progbuf0_read_zero() {
    let (mut dm, _) = ready();
    activate(&mut dm);
    dm.dmi_write(SBCS, 0x1234_5678);
    assert_eq!(dm.dmi_read(SBCS), 0);
    dm.dmi_write(PROGBUF0, 0x00ab_cdef);
    assert_eq!(dm.dmi_read(PROGBUF0), 0);
    dm.dmi_write(0x08, 0xffff_ffff);
    assert_eq!(dm.dmi_read(0x08), 0, "data4 is outside datacount 4");
    assert_eq!(dm.dmi_read(COMMAND), 0, "command reads as 0");
}

/// Demo: discovery, halt, read `dpc` and a GPR, 64-bit memory round-trip,
/// resume with `resumeack`, then the guest stores its exit word.
#[test]
fn debug_module_halts_reads_and_resumes_guest() {
    let (mut dm, ram) = harness();
    // addi x5, x0, 17; lui x6, 0x80002; sd x5, 0(x6); jal x0, 0
    let program = [0x0110_0293, 0x8000_2337, 0x0053_3023, 0x0000_006f];
    for (index, word) in program.iter().enumerate() {
        ram.lock()
            .unwrap()
            .write_word((index * 4) as u64, *word)
            .unwrap();
    }
    dm.hart_mut().state_mut().pc = RAM_BASE;

    assert_eq!(dm.dmi_read(DMSTATUS), 0);
    dm.dmi_write(DMCONTROL, DMACTIVE);
    assert_eq!(dmstatus(&mut dm) & 0xf, 3);

    dm.dmi_write(DMCONTROL, DMACTIVE | HALTREQ);
    assert!(!dm.hart().in_debug_mode());
    let outcome = dm.hart_mut().step_outcome();
    assert!(matches!(
        outcome,
        StepOutcome::DebugHalted {
            cause: DebugCause::HaltReq,
            ..
        }
    ));

    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x7b1),
    );
    assert_eq!(cmderr(&mut dm), 0);
    assert_eq!(arg0(&mut dm), RAM_BASE, "dpc");
    issue(
        &mut dm,
        access_register(3, false, false, true, false, 0x1005),
    );
    assert_eq!(arg0(&mut dm), 0, "x5 has not executed");

    let pattern = 0x1122_3344_5566_7788;
    mem_write(&mut dm, RAM_BASE + 0x3000, 3, pattern);
    assert_eq!(mem_read(&mut dm, RAM_BASE + 0x3000, 3), pattern);
    assert_eq!(ram.lock().unwrap().read_dword(0x3000).unwrap(), pattern);

    dm.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ);
    let status = dmstatus(&mut dm);
    assert!(bit(status, 16) && bit(status, 17), "resumeack");
    assert!(!dm.hart().in_debug_mode());

    let mut exited = false;
    for _ in 0..6 {
        if ram_word(&ram, 0x2000) == 17 {
            exited = true;
            break;
        }
        let outcome = dm.hart_mut().step_outcome();
        assert!(
            matches!(outcome, StepOutcome::InstructionRetired(_)),
            "{outcome:?}"
        );
    }
    assert!(exited, "guest store is the normal exit");
    assert_eq!(dm.hart().state().regs[5], 17);
    assert!(!dm.hart().in_debug_mode());
}
