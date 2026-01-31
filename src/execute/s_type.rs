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
