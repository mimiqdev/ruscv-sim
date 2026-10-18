//! Stage 5 U1 on the public Machine.
//!
//! Privileged specification 20250508 §3.2.1 makes `time` a read-only shadow of
//! `mtime`. §3.1.11 leaves `mcounteren` WARL, and this profile keeps it 0, so
//! an S-mode or U-mode read of `time` is illegal. ACLINT 1.0-rc4 resets MTIME
//! to 0 (line 198) and MSIP to 0 (line 344). MTIMECMP reset is unknown
//! (line 213). This profile uses all-ones. Contract D1 advances `mtime` by one
//! completed Hart turn and does not alias that register with `mcycle` or
//! `minstret`.

#[allow(dead_code)]
mod common;
use common::public_elf as elf;
use ruscv_sim::core::trap::{ExceptionCause, Trap};
use ruscv_sim::core::StepOutcome;
use ruscv_sim::csr::{machine, unprivileged};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::PrivilegeMode;
use std::sync::Arc;

const MTIME: u64 = 0x0200_BFF8;
const MTIMECMP: u64 = 0x0200_4000;
const MSIP: u64 = 0x0200_0000;

fn csrrs(rd: u8, csr: u16) -> u32 {
    (csr as u32) << 20 | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn csrrw(csr: u16, rs1: u8) -> u32 {
    (csr as u32) << 20 | (u32::from(rs1) << 15) | (1 << 12) | 0x73
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

fn machine_with(kind: PlatformKind, code: &[u32]) -> Machine {
    let bytes = elf::elf_with_code(code, 0, true, false, elf::BSS_MEMORY_SIZE);
    let image = Arc::new(LoadImage::parse(&bytes).unwrap());
    let machine = Machine::new(MachineConfig::new(kind));
    machine.install(image).unwrap();
    machine.resume().unwrap();
    machine
}

fn retire(machine: &Machine) {
    let turn = machine.step(false).expect("step");
    assert!(
        turn.hart().control.retired,
        "expected retirement, got {:?}",
        turn.hart().outcome
    );
}

fn reg(machine: &Machine, index: usize) -> u64 {
    machine.inspect().unwrap().hart.regs[index]
}

fn assert_illegal(machine: &Machine) {
    let turn = machine.step(false).unwrap();
    match &turn.hart().outcome {
        StepOutcome::TrapEntered(entered) => {
            assert_eq!(
                entered.cause,
                Trap::Exception(ExceptionCause::IllegalInstruction)
            );
        }
        other => panic!("expected illegal-instruction, got {other:?}"),
    }
}

#[test]
fn native_clint_window_mtime_and_time_stay_off_the_cycle_counters() {
    // At the start of instruction N, mtime is N. The load or `time` read sees
    // that value. A completed store of V leaves V + 1 for the next instruction.
    let code = [
        elf::lui(5, 0x0200C),          // 0  x5 = 0x0200_C000
        elf::addi(5, 5, -8),           // 1  x5 = mtime
        elf::lui(6, 0x02004),          // 2  x6 = mtimecmp
        elf::lui(7, 0x02000),          // 3  x7 = msip
        load(3, 8, 6, 0),              // 4  x8 = mtimecmp reset
        elf::addi(9, 0, -1),           // 5
        store(3, 9, 6, 0),             // 6  mtimecmp = all-ones
        load(3, 10, 6, 0),             // 7  x10 = all-ones
        store(2, 9, 7, 0),             // 8  msip writes keep bit 0
        load(2, 12, 7, 0),             // 9  x12 = 1
        store(2, 0, 7, 0),             // 10
        load(3, 13, 5, 0),             // 11 x13 = mtime == 11
        csrrs(14, unprivileged::TIME), // 12 x14 = time == 12
        elf::lui(15, 0x1),             // 13 x15 = 0x1000
        store(3, 15, 5, 0),            // 14 mtime = 0x1000, then one tick
        load(3, 16, 5, 0),             // 15 x16 = 0x1001
        csrrs(17, unprivileged::TIME), // 16 x17 = 0x1002
        csrrw(machine::MCYCLE, 15),    // 17 mcycle = 0x1000
        load(3, 18, 5, 0),             // 18 x18 = mtime == 0x1004
        csrrs(19, machine::MCYCLE),    // 19 x19 = 0x1001
        csrrs(20, machine::MINSTRET),  // 20 x20 = 20
        csrrs(21, unprivileged::TIME), // 21 trapped from S
    ];
    let machine = machine_with(PlatformKind::Native, &code);
    for _ in 0..21 {
        retire(&machine);
    }

    assert_eq!(reg(&machine, 5), MTIME);
    assert_eq!(reg(&machine, 6), MTIMECMP);
    assert_eq!(reg(&machine, 7), MSIP);
    assert_eq!(
        reg(&machine, 8),
        u64::MAX,
        "ACLINT leaves MTIMECMP unknown; this profile resets it to all-ones"
    );
    assert_eq!(reg(&machine, 10), u64::MAX);
    assert_eq!(reg(&machine, 12), 1, "ACLINT MSIP keeps only bit 0");
    assert_eq!(reg(&machine, 13), 11);
    assert_eq!(
        reg(&machine, 14),
        12,
        "time reads the mtime of this instruction"
    );
    assert_eq!(reg(&machine, 16), 0x1001);
    assert_eq!(reg(&machine, 17), 0x1002);
    assert_eq!(
        reg(&machine, 18),
        0x1004,
        "writing mcycle does not write mtime"
    );
    assert_eq!(
        reg(&machine, 19),
        0x1001,
        "writing mtime does not write mcycle"
    );
    assert_eq!(
        reg(&machine, 20),
        20,
        "writing mtime does not write minstret"
    );

    machine.request_quiesce().unwrap();
    machine.try_drain().unwrap();
    let mut state = machine.inspect().unwrap().hart;
    state.privilege = PrivilegeMode::Supervisor;
    state.csr.set_privilege(PrivilegeMode::Supervisor);
    machine.set_hart_state(state).unwrap();
    machine.resume().unwrap();
    assert_illegal(&machine);
}

#[test]
fn flat_time_tracks_completed_turns_and_rejects_a_write() {
    let code = [
        csrrs(1, unprivileged::TIME), // 0 reads 0
        elf::lui(2, 0x10),            // 1 x2 = 0x1_0000
        csrrw(machine::MCYCLE, 2),    // 2 mcycle = 0x1_0000
        csrrs(3, unprivileged::TIME), // 3 reads 3
        csrrs(4, machine::MCYCLE),    // 4 reads 0x1_0001
        csrrw(unprivileged::TIME, 2), // 5 illegal write
    ];
    let machine = machine_with(PlatformKind::Flat, &code);
    for _ in 0..5 {
        retire(&machine);
    }
    assert_eq!(reg(&machine, 1), 0);
    assert_eq!(reg(&machine, 3), 3);
    assert_eq!(reg(&machine, 4), 0x1_0001);
    assert_illegal(&machine);

    let bytes = elf::elf_with_code(
        &[csrrs(1, unprivileged::TIME)],
        0,
        true,
        false,
        elf::BSS_MEMORY_SIZE,
    );
    let image = Arc::new(LoadImage::parse(&bytes).unwrap());
    let user = Machine::new(MachineConfig::new(PlatformKind::Flat));
    user.install(image).unwrap();
    let mut state = user.inspect().unwrap().hart;
    state.privilege = PrivilegeMode::User;
    state.csr.set_privilege(PrivilegeMode::User);
    user.set_hart_state(state).unwrap();
    user.resume().unwrap();
    assert_illegal(&user);
}
