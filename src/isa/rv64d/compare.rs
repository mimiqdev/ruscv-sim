//! D Extension Comparison Instructions (RV64D)
//!
//! Implements FEQ.D, FLT.D, and FLE.D for double-precision floating-point
//! comparisons. These instructions write 1 to rd if the comparison holds,
//! 0 otherwise, and can raise invalid operation exceptions.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;

/// Execute FEQ.D (Floating-point Equal Double)
/// Writes 1 to rd if rs1 equals rs2, 0 otherwise.
pub fn exec_feq_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FEQ.D requires rs1");
    let rs2 = instr.rs2.expect("FEQ.D requires rs2");
    let rd = instr.rd.expect("FEQ.D requires rd");

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

    // Check for NaN comparisons
    if val1.is_nan() || val2.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0;
    } else {
        state.regs[rd as usize] = if val1 == val2 { 1 } else { 0 };
    }

    Ok(())
}

/// Execute FLT.D (Floating-point Less Than Double)
/// Writes 1 to rd if rs1 < rs2, 0 otherwise.
pub fn exec_flt_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FLT.D requires rs1");
    let rs2 = instr.rs2.expect("FLT.D requires rs2");
    let rd = instr.rd.expect("FLT.D requires rd");

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

    // Check for NaN comparisons
    if val1.is_nan() || val2.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0;
    } else {
        state.regs[rd as usize] = if val1 < val2 { 1 } else { 0 };
    }

    Ok(())
}

/// Execute FLE.D (Floating-point Less Than or Equal Double)
/// Writes 1 to rd if rs1 <= rs2, 0 otherwise.
pub fn exec_fle_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FLE.D requires rs1");
    let rs2 = instr.rs2.expect("FLE.D requires rs2");
    let rd = instr.rd.expect("FLE.D requires rd");

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());

    // Check for NaN comparisons
    if val1.is_nan() || val2.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0;
    } else {
        state.regs[rd as usize] = if val1 <= val2 { 1 } else { 0 };
    }

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
    fn test_feq_d_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

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

        exec_feq_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_feq_d_not_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

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

        exec_feq_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_flt_d_less() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

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

        exec_flt_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_flt_d_greater() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

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

        exec_flt_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_fle_d_less_or_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

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

        exec_fle_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_fle_d_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

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

        exec_fle_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_feq_d_nan() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));
        state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

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

        exec_feq_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 0);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_flt_d_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-5.0f64).to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

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

        exec_flt_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_fle_d_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-5.0f64).to_bits()));
        state.fpr.write(2, Fpr::from_bits((-3.0f64).to_bits()));

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

        exec_fle_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 1);
    }
}
