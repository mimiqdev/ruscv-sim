//! 执行模块
//!
//! RV32I instruction execution

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3, Opcode};
use crate::memory::{MemoryError, MemoryInterface};
use thiserror::Error;

/// 执行错误
#[derive(Error, Debug)]
pub enum ExecuteError {
    #[error("Misaligned memory access: addr 0x{0:08x}, alignment {1}")]
    MisalignedAccess(u32, u32),
    #[error("Invalid register access: x{0}")]
    InvalidRegister(u8),
    #[error("Invalid operation")]
    InvalidOperation,
    #[error("ECALL exception")]
    Ecall,
    #[error("EBREAK exception")]
    Ebreak,
    #[error("Memory access error: {0}")]
    MemoryError(#[from] MemoryError),
}

/// 执行器
pub struct Executor {}

impl Executor {
    /// 创建新的执行器
    pub fn new() -> Self {
        Self {}
    }

    /// 执行译码后的指令
    pub fn execute(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        match instr.opcode {
            Opcode::Lui => self.exec_lui(instr, state),
            Opcode::Auipc => self.exec_auipc(instr, state),
            Opcode::Jal => self.exec_jal(instr, state),
            Opcode::Jalr => self.exec_jalr(instr, state),
            Opcode::Branch => self.exec_branch(instr, state),
            Opcode::Load => self.exec_load(instr, state, mem),
            Opcode::Store => self.exec_store(instr, state, mem),
            Opcode::OpImm => self.exec_op_imm(instr, state),
            Opcode::Op => self.exec_op(instr, state),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// LUI (Load Upper Immediate (LUI) (LUI))
    fn exec_lui(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
            if rd != 0 {
                state.regs[rd as usize] = imm;
            }
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// AUIPC (Add Upper Immediate to PC (AUIPC) (AUIPC))
    fn exec_auipc(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
            if rd != 0 {
                state.regs[rd as usize] = state.pc.wrapping_add(imm);
            }
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// JAL (Jump and Link (JAL) (JAL))
    fn exec_jal(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
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

    /// JALR (Jump and Link (JAL) (JAL) Register)
    fn exec_jalr(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
            let return_addr = state.pc.wrapping_add(4);
            let base = state.regs[rs1 as usize];
            let target = (base.wrapping_add(imm)) & !1u32; // LSB cleared

            if rd != 0 {
                state.regs[rd as usize] = return_addr;
            }

            state.pc = target;
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// 分支指令
    fn exec_branch(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
            (instr.rs1, instr.rs2, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let rs1_val = state.regs[rs1 as usize];
        let rs2_val = state.regs[rs2 as usize];
        let take_branch = match funct3 {
            Funct3::AddSub => rs1_val == rs2_val, // BEQ
            Funct3::Slt => rs1_val != rs2_val,    // BNE
            Funct3::Sltu => {
                // BLT (signed comparison)
                (rs1_val as i32) < (rs2_val as i32)
            }
            Funct3::Xor => {
                // BGE (signed comparison)
                (rs1_val as i32) >= (rs2_val as i32)
            }
            Funct3::SrlSra => {
                // BLTU (unsigned comparison)
                rs1_val < rs2_val
            }
            _ => false,
        };

        // 注意：Branch conditions need correction based on funct3 value
        // BEQ=000, BNE=001, BLT=100, BGE=101, BLTU=110, BGEU=111

        if take_branch {
            state.pc = state.pc.wrapping_add(imm);
        }

        Ok(())
    }

    /// 加载指令
    fn exec_load(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &dyn MemoryInterface,
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

    /// 存储指令
    fn exec_store(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
            (instr.rs1, instr.rs2, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let base = state.regs[rs1 as usize];
        let addr = base.wrapping_add(imm);
        let value = state.regs[rs2 as usize];

        match funct3 {
            Funct3::AddSub => mem.write_word(addr, value)?, // SW
            Funct3::Sll => mem.write_half(addr, value as u16)?, // SH
            Funct3::Slt => mem.write_byte(addr, value as u8)?, // SB
            _ => return Err(ExecuteError::InvalidOperation),
        }

        Ok(())
    }

    /// I-type operation instructions
    fn exec_op_imm(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
            (instr.rd, instr.rs1, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let result: i32 = match funct3 {
            // ADDI (add immediate)
            Funct3::AddSub => {
                let rs1_val = state.regs[rs1 as usize] as i32;
                let imm_val = imm as i32;
                rs1_val.wrapping_add(imm_val)
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
            // ORI (or immediate)
            Funct3::Or => {
                let rs1_val = state.regs[rs1 as usize];
                (rs1_val | imm) as i32
            }
            _ => return Err(ExecuteError::InvalidOperation),
        };

        if rd != 0 {
            state.regs[rd as usize] = result as u32;
        }

        Ok(())
    }

    /// R-type operation instructions
    fn exec_op(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
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
        // SLT (set less than)U
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
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::SimpleMemory;

    #[test]
    fn test_lui_execution() {
        let mut state = CoreState::default();
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::UType,
            opcode: Opcode::Lui,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: Some(1),
            imm: Some(0x12345000),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x12345000);
    }

    #[test]
    fn test_add_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(Funct3::AddSub),
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    #[test]
    fn test_addi_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::AddSub),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_addi_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // ADDI x2, x1, -3
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::AddSub),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-3i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2] as i32, 7);
    }

    #[test]
    fn test_slti_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        // SLTI x2, x1, 5
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Slt),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_slti_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTI x2, x1, -5 (should be false since 10 > -5)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Slt),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-5i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        // SLTIU x2, x1, 5
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_sltiu_negative_rs1() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i32) as u32; // 0xFFFFFFFF (large unsigned)

        // SLTIU x2, x1, 5 (should be false since 0xFFFFFFFF > 5)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTIU x2, x1, -5 (0xFFFFFFFB)
        // In unsigned comparison, 10 < 0xFFFFFFFB is true
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-5i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_xori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000; // 192

        // XORI x2, x1, 0b1010_1010 (0xAA)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Xor),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0b1010_1010), // 0xAA
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0b1100_0000 ^ 0b1010_1010 = 0b0110_1010 = 0x6A = 106
        assert_eq!(state.regs[2], 0b0110_1010);
    }

    #[test]
    fn test_xori_with_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFFF; // -1 as i32

        // XORI x2, x1, -1 (0xFFFFFFFF)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Xor),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-1i32) as u32), // 0xFFFFFFFF
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFF ^ 0xFFFFFFFF = 0
        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_ori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000; // 192

        // ORI x2, x1, 0b1010_1010 (0xAA)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0b1010_1010), // 0xAA
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0b1100_0000 | 0b1010_1010 = 0b1110_1010 = 0xEA = 234
        assert_eq!(state.regs[2], 0b1110_1010);
    }

    #[test]
    fn test_ori_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ORI x2, x1, 0 (should keep the value)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
    }

    #[test]
    fn testori_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ORI x2, x1, -1 (0xFFFFFFFF) should result in all 1s
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-1i32) as u32), // 0xFFFFFFFF
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xFFFFFFFF);
    }
}
