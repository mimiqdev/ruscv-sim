//! Floating-point Classification Instruction (RV64F)
//!
//! Implements FCLASS.S for classifying single-precision floating-point values.
//! Returns a 10-bit mask indicating the classification of the value.

use crate::core::CoreState;
use crate::decode::InstructionFormat;
use crate::decode::{DecodedInstruction, Opcode};
use crate::execute::ExecuteError;
use crate::fpu::Fpr;

/// FCLASS.S result values (bit positions):
/// 0: -inf, 1: -normal, 2: -subnormal, 3: -zero, 4: +zero, 5: +subnormal, 6: +normal, 7: +inf, 8: NaN, 9: NaN (quiet/signaling)

/// Execute FCLASS.S (Classify Single Precision)
/// Writes a 10-bit mask to rd indicating the classification of rs1
pub fn exec_fclass_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCLASS.S requires rs1");
    let rd = instr.rd.expect("FCLASS.S requires rd");

    let val = state.fpr.read(rs1 as usize).get();

    let result = if val == f32::NEG_INFINITY {
        1 << 0 // -infinity
    } else if val < 0.0 && val.is_normal() {
        1 << 1 // -normal
    } else if val < 0.0 && val.is_subnormal() {
        1 << 2 // -subnormal
    } else if val == 0.0 && val.is_sign_negative() {
        1 << 3 // -zero
    } else if val == 0.0 {
        1 << 4 // +zero
    } else if val > 0.0 && val.is_subnormal() {
        1 << 5 // +subnormal
    } else if val > 0.0 && val.is_normal() {
        1 << 6 // +normal
    } else if val == f32::INFINITY {
        1 << 7 // +infinity
    } else {
        1 << 9 // NaN (quiet or signaling)
    };

    state.regs[rd as usize] = result;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fclass_s_neg_inf() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::NEG_INFINITY));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 0);
    }

    #[test]
    fn test_fclass_s_pos_inf() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::INFINITY));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 7);
    }

    #[test]
    fn test_fclass_s_neg_normal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-3.5));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 1);
    }

    #[test]
    fn test_fclass_s_pos_normal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(3.5));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 6);
    }

    #[test]
    fn test_fclass_s_neg_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-0.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 3);
    }

    #[test]
    fn test_fclass_s_pos_zero() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(0.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 4);
    }

    #[test]
    fn test_fclass_s_nan() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::NAN));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fclass_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1 << 9);
    }
}
