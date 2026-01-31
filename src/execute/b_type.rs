//! B-type instruction execution
//!
//! B-type (Branch-type) instructions perform conditional branches.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// Branch instructions (exec_branch)
///
/// Executes branch instructions including:
/// - BEQ: Branch if Equal
/// - BNE: Branch if Not Equal
/// - BLT: Branch if Less Than (signed)
/// - BGE: Branch if Greater or Equal (signed)
/// - BLTU: Branch if Less Than Unsigned
/// - BGEU: Branch if Greater or Equal Unsigned
#[inline]
pub fn exec_branch(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
        (instr.rs1, instr.rs2, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let rs1_val = state.regs[rs1 as usize];
    let rs2_val = state.regs[rs2 as usize];

    // Extract raw funct3 value (3 bits) for branch instruction decoding
    // Branch instructions use specific funct3 codes: 000=BEQ, 001=BNE, 100=BLT, 101=BGE, 110=BLTU, 111=BGEU
    let funct3_val = funct3 as u8;
    let take_branch = match funct3_val {
        0b000 => rs1_val == rs2_val,                   // BEQ
        0b001 => rs1_val != rs2_val,                   // BNE
        0b100 => (rs1_val as i32) < (rs2_val as i32),  // BLT (signed)
        0b101 => (rs1_val as i32) >= (rs2_val as i32), // BGE (signed)
        0b110 => rs1_val < rs2_val,                    // BLTU (unsigned)
        0b111 => rs1_val >= rs2_val,                   // BGEU (unsigned)
        _ => false,
    };

    if take_branch {
        state.pc = state.pc.wrapping_add(imm);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Funct3;
    use crate::decode::{InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_b_type(funct3: Funct3, rs1: u8, rs2: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::BType,
            opcode: Opcode::Branch,
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
    fn test_beq_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::AddSub, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_beq_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_b_type(Funct3::AddSub, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bne_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr_b_type(Funct3::Sll, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_blt_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = (-5i32) as u32;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::Xor, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::SrlSra, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bltu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::Or, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bgeu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::And, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bne_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::Sll, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_blt_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::Xor, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_blt_negative_vs_positive() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = (-10i32) as u32;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::Xor, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_equal() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::SrlSra, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::SrlSra, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bgeu_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::And, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bgeu_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = create_test_instr_b_type(Funct3::And, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_not_taken_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::Or, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_negative_vs_positive() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = (-1i32) as u32;
        state.regs[2] = 5;

        let instr = create_test_instr_b_type(Funct3::Or, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_small_vs_large() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 0xFFFFFFFF;

        let instr = create_test_instr_b_type(Funct3::Or, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }
}
