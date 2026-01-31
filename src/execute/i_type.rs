//! I-type instruction execution
//!
//! I-type (Immediate-type) instructions operate on a source register
//! and an immediate value, writing the result to a destination register.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Load instructions (exec_load)
///
/// Executes load instructions including:
/// - LB/LH/LW: Load byte/halfword/word (sign-extended)
/// - LBU/LHU: Load byte/halfword (zero-extended)
#[inline]
pub fn exec_load(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let base = state.regs[rs1 as usize];
    let addr = base.wrapping_add(imm);

    let value = match funct3 {
        Funct3::AddSub => mem.read_word(addr).map(|v| v as i32 as u32)?, // LW
        Funct3::Sll => mem.read_half(addr).map(|v| v as i16 as i32 as u32)?, // LH
        Funct3::Slt => mem.read_byte(addr).map(|v| v as i8 as i32 as u32)?, // LB
        Funct3::Sltu => mem.read_half_zext(addr)?,                       // LHU
        Funct3::Xor => mem.read_byte_zext(addr)?,                        // LBU
        _ => return Err(ExecuteError::InvalidOperation),
    };

    if rd != 0 {
        state.regs[rd as usize] = value;
    }

    Ok(())
}

/// I-type operation instructions (exec_op_imm)
///
/// Executes I-type arithmetic/logical instructions including:
/// - ADDI: Add Immediate
/// - SLLI: Shift Left Logical Immediate
/// - SLTI/SLTIU: Set Less Than Immediate (signed/unsigned)
/// - XORI: Exclusive OR Immediate
/// - SRLI/SRAI: Shift Right Logical/Arithmetic Immediate
/// - ORI: OR Immediate
/// - ANDI: AND Immediate
#[inline]
pub fn exec_op_imm(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
        (instr.rd, instr.rs1, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    // Extract shamt from imm[4:0] for shift instructions
    let shamt = imm & 0x1F;

    let result: i32 = match funct3 {
        // ADDI (add immediate)
        Funct3::AddSub => {
            let rs1_val = state.regs[rs1 as usize] as i32;
            let imm_val = imm as i32;
            rs1_val.wrapping_add(imm_val)
        }
        // SLLI (shift left logical immediate)
        Funct3::Sll => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val.wrapping_shl(shamt)) as i32
        }
        // SLTI (set less than immediate)
        Funct3::Slt => {
            let rs1_val = state.regs[rs1 as usize] as i32;
            let imm_val = imm as i32;
            if rs1_val < imm_val {
                1
            } else {
                0
            }
        }
        // SLTIU (set less than immediate unsigned)
        Funct3::Sltu => {
            let rs1_val = state.regs[rs1 as usize];
            if rs1_val < imm {
                1
            } else {
                0
            }
        }
        // XORI (exclusive or immediate)
        Funct3::Xor => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val ^ imm) as i32
        }
        // SRLI/SRAI (shift right logical/arithmetic immediate)
        Funct3::SrlSra => {
            let rs1_val = state.regs[rs1 as usize];
            // Distinguish SRLI (funct7=0x00) vs SRAI (funct7=0x20)
            match instr.funct7 {
                Some(0x00) => (rs1_val.wrapping_shr(shamt)) as i32, // SRLI
                Some(0x20) => (rs1_val as i32).wrapping_shr(shamt), // SRAI
                _ => return Err(ExecuteError::InvalidOperation),
            }
        }
        // ORI (or immediate)
        Funct3::Or => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val | imm) as i32
        }
        // ANDI (and immediate)
        Funct3::And => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val & imm) as i32
        }
    };

    if rd != 0 {
        state.regs[rd as usize] = result as u32;
    }

    Ok(())
}
