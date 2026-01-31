//! RISC-V RV32I core module
//!
//! Implements RISC-V processor core fetch-decode-execute cycle

pub mod trap;
use crate::csr::CsrFile;
use crate::decode::InstructionDecoder;
use crate::execute::Executor;
use crate::memory::{MemoryInterface, SimpleMemory};
use crate::tlm::TlmInterface;
use anyhow::Result;
use std::sync::{Arc, Mutex};
pub use trap::{ExceptionCause, InterruptCause, Trap, TrapContext, TrapDelegation, TrapHandler};

/// RISC-V privilege mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

/// RISC-V core state
#[derive(Debug, Clone)]
pub struct CoreState {
    /// 程序计数器
    pub pc: u32,
    /// General purpose registers x0-x31
    pub regs: [u32; 32],
    /// privilege mode
    pub privilege: PrivilegeMode,
    /// CSR file
    pub csr: CsrFile,
    /// 机器状态寄存器 (简化版) - deprecated, use csr field
    pub mstatus: u32,
    /// 异常程序计数器 - deprecated, use csr field
    pub mepc: u32,
    /// 异常原因 - deprecated, use csr field
    pub mcause: u32,
    /// 异常值 - deprecated, use csr field
    pub mtval: u32,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            pc: 0x0000_0000,
            regs: [0; 32],
            privilege: PrivilegeMode::Machine,
            csr: CsrFile::default(),
            mstatus: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
        }
    }
}

/// RISC-V core
pub struct RiscvCore {
    /// core state
    state: CoreState,
    /// 指令存储器
    instruction_mem: Arc<Mutex<SimpleMemory>>,
    /// 数据存储器
    data_mem: Arc<Mutex<SimpleMemory>>,
    /// Instruction decoder
    decoder: InstructionDecoder,
    /// Executor
    executor: Executor,
    /// TLM interface（可选）
    tlm_interface: Option<Arc<Mutex<dyn TlmInterface>>>,
}

impl RiscvCore {
    /// Create new core instance
    pub fn new(
        instruction_mem: Arc<Mutex<SimpleMemory>>,
        data_mem: Arc<Mutex<SimpleMemory>>,
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

    /// 使用相同存储器创建core（指令+数据共用）
    pub fn new_with_memory(mem_size: usize) -> Self {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));
        Self::new(mem.clone(), mem)
    }

    /// Set TLM interface
    pub fn set_tlm_interface(&mut self, tlm: Arc<Mutex<dyn TlmInterface>>) {
        self.tlm_interface = Some(tlm);
    }

    /// Get core state (read-only)
    pub fn state(&self) -> &CoreState {
        &self.state
    }

    /// Get mutable core state
    pub fn state_mut(&mut self) -> &mut CoreState {
        &mut self.state
    }

    /// Step execute
    pub fn step(&mut self) -> Result<()> {
        // 1. Fetch
        let instruction = {
            let mem = self
                .instruction_mem
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to lock instruction memory"))?;
            mem.read_word(self.state.pc)?
        };

        // 2. Decode
        let decoded = self.decoder.decode(instruction)?;

        // 3. Execute
        {
            let mut mem = self
                .data_mem
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to lock data memory"))?;
            self.executor
                .execute(&decoded, &mut self.state, &mut *mem)?;
        }

        // 4. 更新PC（由Executor处理，除非发生异常）
        if !decoded.branch_taken {
            self.state.pc += 4;
        }

        Ok(())
    }

    /// Reset core
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

    #[test]
    fn test_core_initialization() {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
        let core = RiscvCore::new(mem.clone(), mem);

        assert_eq!(core.state.pc, 0);
        assert_eq!(core.state.regs[0], 0); // x0 always returns 0
        assert_eq!(core.state.privilege, PrivilegeMode::Machine);
    }

    #[test]
    fn test_core_reset() {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
        let mut core = RiscvCore::new(mem.clone(), mem);

        core.state.pc = 0x100;
        core.state.regs[1] = 42;

        core.reset(0x200);

        assert_eq!(core.state.pc, 0x200);
        assert_eq!(core.state.regs[1], 0); // regs cleared after reset
    }
}
