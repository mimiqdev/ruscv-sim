//! RV64I Jump Operations
//!
//! This module implements the jump instructions for RV64I:
//! - JAL: Jump and Link
//! - JALR: Jump and Link Register

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// JAL (Jump and Link) - RV64I
///
/// JAL performs an unconditional jump with a 20-bit signed offset,
/// storing the return address (PC + 4) in the destination register.
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (PC, registers)
/// * `_mem` - Memory interface (unused)
#[inline]
pub fn exec_jal(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
        let return_addr = state.pc.wrapping_add(4);
        // J-type immediate is 21 bits (bits 0-20 of encoded instruction)
        // Stored as: imm[20|10:1|11|19:12] in decoder's imm field
        // Sign-extend: shift left 11 bits (move bit 20 to bit 31), then arithmetic right shift 11 bits
        let imm_sext = ((imm as i32) << 11 >> 11) as i64;
        let target = state.pc.wrapping_add(imm_sext as u64);

        if rd != 0 {
            state.regs[rd as usize] = return_addr;
        }

        eprintln!(
            "[JAL] pc={:#x}, imm={:#x}, imm_sext={:#x}, target={:#x}",
            state.pc, imm, imm_sext as u64, target
        );

        state.pc = target;
        // Mark that a jump was taken (used by step() to skip pc += 4)
        state.branch_taken = true;
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

/// JALR (Jump and Link Register) - RV64I
///
/// JALR performs an unconditional jump to the address in a register,
/// with an optional immediate offset, storing the return address in a register.
/// The target address has its least significant bit cleared (aligned to 2 bytes).
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (PC, registers)
/// * `_mem` - Memory interface (unused)
#[inline]
pub fn exec_jalr(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
        let return_addr = state.pc.wrapping_add(4);
        // Sign-extend the 12-bit immediate to 64 bits
        let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
        let target = (state.regs[rs1 as usize].wrapping_add(imm_sext)) & !1u64;

        if rd != 0 {
            state.regs[rd as usize] = return_addr;
        }

        state.pc = target;
        // Mark that a jump was taken (used by step() to skip pc += 4)
        state.branch_taken = true;
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_jal(rd: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::JType,
            opcode: Opcode::Jal,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rs3: None,
            rd: Some(rd),
            imm: Some(imm),
            branch_taken: false,
        }
    }

    fn create_test_instr_jalr(rd: u8, rs1: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::Jalr,
            funct3: None,
            funct7: None,
            rs1: Some(rs1),
            rs2: None,
            rs3: None,
            rd: Some(rd),
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_jal_basic() {
        let mut state = CoreState::default();
        let instr = create_test_instr_jal(1, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 4);
        assert_eq!(state.pc, 4);
    }

    #[test]
    fn test_jal_with_offset() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        let instr = create_test_instr_jal(2, 0x200);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x1004);
        assert_eq!(state.pc, 0x1200);
    }

    #[test]
    fn test_jal_rd_zero() {
        let mut state = CoreState::default();
        let instr = create_test_instr_jal(0, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
        assert_eq!(state.pc, 4);
    }

    #[test]
    fn test_jalr_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x2000;
        let instr = create_test_instr_jalr(2, 1, 0x10);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 4);
        assert_eq!(state.pc, 0x2010);
    }

    #[test]
    fn test_jalr_no_link() {
        let mut state = CoreState::default();
        state.regs[1] = 0x1000;
        let instr = create_test_instr_jalr(0, 1, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_jalr_alignment() {
        let mut state = CoreState::default();
        state.regs[1] = 0x2001; // Odd address
        let instr = create_test_instr_jalr(2, 1, 0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        // LSB should be cleared
        assert_eq!(state.pc, 0x2000);
    }

    #[test]
    fn test_jalr_large_offset() {
        let mut state = CoreState {
            pc: 0x10000,
            ..Default::default()
        };
        state.regs[1] = 0x0000_FFFF_FFFF_F000;
        let instr = create_test_instr_jalr(3, 1, 0x100);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x10004);
        assert_eq!(state.pc, 0x0000_FFFF_FFFF_F100);
    }

    #[test]
    fn test_jalr_negative_offset() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0x2000;
        let instr = create_test_instr_jalr(2, 1, (-256i32) as u32);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x1004);
        assert_eq!(state.pc, 0x1F00);
    }
}
