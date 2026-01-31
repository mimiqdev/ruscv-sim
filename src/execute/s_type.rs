//! S-type instruction execution
//!
//! S-type (Store-type) instructions store data from a register to memory.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Store instructions (exec_store)
///
/// Executes store instructions including:
/// - SB: Store Byte
/// - SH: Store Halfword
/// - SW: Store Word
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
    let addr = base.wrapping_add(imm);
    let value = state.regs[rs2 as usize];

    match funct3 {
        Funct3::AddSub => mem.write_word(addr, value)?, // SW
        Funct3::Sll => mem.write_half(addr, value as u16)?, // SH
        Funct3::Slt => mem.write_byte(addr, value as u8)?, // SB
        _ => return Err(ExecuteError::InvalidOperation),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::execute::ExecuteError;
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
        let instr = create_test_instr_s_type(Funct3::AddSub, 1, 2, 4);

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
