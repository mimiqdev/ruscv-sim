//! RISC-V RV32I 核心模块
//! 
//! 实现RISC-V处理器核心的取指-译码-执行循环

use crate::decode::InstructionDecoder;
use crate::execute::Executor;
use crate::memory::MemoryInterface;
use crate::tlm::TlmInterface;
use anyhow::{Result, bail};
use std::sync::{Arc, Mutex};

/// RISC-V 特权模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

/// RISC-V 核心状态
#[derive(Debug, Clone)]
pub struct CoreState {
    /// 程序计数器
    pub pc: u32,
    /// 通用寄存器 x0-x31
    pub regs: [u32; 32],
    /// 特权模式
    pub privilege: PrivilegeMode,
    /// 机器状态寄存器 (简化版)
    pub mstatus: u32,
    /// 异常程序计数器
    pub mepc: u32,
    /// 异常原因
    pub mcause: u32,
    /// 异常值
    pub mtval: u32,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            pc: 0x0000_0000,
            regs: [0; 32],
            privilege: PrivilegeMode::Machine,
            mstatus: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
        }
    }
}

/// RISC-V 核心
pub struct RiscvCore {
    /// 核心状态
    state: CoreState,
    /// 指令存储器接口
    instruction_mem: Arc<dyn MemoryInterface + Send + Sync>,
    /// 数据存储器接口
    data_mem: Arc<dyn MemoryInterface + Send + Sync>,
    /// 指令译码器
    decoder: InstructionDecoder,
    /// 执行器
    executor: Executor,
    /// TLM接口（可选）
    tlm_interface: Option<Arc<Mutex<dyn TlmInterface>>>,
}

impl RiscvCore {
    /// 创建新的核心实例
    pub fn new(
        instruction_mem: Arc<dyn MemoryInterface + Send + Sync>,
        data_mem: Arc<dyn MemoryInterface + Send + Sync>,
    ) -> Self {
        Self {
            state: CoreState::default(),
            instruction_mem,
            data_mem,
            decoder: InstructionDecoder::new(),
            executor: Executor::new(),
            tlm_interface: None,
        }
    }

    /// 设置TLM接口
    pub fn set_tlm_interface(&mut self, tlm: Arc<Mutex<dyn TlmInterface>>) {
        self.tlm_interface = Some(tlm);
    }

    /// 获取核心状态（只读）
    pub fn state(&self) -> &CoreState {
        &self.state
    }

    /// 获取可变核心状态
    pub fn state_mut(&mut self) -> &mut CoreState {
        &mut self.state
    }

    /// 单步执行
    pub fn step(&mut self) -> Result<()> {
        // 1. 取指
        let instruction = self.fetch_instruction()?;
        
        // 2. 译码
        let decoded = self.decoder.decode(instruction)?;
        
        // 3. 执行
        self.executor.execute(&decoded, &mut self.state, self.data_mem.as_ref())?;
        
        // 4. 更新PC（由执行器处理，除非发生异常）
        if !decoded.branch_taken {
            self.state.pc += 4;
        }
        
        Ok(())
    }

    /// 取指
    fn fetch_instruction(&mut self) -> Result<u32> {
        let instruction = self.instruction_mem.read_word(self.state.pc)?;
        Ok(instruction.to_le())
    }

    /// 重置核心
    pub fn reset(&mut self, entry_point: u32) {
        self.state = CoreState::default();
        self.state.pc = entry_point;
    }

    /// 运行直到停止
    pub fn run(&mut self, max_cycles: u64) -> Result<u64> {
        let mut cycles = 0;
        while cycles < max_cycles {
            self.step()?;
            cycles += 1;
        }
        Ok(cycles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::SimpleMemory;

    #[test]
    fn test_core_initialization() {
        let mem = Arc::new(SimpleMemory::new(0x1000));
        let core = RiscvCore::new(mem.clone(), mem);
        
        assert_eq!(core.state.pc, 0);
        assert_eq!(core.state.regs[0], 0); // x0 始终为0
        assert_eq!(core.state.privilege, PrivilegeMode::Machine);
    }

    #[test]
    fn test_core_reset() {
        let mem = Arc::new(SimpleMemory::new(0x1000));
        let mut core = RiscvCore::new(mem.clone(), mem);
        
        core.state.pc = 0x100;
        core.state.regs[1] = 42;
        
        core.reset(0x200);
        
        assert_eq!(core.state.pc, 0x200);
        assert_eq!(core.state.regs[1], 0); // reset后regs被清零
    }
}
