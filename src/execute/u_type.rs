//! U-type instruction execution (RV64I)
//!
//! U-type (Upper Immediate-type) instructions operate on upper immediates.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// LUI (Load Upper Immediate) - RV64I
///
/// LUI loads the upper 20 bits of a register with an immediate value,
/// setting the lower 12 bits to zero, then sign-extending to 64 bits.
#[inline]
pub fn exec_lui(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
        if rd != 0 {
            // Sign-extend the 32-bit value to 64 bits
            state.regs[rd as usize] = (imm as i32) as i64 as u64;
        }
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

/// AUIPC (Add Upper Immediate to PC) - RV64I
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
            // Sign-extend the immediate and add to PC
            let imm_sext = (imm as i32) as i64 as u64;
            state.regs[rd as usize] = state.pc.wrapping_add(imm_sext);
        }
        Ok(())
    } else {
        Err(ExecuteError::InvalidOperation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_u_type(opcode: Opcode, rd: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::UType,
            opcode,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rs3: None,
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
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        let instr = create_test_instr_u_type(Opcode::Auipc, 1, 0x00001000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_auipc(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x2000);
    }

    #[test]
    fn test_lui_rd_zero() {
        let mut state = CoreState::default();
        let instr = create_test_instr_u_type(Opcode::Lui, 0, 0x12345000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_lui(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_auipc_rd_zero() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        let instr = create_test_instr_u_type(Opcode::Auipc, 0, 0x12345000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_auipc(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_auipc_large_offset() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        let instr = create_test_instr_u_type(Opcode::Auipc, 1, 0x12345000);
        let mut mem = SimpleMemory::new(0x1000);

        exec_auipc(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x1000 + 0x12345000);
    }
}
