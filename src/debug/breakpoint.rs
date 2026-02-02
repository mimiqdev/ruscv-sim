//! 断点管理模块
//!
//! 支持软件断点、硬件断点、条件断点等。

use super::DebugError;
use std::collections::HashMap;

/// 断点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakpointType {
    /// 软件断点（通过替换指令实现）
    Software,
    /// 硬件断点（使用调试寄存器）
    Hardware,
    /// 临时断点（用于单步后的继续执行）
    Temporary,
}

impl BreakpointType {
    /// 从 GDB Z 命令类型代码转换
    pub fn from_gdb_type(t: u8) -> Option<Self> {
        match t {
            0 => Some(BreakpointType::Software),
            1 => Some(BreakpointType::Hardware),
            _ => None,
        }
    }

    /// 转换为 GDB Z 命令类型代码
    pub fn to_gdb_type(self) -> u8 {
        match self {
            BreakpointType::Software => 0,
            BreakpointType::Hardware => 1,
            BreakpointType::Temporary => 0, // 临时断点使用软件断点代码（GDB 协议中无专门的临时断点类型）
        }
    }

    /// 获取硬件断点计数（用于硬件限制检查）
    /// 注意：Temporary 类型返回 0，因为它不占用硬件断点资源
    pub fn hardware_count(&self) -> usize {
        match self {
            BreakpointType::Software => 0,
            BreakpointType::Hardware => 1,
            BreakpointType::Temporary => 0,
        }
    }
}

/// 断点结构
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// 断点地址
    pub address: u64,
    /// 断点类型
    pub bp_type: BreakpointType,
    /// 断点大小（通常为 4 字节）
    pub size: u64,
    /// 原始指令（用于软件断点）
    pub original_instruction: Option<u32>,
    /// 命中次数
    pub hit_count: u64,
    /// 是否启用
    pub enabled: bool,
    /// 条件表达式（可选）
    pub condition: Option<String>,
}

impl Breakpoint {
    /// 创建新的断点
    pub fn new(address: u64, bp_type: BreakpointType, size: u64) -> Self {
        Self {
            address,
            bp_type,
            size,
            original_instruction: None,
            hit_count: 0,
            enabled: true,
            condition: None,
        }
    }

    /// 创建软件断点
    pub fn software(address: u64) -> Self {
        Self::new(address, BreakpointType::Software, 4)
    }

    /// 创建硬件断点
    pub fn hardware(address: u64) -> Self {
        Self::new(address, BreakpointType::Hardware, 4)
    }

    /// 创建临时断点
    pub fn temporary(address: u64) -> Self {
        let mut bp = Self::new(address, BreakpointType::Temporary, 4);
        bp.bp_type = BreakpointType::Temporary;
        bp
    }

    /// 设置原始指令
    pub fn set_original_instruction(&mut self, instr: u32) {
        self.original_instruction = Some(instr);
    }

    /// 增加命中计数
    pub fn hit(&mut self) {
        self.hit_count += 1;
    }

    /// 检查是否为临时断点
    pub fn is_temporary(&self) -> bool {
        matches!(self.bp_type, BreakpointType::Temporary)
    }
}

/// 断点管理器
pub struct BreakpointManager {
    /// 软件断点表：地址 -> 断点
    software_breakpoints: HashMap<u64, Breakpoint>,
    /// 硬件断点表：地址 -> 断点
    hardware_breakpoints: HashMap<u64, Breakpoint>,
    /// 临时断点表：地址 -> 断点
    temporary_breakpoints: HashMap<u64, Breakpoint>,
    /// 最大硬件断点数（RISC-V 通常支持 4-16 个）
    max_hardware_breakpoints: usize,
    /// 当前硬件断点数
    current_hardware_count: usize,
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BreakpointManager {
    /// 创建新的断点管理器
    pub fn new() -> Self {
        Self {
            software_breakpoints: HashMap::new(),
            hardware_breakpoints: HashMap::new(),
            temporary_breakpoints: HashMap::new(),
            max_hardware_breakpoints: 4, // 保守值
            current_hardware_count: 0,
        }
    }

    /// 创建并设置最大硬件断点数
    pub fn with_hardware_limit(mut self, limit: usize) -> Self {
        self.max_hardware_breakpoints = limit;
        self
    }

    /// 添加断点
    pub fn add_breakpoint(
        &mut self,
        address: u64,
        bp_type: BreakpointType,
        size: u64,
    ) -> Result<&Breakpoint, DebugError> {
        match bp_type {
            BreakpointType::Software => {
                let bp = Breakpoint::new(address, bp_type, size);
                self.software_breakpoints.insert(address, bp);
                Ok(self.software_breakpoints.get(&address).unwrap())
            }
            BreakpointType::Hardware => {
                if self.current_hardware_count >= self.max_hardware_breakpoints {
                    return Err(DebugError::InvalidPacket(
                        "Maximum hardware breakpoint count reached".into(),
                    ));
                }
                let bp = Breakpoint::new(address, bp_type, size);
                self.hardware_breakpoints.insert(address, bp);
                self.current_hardware_count += 1;
                Ok(self.hardware_breakpoints.get(&address).unwrap())
            }
            BreakpointType::Temporary => {
                let bp = Breakpoint::temporary(address);
                self.temporary_breakpoints.insert(address, bp);
                Ok(self.temporary_breakpoints.get(&address).unwrap())
            }
        }
    }

    /// 移除断点
    pub fn remove_breakpoint(
        &mut self,
        address: u64,
        bp_type: BreakpointType,
    ) -> Result<Breakpoint, DebugError> {
        match bp_type {
            BreakpointType::Software => self
                .software_breakpoints
                .remove(&address)
                .ok_or(DebugError::BreakpointNotFound(address)),
            BreakpointType::Hardware => {
                let bp = self
                    .hardware_breakpoints
                    .remove(&address)
                    .ok_or(DebugError::BreakpointNotFound(address))?;
                self.current_hardware_count -= 1;
                Ok(bp)
            }
            BreakpointType::Temporary => self
                .temporary_breakpoints
                .remove(&address)
                .ok_or(DebugError::BreakpointNotFound(address)),
        }
    }

    /// 检查指定地址是否有断点
    pub fn has_breakpoint(&self, address: u64) -> bool {
        self.software_breakpoints.contains_key(&address)
            || self.hardware_breakpoints.contains_key(&address)
            || self.temporary_breakpoints.contains_key(&address)
    }

    /// 获取断点
    pub fn get_breakpoint(&self, address: u64, bp_type: BreakpointType) -> Option<&Breakpoint> {
        match bp_type {
            BreakpointType::Software => self.software_breakpoints.get(&address),
            BreakpointType::Hardware => self.hardware_breakpoints.get(&address),
            BreakpointType::Temporary => self.temporary_breakpoints.get(&address),
        }
    }

    /// 获取可变的断点
    pub fn get_breakpoint_mut(
        &mut self,
        address: u64,
        bp_type: BreakpointType,
    ) -> Option<&mut Breakpoint> {
        match bp_type {
            BreakpointType::Software => self.software_breakpoints.get_mut(&address),
            BreakpointType::Hardware => self.hardware_breakpoints.get_mut(&address),
            BreakpointType::Temporary => self.temporary_breakpoints.get_mut(&address),
        }
    }

    /// 标记断点命中
    pub fn hit_breakpoint(&mut self, address: u64) -> bool {
        // 检查所有类型的断点
        for table in [
            &mut self.software_breakpoints,
            &mut self.hardware_breakpoints,
            &mut self.temporary_breakpoints,
        ] {
            if let Some(bp) = table.get_mut(&address) {
                if bp.enabled {
                    bp.hit();
                    return true;
                }
            }
        }
        false
    }

    /// 获取所有断点
    pub fn get_all_breakpoints(&self) -> Vec<&Breakpoint> {
        let mut result = Vec::new();
        result.extend(self.software_breakpoints.values());
        result.extend(self.hardware_breakpoints.values());
        result.extend(self.temporary_breakpoints.values());
        result
    }

    /// 获取所有软件断点地址
    pub fn get_software_breakpoint_addresses(&self) -> Vec<u64> {
        self.software_breakpoints.keys().copied().collect()
    }

    /// 获取所有硬件断点地址
    pub fn get_hardware_breakpoint_addresses(&self) -> Vec<u64> {
        self.hardware_breakpoints.keys().copied().collect()
    }

    /// 获取所有临时断点地址
    pub fn get_temporary_breakpoint_addresses(&self) -> Vec<u64> {
        self.temporary_breakpoints.keys().copied().collect()
    }

    /// 清除所有断点
    pub fn clear_all(&mut self) {
        self.software_breakpoints.clear();
        self.hardware_breakpoints.clear();
        self.temporary_breakpoints.clear();
        self.current_hardware_count = 0;
    }

    /// 清除所有临时断点
    pub fn clear_temporary(&mut self) {
        self.temporary_breakpoints.clear();
    }

    /// 启用/禁用断点
    pub fn set_breakpoint_enabled(
        &mut self,
        address: u64,
        bp_type: BreakpointType,
        enabled: bool,
    ) -> Result<(), DebugError> {
        if let Some(bp) = self.get_breakpoint_mut(address, bp_type) {
            bp.enabled = enabled;
            Ok(())
        } else {
            Err(DebugError::BreakpointNotFound(address))
        }
    }

    /// 设置断点条件
    pub fn set_breakpoint_condition(
        &mut self,
        address: u64,
        bp_type: BreakpointType,
        condition: String,
    ) -> Result<(), DebugError> {
        if let Some(bp) = self.get_breakpoint_mut(address, bp_type) {
            bp.condition = Some(condition);
            Ok(())
        } else {
            Err(DebugError::BreakpointNotFound(address))
        }
    }

    /// 获取断点统计信息
    pub fn get_stats(&self) -> BreakpointStats {
        BreakpointStats {
            software_count: self.software_breakpoints.len(),
            hardware_count: self.hardware_breakpoints.len(),
            temporary_count: self.temporary_breakpoints.len(),
            hardware_limit: self.max_hardware_breakpoints,
            total_hits: self
                .get_all_breakpoints()
                .iter()
                .map(|bp| bp.hit_count)
                .sum(),
        }
    }
}

/// 断点统计信息
#[derive(Debug, Clone)]
pub struct BreakpointStats {
    pub software_count: usize,
    pub hardware_count: usize,
    pub temporary_count: usize,
    pub hardware_limit: usize,
    pub total_hits: u64,
}

impl BreakpointStats {
    /// 格式化为字符串
    pub fn format(&self) -> String {
        format!(
            "Breakpoints: {} software, {}/{} hardware, {} temporary, {} total hits",
            self.software_count,
            self.hardware_count,
            self.hardware_limit,
            self.temporary_count,
            self.total_hits
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_creation() {
        let bp = Breakpoint::software(0x1000);
        assert_eq!(bp.address, 0x1000);
        assert_eq!(bp.bp_type, BreakpointType::Software);
        assert!(bp.enabled);
        assert_eq!(bp.hit_count, 0);

        let bp = Breakpoint::hardware(0x2000);
        assert_eq!(bp.bp_type, BreakpointType::Hardware);

        let bp = Breakpoint::temporary(0x3000);
        assert!(bp.is_temporary());
    }

    #[test]
    fn test_add_remove_breakpoint() {
        let mut mgr = BreakpointManager::new();

        // 添加软件断点
        let bp = mgr
            .add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();
        assert_eq!(bp.address, 0x1000);
        assert!(mgr.has_breakpoint(0x1000));

        // 移除软件断点
        let removed = mgr
            .remove_breakpoint(0x1000, BreakpointType::Software)
            .unwrap();
        assert_eq!(removed.address, 0x1000);
        assert!(!mgr.has_breakpoint(0x1000));

        // 移除不存在的断点
        assert!(mgr
            .remove_breakpoint(0x1000, BreakpointType::Software)
            .is_err());
    }

    #[test]
    fn test_hardware_breakpoint_limit() {
        let mut mgr = BreakpointManager::new().with_hardware_limit(2);

        assert!(mgr
            .add_breakpoint(0x1000, BreakpointType::Hardware, 4)
            .is_ok());
        assert!(mgr
            .add_breakpoint(0x2000, BreakpointType::Hardware, 4)
            .is_ok());
        assert!(mgr
            .add_breakpoint(0x3000, BreakpointType::Hardware, 4)
            .is_err()); // 超出限制

        // 移除后可以添加新的
        mgr.remove_breakpoint(0x1000, BreakpointType::Hardware)
            .unwrap();
        assert!(mgr
            .add_breakpoint(0x3000, BreakpointType::Hardware, 4)
            .is_ok());
    }

    #[test]
    fn test_breakpoint_hit() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();

        assert!(mgr.hit_breakpoint(0x1000));
        assert!(!mgr.hit_breakpoint(0x2000)); // 未命中

        let bp = mgr
            .get_breakpoint(0x1000, BreakpointType::Software)
            .unwrap();
        assert_eq!(bp.hit_count, 1);
    }

    #[test]
    fn test_breakpoint_enable_disable() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();

        // 禁用断点
        mgr.set_breakpoint_enabled(0x1000, BreakpointType::Software, false)
            .unwrap();
        assert!(!mgr.hit_breakpoint(0x1000)); // 禁用时不会命中

        // 启用断点
        mgr.set_breakpoint_enabled(0x1000, BreakpointType::Software, true)
            .unwrap();
        assert!(mgr.hit_breakpoint(0x1000));
    }

    #[test]
    fn test_get_all_breakpoints() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();
        mgr.add_breakpoint(0x2000, BreakpointType::Hardware, 4)
            .unwrap();
        mgr.add_breakpoint(0x3000, BreakpointType::Temporary, 4)
            .unwrap();

        let all = mgr.get_all_breakpoints();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_clear_all() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();
        mgr.add_breakpoint(0x2000, BreakpointType::Hardware, 4)
            .unwrap();

        mgr.clear_all();
        assert!(!mgr.has_breakpoint(0x1000));
        assert!(!mgr.has_breakpoint(0x2000));
        assert_eq!(mgr.get_stats().hardware_count, 0);
    }

    #[test]
    fn test_clear_temporary() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();
        mgr.add_breakpoint(0x2000, BreakpointType::Temporary, 4)
            .unwrap();

        mgr.clear_temporary();
        assert!(mgr.has_breakpoint(0x1000)); // 软件断点保留
        assert!(!mgr.has_breakpoint(0x2000)); // 临时断点被清除
    }

    #[test]
    fn test_breakpoint_condition() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();

        mgr.set_breakpoint_condition(0x1000, BreakpointType::Software, "x1 == 42".to_string())
            .unwrap();

        let bp = mgr
            .get_breakpoint(0x1000, BreakpointType::Software)
            .unwrap();
        assert_eq!(bp.condition, Some("x1 == 42".to_string()));
    }

    #[test]
    fn test_breakpoint_stats() {
        let mut mgr = BreakpointManager::new();
        mgr.add_breakpoint(0x1000, BreakpointType::Software, 4)
            .unwrap();
        mgr.add_breakpoint(0x2000, BreakpointType::Hardware, 4)
            .unwrap();

        mgr.hit_breakpoint(0x1000);
        mgr.hit_breakpoint(0x1000);
        mgr.hit_breakpoint(0x2000);

        let stats = mgr.get_stats();
        assert_eq!(stats.software_count, 1);
        assert_eq!(stats.hardware_count, 1);
        assert_eq!(stats.total_hits, 3);
    }

    #[test]
    fn test_breakpoint_type_conversion() {
        assert_eq!(
            BreakpointType::from_gdb_type(0),
            Some(BreakpointType::Software)
        );
        assert_eq!(
            BreakpointType::from_gdb_type(1),
            Some(BreakpointType::Hardware)
        );
        assert_eq!(BreakpointType::from_gdb_type(99), None);

        assert_eq!(BreakpointType::Software.to_gdb_type(), 0);
        assert_eq!(BreakpointType::Hardware.to_gdb_type(), 1);
        assert_eq!(BreakpointType::Temporary.to_gdb_type(), 0);
    }
}
