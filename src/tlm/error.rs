//! TLM2.0 错误定义
//!
//! 定义 TLM 操作中可能发生的错误

use thiserror::Error;

/// TLM 错误类型
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum TlmError {
    /// 无效地址
    #[error("Invalid address: 0x{0:08x}")]
    InvalidAddress(u32),
    
    /// 无效地址（64位）
    #[error("Invalid address: 0x{0:016x}")]
    InvalidAddress64(u64),
    
    /// 无效事务长度
    #[error("Invalid transaction length: {0}")]
    InvalidLength(usize),
    
    /// 事务超时
    #[error("Transaction timeout")]
    Timeout,
    
    /// 总线繁忙
    #[error("Bus busy")]
    Busy,
    
    /// 未实现
    #[error("Not implemented")]
    NotImplemented,
    
    /// 总线错误
    #[error("Bus error: {0}")]
    BusError(String),
    
    /// 访问权限错误
    #[error("Access denied")]
    AccessDenied,
    
    /// 未连接到目标
    #[error("Not connected to target")]
    NotConnected,
    
    /// 协议错误
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    /// 仲裁失败
    #[error("Arbitration failed")]
    ArbitrationFailed,
    
    /// 同步错误
    #[error("Synchronization error")]
    SyncError,
}

/// TLM 同步枚举
/// 
/// 表示非阻塞传输的同步状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlmSyncEnum {
    /// 接受事务，操作完成
    Accept,
    /// 需要等待，事务进行中
    Wait,
    /// 需要释放资源
    Release,
    /// 更新请求（相位或时间已更新）
    Update,
}

impl TlmSyncEnum {
    /// 检查是否接受
    pub fn is_accept(&self) -> bool {
        matches!(self, TlmSyncEnum::Accept)
    }

    /// 检查是否需要等待
    pub fn is_wait(&self) -> bool {
        matches!(self, TlmSyncEnum::Wait)
    }

    /// 检查是否需要释放
    pub fn is_release(&self) -> bool {
        matches!(self, TlmSyncEnum::Release)
    }

    /// 检查是否更新
    pub fn is_update(&self) -> bool {
        matches!(self, TlmSyncEnum::Update)
    }
}

impl std::fmt::Display for TlmSyncEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlmSyncEnum::Accept => write!(f, "ACCEPT"),
            TlmSyncEnum::Wait => write!(f, "WAIT"),
            TlmSyncEnum::Release => write!(f, "RELEASE"),
            TlmSyncEnum::Update => write!(f, "UPDATE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlm_error_display() {
        let err = TlmError::InvalidAddress(0x1000);
        assert!(err.to_string().contains("0x00001000"));

        let err = TlmError::Timeout;
        assert_eq!(err.to_string(), "Transaction timeout");
    }

    #[test]
    fn test_tlm_sync_enum() {
        assert!(TlmSyncEnum::Accept.is_accept());
        assert!(!TlmSyncEnum::Accept.is_wait());
        
        assert!(TlmSyncEnum::Wait.is_wait());
        assert!(TlmSyncEnum::Release.is_release());
        assert!(TlmSyncEnum::Update.is_update());
    }

    #[test]
    fn test_tlm_sync_enum_display() {
        assert_eq!(format!("{}", TlmSyncEnum::Accept), "ACCEPT");
        assert_eq!(format!("{}", TlmSyncEnum::Wait), "WAIT");
        assert_eq!(format!("{}", TlmSyncEnum::Release), "RELEASE");
    }
}
