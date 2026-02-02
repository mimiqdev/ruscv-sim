//! TLM2.0 Generic Payload
//!
//! 实现 TLM2.0 通用事务载荷，支持地址、数据、字节使能等属性

use super::{TlmCommand, TlmResponseStatus};

/// 数据扩展模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataExtensionMode {
    /// 无扩展
    None,
    /// 原子操作扩展
    Atomic,
    /// 缓存一致性扩展
    CacheCoherent,
    /// 安全扩展
    Secure,
}

/// TLM2.0 通用事务载荷
///
/// 这是 TLM2.0 标准的核心数据结构，用于在不同组件之间传输事务信息
#[derive(Debug, Clone)]
pub struct TlmGenericPayload {
    /// 命令类型（读/写）
    command: TlmCommand,
    /// 访问地址
    address: u64,
    /// 数据缓冲区
    data: Vec<u8>,
    /// 字节使能（可选）
    byte_enable: Option<Vec<u8>>,
    /// 字节使能长度
    byte_enable_length: usize,
    /// 数据长度
    data_length: usize,
    /// 流式传输宽度（0表示非流式）
    streaming_width: usize,
    /// 是否需要 DMI（直接内存接口）
    dmi_allowed: bool,
    /// 响应状态
    response_status: TlmResponseStatus,
    /// 扩展模式
    extension_mode: DataExtensionMode,
}

impl TlmGenericPayload {
    /// 创建新的事务载荷
    ///
    /// # 参数
    /// - `command`: 命令类型（读/写）
    /// - `address`: 访问地址
    /// - `data_length`: 数据长度（字节）
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::{TlmGenericPayload, TlmCommand};
    ///
    /// let payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
    /// assert_eq!(payload.command(), TlmCommand::Read);
    /// assert_eq!(payload.address(), 0x1000);
    /// assert_eq!(payload.data_length(), 4);
    /// ```
    pub fn new(command: TlmCommand, address: u64, data_length: usize) -> Self {
        Self {
            command,
            address,
            data: vec![0; data_length],
            byte_enable: None,
            byte_enable_length: 0,
            data_length,
            streaming_width: 0,
            dmi_allowed: false,
            response_status: TlmResponseStatus::Ok,
            extension_mode: DataExtensionMode::None,
        }
    }

    /// 从现有数据创建事务载荷
    ///
    /// # 参数
    /// - `command`: 命令类型
    /// - `address`: 访问地址
    /// - `data`: 数据缓冲区
    pub fn with_data(command: TlmCommand, address: u64, data: Vec<u8>) -> Self {
        let data_length = data.len();
        Self {
            command,
            address,
            data,
            byte_enable: None,
            byte_enable_length: 0,
            data_length,
            streaming_width: 0,
            dmi_allowed: false,
            response_status: TlmResponseStatus::Ok,
            extension_mode: DataExtensionMode::None,
        }
    }

    /// 获取命令类型
    pub fn command(&self) -> TlmCommand {
        self.command
    }

    /// 设置命令类型
    pub fn set_command(&mut self, command: TlmCommand) {
        self.command = command;
    }

    /// 获取地址
    pub fn address(&self) -> u64 {
        self.address
    }

    /// 设置地址
    pub fn set_address(&mut self, address: u64) {
        self.address = address;
    }

    /// 获取数据长度
    pub fn data_length(&self) -> usize {
        self.data_length
    }

    /// 设置数据长度
    ///
    /// 注意：这会重新分配数据缓冲区
    pub fn set_data_length(&mut self, length: usize) {
        self.data_length = length;
        self.data.resize(length, 0);
    }

    /// 获取数据缓冲区的引用
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// 获取数据缓冲区的可变引用
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// 设置数据
    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data_length = data.len();
        self.data = data;
    }

    /// 获取字节使能
    pub fn byte_enable(&self) -> Option<&[u8]> {
        self.byte_enable.as_deref()
    }

    /// 设置字节使能
    ///
    /// 字节使能用于控制哪些字节实际参与传输
    pub fn set_byte_enable(&mut self, byte_enable: Option<Vec<u8>>) {
        self.byte_enable_length = byte_enable.as_ref().map(|v| v.len()).unwrap_or(0);
        self.byte_enable = byte_enable;
    }

    /// 获取字节使能长度
    pub fn byte_enable_length(&self) -> usize {
        self.byte_enable_length
    }

    /// 检查是否为流式传输
    pub fn is_streaming(&self) -> bool {
        self.streaming_width > 0
    }

    /// 获取流式传输宽度
    pub fn streaming_width(&self) -> usize {
        self.streaming_width
    }

    /// 设置流式传输宽度
    ///
    /// 设置为0表示非流式传输
    pub fn set_streaming_width(&mut self, width: usize) {
        self.streaming_width = width;
    }

    /// 检查是否允许 DMI
    pub fn is_dmi_allowed(&self) -> bool {
        self.dmi_allowed
    }

    /// 设置 DMI 允许标志
    pub fn set_dmi_allowed(&mut self, allowed: bool) {
        self.dmi_allowed = allowed;
    }

    /// 获取响应状态
    pub fn response_status(&self) -> TlmResponseStatus {
        self.response_status
    }

    /// 设置响应状态（向后兼容的方法）
    pub fn set_response_status(&mut self, status: TlmResponseStatus) {
        self.response_status = status;
    }

    /// 检查响应是否成功
    pub fn is_response_ok(&self) -> bool {
        self.response_status.is_ok()
    }

    /// 检查响应是否错误
    pub fn is_response_error(&self) -> bool {
        self.response_status.is_error()
    }

    /// 获取扩展模式
    pub fn extension_mode(&self) -> DataExtensionMode {
        self.extension_mode
    }

    /// 设置扩展模式
    pub fn set_extension_mode(&mut self, mode: DataExtensionMode) {
        self.extension_mode = mode;
    }

    /// 重置载荷到初始状态
    pub fn reset(&mut self) {
        self.command = TlmCommand::Read;
        self.address = 0;
        self.data.fill(0);
        self.byte_enable = None;
        self.byte_enable_length = 0;
        self.streaming_width = 0;
        self.dmi_allowed = false;
        self.response_status = TlmResponseStatus::Ok;
        self.extension_mode = DataExtensionMode::None;
    }

    /// 深拷贝载荷
    pub fn deep_copy(&self) -> Self {
        Self {
            command: self.command,
            address: self.address,
            data: self.data.clone(),
            byte_enable: self.byte_enable.clone(),
            byte_enable_length: self.byte_enable_length,
            data_length: self.data_length,
            streaming_width: self.streaming_width,
            dmi_allowed: self.dmi_allowed,
            response_status: self.response_status,
            extension_mode: self.extension_mode,
        }
    }

    /// 更新数据指针引用（用于获取写操作后的数据）
    pub fn get_data_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// 获取可变数据指针
    pub fn get_data_ptr_mut(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    /// 获取数据指针长度
    pub fn get_data_length(&self) -> usize {
        self.data_length
    }

    /// 设置数据指针（更新数据内容）
    pub fn set_data_ptr(&mut self, data: &[u8]) {
        self.data = data.to_vec();
        self.data_length = data.len();
    }

    /// 更新数据长度
    pub fn update_data_length(&mut self, length: usize) {
        self.data_length = length;
        self.data.resize(length, 0);
    }
}

impl Default for TlmGenericPayload {
    fn default() -> Self {
        Self::new(TlmCommand::Read, 0, 0)
    }
}

/// 用于构建 TlmGenericPayload 的 Builder 模式
#[derive(Debug)]
pub struct TlmPayloadBuilder {
    command: TlmCommand,
    address: u64,
    data: Vec<u8>,
    byte_enable: Option<Vec<u8>>,
    streaming_width: usize,
    dmi_allowed: bool,
    extension_mode: DataExtensionMode,
}

impl TlmPayloadBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            command: TlmCommand::Read,
            address: 0,
            data: Vec::new(),
            byte_enable: None,
            streaming_width: 0,
            dmi_allowed: false,
            extension_mode: DataExtensionMode::None,
        }
    }

    /// 设置命令
    pub fn command(mut self, command: TlmCommand) -> Self {
        self.command = command;
        self
    }

    /// 设置地址
    pub fn address(mut self, address: u64) -> Self {
        self.address = address;
        self
    }

    /// 设置数据
    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    /// 设置字节使能
    pub fn byte_enable(mut self, byte_enable: Vec<u8>) -> Self {
        self.byte_enable = Some(byte_enable);
        self
    }

    /// 设置流式传输宽度
    pub fn streaming_width(mut self, width: usize) -> Self {
        self.streaming_width = width;
        self
    }

    /// 设置 DMI 允许
    pub fn dmi_allowed(mut self, allowed: bool) -> Self {
        self.dmi_allowed = allowed;
        self
    }

    /// 设置扩展模式
    pub fn extension_mode(mut self, mode: DataExtensionMode) -> Self {
        self.extension_mode = mode;
        self
    }

    /// 构建载荷
    pub fn build(self) -> TlmGenericPayload {
        let data_length = self.data.len();
        TlmGenericPayload {
            command: self.command,
            address: self.address,
            data: self.data,
            byte_enable: self.byte_enable,
            byte_enable_length: 0,
            data_length,
            streaming_width: self.streaming_width,
            dmi_allowed: self.dmi_allowed,
            response_status: TlmResponseStatus::Ok,
            extension_mode: self.extension_mode,
        }
    }
}

impl Default for TlmPayloadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_new() {
        let payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        assert_eq!(payload.command(), TlmCommand::Read);
        assert_eq!(payload.address(), 0x1000);
        assert_eq!(payload.data_length(), 4);
        assert!(payload.is_response_ok());
    }

    #[test]
    fn test_payload_with_data() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let payload = TlmGenericPayload::with_data(TlmCommand::Write, 0x2000, data.clone());
        assert_eq!(payload.data(), &data[..]);
    }

    #[test]
    fn test_payload_setters() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

        payload.set_command(TlmCommand::Write);
        assert_eq!(payload.command(), TlmCommand::Write);

        payload.set_address(0x2000);
        assert_eq!(payload.address(), 0x2000);

        payload.set_data_length(8);
        assert_eq!(payload.data_length(), 8);
    }

    #[test]
    fn test_payload_byte_enable() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

        let byte_enable = vec![0xFF, 0x00, 0xFF, 0x00];
        payload.set_byte_enable(Some(byte_enable.clone()));

        assert_eq!(payload.byte_enable(), Some(&byte_enable[..]));
        assert_eq!(payload.byte_enable_length(), 4);
    }

    #[test]
    fn test_payload_streaming() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 16);

        assert!(!payload.is_streaming());

        payload.set_streaming_width(4);
        assert!(payload.is_streaming());
        assert_eq!(payload.streaming_width(), 4);
    }

    #[test]
    fn test_payload_response_status() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

        assert!(payload.is_response_ok());
        assert!(!payload.is_response_error());

        payload.set_response_status(TlmResponseStatus::AddressError);
        assert!(!payload.is_response_ok());
        assert!(payload.is_response_error());
    }

    #[test]
    fn test_payload_reset() {
        let mut payload =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);
        payload.set_response_status(TlmResponseStatus::Ok);

        payload.reset();

        assert_eq!(payload.command(), TlmCommand::Read);
        assert_eq!(payload.address(), 0);
        assert!(payload.is_response_ok());
    }

    #[test]
    fn test_payload_deep_copy() {
        let payload =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);

        let copy = payload.deep_copy();
        assert_eq!(payload.data(), copy.data());
    }

    #[test]
    fn test_payload_builder() {
        let payload = TlmPayloadBuilder::new()
            .command(TlmCommand::Write)
            .address(0x3000)
            .data(vec![0xAA, 0xBB, 0xCC, 0xDD])
            .dmi_allowed(true)
            .build();

        assert_eq!(payload.command(), TlmCommand::Write);
        assert_eq!(payload.address(), 0x3000);
        assert_eq!(payload.data(), &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert!(payload.is_dmi_allowed());
    }
}
