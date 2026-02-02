//! TLM2.0 相位定义
//!
//! 定义 TLM2.0 传输的各个阶段，用于非阻塞传输协议

/// TLM2.0 传输相位
///
/// 遵循 SystemC TLM2.0 标准定义的四阶段协议：
/// - BEGIN_REQ: 请求开始阶段
/// - END_REQ: 请求结束阶段  
/// - BEGIN_RESP: 响应开始阶段
/// - END_RESP: 响应结束阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlmPhase {
    /// 请求开始阶段
    ///
    /// 发起者发送请求到目标，标志着事务处理的开始
    BeginReq,
    /// 请求结束阶段
    ///
    /// 目标接收请求完成，可以开始处理
    EndReq,
    /// 响应开始阶段
    ///
    /// 目标开始发送响应数据
    BeginResp,
    /// 响应结束阶段
    ///
    /// 发起者接收响应完成，事务结束
    EndResp,
}

impl TlmPhase {
    /// 获取相位的下一个阶段
    ///
    /// 按照 TLM2.0 协议的标准顺序推进：
    /// BeginReq -> EndReq -> BeginResp -> EndResp
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert_eq!(TlmPhase::BeginReq.next(), Some(TlmPhase::EndReq));
    /// assert_eq!(TlmPhase::EndResp.next(), None);
    /// ```
    pub fn next(&self) -> Option<Self> {
        match self {
            TlmPhase::BeginReq => Some(TlmPhase::EndReq),
            TlmPhase::EndReq => Some(TlmPhase::BeginResp),
            TlmPhase::BeginResp => Some(TlmPhase::EndResp),
            TlmPhase::EndResp => None,
        }
    }

    /// 获取相位的前一个阶段
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert_eq!(TlmPhase::EndReq.prev(), Some(TlmPhase::BeginReq));
    /// assert_eq!(TlmPhase::BeginReq.prev(), None);
    /// ```
    pub fn prev(&self) -> Option<Self> {
        match self {
            TlmPhase::BeginReq => None,
            TlmPhase::EndReq => Some(TlmPhase::BeginReq),
            TlmPhase::BeginResp => Some(TlmPhase::EndReq),
            TlmPhase::EndResp => Some(TlmPhase::BeginResp),
        }
    }

    /// 检查是否为请求阶段
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert!(TlmPhase::BeginReq.is_request());
    /// assert!(TlmPhase::EndReq.is_request());
    /// assert!(!TlmPhase::BeginResp.is_request());
    /// ```
    pub fn is_request(&self) -> bool {
        matches!(self, TlmPhase::BeginReq | TlmPhase::EndReq)
    }

    /// 检查是否为响应阶段
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert!(TlmPhase::BeginResp.is_response());
    /// assert!(TlmPhase::EndResp.is_response());
    /// assert!(!TlmPhase::BeginReq.is_response());
    /// ```
    pub fn is_response(&self) -> bool {
        matches!(self, TlmPhase::BeginResp | TlmPhase::EndResp)
    }

    /// 检查是否为开始阶段
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert!(TlmPhase::BeginReq.is_begin());
    /// assert!(TlmPhase::BeginResp.is_begin());
    /// assert!(!TlmPhase::EndReq.is_begin());
    /// ```
    pub fn is_begin(&self) -> bool {
        matches!(self, TlmPhase::BeginReq | TlmPhase::BeginResp)
    }

    /// 检查是否为结束阶段
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::TlmPhase;
    ///
    /// assert!(TlmPhase::EndReq.is_end());
    /// assert!(TlmPhase::EndResp.is_end());
    /// assert!(!TlmPhase::BeginReq.is_end());
    /// ```
    pub fn is_end(&self) -> bool {
        matches!(self, TlmPhase::EndReq | TlmPhase::EndResp)
    }
}

impl std::fmt::Display for TlmPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlmPhase::BeginReq => write!(f, "BEGIN_REQ"),
            TlmPhase::EndReq => write!(f, "END_REQ"),
            TlmPhase::BeginResp => write!(f, "BEGIN_RESP"),
            TlmPhase::EndResp => write!(f, "END_RESP"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_next() {
        assert_eq!(TlmPhase::BeginReq.next(), Some(TlmPhase::EndReq));
        assert_eq!(TlmPhase::EndReq.next(), Some(TlmPhase::BeginResp));
        assert_eq!(TlmPhase::BeginResp.next(), Some(TlmPhase::EndResp));
        assert_eq!(TlmPhase::EndResp.next(), None);
    }

    #[test]
    fn test_phase_prev() {
        assert_eq!(TlmPhase::EndResp.prev(), Some(TlmPhase::BeginResp));
        assert_eq!(TlmPhase::BeginResp.prev(), Some(TlmPhase::EndReq));
        assert_eq!(TlmPhase::EndReq.prev(), Some(TlmPhase::BeginReq));
        assert_eq!(TlmPhase::BeginReq.prev(), None);
    }

    #[test]
    fn test_phase_is_request() {
        assert!(TlmPhase::BeginReq.is_request());
        assert!(TlmPhase::EndReq.is_request());
        assert!(!TlmPhase::BeginResp.is_request());
        assert!(!TlmPhase::EndResp.is_request());
    }

    #[test]
    fn test_phase_is_response() {
        assert!(!TlmPhase::BeginReq.is_response());
        assert!(!TlmPhase::EndReq.is_response());
        assert!(TlmPhase::BeginResp.is_response());
        assert!(TlmPhase::EndResp.is_response());
    }

    #[test]
    fn test_phase_is_begin() {
        assert!(TlmPhase::BeginReq.is_begin());
        assert!(!TlmPhase::EndReq.is_begin());
        assert!(TlmPhase::BeginResp.is_begin());
        assert!(!TlmPhase::EndResp.is_begin());
    }

    #[test]
    fn test_phase_is_end() {
        assert!(!TlmPhase::BeginReq.is_end());
        assert!(TlmPhase::EndReq.is_end());
        assert!(!TlmPhase::BeginResp.is_end());
        assert!(TlmPhase::EndResp.is_end());
    }

    #[test]
    fn test_phase_display() {
        assert_eq!(format!("{}", TlmPhase::BeginReq), "BEGIN_REQ");
        assert_eq!(format!("{}", TlmPhase::EndReq), "END_REQ");
        assert_eq!(format!("{}", TlmPhase::BeginResp), "BEGIN_RESP");
        assert_eq!(format!("{}", TlmPhase::EndResp), "END_RESP");
    }
}
