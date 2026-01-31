//! R-type instruction execution
//!
//! R-type (Register-type) instructions operate on two source registers
//! and write the result to a destination register.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;

/// R-type operation instructions (exec_op)
///
/// Executes R-type instructions including:
/// - ADD/SUB: Addition/Subtraction
/// - SLL: Shift Left Logical
/// - SLT/SLTU: Set Less Than (signed/unsigned)
/// - XOR: Exclusive OR
/// - SRL/SRA: Shift Right Logical/Arithmetic
/// - OR: Logical OR
/// - AND: Logical AND
#[inline]
pub fn exec_op(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(rs2), Some(funct3), Some(funct7)) =
        (instr.rd, instr.rs1, instr.rs2, instr.funct3, instr.funct7)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let rs1_val = state.regs[rs1 as usize] as i32;
    let rs2_val = state.regs[rs2 as usize] as i32;
    let mut result: i32 = 0;

    // ADD/SUB
    if funct3 == Funct3::AddSub {
        if funct7 == 0 {
            result = rs1_val.wrapping_add(rs2_val);
        } else if funct7 == 0x20 {
            result = rs1_val.wrapping_sub(rs2_val);
        }
    }
    // SLL (logical left shift)
    else if funct3 == Funct3::Sll {
        let shamt = (rs2_val & 0x1F) as u32;
        result = (rs1_val as u32).wrapping_shl(shamt) as i32;
    }
    // SRL/SRA (shift right logical/arithmetic)
    else if funct3 == Funct3::SrlSra {
        let shamt = (rs2_val & 0x1F) as u32;
        if funct7 == 0 {
            result = (rs1_val as u32).wrapping_shr(shamt) as i32;
        } else {
            result = rs1_val.wrapping_shr(shamt);
        }
    }
    // SLT (set less than)
    else if funct3 == Funct3::Slt {
        result = if rs1_val < rs2_val { 1 } else { 0 };
    }
    // SLTU (set less than unsigned)
    else if funct3 == Funct3::Sltu {
        let rs1_u = state.regs[rs1 as usize];
        let rs2_u = state.regs[rs2 as usize];
        result = if rs1_u < rs2_u { 1 } else { 0 };
    }
    // XOR
    else if funct3 == Funct3::Xor {
        result = rs1_val ^ rs2_val;
    }
    // OR
    else if funct3 == Funct3::Or {
        result = rs1_val | rs2_val;
    }
    // AND
    else if funct3 == Funct3::And {
        result = rs1_val & rs2_val;
    }

    if rd != 0 {
        state.regs[rd as usize] = result as u32;
    }

    Ok(())
}
