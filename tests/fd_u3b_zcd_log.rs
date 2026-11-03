//! F/D U3b: Zcd expansion and the Spike-format FP commit log (issue #152).
//!
//! Covers contract D8 and D9 plus unit U3b:
//!
//! - under `rv64imafdc` the four Zcd parcels expand to FLD/FSD with the
//!   C.LD/C.SD and C.LDSP/C.SDSP 8-byte scaled offsets and execute through the
//!   U3a path, including the `mstatus.FS`-Off illegal-instruction trap;
//! - under `rv64imac` and `rv64imafd` they stay illegal-instruction exactly as
//!   before;
//! - under FP profiles the commit log prints FPR and `fcsr` writes in pinned
//!   Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`'s commit-log format,
//!   and the log text under `rv64ima`/`rv64imac` is byte-for-byte unchanged.
//!
//! The expected log text is derived from the pinned source, not from this
//! implementation: `riscv/execute.cc` `commit_log_print_insn` prints an FPR
//! write as `" %c%-2d "` at FLEN followed by `0x%016` hex digits, a CSR write
//! as `" c%d_%s "` at XLEN followed by `0x%016` hex digits, and iterates the
//! `std::map` keyed by `(register << 4) | kind` in ascending key order
//! (`riscv/processor.h`).

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod fixture;

use fixture::{addi, sd, slli};
use ruscv_sim::core::commits::CommitLogger;
use ruscv_sim::core::observation::{HartTransition, Observation};
use ruscv_sim::core::{ExceptionCause, PrivilegeMode, RiscvCore, StepOutcome, Trap, TrapEntered};
use ruscv_sim::csr::{machine, mstatus};
use ruscv_sim::decode::{FpFormat, FpOp, Opcode};
use ruscv_sim::executor::{load_and_run_with_isa, RiscVSimulator};
use ruscv_sim::fpu::NAN_BOX_MASK;
use ruscv_sim::isa::rv64c::{CompressedDecoder, ZcaClass, ZcaProfile};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use ruscv_sim::IsaProfile;
use std::sync::{Arc, Mutex};

const RAM_SIZE: usize = 0x10_0000;
/// Data offset inside the loaded fixture image.
const ELF_DATA: u64 = 0x7f8;

// ============== Compressed parcel encoders ==============

// The Zcd parcels are encoded here from the specification's bit diagrams
// (ISA Manual 20250508, Chapter "C"), independently of the decoder's
// extractors.

/// C.FLD: quadrant 00, funct3 001. `uimm[5:3] <- p[12:10]`,
/// `rs1' <- p[9:7]`, `uimm[7:6] <- p[6:5]`, `rd' <- p[4:2]`.
fn c_fld(rdp: u32, rs1p: u32, uimm: u32) -> u16 {
    assert!(rdp < 8 && rs1p < 8 && uimm < 256 && uimm.is_multiple_of(8));
    (0b001u16 << 13)
        | (((uimm >> 3) as u16 & 0b111) << 10)
        | ((rs1p as u16) << 7)
        | (((uimm >> 6) as u16 & 0b11) << 5)
        | ((rdp as u16) << 2)
}

/// C.FSD: quadrant 00, funct3 101; the same immediate as C.FLD with
/// `rs2' <- p[4:2]`.
fn c_fsd(rs2p: u32, rs1p: u32, uimm: u32) -> u16 {
    assert!(rs2p < 8 && rs1p < 8 && uimm < 256 && uimm.is_multiple_of(8));
    (0b101u16 << 13)
        | (((uimm >> 3) as u16 & 0b111) << 10)
        | ((rs1p as u16) << 7)
        | (((uimm >> 6) as u16 & 0b11) << 5)
        | ((rs2p as u16) << 2)
}

/// C.FLDSP: quadrant 10, funct3 001. `uimm[5] <- p[12]`, `rd <- p[11:7]`,
/// `uimm[4:3] <- p[6:5]`, `uimm[8:6] <- p[4:2]`.
fn c_fldsp(rd: u32, uimm: u32) -> u16 {
    assert!(rd < 32 && uimm < 512 && uimm.is_multiple_of(8));
    (0b001u16 << 13)
        | (((uimm >> 5) as u16 & 0b1) << 12)
        | ((rd as u16) << 7)
        | (((uimm >> 3) as u16 & 0b11) << 5)
        | (((uimm >> 6) as u16 & 0b111) << 2)
        | 0b10
}

/// C.FSDSP: quadrant 10, funct3 101. `uimm[5:3] <- p[12:10]`,
/// `uimm[8:6] <- p[9:7]`, `rs2 <- p[6:2]`.
fn c_fsdsp(rs2: u32, uimm: u32) -> u16 {
    assert!(rs2 < 32 && uimm < 512 && uimm.is_multiple_of(8));
    (0b101u16 << 13)
        | (((uimm >> 3) as u16 & 0b111) << 10)
        | (((uimm >> 6) as u16 & 0b111) << 7)
        | ((rs2 as u16) << 2)
        | 0b10
}

/// C.LI, used as a legal compressed filler and as the `rv64imac` log subject.
fn c_li(rd: u32, imm: i32) -> u16 {
    assert!(rd < 32 && (-32..32).contains(&imm));
    (0b010u16 << 13)
        | (((imm >> 5) as u16 & 0b1) << 12)
        | ((rd as u16) << 7)
        | (((imm as u16) & 0b1_1111) << 2)
        | 0b01
}

/// C.NOP: `addi x0, x0, 0`.
const C_NOP: u16 = 0x0001;

// ============== F/D instruction encoders ==============

/// OP-FP: `funct7 | rs2 | rs1 | rm/selector | rd | 1010011`.
fn op_fp(funct7: u32, rs2: u32, rs1: u32, rm_or_selector: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (rm_or_selector << 12) | (rd << 7) | 0b101_0011
}

fn fmv_d_x(rd: u32, rs1: u32) -> u32 {
    op_fp(0x79, 0, rs1, 0, rd)
}

/// CSRRW with `rd = x0`: write `rs1` into `csr`.
fn csrw(csr: u16, rs1: u32) -> u32 {
    (u32::from(csr) << 20) | (rs1 << 15) | (1 << 12) | 0x73
}

fn boxed_single(bits: u32) -> u64 {
    NAN_BOX_MASK | u64::from(bits)
}

// ============== Hart fixtures ==============

fn ported_core(isa: IsaProfile) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let port = || {
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )))
    };
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, port(), port());
    core.set_isa_profile(isa);
    core.reset(0, 0);
    (core, ram)
}

fn write16(ram: &Arc<Mutex<SimpleMemory>>, offset: u64, parcel: u16) {
    ram.lock().unwrap().write_half(offset, parcel).unwrap();
}

fn write32(ram: &Arc<Mutex<SimpleMemory>>, offset: u64, word: u32) {
    ram.lock().unwrap().write_word(offset, word).unwrap();
}

fn write64(ram: &Arc<Mutex<SimpleMemory>>, offset: u64, value: u64) {
    ram.lock().unwrap().write_dword(offset, value).unwrap();
}

fn read64(ram: &Arc<Mutex<SimpleMemory>>, offset: u64) -> u64 {
    ram.lock().unwrap().read_dword(offset).unwrap()
}

fn set_fs(core: &mut RiscvCore, fs: u64) {
    core.state_mut()
        .csr
        .write(machine::MSTATUS, fs)
        .expect("mstatus is writable in M mode");
}

fn fs_of(state: &ruscv_sim::core::CoreState) -> u64 {
    let mut csr = state.csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(machine::MSTATUS).unwrap() & mstatus::FS
}

fn fpr_bits(core: &RiscvCore, reg: usize) -> u64 {
    core.state().fpr.read(reg).bits()
}

fn trap(core: &mut RiscvCore) -> TrapEntered {
    match core.step_outcome() {
        StepOutcome::TrapEntered(entered) => entered,
        other => panic!("expected a trap, got {other:?}"),
    }
}

/// One traced step; panics unless it retires.
fn traced_step(core: &mut RiscvCore) -> HartTransition {
    let turn = core.step_transition(true);
    assert!(turn.control.retired, "{:?}", turn.outcome);
    turn
}

/// The exact commit-log text of one traced transition.
fn commit_log_text(turn: &HartTransition) -> String {
    let temp = tempfile::NamedTempFile::new().unwrap();
    {
        let mut logger = CommitLogger::new_file(temp.path()).unwrap();
        turn.deliver(&mut logger).unwrap();
    }
    std::fs::read_to_string(temp.path()).unwrap()
}

/// The commit record an FP instruction produces through the public Hart path.
fn commit_record(turn: &HartTransition) -> &ruscv_sim::core::observation::CommitRecord {
    match &turn.observation {
        Some(Observation::Commit(record)) => record,
        other => panic!("expected a commit record, got {other:?}"),
    }
}

/// A double-precision value with distinguishable halves.
const SENTINEL: u64 = 0x4009_21fb_5444_2d18;

// ============== D8: Zcd expansion ==============

#[test]
fn zcd_parcels_expand_to_fld_and_fsd_with_the_specified_scaled_offsets() {
    let zcd = CompressedDecoder::with_profile(ZcaProfile::Zcd);
    // C.FLD f8, 0(x8) -> fld f8, 0(x8): LOAD-FP funct3 is the D width.
    let decoded = zcd.decode_16bit(c_fld(0, 0, 0)).expect("C.FLD decodes");
    assert_eq!(decoded.raw, 0x0004_3407);
    assert_eq!(decoded.opcode, Opcode::LoadFp);
    assert_eq!(decoded.fp.unwrap().fmt, FpFormat::Double);
    assert_eq!(decoded.fp.unwrap().op, FpOp::Load);
    assert_eq!(decoded.rd, Some(8));
    assert_eq!(decoded.rs1, Some(8));
    assert_eq!(decoded.imm, Some(0));
    // C.FLD f15, 248(x15) -> fld f15, 248(x15): every offset bit set.
    let decoded = zcd.decode_16bit(c_fld(7, 7, 248)).expect("C.FLD decodes");
    assert_eq!(decoded.raw, 0x0f87_b787);
    assert_eq!(decoded.rd, Some(15));
    assert_eq!(decoded.rs1, Some(15));
    assert_eq!(decoded.imm, Some(248));
    // C.FSD f9, 8(x10) -> fsd f9, 8(x10): STORE-FP funct3 is the D width.
    let decoded = zcd.decode_16bit(c_fsd(1, 2, 8)).expect("C.FSD decodes");
    assert_eq!(decoded.raw, 0x0095_3427);
    assert_eq!(decoded.opcode, Opcode::StoreFp);
    assert_eq!(decoded.fp.unwrap().fmt, FpFormat::Double);
    assert_eq!(decoded.fp.unwrap().op, FpOp::Store);
    assert_eq!(decoded.rs1, Some(10));
    assert_eq!(decoded.rs2, Some(9));
    assert_eq!(decoded.imm, Some(8));
    // C.FLDSP f0, 0(x2) -> fld f0, 0(x2): f0 is an ordinary register.
    let decoded = zcd.decode_16bit(c_fldsp(0, 0)).expect("C.FLDSP decodes");
    assert_eq!(decoded.raw, 0x0001_3007);
    assert_eq!(decoded.opcode, Opcode::LoadFp);
    assert_eq!(decoded.rd, Some(0));
    assert_eq!(decoded.rs1, Some(2));
    // C.FLDSP f31, 504(x2) -> fld f31, 504(x2): maximum offset.
    let decoded = zcd.decode_16bit(c_fldsp(31, 504)).expect("C.FLDSP decodes");
    assert_eq!(decoded.raw, 0x1f81_3f87);
    assert_eq!(decoded.rd, Some(31));
    assert_eq!(decoded.rs1, Some(2));
    assert_eq!(decoded.imm, Some(504));
    // C.FSDSP f1, 8(x2) -> fsd f1, 8(x2).
    let decoded = zcd.decode_16bit(c_fsdsp(1, 8)).expect("C.FSDSP decodes");
    assert_eq!(decoded.raw, 0x0011_3427);
    assert_eq!(decoded.opcode, Opcode::StoreFp);
    assert_eq!(decoded.rs1, Some(2));
    assert_eq!(decoded.rs2, Some(1));
    // C.FSDSP f31, 504(x2) -> fsd f31, 504(x2): maximum offset.
    let decoded = zcd.decode_16bit(c_fsdsp(31, 504)).expect("C.FSDSP decodes");
    assert_eq!(decoded.raw, 0x1ff1_3c27);
    assert_eq!(decoded.rs2, Some(31));
    assert_eq!(decoded.imm, Some(504));
}

#[test]
fn the_zca_profile_keeps_every_zcd_parcel_fp_illegal() {
    let zca = CompressedDecoder::new();
    assert_eq!(zca.profile(), ZcaProfile::Zca);
    // Every parcel of the four Zcd slots is FP-illegal and undecodable.
    for funct3 in [0b001u16, 0b101] {
        for quadrant in [0b00u16, 0b10] {
            for fields in [0u16, 0x1ffc, 0x0124, 0x0abc] {
                let parcel = (funct3 << 13) | (fields & 0x1fff) | quadrant;
                assert_eq!(
                    zca.classify_16bit(parcel),
                    ZcaClass::FpIllegal,
                    "parcel {parcel:#06x} must stay FP-illegal under Zca"
                );
                assert!(
                    zca.decode_16bit(parcel).is_err(),
                    "parcel {parcel:#06x} must not decode under Zca"
                );
            }
        }
    }
    // The same parcels expand under Zcd: the profile is the only difference.
    let zcd = CompressedDecoder::with_profile(ZcaProfile::Zcd);
    assert_eq!(zcd.profile(), ZcaProfile::Zcd);
    assert!(matches!(
        zcd.classify_16bit(c_fld(0, 0, 0)),
        ZcaClass::Expansion { .. }
    ));
}

#[test]
fn only_rv64imafdc_selects_zcd() {
    for (isa, has_zcd) in [
        (IsaProfile::Rv64ima, false),
        (IsaProfile::Rv64imac, false),
        (IsaProfile::Rv64imafd, false),
        (IsaProfile::Rv64imafdc, true),
    ] {
        assert_eq!(isa.has_zcd(), has_zcd, "{}", isa.name());
    }
}

// ============== D8: Zcd execution on the public Hart path ==============

#[test]
fn c_fld_and_c_fsd_execute_under_imafdc() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafdc);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[8] = 0x100; // base, also the compact rs1'
    core.state_mut().regs[9] = 0x200; // store base
    write64(&ram, 0x108, SENTINEL);
    write16(&ram, 0, c_fld(0, 0, 8)); // f8, 8(x8)
    write16(&ram, 2, c_fsd(1, 1, 16)); // f9 -> 16(x9)

    let turn = traced_step(&mut core);
    assert!(turn.control.retired);
    assert_eq!(fpr_bits(&core, 8), SENTINEL);
    assert_eq!(fs_of(core.state()), mstatus::FS_DIRTY);
    let record = commit_record(&turn);
    assert_eq!(record.instruction_length, 2);
    assert_eq!(record.retired.instruction, u32::from(c_fld(0, 0, 8)));
    assert_eq!(record.effects.fpr.len(), 1);
    assert_eq!(record.effects.fpr[0].index, 8);
    assert_eq!(record.effects.fpr[0].before, 0);
    assert_eq!(record.effects.fpr[0].after, SENTINEL);
    assert_eq!(record.effects.fcsr, None);

    // C.FSD stores the register's 64 bits and writes no FPR.
    core.state_mut()
        .fpr
        .write(9, ruscv_sim::Fpr::from_bits(SENTINEL));
    let turn = traced_step(&mut core);
    assert_eq!(read64(&ram, 0x210), SENTINEL);
    assert_eq!(fs_of(core.state()), mstatus::FS_DIRTY);
    let record = commit_record(&turn);
    assert!(record.effects.fpr.is_empty());
    assert_eq!(record.effects.memory.len(), 1);
    assert_eq!(
        record.effects.memory[0].width,
        ruscv_sim::physical::PhysicalWidth::Doubleword
    );
}

#[test]
fn c_fldsp_and_c_fsdsp_use_x2_and_any_fpr() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafdc);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[2] = 0x100;
    write64(&ram, 0x100, SENTINEL);
    write64(&ram, 0x100 + 504, SENTINEL);
    write16(&ram, 0, c_fldsp(0, 504)); // fld f0, 504(sp)
    write16(&ram, 2, c_fldsp(31, 0)); // fld f31, 0(sp)
    write16(&ram, 4, c_fsdsp(0, 8)); // fsd f0, 8(sp)
    write16(&ram, 6, c_fsdsp(31, 16)); // fsd f31, 16(sp)

    traced_step(&mut core);
    assert_eq!(fpr_bits(&core, 0), SENTINEL);
    traced_step(&mut core);
    assert_eq!(fpr_bits(&core, 31), SENTINEL);
    traced_step(&mut core);
    assert_eq!(read64(&ram, 0x108), SENTINEL);
    traced_step(&mut core);
    assert_eq!(read64(&ram, 0x110), SENTINEL);
    assert_eq!(fs_of(core.state()), mstatus::FS_DIRTY);
}

#[test]
fn zcd_obeys_the_fs_off_illegal_instruction_rule() {
    for (parcel, name) in [
        (c_fld(0, 0, 0), "C.FLD"),
        (c_fsd(0, 0, 0), "C.FSD"),
        (c_fldsp(1, 0), "C.FLDSP"),
        (c_fsdsp(1, 0), "C.FSDSP"),
    ] {
        let (mut core, ram) = ported_core(IsaProfile::Rv64imafdc);
        // FS resets to Off; a stale FPR and memory value must survive.
        core.state_mut()
            .fpr
            .write(8, ruscv_sim::Fpr::from_bits(0x5555_6666_7777_8888));
        core.state_mut().regs[2] = 0x100;
        write64(&ram, 0x100, SENTINEL);
        write16(&ram, 0, parcel);
        let entered = trap(&mut core);
        assert_eq!(
            entered.cause,
            Trap::Exception(ExceptionCause::IllegalInstruction),
            "{name}"
        );
        assert_eq!(entered.mtval, u64::from(parcel), "{name}");
        assert_eq!(entered.instruction, Some(u32::from(parcel)), "{name}");
        assert_eq!(fpr_bits(&core, 8), 0x5555_6666_7777_8888, "{name}");
        assert_eq!(read64(&ram, 0x100), SENTINEL, "{name}");
        assert_eq!(core.state().fcsr.read(), 0, "{name}");
        assert_eq!(fs_of(core.state()), mstatus::FS_OFF, "{name}");
    }
}

#[test]
fn zcd_stays_illegal_under_imac_and_is_aligned_away_under_imafd() {
    // rv64imac: the parcel is FP-illegal and traps with no architectural
    // effect. The F/D guard would also reject the expansion; the decoder's
    // profile dimension is what classifies the parcel (contract D8).
    let (mut core, ram) = ported_core(IsaProfile::Rv64imac);
    write64(&ram, 0x100, SENTINEL);
    core.state_mut().fcsr.write(0);
    core.state_mut().regs[8] = 0x100;
    write16(&ram, 0, c_fld(0, 0, 0));
    let entered = trap(&mut core);
    assert_eq!(
        entered.cause,
        Trap::Exception(ExceptionCause::IllegalInstruction)
    );
    assert_eq!(entered.mtval, u64::from(c_fld(0, 0, 0)));
    assert_eq!(fpr_bits(&core, 8), 0);
    assert_eq!(read64(&ram, 0x100), SENTINEL);
    assert_eq!(fs_of(core.state()), mstatus::FS_OFF);

    // rv64imafd has IALIGN=32, so a 2-byte-aligned parcel is misaligned
    // before any decode and never executes as C.FLD.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    write64(&ram, 0x100, SENTINEL);
    core.state_mut().regs[8] = 0x100;
    write16(&ram, 2, c_fld(0, 0, 0));
    core.state_mut().pc = 2;
    let entered = trap(&mut core);
    assert_eq!(
        entered.cause,
        Trap::Exception(ExceptionCause::InstructionAddressMisaligned)
    );
    assert_eq!(entered.mtval, 2);
    assert_eq!(fpr_bits(&core, 8), 0);
}

// ============== D8: Zcd from a loaded ELF under rv64imafdc ==============

#[test]
fn a_loaded_rv64imafdc_guest_executes_zcd() {
    // FS Dirty, then store a sentinel with SD, load it with C.FLD and store
    // it back with C.FSD. The two compressed parcels share one code word.
    let code = [
        addi(5, 0, 3),
        slli(5, 5, 13), // mstatus.FS = Dirty
        csrw(machine::MSTATUS, 5),
        // AUIPC at pc 12 gives the ELF base; LUI would sign-extend bit 31.
        fixture::auipc(8, 0),
        addi(8, 8, ELF_DATA as i32 - 12), // x8 = data
        addi(9, 8, 8),                    // x9 = data + 8
        addi(10, 0, -1),                  // x10 = all ones
        sd(10, 8, 0),
        u32::from(c_fld(0, 0, 0)) | (u32::from(c_fsd(0, 1, 0)) << 16),
        u32::from(C_NOP) | (u32::from(C_NOP) << 16),
    ];
    let elf = fixture::elf_with_code(&code, 0, false, false, 0x4000);
    let mut simulator = RiscVSimulator::new_with_isa(0x1_0000, IsaProfile::Rv64imafdc);
    simulator.load_elf(&elf).expect("the fixture ELF loads");
    let result = simulator.run(Some(12)).expect("the guest executes");
    assert_eq!(result.cycles, 12);
    assert_eq!(simulator.state().fpr.read(8).bits(), u64::MAX);
    assert_eq!(
        simulator.read_mem(ELF_DATA + 8, 8).unwrap(),
        u64::MAX.to_le_bytes()
    );
    assert_eq!(fs_of(simulator.state()), mstatus::FS_DIRTY);
}

// ============== D9: Spike-format FP commit log ==============

#[test]
fn commit_log_prints_an_fpr_write_and_an_fcsr_write_in_spike_format() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    core.state_mut().fpr.write_u32(2, 0x3380_0000); // 2^-24 forces NX
    let fadd = op_fp(0x00, 2, 1, 0, 3);
    write32(&ram, 0, fadd);
    let turn = traced_step(&mut core);
    // Spike: `" %c%-2d "` at FLEN then `0x%016` (execute.cc), and the CSR
    // segment after the register bucket of CSR 0x003.
    assert_eq!(
        commit_log_text(&turn),
        format!(
            "core   0: 3 0x0000000000000000 (0x{fadd:08x}) \
             f3  {:#018x} c3_fcsr 0x0000000000000001\n",
            boxed_single(0x3f80_0000)
        )
    );
}

#[test]
fn commit_log_fpr_segment_uses_flen_width_and_spike_padding() {
    // A two-digit FPR index has no padding space before its value in Spike's
    // `%-2d` field; f10 is the distinguishing case against the GPR text.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    let value = 0x4009_21fb_5444_2d18u64;
    let fixed = addi(1, 0, 0x7ff); // keep the GPR path quiet
    write32(&ram, 0, fixed);
    traced_step(&mut core);
    core.state_mut().regs[5] = value;
    let mv = fmv_d_x(10, 5);
    write32(&ram, 4, mv);
    let turn = traced_step(&mut core);
    assert_eq!(
        commit_log_text(&turn),
        format!("core   0: 3 0x0000000000000004 (0x{mv:08x}) f10 0x{value:016x}\n")
    );
}

#[test]
fn commit_log_prints_a_csr_fcsr_write_named_fcsr() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[5] = 5;
    let write = csrw(0x003, 5);
    write32(&ram, 0, write);
    let turn = traced_step(&mut core);
    assert_eq!(
        commit_log_text(&turn),
        format!("core   0: 3 0x0000000000000000 (0x{write:08x}) c3_fcsr 0x0000000000000005\n")
    );
    assert_eq!(core.state().fcsr.read(), 5);
    assert_eq!(fs_of(core.state()), mstatus::FS_DIRTY);
}

#[test]
fn commit_log_orders_effects_like_spikes_log_reg_write_map() {
    // FCVT.W.D of +inf writes x<rd> and raises NV. Spike keys x1 at 0x10 and
    // x5 at 0x50 against the fcsr CSR at 0x34, so x1 prints before the fcsr
    // segment and x5 after it.
    for (rd, expected) in [
        (1u32, "x1  0x000000007fffffff c3_fcsr 0x0000000000000010"),
        (5u32, "c3_fcsr 0x0000000000000010 x5  0x000000007fffffff"),
    ] {
        let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
        set_fs(&mut core, mstatus::FS_CLEAN);
        core.state_mut()
            .fpr
            .write(2, ruscv_sim::Fpr::from_bits(0x7ff0_0000_0000_0000));
        let fcvt = op_fp(0x61, 0, 2, 0, rd);
        write32(&ram, 0, fcvt);
        let turn = traced_step(&mut core);
        assert_eq!(
            commit_log_text(&turn),
            format!("core   0: 3 0x0000000000000000 (0x{fcvt:08x}) {expected}\n")
        );
    }
}

#[test]
fn commit_log_prints_equal_value_fpr_writes() {
    // Spike logs a register write, not a register change.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[5] = 0x100;
    core.state_mut()
        .fpr
        .write(8, ruscv_sim::Fpr::from_bits(SENTINEL));
    write64(&ram, 0x100, SENTINEL);
    let fld = (5 << 15) | (0b011 << 12) | (8 << 7) | 0x07;
    write32(&ram, 0, fld);
    let turn = traced_step(&mut core);
    let record = commit_record(&turn);
    assert_eq!(record.effects.fpr[0].before, SENTINEL);
    assert_eq!(record.effects.fpr[0].after, SENTINEL);
    assert_eq!(
        commit_log_text(&turn),
        format!("core   0: 3 0x0000000000000000 (0x{fld:08x}) f8  0x{SENTINEL:016x}\n")
    );
}

// ============== D9: unchanged text without FP profiles ==============

#[test]
fn commit_log_text_is_unchanged_under_ima() {
    // Under rv64ima a GPR write, a CSR write and a store still print exactly
    // the snapshot-era text: no FPR and no CSR segment appears.
    let code = vec![
        addi(3, 0, 5),
        csrw(machine::MSCRATCH, 3),
        addi(4, 0, -1),
        fixture::auipc(6, 0),
        addi(6, 6, ELF_DATA as i32 - 12),
        sd(4, 6, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, false, false, 0x4000);
    let temp = tempfile::NamedTempFile::new().unwrap();
    let result = load_and_run_with_isa(
        &elf,
        Some(code.len() as u64),
        None,
        Some(temp.path()),
        false,
        IsaProfile::Rv64ima,
    )
    .expect("the guest executes");
    assert_eq!(result.cycles, code.len() as u64);
    let text = std::fs::read_to_string(temp.path()).unwrap();
    let expected = format!(
        "core   0: 3 0x0000000080000000 (0x{:08x}) x3  0x0000000000000005\n\
         core   0: 3 0x0000000080000004 (0x{:08x})\n\
         core   0: 3 0x0000000080000008 (0x{:08x}) x4  0xffffffffffffffff\n\
         core   0: 3 0x000000008000000c (0x{:08x}) x6  0x000000008000000c\n\
         core   0: 3 0x0000000080000010 (0x{:08x}) x6  0x00000000800007f8\n\
         core   0: 3 0x0000000080000014 (0x{:08x})\n",
        code[0], code[1], code[2], code[3], code[4], code[5]
    );
    assert_eq!(text, expected);
}

#[test]
fn commit_log_text_is_unchanged_under_imac() {
    // The compressed opcode field keeps its four hex digits; a HINT parcel
    // and a C.LI still print identity only.
    let (mut core, ram) = ported_core(IsaProfile::Rv64imac);
    write16(&ram, 0, c_li(7, -3));
    write16(&ram, 2, 0x0021); // C.NOP with a nonzero immediate (D2 HINT)
    let first = traced_step(&mut core);
    let second = traced_step(&mut core);
    assert_eq!(
        commit_log_text(&first),
        format!(
            "core   0: 3 0x0000000000000000 (0x{:04x}) x7  0xfffffffffffffffd\n",
            c_li(7, -3)
        )
    );
    assert_eq!(
        commit_log_text(&second),
        "core   0: 3 0x0000000000000002 (0x0021)\n"
    );
}

#[test]
fn commit_log_does_not_print_csr_facts_other_than_fcsr() {
    let (mut core, ram) = ported_core(IsaProfile::Rv64imafd);
    set_fs(&mut core, mstatus::FS_CLEAN);
    core.state_mut().regs[5] = 0x55;
    let write = csrw(machine::MSCRATCH, 5);
    write32(&ram, 0, write);
    let turn = traced_step(&mut core);
    assert_eq!(
        commit_log_text(&turn),
        format!("core   0: 3 0x0000000000000000 (0x{write:08x})\n")
    );
}
