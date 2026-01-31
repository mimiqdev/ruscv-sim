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
        state.regs[1] = 0b1111_1111_0000_0000;

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::And),
            None,
            Some(1),
            Some(2),
            Some(0b1010_1010_1010_1010),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xAA00);
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
        state.regs[1] = 0xFFFFFFF0;

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

        assert_eq!(state.regs[2] as i32, -1);
    }

    #[test]
    fn test_lw_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        let mut mem = SimpleMemory::new(0x1000);
        mem.write_word(0x104, 0x12345678).unwrap();

        let instr = create_test_instr_i_type(
            Opcode::Load,
            Some(Funct3::AddSub),
            None,
            Some(1),
            Some(2),
            Some(4),
        );
        exec_load(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
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
            Some((-1i32) as u32), // 0xFFFFFFFF
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xFFFFFFFF);
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
        state.regs[1] = 0xFFFFFFFF; // -1 as i32

        let instr = create_test_instr_i_type(
            Opcode::OpImm,
            Some(Funct3::Xor),
            None,
            Some(1),
            Some(2),
            Some((-1i32) as u32), // 0xFFFFFFFF
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op_imm(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFF ^ 0xFFFFFFFF = 0
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
        state.regs[1] = (-1i32) as u32; // 0xFFFFFFFF (large unsigned)

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
