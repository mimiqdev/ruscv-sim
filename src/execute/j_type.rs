//! J-type instruction execution
//!
//! J-type (Jump-type) instructions perform unconditional jumps.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
use crate::execute::ExecuteError;
use crate::memory::SimpleMemory;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_instr_j_type(opcode: Opcode, rd: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::JType,
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

    fn create_test_instr_i_type(opcode: Opcode, rd: u8, rs1: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode,
            funct3: None,
            funct7: None,
            rs1: Some(rs1),
            rs2: None,
            rd: Some(rd),
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_jal_execution() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        let instr = create_test_instr_j_type(Opcode::Jal, 1, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x1004); // return address
        assert_eq!(state.pc, 0x1020); // target
    }

    #[test]
    fn test_jalr_execution() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0x2000;

        let instr = create_test_instr_i_type(Opcode::Jalr, 2, 1, 0x10);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x1004); // return address
        assert_eq!(state.pc, 0x2010); // target (with LSB cleared)
    }
}
