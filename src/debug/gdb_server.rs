//! GDB 服务器实现
//!
//! 实现基于 TCP 的 GDB 远程串行协议服务器，支持多连接和异步处理。

use super::{
    BreakpointManager, BreakpointType, DebugError, DebugTarget, GdbPacket, RspProtocol, StopReason,
    WatchpointManager, WatchpointType,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// GDB 服务器配置
#[derive(Debug, Clone)]
pub struct GdbServerConfig {
    /// 监听地址
    pub host: String,
    /// 监听端口
    pub port: u16,
    /// 连接超时（秒）
    pub connection_timeout: u64,
    /// 是否启用确认
    pub ack_enabled: bool,
    /// 最大连接数
    pub max_connections: usize,
}

impl Default for GdbServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 1234,
            connection_timeout: 30,
            ack_enabled: true,
            max_connections: 1,
        }
    }
}

impl GdbServerConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置主机地址
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// 设置端口
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// 设置连接超时
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// 设置是否启用确认
    pub fn with_ack(mut self, ack_enabled: bool) -> Self {
        self.ack_enabled = ack_enabled;
        self
    }

    /// 获取完整的监听地址
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// GDB 服务器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GdbServerState {
    /// 已停止
    Stopped,
    /// 正在监听
    Listening,
    /// 客户端已连接
    Connected,
    /// 正在调试
    Debugging,
}

/// GDB 服务器
pub struct GdbServer {
    config: GdbServerConfig,
    state: Arc<Mutex<GdbServerState>>,
    breakpoint_manager: Arc<Mutex<BreakpointManager>>,
    watchpoint_manager: Arc<Mutex<WatchpointManager>>,
    listener: Option<TcpListener>,
    running: Arc<Mutex<bool>>,
}

impl GdbServer {
    /// 创建新的 GDB 服务器
    pub fn new(config: GdbServerConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(GdbServerState::Stopped)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            listener: None,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> GdbServerState {
        *self.state.lock().unwrap()
    }

    /// 获取配置
    pub fn config(&self) -> &GdbServerConfig {
        &self.config
    }

    /// 启动服务器（阻塞模式）
    pub fn start<T: DebugTarget + Send + 'static>(
        &mut self,
        target: Arc<Mutex<T>>,
    ) -> Result<(), DebugError> {
        let listener = TcpListener::bind(self.config.address())?;
        listener.set_nonblocking(false)?;

        *self.state.lock().unwrap() = GdbServerState::Listening;
        self.listener = Some(listener);
        *self.running.lock().unwrap() = true;

        println!("GDB server listening on {}", self.config.address());

        while *self.running.lock().unwrap() {
            if let Ok((stream, addr)) = self.listener.as_ref().unwrap().accept() {
                println!("GDB client connected from {}", addr);
                *self.state.lock().unwrap() = GdbServerState::Connected;

                stream
                    .set_read_timeout(Some(Duration::from_secs(self.config.connection_timeout)))
                    .ok();
                stream
                    .set_write_timeout(Some(Duration::from_secs(self.config.connection_timeout)))
                    .ok();

                self.handle_client(stream, Arc::clone(&target))?;

                *self.state.lock().unwrap() = GdbServerState::Listening;
            }
        }

        *self.state.lock().unwrap() = GdbServerState::Stopped;
        Ok(())
    }

    /// 在后台线程启动服务器
    pub fn start_background<T: DebugTarget + Send + 'static>(
        &mut self,
        target: Arc<Mutex<T>>,
    ) -> Result<thread::JoinHandle<()>, DebugError> {
        let listener = TcpListener::bind(self.config.address())?;
        listener.set_nonblocking(false)?;

        *self.state.lock().unwrap() = GdbServerState::Listening;
        self.listener = Some(listener);
        *self.running.lock().unwrap() = true;

        let state = Arc::clone(&self.state);
        let running = Arc::clone(&self.running);
        let listener = self.listener.take().unwrap();
        let config = self.config.clone();
        let bp_mgr = Arc::clone(&self.breakpoint_manager);
        let wp_mgr = Arc::clone(&self.watchpoint_manager);

        println!(
            "GDB server listening on {} (background mode)",
            config.address()
        );

        let handle = thread::spawn(move || {
            let mut handler = ClientHandler {
                config,
                state,
                breakpoint_manager: bp_mgr,
                watchpoint_manager: wp_mgr,
                running,
            };

            while *handler.running.lock().unwrap() {
                if let Ok((stream, addr)) = listener.accept() {
                    println!("GDB client connected from {}", addr);
                    *handler.state.lock().unwrap() = GdbServerState::Connected;

                    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
                    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

                    if let Err(e) = handler.handle_client(stream, Arc::clone(&target)) {
                        eprintln!("Client handler error: {}", e);
                    }

                    *handler.state.lock().unwrap() = GdbServerState::Listening;
                }
            }

            *handler.state.lock().unwrap() = GdbServerState::Stopped;
        });

        Ok(handle)
    }

    /// 停止服务器
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        // 连接到自己以解除 accept 阻塞
        let _ = TcpStream::connect(self.config.address());
    }

    /// 处理客户端连接
    fn handle_client<T: DebugTarget + Send + 'static>(
        &self,
        mut stream: TcpStream,
        target: Arc<Mutex<T>>,
    ) -> Result<(), DebugError> {
        let mut protocol = RspProtocol::new();
        protocol.set_ack_enabled(self.config.ack_enabled);

        let mut buffer = [0u8; 4096];
        let mut handler = ClientHandler {
            config: self.config.clone(),
            state: Arc::clone(&self.state),
            breakpoint_manager: Arc::clone(&self.breakpoint_manager),
            watchpoint_manager: Arc::clone(&self.watchpoint_manager),
            running: Arc::clone(&self.running),
        };

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    println!("GDB client disconnected");
                    return Ok(());
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]);
                    protocol.feed(&data);

                    while let Some(result) = protocol.try_parse_packet() {
                        match result {
                            Ok(packet) => {
                                if self.config.ack_enabled {
                                    stream.write_all(RspProtocol::ack().as_bytes())?;
                                }

                                let response = handler.process_packet(&packet, &target)?;
                                let encoded = response.encode();
                                stream.write_all(encoded.as_bytes())?;
                                stream.flush()?;
                            }
                            Err(e) => {
                                eprintln!("Packet parse error: {}", e);
                                if self.config.ack_enabled {
                                    stream.write_all(RspProtocol::nack().as_bytes())?;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    return Err(DebugError::Io(e));
                }
            }
        }
    }

    /// 获取断点管理器
    pub fn breakpoint_manager(&self) -> &Arc<Mutex<BreakpointManager>> {
        &self.breakpoint_manager
    }

    /// 获取观察点管理器
    pub fn watchpoint_manager(&self) -> &Arc<Mutex<WatchpointManager>> {
        &self.watchpoint_manager
    }
}

/// 客户端连接处理器
struct ClientHandler {
    config: GdbServerConfig,
    state: Arc<Mutex<GdbServerState>>,
    breakpoint_manager: Arc<Mutex<BreakpointManager>>,
    watchpoint_manager: Arc<Mutex<WatchpointManager>>,
    running: Arc<Mutex<bool>>,
}

impl ClientHandler {
    /// 处理单个 GDB 数据包
    fn process_packet<T: DebugTarget>(
        &mut self,
        packet: &GdbPacket,
        target: &Arc<Mutex<T>>,
    ) -> Result<GdbPacket, DebugError> {
        let data = &packet.data;

        if data.is_empty() {
            return Ok(GdbPacket::empty());
        }

        match data.chars().next().unwrap() {
            '?' => self.handle_stop_reason(),
            'g' => self.handle_read_all_registers(target),
            'G' => self.handle_write_all_registers(target, data),
            'p' => self.handle_read_register(target, data),
            'P' => self.handle_write_register(target, data),
            'm' => self.handle_read_memory(target, data),
            'M' => self.handle_write_memory(target, data),
            'c' => self.handle_continue(target),
            's' => self.handle_step(target),
            'Z' => self.handle_set_breakpoint(data),
            'z' => self.handle_remove_breakpoint(data),
            'q' => self.handle_query(data),
            'Q' => self.handle_set(data),
            'v' => self.handle_vpacket(target, data),
            'H' => self.handle_set_thread(data),
            'T' => self.handle_check_thread(data),
            'D' => self.handle_detach(),
            'k' => self.handle_kill(target),
            '!' => self.handle_extended_mode(),
            _ => Ok(GdbPacket::empty()), // 不支持的命令返回空响应
        }
    }

    /// 处理停止原因查询 (?)
    fn handle_stop_reason(&self) -> Result<GdbPacket, DebugError> {
        // 返回停止原因 S05 (SIGTRAP)
        Ok(GdbPacket::new("S05"))
    }

    /// 处理读取所有寄存器 (g)
    fn handle_read_all_registers<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
    ) -> Result<GdbPacket, DebugError> {
        let target = target.lock().unwrap();
        let data = target.read_all_registers()?;
        Ok(RspProtocol::encode_registers(&data))
    }

    /// 处理写入所有寄存器 (G)
    fn handle_write_all_registers<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let reg_data = GdbPacket::decode_hex(&data[1..])?;
        let mut target = target.lock().unwrap();
        target.write_all_registers(&reg_data)?;
        Ok(GdbPacket::ok())
    }

    /// 处理读取单个寄存器 (p)
    fn handle_read_register<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let reg_num = RspProtocol::parse_register_read(data)?;
        let target = target.lock().unwrap();
        let value = target.read_register(reg_num)?;
        Ok(GdbPacket::new(format!("{:016x}", value)))
    }

    /// 处理写入单个寄存器 (P)
    fn handle_write_register<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let (reg_num, value) = RspProtocol::parse_register_write(data)?;
        let mut target = target.lock().unwrap();
        target.write_register(reg_num, value)?;
        Ok(GdbPacket::ok())
    }

    /// 处理读取内存 (m)
    fn handle_read_memory<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let (addr, len) = RspProtocol::parse_memory_read(data)?;
        let target = target.lock().unwrap();
        let mem_data = target.read_memory(addr, len)?;
        Ok(RspProtocol::encode_memory_read(&mem_data))
    }

    /// 处理写入内存 (M)
    fn handle_write_memory<T: DebugTarget>(
        &self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let (addr, mem_data) = RspProtocol::parse_memory_write(data)?;
        let mut target = target.lock().unwrap();
        target.write_memory(addr, &mem_data)?;
        Ok(GdbPacket::ok())
    }

    /// 处理继续执行 (c)
    fn handle_continue<T: DebugTarget>(
        &mut self,
        target: &Arc<Mutex<T>>,
    ) -> Result<GdbPacket, DebugError> {
        *self.state.lock().unwrap() = GdbServerState::Debugging;
        let mut target = target.lock().unwrap();
        let stop_reason = target.continue_execution()?;
        *self.state.lock().unwrap() = GdbServerState::Connected;
        Ok(RspProtocol::stop_reply(&stop_reason))
    }

    /// 处理单步执行 (s)
    fn handle_step<T: DebugTarget>(
        &mut self,
        target: &Arc<Mutex<T>>,
    ) -> Result<GdbPacket, DebugError> {
        *self.state.lock().unwrap() = GdbServerState::Debugging;
        let mut target = target.lock().unwrap();
        let stop_reason = target.step()?;
        *self.state.lock().unwrap() = GdbServerState::Connected;
        Ok(RspProtocol::stop_reply(&stop_reason))
    }

    /// 处理设置断点 (Z)
    fn handle_set_breakpoint(&self, data: &str) -> Result<GdbPacket, DebugError> {
        let (bp_type, addr, _kind) = RspProtocol::parse_breakpoint_set(data)?;

        if let Some(bp_type) = BreakpointType::from_gdb_type(bp_type) {
            let mut bp_mgr = self.breakpoint_manager.lock().unwrap();
            match bp_mgr.add_breakpoint(addr, bp_type, 4) {
                Ok(_) => Ok(GdbPacket::ok()),
                Err(_e) => Ok(GdbPacket::error(0x01)), // 通用错误
            }
        } else {
            // 观察点类型 (2=read, 3=write, 4=access)
            if let Some(wp_type) = WatchpointType::from_gdb_type(bp_type) {
                let mut wp_mgr = self.watchpoint_manager.lock().unwrap();
                match wp_mgr.add_watchpoint(addr, wp_type, _kind.max(4)) {
                    Ok(_) => Ok(GdbPacket::ok()),
                    Err(_e) => Ok(GdbPacket::error(0x01)),
                }
            } else {
                Ok(GdbPacket::empty()) // 不支持的类型
            }
        }
    }

    /// 处理删除断点 (z)
    fn handle_remove_breakpoint(&self, data: &str) -> Result<GdbPacket, DebugError> {
        let (bp_type, addr, _kind) = RspProtocol::parse_breakpoint_remove(data)?;

        if let Some(bp_type) = BreakpointType::from_gdb_type(bp_type) {
            let mut bp_mgr = self.breakpoint_manager.lock().unwrap();
            match bp_mgr.remove_breakpoint(addr, bp_type) {
                Ok(_) => Ok(GdbPacket::ok()),
                Err(_e) => Ok(GdbPacket::error(0x01)),
            }
        } else {
            // 观察点类型
            if let Some(wp_type) = WatchpointType::from_gdb_type(bp_type) {
                let mut wp_mgr = self.watchpoint_manager.lock().unwrap();
                match wp_mgr.remove_watchpoint(addr, wp_type, _kind.max(4)) {
                    Ok(_) => Ok(GdbPacket::ok()),
                    Err(_e) => Ok(GdbPacket::error(0x01)),
                }
            } else {
                Ok(GdbPacket::empty())
            }
        }
    }

    /// 处理查询命令 (q)
    fn handle_query(&self, data: &str) -> Result<GdbPacket, DebugError> {
        if data.starts_with("qSupported") {
            // 报告支持的特性
            Ok(GdbPacket::new(
                "PacketSize=1000;qXfer:features:read+;qXfer:threads:read+;multiprocess+",
            ))
        } else if data.starts_with("qAttached") {
            // 报告是否附加到现有进程
            Ok(GdbPacket::new("1"))
        } else if data.starts_with("qC") {
            // 获取当前线程 ID
            Ok(GdbPacket::new("QC1"))
        } else if data.starts_with("qfThreadInfo") {
            // 获取线程信息（开始）
            Ok(GdbPacket::new("m1"))
        } else if data.starts_with("qsThreadInfo") {
            // 获取线程信息（结束）
            Ok(GdbPacket::new("l"))
        } else if data.starts_with("qThreadExtraInfo") {
            // 线程额外信息
            Ok(GdbPacket::new("72756e6e696e67")) // "running" in hex
        } else if data.starts_with("qXfer:features:read:target.xml") {
            // 返回目标描述
            let features = Self::target_features_xml();
            Ok(GdbPacket::new(format!("l{}", features)))
        } else {
            Ok(GdbPacket::empty())
        }
    }

    /// 处理设置命令 (Q)
    fn handle_set(&self, data: &str) -> Result<GdbPacket, DebugError> {
        if data.starts_with("QStartNoAckMode") {
            // 开始无确认模式
            Ok(GdbPacket::ok())
        } else {
            Ok(GdbPacket::empty())
        }
    }

    /// 处理 v 包命令
    fn handle_vpacket<T: DebugTarget>(
        &mut self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        if data == "vMustReplyEmpty" {
            // 必须返回空
            Ok(GdbPacket::empty())
        } else if data.starts_with("vCont") {
            // vCont 命令
            self.handle_vcont(target, data)
        } else if data == "vCtrlC" {
            // Ctrl-C 中断
            let mut target = target.lock().unwrap();
            target.stop();
            Ok(GdbPacket::new("S02")) // SIGINT
        } else {
            Ok(GdbPacket::empty())
        }
    }

    /// 处理 vCont 命令
    fn handle_vcont<T: DebugTarget>(
        &mut self,
        target: &Arc<Mutex<T>>,
        data: &str,
    ) -> Result<GdbPacket, DebugError> {
        let actions = RspProtocol::parse_vcont(data)?;

        for (action, _thread_id) in actions {
            match action {
                'c' => {
                    return self.handle_continue(target);
                }
                's' => {
                    return self.handle_step(target);
                }
                't' => {
                    let mut target = target.lock().unwrap();
                    target.stop();
                    return Ok(GdbPacket::new("S02"));
                }
                _ => {}
            }
        }

        Ok(GdbPacket::empty())
    }

    /// 处理设置线程 (H)
    fn handle_set_thread(&self, _data: &str) -> Result<GdbPacket, DebugError> {
        // Hc/ Hg 命令 - 设置当前线程
        // 我们目前只支持单线程，所以总是返回 OK
        Ok(GdbPacket::ok())
    }

    /// 处理检查线程 (T)
    fn handle_check_thread(&self, _data: &str) -> Result<GdbPacket, DebugError> {
        // Txx 命令 - 检查线程是否存活
        // 我们目前只支持线程 1
        Ok(GdbPacket::ok())
    }

    /// 处理分离 (D)
    fn handle_detach(&self) -> Result<GdbPacket, DebugError> {
        Ok(GdbPacket::ok())
    }

    /// 处理 kill (k)
    fn handle_kill<T: DebugTarget>(&self, target: &Arc<Mutex<T>>) -> Result<GdbPacket, DebugError> {
        let mut target = target.lock().unwrap();
        target.stop();
        Ok(GdbPacket::ok())
    }

    /// 处理扩展模式 (!)
    fn handle_extended_mode(&self) -> Result<GdbPacket, DebugError> {
        Ok(GdbPacket::ok())
    }

    /// 目标特性 XML
    fn target_features_xml() -> String {
        r#"<?xml version="1.0"?>
<!DOCTYPE target SYSTEM "gdb-target.dtd">
<target>
  <architecture>riscv</architecture>
  <xi:include href="riscv-64bit.xml"/>
</target>"#
            .to_string()
    }

    /// 处理客户端连接（用于后台模式）
    fn handle_client<T: DebugTarget + Send + 'static>(
        &mut self,
        stream: TcpStream,
        target: Arc<Mutex<T>>,
    ) -> Result<(), DebugError> {
        let mut protocol = RspProtocol::new();
        protocol.set_ack_enabled(self.config.ack_enabled);

        let mut stream = stream;
        let mut buffer = [0u8; 4096];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    println!("GDB client disconnected");
                    return Ok(());
                }
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buffer[..n]);
                    protocol.feed(&data);

                    while let Some(result) = protocol.try_parse_packet() {
                        match result {
                            Ok(packet) => {
                                if self.config.ack_enabled {
                                    stream.write_all(RspProtocol::ack().as_bytes())?;
                                }

                                let response = self.process_packet(&packet, &target)?;
                                let encoded = response.encode();
                                stream.write_all(encoded.as_bytes())?;
                                stream.flush()?;
                            }
                            Err(e) => {
                                eprintln!("Packet parse error: {}", e);
                                if self.config.ack_enabled {
                                    stream.write_all(RspProtocol::nack().as_bytes())?;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    return Err(DebugError::Io(e));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WatchpointAccess;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// 模拟调试目标（用于测试）
    struct MockTarget {
        registers: [AtomicU64; 66],
        memory: Mutex<Vec<u8>>,
        pc: AtomicU64,
        running: AtomicBool,
        stop_reason: Mutex<StopReason>,
    }

    impl MockTarget {
        fn new() -> Self {
            Self {
                registers: std::array::from_fn(|_| AtomicU64::new(0)),
                memory: Mutex::new(vec![0; 0x10000]),
                pc: AtomicU64::new(0x8000_0000),
                running: AtomicBool::new(false),
                stop_reason: Mutex::new(StopReason::Unknown),
            }
        }
    }

    impl DebugTarget for MockTarget {
        fn read_register(&self, reg_num: u32) -> Result<u64, DebugError> {
            if reg_num as usize >= self.registers.len() {
                return Err(DebugError::InvalidRegister(reg_num));
            }
            Ok(self.registers[reg_num as usize].load(Ordering::SeqCst))
        }

        fn write_register(&mut self, reg_num: u32, value: u64) -> Result<(), DebugError> {
            if reg_num as usize >= self.registers.len() {
                return Err(DebugError::InvalidRegister(reg_num));
            }
            self.registers[reg_num as usize].store(value, Ordering::SeqCst);
            Ok(())
        }

        fn read_memory(&self, addr: u64, len: usize) -> Result<Vec<u8>, DebugError> {
            let memory = self.memory.lock().unwrap();
            let addr = addr as usize;
            if addr + len > memory.len() {
                return Err(DebugError::InvalidAddress(addr as u64));
            }
            Ok(memory[addr..addr + len].to_vec())
        }

        fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), DebugError> {
            let mut memory = self.memory.lock().unwrap();
            let addr = addr as usize;
            if addr + data.len() > memory.len() {
                return Err(DebugError::InvalidAddress(addr as u64));
            }
            memory[addr..addr + data.len()].copy_from_slice(data);
            Ok(())
        }

        fn get_pc(&self) -> u64 {
            self.pc.load(Ordering::SeqCst)
        }

        fn set_pc(&mut self, pc: u64) {
            self.pc.store(pc, Ordering::SeqCst);
        }

        fn continue_execution(&mut self) -> Result<StopReason, DebugError> {
            self.running.store(true, Ordering::SeqCst);
            // 模拟执行后停止
            self.running.store(false, Ordering::SeqCst);
            Ok(StopReason::StepDone)
        }

        fn step(&mut self) -> Result<StopReason, DebugError> {
            let pc = self.pc.load(Ordering::SeqCst);
            self.pc.store(pc + 4, Ordering::SeqCst);
            Ok(StopReason::StepDone)
        }

        fn stop(&mut self) {
            self.running.store(false, Ordering::SeqCst);
        }

        fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }

        fn get_stop_reason(&self) -> StopReason {
            *self.stop_reason.lock().unwrap()
        }

        fn check_breakpoint(&self, _addr: u64) -> bool {
            false
        }

        fn check_watchpoint(&self, _addr: u64, _access_type: WatchpointAccess) -> bool {
            false
        }
    }

    #[test]
    fn test_server_config() {
        let config = GdbServerConfig::new()
            .with_host("0.0.0.0")
            .with_port(5678)
            .with_timeout(60)
            .with_ack(false);

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 5678);
        assert_eq!(config.connection_timeout, 60);
        assert!(!config.ack_enabled);
        assert_eq!(config.address(), "0.0.0.0:5678");
    }

    #[test]
    fn test_server_creation() {
        let config = GdbServerConfig::new();
        let server = GdbServer::new(config);

        assert_eq!(server.state(), GdbServerState::Stopped);
    }

    #[test]
    fn test_client_handler_register_read_write() {
        let config = GdbServerConfig::default();
        let mut handler = ClientHandler {
            config: config.clone(),
            state: Arc::new(Mutex::new(GdbServerState::Connected)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            running: Arc::new(Mutex::new(true)),
        };

        let target = Arc::new(Mutex::new(MockTarget::new()));

        // 测试写入寄存器
        let packet = GdbPacket::new("P1=0000000000000042");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "OK");

        // 测试读取寄存器
        let packet = GdbPacket::new("p1");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "0000000000000042");
    }

    #[test]
    fn test_client_handler_memory_read_write() {
        let config = GdbServerConfig::default();
        let mut handler = ClientHandler {
            config: config.clone(),
            state: Arc::new(Mutex::new(GdbServerState::Connected)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            running: Arc::new(Mutex::new(true)),
        };

        let target = Arc::new(Mutex::new(MockTarget::new()));

        // 测试写入内存
        let packet = GdbPacket::new("M1000,4:12345678");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "OK");

        // 测试读取内存
        let packet = GdbPacket::new("m1000,4");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "12345678");
    }

    #[test]
    fn test_client_handler_breakpoint() {
        let config = GdbServerConfig::default();
        let mut handler = ClientHandler {
            config: config.clone(),
            state: Arc::new(Mutex::new(GdbServerState::Connected)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            running: Arc::new(Mutex::new(true)),
        };

        let target = Arc::new(Mutex::new(MockTarget::new()));

        // 测试设置断点
        let packet = GdbPacket::new("Z0,1000,4");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "OK");

        // 验证断点已添加
        {
            let bp_mgr = handler.breakpoint_manager.lock().unwrap();
            assert!(bp_mgr.has_breakpoint(0x1000));
        }

        // 测试删除断点
        let packet = GdbPacket::new("z0,1000,4");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "OK");
    }

    #[test]
    fn test_client_handler_step() {
        let config = GdbServerConfig::default();
        let mut handler = ClientHandler {
            config: config.clone(),
            state: Arc::new(Mutex::new(GdbServerState::Connected)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            running: Arc::new(Mutex::new(true)),
        };

        let target = Arc::new(Mutex::new(MockTarget::new()));

        let packet = GdbPacket::new("s");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert!(response.data.starts_with("T05"));
    }

    #[test]
    fn test_client_handler_query() {
        let config = GdbServerConfig::default();
        let mut handler = ClientHandler {
            config: config.clone(),
            state: Arc::new(Mutex::new(GdbServerState::Connected)),
            breakpoint_manager: Arc::new(Mutex::new(BreakpointManager::new())),
            watchpoint_manager: Arc::new(Mutex::new(WatchpointManager::new())),
            running: Arc::new(Mutex::new(true)),
        };

        let target = Arc::new(Mutex::new(MockTarget::new()));

        // 测试 qSupported
        let packet = GdbPacket::new("qSupported");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert!(response.data.contains("PacketSize"));

        // 测试 ?
        let packet = GdbPacket::new("?");
        let response = handler.process_packet(&packet, &target).unwrap();
        assert_eq!(response.data, "S05");
    }
}
