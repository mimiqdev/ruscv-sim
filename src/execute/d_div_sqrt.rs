//! D Extension Division and Square Root (RV64D)
//!
//! Implements FDIV.D and FSQRT.D for double-precision floating-point
//! division and square root operations.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::{FpFlags, RoundingMode};
use crate::fpu::Fpr;
use crate::MemoryInterface;

/// Apply rounding mode to result (f64 version)
fn apply_rounding_d(result: f64, rm: RoundingMode) -> f64 {
    match rm {
        RoundingMode::RNE => result,
        RoundingMode::RTZ => result.trunc(),
        RoundingMode::RDN => result.floor(),
        RoundingMode::RUP => result.ceil(),
        RoundingMode::RMM => result,
    }
}

/// Execute FDIV.D (Floating-point Divide Double)
/// Format: R-type
/// Encoding: | funct7=0001101 | rs2 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fdiv_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FDIV.D requires rs1");
    let rs2 = instr.rs2.expect("FDIV.D requires rs2");
    let rd = instr.rd.expect("FDIV.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

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
        state.fcsr.set_flag(flags);
    }

    let result = val1 / val2;
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    // Check for inexact result using FMA
    // If result * val2 - val1 != 0 (computed with FMA), division was inexact
    if !result.is_nan() && !result.is_infinite() && result.mul_add(val2, -val1) != 0.0 {
        state.fcsr.set_flag(FpFlags::NX);
    }

    // Handle division by zero result (inf or -inf)
    if val2 == 0.0 && !val1.is_infinite() {
        let signed_result = if val1.signum() < 0.0 {
            -f64::INFINITY
        } else {
            f64::INFINITY
        };
        state
            .fpr
            .write(rd as usize, Fpr::from_bits(signed_result.to_bits()));
    } else {
        state
            .fpr
            .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    }

    Ok(())
}

/// Execute FSQRT.D (Floating-point Square Root Double)
/// Format: R-type (rs2=0)
/// Encoding: | funct7=0101101 | rs2=0 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fsqrt_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FSQRT.D requires rs1");
    let rd = instr.rd.expect("FSQRT.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());

    // Check for special cases
    let mut flags = FpFlags::empty();

    // Square root of negative (non-zero) = NaN
    if val1 < 0.0 && !val1.is_infinite() {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    let result = val1.sqrt();
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::fpu::Fpr;
    use crate::memory::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fdiv_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(10.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0D),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_fdiv_d_by_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(0.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0D),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!(result.is_infinite());
        assert!(state.fcsr.flags().contains(FpFlags::DZ));
    }

    #[test]
    fn test_fsqrt_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(16.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2D),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!((result - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_fsqrt_d_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(0.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2D),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_fsqrt_d_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-4.0f64).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x2D),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fsqrt_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!(result.is_nan());
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fdiv_d_inf_div_inf() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
        state.fpr.write(2, Fpr::from_bits(f64::INFINITY.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0D),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!(result.is_nan());
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fdiv_d_precision() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 1.0 / 3.0 = 0.333...
        state.fpr.write(1, Fpr::from_bits(1.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x0D),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fdiv_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        let expected = 1.0 / 3.0;
        assert!((result - expected).abs() < 1e-15);
        // Division by 3 should set inexact flag
        assert!(state.fcsr.flags().contains(FpFlags::NX));
    }
}
