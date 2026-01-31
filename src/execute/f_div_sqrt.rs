//! Floating-point Division and Square Root (RV64F)
//!
//! Implements FDIV.S and FSQRT.S for single-precision floating-point
//! division and square root operations.

use crate::core::CoreState;
use crate::decode::InstructionFormat;
use crate::decode::{DecodedInstruction, Opcode};
use crate::fpu::fcsr::{FpFlags, RoundingMode};
use crate::fpu::Fpr;
use crate::execute::ExecuteError;
use crate::memory::{MemoryError, MemoryInterface};

/// Apply rounding mode to result
fn apply_rounding(result: f32, rm: RoundingMode) -> f32 {
    match rm {
        RoundingMode::RNE => result,
        RoundingMode::RTZ => result.trunc(),
        RoundingMode::RDN => result.floor(),
        RoundingMode::RUP => result.ceil(),
        RoundingMode::RMM => result,
    }
}

/// Execute FDIV.S (Floating-point Divide Single)
/// Format: R-type
/// Encoding: | funct7=0001100 | rs2 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fdiv_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FDIV.S requires rs1");
    let rs2 = instr.rs2.expect("FDIV.S requires rs2");
    let rd = instr.rd.expect("FDIV.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    // Divide by zero
    if val2 == 0.0 && !val1.is_infinite() {
        flags.insert(FpFlags::DZ);
    }

    // Infinity / infinity = NaN
    if val1.is_infinite() && val2.is_infinite() {
        flags.insert(FpFlags::NV);
    }

    // 0/0 = NaN
    if val1 == 0.0 && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    // 0/inf = NaN
    if val1 == 0.0 && val2.is_infinite() {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    let result = val1 / val2;
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    // Check for inexact result
    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    // Handle division by zero result (inf or -inf)
    if val2 == 0.0 && !val1.is_infinite() {
        let signed_result = if val1.signum() < 0.0 { -f32::INFINITY } else { f32::INFINITY };
        state.fpr.write(rd as usize, Fpr::new(signed_result));
    } else {
        state.fpr.write(rd as usize, Fpr::new(rounded));
    }

    Ok(())
}

/// Execute FSQRT.S (Floating-point Square Root Single)
/// Format: R-type (rs2=0)
/// Encoding: | funct7=0101100 | rs2=0 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fsqrt_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FSQRT.S requires rs1");
    let rd = instr.rd.expect("FSQRT.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    // Square root of negative (non-zero) = NaN
    if val1 < 0.0 && !val1.is_infinite() {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    let result = val1.sqrt();
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fdiv_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(10.0));
        state.fpr.write(2, Fpr::new(2.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0C),
            rs1: Some(1),
            rs2: Some(2),
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!((result - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_fdiv_s_by_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(5.0));
        state.fpr.write(2, Fpr::new(0.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0C),
            rs1: Some(1),
            rs2: Some(2),
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!(result.is_infinite());
        assert!(state.fpr.fcsr.flags().contains(FpFlags::DZ));
    }

    #[test]
    fn test_fsqrt_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(16.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2C),
            rs1: Some(1),
            rs2: Some(0),
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!((result - 4.0).abs() < 1e-5);
    }

    #[test]
    fn test_fsqrt_s_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(0.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2C),
            rs1: Some(1),
            rs2: Some(0),
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_fsqrt_s_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-4.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2C),
            rs1: Some(1),
            rs2: Some(0),
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!(result.is_nan());
        assert!(state.fpr.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fdiv_s_inf_div_inf() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::INFINITY));
        state.fpr.write(2, Fpr::new(f32::INFINITY));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0C),
            rs1: Some(1),
            rs2: Some(2),
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!(result.is_nan());
        assert!(state.fpr.fcsr.flags().contains(FpFlags::NV));
    }
}
