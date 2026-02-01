//! RV64I Store Operations
//!
//! This module implements the store instructions for RV64I:
//! - SB: Store Byte
//! - SH: Store Halfword
//! - SW: Store Word
//! - SD: Store Doubleword

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Execute store instructions (RV64I)
///
/// # Operations
/// - SB: Store Byte
/// - SH: Store Halfword
/// - SW: Store Word
/// - SD: Store Doubleword
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (registers)
/// * `mem` - Memory interface
#[inline]
pub fn exec_store(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
        (instr.rs1, instr.rs2, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let base = state.regs[rs1 as usize];
    // Sign-extend the 12-bit immediate to 64 bits
    let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
    let addr = base.wrapping_add(imm_sext);
    let value = state.regs[rs2 as usize];

    let funct3_val = funct3 as u8;
    match funct3_val {
        0b000 => mem.write_byte(addr, value as u8)?,  // SB
        0b001 => mem.write_half(addr, value as u16)?, // SH
        0b010 => mem.write_word(addr, value as u32)?, // SW
        0b011 => mem.write_dword(addr, value)?,       // SD
        _ => return Err(ExecuteError::InvalidOperation),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr(funct3: Funct3, rs1: u8, rs2: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::SType,
            opcode: Opcode::Store,
            funct3: Some(funct3),
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
            rd: None,
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_sb() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::AddSub, 1, 2, 4); // SB uses funct3=0

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_byte(0x104).unwrap(), 0x78);
    }

    #[test]
    fn test_sh() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::Sll, 1, 2, 4); // SH uses funct3=1

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_half(0x104).unwrap(), 0x5678);
    }

    #[test]
    fn test_sw() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::Slt, 1, 2, 4); // SW uses funct3=2

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_word(0x104).unwrap(), 0x12345678);
    }

    #[test]
    fn test_sd() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x123456789ABCDEF0;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::Sltu, 1, 2, 8); // SD uses funct3=3

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_dword(0x108).unwrap(), 0x123456789ABCDEF0);
    }

    #[test]
    fn test_negative_offset() {
        let mut state = CoreState::default();
        state.regs[1] = 0x200;
        state.regs[2] = 0xDEADBEEFCAFEBABE;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::Sltu, 1, 2, (-256i32) as u32);

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_dword(0x100).unwrap(), 0xDEADBEEFCAFEBABE);
    }

    #[test]
    fn test_zero_offset() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0xAABBCCDDEEFF0011;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr(Funct3::Sltu, 1, 2, 0);

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_dword(0x100).unwrap(), 0xAABBCCDDEEFF0011);
    }
}
