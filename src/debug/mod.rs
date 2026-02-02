//! GDB 远程调试支持模块
//!
//! 实现 GDB Remote Serial Protocol (RSP) 用于远程调试 RISC-V 模拟器。
//! 支持断点、观察点、寄存器和内存访问。

pub mod breakpoint;
pub mod cli;
pub mod gdb_server;
pub mod rsp;
pub mod watchpoint;

pub use breakpoint::{Breakpoint, BreakpointManager, BreakpointType};
pub use cli::DebugCli;
pub use gdb_server::{GdbServer, GdbServerConfig, GdbServerState};
pub use rsp::{GdbPacket, RspProtocol};
pub use watchpoint::{Watchpoint, WatchpointManager, WatchpointType};

use thiserror::Error;

/// 调试模块错误类型
#[derive(Error, Debug)]
pub enum DebugError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid packet format: {0}")]
    InvalidPacket(String),

    #[error("Checksum mismatch: expected {expected:02x}, got {actual:02x}")]
    ChecksumMismatch { expected: u8, actual: u8 },

    #[error("Unsupported command: {0}")]
    UnsupportedCommand(String),

    #[error("Invalid register number: {0}")]
    InvalidRegister(u32),

    #[error("Invalid address: 0x{0:016x}")]
    InvalidAddress(u64),

    #[error("Breakpoint not found at 0x{0:016x}")]
    BreakpointNotFound(u64),

    #[error("Watchpoint not found at 0x{0:016x}")]
    WatchpointNotFound(u64),

    #[error("Server not running")]
    ServerNotRunning,

    #[error("Client disconnected")]
    ClientDisconnected,

    #[error("Encoding error: {0}")]
    Encoding(String),
}

/// 调试目标 trait，由模拟器核心实现
pub trait DebugTarget {
    /// 读取单个寄存器
    fn read_register(&self, reg_num: u32) -> Result<u64, DebugError>;

    /// 写入单个寄存器
    fn write_register(&mut self, reg_num: u32, value: u64) -> Result<(), DebugError>;

    /// 读取所有寄存器（用于 'g' 命令）
    /// RISC-V 64位: x0-x31 (32个) + pc + f0-f31 (32个) + fcsr
    /// 返回的字节数组按小端序排列
    fn read_all_registers(&self) -> Result<Vec<u8>, DebugError> {
        let mut result = Vec::with_capacity((32 + 1 + 32 + 1) * 8);

        // x0-x31: 寄存器 0-31
        for i in 0..32 {
            let val = self.read_register(i)?;
            result.extend_from_slice(&val.to_le_bytes());
        }

        // pc: 寄存器 32
        let pc = self.read_register(32)?;
        result.extend_from_slice(&pc.to_le_bytes());

        // f0-f31: 寄存器 33-64
        for i in 33..65 {
            let val = self.read_register(i)?;
            result.extend_from_slice(&val.to_le_bytes());
        }

        // fcsr: 寄存器 65
        let fcsr = self.read_register(65)?;
        result.extend_from_slice(&fcsr.to_le_bytes());

        Ok(result)
    }

    /// 写入所有寄存器（用于 'G' 命令）
    fn write_all_registers(&mut self, data: &[u8]) -> Result<(), DebugError> {
        if data.len() < (32 + 1) * 8 {
            return Err(DebugError::InvalidPacket(
                "Insufficient data for register write".into(),
            ));
        }

        // x0-x31: 寄存器 0-31
        for i in 0..32 {
            let offset = i as usize * 8;
            let val = u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            self.write_register(i, val)?;
        }

        // pc: 寄存器 32
        let pc_offset = 32 * 8;
        let pc = u64::from_le_bytes([
            data[pc_offset],
            data[pc_offset + 1],
            data[pc_offset + 2],
            data[pc_offset + 3],
            data[pc_offset + 4],
            data[pc_offset + 5],
            data[pc_offset + 6],
            data[pc_offset + 7],
        ]);
        self.write_register(32, pc)?;

        // f0-f31: 寄存器 33-65 (如果有数据)
        if data.len() >= (65 + 1) * 8 {
            for i in 33..66 {
                let offset = i as usize * 8;
                let val = u64::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]);
                self.write_register(i, val)?;
            }
        }

        Ok(())
    }

    /// 读取内存
    fn read_memory(&self, addr: u64, len: usize) -> Result<Vec<u8>, DebugError>;

    /// 写入内存
    fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), DebugError>;

    /// 获取当前 PC
    fn get_pc(&self) -> u64;

    /// 设置 PC
    fn set_pc(&mut self, pc: u64);

    /// 继续执行
    fn continue_execution(&mut self) -> Result<StopReason, DebugError>;

    /// 单步执行
    fn step(&mut self) -> Result<StopReason, DebugError>;

    /// 停止执行
    fn stop(&mut self);

    /// 检查是否正在运行
    fn is_running(&self) -> bool;

    /// 获取停止原因
    fn get_stop_reason(&self) -> StopReason;

    /// 检查指定地址是否有断点命中
    fn check_breakpoint(&self, addr: u64) -> bool;

    /// 检查指定地址是否有观察点命中
    fn check_watchpoint(&self, addr: u64, access_type: WatchpointAccess) -> bool;
}

/// 停止原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// 正常运行中
    Running,
    /// 断点命中
    Breakpoint(u64),
    /// 观察点命中（读取）
    WatchpointRead(u64),
    /// 观察点命中（写入）
    WatchpointWrite(u64),
    /// 访问观察点（访问）
    WatchpointAccess(u64),
    /// 单步完成
    StepDone,
    /// 程序退出
    Exited(u8),
    /// 被信号中断
    Signal(u8),
    /// 未知原因
    Unknown,
}

impl StopReason {
    /// 转换为 GDB 信号编号
    pub fn to_signal(&self) -> u8 {
        match self {
            StopReason::Running => 0,
            StopReason::Breakpoint(_) => 5, // SIGTRAP
            StopReason::WatchpointRead(_) => 5,
            StopReason::WatchpointWrite(_) => 5,
            StopReason::WatchpointAccess(_) => 5,
            StopReason::StepDone => 5, // SIGTRAP
            StopReason::Exited(code) => *code,
            StopReason::Signal(sig) => *sig,
            StopReason::Unknown => 0,
        }
    }

    /// 转换为 GDB 停止响应字符串
    pub fn to_stop_reply(&self) -> String {
        match self {
            StopReason::Breakpoint(addr) => format!("T05breakpoint:;{:016x};", addr),
            StopReason::WatchpointRead(addr) => {
                format!("T05watch:;{:016x};", addr)
            }
            StopReason::WatchpointWrite(addr) => {
                format!("T05watch:;{:016x};", addr)
            }
            StopReason::WatchpointAccess(addr) => {
                format!("T05awatch:;{:016x};", addr)
            }
            StopReason::StepDone => "T05".to_string(),
            StopReason::Exited(code) => format!("W{:02x}", code),
            StopReason::Signal(sig) => format!("T{:02x}", sig),
            _ => "S00".to_string(),
        }
    }
}

/// 观察点访问类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchpointAccess {
    Read,
    Write,
    ReadWrite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_reason_signals() {
        assert_eq!(StopReason::Running.to_signal(), 0);
        assert_eq!(StopReason::Breakpoint(0x1000).to_signal(), 5);
        assert_eq!(StopReason::StepDone.to_signal(), 5);
        assert_eq!(StopReason::Exited(42).to_signal(), 42);
    }

    #[test]
    fn test_stop_reason_to_reply() {
        assert!(StopReason::Breakpoint(0x1000)
            .to_stop_reply()
            .contains("T05"));
        assert!(StopReason::Breakpoint(0x1000)
            .to_stop_reply()
            .contains("0000000000001000"));
        assert_eq!(StopReason::StepDone.to_stop_reply(), "T05");
        assert_eq!(StopReason::Exited(0).to_stop_reply(), "W00");
    }
}
