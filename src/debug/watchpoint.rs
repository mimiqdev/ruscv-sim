//! 观察点管理模块
//!
//! 支持读观察点、写观察点和访问观察点。

use super::{DebugError, WatchpointAccess};
use std::collections::HashMap;

/// 观察点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WatchpointType {
    /// 读观察点（数据被读取时触发）
    Read,
    /// 写观察点（数据被写入时触发）
    Write,
    /// 访问观察点（读或写时触发）
    Access,
}

impl WatchpointType {
    /// 从 GDB Z 命令类型代码转换
    pub fn from_gdb_type(t: u8) -> Option<Self> {
        match t {
            2 => Some(WatchpointType::Read),
            3 => Some(WatchpointType::Write),
            4 => Some(WatchpointType::Access),
            _ => None,
        }
    }

    /// 转换为 GDB Z 命令类型代码
    pub fn to_gdb_type(self) -> u8 {
        match self {
            WatchpointType::Read => 2,
            WatchpointType::Write => 3,
            WatchpointType::Access => 4,
        }
    }

    /// 检查给定的访问类型是否匹配此观察点类型
    pub fn matches_access(self, access: WatchpointAccess) -> bool {
        match (self, access) {
            (WatchpointType::Read, WatchpointAccess::Read) => true,
            (WatchpointType::Read, WatchpointAccess::ReadWrite) => true,
            (WatchpointType::Write, WatchpointAccess::Write) => true,
            (WatchpointType::Write, WatchpointAccess::ReadWrite) => true,
            (WatchpointType::Access, _) => true,
            _ => false,
        }
    }
}

/// 观察点结构
#[derive(Debug, Clone)]
pub struct Watchpoint {
    /// 观察点地址
    pub address: u64,
    /// 观察点类型
    pub wp_type: WatchpointType,
    /// 观察数据大小（字节）
    pub size: u64,
    /// 命中次数
    pub hit_count: u64,
    /// 是否启用
    pub enabled: bool,
    /// 条件表达式（可选）
    pub condition: Option<String>,
}

impl Watchpoint {
    /// 创建新的观察点
    pub fn new(address: u64, wp_type: WatchpointType, size: u64) -> Self {
        Self {
            address,
            wp_type,
            size,
            hit_count: 0,
            enabled: true,
            condition: None,
        }
    }

    /// 创建读观察点
    pub fn read(address: u64, size: u64) -> Self {
        Self::new(address, WatchpointType::Read, size)
    }

    /// 创建写观察点
    pub fn write(address: u64, size: u64) -> Self {
        Self::new(address, WatchpointType::Write, size)
    }

    /// 创建访问观察点
    pub fn access(address: u64, size: u64) -> Self {
        Self::new(address, WatchpointType::Access, size)
    }

    /// 增加命中计数
    pub fn hit(&mut self) {
        self.hit_count += 1;
    }

    /// 检查地址是否在此观察点范围内
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.address && addr < self.address + self.size
    }

    /// 检查给定的访问是否会触发此观察点
    pub fn check_access(&self, addr: u64, len: u64, access: WatchpointAccess) -> bool {
        if !self.enabled {
            return false;
        }

        // 检查访问范围是否与观察点范围重叠
        let access_end = addr + len;
        let wp_end = self.address + self.size;
        let overlaps = addr < wp_end && access_end > self.address;

        if !overlaps {
            return false;
        }

        // 检查访问类型是否匹配
        self.wp_type.matches_access(access)
    }
}

/// 观察点管理器
pub struct WatchpointManager {
    /// 观察点表：地址 -> 观察点列表（同一个地址可能有多个不同类型/大小的观察点）
    watchpoints: HashMap<u64, Vec<Watchpoint>>,
    /// 最大观察点数
    max_watchpoints: usize,
}

impl Default for WatchpointManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchpointManager {
    /// 创建新的观察点管理器
    pub fn new() -> Self {
        Self {
            watchpoints: HashMap::new(),
            max_watchpoints: 8, // 保守值
        }
    }

    /// 设置最大观察点数
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.max_watchpoints = limit;
        self
    }

    /// 获取当前观察点数量
    pub fn count(&self) -> usize {
        self.watchpoints.values().map(|v| v.len()).sum()
    }

    /// 检查是否还可以添加观察点
    pub fn can_add(&self) -> bool {
        self.count() < self.max_watchpoints
    }

    /// 添加观察点
    pub fn add_watchpoint(
        &mut self,
        address: u64,
        wp_type: WatchpointType,
        size: u64,
    ) -> Result<(), DebugError> {
        if !self.can_add() {
            return Err(DebugError::InvalidPacket(
                "Maximum watchpoint count reached".into(),
            ));
        }

        let wp = Watchpoint::new(address, wp_type, size);
        self.watchpoints
            .entry(address)
            .or_default()
            .push(wp);

        Ok(())
    }

    /// 移除观察点
    pub fn remove_watchpoint(
        &mut self,
        address: u64,
        wp_type: WatchpointType,
        size: u64,
    ) -> Result<(), DebugError> {
        if let Some(wps) = self.watchpoints.get_mut(&address) {
            let initial_len = wps.len();
            wps.retain(|wp| wp.wp_type != wp_type || wp.size != size);

            if wps.len() == initial_len {
                return Err(DebugError::WatchpointNotFound(address));
            }

            if wps.is_empty() {
                self.watchpoints.remove(&address);
            }

            Ok(())
        } else {
            Err(DebugError::WatchpointNotFound(address))
        }
    }

    /// 检查指定地址是否有观察点
    pub fn has_watchpoint(&self, address: u64) -> bool {
        self.watchpoints.contains_key(&address)
    }

    /// 获取指定地址的所有观察点
    pub fn get_watchpoints(&self, address: u64) -> Option<&Vec<Watchpoint>> {
        self.watchpoints.get(&address)
    }

    /// 获取所有观察点
    pub fn get_all_watchpoints(&self) -> Vec<&Watchpoint> {
        self.watchpoints.values().flatten().collect()
    }

    /// 检查内存访问是否触发观察点
    /// 返回触发观察点的地址列表
    pub fn check_memory_access(
        &self,
        addr: u64,
        len: u64,
        access: WatchpointAccess,
    ) -> Vec<u64> {
        let mut triggered = Vec::new();

        for (wp_addr, wps) in &self.watchpoints {
            for wp in wps {
                if wp.check_access(addr, len, access) {
                    triggered.push(*wp_addr);
                    break; // 同一个地址只报告一次
                }
            }
        }

        triggered
    }

    /// 标记观察点命中并返回命中的观察点地址列表
    pub fn hit_watchpoints(
        &mut self,
        addr: u64,
        len: u64,
        access: WatchpointAccess,
    ) -> Vec<u64> {
        let mut hit_addresses = Vec::new();

        for (wp_addr, wps) in &mut self.watchpoints {
            for wp in wps {
                if wp.check_access(addr, len, access) {
                    wp.hit();
                    hit_addresses.push(*wp_addr);
                    break;
                }
            }
        }

        hit_addresses
    }

    /// 清除所有观察点
    pub fn clear_all(&mut self) {
        self.watchpoints.clear();
    }

    /// 启用/禁用观察点
    pub fn set_watchpoint_enabled(
        &mut self,
        address: u64,
        wp_type: WatchpointType,
        enabled: bool,
    ) -> Result<(), DebugError> {
        if let Some(wps) = self.watchpoints.get_mut(&address) {
            for wp in wps {
                if wp.wp_type == wp_type {
                    wp.enabled = enabled;
                    return Ok(());
                }
            }
        }
        Err(DebugError::WatchpointNotFound(address))
    }

    /// 设置观察点条件
    pub fn set_watchpoint_condition(
        &mut self,
        address: u64,
        wp_type: WatchpointType,
        condition: String,
    ) -> Result<(), DebugError> {
        if let Some(wps) = self.watchpoints.get_mut(&address) {
            for wp in wps {
                if wp.wp_type == wp_type {
                    wp.condition = Some(condition);
                    return Ok(());
                }
            }
        }
        Err(DebugError::WatchpointNotFound(address))
    }

    /// 获取观察点统计信息
    pub fn get_stats(&self) -> WatchpointStats {
        let all_wps: Vec<_> = self.get_all_watchpoints();
        let read_count = all_wps
            .iter()
            .filter(|wp| matches!(wp.wp_type, WatchpointType::Read))
            .count();
        let write_count = all_wps
            .iter()
            .filter(|wp| matches!(wp.wp_type, WatchpointType::Write))
            .count();
        let access_count = all_wps
            .iter()
            .filter(|wp| matches!(wp.wp_type, WatchpointType::Access))
            .count();

        WatchpointStats {
            read_count,
            write_count,
            access_count,
            total_count: all_wps.len(),
            limit: self.max_watchpoints,
            total_hits: all_wps.iter().map(|wp| wp.hit_count).sum(),
        }
    }
}

/// 观察点统计信息
#[derive(Debug, Clone)]
pub struct WatchpointStats {
    pub read_count: usize,
    pub write_count: usize,
    pub access_count: usize,
    pub total_count: usize,
    pub limit: usize,
    pub total_hits: u64,
}

impl WatchpointStats {
    /// 格式化为字符串
    pub fn format(&self) -> String {
        format!(
            "Watchpoints: {} read, {} write, {} access ({}/{}), {} total hits",
            self.read_count, self.write_count, self.access_count, self.total_count, self.limit, self.total_hits
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchpoint_creation() {
        let wp = Watchpoint::read(0x1000, 4);
        assert_eq!(wp.address, 0x1000);
        assert_eq!(wp.wp_type, WatchpointType::Read);
        assert_eq!(wp.size, 4);
        assert!(wp.enabled);

        let wp = Watchpoint::write(0x2000, 8);
        assert_eq!(wp.wp_type, WatchpointType::Write);
        assert_eq!(wp.size, 8);

        let wp = Watchpoint::access(0x3000, 4);
        assert_eq!(wp.wp_type, WatchpointType::Access);
    }

    #[test]
    fn test_watchpoint_contains() {
        let wp = Watchpoint::write(0x1000, 4);

        assert!(wp.contains(0x1000));
        assert!(wp.contains(0x1001));
        assert!(wp.contains(0x1002));
        assert!(wp.contains(0x1003));
        assert!(!wp.contains(0x1004));
        assert!(!wp.contains(0x0FFF));
    }

    #[test]
    fn test_watchpoint_check_access() {
        let wp_write = Watchpoint::write(0x1000, 4);

        // 写访问应该匹配
        assert!(wp_write.check_access(0x1000, 4, WatchpointAccess::Write));
        assert!(wp_write.check_access(0x1001, 2, WatchpointAccess::Write));

        // 读访问不应该匹配
        assert!(!wp_write.check_access(0x1000, 4, WatchpointAccess::Read));

        let wp_read = Watchpoint::read(0x1000, 4);
        assert!(wp_read.check_access(0x1000, 4, WatchpointAccess::Read));
        assert!(!wp_read.check_access(0x1000, 4, WatchpointAccess::Write));

        let wp_access = Watchpoint::access(0x1000, 4);
        assert!(wp_access.check_access(0x1000, 4, WatchpointAccess::Read));
        assert!(wp_access.check_access(0x1000, 4, WatchpointAccess::Write));
        assert!(wp_access.check_access(0x1000, 4, WatchpointAccess::ReadWrite));
    }

    #[test]
    fn test_add_remove_watchpoint() {
        let mut mgr = WatchpointManager::new();

        // 添加观察点
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();
        assert!(mgr.has_watchpoint(0x1000));
        assert_eq!(mgr.count(), 1);

        // 移除观察点
        mgr.remove_watchpoint(0x1000, WatchpointType::Write, 4)
            .unwrap();
        assert!(!mgr.has_watchpoint(0x1000));
        assert_eq!(mgr.count(), 0);

        // 移除不存在的观察点
        assert!(mgr
            .remove_watchpoint(0x1000, WatchpointType::Write, 4)
            .is_err());
    }

    #[test]
    fn test_watchpoint_limit() {
        let mut mgr = WatchpointManager::new().with_limit(2);

        assert!(mgr
            .add_watchpoint(0x1000, WatchpointType::Write, 4)
            .is_ok());
        assert!(mgr
            .add_watchpoint(0x2000, WatchpointType::Write, 4)
            .is_ok());
        assert!(mgr
            .add_watchpoint(0x3000, WatchpointType::Write, 4)
            .is_err()); // 超出限制
    }

    #[test]
    fn test_check_memory_access() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();
        mgr.add_watchpoint(0x2000, WatchpointType::Read, 8).unwrap();

        // 应该触发写观察点
        let triggered = mgr.check_memory_access(0x1000, 4, WatchpointAccess::Write);
        assert_eq!(triggered, vec![0x1000]);

        // 不应该触发（读访问）
        let triggered = mgr.check_memory_access(0x1000, 4, WatchpointAccess::Read);
        assert!(triggered.is_empty());

        // 应该触发读观察点
        let triggered = mgr.check_memory_access(0x2000, 4, WatchpointAccess::Read);
        assert_eq!(triggered, vec![0x2000]);

        // 部分重叠也应该触发
        let triggered = mgr.check_memory_access(0x1002, 4, WatchpointAccess::Write);
        assert_eq!(triggered, vec![0x1000]);
    }

    #[test]
    fn test_hit_watchpoints() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();

        let hit = mgr.hit_watchpoints(0x1000, 4, WatchpointAccess::Write);
        assert_eq!(hit, vec![0x1000]);

        // 检查命中计数
        let wps = mgr.get_watchpoints(0x1000).unwrap();
        assert_eq!(wps[0].hit_count, 1);
    }

    #[test]
    fn test_multiple_watchpoints_same_address() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Read, 4).unwrap();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();

        assert_eq!(mgr.count(), 2);

        // 获取所有观察点
        let wps = mgr.get_watchpoints(0x1000).unwrap();
        assert_eq!(wps.len(), 2);
    }

    #[test]
    fn test_clear_all() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();
        mgr.add_watchpoint(0x2000, WatchpointType::Read, 4).unwrap();

        mgr.clear_all();
        assert_eq!(mgr.count(), 0);
        assert!(!mgr.has_watchpoint(0x1000));
    }

    #[test]
    fn test_watchpoint_enable_disable() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();

        // 禁用
        mgr.set_watchpoint_enabled(0x1000, WatchpointType::Write, false)
            .unwrap();
        let triggered = mgr.check_memory_access(0x1000, 4, WatchpointAccess::Write);
        assert!(triggered.is_empty());

        // 启用
        mgr.set_watchpoint_enabled(0x1000, WatchpointType::Write, true)
            .unwrap();
        let triggered = mgr.check_memory_access(0x1000, 4, WatchpointAccess::Write);
        assert_eq!(triggered, vec![0x1000]);
    }

    #[test]
    fn test_watchpoint_condition() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Write, 4).unwrap();

        mgr.set_watchpoint_condition(
            0x1000,
            WatchpointType::Write,
            "x1 == 42".to_string(),
        )
        .unwrap();

        let wps = mgr.get_watchpoints(0x1000).unwrap();
        assert_eq!(wps[0].condition, Some("x1 == 42".to_string()));
    }

    #[test]
    fn test_watchpoint_stats() {
        let mut mgr = WatchpointManager::new();
        mgr.add_watchpoint(0x1000, WatchpointType::Read, 4).unwrap();
        mgr.add_watchpoint(0x2000, WatchpointType::Write, 4).unwrap();
        mgr.add_watchpoint(0x3000, WatchpointType::Access, 4).unwrap();

        mgr.hit_watchpoints(0x1000, 4, WatchpointAccess::Read);
        mgr.hit_watchpoints(0x2000, 4, WatchpointAccess::Write);
        mgr.hit_watchpoints(0x2000, 4, WatchpointAccess::Write);

        let stats = mgr.get_stats();
        assert_eq!(stats.read_count, 1);
        assert_eq!(stats.write_count, 1);
        assert_eq!(stats.access_count, 1);
        assert_eq!(stats.total_hits, 3);
    }

    #[test]
    fn test_watchpoint_type_conversion() {
        assert_eq!(WatchpointType::from_gdb_type(2), Some(WatchpointType::Read));
        assert_eq!(WatchpointType::from_gdb_type(3), Some(WatchpointType::Write));
        assert_eq!(WatchpointType::from_gdb_type(4), Some(WatchpointType::Access));
        assert_eq!(WatchpointType::from_gdb_type(99), None);

        assert_eq!(WatchpointType::Read.to_gdb_type(), 2);
        assert_eq!(WatchpointType::Write.to_gdb_type(), 3);
        assert_eq!(WatchpointType::Access.to_gdb_type(), 4);
    }

    #[test]
    fn test_watchpoint_type_matches_access() {
        assert!(WatchpointType::Read.matches_access(WatchpointAccess::Read));
        assert!(WatchpointType::Read.matches_access(WatchpointAccess::ReadWrite));
        assert!(!WatchpointType::Read.matches_access(WatchpointAccess::Write));

        assert!(WatchpointType::Write.matches_access(WatchpointAccess::Write));
        assert!(WatchpointType::Write.matches_access(WatchpointAccess::ReadWrite));
        assert!(!WatchpointType::Write.matches_access(WatchpointAccess::Read));

        assert!(WatchpointType::Access.matches_access(WatchpointAccess::Read));
        assert!(WatchpointType::Access.matches_access(WatchpointAccess::Write));
        assert!(WatchpointType::Access.matches_access(WatchpointAccess::ReadWrite));
    }
}
