//! J-type instruction execution
//!
//! J-type (Jump-type) instructions perform unconditional jumps.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// JAL (Jump and Link)
///
/// JAL performs an unconditional jump with a 20-bit signed offset,
/// storing the return address (PC + 4) in the destination register.
#[inline]
pub fn exec_jal(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
        let return_addr = state.pc.wrapping_add(4);
        let target = state.pc.wrapping_add(imm);

        if rd != 0 {
            state.regs[rd as usize] = return_addr;
        }

        state.pc = target;
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

/// JALR (Jump and Link Register)
///
/// JALR performs an unconditional jump to a register-based address,
/// storing the return address (PC + 4) in the destination register.
/// The target address is computed as (rs1 + imm) with the LSB cleared.
#[inline]
pub fn exec_jalr(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
        let return_addr = state.pc.wrapping_add(4);
        let base = state.regs[rs1 as usize];
        let target = (base.wrapping_add(imm)) & !1u32; // LSB cleared

        if rd != 0 {
            state.regs[rd as usize] = return_addr;
        }

        state.pc = target;
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}
