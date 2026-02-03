//! RV64I Branch Operations
//!
//! This module implements the branch instructions for RV64I:
//! - BEQ: Branch if Equal
//! - BNE: Branch if Not Equal
//! - BLT: Branch if Less Than (signed)
//! - BGE: Branch if Greater or Equal (signed)
//! - BLTU: Branch if Less Than Unsigned
//! - BGEU: Branch if Greater or Equal Unsigned

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// Execute branch instructions (RV64I)
///
/// # Operations
/// - BEQ: Branch if Equal
/// - BNE: Branch if Not Equal
/// - BLT: Branch if Less Than (signed)
/// - BGE: Branch if Greater or Equal (signed)
/// - BLTU: Branch if Less Than Unsigned
/// - BGEU: Branch if Greater or Equal Unsigned
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (PC, registers)
/// * `_mem` - Memory interface (unused for branches)
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
    let funct3_val = funct3 as u8;
    let take_branch = match funct3_val {
        0b000 => rs1_val == rs2_val,                   // BEQ
        0b001 => rs1_val != rs2_val,                   // BNE
        0b100 => (rs1_val as i64) < (rs2_val as i64),  // BLT (signed)
        0b101 => (rs1_val as i64) >= (rs2_val as i64), // BGE (signed)
        0b110 => rs1_val < rs2_val,                    // BLTU (unsigned)
        0b111 => rs1_val >= rs2_val,                   // BGEU (unsigned)
        _ => false,
    };

    // DEBUG
    if funct3_val == 0b100 {
        let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
        eprintln!("[BRANCH-DETAIL] blt: rs1={} (x{}, gp), rs2={} (x{}, sp), take_branch={}, pc={:#x}, imm={:#x}, imm_sext={:#x}, target={:#x}",
                  rs1_val, rs1, rs2_val, rs2, take_branch, state.pc, imm, imm_sext,
                  state.pc.wrapping_add(imm_sext));
    }

    if take_branch {
        // Sign-extend the branch offset and add to PC
        // B-type immediate: imm[12|10:5|4:1|11] is a 13-bit signed offset (bit 0 is always 0)
        // Decode ensures imm only contains 13 valid bits (bits 0-12), so we sign-extend from bit 12
        // Using i32 << 20 >> 20 to sign-extend the 13-bit value to 32 bits
        let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
        eprintln!(
            "[BRANCH-TAKEN] pc={:#x}, imm={:#x}, imm_sext={:#x}, target={:#x}",
            state.pc,
            imm,
            imm_sext,
            state.pc.wrapping_add(imm_sext)
        );
        state.pc = state.pc.wrapping_add(imm_sext);
        // Mark that a branch was taken (used by step() to skip pc += 4)
        state.branch_taken = true;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr(funct3: Funct3, rs1: u8, rs2: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::BType,
            opcode: Opcode::Branch,
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
    fn test_beq_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::AddSub, 1, 2, 0x20); // BEQ uses funct3=0
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

        let instr = create_test_instr(Funct3::AddSub, 1, 2, 0x20);
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

        let instr = create_test_instr(Funct3::Sll, 1, 2, 0x20); // BNE uses funct3=1
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
        state.regs[1] = (-5i64) as u64;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::Xor, 1, 2, 0x20); // BLT uses funct3=4
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

        let instr = create_test_instr(Funct3::SrlSra, 1, 2, 0x20); // BGE uses funct3=5
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

        let instr = create_test_instr(Funct3::SrlSra, 1, 2, 0x20);
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

        let instr = create_test_instr(Funct3::Or, 1, 2, 0x20); // BLTU uses funct3=6
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

        let instr = create_test_instr(Funct3::And, 1, 2, 0x20); // BGEU uses funct3=7
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bltu_not_taken_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE;
        state.regs[2] = 5;

        let instr = create_test_instr(Funct3::Or, 1, 2, 0x20);
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

        let instr = create_test_instr(Funct3::And, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_negative_offset() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::AddSub, 1, 2, (-32i32) as u32);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x0FE0);
    }
}
