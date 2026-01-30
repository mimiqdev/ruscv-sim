//! 执行模块
//!
//! 实现RV32I指令的执行逻辑

use crate::core::{CoreState, PrivilegeMode};
use crate::decode::{DecodedInstruction, Funct3, Opcode};
use crate::memory::{MemoryError, MemoryInterface};
use thiserror::Error;

/// 执行错误
#[derive(Error, Debug)]
pub enum ExecuteError {
    #[error("未对齐的内存访问: 地址 0x{0:08x}, 对齐要求 {1}")]
    MisalignedAccess(u32, u32),
    #[error("无效的寄存器访问: x{0}")]
    InvalidRegister(u8),
    #[error("无效的操作")]
    InvalidOperation,
    #[error("ECALL异常")]
    Ecall,
    #[error("EBREAK异常")]
    Ebreak,
    #[error("内存访问错误: {0}")]
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
            Opcode::Op => self.exec_op(instr, state),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// LUI (Load Upper Immediate)
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

    /// AUIPC (Add Upper Immediate to PC)
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

    /// JAL (Jump and Link)
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

    /// JALR (Jump and Link Register)
    fn exec_jalr(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
            let return_addr = state.pc.wrapping_add(4);
            let base = state.regs[rs1 as usize];
            let target = (base.wrapping_add(imm)) & !1u32; // LSB清零

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
                // BLT (有符号比较)
                (rs1_val as i32) < (rs2_val as i32)
            }
            Funct3::Xor => {
                // BGE (有符号比较)
                (rs1_val as i32) >= (rs2_val as i32)
            }
            Funct3::SrlSra => {
                // BLTU (无符号比较)
                rs1_val < rs2_val
            }
            _ => false,
        };

        // 注意：实际的分支条件需要根据funct3值修正
        // BEQ=000, BNE=001, BLT=100, BGE=101, BLTU=110, BGEU=111

        if let Some(branch_taken) = state.pc.checked_add(4) {
            // 临时存储，在调用者中更新
            if take_branch {
                state.pc = state.pc.wrapping_add(imm);
            }
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

    /// R-type 操作指令
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
        // SLL
        else if funct3 == Funct3::Sll {
            let shamt = (rs2_val & 0x1F) as u32;
            result = (rs1_val as u32).wrapping_shl(shamt) as i32;
        }
        // SRL/SRA
        else if funct3 == Funct3::SrlSra {
            let shamt = (rs2_val & 0x1F) as u32;
            if funct7 == 0 {
                result = (rs1_val as u32).wrapping_shr(shamt) as i32;
            } else {
                result = rs1_val.wrapping_shr(shamt);
            }
        }
        // SLT
        else if funct3 == Funct3::Slt {
            result = if rs1_val < rs2_val { 1 } else { 0 };
        }
        // SLTU
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
        executor.execute(&instr, &mut state, &mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }
}
