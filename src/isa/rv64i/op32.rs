//! RV64I 32-bit Operations (RV64I OpImm32 and Op32)
//!
//! This module implements the 32-bit operations for RV64I:
//! - Immediate operations (OpImm32): ADDIW, SLLIW, SRLIW, SRAIW
//! - Register operations (Op32): ADDW, SUBW, SLLW, SRLW, SRAW
//!
//! All 32-bit operations produce a 32-bit result that is sign-extended to 64 bits.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;

/// Execute 32-bit immediate operations (OpImm32)
///
/// # Operations
/// - ADDIW: Add Immediate Word (result sign-extended to 64 bits)
/// - SLLIW: Shift Left Logical Immediate Word
/// - SRLIW: Shift Right Logical Immediate Word
/// - SRAIW: Shift Right Arithmetic Immediate Word
#[inline]
pub fn exec_op_imm_32(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    // Get 32-bit value from rs1 (lower 32 bits)
    let rs1_val = state.regs[rs1 as usize] as u32;
    let rs1_i32 = rs1_val as i32;

    // Compute 32-bit result
    let result_32: u32 = match funct3 {
        Funct3::AddSub => {
            // ADDIW: Add Immediate Word
            // Sign-extend 12-bit immediate to 32 bits, add, then sign-extend to 64 bits
            let imm_sext_32 = (imm as i32) << 20 >> 20;
            rs1_i32.wrapping_add(imm_sext_32) as u32
        }
        Funct3::Sll => {
            // SLLIW: Shift Left Logical Immediate Word
            // RV64I uses lower 5 bits of immediate for shift amount (shamt[4:0])
            let shamt = imm & 0x1F;
            rs1_val.wrapping_shl(shamt)
        }
        Funct3::SrlSra => {
            // Distinguish SRLIW (funct7=0x00) vs SRAIW (funct7=0x20)
            let shamt = imm & 0x1F;
            match instr.funct7 {
                Some(f7) if (f7 & 0x20) == 0 => {
                    // SRLIW: Shift Right Logical Immediate Word
                    rs1_val.wrapping_shr(shamt)
                }
                Some(f7) if (f7 & 0x20) != 0 => {
                    // SRAIW: Shift Right Arithmetic Immediate Word
                    rs1_i32.wrapping_shr(shamt) as u32
                }
                _ => return Err(ExecuteError::InvalidOperation),
            }
        }
        _ => return Err(ExecuteError::InvalidOperation),
    };

    // Sign-extend 32-bit result to 64 bits
    if rd != 0 {
        state.regs[rd as usize] = (result_32 as i32) as i64 as u64;
    }

    Ok(())
}

/// Execute 32-bit register operations (Op32)
///
/// # Operations
/// - ADDW: Add Word (result sign-extended to 64 bits)
/// - SUBW: Subtract Word (result sign-extended to 64 bits)
/// - SLLW: Shift Left Logical Word
/// - SRLW: Shift Right Logical Word
/// - SRAW: Shift Right Arithmetic Word
#[inline]
pub fn exec_op_32(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(rs2), Some(funct3), Some(funct7)) =
        (instr.rd, instr.rs1, instr.rs2, instr.funct3, instr.funct7)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    // Get 32-bit values from registers (lower 32 bits)
    let rs1_val = state.regs[rs1 as usize] as u32;
    let rs1_i32 = rs1_val as i32;
    let rs2_val = state.regs[rs2 as usize] as u32;
    let rs2_i32 = rs2_val as i32;

    // Compute 32-bit result
    let result_32: u32 = match funct3 {
        Funct3::AddSub => {
            // ADDW or SUBW (distinguished by funct7)
            if funct7 == 0 {
                // ADDW: Add Word
                rs1_i32.wrapping_add(rs2_i32) as u32
            } else if funct7 == 0x20 {
                // SUBW: Subtract Word
                rs1_i32.wrapping_sub(rs2_i32) as u32
            } else {
                return Err(ExecuteError::InvalidOperation);
            }
        }
        Funct3::Sll => {
            // SLLW: Shift Left Logical Word
            // RV64I uses lower 5 bits of rs2 for shift amount
            let shamt = rs2_val & 0x1F;
            rs1_val.wrapping_shl(shamt)
        }
        Funct3::SrlSra => {
            // Distinguish SRLW (funct7=0) vs SRAW (funct7=0x20)
            // RV64I uses lower 5 bits of rs2 for shift amount
            let shamt = rs2_val & 0x1F;
            if funct7 == 0 {
                // SRLW: Shift Right Logical Word
                rs1_val.wrapping_shr(shamt)
            } else if funct7 == 0x20 {
                // SRAW: Shift Right Arithmetic Word
                rs1_i32.wrapping_shr(shamt) as u32
            } else {
                return Err(ExecuteError::InvalidOperation);
            }
        }
        _ => return Err(ExecuteError::InvalidOperation),
    };

    // Sign-extend 32-bit result to 64 bits
    if rd != 0 {
        state.regs[rd as usize] = (result_32 as i32) as i64 as u64;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_op_imm_32(
        funct3: Funct3,
        funct7: Option<u8>,
        rs1: u8,
        rd: u8,
        imm: u32,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::OpImm32,
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

    fn create_test_instr_op_32(
        funct3: Funct3,
        funct7: u8,
        rs1: u8,
        rs2: u8,
        rd: u8,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::Op32,
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

    // ===== ADDIW Tests =====

    #[test]
    fn test_addiw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 2, 5);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_addiw_negative() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // -3 as 12-bit immediate
        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 2, (-3i32) as u32);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 7);
    }

    #[test]
    fn test_addiw_sign_extend_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 0x7FFF_FFFF; // Max positive 32-bit value

        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 2, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // Should remain positive when sign-extended
        assert_eq!(state.regs[2], 0x7FFF_FFFF);
    }

    #[test]
    fn test_addiw_sign_extend_negative() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // Min negative 32-bit value

        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 2, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // Should be sign-extended to 64-bit negative value
        assert_eq!(state.regs[2], 0xFFFF_FFFF_8000_0000);
        assert_eq!(state.regs[2] as i64, i32::MIN as i64);
    }

    #[test]
    fn test_addiw_overflow() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF; // -1 as 32-bit

        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 2, 1);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // -1 + 1 = 0, sign-extended to 64-bit
        assert_eq!(state.regs[2], 0);
    }

    // ===== SLLIW Tests =====

    #[test]
    fn test_slliw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0001;

        let instr = create_test_instr_op_imm_32(Funct3::Sll, Some(0), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_slliw_sign_extend() {
        let mut state = CoreState::default();
        state.regs[1] = 0x4000_0000; // 1 << 30

        let instr = create_test_instr_op_imm_32(Funct3::Sll, Some(0), 1, 2, 1);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // 0x4000_0000 << 1 = 0x8000_0000 (negative as 32-bit)
        assert_eq!(state.regs[2], 0xFFFF_FFFF_8000_0000);
        assert!((state.regs[2] as i64) < 0);
    }

    // ===== SRLIW Tests =====

    #[test]
    fn test_srliw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0100;

        let instr = create_test_instr_op_imm_32(Funct3::SrlSra, Some(0), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srliw_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 0x7FFF_FFFF; // Large positive 32-bit value

        let instr = create_test_instr_op_imm_32(Funct3::SrlSra, Some(0), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // 0x7FFF_FFFF >> 4 = 0x07FF_FFFF (still positive)
        assert_eq!(state.regs[2], 0x07FF_FFFF);
    }

    // ===== SRAIW Tests =====

    #[test]
    fn test_sraiw_negative() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFF0; // -16 as 32-bit

        let instr = create_test_instr_op_imm_32(Funct3::SrlSra, Some(0x20), 1, 2, 2);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // -16 >> 2 = -4, sign-extended to 64-bit
        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_FFFC);
        assert_eq!(state.regs[2] as i64, -4);
    }

    #[test]
    fn test_sraiw_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0100; // 256 as 32-bit

        let instr = create_test_instr_op_imm_32(Funct3::SrlSra, Some(0x20), 1, 2, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // 256 >> 4 = 16
        assert_eq!(state.regs[2], 16);
    }

    // ===== ADDW Tests =====

    #[test]
    fn test_addw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_op_32(Funct3::AddSub, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    #[test]
    fn test_addw_sign_extend() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // Negative as 32-bit
        state.regs[2] = 0;

        let instr = create_test_instr_op_32(Funct3::AddSub, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // Should be sign-extended
        assert_eq!(state.regs[3], 0xFFFF_FFFF_8000_0000);
    }

    // ===== SUBW Tests =====

    #[test]
    fn test_subw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 30;
        state.regs[2] = 10;

        let instr = create_test_instr_op_32(Funct3::AddSub, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 20);
    }

    #[test]
    fn test_subw_negative_result() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_op_32(Funct3::AddSub, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // 10 - 20 = -10, sign-extended to 64-bit
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFF6);
        assert_eq!(state.regs[3] as i64, -10);
    }

    // ===== SLLW Tests =====

    #[test]
    fn test_sllw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0001;
        state.regs[2] = 4;

        let instr = create_test_instr_op_32(Funct3::Sll, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_sllw_only_5bit_shamt() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0001;
        state.regs[2] = 0x1234_0021; // 0x21 = 33, but only lower 5 bits used

        let instr = create_test_instr_op_32(Funct3::Sll, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // 33 & 0x1F = 1, so 1 << 1 = 2
        assert_eq!(state.regs[3], 2);
    }

    // ===== SRLW Tests =====

    #[test]
    fn test_srlw_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0100;
        state.regs[2] = 4;

        let instr = create_test_instr_op_32(Funct3::SrlSra, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_srlw_32bit_only() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFFF; // Upper bits set in 64-bit
        state.regs[2] = 4;

        let instr = create_test_instr_op_32(Funct3::SrlSra, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // Only lower 32 bits are used: 0xFFFF_FFFF >> 4 = 0x0FFF_FFFF
        assert_eq!(state.regs[3], 0x0FFF_FFFF);
    }

    // ===== SRAW Tests =====

    #[test]
    fn test_sraw_negative() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFF0; // -16 as 32-bit
        state.regs[2] = 2;

        let instr = create_test_instr_op_32(Funct3::SrlSra, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // -16 >> 2 = -4, sign-extended
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFC);
        assert_eq!(state.regs[3] as i64, -4);
    }

    #[test]
    fn test_sraw_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0000_0100; // 256 as 32-bit
        state.regs[2] = 4;

        let instr = create_test_instr_op_32(Funct3::SrlSra, 0x20, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    // ===== Edge Cases =====

    #[test]
    fn test_rd_zero_ignored() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000;

        let instr = create_test_instr_op_imm_32(Funct3::AddSub, None, 1, 0, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm_32(&instr, &mut state, &mut mem).unwrap();

        // x0 should remain 0
        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_32bit_wraparound() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF; // -1 as 32-bit
        state.regs[2] = 1;

        let instr = create_test_instr_op_32(Funct3::AddSub, 0, 1, 2, 3);
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_32(&instr, &mut state, &mut mem).unwrap();

        // -1 + 1 = 0 (32-bit), sign-extended to 64-bit
        assert_eq!(state.regs[3], 0);
    }
}
