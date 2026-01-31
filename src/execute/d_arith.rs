//! D Extension Arithmetic Instructions (RV64D)
//!
//! Implements FADD.D, FSUB.D, and FMUL.D for double-precision floating-point
//! arithmetic operations.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;
use crate::fpu::Fpr;
use crate::MemoryInterface;

/// Apply rounding mode to result (f64 version)
fn apply_rounding_d(result: f64, rm: u8) -> f64 {
    match rm {
        1 => result.trunc(), // RTZ
        2 => result.floor(), // RDN
        3 => result.ceil(),  // RUP
        _ => result,         // RNE or others
    }
}

/// Execute FADD.D (Floating-point Add Double)
/// Format: R-type
/// Encoding: | funct7=0000001 | rs2 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fadd_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FADD.D requires rs1");
    let rs2 = instr.rs2.expect("FADD.D requires rs2");
    let rd = instr.rd.expect("FADD.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

    let result = val1 + val2;
    let rounded = apply_rounding_d(result, rm);

    // Check for inexact result
    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    // Check for overflow
    if result.is_infinite() && !val1.is_infinite() && !val2.is_infinite() {
        state.fcsr.set_flag(FpFlags::OF);
    }
    if result.is_normal() && !val1.is_normal() && !val2.is_normal() {
        state.fcsr.set_flag(FpFlags::UF);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

/// Execute FSUB.D (Floating-point Subtract Double)
/// Format: R-type
/// Encoding: | funct7=0000101 | rs2 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fsub_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FSUB.D requires rs1");
    let rs2 = instr.rs2.expect("FSUB.D requires rs2");
    let rd = instr.rd.expect("FSUB.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

    let result = val1 - val2;
    let rounded = apply_rounding_d(result, rm);

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

/// Execute FMUL.D (Floating-point Multiply Double)
/// Format: R-type
/// Encoding: | funct7=0001001 | rs2 | rs1 | funct3=rd' | rd | opcode=OpFp(1010011) |
pub fn exec_fmul_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMUL.D requires rs1");
    let rs2 = instr.rs2.expect("FMUL.D requires rs2");
    let rd = instr.rd.expect("FMUL.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

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
    let rounded = apply_rounding_d(result, rm);

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
    fn test_fadd_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state
            .fpr
            .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));
        state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - (std::f64::consts::PI + 1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fsub_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(10.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.5f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x05),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fsub_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - 6.5).abs() < 1e-10);
    }

    #[test]
    fn test_fmul_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(4.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(2.5f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x09),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_fadd_d_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-3.0f64).to_bits()));
        state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - (-2.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fmul_d_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(0.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(999.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x09),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_fadd_d_infinity() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
        state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert_eq!(result, f64::INFINITY);
    }

    #[test]
    fn test_fmul_d_infinity() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
        state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x09),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert_eq!(result, f64::INFINITY);
    }

    #[test]
    fn test_chained_operations() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // ((1.0 + 2.0) * 3.0) - 4.0 = 5.0
        state.fpr.write(1, Fpr::from_bits(1.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(4, Fpr::from_bits(4.0f64.to_bits()));

        // Add
        let add_dec = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(5),
            imm: None,
            branch_taken: false,
        };
        exec_fadd_d(&add_dec, &mut state, &mut mem).unwrap();

        // Multiply
        let mul_dec = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x09),
            rs1: Some(5),
            rs2: Some(3),
            rs3: None,
            rd: Some(6),
            imm: None,
            branch_taken: false,
        };
        exec_fmul_d(&mul_dec, &mut state, &mut mem).unwrap();

        // Subtract
        let sub_dec = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x05),
            rs1: Some(6),
            rs2: Some(4),
            rs3: None,
            rd: Some(7),
            imm: None,
            branch_taken: false,
        };
        exec_fsub_d(&sub_dec, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(7).bits());
        assert!((result - 5.0).abs() < 1e-10);
    }
}
