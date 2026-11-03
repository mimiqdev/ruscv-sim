//! F/D U1: FP profiles, FPR/`fcsr` state, `fflags`/`frm`/`fcsr` and
//! `mstatus.FS` (issue #145; contract D3, D5, D6, §3 and unit U1).
//!
//! These assertions pin the profile boundary and the floating-point state.
//! FP instruction execution is U3a, so every F/D/Zcd encoding still raises
//! illegal-instruction here.  The `rv64ima` and `rv64imac`
//! profiles are proven unchanged by the existing suites running unmodified;
//! the controls below pin their hard-wired `FS`/`SD` and absent FP CSRs.

pub mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome, Trap};
use ruscv_sim::csr::{self, machine, mstatus, supervisor, unprivileged, CsrError};
use ruscv_sim::executor::RiscVSimulator;
use ruscv_sim::fpu::{Fcsr, FpFlags, Fpr, FpuRegisterFile, CANONICAL_NAN_F32, NAN_BOX_MASK};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::IsaProfile;
use std::sync::{Arc, Mutex};

const RAM_SIZE: usize = 0x1000;

// ============== Instruction encoding ==============

const CSRRW: u32 = 0b001;
const CSRRS: u32 = 0b010;
const CSRRC: u32 = 0b011;
const CSRRWI: u32 = 0b101;
const CSRRSI: u32 = 0b110;
const CSRRCI: u32 = 0b111;

/// Encode a CSR instruction.  For the immediate forms `rs1` is the uimm.
fn csr_op(funct3: u32, rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (funct3 << 12) | (u32::from(rd) << 7) | 0x73
}

fn csrr(rd: u8, csr: u16) -> u32 {
    csr_op(CSRRS, rd, csr, 0)
}

fn csrw(csr: u16, rs1: u8) -> u32 {
    csr_op(CSRRW, 0, csr, rs1)
}

// ============== Hart fixtures ==============

fn core_with(isa: IsaProfile) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let mut core = RiscvCore::new(typed.clone(), typed);
    core.set_isa_profile(isa);
    core.reset(0, 0);
    (core, ram)
}

fn load(ram: &Arc<Mutex<SimpleMemory>>, words: &[u32]) {
    let mut mem = ram.lock().unwrap();
    for (index, word) in words.iter().enumerate() {
        mem.write_word((index * 4) as u64, *word).unwrap();
    }
}

fn retire(core: &mut RiscvCore) {
    match core.step_outcome() {
        StepOutcome::InstructionRetired(_) => {}
        other => panic!("expected retirement, got {other:?}"),
    }
}

fn trap(core: &mut RiscvCore) -> ruscv_sim::core::TrapEntered {
    match core.step_outcome() {
        StepOutcome::TrapEntered(entered) => entered,
        other => panic!("expected trap, got {other:?}"),
    }
}

fn misa(core: &RiscvCore) -> u64 {
    core.state()
        .csr
        .read(machine::MISA)
        .expect("misa is readable in M-mode")
}

fn read_csr(core: &RiscvCore, addr: u16) -> u64 {
    core.state().csr.read(addr).expect("M-mode CSR read")
}

fn write_csr(core: &mut RiscvCore, addr: u16, value: u64) {
    core.state_mut()
        .csr
        .write(addr, value)
        .expect("M-mode CSR write")
}

/// Run one instruction that targets an FP CSR and expect illegal-instruction.
fn expect_fp_csr_illegal(isa: IsaProfile, instruction: u32, fs: u64) {
    let (mut core, ram) = core_with(isa);
    // Seed nonzero architectural state and a destination sentinel, so the
    // post-trap assertions prove preservation instead of matching the reset
    // values.
    core.state_mut().fcsr.write(0xB5);
    // f0 is an ordinary register, so seed it with the others.
    core.state_mut().fpr.write_u32(0, 0x3F80_0000);
    core.state_mut().fpr.write_u32(7, 0x4049_0FDB);
    core.state_mut().regs[1] = 0xDEAD_BEEF;
    let fprs: [u64; 32] = std::array::from_fn(|reg| core.state().fpr.read(reg).bits());
    if fs != mstatus::FS_OFF {
        write_csr(&mut core, machine::MSTATUS, fs);
    }
    load(&ram, &[instruction]);
    let entered = trap(&mut core);
    assert_eq!(
        entered.cause,
        Trap::Exception(ExceptionCause::IllegalInstruction),
        "profile {} instruction {instruction:#010x}",
        isa.name()
    );
    assert_eq!(
        entered.mtval,
        u64::from(instruction),
        "illegal FP CSR access reports the instruction in mtval"
    );
    assert_eq!(
        core.state().fcsr.read(),
        0xB5,
        "a trapping FP CSR access has no fcsr effect"
    );
    for (reg, expected) in fprs.iter().enumerate() {
        assert_eq!(
            core.state().fpr.read(reg).bits(),
            *expected,
            "a trapping FP CSR access has no FPR effect (f{reg})"
        );
    }
    assert_eq!(
        core.state().regs[1],
        0xDEAD_BEEF,
        "a trapping CSR read does not write its destination register"
    );
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        fs & mstatus::FS,
        "a trapping FP CSR access has no FS effect"
    );
}

/// Run one FP CSR instruction expected to succeed, with `fs` as the entry
/// state, and return the retired instruction's observation through state.
fn fp_csr_instruction(isa: IsaProfile, fs: u64, instruction: u32) -> RiscvCore {
    let (mut core, ram) = core_with(isa);
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        mstatus::FS_OFF
    );
    write_csr(&mut core, machine::MSTATUS, fs);
    assert_eq!(read_csr(&core, machine::MSTATUS) & mstatus::FS, fs);
    load(&ram, &[instruction]);
    retire(&mut core);
    core
}

// ============== D3: profile selection and misa ==============

#[test]
fn profiles_report_exact_read_only_misa() {
    for (isa, expected) in [
        (IsaProfile::Rv64ima, 0x8000_0000_0014_1101),
        (IsaProfile::Rv64imac, 0x8000_0000_0014_1105),
        (IsaProfile::Rv64imafd, 0x8000_0000_0014_1129),
        (IsaProfile::Rv64imafdc, 0x8000_0000_0014_112D),
    ] {
        let (mut core, _) = core_with(isa);
        assert_eq!(misa(&core), expected, "profile {}", isa.name());
        // `misa` stays read-only under every profile (contract D3): no
        // extension can be enabled or disabled, so a write keeps the value.
        let _ = core.state_mut().csr.write(machine::MISA, 0);
        assert_eq!(misa(&core), expected, "profile {}", isa.name());
    }
}

#[test]
fn fp_capability_checks_are_explicit() {
    assert!(!IsaProfile::Rv64ima.has_f());
    assert!(!IsaProfile::Rv64ima.has_d());
    assert!(!IsaProfile::Rv64imac.has_f());
    assert!(!IsaProfile::Rv64imac.has_d());

    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        assert!(isa.has_f(), "{}", isa.name());
        assert!(isa.has_d(), "{}", isa.name());
        assert!(isa.has_fp(), "{}", isa.name());
    }
    assert!(!IsaProfile::Rv64imafd.allows_compressed());
    assert!(IsaProfile::Rv64imafdc.allows_compressed());
    assert_eq!(IsaProfile::Rv64imafd.ialign_bytes(), 4);
    assert_eq!(IsaProfile::Rv64imafdc.ialign_bytes(), 2);
}

#[test]
fn approved_profile_names_round_trip_and_alias_nothing() {
    for isa in [
        IsaProfile::Rv64ima,
        IsaProfile::Rv64imac,
        IsaProfile::Rv64imafd,
        IsaProfile::Rv64imafdc,
    ] {
        assert_eq!(IsaProfile::parse(isa.name()), Some(isa));
    }
    for rejected in [
        "rv64gc",
        "rv64imaf",
        "rv64imafdc_zicsr",
        "RV64IMAFD",
        "rv64im",
        "",
    ] {
        assert_eq!(IsaProfile::parse(rejected), None, "{rejected}");
    }
}

/// Guest storage offset of the tohost word the fixtures write.  Each guest
/// builds the address from an AUIPC at its own PC, so the following ADDI
/// carries `offset - auipc_offset`.
const TOHOST_VA_OFFSET: i32 = 0x100;
const EXIT_GUEST_TOHOST_DELTA: i32 = TOHOST_VA_OFFSET - 4;
const MISA_GUEST_TOHOST_DELTA: i32 = TOHOST_VA_OFFSET - 12;
const TOHOST_VA: u64 = fixture::BASE + TOHOST_VA_OFFSET as u64;

/// The guest reads `misa` and exits with its low 32 bits.
fn misa_guest() -> Vec<u8> {
    let code = vec![
        csrr(5, machine::MISA),
        fixture::slli(5, 5, 1),
        fixture::ori(5, 5, 1),
        fixture::auipc(4, 0),
        fixture::addi(4, 4, MISA_GUEST_TOHOST_DELTA),
        fixture::sd(5, 4, 0),
    ];
    fixture::elf_with_code(&code, 0, false, false, 0)
}

fn exit_code_guest() -> Vec<u8> {
    let code = vec![
        fixture::standard_exit(0),
        fixture::auipc(4, 0),
        fixture::addi(4, 4, EXIT_GUEST_TOHOST_DELTA),
        fixture::sd(5, 4, 0),
    ];
    fixture::elf_with_code(&code, 0, false, false, 0)
}

#[test]
fn cli_accepts_every_profile_and_reports_its_misa() {
    use assert_cmd::cargo::cargo_bin_cmd;

    let elf = misa_guest();
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("misa.elf");
    std::fs::write(&path, &elf).unwrap();

    for (name, misa) in [
        ("rv64ima", 0x0014_1101u32),
        ("rv64imac", 0x0014_1105),
        ("rv64imafd", 0x0014_1129),
        ("rv64imafdc", 0x0014_112D),
    ] {
        let output = cargo_bin_cmd!("ruscv-sim")
            .arg("run")
            .arg(&path)
            .arg("--isa")
            .arg(name)
            .arg("--tohost")
            .arg("0x80000100")
            .arg("--max-cycles")
            .arg("10000")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        // The guest exits with `misa[31:0]`, so the run reaches its tohost
        // exit only when the profile is installed before the first fetch.
        assert!(
            stdout.contains(&format!("Exit Code:  {misa}")),
            "profile {name} must report misa {misa:#x}: {stdout}"
        );
        assert!(
            !stdout.contains("Status:     TIMEOUT"),
            "profile {name} must retire the misa guest: {stdout}"
        );
    }

    // An unapproved name stays rejected at the CLI boundary.
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&path)
        .arg("--isa")
        .arg("rv64imaf")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid ISA profile"),
        "unapproved names are rejected: {stderr}"
    );
}

#[test]
fn native_and_flat_facades_accept_every_profile() {
    let elf = exit_code_guest();
    for isa in [
        IsaProfile::Rv64ima,
        IsaProfile::Rv64imac,
        IsaProfile::Rv64imafd,
        IsaProfile::Rv64imafdc,
    ] {
        let native =
            ruscv_sim::load_and_run_with_isa(&elf, Some(10_000), Some(TOHOST_VA), None, false, isa)
                .unwrap();
        assert_eq!(native.exit_code, 0, "native {}", isa.name());
        assert!(!native.timed_out, "native {}", isa.name());

        let mut simulator = RiscVSimulator::new_with_isa(0x1_0000, isa);
        simulator.load_elf(&elf).unwrap();
        simulator.set_tohost(TOHOST_VA_OFFSET as u64);
        let flat = simulator.run(Some(10_000)).unwrap();
        assert_eq!(flat.exit_code, 0, "flat {}", isa.name());
        assert!(!flat.timed_out, "flat {}", isa.name());
    }
}

#[test]
fn flat_facade_reports_each_profiles_misa() {
    for (isa, expected) in [
        (IsaProfile::Rv64ima, 0x8000_0000_0014_1101u64),
        (IsaProfile::Rv64imac, 0x8000_0000_0014_1105),
        (IsaProfile::Rv64imafd, 0x8000_0000_0014_1129),
        (IsaProfile::Rv64imafdc, 0x8000_0000_0014_112D),
    ] {
        let simulator = RiscVSimulator::new_with_isa(0x1_0000, isa);
        assert_eq!(
            simulator.state().csr.read(machine::MISA).unwrap(),
            expected,
            "{}",
            isa.name()
        );
    }
}

// ============== D5: FPR state ==============

#[test]
fn fpr_file_has_thirty_two_ordinary_64_bit_registers() {
    let mut file = FpuRegisterFile::new();
    for reg in 0..32usize {
        file.write(reg, Fpr::from_bits(0x4000_0000_0000_0000 | reg as u64));
    }
    for reg in 0..32usize {
        assert_eq!(
            file.read(reg).bits(),
            0x4000_0000_0000_0000 | reg as u64,
            "f{reg} must keep its 64-bit value"
        );
    }
    // `f0` is an ordinary register, not hard-wired (contract D5).
    file.write(0, Fpr::from_bits(0x7FF8_0000_0000_0001));
    assert_eq!(file.read(0).bits(), 0x7FF8_0000_0000_0001);
}

#[test]
fn thirty_two_bit_writes_nan_box_and_reads_distinguish_transfer_from_computation() {
    let mut file = FpuRegisterFile::new();
    file.write_u32(0, 0x4049_0FDB); // 3.1415927f32
    file.write_u32(31, 0x3F80_0000); // 1.0f32

    for (reg, single) in [(0usize, 0x4049_0FDBu32), (31, 0x3F80_0000)] {
        let register = file.read(reg);
        assert!(register.is_nan_boxed(), "f{reg} must be NaN-boxed");
        assert_eq!(register.bits() >> 32, NAN_BOX_MASK >> 32);
        assert_eq!(register.as_single_bits(), single);
        assert_eq!(file.read_single_bits(reg), single);
        assert_eq!(file.read_raw_u32(reg), single);
    }

    // An unboxed value (for example a 64-bit write, or a debug write of a
    // value the guest never boxed) yields the canonical NaN on a
    // computational single read, while the raw transfer read keeps the low
    // bits unchanged (contract D5).
    file.write(7, Fpr::from_bits(0x3FF0_0000_DEAD_BEEF));
    assert!(!file.read(7).is_nan_boxed());
    assert_eq!(file.read_single_bits(7), CANONICAL_NAN_F32);
    assert_eq!(file.read_raw_u32(7), 0xDEAD_BEEF);
    assert_eq!(file.read(7).get().to_bits(), CANONICAL_NAN_F32);

    // A correctly boxed NaN keeps its payload on a computational read.
    file.write(8, Fpr::from_bits(NAN_BOX_MASK | 0x7F80_0001));
    assert_eq!(file.read_single_bits(8), 0x7F80_0001);
}

#[test]
fn core_state_fpr_is_the_same_register_file() {
    let (mut core, _) = core_with(IsaProfile::Rv64imafd);
    let state = core.state_mut();
    state.fpr.write_u32(0, 0x3F80_0000);
    state.fpr.write(1, Fpr::from_bits(0x4000_0000_0000_0000));
    assert_eq!(core.state().fpr.read(0).get(), 1.0f32);
    assert_eq!(core.state().fpr.read(1).bits(), 0x4000_0000_0000_0000);
}

// ============== D5: fflags/frm/fcsr aliasing ==============

#[test]
fn fp_csrs_alias_one_fcsr_through_csr_instructions() {
    let (mut core, ram) = core_with(IsaProfile::Rv64imafd);
    // FS must be non-Off for any FP-CSR access (contract D6).
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_INITIAL);
    load(
        &ram,
        &[
            // fcsr = 0b1011_0101: frm = 5 (bits 7:5), fflags = 0b1_0101
            // (bits 4:0: NX, OF and NV).
            fixture::addi(5, 0, 0xB5),
            csrw(unprivileged::FCSR, 5),
            csrr(6, unprivileged::FFLAGS),
            csrr(7, unprivileged::FRM),
            csrr(8, unprivileged::FCSR),
        ],
    );
    for _ in 0..5 {
        retire(&mut core);
    }

    let state = core.state();
    assert_eq!(state.fcsr.read(), 0xB5);
    assert_eq!(state.fcsr.frm_bits(), 5);
    assert_eq!(state.fcsr.flags_bits(), 0b1_0101);
    assert_eq!(state.regs[6], 0b1_0101, "fflags is fcsr bits 4:0");
    assert_eq!(state.regs[7], 5, "frm is fcsr bits 7:5");
    assert_eq!(state.regs[8], 0xB5, "fcsr is fcsr bits 7:0");
    // The architectural flag order: NX bit 0 through NV bit 4.
    let flags = state.fcsr.flags();
    assert!(flags.contains(FpFlags::NX));
    assert!(flags.contains(FpFlags::OF));
    assert!(flags.contains(FpFlags::NV));
    assert!(!flags.contains(FpFlags::UF));
    assert!(!flags.contains(FpFlags::DZ));
    // A retired write to an FP CSR sets FS to Dirty (contract D6).
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        mstatus::FS_DIRTY
    );
}

#[test]
fn fp_csr_member_writes_change_only_their_own_bits() {
    let (mut core, ram) = core_with(IsaProfile::Rv64imafd);
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_CLEAN);
    load(
        &ram,
        &[
            fixture::addi(5, 0, 0xFF),
            csrw(unprivileged::FCSR, 5), // fcsr = 0xFF: frm = 7, fflags = 0x1F
            fixture::addi(5, 0, 2),
            csrw(unprivileged::FRM, 5), // only frm changes, to 2
            fixture::addi(5, 0, 1),
            csrw(unprivileged::FFLAGS, 5), // only fflags changes, to 1
            csrr(9, unprivileged::FCSR),
        ],
    );
    for _ in 0..7 {
        retire(&mut core);
    }
    let state = core.state();
    assert_eq!(state.fcsr.frm_bits(), 2);
    assert_eq!(state.fcsr.flags_bits(), 1);
    // frm = 2 in bits 7:5, fflags = 1 in bits 4:0.
    assert_eq!(state.regs[9], 2 << 5 | 1);
}

#[test]
fn fcsr_reserved_bits_read_zero_and_ignore_writes() {
    let (mut core, ram) = core_with(IsaProfile::Rv64imafd);
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_INITIAL);
    load(
        &ram,
        &[
            fixture::addi(5, 0, 0x7FF), // every low bit set
            csrw(unprivileged::FCSR, 5),
            csrr(6, unprivileged::FCSR),
            fixture::addi(5, 0, 0x3F),
            csrw(unprivileged::FFLAGS, 5), // bits above 4 are reserved
            csrr(7, unprivileged::FFLAGS),
            fixture::addi(5, 0, 0xB),
            csrw(unprivileged::FRM, 5), // bit 3 of frm is reserved
            csrr(8, unprivileged::FRM),
            csrr(9, unprivileged::FCSR),
        ],
    );
    for _ in 0..10 {
        retire(&mut core);
    }
    let state = core.state();
    assert_eq!(state.regs[6], u64::from(Fcsr::MASK));
    assert_eq!(state.regs[7], u64::from(Fcsr::FFLAGS_MASK));
    assert_eq!(state.regs[8], u64::from(0xBu8 & Fcsr::FRM_MASK));
    // frm = 3 in bits 7:5, the five flags in bits 4:0.
    assert_eq!(state.regs[9], 3 << 5 | u64::from(Fcsr::FFLAGS_MASK));
    assert_eq!(
        u64::from(state.fcsr.read()),
        3 << 5 | u64::from(Fcsr::FFLAGS_MASK)
    );
}

// ============== D6: mstatus.FS and SD ==============

#[test]
fn fp_profiles_reset_fs_off_with_sd_clear() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        let (core, _) = core_with(isa);
        let status = read_csr(&core, machine::MSTATUS);
        assert_eq!(status & mstatus::FS, mstatus::FS_OFF, "{}", isa.name());
        assert_eq!(status & mstatus::SD, 0, "{}", isa.name());
        let sstatus = read_csr(&core, supervisor::SSTATUS);
        assert_eq!(sstatus & mstatus::FS, 0, "{}", isa.name());
        assert_eq!(sstatus & mstatus::SD, 0, "{}", isa.name());
    }
}

#[test]
fn fs_is_writable_through_mstatus_and_its_sstatus_view() {
    let (mut core, _) = core_with(IsaProfile::Rv64imafd);

    // mstatus writes reach FS, and SD follows FS == Dirty.
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_DIRTY);
    let status = read_csr(&core, machine::MSTATUS);
    assert_eq!(status & mstatus::FS, mstatus::FS_DIRTY);
    assert_eq!(status & mstatus::SD, mstatus::SD);
    let sstatus = read_csr(&core, supervisor::SSTATUS);
    assert_eq!(sstatus & mstatus::FS, mstatus::FS_DIRTY);
    assert_eq!(sstatus & mstatus::SD, mstatus::SD);

    // Every FS encoding is writable, and only Dirty sets SD.
    for (fs, sd) in [
        (mstatus::FS_INITIAL, 0),
        (mstatus::FS_CLEAN, 0),
        (mstatus::FS_OFF, 0),
        (mstatus::FS_DIRTY, mstatus::SD),
    ] {
        write_csr(&mut core, machine::MSTATUS, fs);
        let status = read_csr(&core, machine::MSTATUS);
        assert_eq!(status & mstatus::FS, fs, "FS {fs:#x}");
        assert_eq!(status & mstatus::SD, sd, "FS {fs:#x}");
    }

    // The sstatus view writes FS too (contract D6).
    write_csr(&mut core, supervisor::SSTATUS, mstatus::FS_CLEAN);
    let status = read_csr(&core, machine::MSTATUS);
    assert_eq!(status & mstatus::FS, mstatus::FS_CLEAN);
    assert_eq!(status & mstatus::SD, 0);

    // SD itself is read-only: a write of bit 63 does not stick.
    write_csr(&mut core, machine::MSTATUS, mstatus::SD);
    let status = read_csr(&core, machine::MSTATUS);
    assert_eq!(status & mstatus::FS, mstatus::FS_OFF);
    assert_eq!(status & mstatus::SD, 0);
}

#[test]
fn non_fp_profiles_keep_fs_and_sd_hardwired_zero() {
    for isa in [IsaProfile::Rv64ima, IsaProfile::Rv64imac] {
        let (mut core, _) = core_with(isa);
        write_csr(&mut core, machine::MSTATUS, mstatus::FS_DIRTY | mstatus::SD);
        let status = read_csr(&core, machine::MSTATUS);
        assert_eq!(status & mstatus::FS, 0, "{}", isa.name());
        assert_eq!(status & mstatus::SD, 0, "{}", isa.name());

        write_csr(&mut core, supervisor::SSTATUS, mstatus::FS_DIRTY);
        let status = read_csr(&core, machine::MSTATUS);
        assert_eq!(status & mstatus::FS, 0, "{}", isa.name());
        assert_eq!(status & mstatus::SD, 0, "{}", isa.name());
    }
}

// ============== D6: FS Off and the FS-Dirty write rule ==============

#[test]
fn fs_off_makes_every_fp_csr_access_illegal() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        for addr in [unprivileged::FFLAGS, unprivileged::FRM, unprivileged::FCSR] {
            for instruction in [
                csr_op(CSRRS, 1, addr, 0),  // read
                csr_op(CSRRW, 1, addr, 0),  // write from x0
                csr_op(CSRRSI, 0, addr, 1), // immediate write
            ] {
                expect_fp_csr_illegal(isa, instruction, mstatus::FS_OFF);
            }
        }
    }
}

#[test]
fn fp_csrs_are_accessible_when_fs_is_not_off() {
    for fs in [mstatus::FS_INITIAL, mstatus::FS_CLEAN, mstatus::FS_DIRTY] {
        for addr in [unprivileged::FFLAGS, unprivileged::FRM, unprivileged::FCSR] {
            let core = fp_csr_instruction(IsaProfile::Rv64imafd, fs, csrr(1, addr));
            assert_eq!(core.state().regs[1], 0, "CSR {addr:#x} with FS {fs:#x}");
        }
    }
}

#[test]
fn fp_csr_write_classification_sets_fs_dirty_only_on_a_write() {
    // (funct3, rs1/zimm, expected FS after the instruction)
    let cases = [
        (CSRRW, 0u8, mstatus::FS_DIRTY), // CSRRW always writes
        (CSRRS, 0, mstatus::FS_CLEAN),   // CSRRS with rs1=x0 only reads
        (CSRRS, 1, mstatus::FS_DIRTY),   // CSRRS with rs1!=x0 writes
        (CSRRC, 0, mstatus::FS_CLEAN),   // CSRRC with rs1=x0 only reads
        (CSRRC, 1, mstatus::FS_DIRTY),   // CSRRC with rs1!=x0 writes
        (CSRRWI, 0, mstatus::FS_DIRTY),  // CSRRWI always writes
        (CSRRSI, 0, mstatus::FS_CLEAN),  // zero immediate does not write
        (CSRRSI, 1, mstatus::FS_DIRTY),  // non-zero immediate writes
        (CSRRCI, 0, mstatus::FS_CLEAN),  // zero immediate does not write
        (CSRRCI, 1, mstatus::FS_DIRTY),  // non-zero immediate writes
    ];

    for (funct3, source, expected_fs) in cases {
        let instruction = csr_op(funct3, 1, unprivileged::FFLAGS, source);
        let core = fp_csr_instruction(IsaProfile::Rv64imafd, mstatus::FS_CLEAN, instruction);
        assert_eq!(
            read_csr(&core, machine::MSTATUS) & mstatus::FS,
            expected_fs,
            "funct3 {funct3:#05b} source {source} must leave FS {expected_fs:#x}"
        );
    }

    // CSRRW with rs1=x0 still writes: the fcsr value becomes zero and FS is
    // Dirty even though no flag changed.
    let (mut core, ram) = core_with(IsaProfile::Rv64imafd);
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_CLEAN);
    core.state_mut().fcsr.write(0xE5);
    load(&ram, &[csr_op(CSRRW, 0, unprivileged::FCSR, 0)]);
    retire(&mut core);
    assert_eq!(core.state().fcsr.read(), 0);
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        mstatus::FS_DIRTY
    );
}

#[test]
fn non_fp_profiles_keep_the_fp_csrs_illegal() {
    for isa in [IsaProfile::Rv64ima, IsaProfile::Rv64imac] {
        for addr in [unprivileged::FFLAGS, unprivileged::FRM, unprivileged::FCSR] {
            for instruction in [csr_op(CSRRS, 1, addr, 0), csr_op(CSRRW, 1, addr, 0)] {
                expect_fp_csr_illegal(isa, instruction, mstatus::FS_OFF);
            }
        }
        // The addresses are not merely FS-gated, they do not exist.
        let (core, _) = core_with(isa);
        for addr in [unprivileged::FFLAGS, unprivileged::FRM, unprivileged::FCSR] {
            assert_eq!(
                core.state().csr.read(addr),
                Err(CsrError::InvalidAddress(addr)),
                "{} {addr:#x}",
                isa.name()
            );
        }
        assert!(!csr::is_fp_csr(machine::MSTATUS));
        assert!(csr::is_fp_csr(unprivileged::FCSR));
    }
}

// ============== §3: reset restores the FP state ==============

#[test]
fn reset_restores_fprs_fcsr_and_fs() {
    let (mut core, _) = core_with(IsaProfile::Rv64imafd);
    {
        let state = core.state_mut();
        state.fpr.write(0, Fpr::from_bits(0x4009_21FB_5444_2D18));
        state.fpr.write(31, Fpr::from_bits(0x3FF0_0000_0000_0000));
        state.fpr.write_u32(4, 0x4049_0FDB);
        state.fcsr.write(0xE5);
        state
            .csr
            .write(machine::MSTATUS, mstatus::FS_DIRTY)
            .unwrap();
    }
    // Confirm the setup took effect before proving reset clears it.
    assert_eq!(core.state().fcsr.read(), 0xE5);
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        mstatus::FS_DIRTY
    );

    core.reset(0, 0);

    let state = core.state();
    for reg in 0..32 {
        assert_eq!(state.fpr.read(reg).bits(), 0, "f{reg} after reset");
    }
    assert_eq!(state.fcsr.read(), 0);
    assert_eq!(state.fcsr.frm_bits(), 0);
    assert_eq!(state.fcsr.flags_bits(), 0);
    let status = read_csr(&core, machine::MSTATUS);
    assert_eq!(status & mstatus::FS, mstatus::FS_OFF);
    assert_eq!(status & mstatus::SD, 0);
    // Reset never changes the selected profile.
    assert_eq!(misa(&core), 0x8000_0000_0014_1129);
}

#[test]
fn reset_restores_fp_state_under_the_compressed_fp_profile() {
    let (mut core, _) = core_with(IsaProfile::Rv64imafdc);
    core.state_mut().fpr.write_u32(9, 0x3F80_0000);
    core.state_mut().fcsr.write(0x1F);
    write_csr(&mut core, machine::MSTATUS, mstatus::FS_DIRTY);

    core.reset(0, 0);

    assert_eq!(core.state().fpr.read(9).bits(), 0);
    assert_eq!(core.state().fcsr.read(), 0);
    assert_eq!(
        read_csr(&core, machine::MSTATUS) & mstatus::FS,
        mstatus::FS_OFF
    );
    assert_eq!(misa(&core), 0x8000_0000_0014_112D);
}
