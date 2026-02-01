//! I-type instruction execution (RV64I)
//!
//! I-type (Immediate-type) instructions operate on a source register
//! and an immediate value, writing the result to a destination register.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Load instructions (exec_load) - RV64I
///
/// Executes load instructions including:
/// - LB/LH/LW/LD: Load byte/halfword/word/doubleword (sign-extended)
/// - LBU/LHU/LWU: Load byte/halfword/word (zero-extended)
///
/// RV64I funct3 encoding:
/// - 000: LB (Load Byte, sign-extend)
/// - 001: LH (Load Halfword, sign-extend)
/// - 010: LW (Load Word, sign-extend)
/// - 011: LD (Load Doubleword)
/// - 100: LBU (Load Byte Unsigned)
/// - 101: LHU (Load Halfword Unsigned)
/// - 110: LWU (Load Word Unsigned)
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
    // Sign-extend the 12-bit immediate to 64 bits
    let imm_sext = ((imm as i32) << 20 >> 20) as i64 as u64;
    let addr = base.wrapping_add(imm_sext);

    let funct3_val = funct3 as u8;
    let value = match funct3_val {
        0b010 => mem.read_word_sext(addr)?, // LW (sign-extend to 64-bit)
        0b011 => mem.read_dword(addr)?,     // LD (load doubleword)
        0b001 => mem.read_half_sext(addr)?, // LH (sign-extend)
        0b000 => mem.read_byte_sext(addr)?, // LB (sign-extend)
        0b101 => mem.read_half_zext(addr)?, // LHU (zero-extend)
        0b100 => mem.read_byte_zext(addr)?, // LBU (zero-extend)
        0b110 => mem.read_word_zext(addr)?, // LWU (zero-extend to 64-bit)
        _ => return Err(ExecuteError::InvalidOperation),
    };

    if rd != 0 {
        state.regs[rd as usize] = value;
    }

    Ok(())
}

/// I-type operation instructions (exec_op_imm) - RV64I
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

    // Sign-extend the 12-bit immediate to 64 bits
    let imm_sext = ((imm as i32) << 20 >> 20) as i64;
    // Extract shamt from imm[5:0] for shift instructions (RV64I uses 6-bit shamt)
    let shamt = (imm & 0x3F) as u32;

    let result: i64 = match funct3 {
        // ADDI (add immediate)
        Funct3::AddSub => {
            let rs1_val = state.regs[rs1 as usize] as i64;
            rs1_val.wrapping_add(imm_sext)
        }
        // SLLI (shift left logical immediate)
        Funct3::Sll => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val.wrapping_shl(shamt)) as i64
        }
        // SLTI (set less than immediate)
        Funct3::Slt => {
            let rs1_val = state.regs[rs1 as usize] as i64;
            if rs1_val < imm_sext {
                1
            } else {
                0
            }
        }
        // SLTIU (set less than immediate unsigned)
        Funct3::Sltu => {
            let rs1_val = state.regs[rs1 as usize];
            let imm_u = imm_sext as u64;
            if rs1_val < imm_u {
                1
            } else {
                0
            }
        }
        // XORI (exclusive or immediate)
        Funct3::Xor => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val ^ (imm_sext as u64)) as i64
        }
        // SRLI/SRAI (shift right logical/arithmetic immediate)
        Funct3::SrlSra => {
            let rs1_val = state.regs[rs1 as usize];
            // Distinguish SRLI (funct7=0x00) vs SRAI (funct7=0x20)
            match instr.funct7 {
                Some(f7) if (f7 & 0x20) == 0 => (rs1_val.wrapping_shr(shamt)) as i64, // SRLI
                Some(f7) if (f7 & 0x20) != 0 => (rs1_val as i64).wrapping_shr(shamt), // SRAI
                _ => return Err(ExecuteError::InvalidOperation),
            }
        }
        // ORI (or immediate)
        Funct3::Or => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val | (imm_sext as u64)) as i64
        }
        // ANDI (and immediate)
        Funct3::And => {
            let rs1_val = state.regs[rs1 as usize];
            (rs1_val & (imm_sext as u64)) as i64
        }
    };

    if rd != 0 {
        state.regs[rd as usize] = result as u64;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_i_type(
        opcode: Opcode,
        funct3: Option<Funct3>,
        funct7: Option<u8>,
        rs1: Option<u8>,
        rd: Option<u8>,
        imm: Option<u32>,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode,
            funct3,
            funct7,
            rs1,
            rs2: None,
            rs3: None,
            rd,
            imm,
            branch_taken: false,
        }
    }

    #[test]
    fn test_addi_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::AddSub),
            None,
            Some(1),
            Some(2),
            Some(5),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_addi_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::AddSub),
            None,
            Some(1),
            Some(2),
            Some((-3i32) as u32),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2] as i32, 7);
    }

    #[test]
    fn test_slti_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Slt),
            None,
            Some(1),
            Some(2),
            Some(5),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_sltiu_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Sltu),
            None,
            Some(1),
            Some(2),
            Some(5),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_xori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Xor),
            None,
            Some(1),
            Some(2),
            Some(0b1010_1010),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0b0110_1010);
    }

    #[test]
    fn test_ori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Or),
            None,
            Some(1),
            Some(2),
            Some(0b1010_1010),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0b1110_1010);
    }

    #[test]
    fn test_andi_execution() {
        let mut state = CoreState::default();
        // Use a value that will have meaningful AND result with 12-bit immediate
        state.regs[1] = 0x0000_00FF; // bits 0-7 set

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::And),
            None,
            Some(1),
            Some(2),
            Some(0x0AA), // 12-bit immediate (positive, bits 1,3,5,7 set)
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // 0xFF & 0xAA = 0xAA
        assert_eq!(state.regs[2], 0xAA);
    }

    #[test]
    fn test_slli_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Sll),
            None,
            Some(1),
            Some(2),
            Some(4),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srli_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1_0000_0000;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::SrlSra),
            Some(0x00),
            Some(1),
            Some(2),
            Some(4),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srai_execution() {
        let mut state = CoreState::default();
        // In RV64, use 64-bit sign-extended -16: 0xFFFFFFFFFFFFFFF0
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFF0;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::SrlSra),
            Some(0x20),
            Some(1),
            Some(2),
            Some(4),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // SRAI: arithmetic shift right by 4, fills with sign bit
        // 0xFFFFFFFFFFFFFFF0 >> 4 = 0xFFFFFFFFFFFFFFFF (-1)
        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(state.regs[2] as i64, -1);
    }

    #[test]
    fn test_lw_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_word(0x104, 0x12345678).unwrap();

        let instr = create_test_instr_i_type(
            Opcode::Load,
            Some(Funct3::Slt), // LW uses funct3=0b010 (Funct3::Slt in enum)
            None,
            Some(1),
            Some(2),
            Some(4),
        );
        exec_load(&instr, &mut state, &mut mem).unwrap();

        // LW sign-extends 32-bit value to 64-bit; 0x12345678 is positive, so zero-extends
        assert_eq!(state.regs[2], 0x0000_0000_12345678);
    }

    #[test]
    fn test_ori_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Or),
            None,
            Some(1),
            Some(2),
            Some(0),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
    }

    #[test]
    fn test_ori_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Or),
            None,
            Some(1),
            Some(2),
            Some((-1i32) as u32), // 12-bit immediate sign-extends to 64-bit -1
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // -1 sign-extended to 64-bit is 0xFFFFFFFFFFFFFFFF
        assert_eq!(state.regs[2], 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_andi_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::And),
            None,
            Some(1),
            Some(2),
            Some(0),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_andi_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::And),
            None,
            Some(1),
            Some(2),
            Some((-1i32) as u32), // 0xFFFFFFFF
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
    }

    #[test]
    fn test_slli_large_shift() {
        let mut state = CoreState::default();
        state.regs[1] = 1;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Sll),
            None,
            Some(1),
            Some(2),
            Some(8), // shamt = 8
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 256);
    }

    #[test]
    fn test_srli_with_negative_value() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0; // Large unsigned value

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::SrlSra),
            Some(0x00), // SRLI
            Some(1),
            Some(2),
            Some(4), // shamt = 4
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFF0 >> 4 = 0x0FFFFFFF
        assert_eq!(state.regs[2], 0x0FFFFFFF);
    }

    #[test]
    fn test_srai_with_positive_value() {
        let mut state = CoreState::default();
        state.regs[1] = 256; // Positive value

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::SrlSra),
            Some(0x20), // SRAI
            Some(1),
            Some(2),
            Some(4), // shamt = 4
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // 256 >> 4 = 16
        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_xori_with_negative_immediate() {
        let mut state = CoreState::default();
        // In RV64, -1 is 0xFFFFFFFFFFFFFFFF
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFFF;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Xor),
            None,
            Some(1),
            Some(2),
            Some((-1i32) as u32), // sign-extends to 64-bit -1
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFFFFFFFFFF ^ 0xFFFFFFFFFFFFFFFF = 0
        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_slti_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTI x2, x1, -5 (should be false since 10 > -5)
        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Slt),
            None,
            Some(1),
            Some(2),
            Some((-5i32) as u32),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_negative_rs1() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i64) as u64; // 0xFFFFFFFF (large unsigned)

        // SLTIU x2, x1, 5 (should be false since 0xFFFFFFFF > 5)
        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Sltu),
            None,
            Some(1),
            Some(2),
            Some(5),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTIU x2, x1, -5 (0xFFFFFFFB)
        // In unsigned comparison, 10 < 0xFFFFFFFB is true
        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Sltu),
            None,
            Some(1),
            Some(2),
            Some((-5i32) as u32),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }
}
