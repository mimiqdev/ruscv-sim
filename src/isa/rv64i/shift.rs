//! RV64I Shift Operations
//!
//! This module implements the shift operations for RV64I:
//! - Register shifts: SLL, SRL, SRA
//! - Immediate shifts: SLLI, SRLI, SRAI

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;

/// Execute register-register shift operations (RV64I R-type)
///
/// # Operations
/// - SLL: Shift Left Logical
/// - SRL: Shift Right Logical
/// - SRA: Shift Right Arithmetic
#[inline]
pub fn exec_shift(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(rs2), Some(funct3), Some(funct7)) =
        (instr.rd, instr.rs1, instr.rs2, instr.funct3, instr.funct7)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let rs1_val = state.regs[rs1 as usize];
    let rs1_i64 = rs1_val as i64;
    // RV64I uses lower 6 bits of rs2 for shift amount
    let shamt = (state.regs[rs2 as usize] & 0x3F) as u32;

    let result = if funct3 == Funct3::Sll {
        // SLL (logical left shift)
        rs1_val.wrapping_shl(shamt)
    } else if funct3 == Funct3::SrlSra {
        // Distinguish SRL (funct7=0) vs SRA (funct7=0x20)
        if funct7 == 0 {
            // SRL (logical right shift)
            rs1_val.wrapping_shr(shamt)
        } else if funct7 == 0x20 {
            // SRA (arithmetic right shift)
            rs1_i64.wrapping_shr(shamt) as u64
        } else {
            return Err(ExecuteError::InvalidOperation);
        }
    } else {
        return Err(ExecuteError::InvalidOperation);
    };

    if rd != 0 {
        state.regs[rd as usize] = result;
    }

    Ok(())
}

/// Execute shift immediate operations (RV64I I-type)
///
/// # Operations
/// - SLLI: Shift Left Logical Immediate
/// - SRLI: Shift Right Logical Immediate
/// - SRAI: Shift Right Arithmetic Immediate
#[inline]
pub fn exec_shift_imm(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    // RV64I uses 6-bit shamt (imm[5:0])
    let shamt = imm & 0x3F;
    let rs1_val = state.regs[rs1 as usize];
    let rs1_i64 = rs1_val as i64;

    let result = match funct3 {
        Funct3::Sll => {
            // SLLI
            rs1_val.wrapping_shl(shamt)
        }
        Funct3::SrlSra => {
            // Distinguish SRLI (funct7=0x00) vs SRAI (funct7=0x20)
            match instr.funct7 {
                Some(f7) if (f7 & 0x20) == 0 => rs1_val.wrapping_shr(shamt), // SRLI
                Some(f7) if (f7 & 0x20) != 0 => rs1_i64.wrapping_shr(shamt) as u64, // SRAI
                _ => return Err(ExecuteError::InvalidOperation),
            }
        }
        _ => return Err(ExecuteError::InvalidOperation),
    };

    if rd != 0 {
        state.regs[rd as usize] = result;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_r(
        funct3: Funct3,
        funct7: u8,
        rs1: u8,
        rs2: u8,
        rd: u8,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(funct3),
            funct7: Some(funct7),
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    fn create_test_instr_i(
        funct3: Funct3,
        funct7: Option<u8>,
        rs1: u8,
        rd: u8,
        imm: u32,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(funct3),
            funct7,
            rs1: Some(rs1),
            rs2: None,
            rs3: None,
            rd: Some(rd),
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_sll() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::Sll, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_srl() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1_0000_0000;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::SrlSra, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_srl_with_negative_value() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::SrlSra, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x0FFFFFFF);
    }

    #[test]
    fn test_sra() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFF0;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::SrlSra, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(state.regs[3] as i64, -1);
    }

    #[test]
    fn test_sra_with_positive_value() {
        let mut state = CoreState::default();
        state.regs[1] = 256;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::SrlSra, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_slli() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001;

        let instr = create_test_instr_i(Funct3::Sll, None, 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srli() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1_0000_0000;

        let instr = create_test_instr_i(Funct3::SrlSra, Some(0x00), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srai() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFF0;

        let instr = create_test_instr_i(Funct3::SrlSra, Some(0x20), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(state.regs[2] as i64, -1);
    }

    #[test]
    fn test_srai_with_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 256;

        let instr = create_test_instr_i(Funct3::SrlSra, Some(0x20), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_shamt_masking() {
        let mut state = CoreState::default();
        state.regs[1] = 1;
        // RV64I uses 6-bit shamt mask (0x3F), so 0x28 = 40 means shift by 40
        state.regs[2] = 0x12345628; // lower 6 bits = 0x28 = 40

        let instr = create_test_instr_r(Funct3::Sll, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1u64.wrapping_shl(40));
    }

    #[test]
    fn test_rd_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001;
        state.regs[2] = 4;

        let instr = create_test_instr_r(Funct3::Sll, 0, 1, 2, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_shift(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }
}
