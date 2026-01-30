//! TLM2.0 interface abstraction
//!
//! Provide SystemC TLM2.0-style interface abstraction for external component communication

use thiserror::Error;

/// TLM transaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmPhase {
    BeginReq,  // Request begin
    EndReq,    // Request end
    BeginResp, // Response begin
    EndResp,   // Response end
}

/// TLM response status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmResponseStatus {
    Ok,              // Success
    AddressError,    // Address error
    CommandError,    // Command error
    BurstError,      // Burst error
    DataError,       // Data error
    InvalidAddress,  // Invalid address
    WaitRequest,     // Wait request
    WaitResponse,    // Wait response
    ReleaseRequired, // Release required
}

/// TLM command type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmCommand {
    Read,
    Write,
}

/// TLM generic transaction
#[derive(Debug, Clone)]
pub struct TlmGenericPayload {
    /// 命令
    pub command: TlmCommand,
    /// 地址
    pub address: u32,
    /// 数据指针
    pub data: Vec<u8>,
    /// 字节使能
    pub byte_enable: Option<Vec<u8>>,
    /// 字节使能长度
    pub byte_enable_length: usize,
    /// 数据长度
    pub data_length: usize,
    /// 流式传输
    pub streaming: bool,
    /// 响应状态
    pub response_status: TlmResponseStatus,
}

impl TlmGenericPayload {
    /// 创建新的事务
    pub fn new(command: TlmCommand, address: u32, data: Vec<u8>) -> Self {
        Self {
            command,
            address,
            data: data.clone(),
            byte_enable: None,
            byte_enable_length: 0,
            data_length: data.len(),
            streaming: false,
            response_status: TlmResponseStatus::Ok,
        }
    }

    /// 设置响应状态
    pub fn set_response_status(&mut self, status: TlmResponseStatus) {
        self.response_status = status;
    }
}

/// TLM synchronization type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmSyncEnum {
    Accept,  // Accept
    Wait,    // Wait
    Release, // Release
}

/// TLM time point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmTime {
    /// 模拟时间（皮秒）
    Ps(u64),
    /// 模拟时间（纳秒）
    Ns(u64),
    /// 模拟时间（微秒）
    Us(u64),
    /// 模拟时间（毫秒）
    Ms(u64),
    /// 模拟时间（秒）
    S(u64),
}

impl TlmTime {
    /// 转换为皮秒
    pub fn to_ps(&self) -> u64 {
        match self {
            TlmTime::Ps(v) => *v,
            TlmTime::Ns(v) => *v * 1000,
            TlmTime::Us(v) => *v * 1_000_000,
            TlmTime::Ms(v) => *v * 1_000_000_000,
            TlmTime::S(v) => *v * 1_000_000_000_000,
        }
    }
}

/// TLM error
#[derive(Error, Debug)]
pub enum TlmError {
    #[error("Invalid address: 0x{0:08x}")]
    InvalidAddress(u32),
    #[error("Invalid transaction length: {0}")]
    InvalidLength(usize),
    #[error("Transaction timeout")]
    Timeout,
    #[error("Bus busy")]
    Busy,
    #[error("Not implemented")]
    NotImplemented,
}

/// TLM 发起者接口 (Initiator)
/// Used to initiate TLM transactions
pub trait TlmInitiatorInterface {
    /// 阻塞传输
    fn b_transport(
        &self,
        trans: &mut TlmGenericPayload,
        time: &mut TlmTime,
    ) -> Result<(), TlmError>;

    /// 非阻塞传输（带时间更新）
    fn nb_transport_fw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        time: &mut TlmTime,
    ) -> Result<TlmSyncEnum, TlmError>;
}

/// TLM 目标接口 (Target)
/// Used to respond to TLM transactions
pub trait TlmTargetInterface {
    /// 阻塞传输回调
    fn b_transport_cb(
        &self,
        trans: &mut TlmGenericPayload,
        time: &mut TlmTime,
    ) -> Result<(), TlmError>;

    /// 非阻塞传输回调
    fn nb_transport_bw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        time: &mut TlmTime,
    ) -> Result<TlmSyncEnum, TlmError>;
}

/// TLM 通用接口 (用于核心与外部交互)
pub trait TlmInterface: Send + Sync {
    /// 读操作 (阻塞)
    fn read(&self, addr: u32, size: usize) -> Result<Vec<u8>, TlmError>;
    /// 写操作 (阻塞)
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), TlmError>;
    /// 获取延迟
    fn get_delay(&self) -> TlmTime;
    /// 设置延迟
    fn set_delay(&mut self, delay: TlmTime);
}

/// TLM 总线 (连接多个TLM组件)
#[allow(dead_code)]
pub struct TlmBus {
    /// 存储器映射
    memory_map: Vec<Box<dyn TlmInterface + Send + Sync>>,
    /// 延迟
    delay: TlmTime,
}

impl TlmBus {
    /// Create new TLM bus
    pub fn new() -> Self {
        Self {
            memory_map: Vec::new(),
            delay: TlmTime::Ns(10), // Default 10ns delay
        }
    }

    /// 添加存储器映射区域
    pub fn add_memory_region(&mut self, region: Box<dyn TlmInterface + Send + Sync>) {
        self.memory_map.push(region);
    }
}

impl Default for TlmBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple memory TLM wrapper
pub struct TlmSimpleMemory {
    /// 基础存储器
    memory: Vec<u8>,
    /// 访问延迟
    delay: TlmTime,
    /// 基地址
    base_addr: u32,
    /// 大小
    size: usize,
}

impl TlmSimpleMemory {
    /// Create new TLM simple memory
    pub fn new(base_addr: u32, size: usize) -> Self {
        Self {
            memory: vec![0; size],
            delay: TlmTime::Ns(1),
            base_addr,
            size,
        }
    }

    /// 加载数据
    pub fn load(&mut self, data: &[u8], offset: u32) {
        for (i, &byte) in data.iter().enumerate() {
            let addr = (offset as usize) + i;
            if addr < self.size {
                self.memory[addr] = byte;
            }
        }
    }
}

impl TlmInterface for TlmSimpleMemory {
    fn read(&self, addr: u32, size: usize) -> Result<Vec<u8>, TlmError> {
        if addr < self.base_addr || addr as usize + size > self.base_addr as usize + self.size {
            return Err(TlmError::InvalidAddress(addr));
        }

        let offset = (addr - self.base_addr) as usize;
        Ok(self.memory[offset..offset + size].to_vec())
    }

    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), TlmError> {
        if addr < self.base_addr || addr as usize + data.len() > self.base_addr as usize + self.size
        {
            return Err(TlmError::InvalidAddress(addr));
        }

        let offset = (addr - self.base_addr) as usize;
        self.memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn get_delay(&self) -> TlmTime {
        self.delay
    }

    fn set_delay(&mut self, delay: TlmTime) {
        self.delay = delay;
    }
}

/// Debug TLM interface
pub struct DebugTlmInterface;

impl TlmInterface for DebugTlmInterface {
    fn read(&self, _addr: u32, size: usize) -> Result<Vec<u8>, TlmError> {
        Ok(vec![0xCC; size])
    }

    fn write(&mut self, _addr: u32, _data: &[u8]) -> Result<(), TlmError> {
        Ok(())
    }

    fn get_delay(&self) -> TlmTime {
        TlmTime::Ns(0)
    }

    fn set_delay(&mut self, _delay: TlmTime) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlm_payload() {
        let payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, vec![0; 4]);
        assert_eq!(payload.command, TlmCommand::Read);
        assert_eq!(payload.address, 0x1000);
        assert_eq!(payload.data_length, 4);
    }

    #[test]
    fn test_tlm_simple_memory() {
        let mut mem = TlmSimpleMemory::new(0x1000, 1024);
        mem.load(&[0x01, 0x02, 0x03, 0x04], 0);

        let data = mem.read(0x1000, 4).unwrap();
        assert_eq!(data, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_tlm_time() {
        assert_eq!(TlmTime::Ns(100).to_ps(), 100_000);
        assert_eq!(TlmTime::Us(1).to_ps(), 1_000_000);
    }
}
