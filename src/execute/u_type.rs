//! U-type instruction execution
//!
//! U-type (Upper Immediate-type) instructions operate on upper immediates.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
use crate::execute::ExecuteError;
use crate::memory::SimpleMemory;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_instr_u_type(
        opcode: Opcode,
        rd: u8,
        imm: u32,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::UType,
            opcode,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: Some(rd),
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_lui_execution() {
        let mut state = CoreState::default();
        let instr = create_test_instr_u_type(Opcode::Lui, 1, 0x12345000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_lui(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x12345000);
    }

    #[test]
    fn test_auipc_execution() {
        let mut state = CoreState { pc: 0x1000, ..Default::default() };
        let instr = create_test_instr_u_type(Opcode::Auipc, 1, 0x00001000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_auipc(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x2000);
    }
}
