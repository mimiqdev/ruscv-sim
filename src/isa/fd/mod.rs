//! F and D floating-point instruction execution (contract U3a, D4-D7).
//!
//! Every F/D instruction that the decoder admits executes here, on the
//! ordinary Hart path, through the bit-exact arithmetic core in
//! [`crate::fpu::arith`]. The arithmetic core owns rounding, exception
//! detection and canonical NaN results; this module owns the architectural
//! register interface:
//!
//! - computational single-precision reads are NaN-box checked, while
//!   transfers (`FSW`, `FSD`, `FMV.X.*`) use the register's raw bits;
//! - every 32-bit FPR write NaN-boxes its result;
//! - accrued exception flags are OR-ed into `fcsr`;
//! - an instruction that writes an FPR, or accrues any nonzero flag set,
//!   leaves `mstatus.FS` Dirty (contract D6).
//!
//! The Hart checks the profile and `mstatus.FS` before this module runs.
//! This module owns the rounding-mode rule (contract D5) and validates it
//! before writing any architectural state. Any error returned here is an
//! illegal-instruction trap, never a [`crate::core::SimulatorFailure`].

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, FpFormat, FpInstruction, FpOp, IntWidth};
use crate::execute::ExecuteError;
use crate::fpu::arith::{self, FpFlags, Rounding};
use crate::fpu::Fpr;
use crate::memory::MemoryInterface;

/// Execute one decoded F/D instruction and return its integer destination.
///
/// A GPR-writing instruction returns `Some(rd)` so the Hart records the
/// destination fact; every other instruction returns `None`.
pub fn execute(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<Option<u8>, ExecuteError> {
    let fp = instr.fp.as_ref().ok_or(ExecuteError::IllegalInstruction)?;
    let rounding = resolve_rounding(fp, state)?;
    let rs1 = instr.rs1.unwrap_or(0);
    let rs2 = instr.rs2.unwrap_or(0);
    let rd = instr.rd.unwrap_or(0);

    match fp.op {
        FpOp::Add | FpOp::Sub | FpOp::Mul | FpOp::Div | FpOp::Sqrt => {
            let rm = rounding.expect("arithmetic carries an rm field");
            match fp.fmt {
                FpFormat::Single => {
                    let a = single_operand(state, rs1);
                    let (result, flags) = match fp.op {
                        FpOp::Add => arith::single::add(a, single_operand(state, rs2), rm),
                        FpOp::Sub => arith::single::sub(a, single_operand(state, rs2), rm),
                        FpOp::Mul => arith::single::mul(a, single_operand(state, rs2), rm),
                        FpOp::Div => arith::single::div(a, single_operand(state, rs2), rm),
                        _ => arith::single::sqrt(a, rm),
                    };
                    write_single(state, rd, result);
                    accrue(state, flags);
                }
                FpFormat::Double => {
                    let a = double_operand(state, rs1);
                    let (result, flags) = match fp.op {
                        FpOp::Add => arith::double::add(a, double_operand(state, rs2), rm),
                        FpOp::Sub => arith::double::sub(a, double_operand(state, rs2), rm),
                        FpOp::Mul => arith::double::mul(a, double_operand(state, rs2), rm),
                        FpOp::Div => arith::double::div(a, double_operand(state, rs2), rm),
                        _ => arith::double::sqrt(a, rm),
                    };
                    write_double(state, rd, result);
                    accrue(state, flags);
                }
            }
        }
        FpOp::Madd | FpOp::Msub | FpOp::Nmsub | FpOp::Nmadd => {
            let rm = rounding.expect("fused multiply-add carries an rm field");
            let rs3 = instr.rs3.unwrap_or(0);
            match fp.fmt {
                FpFormat::Single => {
                    let (a, b, c) = (
                        single_operand(state, rs1),
                        single_operand(state, rs2),
                        single_operand(state, rs3),
                    );
                    let (result, flags) = match fp.op {
                        FpOp::Madd => arith::single::fmadd(a, b, c, rm),
                        FpOp::Msub => arith::single::fmsub(a, b, c, rm),
                        FpOp::Nmsub => arith::single::fnmsub(a, b, c, rm),
                        _ => arith::single::fnmadd(a, b, c, rm),
                    };
                    write_single(state, rd, result);
                    accrue(state, flags);
                }
                FpFormat::Double => {
                    let (a, b, c) = (
                        double_operand(state, rs1),
                        double_operand(state, rs2),
                        double_operand(state, rs3),
                    );
                    let (result, flags) = match fp.op {
                        FpOp::Madd => arith::double::fmadd(a, b, c, rm),
                        FpOp::Msub => arith::double::fmsub(a, b, c, rm),
                        FpOp::Nmsub => arith::double::fnmsub(a, b, c, rm),
                        _ => arith::double::fnmadd(a, b, c, rm),
                    };
                    write_double(state, rd, result);
                    accrue(state, flags);
                }
            }
        }
        FpOp::Sgnj | FpOp::Sgnjn | FpOp::Sgnjx => match fp.fmt {
            FpFormat::Single => {
                let (a, b) = (single_operand(state, rs1), single_operand(state, rs2));
                let result = match fp.op {
                    FpOp::Sgnj => arith::single::fsgnj(a, b),
                    FpOp::Sgnjn => arith::single::fsgnjn(a, b),
                    _ => arith::single::fsgnjx(a, b),
                };
                write_single(state, rd, result);
            }
            FpFormat::Double => {
                let (a, b) = (double_operand(state, rs1), double_operand(state, rs2));
                let result = match fp.op {
                    FpOp::Sgnj => arith::double::fsgnj(a, b),
                    FpOp::Sgnjn => arith::double::fsgnjn(a, b),
                    _ => arith::double::fsgnjx(a, b),
                };
                write_double(state, rd, result);
            }
        },
        FpOp::Min | FpOp::Max => match fp.fmt {
            FpFormat::Single => {
                let (a, b) = (single_operand(state, rs1), single_operand(state, rs2));
                let (result, flags) = if fp.op == FpOp::Min {
                    arith::single::min(a, b)
                } else {
                    arith::single::max(a, b)
                };
                write_single(state, rd, result);
                accrue(state, flags);
            }
            FpFormat::Double => {
                let (a, b) = (double_operand(state, rs1), double_operand(state, rs2));
                let (result, flags) = if fp.op == FpOp::Min {
                    arith::double::min(a, b)
                } else {
                    arith::double::max(a, b)
                };
                write_double(state, rd, result);
                accrue(state, flags);
            }
        },
        FpOp::Eq | FpOp::Lt | FpOp::Le => {
            let (value, flags) = match fp.fmt {
                FpFormat::Single => {
                    let (a, b) = (single_operand(state, rs1), single_operand(state, rs2));
                    match fp.op {
                        FpOp::Eq => arith::single::eq(a, b),
                        FpOp::Lt => arith::single::lt(a, b),
                        _ => arith::single::le(a, b),
                    }
                }
                FpFormat::Double => {
                    let (a, b) = (double_operand(state, rs1), double_operand(state, rs2));
                    match fp.op {
                        FpOp::Eq => arith::double::eq(a, b),
                        FpOp::Lt => arith::double::lt(a, b),
                        _ => arith::double::le(a, b),
                    }
                }
            };
            state.regs[rd as usize] = u64::from(value);
            accrue(state, flags);
        }
        FpOp::Classify => {
            state.regs[rd as usize] = match fp.fmt {
                FpFormat::Single => u64::from(arith::single::classify(single_operand(state, rs1))),
                FpFormat::Double => arith::double::classify(double_operand(state, rs1)),
            };
            // FCLASS raises no exception and writes no FPR, so FS is
            // unchanged (contract D6).
        }
        FpOp::CvtToInt { width, signed } => {
            let rm = rounding.expect("FCVT to integer carries an rm field");
            // The arithmetic core sign-extends every 32-bit result to XLEN.
            let (value, flags) = match fp.fmt {
                FpFormat::Single => {
                    let a = single_operand(state, rs1);
                    match (width, signed) {
                        (IntWidth::Word, true) => arith::single::to_i32(a, rm),
                        (IntWidth::Word, false) => arith::single::to_ui32(a, rm),
                        (IntWidth::Long, true) => arith::single::to_i64(a, rm),
                        (IntWidth::Long, false) => arith::single::to_ui64(a, rm),
                    }
                }
                FpFormat::Double => {
                    let a = double_operand(state, rs1);
                    match (width, signed) {
                        (IntWidth::Word, true) => arith::double::to_i32(a, rm),
                        (IntWidth::Word, false) => arith::double::to_ui32(a, rm),
                        (IntWidth::Long, true) => arith::double::to_i64(a, rm),
                        (IntWidth::Long, false) => arith::double::to_ui64(a, rm),
                    }
                }
            };
            state.regs[rd as usize] = value;
            accrue(state, flags);
        }
        FpOp::CvtFromInt { width, signed } => {
            let rm = rounding.expect("FCVT from integer carries an rm field");
            let source = state.regs[rs1 as usize];
            match fp.fmt {
                FpFormat::Single => {
                    let (result, flags) = match (width, signed) {
                        (IntWidth::Word, true) => arith::single::from_i32(source as i32, rm),
                        (IntWidth::Word, false) => arith::single::from_ui32(source as u32, rm),
                        (IntWidth::Long, true) => arith::single::from_i64(source as i64, rm),
                        (IntWidth::Long, false) => arith::single::from_ui64(source, rm),
                    };
                    write_single(state, rd, result);
                    accrue(state, flags);
                }
                FpFormat::Double => {
                    let (result, flags) = match (width, signed) {
                        (IntWidth::Word, true) => arith::double::from_i32(source as i32, rm),
                        (IntWidth::Word, false) => arith::double::from_ui32(source as u32, rm),
                        (IntWidth::Long, true) => arith::double::from_i64(source as i64, rm),
                        (IntWidth::Long, false) => arith::double::from_ui64(source, rm),
                    };
                    write_double(state, rd, result);
                    accrue(state, flags);
                }
            }
        }
        FpOp::CvtToSingle => {
            let rm = rounding.expect("FCVT.S.D carries an rm field");
            let (result, flags) = arith::single::from_double(double_operand(state, rs1), rm);
            write_single(state, rd, result);
            accrue(state, flags);
        }
        FpOp::CvtToDouble => {
            // FCVT.D.S is exact; the `rm` field is decoded and validated but
            // the widening result never depends on it (contract D4).
            let (result, flags) = arith::double::from_single(single_operand(state, rs1));
            write_double(state, rd, result);
            accrue(state, flags);
        }
        FpOp::MoveToGpr => match fp.fmt {
            FpFormat::Single => {
                // FMV.X.W is a transfer: raw low 32 bits, sign-extended to
                // XLEN (contract D5).
                let raw = state.fpr.read_raw_u32(rs1 as usize);
                state.regs[rd as usize] = (raw as i32 as i64) as u64;
            }
            FpFormat::Double => {
                state.regs[rd as usize] = double_operand(state, rs1);
            }
        },
        FpOp::MoveFromGpr => match fp.fmt {
            FpFormat::Single => write_single(state, rd, state.regs[rs1 as usize] as u32),
            FpFormat::Double => write_double(state, rd, state.regs[rs1 as usize]),
        },
        FpOp::Load => {
            let address = effective_address(instr, state)?;
            match fp.fmt {
                FpFormat::Single => {
                    // FLW NaN-boxes its result (contract D5).
                    write_single(state, rd, mem.read_word(address)?);
                }
                FpFormat::Double => {
                    write_double(state, rd, mem.read_dword(address)?);
                }
            }
        }
        FpOp::Store => {
            let address = effective_address(instr, state)?;
            match fp.fmt {
                FpFormat::Single => {
                    // FSW is a transfer: the raw low 32 bits (contract D5).
                    mem.write_word(address, state.fpr.read_raw_u32(rs2 as usize))?;
                }
                FpFormat::Double => {
                    mem.write_dword(address, double_operand(state, rs2))?;
                }
            }
            // A store writes no FPR and raises no flag, so FS is unchanged.
        }
    }

    Ok(fp.op.writes_gpr().then_some(rd))
}

/// Resolve the instruction's `rm` field to an implemented rounding mode.
///
/// This is the single D5 rounding-mode check: the Hart's pre-check covers
/// profile, `mstatus.FS` and the format only. The error here is an
/// illegal-instruction trap, never a simulator failure; because the caller
/// runs on staged Hart state, the trap leaves no architectural effect.
fn resolve_rounding(
    fp: &FpInstruction,
    state: &CoreState,
) -> Result<Option<Rounding>, ExecuteError> {
    if !fp.op.requires_rounding() {
        return Ok(None);
    }
    let raw = fp.rm.ok_or(ExecuteError::IllegalInstruction)?;
    let resolved = if raw == 7 { state.fcsr.frm_bits() } else { raw };
    Rounding::from_encoding(resolved)
        .map(Some)
        .ok_or(ExecuteError::IllegalInstruction)
}

/// Computational single-precision operand read (contract D5).
///
/// An FPR whose upper 32 bits are not all ones yields the canonical NaN.
fn single_operand(state: &CoreState, reg: u8) -> u32 {
    state.fpr.read_single_bits(reg as usize)
}

/// Double-precision operand read: the register's full 64-bit content.
fn double_operand(state: &CoreState, reg: u8) -> u64 {
    state.fpr.read(reg as usize).bits()
}

/// Write a 32-bit result, NaN-boxed, and mark `mstatus.FS` Dirty.
fn write_single(state: &mut CoreState, rd: u8, result: u32) {
    state.fpr.write_u32(rd as usize, result);
    state.mark_fs_dirty();
}

/// Write a 64-bit result and mark `mstatus.FS` Dirty.
fn write_double(state: &mut CoreState, rd: u8, result: u64) {
    state.fpr.write(rd as usize, Fpr::from_bits(result));
    state.mark_fs_dirty();
}

/// Accrue this instruction's exception flags and apply the D6 Dirty rule.
///
/// Flags are sticky. A nonzero accrual marks FS Dirty even when every
/// raised flag was already set, so the write cannot be inferred from a
/// `fcsr` value change.
fn accrue(state: &mut CoreState, flags: FpFlags) {
    if flags.is_empty() {
        return;
    }
    state
        .fcsr
        .write_flags_bits(state.fcsr.flags_bits() | flags.bits());
    state.mark_fs_dirty();
}

/// Effective address of an FP load or store.
fn effective_address(instr: &DecodedInstruction, state: &CoreState) -> Result<u64, ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::IllegalInstruction)?;
    let imm = instr.imm.ok_or(ExecuteError::IllegalInstruction)?;
    let offset = ((imm as i32) << 20 >> 20) as i64 as u64;
    Ok(state.regs[rs1 as usize].wrapping_add(offset))
}
