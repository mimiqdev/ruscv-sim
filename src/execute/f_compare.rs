//! Floating-point Comparison Instructions (RV64F)
//!
//! Implements FEQ.S, FLT.S, and FLE.S for single-precision floating-point
//! comparisons. These instructions write 1 to rd if the comparison holds,
//! 0 otherwise, and can raise invalid operation exceptions.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;

/// Execute FEQ.S (Floating-point Equal Single)
/// Writes 1 to rd if rs1 equals rs2, 0 otherwise.
pub fn exec_feq_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FEQ.S requires rs1");
    let rs2 = instr.rs2.expect("FEQ.S requires rs2");
    let rd = instr.rd.expect("FEQ.S requires rd");

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    // Check for NaN comparisons
    if val1.is_nan() || val2.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0;
    } else {
        state.regs[rd as usize] = if val1 == val2 { 1 } else { 0 };
    }

    Ok(())
}

/// Execute FLT.S (Floating-point Less Than Single)
/// Writes 1 to rd if rs1 < rs2, 0 otherwise.
pub fn exec_flt_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FLT.S requires rs1");
    let rs2 = instr.rs2.expect("FLT.S requires rs2");
    let rd = instr.rd.expect("FLT.S requires rd");

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

    // Check for NaN comparisons
    if val1.is_nan() || val2.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0;
    } else {
        state.regs[rd as usize] = if val1 < val2 { 1 } else { 0 };
    }

    Ok(())
}

/// Execute FLE.S (Floating-point Less Than or Equal Single)
/// Writes 1 to rd if rs1 <= rs2, 0 otherwise.
pub fn exec_fle_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FLE.S requires rs1");
    let rs2 = instr.rs2.expect("FLE.S requires rs2");
    let rd = instr.rd.expect("FLE.S requires rd");

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();

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
    use crate::decode::{InstructionFormat, Opcode};
    use crate::fpu::Fpr;
    use crate::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_feq_s_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(5.0));
        state.fpr.write(2, Fpr::new(5.0));

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

        exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_feq_s_not_equal() {
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

        exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_flt_s_less() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(3.0));
        state.fpr.write(2, Fpr::new(5.0));

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

        exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_flt_s_greater() {
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

        exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_fle_s_less_or_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(3.0));
        state.fpr.write(2, Fpr::new(5.0));

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

        exec_fle_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_fle_s_equal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(5.0));
        state.fpr.write(2, Fpr::new(5.0));

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

        exec_fle_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_feq_s_nan() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::NAN));
        state.fpr.write(2, Fpr::new(5.0));

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

        exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_flt_s_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-5.0));
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

        exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }
}
