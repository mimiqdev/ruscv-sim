//! R-type instruction execution (RV64I)
//!
//! R-type (Register-type) instructions operate on two source registers
//! and write the result to a destination register.

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3};
use crate::execute::ExecuteError;

/// R-type operation instructions (exec_op) - RV64I
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

    let rs1_val = state.regs[rs1 as usize] as i64;
    let rs2_val = state.regs[rs2 as usize] as i64;
    let mut result: i64 = 0;

    // ADD/SUB
    if funct3 == Funct3::AddSub {
        if funct7 == 0 {
            result = rs1_val.wrapping_add(rs2_val);
        } else if funct7 == 0x20 {
            result = rs1_val.wrapping_sub(rs2_val);
        }
    }
    // SLL (logical left shift) - RV64I uses lower 6 bits of rs2
    else if funct3 == Funct3::Sll {
        let shamt = (rs2_val & 0x3F) as u32;
        result = (state.regs[rs1 as usize].wrapping_shl(shamt)) as i64;
    }
    // SRL/SRA (shift right logical/arithmetic) - RV64I uses lower 6 bits of rs2
    else if funct3 == Funct3::SrlSra {
        let shamt = (rs2_val & 0x3F) as u32;
        if funct7 == 0 {
            result = (state.regs[rs1 as usize].wrapping_shr(shamt)) as i64;
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
        state.regs[rd as usize] = result as u64;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr(
        opcode: Opcode,
        funct3: Option<Funct3>,
        funct7: Option<u8>,
        rs1: Option<u8>,
        rs2: Option<u8>,
        rd: Option<u8>,
    ) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode,
            funct3,
            funct7,
            rs1,
            rs2,
            rs3: None,
            rd,
            imm: None,
            branch_taken: false,
        }
    }

    #[test]
    fn test_add_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    #[test]
    fn test_sub_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 20;
        state.regs[2] = 10;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 10);
    }

    #[test]
    fn test_sub_negative_result() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3] as i32, -10);
    }

    #[test]
    fn test_sll_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001;
        state.regs[2] = 4;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Sll),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_sll_large_shift() {
        let mut state = CoreState::default();
        state.regs[1] = 1;
        state.regs[2] = 8;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Sll),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 256);
    }

    #[test]
    fn test_slt_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;
        state.regs[2] = 5;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Slt),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_slt_false() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Slt),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_slt_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-5i64) as u64;
        state.regs[2] = 10;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Slt),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_sltu_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;
        state.regs[2] = 5;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Sltu),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_sltu_negative_rs1() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i64) as u64;
        state.regs[2] = 5;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Sltu),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_xor_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;
        state.regs[2] = 0b1010_1010;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Xor),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0b0110_1010);
    }

    #[test]
    fn test_xor_with_self() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Xor),
            Some(0),
            Some(1),
            Some(1),
            Some(2),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_srl_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1_0000_0000;
        state.regs[2] = 4;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_srl_with_negative_value() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0;
        state.regs[2] = 4;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x0FFFFFFF);
    }

    #[test]
    fn test_sra_execution() {
        let mut state = CoreState::default();
        // In RV64, use 64-bit sign-extended -16: 0xFFFFFFFFFFFFFFF0
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFF0;
        state.regs[2] = 4;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        // Arithmetic shift right preserves sign: 0xFFFFFFFFFFFFFFF0 >> 4 = 0xFFFFFFFFFFFFFFFF (-1)
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(state.regs[3] as i64, -1);
    }

    #[test]
    fn test_sra_with_positive_value() {
        let mut state = CoreState::default();
        state.regs[1] = 256;
        state.regs[2] = 4;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 16);
    }

    #[test]
    fn test_or_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000;
        state.regs[2] = 0b1010_1010;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Or),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0b1110_1010);
    }

    #[test]
    fn test_or_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;
        state.regs[2] = 0;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Or),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x12345678);
    }

    #[test]
    fn test_and_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1111_1111_0000_0000;
        state.regs[2] = 0b1010_1010_1010_1010;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::And),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0xAA00);
    }

    #[test]
    fn test_and_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;
        state.regs[2] = 0xFFFFFFFF;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::And),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 0x12345678);
    }

    #[test]
    fn test_r_type_rd_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0),
            Some(1),
            Some(2),
            Some(0),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_add_overflow() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0;
        state.regs[2] = 0x00000020;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::AddSub),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3] as i32, 16);
    }

    #[test]
    fn test_sll_shamt_masking() {
        let mut state = CoreState::default();
        state.regs[1] = 1;
        state.regs[2] = 7;

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::Sll),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 128);
    }

    #[test]
    fn test_srl_shamt_masking() {
        let mut state = CoreState::default();
        // In RV64, use full 64-bit value
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFFF;
        // RV64I uses 6-bit shamt mask (0x3F), so 0x28 = 40 means shift by 40
        state.regs[2] = 0x12345628; // lower 6 bits = 0x28 = 40

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFFFFFFFFFF >> 40 = 0x00000000_00FFFFFF
        assert_eq!(state.regs[3], 0x00FF_FFFF);
    }

    #[test]
    fn test_sra_shamt_masking() {
        let mut state = CoreState::default();
        // In RV64, use 64-bit sign-extended -16: 0xFFFFFFFFFFFFFFF0
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFF0;
        // RV64I uses 6-bit shamt mask (0x3F), so 0x24 = 36 means shift by 36
        state.regs[2] = 0x12345624; // lower 6 bits = 0x24 = 36

        let instr = create_test_instr(
            Opcode::Op,
            Some(Funct3::SrlSra),
            Some(0x20),
            Some(1),
            Some(2),
            Some(3),
        );
        let mut mem = SimpleMemory::new(0x1000);

        exec_op(&instr, &mut state, &mut mem).unwrap();

        // Arithmetic shift right by 36, fills with sign bit
        // 0xFFFFFFFFFFFFFFF0 >> 36 = 0xFFFFFFFFFFFFFFFF (-1)
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(state.regs[3] as i64, -1);
    }
}
