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
/// JALR performs an unconditional jump to the address in a register,
/// with an optional immediate offset, storing the return address in a register.
#[inline]
pub fn exec_jalr(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
        let return_addr = state.pc.wrapping_add(4);
        let target = (state.regs[rs1 as usize].wrapping_add(imm)) & !1u32;

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

    #[test]
    fn test_jal_execution() {
        let mut state = CoreState::default();
        let instr = create_test_instr_j_type(Opcode::Jal, 1, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 4); // PC + 4
        assert_eq!(state.pc, 4); // PC + 4 (immediate)
    }

    #[test]
    fn test_jal_with_offset() {
        let mut state = CoreState::default();
        state.pc = 0x1000;
        let instr = create_test_instr_j_type(Opcode::Jal, 2, 0x200);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x1004); // PC + 4
        assert_eq!(state.pc, 0x1200); // 0x1000 + 0x200
    }

    #[test]
    fn test_jal_rd_zero() {
        let mut state = CoreState::default();
        let instr = create_test_instr_j_type(Opcode::Jal, 0, 4);
        let mut mem = SimpleMemory::new(0x1000);

        exec_jal(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0); // x0 is always 0
        assert_eq!(state.pc, 4);
    }

    #[test]
    fn test_jalr_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x2000;
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::Jalr,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0x10),
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 4); // PC + 4
        assert_eq!(state.pc, 0x2010); // 0x2000 + 0x10 (clears LSB)
    }

    #[test]
    fn test_jalr_no_link() {
        let mut state = CoreState::default();
        state.regs[1] = 0x1000;
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::Jalr,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(0),
            imm: Some(0),
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0); // x0 is always 0
        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_jalr_large_offset() {
        let mut state = CoreState::default();
        state.pc = 0x10000;
        state.regs[1] = 0xFFFFF000;
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::Jalr,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(3),
            imm: Some(0x1000),
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        exec_jalr(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x10004);
        assert_eq!(state.pc, 0xFFFFF000u32.wrapping_add(0x1000) & !1u32);
    }
}
