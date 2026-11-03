//! F/D U3a: FP decode and execution on the public path (issue #147).
//!
//! Covers contract U3a and decisions D4-D7: every F/D instruction executes
//! through the single Hart engine and the U2 arithmetic core, with explicit
//! `fmt`/`rm` decode; static and dynamic rounding plus reserved-`rm` traps;
//! the D6 `mstatus.FS` rule; D4/D5 NaN-boxing and payload preservation; FP
//! loads and stores through the physical data port, including misaligned,
//! Sv39 and access faults; malformed encodings trapping instead of a
//! `SimulatorFailure`; and commit facts carrying FPR and `fcsr` effects.
//!
//! The default `rv64ima` and `rv64imac` profiles are proven unchanged by the
//! existing suites; the controls below pin that every FP encoding still
//! traps there and that no FP state changes.

use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::{
    ExceptionCause, PrivilegeMode, RiscvCore, SimulatorFailureKind, StepOutcome, Trap,
};
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::fpu::{CANONICAL_NAN_F32, NAN_BOX_MASK};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AccessCategory, AccessWidth, AtomicBackend, AtomicBackendResult, AtomicRequest,
    NativeRamBackend, PhysicalBackend, PhysicalBackendResult, PhysicalRequest, PhysicalWidth,
    ValidatedPhysicalAccess,
};
use ruscv_sim::IsaProfile;
use std::sync::{Arc, Mutex};

const RAM_SIZE: usize = 0x10_0000;
/// Data area used by the FP load/store instructions in the retirement sweep.
const DATA: u64 = 0x200;

// ============== Instruction encoders ==============

/// OP-FP: `funct7 | rs2 | rs1 | rm/selector | rd | 1010011`.
fn op_fp(funct7: u32, rs2: u32, rs1: u32, rm_or_selector: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (rm_or_selector << 12) | (rd << 7) | 0b101_0011
}

/// R4: `rs3 | fmt | rs2 | rs1 | rm | rd | opcode`.
fn r4(opcode: u32, rs3: u32, fmt: u32, rs2: u32, rs1: u32, rm: u32, rd: u32) -> u32 {
    (rs3 << 27) | (fmt << 25) | (rs2 << 20) | (rs1 << 15) | (rm << 12) | (rd << 7) | opcode
}

fn flw(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0b000_0111
}

fn fld(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (0b011 << 12) | (rd << 7) | 0b000_0111
}

fn fsw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    ((((imm as u32) >> 5) & 0x7f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0b010 << 12)
        | (((imm as u32) & 0x1f) << 7)
        | 0b010_0111
}

fn fsd(rs2: u32, rs1: u32, imm: i32) -> u32 {
    ((((imm as u32) >> 5) & 0x7f) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (0b011 << 12)
        | (((imm as u32) & 0x1f) << 7)
        | 0b010_0111
}

/// Every F/D instruction the profile implements, one encoding per line.
fn every_f_and_d_instruction() -> Vec<(&'static str, u32)> {
    let mut program = Vec::new();
    // Single-precision arithmetic, sign injection, min/max and compare.
    program.extend([
        ("fadd.s", op_fp(0x00, 2, 1, 0, 3)),
        ("fsub.s", op_fp(0x04, 2, 1, 0, 3)),
        ("fmul.s", op_fp(0x08, 2, 1, 0, 3)),
        ("fdiv.s", op_fp(0x0c, 2, 1, 0, 3)),
        ("fsqrt.s", op_fp(0x2c, 0, 1, 0, 3)),
        ("fsgnj.s", op_fp(0x10, 2, 1, 0, 3)),
        ("fsgnjn.s", op_fp(0x10, 2, 1, 1, 3)),
        ("fsgnjx.s", op_fp(0x10, 2, 1, 2, 3)),
        ("fmin.s", op_fp(0x14, 2, 1, 0, 3)),
        ("fmax.s", op_fp(0x14, 2, 1, 1, 3)),
        ("feq.s", op_fp(0x50, 2, 1, 2, 4)),
        ("flt.s", op_fp(0x50, 2, 1, 1, 4)),
        ("fle.s", op_fp(0x50, 2, 1, 0, 4)),
        ("fclass.s", op_fp(0x70, 0, 1, 1, 4)),
        ("fcvt.w.s", op_fp(0x60, 0, 1, 0, 4)),
        ("fcvt.wu.s", op_fp(0x60, 1, 1, 0, 4)),
        ("fcvt.l.s", op_fp(0x60, 2, 1, 0, 4)),
        ("fcvt.lu.s", op_fp(0x60, 3, 1, 0, 4)),
        ("fcvt.s.w", op_fp(0x68, 0, 1, 0, 3)),
        ("fcvt.s.wu", op_fp(0x68, 1, 1, 0, 3)),
        ("fcvt.s.l", op_fp(0x68, 2, 1, 0, 3)),
        ("fcvt.s.lu", op_fp(0x68, 3, 1, 0, 3)),
        ("fmv.x.w", op_fp(0x70, 0, 1, 0, 4)),
        ("fmv.w.x", op_fp(0x78, 0, 1, 0, 3)),
    ]);
    // Double-precision arithmetic, sign injection, min/max and compare.
    program.extend([
        ("fadd.d", op_fp(0x01, 2, 1, 0, 3)),
        ("fsub.d", op_fp(0x05, 2, 1, 0, 3)),
        ("fmul.d", op_fp(0x09, 2, 1, 0, 3)),
        ("fdiv.d", op_fp(0x0d, 2, 1, 0, 3)),
        ("fsqrt.d", op_fp(0x2d, 0, 1, 0, 3)),
        ("fsgnj.d", op_fp(0x11, 2, 1, 0, 3)),
        ("fsgnjn.d", op_fp(0x11, 2, 1, 1, 3)),
        ("fsgnjx.d", op_fp(0x11, 2, 1, 2, 3)),
        ("fmin.d", op_fp(0x15, 2, 1, 0, 3)),
        ("fmax.d", op_fp(0x15, 2, 1, 1, 3)),
        ("feq.d", op_fp(0x51, 2, 1, 2, 4)),
        ("flt.d", op_fp(0x51, 2, 1, 1, 4)),
        ("fle.d", op_fp(0x51, 2, 1, 0, 4)),
        ("fclass.d", op_fp(0x71, 0, 1, 1, 4)),
        ("fcvt.w.d", op_fp(0x61, 0, 1, 0, 4)),
        ("fcvt.wu.d", op_fp(0x61, 1, 1, 0, 4)),
        ("fcvt.l.d", op_fp(0x61, 2, 1, 0, 4)),
        ("fcvt.lu.d", op_fp(0x61, 3, 1, 0, 4)),
        ("fcvt.d.w", op_fp(0x69, 0, 1, 0, 3)),
        ("fcvt.d.wu", op_fp(0x69, 1, 1, 0, 3)),
        ("fcvt.d.l", op_fp(0x69, 2, 1, 0, 3)),
        ("fcvt.d.lu", op_fp(0x69, 3, 1, 0, 3)),
        ("fmv.x.d", op_fp(0x71, 0, 1, 0, 4)),
        ("fmv.d.x", op_fp(0x79, 0, 1, 0, 3)),
    ]);
    // Float-to-float conversions.
    program.extend([
        ("fcvt.s.d", op_fp(0x20, 1, 1, 0, 3)),
        ("fcvt.d.s", op_fp(0x21, 0, 1, 0, 3)),
    ]);
    // R4 fused multiply-add, both formats and all four sign variants.
    for (single, double, opcode) in [
        ("fmadd.s", "fmadd.d", 0b100_0011u32),
        ("fmsub.s", "fmsub.d", 0b100_0111),
        ("fnmsub.s", "fnmsub.d", 0b100_1011),
        ("fnmadd.s", "fnmadd.d", 0b100_1111),
    ] {
        program.push((single, r4(opcode, 3, 0, 2, 1, 0, 5)));
        program.push((double, r4(opcode, 3, 1, 2, 1, 0, 5)));
    }
    // Loads and stores.
    program.extend([
        ("flw", flw(3, 5, 0)),
        ("fld", fld(3, 5, 8)),
        ("fsw", fsw(2, 5, 16)),
        ("fsd", fsd(2, 5, 24)),
    ]);
    program
}

// ============== Hart fixtures ==============

fn ported_core(isa: IsaProfile) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    ported_core_at(isa, 0)
}

fn ported_core_at(isa: IsaProfile, base: u64) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.set_isa_profile(isa);
    core.reset(0, base);
    (core, ram)
}

fn write_word(ram: &Arc<Mutex<SimpleMemory>>, address: u64, word: u32) {
    ram.lock().unwrap().write_word(address, word).unwrap();
}

fn read_word(ram: &Arc<Mutex<SimpleMemory>>, address: u64) -> u32 {
    ram.lock().unwrap().read_word(address).unwrap()
}

fn read_dword(ram: &Arc<Mutex<SimpleMemory>>, address: u64) -> u64 {
    ram.lock().unwrap().read_dword(address).unwrap()
}

fn set_fs(core: &mut RiscvCore, fs: u64) {
    core.state_mut()
        .csr
        .write(machine::MSTATUS, fs)
        .expect("mstatus is writable in M mode");
}

fn fs_of(core: &RiscvCore) -> u64 {
    // `mstatus` is a machine CSR; read it through the M view even when the
    // hart currently executes in S mode.
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(machine::MSTATUS)
        .expect("mstatus is readable in M mode")
        & mstatus::FS
}

fn fpr_bits(core: &RiscvCore, reg: usize) -> u64 {
    core.state().fpr.read(reg).bits()
}

fn boxed_single(bits: u32) -> u64 {
    NAN_BOX_MASK | u64::from(bits)
}

fn retire(core: &mut RiscvCore) -> ruscv_sim::core::InstructionRetired {
    match core.step_outcome() {
        StepOutcome::InstructionRetired(retired) => retired,
        other => panic!("expected retirement, got {other:?}"),
    }
}

fn trap(core: &mut RiscvCore) -> ruscv_sim::core::TrapEntered {
    match core.step_outcome() {
        StepOutcome::TrapEntered(entered) => entered,
        other => panic!("expected a trap, got {other:?}"),
    }
}

/// Step one instruction and require illegal-instruction with the encoding in
/// `mtval`, never a simulator failure.
fn expect_illegal(core: &mut RiscvCore, raw: u32) {
    match core.step_outcome() {
        StepOutcome::TrapEntered(entered) => {
            assert_eq!(
                entered.cause,
                Trap::Exception(ExceptionCause::IllegalInstruction),
                "{raw:#010x}"
            );
            assert_eq!(entered.mtval, u64::from(raw), "{raw:#010x} mtval");
        }
        StepOutcome::SimulatorFailure(failure) => {
            panic!("{raw:#010x} became a simulator failure: {failure:?}")
        }
        other => panic!("{raw:#010x} expected illegal-instruction, got {other:?}"),
    }
}

/// Snapshot of the FP architectural state a trapping instruction must not
/// change.
fn fp_state(core: &RiscvCore) -> (Vec<u64>, u32, u64) {
    (
        (0..32).map(|reg| fpr_bits(core, reg)).collect(),
        core.state().fcsr.read(),
        fs_of(core),
    )
}

// ============== Every form retires through the single engine ==============

#[test]
fn every_f_and_d_instruction_retires_under_every_fp_profile() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        every_f_and_d_instruction_retires(isa);
    }
}

fn every_f_and_d_instruction_retires(isa: IsaProfile) {
    let (mut core, ram) = ported_core(isa);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // Boxed single 1.0, double 1.0, and integer operands.
    core.state_mut()
        .fpr
        .write(1, ruscv_sim::Fpr::from_bits(boxed_single(0x3f80_0000)));
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(1.0f64.to_bits()));
    core.state_mut()
        .fpr
        .write(3, ruscv_sim::Fpr::from_bits(2.0f64.to_bits()));
    core.state_mut().regs[1] = 1;
    core.state_mut().regs[5] = DATA;
    ram.lock().unwrap().write_word(DATA, 0x3f80_0000).unwrap();
    ram.lock()
        .unwrap()
        .write_dword(DATA + 8, 0x3ff0_0000_0000_0000)
        .unwrap();

    let program = every_f_and_d_instruction();
    assert!(
        program.len() >= 60,
        "the sweep must cover every F/D form, got {}",
        program.len()
    );
    for (name, raw) in program {
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        match core.step_outcome() {
            StepOutcome::InstructionRetired(retired) => {
                assert_eq!(retired.pc, 0, "{name}");
                assert_eq!(core.state().pc, 4, "{name}");
            }
            StepOutcome::SimulatorFailure(failure) => {
                panic!("{name} ({raw:#010x}) became a simulator failure: {failure:?}")
            }
            other => panic!("{name} ({raw:#010x}) did not retire: {other:?}"),
        }
    }
    // The sweep wrote FPRs and accrued flags, so D6 made FS Dirty.
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY, "{}", isa.name());
}

#[test]
fn r4_forms_execute_their_fused_arithmetic() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // f1 = 2.0, f2 = 3.0, f3 = 5.0 -> 2*3+5 = 11 for FMADD.D.
    core.state_mut()
        .fpr
        .write(1, ruscv_sim::Fpr::from_bits(2.0f64.to_bits()));
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(3.0f64.to_bits()));
    core.state_mut()
        .fpr
        .write(3, ruscv_sim::Fpr::from_bits(5.0f64.to_bits()));
    for (raw, expected) in [
        (r4(0b100_0011, 3, 1, 2, 1, 0, 5), 11.0f64),  // FMADD.D
        (r4(0b100_0111, 3, 1, 2, 1, 0, 5), 1.0f64),   // FMSUB.D
        (r4(0b100_1011, 3, 1, 2, 1, 0, 5), -1.0f64),  // FNMSUB.D
        (r4(0b100_1111, 3, 1, 2, 1, 0, 5), -11.0f64), // FNMADD.D
    ] {
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fpr_bits(&core, 5), expected.to_bits(), "{raw:#010x}");
    }
}

// ============== Static, dynamic and reserved rounding ==============

#[test]
fn static_rounding_modes_produce_their_own_results() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 0x3380_0000); // 2^-24, the exact tie

    for (rm, expected) in [
        (0, 0x3f80_0000u32), // RNE: ties to even keeps 1.0
        (1, 0x3f80_0000),    // RTZ
        (2, 0x3f80_0000),    // RDN
        (3, 0x3f80_0001),    // RUP: up to the next f32
        (4, 0x3f80_0001),    // RMM: ties to max magnitude
    ] {
        core.state_mut().fcsr.clear_flags();
        write_word(&ram, 0, op_fp(0x00, 2, 1, rm, 3));
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fpr_bits(&core, 3) as u32, expected, "rm {rm}");
        assert_eq!(core.state().fcsr.flags_bits(), 0b1, "rm {rm} must raise NX");
        assert_eq!(fs_of(&core), mstatus::FS_DIRTY, "rm {rm}");
    }
}

#[test]
fn dynamic_rounding_reads_frm() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 0x3380_0000);
    for (frm, expected) in [(1u8, 0x3f80_0000u32), (3, 0x3f80_0001), (4, 0x3f80_0001)] {
        core.state_mut().fcsr.write_frm_bits(frm);
        core.state_mut().fcsr.clear_flags();
        write_word(&ram, 0, op_fp(0x00, 2, 1, 7, 3)); // dynamic rm
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fpr_bits(&core, 3) as u32, expected, "frm {frm}");
    }
}

#[test]
fn static_reserved_rounding_modes_are_illegal() {
    for rm in [5u32, 6] {
        for raw in [
            op_fp(0x00, 2, 1, rm, 3),          // FADD.S
            op_fp(0x01, 2, 1, rm, 3),          // FADD.D
            op_fp(0x60, 0, 1, rm, 4),          // FCVT.W.S
            op_fp(0x21, 0, 1, rm, 3),          // FCVT.D.S
            r4(0b100_0011, 3, 0, 2, 1, rm, 5), // FMADD.S
        ] {
            let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
            set_fs(&mut core, mstatus::FS_CLEAN);
            core.state_mut().fcsr.write(0b1010_0001); // frm 5, NX set
            core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
            core.state_mut().fpr.write_u32(2, 2.0f32.to_bits());
            let before = fp_state(&core);
            write_word(&ram, 0, raw);
            core.state_mut().pc = 0;
            expect_illegal(&mut core, raw);
            assert_eq!(fp_state(&core), before, "rm {rm} {raw:#010x}");
        }
    }
}

#[test]
fn dynamic_reserved_rounding_modes_are_illegal_but_legal_frm_values_work() {
    for frm in [5u8, 6, 7] {
        for raw in [
            op_fp(0x00, 2, 1, 7, 3), // FADD.S
            op_fp(0x61, 0, 1, 7, 4), // FCVT.W.D
        ] {
            let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
            set_fs(&mut core, mstatus::FS_CLEAN);
            core.state_mut().fcsr.write_frm_bits(frm);
            let before = fp_state(&core);
            write_word(&ram, 0, raw);
            core.state_mut().pc = 0;
            expect_illegal(&mut core, raw);
            assert_eq!(fp_state(&core), before, "frm {frm} {raw:#010x}");
        }
    }
    // Every implemented dynamic mode executes; RTZ and RUP differ on the tie.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 0x3380_0000);
    for frm in 0u8..=4 {
        core.state_mut().fcsr.write_frm_bits(frm);
        write_word(&ram, 0, op_fp(0x00, 2, 1, 7, 3));
        core.state_mut().pc = 0;
        retire(&mut core);
    }
}

// ============== D6: FS Off and the FS-Dirty rule ==============

#[test]
fn fs_off_makes_every_fp_encoding_illegal_without_side_effects() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        let (mut core, ram) = ported_core(isa);
        set_fs(&mut core, mstatus::FS_OFF);
        core.state_mut().fpr.write_u32(1, 0x3f80_0000);
        core.state_mut().fpr.write_u32(2, 0x4000_0000);
        core.state_mut().regs[5] = DATA;
        write_word(&ram, DATA, 0x1234_5678);
        for (_, raw) in every_f_and_d_instruction() {
            set_fs(&mut core, mstatus::FS_OFF);
            let before = fp_state(&core);
            let memory_before = read_word(&ram, DATA);
            write_word(&ram, 0, raw);
            core.state_mut().pc = 0;
            expect_illegal(&mut core, raw);
            assert_eq!(fp_state(&core), before, "{} {raw:#010x}", isa.name());
            assert_eq!(
                read_word(&ram, DATA),
                memory_before,
                "{} {raw:#010x} store side effect",
                isa.name()
            );
        }
    }
}

#[test]
fn non_fp_profiles_keep_every_fp_encoding_illegal() {
    for isa in [IsaProfile::Rv64ima, IsaProfile::Rv64imac] {
        let (mut core, ram) = ported_core(isa);
        core.state_mut().fpr.write_u32(1, 0x3f80_0000);
        core.state_mut().regs[5] = DATA;
        for (_, raw) in every_f_and_d_instruction() {
            let before = fp_state(&core);
            write_word(&ram, 0, raw);
            core.state_mut().pc = 0;
            expect_illegal(&mut core, raw);
            assert_eq!(fp_state(&core), before, "{} {raw:#010x}", isa.name());
        }
        // FS and SD stay hard-wired to zero (contract D6).
        set_fs(&mut core, mstatus::FS_DIRTY | mstatus::SD);
        assert_eq!(fs_of(&core), 0, "{}", isa.name());
    }
}

#[test]
fn an_fpr_write_sets_fs_dirty_even_without_flags() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 1.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x00, 2, 1, 0, 3)); // 1.0 + 1.0 is exact
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().fcsr.flags_bits(), 0);
    assert_eq!(
        fs_of(&core),
        mstatus::FS_DIRTY,
        "an FPR write sets FS Dirty"
    );
}

#[test]
fn fsw_fsd_and_fmv_x_leave_fs_unchanged() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    core.state_mut().fpr.write_u32(2, 0x3f80_0000);
    core.state_mut().regs[5] = DATA;
    for raw in [
        fsw(2, 5, 0),
        fsd(2, 5, 8),
        op_fp(0x70, 0, 2, 0, 4), // FMV.X.W
        op_fp(0x71, 0, 2, 0, 4), // FMV.X.D
    ] {
        set_fs(&mut core, mstatus::FS_CLEAN);
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fs_of(&core), mstatus::FS_CLEAN, "{raw:#010x}");
        assert_eq!(core.state().fcsr.read(), 0, "{raw:#010x}");
    }
}

#[test]
fn fpr_writing_moves_set_fs_dirty() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    core.state_mut().regs[1] = 0x3f80_0000;
    for raw in [
        op_fp(0x78, 0, 1, 0, 3), // FMV.W.X
        op_fp(0x79, 0, 1, 0, 3), // FMV.D.X
        flw(3, 5, 0),            // FLW
        fld(3, 5, 0),            // FLD
    ] {
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut().regs[5] = DATA;
        write_word(&ram, DATA, 0x3f80_0000);
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fs_of(&core), mstatus::FS_DIRTY, "{raw:#010x}");
    }
}

#[test]
fn gpr_only_fp_instructions_follow_the_exception_rule() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 2.0f32.to_bits());
    // FCLASS and a quiet-NaN FEQ raise nothing: FS stays Clean and no flag
    // is set. FLT/FLE of the same quiet NaN raise NV and Dirty FS.
    for (raw, expected_fs, expected_flags) in [
        (op_fp(0x70, 0, 1, 1, 4), mstatus::FS_CLEAN, 0u8), // FCLASS.S
        (op_fp(0x50, 1, 1, 2, 4), mstatus::FS_CLEAN, 0),   // FEQ.S x,x
    ] {
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut().fcsr.clear_flags();
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fs_of(&core), expected_fs, "{raw:#010x}");
        assert_eq!(
            core.state().fcsr.flags_bits(),
            expected_flags,
            "{raw:#010x}"
        );
    }

    // A quiet-NaN FLT.S raises NV (signaling compare) and therefore Dirty.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, CANONICAL_NAN_F32);
    core.state_mut().fpr.write_u32(2, 1.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x50, 2, 1, 1, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().fcsr.flags_bits(), 0x10);
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY);

    // A quiet-NaN FEQ.S is quiet: no flag and no FS transition.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, CANONICAL_NAN_F32);
    core.state_mut().fpr.write_u32(2, 1.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x50, 2, 1, 2, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().fcsr.flags_bits(), 0);
    assert_eq!(fs_of(&core), mstatus::FS_CLEAN);

    // An exact FCVT.W.S writes only the GPR: FS stays Clean.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 2.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x60, 0, 1, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().regs[4], 2);
    assert_eq!(core.state().fcsr.flags_bits(), 0);
    assert_eq!(fs_of(&core), mstatus::FS_CLEAN);

    // An inexact FCVT.W.S accrues NX and marks FS Dirty.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.5f32.to_bits());
    write_word(&ram, 0, op_fp(0x60, 0, 1, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().fcsr.flags_bits(), 0b1);
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY);
}

#[test]
fn flag_accrual_dirties_fs_even_when_the_flag_was_already_set() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // NX is already set, so the 1.5 -> 1 conversion changes no fcsr bit.
    core.state_mut().fcsr.write_flags_bits(0b1);
    core.state_mut().fpr.write_u32(1, 1.5f32.to_bits());
    write_word(&ram, 0, op_fp(0x60, 0, 1, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().fcsr.flags_bits(), 0b1);
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY);
}

// ============== D4/D5: NaN boxing and payload preservation ==============

#[test]
fn flw_nan_boxes_and_fsw_uses_the_raw_low_word() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[5] = DATA;
    // A signaling NaN with a payload survives the memory transfer and is
    // NaN-boxed on the FLW write (contract D5).
    write_word(&ram, DATA, 0x7f80_0001);
    write_word(&ram, 0, flw(3, 5, 0));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), boxed_single(0x7f80_0001));

    // FSW moves the raw low 32 bits of an unboxed register. Seed f4 with a
    // double bit pattern whose low word is zero.
    core.state_mut()
        .fpr
        .write(4, ruscv_sim::Fpr::from_bits(0x3ff0_0000_0000_0000));
    write_word(&ram, 0, fsw(4, 5, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(read_word(&ram, DATA + 4), 0);
    // The register itself is unchanged by the transfer.
    assert_eq!(fpr_bits(&core, 4), 0x3ff0_0000_0000_0000);
}

#[test]
fn fmv_x_w_reads_raw_bits_and_sign_extends() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // An unboxed FPR: FMV.X.W must return the low word, not the canonical
    // NaN the computational read would produce.
    core.state_mut()
        .fpr
        .write(1, ruscv_sim::Fpr::from_bits(0x0000_0000_8000_0000));
    write_word(&ram, 0, op_fp(0x70, 0, 1, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().regs[4], 0xffff_ffff_8000_0000);
    assert_eq!(fs_of(&core), mstatus::FS_CLEAN);

    // A boxed signaling NaN keeps its payload through the transfer.
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(boxed_single(0x7f80_0001)));
    write_word(&ram, 0, op_fp(0x70, 0, 2, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().regs[4], 0x7f80_0001);

    // The reverse transfer re-boxes the payload bit for bit.
    core.state_mut().regs[1] = 0x7f80_0001;
    write_word(&ram, 0, op_fp(0x78, 0, 1, 0, 3));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), boxed_single(0x7f80_0001));
}

#[test]
fn unboxed_single_reads_yield_the_canonical_nan() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // f1 holds a valid double 1.0, not a NaN box.
    core.state_mut()
        .fpr
        .write(1, ruscv_sim::Fpr::from_bits(1.0f64.to_bits()));
    core.state_mut().fpr.write_u32(2, 1.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x00, 2, 1, 0, 3)); // FADD.S
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(
        fpr_bits(&core, 3),
        boxed_single(CANONICAL_NAN_F32),
        "an unboxed FPR must read as the canonical NaN"
    );

    // FSGNJ.S preserves the payload of the correctly boxed operand but the
    // unboxed operand still reads as the canonical NaN.
    core.state_mut()
        .fpr
        .write(4, ruscv_sim::Fpr::from_bits(boxed_single(0x7f80_0001)));
    write_word(&ram, 0, op_fp(0x10, 2, 1, 0, 5)); // FSGNJ.S f5, f1, f2
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 5), boxed_single(CANONICAL_NAN_F32));

    // A correctly boxed payload is preserved exactly (sign from f2 = +).
    write_word(&ram, 0, op_fp(0x10, 2, 4, 0, 6)); // FSGNJ.S f6, f4, f2
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 6), boxed_single(0x7f80_0001));
}

#[test]
fn arithmetic_nan_results_are_canonical_and_conversions_follow_d4() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // 0.0 / 0.0 is the canonical single NaN, NaN-boxed, with NV.
    core.state_mut().fpr.write_u32(1, 0);
    core.state_mut().fpr.write_u32(2, 0);
    write_word(&ram, 0, op_fp(0x0c, 2, 1, 0, 3)); // FDIV.S
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), boxed_single(CANONICAL_NAN_F32));
    assert_eq!(core.state().fcsr.flags_bits(), 0b1_0000);

    // Double arithmetic canonicalizes to 0x7ff8000000000000.
    core.state_mut().fcsr.clear_flags();
    core.state_mut().fpr.write(1, ruscv_sim::Fpr::from_bits(0));
    core.state_mut().fpr.write(2, ruscv_sim::Fpr::from_bits(0));
    write_word(&ram, 0, op_fp(0x0d, 2, 1, 0, 3)); // FDIV.D of zeros
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), 0x7ff8_0000_0000_0000);
    assert_eq!(core.state().fcsr.flags_bits(), 0b1_0000);

    // FCVT.D.S preserves a signaling NaN as the canonical double NaN with NV.
    core.state_mut().fcsr.clear_flags();
    core.state_mut()
        .fpr
        .write(4, ruscv_sim::Fpr::from_bits(boxed_single(0x7f80_0001)));
    write_word(&ram, 0, op_fp(0x21, 0, 4, 0, 6));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 6), 0x7ff8_0000_0000_0000);
    assert_eq!(core.state().fcsr.flags_bits(), 0b1_0000);
}

#[test]
fn double_nan_payloads_survive_load_store_and_sign_injection() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[5] = DATA;
    let payload = 0x7ff8_0000_0000_0001u64;
    ram.lock().unwrap().write_dword(DATA, payload).unwrap();
    write_word(&ram, 0, fld(3, 5, 0));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), payload);
    write_word(&ram, 0, fsd(3, 5, 8));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(read_dword(&ram, DATA + 8), payload);

    // FSGNJ.D with a positive sign source preserves the payload bits.
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(1.0f64.to_bits()));
    write_word(&ram, 0, op_fp(0x11, 2, 3, 0, 6));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 6), payload);
}

#[test]
fn f0_is_an_ordinary_floating_point_register() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(0, 1.5f32.to_bits());
    core.state_mut().fpr.write_u32(2, 2.5f32.to_bits());
    write_word(&ram, 0, op_fp(0x00, 2, 0, 0, 0)); // FADD.S f0, f0, f2
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 0), boxed_single(4.0f32.to_bits()));
}

#[test]
fn conversions_produce_exact_bits_on_the_path() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // Integer to floating point: the source is x[rs1].
    for (source, raw, expected) in [
        // FCVT.S.W of -1 writes NaN-boxed -1.0f32.
        (
            0xffff_ffff_ffff_ffffu64,
            op_fp(0x68, 0, 1, 0, 3),
            boxed_single((-1.0f32).to_bits()),
        ),
        // FCVT.D.W of the sign-extended low word -2^31.
        (
            0xffff_ffff_8000_0000,
            op_fp(0x69, 0, 1, 0, 3),
            (-2_147_483_648.0f64).to_bits(),
        ),
    ] {
        core.state_mut().regs[1] = source;
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fpr_bits(&core, 3), expected, "{raw:#010x}");
        assert_eq!(core.state().fcsr.flags_bits(), 0, "{raw:#010x}");
    }

    // Floating-point to floating-point: the source is f[rs1].
    for (source, raw, expected) in [
        // FCVT.S.D of 1.0 narrows exactly.
        (
            1.0f64.to_bits(),
            op_fp(0x20, 1, 1, 0, 3),
            boxed_single(1.0f32.to_bits()),
        ),
        // FCVT.D.S widens a correctly boxed 1.5 exactly.
        (
            boxed_single(1.5f32.to_bits()),
            op_fp(0x21, 0, 1, 0, 3),
            1.5f64.to_bits(),
        ),
    ] {
        core.state_mut()
            .fpr
            .write(1, ruscv_sim::Fpr::from_bits(source));
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(fpr_bits(&core, 3), expected, "{raw:#010x}");
        assert_eq!(core.state().fcsr.flags_bits(), 0, "{raw:#010x}");
    }

    // FMV.X.D transfers all 64 bits and FMV.D.X restores them.
    core.state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(0x0123_4567_89ab_cdef));
    write_word(&ram, 0, op_fp(0x71, 0, 2, 0, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().regs[4], 0x0123_4567_89ab_cdef);
    core.state_mut().regs[1] = 0xfedc_ba98_7654_3210;
    write_word(&ram, 0, op_fp(0x79, 0, 1, 0, 5));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 5), 0xfedc_ba98_7654_3210);

    // FEQ writes 1/0 to the integer destination without a flag.
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 1.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x50, 2, 1, 2, 4));
    core.state_mut().pc = 0;
    retire(&mut core);
    assert_eq!(core.state().regs[4], 1);
}

#[test]
fn integer_conversions_follow_the_riscv_saturation_table_on_the_path() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    // (source bits, double?, instruction, expected GPR value)
    let cases = [
        // A negative value to an unsigned destination saturates to 0 (NV).
        (
            u64::from((-1.0f32).to_bits()),
            false,
            op_fp(0x60, 1, 1, 0, 4),
            0u64,
        ), // FCVT.WU.S
        // NaN to signed word saturates to the positive maximum (NV).
        (0x7fc0_0000u64, false, op_fp(0x60, 0, 1, 0, 4), 0x7fff_ffff),
        // Negative to unsigned long saturates to 0 (NV).
        ((-1.0f64).to_bits(), true, op_fp(0x61, 3, 1, 0, 4), 0),
        // +inf to signed long saturates to the positive maximum (NV).
        (
            f64::INFINITY.to_bits(),
            true,
            op_fp(0x61, 2, 1, 0, 4),
            0x7fff_ffff_ffff_ffff,
        ),
    ];
    for (source, is_double, raw, expected) in cases {
        core.state_mut().fcsr.clear_flags();
        if is_double {
            core.state_mut()
                .fpr
                .write(1, ruscv_sim::Fpr::from_bits(source));
        } else {
            core.state_mut().fpr.write_u32(1, source as u32);
        }
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(core.state().regs[4], expected, "{raw:#010x}");
        assert_eq!(core.state().fcsr.flags_bits(), 0b1_0000, "{raw:#010x}");
    }
}

#[test]
fn fclass_distinguishes_quiet_and_signaling_nan_on_the_path() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    for (bits, expected) in [
        (0x7f80_0001u32, 1u64 << 8),    // signaling NaN
        (CANONICAL_NAN_F32, 1u64 << 9), // quiet NaN
        (0xff80_0000, 1u64 << 0),       // -infinity
        (0x0000_0000, 1u64 << 4),       // +0
        (0x8000_0000, 1u64 << 3),       // -0
        (0x0000_0001, 1u64 << 5),       // +subnormal
        (0x8000_0001, 1u64 << 2),       // -subnormal
        (0x3f80_0000, 1u64 << 6),       // +normal
        (0xbf80_0000, 1u64 << 1),       // -normal
        (0x7f80_0000, 1u64 << 7),       // +infinity
    ] {
        core.state_mut().fpr.write_u32(1, bits);
        write_word(&ram, 0, op_fp(0x70, 0, 1, 1, 4));
        core.state_mut().pc = 0;
        retire(&mut core);
        assert_eq!(core.state().regs[4], expected, "{bits:#010x}");
        assert_eq!(fs_of(&core), mstatus::FS_CLEAN, "{bits:#010x}");
    }
}

// ============== FP loads and stores on the physical path ==============

type SharedRam = Arc<Mutex<SimpleMemory>>;
type TraceCalls = Arc<Mutex<Vec<(AccessCategory, AccessWidth, u64)>>>;

/// Recording wrapper around the RAM data port.
struct TraceBackend {
    inner: NativeRamBackend,
    calls: TraceCalls,
}

impl PhysicalBackend for TraceBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let descriptor = request.descriptor();
        self.calls
            .lock()
            .unwrap()
            .push((descriptor.category, descriptor.width, descriptor.paddr));
        self.inner.transact(request)
    }
}

impl AtomicBackend for TraceBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.inner.transact_atomic(request)
    }
}

fn traced_core(isa: IsaProfile) -> (RiscvCore, SharedRam, TraceCalls) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(TraceBackend {
        inner: NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        calls: calls.clone(),
    })));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.set_isa_profile(isa);
    core.reset(0, 0);
    (core, ram, calls)
}

#[test]
fn fp_loads_and_stores_use_the_physical_data_port() {
    let (mut core, ram, calls) = traced_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    for (index, word) in [flw(3, 5, 0), fld(4, 5, 8), fsw(3, 5, 16), fsd(4, 5, 24)]
        .into_iter()
        .enumerate()
    {
        ram.lock()
            .unwrap()
            .write_word(index as u64 * 4, word)
            .unwrap();
    }
    ram.lock().unwrap().write_word(DATA, 0x3fc0_0000).unwrap();
    ram.lock()
        .unwrap()
        .write_dword(DATA + 8, 0x4009_21fb_5444_2d18)
        .unwrap();
    core.state_mut().regs[5] = DATA;
    core.state_mut()
        .fpr
        .write(3, ruscv_sim::Fpr::from_bits(boxed_single(0x3fc0_0000)));
    core.state_mut()
        .fpr
        .write(4, ruscv_sim::Fpr::from_bits(0x4009_21fb_5444_2d18));
    calls.lock().unwrap().clear();

    for _ in 0..4 {
        retire(&mut core);
    }
    assert_eq!(fpr_bits(&core, 3), boxed_single(0x3fc0_0000));
    assert_eq!(fpr_bits(&core, 4), 0x4009_21fb_5444_2d18);
    assert_eq!(read_word(&ram, DATA + 16), 0x3fc0_0000);
    assert_eq!(read_dword(&ram, DATA + 24), 0x4009_21fb_5444_2d18);

    let data_calls: Vec<_> = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(category, _, _)| *category != AccessCategory::Fetch)
        .copied()
        .collect();
    assert_eq!(
        data_calls,
        vec![
            (AccessCategory::DataRead, AccessWidth::Word, DATA),
            (AccessCategory::DataRead, AccessWidth::Doubleword, DATA + 8),
            (AccessCategory::DataWrite, AccessWidth::Word, DATA + 16),
            (
                AccessCategory::DataWrite,
                AccessWidth::Doubleword,
                DATA + 24
            ),
        ]
    );
}

#[test]
fn misaligned_fp_access_traps_before_any_data_request() {
    for (raw, cause, address) in [
        (
            flw(3, 5, 2),
            ExceptionCause::LoadAddressMisaligned,
            DATA + 2,
        ),
        (
            fld(3, 5, 4),
            ExceptionCause::LoadAddressMisaligned,
            DATA + 4,
        ),
        (
            fsw(3, 5, 2),
            ExceptionCause::StoreAddressMisaligned,
            DATA + 2,
        ),
        (
            fsd(3, 5, 4),
            ExceptionCause::StoreAddressMisaligned,
            DATA + 4,
        ),
    ] {
        let (mut core, ram, calls) = traced_core(IsaProfile::Rv64imafd);
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut().regs[5] = DATA;
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        calls.lock().unwrap().clear();
        let entered = trap(&mut core);
        assert_eq!(entered.cause, Trap::Exception(cause), "{raw:#010x}");
        assert_eq!(entered.mtval, address, "{raw:#010x}");
        assert!(
            calls
                .lock()
                .unwrap()
                .iter()
                .all(|(category, _, _)| *category == AccessCategory::Fetch),
            "{raw:#010x} issued a data request"
        );
        // The trap leaves no FPR, fcsr or FS effect from the load/store.
        assert_eq!(fpr_bits(&core, 3), 0);
        assert_eq!(core.state().fcsr.read(), 0);
        assert_eq!(fs_of(&core), mstatus::FS_CLEAN);
    }
}

#[test]
fn fp_access_outside_the_ported_range_is_an_access_fault() {
    for (raw, cause) in [
        (fld(3, 5, 0), ExceptionCause::LoadAccessFault),
        (fsd(3, 5, 0), ExceptionCause::StoreAccessFault),
    ] {
        let (mut core, ram, _calls) = traced_core(IsaProfile::Rv64imafd);
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut().regs[5] = RAM_SIZE as u64 + 0x100;
        write_word(&ram, 0, raw);
        core.state_mut().pc = 0;
        let entered = trap(&mut core);
        assert_eq!(entered.cause, Trap::Exception(cause), "{raw:#010x}");
        assert_eq!(entered.mtval, RAM_SIZE as u64 + 0x100);
        assert_eq!(fs_of(&core), mstatus::FS_CLEAN);
    }
}

// ============== Malformed encodings never become SimulatorFailure ==============

#[test]
fn malformed_fp_encodings_trap_as_illegal_instructions() {
    let malformed = [
        op_fp(0x02, 2, 1, 0, 3),           // fmt = 10 (Zfh), out of profile
        op_fp(0x2c, 1, 1, 0, 3),           // FSQRT.S with rs2 != 0
        op_fp(0x10, 2, 1, 3, 3),           // FSGNJ selector 3
        op_fp(0x14, 2, 1, 2, 3),           // FMIN selector 2
        op_fp(0x50, 2, 1, 3, 4),           // compare selector 3
        op_fp(0x60, 4, 1, 0, 4),           // FCVT with rs2 = 4
        op_fp(0x20, 0, 1, 0, 3),           // FCVT.S.S is not an instruction
        op_fp(0x21, 1, 1, 0, 3),           // FCVT.D.D is not an instruction
        op_fp(0x70, 0, 1, 2, 4),           // FCLASS/FMV selector 2
        op_fp(0x70, 1, 1, 0, 4),           // FMV.X.W with rs2 != 0
        op_fp(0x78, 1, 1, 0, 3),           // FMV.W.X with rs2 != 0
        op_fp(0x06, 0, 1, 0, 3),           // unassigned OP-FP operation
        0b000_0111 | (5 << 15) | (3 << 7), // LOAD-FP funct3 = 000
        0b000_0111 | (5 << 15) | (1 << 12) | (3 << 7), // FLH (Zfh)
        0b010_0111 | (3 << 20) | (5 << 15) | (1 << 12), // S-H (Zfh)
        r4(0b100_0011, 3, 2, 2, 1, 0, 5),  // FMADD with fmt = 10
        r4(0b100_0011, 3, 3, 2, 1, 0, 5),  // FMADD with fmt = 11
    ];
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        let (mut core, ram) = ported_core(isa);
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut().regs[5] = DATA;
        for raw in malformed {
            let before = fp_state(&core);
            write_word(&ram, 0, raw);
            core.state_mut().pc = 0;
            expect_illegal(&mut core, raw);
            assert_eq!(fp_state(&core), before, "{} {raw:#010x}", isa.name());
        }
    }
}

#[test]
fn a_legal_fp_encoding_is_not_a_simulator_failure_under_any_profile() {
    // A well-formed FADD.S must either retire (FP profile, FS not Off) or
    // trap illegal (default/compressed profiles). It must never surface
    // UnsupportedLegalInstruction or Invariant.
    for isa in [
        IsaProfile::Rv64ima,
        IsaProfile::Rv64imac,
        IsaProfile::Rv64imafd,
        IsaProfile::Rv64imafdc,
    ] {
        let (mut core, ram) = ported_core(isa);
        if isa.has_fp() {
            set_fs(&mut core, mstatus::FS_CLEAN);
        }
        core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
        core.state_mut().fpr.write_u32(2, 2.0f32.to_bits());
        write_word(&ram, 0, op_fp(0x00, 2, 1, 0, 3));
        core.state_mut().pc = 0;
        match core.step_outcome() {
            StepOutcome::InstructionRetired(_) => assert!(isa.has_fp(), "{}", isa.name()),
            StepOutcome::TrapEntered(entered) => {
                assert!(!isa.has_fp(), "{}", isa.name());
                assert_eq!(
                    entered.cause,
                    Trap::Exception(ExceptionCause::IllegalInstruction)
                );
            }
            StepOutcome::SimulatorFailure(failure) => {
                assert_ne!(
                    failure.kind,
                    SimulatorFailureKind::UnsupportedLegalInstruction
                );
                panic!("{}: {failure:?}", isa.name());
            }
            other => panic!("{}: {other:?}", isa.name()),
        }
    }
}

// ============== Commit facts ==============

#[test]
fn commit_facts_carry_fpr_and_fcsr_effects() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 0x3380_0000); // 2^-24 forces NX
    write_word(&ram, 0, op_fp(0x00, 2, 1, 0, 3));
    core.state_mut().pc = 0;

    let turn = core.step_transition(true);
    assert!(turn.control.retired);
    let Some(Observation::Commit(record)) = turn.observation else {
        panic!("expected a commit record");
    };
    assert_eq!(record.effects.fpr.len(), 1);
    let write = record.effects.fpr[0];
    assert_eq!(write.index, 3);
    assert_eq!(write.before, 0);
    assert_eq!(write.after, boxed_single(0x3f80_0000));
    assert_eq!(record.effects.fcsr, Some((0b0000_0000, 0b0000_0001)));
    assert_eq!(record.effects.gpr, Vec::new());
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY);
}

#[test]
fn commit_facts_show_gpr_writes_and_store_effects() {
    // FCVT.W.S writes only the integer destination.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 7.0f32.to_bits());
    write_word(&ram, 0, op_fp(0x60, 0, 1, 0, 4));
    core.state_mut().pc = 0;
    let turn = core.step_transition(true);
    let Some(Observation::Commit(record)) = turn.observation else {
        panic!("expected a commit record");
    };
    assert_eq!(record.effects.gpr.len(), 1);
    assert_eq!(record.effects.gpr[0].index, 4);
    assert_eq!(record.effects.gpr[0].after, 7);
    assert!(record.effects.fpr.is_empty());
    assert_eq!(record.effects.fcsr, None);

    // FSW writes memory only: no FPR or fcsr effect and FS stays Clean.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(2, 0x3f80_0000);
    core.state_mut().regs[5] = DATA;
    write_word(&ram, 0, fsw(2, 5, 0));
    core.state_mut().pc = 0;
    let turn = core.step_transition(true);
    let Some(Observation::Commit(record)) = turn.observation else {
        panic!("expected a commit record");
    };
    assert!(record.effects.fpr.is_empty());
    assert_eq!(record.effects.fcsr, None);
    assert_eq!(record.effects.memory.len(), 1);
    assert_eq!(record.effects.memory[0].width, PhysicalWidth::Word);
    assert_eq!(
        record.effects.memory[0].write,
        Some(0x3f80_0000u64.to_le_bytes())
    );
    assert_eq!(fs_of(&core), mstatus::FS_CLEAN);
}

// ============== FP loads and stores under Sv39 ==============

const PAGE: u64 = 0x1000;
const BASE: u64 = 0x8000_0000;
const ROOT_PPN: u64 = 0x80000;
const PROGRAM_VA: u64 = 0x1000;
const DATA_VA: u64 = 0x2000;
const READONLY_VA: u64 = 0x3000;
/// Guest-physical program page (PPN 0x80005).
const PROGRAM_PA: u64 = 0x8000_5000;
/// Guest-physical data page (PPN 0x80004).
const DATA_PA: u64 = 0x8000_4000;

fn gp_write(ram: &Arc<Mutex<SimpleMemory>>, guest: u64, value: u64) {
    ram.lock()
        .unwrap()
        .write_dword(guest - BASE, value)
        .unwrap();
}

fn gp_read(ram: &Arc<Mutex<SimpleMemory>>, guest: u64) -> u64 {
    ram.lock().unwrap().read_dword(guest - BASE).unwrap()
}

/// Installs a 4 KiB leaf mapping `va` to guest-physical `ppn`.
fn install_leaf(ram: &Arc<Mutex<SimpleMemory>>, va: u64, ppn: u64, perm: u64) {
    let vpn = [(va >> 12) & 0x1ff, (va >> 21) & 0x1ff, (va >> 30) & 0x1ff];
    gp_write(
        ram,
        ROOT_PPN * PAGE + vpn[2] * 8,
        ((ROOT_PPN + 1) << 10) | 0x1,
    );
    gp_write(
        ram,
        (ROOT_PPN + 1) * PAGE + vpn[1] * 8,
        ((ROOT_PPN + 2) << 10) | 0x1,
    );
    gp_write(
        ram,
        (ROOT_PPN + 2) * PAGE + vpn[0] * 8,
        (ppn << 10) | perm | 0x1,
    );
}

fn sv39_core() -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let (mut core, ram) = ported_core_at(IsaProfile::Rv64imafd, BASE);
    // Root, mid, leaf tables plus the program, data and read-only pages.
    install_leaf(&ram, PROGRAM_VA, 0x80005, 0x02 | 0x08 | 0x40); // R-X, A
    install_leaf(&ram, DATA_VA, 0x80004, 0x02 | 0x04 | 0x40 | 0x80); // RW, A|D
    install_leaf(&ram, READONLY_VA, 0x80006, 0x02 | 0x40); // R--, A
    core.state_mut()
        .csr
        .write(supervisor::SATP, (8u64 << 60) | ROOT_PPN)
        .unwrap();
    (core, ram)
}

fn enter_supervisor(core: &mut RiscvCore, fs: u64) {
    core.state_mut().csr.write(machine::MSTATUS, fs).unwrap();
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.state_mut().pc = PROGRAM_VA;
}

#[test]
fn sv39_fp_loads_and_stores_translate_through_the_physical_port() {
    let (mut core, ram) = sv39_core();
    core.state_mut().regs[5] = DATA_VA;
    let program = [fld(3, 5, 0), fsd(3, 5, 8), flw(4, 5, 16)];
    for (index, word) in program.into_iter().enumerate() {
        ram.lock()
            .unwrap()
            .write_word(PROGRAM_PA - BASE + index as u64 * 4, word)
            .unwrap();
    }
    gp_write(&ram, DATA_PA, 0x4009_21fb_5444_2d18);
    gp_write(&ram, DATA_PA + 16, 0xffff_ffff_3fc0_0000);
    enter_supervisor(&mut core, mstatus::FS_CLEAN);

    retire(&mut core);
    assert_eq!(fpr_bits(&core, 3), 0x4009_21fb_5444_2d18);
    retire(&mut core);
    assert_eq!(gp_read(&ram, DATA_PA + 8), 0x4009_21fb_5444_2d18);
    retire(&mut core);
    assert_eq!(fpr_bits(&core, 4), boxed_single(0x3fc0_0000));
    assert_eq!(fs_of(&core), mstatus::FS_DIRTY);
}

#[test]
fn sv39_fp_page_faults_leave_no_architectural_effect() {
    // An unmapped load and a store into a read-only page.
    for (raw, base, cause, faulting_va) in [
        // VA 0x4000 has no PTE.
        (
            fld(3, 5, 0),
            0x4000u64,
            ExceptionCause::LoadPageFault,
            0x4000,
        ),
        // VA 0x3000 is mapped read-only, so a store is a page fault.
        (
            fsd(3, 5, 0),
            READONLY_VA,
            ExceptionCause::StorePageFault,
            READONLY_VA,
        ),
    ] {
        let (mut core, ram) = sv39_core();
        core.state_mut().regs[5] = base;
        ram.lock()
            .unwrap()
            .write_word(PROGRAM_PA - BASE, raw)
            .unwrap();
        gp_write(&ram, DATA_PA, 0x1111_2222_3333_4444);
        enter_supervisor(&mut core, mstatus::FS_CLEAN);
        core.state_mut()
            .fpr
            .write(3, ruscv_sim::Fpr::from_bits(0x5555_6666_7777_8888));
        let before = fp_state(&core);
        let entered = trap(&mut core);
        assert_eq!(entered.cause, Trap::Exception(cause), "{raw:#010x}");
        assert_eq!(entered.mtval, faulting_va, "{raw:#010x}");
        assert_eq!(fp_state(&core), before, "{raw:#010x}");
        assert_eq!(gp_read(&ram, DATA_PA), 0x1111_2222_3333_4444);
    }
}

// ============== End-to-end flat facade ==============

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod fixture;

/// A flat-facade guest runs FP instructions from a loaded ELF under an FP
/// profile. This exercises the public load-and-run path, not a helper call.
#[test]
fn a_flat_facade_guest_executes_fp_from_a_loaded_elf() {
    fn csrw(csr: u16, rs1: u8) -> u32 {
        (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (1 << 12) | 0x73
    }
    fn fmv_w_x(rd: u8, rs1: u8) -> u32 {
        (0x78 << 25) | (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x53
    }
    fn fmv_x_w(rd: u8, rs1: u8) -> u32 {
        (0x70 << 25) | (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x53
    }
    fn fadd_s(rd: u8, rs1: u8, rs2: u8) -> u32 {
        (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (u32::from(rd) << 7) | 0x53
    }

    let code = [
        fixture::addi(5, 0, 3),
        fixture::slli(5, 5, 13), // FS_DIRTY (3 << 13)
        csrw(machine::MSTATUS, 5),
        fixture::lui(6, 0x3f800), // 1.0f
        fmv_w_x(1, 6),
        fixture::lui(7, 0x40000), // 2.0f
        fmv_w_x(2, 7),
        fadd_s(3, 1, 2), // 3.0f
        fmv_x_w(8, 3),   // 0x40400000
        fixture::nop(),
    ];
    let elf = fixture::elf_with_code(&code, 0, false, false, 0x4000);
    let mut simulator =
        ruscv_sim::executor::RiscVSimulator::new_with_isa(0x1_0000, IsaProfile::Rv64imafd);
    simulator.load_elf(&elf).expect("the fixture ELF loads");
    let result = simulator
        .run(Some(code.len() as u64))
        .expect("the guest executes");
    assert_eq!(result.cycles, code.len() as u64);
    assert_eq!(simulator.state().regs[8], 0x4040_0000);
    assert_eq!(
        simulator.state().fpr.read(3).bits(),
        boxed_single(0x4040_0000)
    );
    let mut csr = simulator.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    assert_eq!(
        csr.read(machine::MSTATUS).unwrap() & mstatus::FS,
        mstatus::FS_DIRTY
    );
}
