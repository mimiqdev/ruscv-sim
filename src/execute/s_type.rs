//! S-type instruction execution (RV64I)
//!
//! S-type (Store-type) instructions store data from a register to memory.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Store instructions (exec_store) - RV64I
///
/// Executes store instructions including:
/// - SB: Store Byte
/// - SH: Store Halfword
/// - SW: Store Word
/// - SD: Store Doubleword (RV64I)
///
/// RV64I funct3 encoding:
/// - 000: SB (Store Byte)
/// - 001: SH (Store Halfword)
/// - 010: SW (Store Word)
/// - 011: SD (Store Doubleword)
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
        0b010 => mem.write_word(addr, value as u32)?, // SW
        0b011 => mem.write_dword(addr, value)?,       // SD (store doubleword)
        0b001 => mem.write_half(addr, value as u16)?, // SH
        0b000 => mem.write_byte(addr, value as u8)?,  // SB
        _ => return Err(ExecuteError::InvalidOperation),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::{MemoryInterface, SimpleMemory};

    fn create_test_instr_s_type(funct3: Funct3, rs1: u8, rs2: u8, imm: u32) -> DecodedInstruction {
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
    fn test_sw_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        // SW uses funct3=0b010 (Funct3::Slt in enum, but represents SW for Store)
        let instr = create_test_instr_s_type(Funct3::Slt, 1, 2, 4);

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_word(0x104).unwrap(), 0x12345678);
    }

    #[test]
    fn test_sh_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr_s_type(Funct3::Sll, 1, 2, 4);

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_half(0x104).unwrap(), 0x5678);
    }

    #[test]
    fn test_sb_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;
        state.regs[2] = 0x12345678;

        let mut mem = SimpleMemory::new(0x1000);
        let instr = create_test_instr_s_type(Funct3::Slt, 1, 2, 4);

        exec_store(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_byte(0x104).unwrap(), 0x78);
    }
}
