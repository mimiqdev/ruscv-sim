//! RISC-V RV64I core module
//!
//! Implements RISC-V processor core fetch-decode-execute cycle

pub mod trap;
use crate::csr::CsrFile;
use crate::decode::InstructionDecoder;
use crate::execute::Executor;
use crate::fpu::{Fcsr, FpuRegisterFile};
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
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

/// RISC-V core state (RV64I)
#[derive(Debug, Clone)]
pub struct CoreState {
    /// 程序计数器 (64-bit for RV64)
    pub pc: u64,
    /// General purpose registers x0-x31 (64-bit for RV64)
    pub regs: [u64; 32],
    /// privilege mode
    pub privilege: PrivilegeMode,
    /// CSR file
    pub csr: CsrFile,
    /// FPU register file (f0-f31)
    pub fpr: FpuRegisterFile,
    /// FCSR (Floating-Point Control and Status Register)
    pub fcsr: Fcsr,
    /// 机器状态寄存器 (简化版) - deprecated, use csr field
    pub mstatus: u64,
    /// 异常程序计数器 - deprecated, use csr field
    pub mepc: u64,
    /// 异常原因 - deprecated, use csr field
    pub mcause: u64,
    /// 异常值 - deprecated, use csr field
    pub mtval: u64,
    /// Track if last instruction was a taken branch (used to skip pc += 4)
    pub branch_taken: bool,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            pc: 0x0000_0000_0000_0000,
            regs: [0; 32],
            privilege: PrivilegeMode::Machine,
            csr: CsrFile::default(),
            fpr: FpuRegisterFile::new(),
            fcsr: Fcsr::new(),
            mstatus: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            branch_taken: false,
        }
    }
}

/// RISC-V core
pub struct RiscvCore {
    /// core state
    state: CoreState,
    /// 指令存储器
    instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// 数据存储器
    data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// Instruction decoder
    decoder: InstructionDecoder,
    /// Executor
    executor: Executor,
    /// TLM interface（可选）
    tlm_interface: Option<Arc<Mutex<dyn TlmInterface>>>,
    /// Base address for virtual address translation (loaded ELF base address)
    base_addr: u64,
}

/// Memory adapter for virtual to physical address translation
///
/// This adapter wraps a MemoryInterface and translates virtual addresses
/// to physical addresses before performing memory operations.
pub struct MemoryAdapter<'a> {
    /// Inner memory interface (mutable reference)
    mem: &'a mut dyn MemoryInterface,
    /// Base address for VA -> PA translation
    base_addr: u64,
}

impl<'a> MemoryAdapter<'a> {
    /// Create new memory adapter
    pub fn new(mem: &'a mut dyn MemoryInterface, base_addr: u64) -> Self {
        Self { mem, base_addr }
    }

    /// Convert virtual address to physical address
    #[inline]
    fn va_to_pa(&self, va: u64) -> u64 {
        va.wrapping_sub(self.base_addr)
    }
}

impl MemoryInterface for MemoryAdapter<'_> {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_dword(self.va_to_pa(addr))
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        self.mem.read_word(self.va_to_pa(addr))
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        self.mem.read_half(self.va_to_pa(addr))
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        self.mem.read_byte(self.va_to_pa(addr))
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_word_zext(self.va_to_pa(addr))
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_half_zext(self.va_to_pa(addr))
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_byte_zext(self.va_to_pa(addr))
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_word_sext(self.va_to_pa(addr))
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_half_sext(self.va_to_pa(addr))
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_byte_sext(self.va_to_pa(addr))
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.mem.write_dword(self.va_to_pa(addr), value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.mem.write_word(self.va_to_pa(addr), value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.mem.write_half(self.va_to_pa(addr), value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.mem.write_byte(self.va_to_pa(addr), value)
    }

    fn size(&self) -> usize {
        self.mem.size()
    }
}

impl RiscvCore {
    /// Create new core instance
    pub fn new(
        instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    ) -> Self {
        Self {
            state: CoreState::default(),
            instruction_mem,
            data_mem,
            decoder: InstructionDecoder::new(),
            executor: Executor::new(),
            tlm_interface: None,
            base_addr: 0,
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
        // Keep PC as virtual address for instruction execution
        // Only convert to physical offset when accessing memory

        let pc_before = self.state.pc;

        // 1. Fetch instruction (convert VA to PA)
        let instruction_addr = self.state.pc.wrapping_sub(self.base_addr);
        let instruction = {
            let mem = self
                .instruction_mem
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to lock instruction memory"))?;
            mem.read_word(instruction_addr)?
        };

        // 2. Decode
        let decoded = self.decoder.decode(instruction)?;

        // Reset branch_taken flag for this instruction
        self.state.branch_taken = false;

        // 3. Execute (use MemoryAdapter for VA -> PA translation in data access)
        {
            let mut mem = self
                .data_mem
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to lock data memory"))?;
            let mut mem_adapter = MemoryAdapter::new(&mut *mem, self.base_addr);
            self.executor
                .execute(&decoded, &mut self.state, &mut mem_adapter)?;
        }

        // 4. Update PC (handled by Executor unless exception)
        if !self.state.branch_taken {
            self.state.pc += 4;
        }

        eprintln!(
            "[STEP] PC: {:#010x} -> {:#010x}, branch_taken={}, instr={:#010x}",
            pc_before, self.state.pc, self.state.branch_taken, instruction
        );

        // Debug: trace gp and sp registers
        eprintln!(
            "[REGS] gp(x3)={}, sp(x2)={}",
            self.state.regs[3], self.state.regs[2]
        );

        Ok(())
    }

    /// Reset core
    /// entry_point: virtual address of entry point
    /// base_addr: base address of loaded ELF (used for VA -> PA translation)
    pub fn reset(&mut self, entry_point: u64, base_addr: u64) {
        self.state = CoreState::default();
        self.state.pc = entry_point;
        self.base_addr = base_addr;
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

        core.reset(0x200, 0x80000000);

        assert_eq!(core.state.pc, 0x200);
        assert_eq!(core.state.regs[1], 0); // regs cleared after reset
    }

    #[test]
    fn test_memory_adapter_va_to_pa() {
        let mut mem = SimpleMemory::new(0x10000);
        let base_addr = 0x80000000u64;

        // Write to physical address 0x100 using raw memory
        mem.write_dword(0x100, 0x12345678).unwrap();

        // Create adapter and read from virtual address
        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x80000100
        // VA 0x80000100 -> PA 0x100
        let value = adapter.read_dword(0x80000100).unwrap();
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_memory_adapter_different_base() {
        let mut mem = SimpleMemory::new(0x10000);
        let base_addr = 0x40000000u64;

        // Write to physical address 0x200
        mem.write_dword(0x200, 0xDEADBEEF).unwrap();

        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x40000200
        let value = adapter.read_dword(0x40000200).unwrap();
        assert_eq!(value, 0xDEADBEEF);
    }

    #[test]
    fn test_memory_adapter_byte_access() {
        let mut mem = SimpleMemory::new(0x1000);
        let base_addr = 0x80000000u64;

        // Write byte to physical address 0x50
        mem.write_byte(0x50, 0xAB).unwrap();

        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x80000050
        let value = adapter.read_byte(0x80000050).unwrap();
        assert_eq!(value, 0xAB);
    }
}
