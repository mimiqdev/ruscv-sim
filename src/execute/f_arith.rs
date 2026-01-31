//! Floating-point Arithmetic Instructions (RV64F)
//!
//! Implements FADD.S, FSUB.S, and FMUL.S for single-precision floating-point
//! arithmetic operations.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;
use crate::fpu::Fpr;
use crate::MemoryInterface;

/// Apply rounding mode to result
fn apply_rounding(result: f32, rm: u8) -> f32 {
    match rm {
        1 => result.trunc(), // RTZ
        2 => result.floor(), // RDN
        3 => result.ceil(),  // RUP
        _ => result,         // RNE or others
    }
}

/// Execute FADD.S (Floating-point Add Single)
/// Format: R4-type (for FMA) but we handle as R-type with rs3=0
/// Encoding: | funct7=0000000 | rs2 | rs1 | funct3=rd'| rs3 | opcode=OpFp(1010011) |
pub fn exec_fadd_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FADD.S requires rs1");
    let rs2 = instr.rs2.expect("FADD.S requires rs2");
    let rd = instr.rd.expect("FADD.S requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    let result = val1 + val2;
    let rounded = apply_rounding(result, rm);

    // Check for inexact result
    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    // Check for overflow/underflow
    if result.is_infinite() && !val1.is_infinite() && !val2.is_infinite() {
        state.fcsr.set_flag(FpFlags::OF);
    }
    if result.is_normal() && !val1.is_normal() && !val2.is_normal() {
        state.fcsr.set_flag(FpFlags::UF);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

/// Execute FSUB.S (Floating-point Subtract Single)
/// Format: R4-type
pub fn exec_fsub_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FSUB.S requires rs1");
    let rs2 = instr.rs2.expect("FSUB.S requires rs2");
    let rd = instr.rd.expect("FSUB.S requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    let result = val1 - val2;
    let rounded = apply_rounding(result, rm);

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

/// Execute FMUL.S (Floating-point Multiply Single)
/// Format: R4-type
pub fn exec_fmul_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMUL.S requires rs1");
    let rs2 = instr.rs2.expect("FMUL.S requires rs2");
    let rd = instr.rd.expect("FMUL.S requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    let result = val1 * val2;
    let rounded = apply_rounding(result, rm);

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{InstructionFormat, Opcode};
    use crate::fpu::Fpr;
    use crate::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fadd_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(1.5));
        state.fpr.write(2, Fpr::new(2.5));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!((result - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_fsub_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(5.0));
        state.fpr.write(2, Fpr::new(3.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fsub_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!((result - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_fmul_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(2.0));
        state.fpr.write(2, Fpr::new(3.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!((result - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_fadd_s_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-1.5));
        state.fpr.write(2, Fpr::new(2.5));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_fmul_s_zero_multiply() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(0.0));
        state.fpr.write(2, Fpr::new(3.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(3).get();
        assert_eq!(result, 0.0);
    }
}
