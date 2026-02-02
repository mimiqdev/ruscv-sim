//! TLM2.0 命令类型定义
//!
//! 定义 TLM 事务的命令类型（读/写）

/// TLM2.0 命令类型
/// 
/// 表示 TLM 事务的操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlmCommand {
    /// 读操作
    Read,
    /// 写操作
    Write,
}

impl TlmCommand {
    /// 检查是否为读操作
    /// 
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmCommand;
    /// 
    /// assert!(TlmCommand::Read.is_read());
    /// assert!(!TlmCommand::Write.is_read());
    /// ```
    pub fn is_read(&self) -> bool {
        matches!(self, TlmCommand::Read)
    }

    /// 检查是否为写操作
    /// 
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmCommand;
    /// 
    /// assert!(TlmCommand::Write.is_write());
    /// assert!(!TlmCommand::Read.is_write());
    /// ```
    pub fn is_write(&self) -> bool {
        matches!(self, TlmCommand::Write)
    }

    /// 获取相反的操作类型
    /// 
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmCommand;
    /// 
    /// assert_eq!(TlmCommand::Read.opposite(), TlmCommand::Write);
    /// assert_eq!(TlmCommand::Write.opposite(), TlmCommand::Read);
    /// ```
    pub fn opposite(&self) -> Self {
        match self {
            TlmCommand::Read => TlmCommand::Write,
            TlmCommand::Write => TlmCommand::Read,
        }
    }
}

impl std::fmt::Display for TlmCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlmCommand::Read => write!(f, "READ"),
            TlmCommand::Write => write!(f, "WRITE"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_is_read() {
        assert!(TlmCommand::Read.is_read());
        assert!(!TlmCommand::Write.is_read());
    }

    #[test]
    fn test_command_is_write() {
        assert!(!TlmCommand::Read.is_write());
        assert!(TlmCommand::Write.is_write());
    }

    #[test]
    fn test_command_opposite() {
        assert_eq!(TlmCommand::Read.opposite(), TlmCommand::Write);
        assert_eq!(TlmCommand::Write.opposite(), TlmCommand::Read);
    }

    #[test]
    fn test_command_display() {
        assert_eq!(format!("{}", TlmCommand::Read), "READ");
        assert_eq!(format!("{}", TlmCommand::Write), "WRITE");
    }
}
