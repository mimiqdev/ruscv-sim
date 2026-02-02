//! TLM2.0 响应状态定义
//!
//! 定义 TLM 事务的响应状态码，用于表示事务执行结果

/// TLM2.0 响应状态
///
/// 表示 TLM 事务的执行结果状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlmResponseStatus {
    /// 事务成功完成
    Ok,
    /// 地址错误（地址无效或不存在）
    AddressError,
    /// 命令错误（不支持的操作）
    CommandError,
    /// 突发传输错误
    BurstError,
    /// 数据错误（校验失败等）
    DataError,
    /// 无效地址（越界访问）
    InvalidAddress,
    /// 需要等待请求
    WaitRequest,
    /// 需要等待响应
    WaitResponse,
    /// 需要释放资源
    ReleaseRequired,
    /// 总线错误
    BusError,
    /// 超时错误
    Timeout,
    /// 权限错误
    PermissionError,
}

impl TlmResponseStatus {
    /// 检查状态是否为成功
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmResponseStatus;
    ///
    /// assert!(TlmResponseStatus::Ok.is_ok());
    /// assert!(!TlmResponseStatus::AddressError.is_ok());
    /// ```
    pub fn is_ok(&self) -> bool {
        matches!(self, TlmResponseStatus::Ok)
    }

    /// 检查状态是否为错误
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmResponseStatus;
    ///
    /// assert!(TlmResponseStatus::AddressError.is_error());
    /// assert!(!TlmResponseStatus::Ok.is_error());
    /// ```
    pub fn is_error(&self) -> bool {
        !self.is_ok()
    }

    /// 获取状态的错误分类
    ///
    /// 返回状态属于哪种错误类型：
    /// - Address: 地址相关错误
    /// - Command: 命令相关错误  
    /// - Data: 数据相关错误
    /// - Bus: 总线相关错误
    /// - None: 非错误状态
    pub fn error_category(&self) -> Option<ErrorCategory> {
        match self {
            TlmResponseStatus::Ok => None,
            TlmResponseStatus::AddressError | TlmResponseStatus::InvalidAddress => {
                Some(ErrorCategory::Address)
            }
            TlmResponseStatus::CommandError => Some(ErrorCategory::Command),
            TlmResponseStatus::BurstError | TlmResponseStatus::DataError => {
                Some(ErrorCategory::Data)
            }
            TlmResponseStatus::WaitRequest | TlmResponseStatus::WaitResponse => {
                Some(ErrorCategory::Timing)
            }
            TlmResponseStatus::ReleaseRequired => Some(ErrorCategory::Resource),
            TlmResponseStatus::BusError => Some(ErrorCategory::Bus),
            TlmResponseStatus::Timeout => Some(ErrorCategory::Timing),
            TlmResponseStatus::PermissionError => Some(ErrorCategory::Permission),
        }
    }

    /// 获取状态描述文本
    pub fn description(&self) -> &'static str {
        match self {
            TlmResponseStatus::Ok => "Transaction completed successfully",
            TlmResponseStatus::AddressError => "Address error occurred",
            TlmResponseStatus::CommandError => "Invalid command or operation",
            TlmResponseStatus::BurstError => "Burst transfer error",
            TlmResponseStatus::DataError => "Data error or corruption",
            TlmResponseStatus::InvalidAddress => "Invalid or out-of-bounds address",
            TlmResponseStatus::WaitRequest => "Waiting for request phase",
            TlmResponseStatus::WaitResponse => "Waiting for response phase",
            TlmResponseStatus::ReleaseRequired => "Resource release required",
            TlmResponseStatus::BusError => "Bus error occurred",
            TlmResponseStatus::Timeout => "Transaction timeout",
            TlmResponseStatus::PermissionError => "Permission denied",
        }
    }
}

impl std::fmt::Display for TlmResponseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlmResponseStatus::Ok => write!(f, "OK"),
            TlmResponseStatus::AddressError => write!(f, "ADDRESS_ERROR"),
            TlmResponseStatus::CommandError => write!(f, "COMMAND_ERROR"),
            TlmResponseStatus::BurstError => write!(f, "BURST_ERROR"),
            TlmResponseStatus::DataError => write!(f, "DATA_ERROR"),
            TlmResponseStatus::InvalidAddress => write!(f, "INVALID_ADDRESS"),
            TlmResponseStatus::WaitRequest => write!(f, "WAIT_REQUEST"),
            TlmResponseStatus::WaitResponse => write!(f, "WAIT_RESPONSE"),
            TlmResponseStatus::ReleaseRequired => write!(f, "RELEASE_REQUIRED"),
            TlmResponseStatus::BusError => write!(f, "BUS_ERROR"),
            TlmResponseStatus::Timeout => write!(f, "TIMEOUT"),
            TlmResponseStatus::PermissionError => write!(f, "PERMISSION_ERROR"),
        }
    }
}

/// 错误分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// 地址相关错误
    Address,
    /// 命令相关错误
    Command,
    /// 数据相关错误
    Data,
    /// 总线相关错误
    Bus,
    /// 时序相关错误
    Timing,
    /// 资源相关错误
    Resource,
    /// 权限相关错误
    Permission,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Address => write!(f, "Address"),
            ErrorCategory::Command => write!(f, "Command"),
            ErrorCategory::Data => write!(f, "Data"),
            ErrorCategory::Bus => write!(f, "Bus"),
            ErrorCategory::Timing => write!(f, "Timing"),
            ErrorCategory::Resource => write!(f, "Resource"),
            ErrorCategory::Permission => write!(f, "Permission"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_is_ok() {
        assert!(TlmResponseStatus::Ok.is_ok());
        assert!(!TlmResponseStatus::AddressError.is_ok());
        assert!(!TlmResponseStatus::Timeout.is_ok());
    }

    #[test]
    fn test_status_is_error() {
        assert!(!TlmResponseStatus::Ok.is_error());
        assert!(TlmResponseStatus::AddressError.is_error());
        assert!(TlmResponseStatus::Timeout.is_error());
    }

    #[test]
    fn test_status_error_category() {
        assert_eq!(TlmResponseStatus::Ok.error_category(), None);
        assert_eq!(
            TlmResponseStatus::AddressError.error_category(),
            Some(ErrorCategory::Address)
        );
        assert_eq!(
            TlmResponseStatus::InvalidAddress.error_category(),
            Some(ErrorCategory::Address)
        );
        assert_eq!(
            TlmResponseStatus::CommandError.error_category(),
            Some(ErrorCategory::Command)
        );
        assert_eq!(
            TlmResponseStatus::Timeout.error_category(),
            Some(ErrorCategory::Timing)
        );
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", TlmResponseStatus::Ok), "OK");
        assert_eq!(
            format!("{}", TlmResponseStatus::AddressError),
            "ADDRESS_ERROR"
        );
        assert_eq!(format!("{}", TlmResponseStatus::Timeout), "TIMEOUT");
    }

    #[test]
    fn test_status_description() {
        assert!(TlmResponseStatus::Ok.description().contains("success"));
        assert!(TlmResponseStatus::Timeout.description().contains("timeout"));
    }
}
