//! TLM2.0 接口抽象层
//!
//! 提供SystemC TLM2.0风格的接口抽象，用于与外部组件通信

use thiserror::Error;
use std::sync::{Arc, Mutex};

/// TLM 传输类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmPhase {
    BeginReq,   // 请求开始
    EndReq,     // 请求结束
    BeginResp,  // 响应开始
    EndResp,    // 响应结束
}

/// TLM 响应状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmResponseStatus {
    Ok,                     // 成功
    AddressError,           // 地址错误
    CommandError,           // 命令错误
    BurstError,             // 突发错误
    DataError,              // 数据错误
    InvalidAddress,         // 无效地址
    WaitRequest,            // 等待请求
    WaitResponse,           // 等待响应
    ReleaseRequired,        // 需要释放
}

/// TLM 命令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmCommand {
    Read,
    Write,
}

/// TLM 通用事务
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
            data,
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

/// TLM 同步类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmSyncEnum {
    Accept,     // 接受
    Wait,       // 等待
    Release,    // 释放
}

/// TLM 时间点
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

/// TLM 错误
#[derive(Error, Debug)]
pub enum TlmError {
    #[error("无效的地址: 0x{0:08x}")]
    InvalidAddress(u32),
    #[error("无效的传输长度: {0}")]
    InvalidLength(usize),
    #[error("传输超时")]
    Timeout,
    #[error("总线忙")]
    Busy,
    #[error("未实现")]
    NotImplemented,
}

/// TLM 发起者接口 (Initiator)
/// 用于发起TLM事务
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
/// 用于响应TLM事务
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
    fn write(&self, addr: u32, data: &[u8]) -> Result<(), TlmError>;
    /// 获取延迟
    fn get_delay(&self) -> TlmTime;
    /// 设置延迟
    fn set_delay(&self, delay: TlmTime);
}

/// TLM 总线 (连接多个TLM组件)
pub struct TlmBus {
    /// 存储器映射
    memory_map: Vec<Box<dyn TlmInterface + Send + Sync>>,
    /// 延迟
    delay: TlmTime,
}

impl TlmBus {
    /// 创建新的TLM总线
    pub fn new() -> Self {
        Self {
            memory_map: Vec::new(),
            delay: TlmTime::Ns(10), // 默认10ns延迟
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

/// 简单内存 TLM 封装
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
    /// 创建新的TLM简单存储器
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

    fn write(&self, addr: u32, data: &[u8]) -> Result<(), TlmError> {
        if addr < self.base_addr || addr as usize + data.len() > self.base_addr as usize + self.size {
            return Err(TlmError::InvalidAddress(addr));
        }
        
        let offset = (addr - self.base_addr) as usize;
        self.memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn get_delay(&self) -> TlmTime {
        self.delay
    }

    fn set_delay(&self, delay: TlmTime) {
        // 简单实现，忽略
    }
}

/// 调试 TLM 接口
pub struct DebugTlmInterface;

impl TlmInterface for DebugTlmInterface {
    fn read(&self, addr: u32, size: usize) -> Result<Vec<u8>, TlmError> {
        Ok(vec![0xCC; size])
    }

    fn write(&self, _addr: u32, _data: &[u8]) -> Result<(), TlmError> {
        Ok(())
    }

    fn get_delay(&self) -> TlmTime {
        TlmTime::Ns(0)
    }

    fn set_delay(&self, _delay: TlmTime) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlm_payload() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, vec![0; 4]);
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
