//! U-type instruction execution
//!
//! U-type (Upper Immediate-type) instructions operate on upper immediates.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// LUI (Load Upper Immediate)
///
/// LUI loads the upper 20 bits of a register with a immediate value,
/// setting the lower 12 bits to zero.
#[inline]
pub fn exec_lui(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
        if rd != 0 {
            state.regs[rd as usize] = imm;
        }
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

/// AUIPC (Add Upper Immediate to PC)
///
/// AUIPC adds a 20-bit upper immediate to the current PC value,
/// storing the result in a register.
#[inline]
pub fn exec_auipc(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
        if rd != 0 {
            state.regs[rd as usize] = state.pc.wrapping_add(imm);
        }
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}
