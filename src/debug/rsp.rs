//! GDB Remote Serial Protocol (RSP) 协议实现
//!
//! RSP 协议使用基于文本的数据包格式：$packet-data#checksum
//! 校验和是 packet-data 中所有字节的和的低 8 位

use super::DebugError;

/// GDB 数据包
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GdbPacket {
    /// 数据内容（不包括 $ 和 #xx）
    pub data: String,
}

impl GdbPacket {
    /// 创建新的数据包
    pub fn new(data: impl Into<String>) -> Self {
        Self { data: data.into() }
    }

    /// 从原始数据解析数据包
    pub fn parse(input: &str) -> Result<(Self, usize), DebugError> {
        let bytes = input.as_bytes();

        // 查找 '$'
        let mut pos = 0;
        while pos < bytes.len() && bytes[pos] != b'$' {
            pos += 1;
        }

        if pos >= bytes.len() {
            return Err(DebugError::InvalidPacket(
                "Packet start marker '$' not found".into(),
            ));
        }

        let start = pos;
        pos += 1; // 跳过 '$'

        // 查找 '#'（校验和开始）
        let data_start = pos;
        while pos < bytes.len() && bytes[pos] != b'#' {
            pos += 1;
        }

        if pos + 2 >= bytes.len() {
            return Err(DebugError::InvalidPacket(
                "Packet checksum not found".into(),
            ));
        }

        let data_end = pos;
        let data = &input[data_start..data_end];

        // 解析校验和
        pos += 1; // 跳过 '#'
        let checksum_str = &input[pos..pos + 2];
        let expected_checksum = u8::from_str_radix(checksum_str, 16)
            .map_err(|_| DebugError::InvalidPacket("Invalid checksum format".into()))?;

        // 计算实际校验和
        let actual_checksum = calculate_checksum(data);

        if expected_checksum != actual_checksum {
            return Err(DebugError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        Ok((Self::new(data), pos + 2 - start))
    }

    /// 将数据包编码为传输格式
    pub fn encode(&self) -> String {
        let checksum = calculate_checksum(&self.data);
        format!("${}#{:02x}", self.data, checksum)
    }

    /// 创建确认响应
    pub fn ack() -> &'static str {
        "+"
    }

    /// 创建否定响应
    pub fn nack() -> &'static str {
        "-"
    }

    /// 创建空成功响应
    pub fn ok() -> Self {
        Self::new("OK")
    }

    /// 创建错误响应
    pub fn error(code: u8) -> Self {
        Self::new(format!("E{:02x}", code))
    }

    /// 创建空响应
    pub fn empty() -> Self {
        Self::new("")
    }

    /// 将二进制数据编码为十六进制字符串
    pub fn encode_hex(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// 将十六进制字符串解码为二进制数据
    pub fn decode_hex(hex: &str) -> Result<Vec<u8>, DebugError> {
        if hex.len() % 2 != 0 {
            return Err(DebugError::Encoding(
                "Hex string length must be even".into(),
            ));
        }

        let mut result = Vec::with_capacity(hex.len() / 2);
        for i in (0..hex.len()).step_by(2) {
            let byte = u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| DebugError::Encoding(format!("Invalid hex at position {}", i)))?;
            result.push(byte);
        }
        Ok(result)
    }

    /// 转义二进制数据（用于 X 命令）
    pub fn escape_binary(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        for &b in data {
            if b == b'#' || b == b'$' || b == b'}' || b == b'*' {
                result.push(b'}');
                result.push(b ^ 0x20);
            } else {
                result.push(b);
            }
        }
        result
    }

    /// 解转义二进制数据
    pub fn unescape_binary(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut i = 0;
        while i < data.len() {
            if data[i] == b'}' && i + 1 < data.len() {
                result.push(data[i + 1] ^ 0x20);
                i += 2;
            } else {
                result.push(data[i]);
                i += 1;
            }
        }
        result
    }
}

/// 计算校验和
fn calculate_checksum(data: &str) -> u8 {
    data.bytes().fold(0u8, |sum, b| sum.wrapping_add(b))
}

/// RSP 协议处理器
pub struct RspProtocol {
    /// 接收缓冲区
    buffer: String,
    /// 是否启用确认
    ack_enabled: bool,
}

impl Default for RspProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl RspProtocol {
    /// 创建新的协议处理器
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            ack_enabled: true,
        }
    }

    /// 设置是否启用确认
    pub fn set_ack_enabled(&mut self, enabled: bool) {
        self.ack_enabled = enabled;
    }

    /// 将数据添加到接收缓冲区
    pub fn feed(&mut self, data: &str) {
        self.buffer.push_str(data);
    }

    /// 尝试从缓冲区解析一个数据包
    pub fn try_parse_packet(&mut self) -> Option<Result<GdbPacket, DebugError>> {
        // 查找包开始标记 '$'
        if let Some(start) = self.buffer.find('$') {
            // 查找校验和标记 '#'
            if let Some(hash_pos) = self.buffer[start..].find('#') {
                let hash_abs_pos = start + hash_pos;
                // 确保有足够的字节用于校验和
                if hash_abs_pos + 2 < self.buffer.len() {
                    // 尝试解析
                    match GdbPacket::parse(&self.buffer[start..]) {
                        Ok((packet, consumed)) => {
                            // 从缓冲区移除已解析的数据
                            self.buffer.drain(..start + consumed);
                            return Some(Ok(packet));
                        }
                        Err(e) => {
                            // 如果校验和不匹配，可能需要重传
                            // 这里我们清除缓冲区并重试
                            if matches!(e, DebugError::ChecksumMismatch { .. }) {
                                self.buffer.drain(..start + hash_abs_pos + 3);
                                return Some(Err(e));
                            }
                            // 其他错误，尝试跳过这个数据
                            self.buffer.drain(..start + 1);
                            return self.try_parse_packet();
                        }
                    }
                }
            }
        }
        None
    }

    /// 创建确认响应
    pub fn ack() -> &'static str {
        "+"
    }

    /// 创建否定响应
    pub fn nack() -> &'static str {
        "-"
    }

    /// 创建停止响应
    pub fn stop_reply(reason: &super::StopReason) -> GdbPacket {
        GdbPacket::new(reason.to_stop_reply())
    }

    /// 编码内存读取响应
    pub fn encode_memory_read(data: &[u8]) -> GdbPacket {
        GdbPacket::new(GdbPacket::encode_hex(data))
    }

    /// 编码寄存器读取响应
    pub fn encode_registers(data: &[u8]) -> GdbPacket {
        GdbPacket::new(GdbPacket::encode_hex(data))
    }

    /// 解析设置断点命令 (Z0/Z1/Z2/Z3/Z4)
    /// 格式: Ztype,addr,kind
    pub fn parse_breakpoint_set(cmd: &str) -> Result<(u8, u64, u64), DebugError> {
        // 跳过 'Z'
        let parts: Vec<&str> = cmd[1..].split(',').collect();
        if parts.len() < 3 {
            return Err(DebugError::InvalidPacket(
                "Breakpoint command needs 3 parameters".into(),
            ));
        }

        let bp_type = parts[0]
            .parse::<u8>()
            .map_err(|_| DebugError::InvalidPacket("Invalid breakpoint type".into()))?;

        let addr = u64::from_str_radix(parts[1], 16).map_err(|_| DebugError::InvalidAddress(0))?;

        let kind = parts[2]
            .parse::<u64>()
            .map_err(|_| DebugError::InvalidPacket("Invalid breakpoint kind".into()))?;

        Ok((bp_type, addr, kind))
    }

    /// 解析删除断点命令 (z0/z1/z2/z3/z4)
    pub fn parse_breakpoint_remove(cmd: &str) -> Result<(u8, u64, u64), DebugError> {
        // 格式与设置相同
        Self::parse_breakpoint_set(&cmd.replacen('z', "Z", 1))
    }

    /// 解析内存读取命令 (m)
    /// 格式: maddr,length
    pub fn parse_memory_read(cmd: &str) -> Result<(u64, usize), DebugError> {
        // 跳过 'm'
        let parts: Vec<&str> = cmd[1..].split(',').collect();
        if parts.len() != 2 {
            return Err(DebugError::InvalidPacket(
                "Memory read command needs 2 parameters".into(),
            ));
        }

        let addr = u64::from_str_radix(parts[0], 16).map_err(|_| DebugError::InvalidAddress(0))?;

        let len = usize::from_str_radix(parts[1], 16)
            .map_err(|_| DebugError::InvalidPacket("Invalid length".into()))?;

        Ok((addr, len))
    }

    /// 解析内存写入命令 (M)
    /// 格式: Maddr,length:data
    pub fn parse_memory_write(cmd: &str) -> Result<(u64, Vec<u8>), DebugError> {
        // 跳过 'M'
        let colon_pos = cmd
            .find(':')
            .ok_or_else(|| DebugError::InvalidPacket("Memory write command missing ':'".into()))?;

        let params = &cmd[1..colon_pos];
        let data_hex = &cmd[colon_pos + 1..];

        let parts: Vec<&str> = params.split(',').collect();
        if parts.len() != 2 {
            return Err(DebugError::InvalidPacket(
                "Memory write command needs 2 parameters".into(),
            ));
        }

        let addr = u64::from_str_radix(parts[0], 16).map_err(|_| DebugError::InvalidAddress(0))?;

        let data = GdbPacket::decode_hex(data_hex)?;
        let expected_len = usize::from_str_radix(parts[1], 16)
            .map_err(|_| DebugError::InvalidPacket("Invalid length".into()))?;

        if data.len() != expected_len {
            return Err(DebugError::InvalidPacket(format!(
                "Data length mismatch: expected {}, got {}",
                expected_len,
                data.len()
            )));
        }

        Ok((addr, data))
    }

    /// 解析寄存器读取命令 (p)
    /// 格式: pn
    pub fn parse_register_read(cmd: &str) -> Result<u32, DebugError> {
        // 跳过 'p'
        u32::from_str_radix(&cmd[1..], 16)
            .map_err(|_| DebugError::InvalidPacket("Invalid register number".into()))
    }

    /// 解析寄存器写入命令 (P)
    /// 格式: Pn=value
    pub fn parse_register_write(cmd: &str) -> Result<(u32, u64), DebugError> {
        // 跳过 'P'
        let eq_pos = cmd.find('=').ok_or_else(|| {
            DebugError::InvalidPacket("Register write command missing '='".into())
        })?;

        let reg_num =
            u32::from_str_radix(&cmd[1..eq_pos], 16).map_err(|_| DebugError::InvalidRegister(0))?;

        let value = u64::from_str_radix(&cmd[eq_pos + 1..], 16)
            .map_err(|_| DebugError::InvalidPacket("Invalid register value".into()))?;

        Ok((reg_num, value))
    }

    /// 解析 vCont 命令
    /// 格式: vCont[;action[:thread-id]]...
    pub fn parse_vcont(cmd: &str) -> Result<Vec<(char, Option<u64>)>, DebugError> {
        let mut actions = Vec::new();

        for part in cmd[6..].split(';') {
            if part.is_empty() {
                continue;
            }

            let mut chars = part.chars();
            let action = chars
                .next()
                .ok_or_else(|| DebugError::InvalidPacket("Empty vCont action".into()))?;

            let thread_id = if chars.next() == Some(':') {
                let tid: String = chars.collect();
                Some(
                    tid.parse::<u64>()
                        .map_err(|_| DebugError::InvalidPacket("Invalid thread ID".into()))?,
                )
            } else {
                None
            };

            actions.push((action, thread_id));
        }

        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_calculation() {
        assert_eq!(calculate_checksum("OK"), 0x4F + 0x4B); // 'O' + 'K'
        assert_eq!(calculate_checksum("g"), 0x67); // 'g'
    }

    #[test]
    fn test_packet_encode_decode() {
        let packet = GdbPacket::new("OK");
        let encoded = packet.encode();
        assert!(encoded.starts_with("$OK#"));

        let (decoded, _) = GdbPacket::parse(&encoded).unwrap();
        assert_eq!(decoded.data, "OK");
    }

    #[test]
    fn test_packet_parse_with_junk() {
        // 测试带有前置垃圾数据的包
        let input = "xx$OK#9a";
        let (packet, consumed) = GdbPacket::parse(input).unwrap();
        assert_eq!(packet.data, "OK");
        assert_eq!(consumed, 8); // 整个字符串
    }

    #[test]
    fn test_hex_encoding() {
        let data = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let hex = GdbPacket::encode_hex(&data);
        assert_eq!(hex, "0123456789abcdef");

        let decoded = GdbPacket::decode_hex(&hex).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_parse_memory_read() {
        let (addr, len) = RspProtocol::parse_memory_read("m1000,20").unwrap();
        assert_eq!(addr, 0x1000);
        assert_eq!(len, 0x20);
    }

    #[test]
    fn test_parse_memory_write() {
        let (addr, data) = RspProtocol::parse_memory_write("M1000,4:12345678").unwrap();
        assert_eq!(addr, 0x1000);
        assert_eq!(data, vec![0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn test_parse_register_read() {
        let reg = RspProtocol::parse_register_read("p1f").unwrap();
        assert_eq!(reg, 31);

        let reg = RspProtocol::parse_register_read("p20").unwrap();
        assert_eq!(reg, 32); // PC
    }

    #[test]
    fn test_parse_register_write() {
        let (reg, val) = RspProtocol::parse_register_write("P1=0000000000000042").unwrap();
        assert_eq!(reg, 1);
        assert_eq!(val, 0x42);
    }

    #[test]
    fn test_parse_breakpoint() {
        let (bp_type, addr, kind) = RspProtocol::parse_breakpoint_set("Z0,1000,4").unwrap();
        assert_eq!(bp_type, 0); // 软件断点
        assert_eq!(addr, 0x1000);
        assert_eq!(kind, 4);
    }

    #[test]
    fn test_vcont_parsing() {
        let actions = RspProtocol::parse_vcont("vCont;c:1").unwrap();
        assert_eq!(actions, vec![('c', Some(1))]);

        let actions = RspProtocol::parse_vcont("vCont;s:1;c").unwrap();
        assert_eq!(actions, vec![('s', Some(1)), ('c', None)]);
    }

    #[test]
    fn test_protocol_buffer() {
        let mut proto = RspProtocol::new();

        // 分段接收
        proto.feed("$OK#9a$");
        let result = proto.try_parse_packet();
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap().data, "OK");

        // 缓冲区应该保留未解析的部分
        proto.feed("g#");
        // 还不够完整
        assert!(proto.try_parse_packet().is_none());

        proto.feed("67"); // 添加校验和
        let result = proto.try_parse_packet();
        assert!(result.is_some());
        assert_eq!(result.unwrap().unwrap().data, "g");
    }

    #[test]
    fn test_escape_unescape() {
        // 测试包含特殊字符的数据
        let data = vec![0x24, 0x23, 0x7d, 0x2a]; // $ # } *
        let escaped = GdbPacket::escape_binary(&data);
        assert_eq!(
            escaped,
            vec![0x7d, 0x04, 0x7d, 0x03, 0x7d, 0x5d, 0x7d, 0x0a]
        );

        let unescaped = GdbPacket::unescape_binary(&escaped);
        assert_eq!(unescaped, data);
    }
}
