//! 命令行调试界面
//!
//! 提供交互式调试命令行界面，支持断点管理、寄存器查看、内存操作等。

use super::{
    BreakpointManager, BreakpointType, DebugError, DebugTarget, GdbServer, GdbServerConfig,
    StopReason, WatchpointManager, WatchpointType,
};
use std::io::{self, BufRead, Write};

/// CLI 调试器命令
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugCommand {
    /// 帮助
    Help,
    /// 运行/继续
    Run,
    Continue,
    /// 单步执行
    Step,
    StepInstruction,
    /// 下断点
    Break(u64),
    BreakSymbol(String),
    /// 删除断点
    DeleteBreakpoint(usize),
    /// 列出断点
    ListBreakpoints,
    /// 设置观察点
    WatchRead(u64, u64),
    WatchWrite(u64, u64),
    WatchAccess(u64, u64),
    /// 列出观察点
    ListWatchpoints,
    /// 删除观察点
    DeleteWatchpoint(usize),
    /// 打印寄存器
    PrintRegisters,
    PrintRegister(String),
    /// 打印内存
    Examine(u64, usize),
    /// 设置寄存器
    SetRegister(String, u64),
    /// 设置内存
    SetMemory(u64, Vec<u8>),
    /// 反汇编
    Disassemble(u64, usize),
    /// 显示调用栈
    Backtrace,
    /// 跳转到地址
    Jump(u64),
    /// 启动 GDB 服务器
    GdbServer(u16),
    /// 停止 GDB 服务器
    StopGdbServer,
    /// 显示信息
    Info(String),
    /// 退出
    Quit,
    /// 空命令
    Empty,
    /// 未知命令
    Unknown(String),
}

impl DebugCommand {
    /// 从字符串解析命令
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.is_empty() {
            return Self::Empty;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "h" | "help" | "?" => Self::Help,
            "r" | "run" => Self::Run,
            "c" | "continue" | "cont" => Self::Continue,
            "s" | "step" | "si" => Self::Step,
            "ni" | "nexti" => Self::StepInstruction,
            "b" | "break" | "breakpoint" => Self::parse_breakpoint(&parts),
            "d" | "delete" => Self::parse_delete(&parts),
            "info" | "i" => Self::parse_info(&parts),
            "x" | "examine" => Self::parse_examine(&parts),
            "p" | "print" => Self::parse_print(&parts),
            "set" => Self::parse_set(&parts),
            "disas" | "disassemble" => Self::parse_disassemble(&parts),
            "bt" | "backtrace" => Self::Backtrace,
            "j" | "jump" => Self::parse_jump(&parts),
            "gdb" => Self::parse_gdb(&parts),
            "watch" => Self::parse_watch(&parts, WatchpointType::Write),
            "rwatch" => Self::parse_watch(&parts, WatchpointType::Read),
            "awatch" => Self::parse_watch(&parts, WatchpointType::Access),
            "q" | "quit" | "exit" => Self::Quit,
            _ => Self::Unknown(input.to_string()),
        }
    }

    fn parse_breakpoint(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("break requires address".to_string());
        }

        // 尝试解析为十六进制地址
        if let Ok(addr) = Self::parse_address(parts[1]) {
            Self::Break(addr)
        } else {
            // 否则视为符号
            Self::BreakSymbol(parts[1].to_string())
        }
    }

    fn parse_delete(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("delete requires breakpoint number".to_string());
        }

        if let Ok(num) = parts[1].parse::<usize>() {
            Self::DeleteBreakpoint(num)
        } else {
            Self::Unknown(format!("invalid breakpoint number: {}", parts[1]))
        }
    }

    fn parse_info(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Info("all".to_string());
        }

        let subcmd = parts[1].to_lowercase();
        match subcmd.as_str() {
            "b" | "break" | "breakpoints" => Self::ListBreakpoints,
            "w" | "watch" | "watchpoints" => Self::ListWatchpoints,
            "r" | "reg" | "registers" => Self::PrintRegisters,
            _ => Self::Info(subcmd),
        }
    }

    fn parse_examine(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("examine requires address".to_string());
        }

        let addr = match Self::parse_address(parts[1]) {
            Ok(a) => a,
            Err(_) => return Self::Unknown(format!("invalid address: {}", parts[1])),
        };

        let count = if parts.len() >= 3 {
            parts[2].parse::<usize>().unwrap_or(16)
        } else {
            16
        };

        Self::Examine(addr, count)
    }

    fn parse_print(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::PrintRegisters;
        }

        let arg = parts[1].to_lowercase();
        if arg == "regs" || arg == "registers" {
            Self::PrintRegisters
        } else {
            Self::PrintRegister(parts[1].to_string())
        }
    }

    fn parse_set(parts: &[&str]) -> Self {
        if parts.len() < 3 {
            return Self::Unknown("set requires variable and value".to_string());
        }

        let var = parts[1].to_lowercase();
        if var.starts_with('$') {
            // 设置寄存器
            let reg = var.strip_prefix('$').unwrap();
            if let Ok(val) = Self::parse_value(parts[2]) {
                Self::SetRegister(reg.to_string(), val)
            } else {
                Self::Unknown(format!("invalid value: {}", parts[2]))
            }
        } else if var.starts_with("*0x") || var.starts_with('*') {
            // 设置内存
            let addr = if var.starts_with("*0x") {
                u64::from_str_radix(var.strip_prefix("*0x").unwrap(), 16)
            } else {
                u64::from_str_radix(&var[1..], 16)
            };

            match addr {
                Ok(a) => {
                    let data = parts[2].as_bytes().to_vec();
                    Self::SetMemory(a, data)
                }
                Err(_) => Self::Unknown(format!("invalid address: {}", var)),
            }
        } else {
            Self::Unknown(format!("unknown set variable: {}", var))
        }
    }

    fn parse_disassemble(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("disassemble requires address".to_string());
        }

        let addr = match Self::parse_address(parts[1]) {
            Ok(a) => a,
            Err(_) => return Self::Unknown(format!("invalid address: {}", parts[1])),
        };

        let count = if parts.len() >= 3 {
            parts[2].parse::<usize>().unwrap_or(10)
        } else {
            10
        };

        Self::Disassemble(addr, count)
    }

    fn parse_jump(parts: &[&str]) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("jump requires address".to_string());
        }

        match Self::parse_address(parts[1]) {
            Ok(addr) => Self::Jump(addr),
            Err(_) => Self::Unknown(format!("invalid address: {}", parts[1])),
        }
    }

    fn parse_gdb(parts: &[&str]) -> Self {
        let port = if parts.len() >= 2 {
            parts[1].parse::<u16>().unwrap_or(1234)
        } else {
            1234
        };
        Self::GdbServer(port)
    }

    fn parse_watch(parts: &[&str], wp_type: WatchpointType) -> Self {
        if parts.len() < 2 {
            return Self::Unknown("watch requires address".to_string());
        }

        let addr = match Self::parse_address(parts[1]) {
            Ok(a) => a,
            Err(_) => return Self::Unknown(format!("invalid address: {}", parts[1])),
        };

        let size = if parts.len() >= 3 {
            parts[2].parse::<u64>().unwrap_or(4)
        } else {
            4
        };

        match wp_type {
            WatchpointType::Read => Self::WatchRead(addr, size),
            WatchpointType::Write => Self::WatchWrite(addr, size),
            WatchpointType::Access => Self::WatchAccess(addr, size),
        }
    }

    fn parse_address(s: &str) -> Result<u64, ()> {
        let s = s.trim();
        if s.starts_with("0x") || s.starts_with("0X") {
            u64::from_str_radix(&s[2..], 16).map_err(|_| ())
        } else if s.starts_with('$') {
            // 寄存器引用，这里简化处理
            Err(())
        } else {
            s.parse::<u64>().map_err(|_| ())
        }
    }

    fn parse_value(s: &str) -> Result<u64, ()> {
        let s = s.trim();
        if s.starts_with("0x") || s.starts_with("0X") {
            u64::from_str_radix(&s[2..], 16).map_err(|_| ())
        } else {
            s.parse::<u64>().map_err(|_| ())
        }
    }
}

/// CLI 调试器
pub struct DebugCli {
    breakpoint_manager: BreakpointManager,
    watchpoint_manager: WatchpointManager,
    gdb_server: Option<GdbServer>,
    last_command: DebugCommand,
    history: Vec<String>,
    max_history: usize,
}

impl Default for DebugCli {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugCli {
    /// 创建新的 CLI 调试器
    pub fn new() -> Self {
        Self {
            breakpoint_manager: BreakpointManager::new(),
            watchpoint_manager: WatchpointManager::new(),
            gdb_server: None,
            last_command: DebugCommand::Empty,
            history: Vec::new(),
            max_history: 100,
        }
    }

    /// 设置最大历史记录数
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    /// 启动交互式调试会话
    pub fn run_interactive<T: DebugTarget>(&mut self, target: &mut T) -> Result<(), DebugError> {
        println!("RISC-V Simulator Debugger");
        println!("Type 'help' for a list of commands.");
        println!();

        let stdin = io::stdin();
        let stdout = io::stdout();

        loop {
            print!("(ruscv) ");
            stdout.lock().flush().unwrap();

            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_err() {
                break;
            }

            let line = line.trim();
            if line.is_empty() {
                // 空行重复上一条命令（如果是 step/continue）
                if matches!(
                    self.last_command,
                    DebugCommand::Step | DebugCommand::StepInstruction | DebugCommand::Continue
                ) {
                    let cmd = self.last_command.clone();
                    if let Err(e) = self.execute_command(cmd, target) {
                        println!("Error: {}", e);
                    }
                }
                continue;
            }

            self.add_to_history(line);
            let cmd = DebugCommand::parse(line);

            if cmd == DebugCommand::Quit {
                println!("Quitting debugger.");
                break;
            }

            if let Err(e) = self.execute_command(cmd.clone(), target) {
                println!("Error: {}", e);
            } else {
                self.last_command = cmd;
            }
        }

        Ok(())
    }

    /// 执行单个命令
    pub fn execute_command<T: DebugTarget>(
        &mut self,
        cmd: DebugCommand,
        target: &mut T,
    ) -> Result<(), DebugError> {
        match cmd {
            DebugCommand::Help => self.print_help(),
            DebugCommand::Run | DebugCommand::Continue => self.cmd_continue(target)?,
            DebugCommand::Step => self.cmd_step(target, false)?,
            DebugCommand::StepInstruction => self.cmd_step(target, true)?,
            DebugCommand::Break(addr) => self.cmd_breakpoint(addr)?,
            DebugCommand::BreakSymbol(sym) => {
                println!("Breakpoint at symbol '{}' not yet implemented", sym)
            }
            DebugCommand::DeleteBreakpoint(num) => self.cmd_delete_breakpoint(num)?,
            DebugCommand::ListBreakpoints => self.cmd_list_breakpoints(),
            DebugCommand::WatchRead(addr, size) => {
                self.cmd_watchpoint(addr, size, WatchpointType::Read)?
            }
            DebugCommand::WatchWrite(addr, size) => {
                self.cmd_watchpoint(addr, size, WatchpointType::Write)?
            }
            DebugCommand::WatchAccess(addr, size) => {
                self.cmd_watchpoint(addr, size, WatchpointType::Access)?
            }
            DebugCommand::ListWatchpoints => self.cmd_list_watchpoints(),
            DebugCommand::DeleteWatchpoint(num) => self.cmd_delete_watchpoint(num)?,
            DebugCommand::PrintRegisters => self.cmd_print_registers(target)?,
            DebugCommand::PrintRegister(reg) => self.cmd_print_register(target, &reg)?,
            DebugCommand::Examine(addr, count) => self.cmd_examine(target, addr, count)?,
            DebugCommand::SetRegister(reg, val) => self.cmd_set_register(target, &reg, val)?,
            DebugCommand::SetMemory(addr, data) => self.cmd_set_memory(target, addr, &data)?,
            DebugCommand::Disassemble(addr, count) => println!(
                "Disassemble at 0x{:x} ({} instrs) not yet implemented",
                addr, count
            ),
            DebugCommand::Backtrace => println!("Backtrace not yet implemented"),
            DebugCommand::Jump(addr) => self.cmd_jump(target, addr)?,
            DebugCommand::GdbServer(port) => self.cmd_gdb_server(port, target)?,
            DebugCommand::StopGdbServer => self.cmd_stop_gdb_server()?,
            DebugCommand::Info(topic) => self.cmd_info(target, &topic)?,
            DebugCommand::Quit => {}
            DebugCommand::Empty => {}
            DebugCommand::Unknown(msg) => println!("Unknown command: {}", msg),
        }

        Ok(())
    }

    /// 打印帮助信息
    fn print_help(&self) {
        println!("RISC-V Simulator Debugger Commands:");
        println!();
        println!("Execution:");
        println!("  run, r              - Start/continue execution");
        println!("  continue, c         - Continue execution");
        println!("  step, s             - Step one source line");
        println!("  nexti, ni           - Step one instruction");
        println!();
        println!("Breakpoints:");
        println!("  break <addr>, b     - Set breakpoint at address");
        println!("  delete <n>, d       - Delete breakpoint number <n>");
        println!("  info breakpoints    - List all breakpoints");
        println!();
        println!("Watchpoints:");
        println!("  watch <addr> [size] - Set write watchpoint");
        println!("  rwatch <addr> [size] - Set read watchpoint");
        println!("  awatch <addr> [size] - Set access watchpoint");
        println!("  info watchpoints    - List all watchpoints");
        println!();
        println!("Registers:");
        println!("  print, p            - Print all registers");
        println!("  p <reg>             - Print specific register");
        println!("  set $<reg> <val>    - Set register value");
        println!();
        println!("Memory:");
        println!("  x <addr> [count]    - Examine memory");
        println!("  set *<addr> <data>  - Set memory value");
        println!();
        println!("Other:");
        println!("  disas <addr> [n]    - Disassemble instructions");
        println!("  backtrace, bt       - Show call stack");
        println!("  jump <addr>, j      - Jump to address");
        println!("  gdb [port]          - Start GDB server");
        println!("  info                - Show target info");
        println!("  help, h, ?          - Show this help");
        println!("  quit, q             - Quit debugger");
    }

    /// 继续执行
    fn cmd_continue<T: DebugTarget>(&mut self, target: &mut T) -> Result<(), DebugError> {
        let stop_reason = target.continue_execution()?;
        self.print_stop_reason(&stop_reason);
        Ok(())
    }

    /// 单步执行
    fn cmd_step<T: DebugTarget>(
        &mut self,
        target: &mut T,
        _instruction: bool,
    ) -> Result<(), DebugError> {
        let stop_reason = target.step()?;
        self.print_stop_reason(&stop_reason);

        // 显示当前 PC 和指令
        let pc = target.get_pc();
        println!("=> 0x{:016x}", pc);

        Ok(())
    }

    /// 打印停止原因
    fn print_stop_reason(&self, reason: &StopReason) {
        match reason {
            StopReason::Breakpoint(addr) => println!("Breakpoint hit at 0x{:016x}", addr),
            StopReason::WatchpointRead(addr) => println!("Read watchpoint at 0x{:016x}", addr),
            StopReason::WatchpointWrite(addr) => println!("Write watchpoint at 0x{:016x}", addr),
            StopReason::WatchpointAccess(addr) => println!("Access watchpoint at 0x{:016x}", addr),
            StopReason::StepDone => {} // 单步完成不打印
            StopReason::Exited(code) => println!("Program exited with code {}", code),
            StopReason::Signal(sig) => println!("Program received signal {}", sig),
            _ => println!("Stopped: {:?}", reason),
        }
    }

    /// 设置断点
    fn cmd_breakpoint(&mut self, addr: u64) -> Result<(), DebugError> {
        match self
            .breakpoint_manager
            .add_breakpoint(addr, BreakpointType::Software, 4)
        {
            Ok(bp) => {
                println!("Breakpoint set at 0x{:016x}", bp.address);
            }
            Err(e) => {
                println!("Failed to set breakpoint: {}", e);
            }
        }
        Ok(())
    }

    /// 删除断点
    fn cmd_delete_breakpoint(&mut self, num: usize) -> Result<(), DebugError> {
        let bps = self.breakpoint_manager.get_all_breakpoints();
        if num == 0 || num > bps.len() {
            println!("Invalid breakpoint number {}", num);
            return Ok(());
        }

        let bp = bps[num - 1];
        let addr = bp.address;
        let bp_type = bp.bp_type;

        match self.breakpoint_manager.remove_breakpoint(addr, bp_type) {
            Ok(_) => println!("Deleted breakpoint {}", num),
            Err(e) => println!("Failed to delete breakpoint: {}", e),
        }

        Ok(())
    }

    /// 列出断点
    fn cmd_list_breakpoints(&self) {
        let bps = self.breakpoint_manager.get_all_breakpoints();
        if bps.is_empty() {
            println!("No breakpoints.");
            return;
        }

        println!("Num  Type       Address          Enabled  Hits");
        for (i, bp) in bps.iter().enumerate() {
            let bp_type = match bp.bp_type {
                BreakpointType::Software => "software",
                BreakpointType::Hardware => "hardware",
                BreakpointType::Temporary => "temporary",
            };
            println!(
                "{:3}  {:10} 0x{:016x} {:8} {}",
                i + 1,
                bp_type,
                bp.address,
                if bp.enabled { "y" } else { "n" },
                bp.hit_count
            );
        }
    }

    /// 设置观察点
    fn cmd_watchpoint(
        &mut self,
        addr: u64,
        size: u64,
        wp_type: WatchpointType,
    ) -> Result<(), DebugError> {
        match self.watchpoint_manager.add_watchpoint(addr, wp_type, size) {
            Ok(_) => {
                let type_str = match wp_type {
                    WatchpointType::Read => "read",
                    WatchpointType::Write => "write",
                    WatchpointType::Access => "access",
                };
                println!(
                    "{} watchpoint set at 0x{:016x} (size: {})",
                    type_str, addr, size
                );
            }
            Err(e) => {
                println!("Failed to set watchpoint: {}", e);
            }
        }
        Ok(())
    }

    /// 列出观察点
    fn cmd_list_watchpoints(&self) {
        let wps = self.watchpoint_manager.get_all_watchpoints();
        if wps.is_empty() {
            println!("No watchpoints.");
            return;
        }

        println!("Num  Type       Address          Size  Enabled  Hits");
        for (i, wp) in wps.iter().enumerate() {
            let wp_type = match wp.wp_type {
                WatchpointType::Read => "read",
                WatchpointType::Write => "write",
                WatchpointType::Access => "access",
            };
            println!(
                "{:3}  {:10} 0x{:016x} {:4} {:8} {}",
                i + 1,
                wp_type,
                wp.address,
                wp.size,
                if wp.enabled { "y" } else { "n" },
                wp.hit_count
            );
        }
    }

    /// 删除观察点
    fn cmd_delete_watchpoint(&mut self, num: usize) -> Result<(), DebugError> {
        let wps = self.watchpoint_manager.get_all_watchpoints();
        if num == 0 || num > wps.len() {
            println!("Invalid watchpoint number {}", num);
            return Ok(());
        }

        let wp = wps[num - 1];
        let addr = wp.address;
        let wp_type = wp.wp_type;
        let size = wp.size;

        match self
            .watchpoint_manager
            .remove_watchpoint(addr, wp_type, size)
        {
            Ok(_) => println!("Deleted watchpoint {}", num),
            Err(e) => println!("Failed to delete watchpoint: {}", e),
        }

        Ok(())
    }

    /// 打印所有寄存器
    fn cmd_print_registers<T: DebugTarget>(&self, target: &T) -> Result<(), DebugError> {
        let regs = target.read_all_registers()?;

        // x0-x31
        println!("General Purpose Registers:");
        for i in 0..8 {
            let mut line = String::new();
            for j in 0..4 {
                let reg_num = i * 4 + j;
                if reg_num < 32 {
                    let offset = reg_num * 8;
                    let val = u64::from_le_bytes([
                        regs[offset],
                        regs[offset + 1],
                        regs[offset + 2],
                        regs[offset + 3],
                        regs[offset + 4],
                        regs[offset + 5],
                        regs[offset + 6],
                        regs[offset + 7],
                    ]);
                    line.push_str(&format!(" x{:2}=0x{:016x}", reg_num, val));
                }
            }
            println!("{}", line);
        }

        // PC
        let pc = target.get_pc();
        println!(" PC =0x{:016x}", pc);

        Ok(())
    }

    /// 打印单个寄存器
    fn cmd_print_register<T: DebugTarget>(&self, target: &T, reg: &str) -> Result<(), DebugError> {
        let reg_num = Self::parse_register_name(reg);

        match reg_num {
            Some(num) => {
                let val = target.read_register(num)?;
                println!("{} = 0x{:016x} ({})", reg, val, val);
            }
            None => {
                println!("Unknown register: {}", reg);
            }
        }

        Ok(())
    }

    /// 解析寄存器名称
    fn parse_register_name(name: &str) -> Option<u32> {
        let name = name.to_lowercase();

        // x0-x31
        if let Some(num) = name.strip_prefix('x') {
            return num.parse::<u32>().ok().filter(|&n| n < 32);
        }

        // ABI 名称
        let abi_map = [
            ("zero", 0),
            ("ra", 1),
            ("sp", 2),
            ("gp", 3),
            ("tp", 4),
            ("t0", 5),
            ("t1", 6),
            ("t2", 7),
            ("s0", 8),
            ("fp", 8),
            ("s1", 9),
            ("a0", 10),
            ("a1", 11),
            ("a2", 12),
            ("a3", 13),
            ("a4", 14),
            ("a5", 15),
            ("a6", 16),
            ("a7", 17),
            ("s2", 18),
            ("s3", 19),
            ("s4", 20),
            ("s5", 21),
            ("s6", 22),
            ("s7", 23),
            ("s8", 24),
            ("s9", 25),
            ("s10", 26),
            ("s11", 27),
            ("t3", 28),
            ("t4", 29),
            ("t5", 30),
            ("t6", 31),
            ("pc", 32),
        ];

        for (abi, num) in abi_map {
            if name == abi {
                return Some(num);
            }
        }

        None
    }

    /// 查看内存
    fn cmd_examine<T: DebugTarget>(
        &self,
        target: &T,
        addr: u64,
        count: usize,
    ) -> Result<(), DebugError> {
        let data = target.read_memory(addr, count)?;

        // 以十六进制和 ASCII 格式显示
        for (i, chunk) in data.chunks(16).enumerate() {
            let line_addr = addr + (i * 16) as u64;
            print!("0x{:016x}: ", line_addr);

            // 十六进制
            for byte in chunk {
                print!("{:02x} ", byte);
            }
            for _ in chunk.len()..16 {
                print!("   ");
            }

            print!(" ");

            // ASCII
            for byte in chunk {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    print!("{}", *byte as char);
                } else {
                    print!(".");
                }
            }

            println!();
        }

        Ok(())
    }

    /// 设置寄存器
    fn cmd_set_register<T: DebugTarget>(
        &mut self,
        target: &mut T,
        reg: &str,
        val: u64,
    ) -> Result<(), DebugError> {
        match Self::parse_register_name(reg) {
            Some(num) => {
                target.write_register(num, val)?;
                println!("Set {} = 0x{:016x}", reg, val);
            }
            None => {
                println!("Unknown register: {}", reg);
            }
        }

        Ok(())
    }

    /// 设置内存
    fn cmd_set_memory<T: DebugTarget>(
        &mut self,
        target: &mut T,
        addr: u64,
        data: &[u8],
    ) -> Result<(), DebugError> {
        target.write_memory(addr, data)?;
        println!("Wrote {} bytes to 0x{:016x}", data.len(), addr);
        Ok(())
    }

    /// 跳转
    fn cmd_jump<T: DebugTarget>(&mut self, target: &mut T, addr: u64) -> Result<(), DebugError> {
        target.set_pc(addr);
        println!("Jumped to 0x{:016x}", addr);
        Ok(())
    }

    /// 启动 GDB 服务器
    fn cmd_gdb_server<T: DebugTarget>(&mut self, port: u16, _target: &T) -> Result<(), DebugError> {
        if self.gdb_server.is_some() {
            println!("GDB server already running");
            return Ok(());
        }

        let config = GdbServerConfig::new().with_port(port);
        self.gdb_server = Some(GdbServer::new(config));

        println!("GDB server started on port {}", port);
        println!("Use 'target remote :{}' in GDB to connect", port);

        // 注意：实际启动服务器需要 target，这里只是准备配置
        println!("(Note: Server ready but not yet accepting connections in this mode)");

        Ok(())
    }

    /// 停止 GDB 服务器
    fn cmd_stop_gdb_server(&mut self) -> Result<(), DebugError> {
        if let Some(ref mut server) = self.gdb_server {
            server.stop();
            println!("GDB server stopped");
        } else {
            println!("GDB server not running");
        }
        self.gdb_server = None;
        Ok(())
    }

    /// 显示信息
    fn cmd_info<T: DebugTarget>(&self, target: &T, topic: &str) -> Result<(), DebugError> {
        match topic {
            "all" => {
                println!("Target: RISC-V 64-bit");
                println!("PC: 0x{:016x}", target.get_pc());
                self.cmd_list_breakpoints();
                self.cmd_list_watchpoints();
            }
            "target" => {
                println!("Target: RISC-V 64-bit");
                println!("PC: 0x{:016x}", target.get_pc());
            }
            _ => {
                println!("Unknown info topic: {}", topic);
            }
        }

        Ok(())
    }

    /// 添加命令到历史
    fn add_to_history(&mut self, cmd: &str) {
        self.history.push(cmd.to_string());
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// 获取命令历史
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// 获取断点管理器
    pub fn breakpoint_manager(&self) -> &BreakpointManager {
        &self.breakpoint_manager
    }

    /// 获取断点管理器的可变引用
    pub fn breakpoint_manager_mut(&mut self) -> &mut BreakpointManager {
        &mut self.breakpoint_manager
    }

    /// 获取观察点管理器
    pub fn watchpoint_manager(&self) -> &WatchpointManager {
        &self.watchpoint_manager
    }

    /// 获取观察点管理器的可变引用
    pub fn watchpoint_manager_mut(&mut self) -> &mut WatchpointManager {
        &mut self.watchpoint_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WatchpointAccess;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    /// 模拟调试目标
    struct MockTarget {
        registers: [AtomicU64; 66],
        memory: Mutex<Vec<u8>>,
        pc: AtomicU64,
        running: AtomicBool,
    }

    impl MockTarget {
        fn new() -> Self {
            let mut memory = vec![0; 0x10000];
            // 初始化一些测试数据
            for (i, byte) in memory.iter_mut().enumerate() {
                *byte = (i % 256) as u8;
            }

            Self {
                registers: std::array::from_fn(|i| AtomicU64::new(i as u64)),
                memory: Mutex::new(memory),
                pc: AtomicU64::new(0x8000_0000),
                running: AtomicBool::new(false),
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
            StopReason::Unknown
        }

        fn check_breakpoint(&self, _addr: u64) -> bool {
            false
        }

        fn check_watchpoint(&self, _addr: u64, _access_type: WatchpointAccess) -> bool {
            false
        }
    }

    #[test]
    fn test_command_parsing() {
        // 基本命令
        assert_eq!(DebugCommand::parse("help"), DebugCommand::Help);
        assert_eq!(DebugCommand::parse("h"), DebugCommand::Help);
        assert_eq!(DebugCommand::parse("?"), DebugCommand::Help);
        assert_eq!(DebugCommand::parse("run"), DebugCommand::Run);
        assert_eq!(DebugCommand::parse("r"), DebugCommand::Run);
        assert_eq!(DebugCommand::parse("continue"), DebugCommand::Continue);
        assert_eq!(DebugCommand::parse("c"), DebugCommand::Continue);
        assert_eq!(DebugCommand::parse("step"), DebugCommand::Step);
        assert_eq!(DebugCommand::parse("s"), DebugCommand::Step);
        assert_eq!(DebugCommand::parse("quit"), DebugCommand::Quit);
        assert_eq!(DebugCommand::parse("q"), DebugCommand::Quit);

        // 断点命令
        assert_eq!(
            DebugCommand::parse("break 0x1000"),
            DebugCommand::Break(0x1000)
        );
        assert_eq!(DebugCommand::parse("b 0x1000"), DebugCommand::Break(0x1000));

        // 检查空命令
        assert_eq!(DebugCommand::parse(""), DebugCommand::Empty);
        assert_eq!(DebugCommand::parse("   "), DebugCommand::Empty);
    }

    #[test]
    fn test_address_parsing() {
        // 十六进制地址
        assert_eq!(
            DebugCommand::parse("break 0x1000"),
            DebugCommand::Break(0x1000)
        );
        assert_eq!(
            DebugCommand::parse("break 0xABCDEF"),
            DebugCommand::Break(0xABCDEF)
        );

        // 十进制地址
        assert_eq!(DebugCommand::parse("break 4096"), DebugCommand::Break(4096));

        // 查看内存
        assert_eq!(
            DebugCommand::parse("x 0x1000 32"),
            DebugCommand::Examine(0x1000, 32)
        );
    }

    #[test]
    fn test_cli_creation() {
        let cli = DebugCli::new();
        assert!(cli.breakpoint_manager().get_all_breakpoints().is_empty());
        assert!(cli.watchpoint_manager().get_all_watchpoints().is_empty());
        assert!(cli.history().is_empty());
    }

    #[test]
    fn test_cli_breakpoint_commands() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        // 设置断点
        cli.execute_command(DebugCommand::Break(0x1000), &mut target)
            .unwrap();
        assert!(cli.breakpoint_manager().has_breakpoint(0x1000));

        // 列出断点
        cli.execute_command(DebugCommand::ListBreakpoints, &mut target)
            .unwrap();

        // 删除断点
        cli.execute_command(DebugCommand::DeleteBreakpoint(1), &mut target)
            .unwrap();
        assert!(!cli.breakpoint_manager().has_breakpoint(0x1000));
    }

    #[test]
    fn test_cli_watchpoint_commands() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        // 设置观察点
        cli.execute_command(DebugCommand::WatchWrite(0x2000, 4), &mut target)
            .unwrap();
        assert!(cli.watchpoint_manager().has_watchpoint(0x2000));

        // 列出观察点
        cli.execute_command(DebugCommand::ListWatchpoints, &mut target)
            .unwrap();

        // 删除观察点
        cli.execute_command(DebugCommand::DeleteWatchpoint(1), &mut target)
            .unwrap();
        assert!(!cli.watchpoint_manager().has_watchpoint(0x2000));
    }

    #[test]
    fn test_cli_register_commands() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        // 打印所有寄存器
        cli.execute_command(DebugCommand::PrintRegisters, &mut target)
            .unwrap();

        // 设置寄存器
        cli.execute_command(
            DebugCommand::SetRegister("x1".to_string(), 0x1234),
            &mut target,
        )
        .unwrap();

        // 打印单个寄存器
        cli.execute_command(DebugCommand::PrintRegister("x1".to_string()), &mut target)
            .unwrap();
    }

    #[test]
    fn test_cli_memory_commands() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        // 查看内存
        cli.execute_command(DebugCommand::Examine(0x1000, 16), &mut target)
            .unwrap();

        // 设置内存
        cli.execute_command(
            DebugCommand::SetMemory(0x1000, vec![0x12, 0x34, 0x56, 0x78]),
            &mut target,
        )
        .unwrap();
    }

    #[test]
    fn test_cli_step_continue() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        let initial_pc = target.get_pc();

        // 单步
        cli.execute_command(DebugCommand::Step, &mut target)
            .unwrap();
        assert_eq!(target.get_pc(), initial_pc + 4);

        // 继续
        cli.execute_command(DebugCommand::Continue, &mut target)
            .unwrap();
    }

    #[test]
    fn test_cli_jump() {
        let mut cli = DebugCli::new();
        let mut target = MockTarget::new();

        cli.execute_command(DebugCommand::Jump(0x8000_1000), &mut target)
            .unwrap();
        assert_eq!(target.get_pc(), 0x8000_1000);
    }

    #[test]
    fn test_register_name_parsing() {
        // x 寄存器
        assert_eq!(DebugCli::parse_register_name("x0"), Some(0));
        assert_eq!(DebugCli::parse_register_name("x31"), Some(31));
        assert_eq!(DebugCli::parse_register_name("X0"), Some(0));

        // ABI 名称
        assert_eq!(DebugCli::parse_register_name("zero"), Some(0));
        assert_eq!(DebugCli::parse_register_name("ra"), Some(1));
        assert_eq!(DebugCli::parse_register_name("sp"), Some(2));
        assert_eq!(DebugCli::parse_register_name("gp"), Some(3));
        assert_eq!(DebugCli::parse_register_name("tp"), Some(4));
        assert_eq!(DebugCli::parse_register_name("fp"), Some(8));
        assert_eq!(DebugCli::parse_register_name("a0"), Some(10));
        assert_eq!(DebugCli::parse_register_name("s0"), Some(8));
        assert_eq!(DebugCli::parse_register_name("pc"), Some(32));

        // 无效寄存器
        assert_eq!(DebugCli::parse_register_name("x32"), None);
        assert_eq!(DebugCli::parse_register_name("invalid"), None);
    }

    #[test]
    fn test_cli_history() {
        let mut cli = DebugCli::new().with_max_history(3);

        cli.add_to_history("step");
        cli.add_to_history("continue");
        cli.add_to_history("break 0x1000");

        assert_eq!(cli.history().len(), 3);

        // 超过限制时应该移除最旧的
        cli.add_to_history("info registers");
        assert_eq!(cli.history().len(), 3);
        assert_eq!(cli.history()[0], "continue");
    }
}
