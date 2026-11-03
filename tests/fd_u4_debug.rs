//! F/D U4: debugger access to FP registers and CSRs (issue #148).
//!
//! Covers contract D10 and unit U4: DM abstract Access Register reads and
//! writes `f0`–`f31` at `aarsize` 2 and 3 and `fflags`/`frm`/`fcsr` through
//! the CSR regnos, independent of `mstatus.FS`; following pinned Spike the
//! abstract path leaves FS unchanged. A 32-bit FPR write is NaN-boxed and a
//! 32-bit read returns the raw low 32 bits. Program-buffer FP instructions
//! follow the ordinary D6 FS rule, and the DM `misa` read reflects all four
//! profiles. Under `rv64ima`/`rv64imac` the FPR and FP CSR regnos still
//! return `cmderr` 3 with no state change; the existing abstract,
//! program-buffer and SBA suites remain the unchanged controls for those
//! profiles.

use ruscv_sim::core::{DebugCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::{machine, mstatus, unprivileged};
use ruscv_sim::fpu::{CANONICAL_NAN_F32, NAN_BOX_MASK};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::{DebugModule, IsaProfile};
use std::sync::{Arc, Mutex};

const BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;

const DATA0: u8 = 0x04;
const DMCONTROL: u8 = 0x10;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const PROGBUF0: u8 = 0x20;

const HALTREQ: u32 = 1 << 31;
const DMACTIVE: u32 = 1;

const CMDERR_NOT_SUPPORTED: u32 = 2;
const CMDERR_EXCEPTION: u32 = 3;

/// `f0`'s debug regno; `f31` is `0x103f`.
const F0: u32 = 0x1020;
const F31: u32 = 0x103f;

/// `c.nop` parcel.
const C_NOP: u32 = 0x0001;
/// Whole-word `addi x0, x0, 0`.
const NOP: u32 = 0x0000_0013;

// ============== Instruction encoders ==============

/// OP-FP: `funct7 | rs2 | rs1 | rm/selector | rd | 1010011`.
fn op_fp(funct7: u32, rs2: u32, rs1: u32, rm_or_selector: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (rm_or_selector << 12) | (rd << 7) | 0b101_0011
}

/// `fadd.s rd, rs1, rs2` with static RNE.
fn fadd_s(rd: u32, rs1: u32, rs2: u32) -> u32 {
    op_fp(0x00, rs2, rs1, 0, rd)
}

/// CSRRW `csr, rs1, x0` (the immediate forms are not needed here).
fn csrw(csr: u16, rs1: u32) -> u32 {
    (u32::from(csr) << 20) | (rs1 << 15) | (0b001 << 12) | 0x73
}

// ============== Hart and DM harness ==============

struct Harness {
    dm: DebugModule,
    hart: RiscvCore,
}

fn hart_with(isa: IsaProfile) -> RiscvCore {
    let memory: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
        Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let mut hart = RiscvCore::new(Arc::clone(&memory), memory);
    hart.set_isa_profile(isa);
    hart.reset(BASE, BASE);
    hart
}

impl Harness {
    /// Halted hart with `dmactive` set and both buffer words at a safe NOP.
    fn halted(isa: IsaProfile) -> Self {
        let mut h = Self {
            dm: DebugModule::new(),
            hart: hart_with(isa),
        };
        h.hart.state_mut().pc = BASE;
        h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE);
        h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE | HALTREQ);
        match h.hart.step_outcome() {
            StepOutcome::DebugHalted {
                cause: DebugCause::HaltReq,
                completed_turn: false,
            } => {}
            other => panic!("haltreq did not halt the hart: {other:?}"),
        }
        assert!(h.hart.in_debug_mode());
        h.fill_safe_tail();
        h
    }

    fn fill_safe_tail(&mut self) {
        let fill = if self.hart.allows_compressed() {
            (C_NOP << 16) | C_NOP
        } else {
            NOP
        };
        self.prog(0, fill);
        self.prog(1, fill);
    }

    fn cmderr(&mut self) -> u32 {
        (self.dm.dmi_read(&mut self.hart, ABSTRACTCS) >> 8) & 0b111
    }

    fn busy(&mut self) -> bool {
        self.dm.dmi_read(&mut self.hart, ABSTRACTCS) & (1 << 12) != 0
    }

    fn clear_cmderr(&mut self) {
        self.dm.dmi_write(&mut self.hart, ABSTRACTCS, 0b111 << 8);
        assert_eq!(self.cmderr(), 0);
    }

    /// One Access Register command. `aarsize` and `write` are the fields the
    /// D10 cases vary; `transfer` is always 1.
    fn command(&mut self, aarsize: u32, write: bool, regno: u32) {
        let command = (aarsize << 20) | (1 << 17) | (u32::from(write) << 16) | regno;
        self.dm.dmi_write(&mut self.hart, COMMAND, command);
    }

    fn write_reg64(&mut self, regno: u32, value: u64) {
        self.dm.dmi_write(&mut self.hart, DATA0, value as u32);
        self.dm
            .dmi_write(&mut self.hart, DATA0 + 1, (value >> 32) as u32);
        self.command(3, true, regno);
        assert_eq!(self.cmderr(), 0, "writing regno {regno:#x} at aarsize 3");
    }

    fn read_reg64(&mut self, regno: u32) -> u64 {
        self.command(3, false, regno);
        assert_eq!(self.cmderr(), 0, "reading regno {regno:#x} at aarsize 3");
        let low = u64::from(self.dm.dmi_read(&mut self.hart, DATA0));
        let high = u64::from(self.dm.dmi_read(&mut self.hart, DATA0 + 1));
        low | (high << 32)
    }

    /// 32-bit write. `data1` is a sentinel so a 64-bit implementation that
    /// wrongly consumed it would be visible.
    fn write_reg32(&mut self, regno: u32, value: u32) {
        self.dm.dmi_write(&mut self.hart, DATA0, value);
        self.dm.dmi_write(&mut self.hart, DATA0 + 1, 0xDEAD_BEEF);
        self.command(2, true, regno);
        assert_eq!(self.cmderr(), 0, "writing regno {regno:#x} at aarsize 2");
    }

    fn read_reg32(&mut self, regno: u32) -> u32 {
        self.command(2, false, regno);
        assert_eq!(self.cmderr(), 0, "reading regno {regno:#x} at aarsize 2");
        self.dm.dmi_read(&mut self.hart, DATA0)
    }

    fn prog(&mut self, index: u8, value: u32) {
        self.dm.dmi_write(&mut self.hart, PROGBUF0 + index, value);
    }

    /// Access-register command with `postexec` and no transfer.
    fn postexec(&mut self) {
        self.dm
            .dmi_write(&mut self.hart, COMMAND, (3 << 20) | (1 << 18));
    }

    /// Run the active command to completion, one grant at a time.
    fn run_busy(&mut self) {
        for _ in 0..8 {
            if !self.dm.advance(&mut self.hart) {
                assert!(!self.busy(), "advance returned idle but abstractcs is busy");
                return;
            }
        }
        panic!("the command ran more grants than two words plus the implicit EBREAK");
    }

    fn fpr_bits(&self, index: usize) -> u64 {
        self.hart.state().fpr.read(index).bits()
    }

    fn fcsr(&self) -> u32 {
        self.hart.state().fcsr.read()
    }

    fn mstatus(&self) -> u64 {
        self.hart
            .state()
            .csr
            .read_as_machine(machine::MSTATUS)
            .unwrap()
    }

    fn fs(&self) -> u64 {
        self.mstatus() & mstatus::FS
    }

    fn set_fs(&mut self, fs: u64) {
        let value = (self.mstatus() & !mstatus::FS) | fs;
        self.hart
            .state_mut()
            .csr
            .write_as_machine(machine::MSTATUS, value)
            .unwrap();
        assert_eq!(self.fs(), fs, "the harness could not set mstatus.FS");
    }

    /// Drive one abstract access that must be rejected as a missing register,
    /// and prove no data word survived from an earlier command.
    fn expect_missing(&mut self, aarsize: u32, write: bool, regno: u32) {
        if write {
            self.dm.dmi_write(&mut self.hart, DATA0, 0x1111_2222);
            self.dm.dmi_write(&mut self.hart, DATA0 + 1, 0x3333_4444);
        }
        self.command(aarsize, write, regno);
        assert_eq!(
            self.cmderr(),
            CMDERR_EXCEPTION,
            "regno {regno:#x} aarsize {aarsize} write {write}"
        );
        self.clear_cmderr();
    }
}

// ============== D10: abstract FPR access ==============

#[test]
fn abstract_fpr_access_covers_f0_to_f31_at_aarsize_3() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        let mut h = Harness::halted(isa);
        // FP state is independent of mstatus.FS: the reset value is Off.
        assert_eq!(h.fs(), mstatus::FS_OFF);

        for reg in 0..32u32 {
            let value = 0x0123_4567_89AB_CDEFu64.rotate_left(reg * 3) ^ u64::from(reg);
            h.write_reg64(F0 + reg, value);
            assert_eq!(h.read_reg64(F0 + reg), value, "{} f{reg}", isa.name());
            assert_eq!(
                h.fpr_bits(reg as usize),
                value,
                "{} f{reg} architectural state",
                isa.name()
            );
        }

        // f0 is an ordinary register (contract D5), not the GPR x0.
        h.write_reg64(F0, 0x8000_0000_0000_0000);
        assert_eq!(h.fpr_bits(0), 0x8000_0000_0000_0000);
        // f31 is the last implemented register and reads back its full 64 bits.
        h.write_reg64(F31, 0x7FF8_0000_0000_0000);
        assert_eq!(h.read_reg64(F31), 0x7FF8_0000_0000_0000);

        // The abstract path leaves FS Off; a guest FPR write would set Dirty.
        assert_eq!(
            h.fs(),
            mstatus::FS_OFF,
            "{}: abstract register access leaves FS unchanged",
            isa.name()
        );
        assert_eq!(h.mstatus() & mstatus::SD, 0);
    }
}

#[test]
fn abstract_32_bit_fpr_access_nan_boxes_writes_and_returns_raw_low_bits() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        let mut h = Harness::halted(isa);
        assert_eq!(h.fs(), mstatus::FS_OFF);

        for reg in 0..32u32 {
            let value = 0x3F80_0000u32.wrapping_add(reg);
            h.write_reg32(F0 + reg, value);
            assert_eq!(
                h.fpr_bits(reg as usize),
                NAN_BOX_MASK | u64::from(value),
                "{} f{reg} is stored NaN-boxed",
                isa.name()
            );
            assert_eq!(
                h.read_reg32(F0 + reg),
                value,
                "{} f{reg} 32-bit read",
                isa.name()
            );
            assert_eq!(
                h.read_reg64(F0 + reg),
                NAN_BOX_MASK | u64::from(value),
                "{} f{reg} 64-bit read sees the box",
                isa.name()
            );
        }

        // A raw low read does not apply the computational unboxing rule: an
        // unboxed register returns its low 32 bits, not the canonical NaN.
        let raw = 0x1234_5678_9ABC_DEF0u64;
        h.hart
            .state_mut()
            .fpr
            .write(6, ruscv_sim::fpu::Fpr::from_bits(raw));
        assert_eq!(
            h.read_reg32(F0 + 6),
            raw as u32,
            "{}: raw low 32 bits, unchanged",
            isa.name()
        );
        assert_eq!(
            h.fpr_bits(6),
            raw,
            "the 32-bit read did not rewrite the register"
        );
        assert_eq!(
            h.hart.state().fpr.read(6).as_single_bits(),
            CANONICAL_NAN_F32,
            "negative control: a computational single read would box-fault"
        );

        // A 32-bit write never consults the recorded upper half.
        h.write_reg32(F0 + 7, 0x4040_0000);
        assert_eq!(h.fpr_bits(7), 0xFFFF_FFFF_4040_0000);

        // `aarsize` 4 and 0 stay unsupported options (contract D10: 2 or 3).
        h.command(4, false, F0);
        assert_eq!(h.cmderr(), CMDERR_NOT_SUPPORTED);
        h.clear_cmderr();
        h.command(0, false, F0);
        assert_eq!(h.cmderr(), CMDERR_NOT_SUPPORTED);
        h.clear_cmderr();

        assert_eq!(
            h.fs(),
            mstatus::FS_OFF,
            "{}: abstract access leaves FS unchanged",
            isa.name()
        );
    }
}

/// The abstract FPR transfer is independent of `mstatus.FS` and does not
/// change it in any of the four states. Reading back after each write also
/// proves the transfer was not silently dropped.
#[test]
fn abstract_fpr_transfer_is_independent_of_fs_and_leaves_it_unchanged() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        for fs in [
            mstatus::FS_OFF,
            mstatus::FS_INITIAL,
            mstatus::FS_CLEAN,
            mstatus::FS_DIRTY,
        ] {
            let mut h = Harness::halted(isa);
            h.set_fs(fs);
            let bits = 0x0123_4567_89AB_CDEF;

            h.write_reg64(F0 + 5, bits);
            assert_eq!(h.read_reg64(F0 + 5), bits, "{} FS {fs:#x}", isa.name());
            assert_eq!(h.fs(), fs, "{} FS {fs:#x}: 64-bit write", isa.name());

            h.write_reg32(F0 + 6, 0x3F80_0000);
            assert_eq!(
                h.fpr_bits(6),
                NAN_BOX_MASK | 0x3F80_0000,
                "{} FS {fs:#x}",
                isa.name()
            );
            assert_eq!(h.read_reg32(F0 + 6), 0x3F80_0000);
            assert_eq!(h.fs(), fs, "{} FS {fs:#x}: 32-bit write", isa.name());

            let expected_sd = if fs == mstatus::FS_DIRTY {
                mstatus::SD
            } else {
                0
            };
            assert_eq!(h.mstatus() & mstatus::SD, expected_sd);
        }
    }
}

// ============== D10: abstract FP CSR access ==============

#[test]
fn abstract_fp_csr_access_is_independent_of_fs_and_leaves_fs_unchanged() {
    let mut h = Harness::halted(IsaProfile::Rv64imafdc);
    assert_eq!(h.fs(), mstatus::FS_OFF);

    // `fcsr` = 0xA5: frm = 5, fflags = 0x05. FS stays Off.
    h.write_reg64(u32::from(unprivileged::FCSR), 0xA5);
    assert_eq!(h.fcsr(), 0xA5);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FCSR)), 0xA5);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FFLAGS)), 0x05);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FRM)), 0x05);
    assert_eq!(
        h.fs(),
        mstatus::FS_OFF,
        "an abstract FP CSR write does not set FS Dirty"
    );
    assert_eq!(h.mstatus() & mstatus::SD, 0);

    // The 32-bit form writes the same architectural bits; the reserved
    // `fcsr` bits above 7 still read zero.
    h.write_reg32(u32::from(unprivileged::FFLAGS), 0x1F);
    assert_eq!(h.read_reg32(u32::from(unprivileged::FFLAGS)), 0x1F);
    assert_eq!(h.fcsr(), 0x05 << 5 | 0x1F);
    h.write_reg32(u32::from(unprivileged::FRM), 0x02);
    assert_eq!(
        h.read_reg64(u32::from(unprivileged::FCSR)),
        0x02 << 5 | 0x1F
    );
    h.write_reg64(u32::from(unprivileged::FCSR), 0xFFFF_FFFF_FFFF_FFE3);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FCSR)), 0xE3);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FFLAGS)), 0x03);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FRM)), 0x07);

    // With FS Clean the abstract write still leaves FS Clean (pinned Spike).
    h.set_fs(mstatus::FS_CLEAN);
    h.write_reg64(u32::from(unprivileged::FFLAGS), 0x09);
    assert_eq!(
        h.read_reg64(u32::from(unprivileged::FFLAGS)),
        0x09,
        "the Clean-state abstract write took effect"
    );
    assert_eq!(h.fs(), mstatus::FS_CLEAN);
    assert_eq!(h.mstatus() & mstatus::SD, 0);

    // Contrast: the same write as a program-buffer instruction follows D6
    // and sets FS Dirty.
    h.hart.state_mut().regs[5] = 0x11;
    h.prog(0, csrw(unprivileged::FFLAGS, 5));
    h.postexec();
    h.run_busy();
    assert_eq!(h.cmderr(), 0);
    assert_eq!(h.read_reg64(u32::from(unprivileged::FFLAGS)), 0x11);
    assert_eq!(
        h.fs(),
        mstatus::FS_DIRTY,
        "a guest FP CSR write sets FS Dirty"
    );
}

// ============== D10: no side effects beyond the named register ==============

/// An abstract FP CSR write is independent of `mstatus.FS` and never changes
/// it. The readback after each write proves the value was not dropped.
#[test]
fn abstract_fp_csr_write_is_independent_of_fs() {
    for isa in [IsaProfile::Rv64imafd, IsaProfile::Rv64imafdc] {
        for fs in [
            mstatus::FS_OFF,
            mstatus::FS_INITIAL,
            mstatus::FS_CLEAN,
            mstatus::FS_DIRTY,
        ] {
            let mut h = Harness::halted(isa);
            h.set_fs(fs);

            h.write_reg64(u32::from(unprivileged::FCSR), 0xA5);
            assert_eq!(
                h.read_reg64(u32::from(unprivileged::FCSR)),
                0xA5,
                "{} FS {fs:#x}: fcsr write took effect",
                isa.name()
            );
            assert_eq!(h.fs(), fs, "{} FS {fs:#x}: fcsr write", isa.name());

            h.write_reg32(u32::from(unprivileged::FFLAGS), 0x1F);
            assert_eq!(
                h.read_reg64(u32::from(unprivileged::FFLAGS)),
                0x1F,
                "{} FS {fs:#x}: fflags write took effect",
                isa.name()
            );
            assert_eq!(h.fs(), fs, "{} FS {fs:#x}: fflags write", isa.name());

            let expected_sd = if fs == mstatus::FS_DIRTY {
                mstatus::SD
            } else {
                0
            };
            assert_eq!(h.mstatus() & mstatus::SD, expected_sd);
        }
    }
}

/// Public guest state a debugger transfer must not disturb.
#[derive(Debug, PartialEq, Eq)]
struct GuestSnapshot {
    pc: u64,
    regs: [u64; 32],
    privilege: ruscv_sim::core::PrivilegeMode,
    mstatus: u64,
    mepc: u64,
    mcause: u64,
    mtval: u64,
    minstret: u64,
    mcycle: u64,
    misa: u64,
    dpc: u64,
    dcsr: u64,
    dscratch0: u64,
    dscratch1: u64,
    instruction_length: u8,
    branch_taken: bool,
}

impl Harness {
    fn guest_snapshot(&self) -> GuestSnapshot {
        let state = self.hart.state();
        let read = |addr| state.csr.read_as_machine(addr).unwrap();
        GuestSnapshot {
            pc: state.pc,
            regs: state.regs,
            privilege: state.privilege,
            mstatus: read(machine::MSTATUS),
            mepc: read(machine::MEPC),
            mcause: read(machine::MCAUSE),
            mtval: read(machine::MTVAL),
            minstret: read(machine::MINSTRET),
            mcycle: read(machine::MCYCLE),
            misa: read(machine::MISA),
            dpc: self.hart.dpc(),
            dcsr: self.hart.dcsr(),
            dscratch0: self.hart.dscratch0(),
            dscratch1: self.hart.dscratch1(),
            instruction_length: state.instruction_length,
            branch_taken: state.branch_taken,
        }
    }
}

/// A successful abstract FP transfer changes only FP state, and a rejected
/// transfer under a non-FP profile changes nothing at all. Guest-visible
/// state is seeded non-zero so an accidental clear cannot pass.
#[test]
fn abstract_fp_access_leaves_other_architectural_state_unchanged() {
    let mut h = Harness::halted(IsaProfile::Rv64imafdc);
    h.set_fs(mstatus::FS_CLEAN);
    h.hart.state_mut().pc = BASE + 0x20;
    h.hart.state_mut().regs[1] = 0x1111_2222_3333_4444;
    h.hart.state_mut().regs[5] = 0x5555_6666_7777_8888;
    h.hart.state_mut().regs[31] = 0x9999_AAAA_BBBB_CCCC;
    for (addr, value) in [
        (machine::MEPC, 0x8000_1234),
        (machine::MCAUSE, 0xB),
        (machine::MTVAL, 0xDEAD_BEEF),
        (machine::MINSTRET, 0x77),
        (machine::MCYCLE, 0x99),
    ] {
        h.hart
            .state_mut()
            .csr
            .write_as_machine(addr, value)
            .unwrap();
    }
    h.hart.write_dscratch0(0xAAAA_BBBB_CCCC_DDDD);
    h.hart.write_dscratch1(0xEEEE_FFFF_0000_1111);
    let before = h.guest_snapshot();

    h.write_reg64(F0 + 3, 0x0123_4567_89AB_CDEF);
    h.write_reg32(F0 + 4, 0x3F80_0000);
    h.read_reg64(F0 + 3);
    h.read_reg32(F0 + 4);
    h.write_reg64(u32::from(unprivileged::FCSR), 0xA5);
    h.read_reg64(u32::from(unprivileged::FCSR));
    h.write_reg32(u32::from(unprivileged::FFLAGS), 0x1F);
    h.read_reg64(u32::from(unprivileged::FFLAGS));

    assert_eq!(
        h.guest_snapshot(),
        before,
        "a successful abstract FP access changed non-FP architectural state"
    );

    // The rejected path under a non-FP profile changes nothing either.
    let mut h = Harness::halted(IsaProfile::Rv64ima);
    h.hart.state_mut().pc = BASE + 0x40;
    h.hart.state_mut().regs[1] = 0x1111_2222_3333_4444;
    h.hart
        .state_mut()
        .csr
        .write_as_machine(machine::MEPC, 0x8000_5678)
        .unwrap();
    h.hart.write_dscratch0(0x1234_5678_9ABC_DEF0);
    let before = h.guest_snapshot();
    h.expect_missing(3, true, F0);
    h.expect_missing(3, false, F31);
    h.expect_missing(2, true, F0 + 7);
    h.expect_missing(3, true, u32::from(unprivileged::FCSR));
    h.expect_missing(2, false, u32::from(unprivileged::FFLAGS));
    assert_eq!(
        h.guest_snapshot(),
        before,
        "a rejected abstract transfer changed architectural state"
    );
    for reg in 0..32 {
        assert_eq!(h.fpr_bits(reg), 0, "f{reg}");
    }
    assert_eq!(h.fcsr(), 0);
}

// ============== D10: non-FP profiles are unchanged ==============

#[test]
fn abstract_fp_state_does_not_exist_under_non_fp_profiles() {
    for isa in [IsaProfile::Rv64ima, IsaProfile::Rv64imac] {
        let mut h = Harness::halted(isa);
        assert_eq!(h.fs(), mstatus::FS_OFF);

        for regno in [F0, 0x1027, F31] {
            h.expect_missing(3, false, regno);
            h.expect_missing(3, true, regno);
            h.expect_missing(2, false, regno);
            h.expect_missing(2, true, regno);
        }
        for csr in [unprivileged::FFLAGS, unprivileged::FRM, unprivileged::FCSR] {
            h.expect_missing(3, false, u32::from(csr));
            h.expect_missing(3, true, u32::from(csr));
            h.expect_missing(2, false, u32::from(csr));
            h.expect_missing(2, true, u32::from(csr));
        }

        // No rejected access changed FP state or writable FS bits.
        for reg in 0..32 {
            assert_eq!(h.fpr_bits(reg), 0, "{} f{reg}", isa.name());
        }
        assert_eq!(h.fcsr(), 0, "{}", isa.name());
        assert_eq!(h.fs(), mstatus::FS_OFF, "{}", isa.name());
        h.hart
            .state_mut()
            .csr
            .write_as_machine(machine::MSTATUS, mstatus::FS)
            .unwrap();
        assert_eq!(
            h.fs(),
            mstatus::FS_OFF,
            "{} keeps FS hard-wired to zero",
            isa.name()
        );

        // `transfer` 0 does not check regno (Debug Spec 1.0 §3.7.1.1).
        h.dm.dmi_write(&mut h.hart, COMMAND, (3 << 20) | F0);
        assert_eq!(h.cmderr(), 0);
    }
}

// ============== D10: the DM misa read ==============

#[test]
fn dm_misa_read_reflects_all_four_profiles() {
    let expected = [
        (IsaProfile::Rv64ima, 0x8000_0000_0014_1101),
        (IsaProfile::Rv64imac, 0x8000_0000_0014_1105),
        (IsaProfile::Rv64imafd, 0x8000_0000_0014_1129),
        (IsaProfile::Rv64imafdc, 0x8000_0000_0014_112D),
    ];
    for (isa, misa) in expected {
        let mut h = Harness::halted(isa);
        assert_eq!(
            h.read_reg64(u32::from(machine::MISA)),
            misa,
            "{} misa",
            isa.name()
        );
    }
}

// ============== D10: program-buffer FP instructions ==============

#[test]
fn program_buffer_fp_instruction_retires_with_fs_on() {
    let mut h = Harness::halted(IsaProfile::Rv64imafd);
    h.set_fs(mstatus::FS_INITIAL);
    h.hart.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    h.hart.state_mut().fpr.write_u32(2, 2.0f32.to_bits());

    h.prog(0, fadd_s(3, 1, 2));
    h.postexec();
    h.run_busy();

    assert_eq!(h.cmderr(), 0);
    assert!(!h.busy());
    assert_eq!(
        h.fpr_bits(3),
        NAN_BOX_MASK | 0x4040_0000,
        "fadd.s wrote 3.0f as a NaN-boxed FPR"
    );
    assert_eq!(h.fcsr(), 0, "1.0 + 2.0 accrues no exception flag");
    assert_eq!(
        h.fs(),
        mstatus::FS_DIRTY,
        "the FPR write sets FS Dirty as in guest execution"
    );
}

#[test]
fn program_buffer_fp_instruction_and_fp_csr_trap_with_fs_off() {
    let mut h = Harness::halted(IsaProfile::Rv64imafd);
    assert_eq!(h.fs(), mstatus::FS_OFF);
    h.hart.state_mut().fpr.write_u32(1, 1.0f32.to_bits());
    h.hart.state_mut().fpr.write_u32(2, 2.0f32.to_bits());
    h.hart.state_mut().fpr.write_u32(3, 0xDEAD_BEEF);
    h.hart.state_mut().fcsr.write_flags_bits(0x05);

    h.prog(0, fadd_s(3, 1, 2));
    h.postexec();
    h.run_busy();

    assert_eq!(h.cmderr(), CMDERR_EXCEPTION, "FS Off makes FADD.S trap");
    assert_eq!(h.fpr_bits(3) as u32, 0xDEAD_BEEF, "no FPR effect");
    assert_eq!(h.fcsr(), 0x05, "no fcsr effect");
    assert_eq!(h.fs(), mstatus::FS_OFF);

    // A program-buffer FP CSR instruction follows the same ordinary FS rule.
    h.clear_cmderr();
    h.hart.state_mut().regs[5] = 0x1F;
    h.prog(0, csrw(unprivileged::FFLAGS, 5));
    h.postexec();
    h.run_busy();
    assert_eq!(h.cmderr(), CMDERR_EXCEPTION);
    assert_eq!(
        h.fcsr(),
        0x05,
        "the trapped CSR write left fflags unchanged"
    );
    assert_eq!(h.fs(), mstatus::FS_OFF);
}
