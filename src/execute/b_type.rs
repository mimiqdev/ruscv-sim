//! B-type instruction execution
//!
//! B-type (Branch-type) instructions perform conditional branches.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
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
