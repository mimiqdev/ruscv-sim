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
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AccessWidth, PhysicalAccessError, PhysicalRequest, PhysicalTargetRejectionReason,
    ValidatedPhysicalAccess,
};
use ruscv_sim::PrivilegeMode;
use std::sync::{Arc, Mutex};

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
    for _ in 0..14 {
        retire(&machine);
    }
    let before_store = machine.inspect().unwrap().hart;
    let mcycle_before = before_store.csr.read(machine::MCYCLE).unwrap();
    let minstret_before = before_store.csr.read(machine::MINSTRET).unwrap();
    retire(&machine);
    let after_store = machine.inspect().unwrap().hart;
    assert_eq!(
        after_store.csr.read(machine::MCYCLE).unwrap(),
        mcycle_before.wrapping_add(1),
        "the mtime store must not write mcycle"
    );
    assert_eq!(
        after_store.csr.read(machine::MINSTRET).unwrap(),
        minstret_before.wrapping_add(1),
        "the mtime store must not write minstret"
    );
    for _ in 0..6 {
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
        "the explicit mcycle write reads back as that value plus one tick"
    );
    assert_eq!(
        machine
            .inspect()
            .unwrap()
            .hart
            .csr
            .read(unprivileged::TIME)
            .unwrap(),
        0x1007,
        "after the last retired turn, time matches the post-tick mtime"
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
    assert_eq!(
        machine
            .inspect()
            .unwrap()
            .hart
            .csr
            .read(unprivileged::TIME)
            .unwrap(),
        6,
        "trap entry completes a turn, so mtime and time advance"
    );

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

#[test]
fn native_word_halves_of_mtime_and_mtimecmp() {
    // mtimecmp is not a clock, so the two half writes read back exactly.
    // mtime gains one tick after every completed turn, including each `sw`.
    let code = [
        elf::lui(5, 0x0200C),    // 0
        elf::addi(5, 5, -8),     // 1  x5 = mtime
        elf::addi(6, 5, 4),      // 2  x6 = mtime + 4
        elf::lui(7, 0x02004),    // 3  x7 = mtimecmp
        elf::addi(8, 7, 4),      // 4  x8 = mtimecmp + 4
        elf::addi(15, 0, 0x333), // 5
        elf::addi(16, 0, 0x444), // 6
        store(2, 15, 7, 0),      // 7  sw low mtimecmp
        store(2, 16, 8, 0),      // 8  sw high mtimecmp
        load(3, 17, 7, 0),       // 9  ld mtimecmp
        load(2, 18, 7, 0),       // 10 lw low
        load(2, 19, 8, 0),       // 11 lw high
        elf::addi(10, 0, 0x111), // 12
        elf::addi(11, 0, 0x222), // 13
        store(2, 10, 5, 0),      // 14 sw low mtime, then one tick
        store(2, 11, 6, 0),      // 15 sw high mtime, then one tick
        load(3, 12, 5, 0),       // 16 ld mtime
        load(2, 13, 5, 0),       // 17 lw low mtime
        load(2, 14, 6, 0),       // 18 lw high mtime
    ];
    let machine = machine_with(PlatformKind::Native, &code);
    for _ in 0..code.len() {
        retire(&machine);
    }
    assert_eq!(reg(&machine, 17), (0x444u64 << 32) | 0x333);
    assert_eq!(reg(&machine, 18), 0x333);
    assert_eq!(reg(&machine, 19), 0x444);
    // sw low plants 0x111 and the turn ticks to 0x112. sw high keeps that
    // low half and the next turn ticks it to 0x113, which `ld` reads.
    assert_eq!(reg(&machine, 12), (0x222u64 << 32) | 0x113);
    assert_eq!(reg(&machine, 13), 0x114, "the ld's turn ticks the low half");
    assert_eq!(reg(&machine, 14), 0x222);
}

fn expect_trap(code: &[u32], cause: ExceptionCause, mtval: u64) {
    let machine = machine_with(PlatformKind::Native, code);
    for _ in 0..code.len() - 1 {
        retire(&machine);
    }
    let turn = machine.step(false).expect("step");
    match &turn.hart().outcome {
        StepOutcome::TrapEntered(entered) => {
            assert_eq!(entered.cause, Trap::Exception(cause));
            assert_eq!(entered.mtval, mtval);
        }
        StepOutcome::SimulatorFailure(failure) => {
            panic!("negative CLINT access must trap, got {failure:?}")
        }
        other => panic!("expected {cause:?}, got {other:?}"),
    }
}

fn amoadd_w(rd: u8, rs1: u8, rs2: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x2f
}

fn lr_d(rd: u8, rs1: u8) -> u32 {
    (0b00010 << 27) | (u32::from(rs1) << 15) | (3 << 12) | (u32::from(rd) << 7) | 0x2f
}

fn jalr(rd: u8, rs1: u8) -> u32 {
    (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x67
}

fn mtime_in(x: u8) -> [u32; 2] {
    [elf::lui(x, 0x0200C), elf::addi(x, x, -8)]
}

#[test]
fn native_clint_rejects_narrow_atomic_misaligned_and_fetch() {
    // Unprivileged load/store rules allow either address-misaligned or an
    // access fault when the region does not support the alignment. This
    // profile's hart raises address-misaligned before the bus. Narrow and
    // atomic accesses that are aligned still reach the window and trap as
    // access faults. A fetch of the window is an instruction access fault.
    let mut lb = mtime_in(5).to_vec();
    lb.push(load(0, 6, 5, 0));
    expect_trap(&lb, ExceptionCause::LoadAccessFault, MTIME);

    let mut sh = mtime_in(5).to_vec();
    sh.push(store(1, 6, 5, 0));
    expect_trap(&sh, ExceptionCause::StoreAccessFault, MTIME);

    let mut misaligned = mtime_in(5).to_vec();
    misaligned.push(elf::addi(5, 5, 2));
    misaligned.push(load(2, 6, 5, 0));
    expect_trap(
        &misaligned,
        ExceptionCause::LoadAddressMisaligned,
        0x0200_BFFA,
    );

    let mut amo = mtime_in(5).to_vec();
    amo.push(amoadd_w(6, 5, 7));
    expect_trap(&amo, ExceptionCause::StoreAccessFault, MTIME);

    let mut lr = mtime_in(5).to_vec();
    lr.push(lr_d(6, 5));
    expect_trap(&lr, ExceptionCause::LoadAccessFault, MTIME);

    let machine = machine_with(PlatformKind::Native, &[elf::lui(1, 0x02000), jalr(0, 1)]);
    retire(&machine);
    retire(&machine);
    let turn = machine.step(false).expect("fetch");
    match &turn.hart().outcome {
        StepOutcome::TrapEntered(entered) => {
            assert_eq!(
                entered.cause,
                Trap::Exception(ExceptionCause::InstructionAccessFault)
            );
            assert_eq!(entered.mtval, 0x0200_0000);
        }
        StepOutcome::SimulatorFailure(failure) => {
            panic!("a fetch of the CLINT must trap, got {failure:?}")
        }
        other => panic!("expected instruction access fault, got {other:?}"),
    }
}

#[test]
fn raw_misaligned_clint_word_is_a_target_error() {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0x8000_0000, 0x1000)));
    let mut port = ValidatedPhysicalAccess::new(SystemBus::physical_backend(bus));
    let error = port
        .access(PhysicalRequest::data_read(0x0200_BFFA, AccessWidth::Word).unwrap())
        .expect_err("misaligned CLINT word");
    match error {
        PhysicalAccessError::TargetRejected(rejection) => {
            assert_eq!(rejection.reason, PhysicalTargetRejectionReason::TargetError);
        }
        other => panic!("expected a target error, got {other}"),
    }
}
