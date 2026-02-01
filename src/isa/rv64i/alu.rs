//! RV64I ALU Operations
//!
//! This module implements the arithmetic and logical operations for RV64I:
//! - Register operations: ADD, SUB, SLT, SLTU, XOR, OR, AND
//! - Immediate operations: ADDI, SLTI, SLTIU, XORI, ORI, ANDI

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;

/// Execute register-register ALU operations (RV64I R-type)
///
/// # Operations
/// - ADD/SUB: Addition/Subtraction (distinguished by funct7)
/// - SLT/SLTU: Set Less Than (signed/unsigned)
/// - XOR: Exclusive OR
/// - OR: Logical OR
/// - AND: Logical AND
#[inline]
pub fn exec_op(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(rs2), Some(funct3), Some(funct7)) =
        (instr.rd, instr.rs1, instr.rs2, instr.funct3, instr.funct7)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let rs1_val = state.regs[rs1 as usize] as i64;
    let rs2_val = state.regs[rs2 as usize] as i64;
    let mut result: i64 = 0;

    // ADD/SUB
    if funct3 == Funct3::AddSub {
        if funct7 == 0 {
            result = rs1_val.wrapping_add(rs2_val);
        } else if funct7 == 0x20 {
            result = rs1_val.wrapping_sub(rs2_val);
        }
    }
    // SLT (set less than)
    else if funct3 == Funct3::Slt {
        result = if rs1_val < rs2_val { 1 } else { 0 };
    }
    // SLTU (set less than unsigned)
    else if funct3 == Funct3::Sltu {
        let rs1_u = state.regs[rs1 as usize];
        let rs2_u = state.regs[rs2 as usize];
        result = if rs1_u < rs2_u { 1 } else { 0 };
    }
    // XOR
    else if funct3 == Funct3::Xor {
        result = rs1_val ^ rs2_val;
    }
    // OR
    else if funct3 == Funct3::Or {
        result = rs1_val | rs2_val;
    }
    // AND
    else if funct3 == Funct3::And {
        result = rs1_val & rs2_val;
    }

    if rd != 0 {
        state.regs[rd as usize] = result as u64;
    }

    Ok(())
}

/// Execute ALU immediate operations (RV64I I-type)
///
/// # Operations
/// - ADDI: Add Immediate
/// - SLTI: Set Less Than Immediate
/// - SLTIU: Set Less Than Immediate Unsigned
/// - XORI: XOR Immediate
/// - ORI: OR Immediate
/// - ANDI: AND Immediate
#[inline]
pub fn exec_op_imm(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    // Sign-extend the 12-bit immediate to 64 bits
    let imm_sext = ((imm as i32) << 20 >> 20) as i64;

    let result: i64 = match funct3 {
        // ADDI (add immediate)
        Funct3::AddSub => {
            let rs1_val = state.regs[rs1 as usize] as i64;
            rs1_val.wrapping_add(imm_sext)
        }
        // SLTI (set less than immediate)
        Funct3::Slt => {
            let rs1_val = state.regs[rs1 as usize] as i64;
            if rs1_val < imm_sext {
                1
            } else {
                0
            }
        }
        // SLTIU (set less than immediate unsigned)
        Funct3::Sltu => {
            let rs1_val = state.regs[rs1 as usize];
            let imm_u = imm_sext as u64;
            if rs1_val < imm_u {
                1
            } else {
                0
            }
        }
        // XORI (exclusive or immediate)
        Funct3::Xor => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val ^ (imm_sext as u64)) as i64
        }
        // ORI (or immediate)
        Funct3::Or => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val | (imm_sext as u64)) as i64
        }
        // ANDI (and immediate)
        Funct3::And => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val & (imm_sext as u64)) as i64
        }
        _ => return Err(ExecuteError::InvalidOperation),
    };

    if rd != 0 {
        state.regs[rd as usize] = result as u64;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_r(
        opcode: Opcode,
        funct3: Option<Funct3>,
        funct7: Option<u8>,
        rs1: Option<u8>,
        rs2: Option<u8>,
        rd: Option<u8>,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode,
            funct3,
            funct7,
            rs1,
            rs2,
            rs3: None,
            rd,
            imm: None,
            branch_taken: false,
        }
    }

    fn create_test_instr_i(
        opcode: Opcode,
        funct3: Option<Funct3>,
        rs1: Option<u8>,
        rd: Option<u8>,
        imm: Option<u32>,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode,
            funct3,
            funct7: None,
            rs1,
            rs2: None,
            rs3: None,
            rd,
            imm,
            branch_taken: false,
        }
    }

    #[test]
    fn test_add() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    #[test]
    fn test_sub() {
        let mut state = CoreState::default();
        state.regs[1] = 20;
        state.regs[2] = 10;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 10);
    }

    #[test]
    fn test_slt() {
        let mut state = CoreState::default();
        state.regs[1] = 3;
        state.regs[2] = 5;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::Slt),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_sltu() {
        let mut state = CoreState::default();
        state.regs[1] = 3;
        state.regs[2] = 5;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::Sltu),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_xor() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;
        state.regs[2] = 0b1010_1010;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::Xor),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0b0110_1010);
    }

    #[test]
    fn test_or() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;
        state.regs[2] = 0b1010_1010;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::Or),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0b1110_1010);
    }

    #[test]
    fn test_and() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1111_1111_0000_0000;
        state.regs[2] = 0b1010_1010_1010_1010;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::And),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0xAA00);
    }

    #[test]
    fn test_addi() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = create_test_instr_i(
            Opcode::OpImm,
            Some(Funct3::AddSub),
            Some(1),
            Some(2),
            Some(5),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_addi_negative() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = create_test_instr_i(
            Opcode::OpImm,
            Some(Funct3::AddSub),
            Some(1),
            Some(2),
            Some((-3i32) as u32),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2] as i32, 7);
    }

    #[test]
    fn test_slti() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        let instr =
            create_test_instr_i(Opcode::OpImm, Some(Funct3::Slt), Some(1), Some(2), Some(5));
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_sltiu() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        let instr =
            create_test_instr_i(Opcode::OpImm, Some(Funct3::Sltu), Some(1), Some(2), Some(5));
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_xori() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;

        let instr = create_test_instr_i(
            Opcode::OpImm,
            Some(Funct3::Xor),
            Some(1),
            Some(2),
            Some(0b1010_1010),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0b0110_1010);
    }

    #[test]
    fn test_ori() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;

        let instr = create_test_instr_i(
            Opcode::OpImm,
            Some(Funct3::Or),
            Some(1),
            Some(2),
            Some(0b1010_1010),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0b1110_1010);
    }

    #[test]
    fn test_andi() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_00FF;

        let instr = create_test_instr_i(
            Opcode::OpImm,
            Some(Funct3::And),
            Some(1),
            Some(2),
            Some(0x0AA),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xAA);
    }

    #[test]
    fn test_rd_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_r(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0),
            Some(1),
            Some(2),
            Some(0),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }
}
