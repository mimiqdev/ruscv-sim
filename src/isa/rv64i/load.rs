//! RV64I Load Operations
//!
//! This module implements the load instructions for RV64I:
//! - LB: Load Byte (sign-extend)
//! - LH: Load Halfword (sign-extend)
//! - LW: Load Word (sign-extend)
//! - LD: Load Doubleword
//! - LBU: Load Byte Unsigned (zero-extend)
//! - LHU: Load Halfword Unsigned (zero-extend)
//! - LWU: Load Word Unsigned (zero-extend)

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Execute load instructions (RV64I)
///
/// # Operations
/// - LB: Load Byte, sign-extend to 64-bit
/// - LH: Load Halfword, sign-extend to 64-bit
/// - LW: Load Word, sign-extend to 64-bit
/// - LD: Load Doubleword
/// - LBU: Load Byte Unsigned, zero-extend
/// - LHU: Load Halfword Unsigned, zero-extend
/// - LWU: Load Word Unsigned, zero-extend
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (registers)
/// * `mem` - Memory interface
#[inline]
pub fn exec_load(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let base = state.regs[rs1 as usize];
    // Sign-extend the 12-bit immediate to 64 bits
    let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
    let addr = base.wrapping_add(imm_sext);

    let funct3_val = funct3 as u8;
    let value = match funct3_val {
        0b000 => mem.read_byte_sext(addr)?, // LB
        0b001 => mem.read_half_sext(addr)?, // LH
        0b010 => mem.read_word_sext(addr)?, // LW
        0b011 => mem.read_dword(addr)?,     // LD
        0b100 => mem.read_byte_zext(addr)?, // LBU
        0b101 => mem.read_half_zext(addr)?, // LHU
        0b110 => mem.read_word_zext(addr)?, // LWU
        _ => return Err(ExecuteError::InvalidOperation),
    };

    if rd != 0 {
        state.regs[rd as usize] = value;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr(funct3: Funct3, rs1: u8, rd: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::Load,
            funct3: Some(funct3),
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
    fn test_lb() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_byte(0x104, 0x82).unwrap(); // Negative byte

        let instr = create_test_instr(Funct3::AddSub, 1, 2, 4); // LB uses funct3=0
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Sign-extend 0x82 to 64-bit
        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_FF82);
    }

    #[test]
    fn test_lbu() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_byte(0x104, 0x82).unwrap();

        let instr = create_test_instr(Funct3::Xor, 1, 2, 4); // LBU uses funct3=4
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Zero-extend 0x82
        assert_eq!(state.regs[2], 0x82);
    }

    #[test]
    fn test_lh() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_half(0x104, 0x8765).unwrap(); // Negative halfword

        let instr = create_test_instr(Funct3::Sll, 1, 2, 4); // LH uses funct3=1
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Sign-extend 0x8765 to 64-bit
        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_8765);
    }

    #[test]
    fn test_lhu() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_half(0x104, 0x8765).unwrap();

        let instr = create_test_instr(Funct3::SrlSra, 1, 2, 4); // LHU uses funct3=5
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Zero-extend 0x8765
        assert_eq!(state.regs[2], 0x8765);
    }

    #[test]
    fn test_lw() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_word(0x104, 0x87654321).unwrap(); // Negative word

        let instr = create_test_instr(Funct3::Slt, 1, 2, 4); // LW uses funct3=2
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Sign-extend 0x87654321 to 64-bit
        assert_eq!(state.regs[2], 0xFFFF_FFFF_8765_4321);
    }

    #[test]
    fn test_lwu() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_word(0x104, 0x87654321).unwrap();

        let instr = create_test_instr(Funct3::Or, 1, 2, 4); // LWU uses funct3=6
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // Zero-extend 0x87654321
        assert_eq!(state.regs[2], 0x87654321);
    }

    #[test]
    fn test_ld() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x108, 0x123456789ABCDEF0).unwrap();

        let instr = create_test_instr(Funct3::Sltu, 1, 2, 8); // LD uses funct3=3
        exec_load(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_rd_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x108, 0x123456789ABCDEF0).unwrap();

        let instr = create_test_instr(Funct3::Sltu, 1, 0, 8);
        exec_load(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_negative_offset() {
        let mut state = CoreState::default();
        state.regs[1] = 0x200;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x100, 0xDEADBEEFCAFEBABE).unwrap();

        let instr = create_test_instr(Funct3::Sltu, 1, 2, (-256i32) as u32);
        exec_load(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xDEADBEEFCAFEBABE);
    }
}
