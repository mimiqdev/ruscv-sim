//! TLM2.0 总线实现
//!
//! 实现多 initiator/target 互联和路由仲裁

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use super::{
    AddressRange, DmiAccessRights, DmiData, ScTime, TlmCommand, TlmError, TlmGenericPayload,
    TlmInitiator, TlmInterface, TlmPhase, TlmResponseStatus, TlmSyncEnum, TlmTarget,
};

/// TLM 总线路由项
///
/// 将一个地址范围映射到一个目标
pub struct BusRoute {
    /// 地址范围
    pub range: AddressRange,
    /// 目标（使用 Arc<Mutex> 支持多线程）
    pub target: Arc<Mutex<dyn TlmTarget>>,
    /// 优先级（数值越小优先级越高）
    pub priority: u32,
    /// 路由名称
    pub name: String,
}

impl std::fmt::Debug for BusRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusRoute")
            .field("range", &self.range)
            .field("priority", &self.priority)
            .field("name", &self.name)
            .field("target", &"<dyn TlmTarget>")
            .finish()
    }
}

/// 总线仲裁策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArbitrationPolicy {
    /// 固定优先级
    FixedPriority,
    /// 轮询
    RoundRobin,
    /// 最近最少使用
    LRU,
}

/// TLM 总线
///
/// 实现多 initiator 到多 target 的路由和仲裁
#[derive(Debug)]
pub struct TlmBus {
    /// 路由表：地址范围到目标的映射
    routes: Vec<BusRoute>,
    /// 仲裁策略
    arbitration_policy: ArbitrationPolicy,
    /// 默认延迟
    default_delay: ScTime,
    /// 总线繁忙标志（保留用于将来实现）
    #[allow(dead_code)]
    busy: Arc<RwLock<bool>>,
    /// 轮询索引
    round_robin_index: Arc<RwLock<usize>>,
    /// DMI 缓存
    dmi_cache: Arc<RwLock<HashMap<u64, DmiData>>>,
}

impl TlmBus {
    /// 创建新的 TLM 总线
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::{TlmBus, ArbitrationPolicy};
    ///
    /// let bus = TlmBus::new(ArbitrationPolicy::RoundRobin);
    /// ```
    pub fn new(policy: ArbitrationPolicy) -> Self {
        Self {
            routes: Vec::new(),
            arbitration_policy: policy,
            default_delay: ScTime::from_nanoseconds(10),
            busy: Arc::new(RwLock::new(false)),
            round_robin_index: Arc::new(RwLock::new(0)),
            dmi_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建使用默认设置的 TLM 总线
    pub fn default_bus() -> Self {
        Self::new(ArbitrationPolicy::FixedPriority)
    }
}

impl Default for TlmBus {
    fn default() -> Self {
        Self::new(ArbitrationPolicy::FixedPriority)
    }
}

impl TlmBus {
    /// 添加路由
    ///
    /// # 参数
    /// - `range`: 地址范围
    /// - `target`: 目标组件
    /// - `priority`: 优先级（数值越小优先级越高）
    /// - `name`: 路由名称
    pub fn add_route(
        &mut self,
        range: AddressRange,
        target: Arc<Mutex<dyn TlmTarget>>,
        priority: u32,
        name: impl Into<String>,
    ) {
        let route = BusRoute {
            range,
            target,
            priority,
            name: name.into(),
        };
        self.routes.push(route);
        // 按优先级排序
        self.routes.sort_by_key(|r| r.priority);
    }

    /// 移除路由
    pub fn remove_route(&mut self, name: &str) -> bool {
        let initial_len = self.routes.len();
        self.routes.retain(|r| r.name != name);
        self.routes.len() < initial_len
    }

    /// 查找地址对应的路由
    fn find_route(&self, address: u64) -> Option<&BusRoute> {
        match self.arbitration_policy {
            ArbitrationPolicy::FixedPriority => {
                // 固定优先级：返回第一个匹配的（已按优先级排序）
                self.routes.iter().find(|r| r.range.contains(address))
            }
            ArbitrationPolicy::RoundRobin => {
                // 轮询：从当前索引开始查找
                let index = *self.round_robin_index.read().unwrap();
                let len = self.routes.len();

                for i in 0..len {
                    let idx = (index + i) % len;
                    if let Some(route) = self.routes.get(idx) {
                        if route.range.contains(address) {
                            return Some(route);
                        }
                    }
                }
                None
            }
            ArbitrationPolicy::LRU => {
                // LRU：返回第一个匹配的（简化实现）
                self.routes.iter().find(|r| r.range.contains(address))
            }
        }
    }

    /// 更新轮询索引
    fn update_round_robin(&self) {
        if self.arbitration_policy == ArbitrationPolicy::RoundRobin {
            let mut index = self.round_robin_index.write().unwrap();
            *index = (*index + 1) % self.routes.len().max(1);
        }
    }

    /// 设置默认延迟
    pub fn set_default_delay(&mut self, delay: ScTime) {
        self.default_delay = delay;
    }

    /// 获取默认延迟
    pub fn default_delay(&self) -> ScTime {
        self.default_delay
    }

    /// 设置仲裁策略
    pub fn set_arbitration_policy(&mut self, policy: ArbitrationPolicy) {
        self.arbitration_policy = policy;
    }

    /// 获取仲裁策略
    pub fn arbitration_policy(&self) -> ArbitrationPolicy {
        self.arbitration_policy
    }

    /// 获取路由数量
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// 清空 DMI 缓存
    pub fn invalidate_dmi(&self) {
        self.dmi_cache.write().unwrap().clear();
    }

    /// 获取 DMI 数据
    pub fn get_dmi(&self, address: u64) -> Option<DmiData> {
        self.dmi_cache.read().unwrap().get(&address).cloned()
    }

    /// 注册 DMI
    pub fn register_dmi(&self, start_address: u64, dmi_data: DmiData) {
        self.dmi_cache
            .write()
            .unwrap()
            .insert(start_address, dmi_data);
    }
}

impl TlmInitiator for TlmBus {
    fn b_transport(
        &self,
        trans: &mut TlmGenericPayload,
        delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        let address = trans.address();

        // 查找路由
        let route = self
            .find_route(address)
            .ok_or(TlmError::InvalidAddress(address as u32))?;

        // 更新轮询索引
        self.update_round_robin();

        // 添加总线延迟
        *delay += self.default_delay;

        // 转发到目标
        let mut target = route.target.lock().unwrap();
        target.b_transport(trans, delay)
    }

    fn nb_transport_fw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        delay: &mut ScTime,
    ) -> Result<TlmSyncEnum, TlmError> {
        let address = trans.address();

        // 查找路由
        let route = match self.find_route(address) {
            Some(r) => r,
            None => {
                trans.set_response_status(TlmResponseStatus::InvalidAddress);
                return Err(TlmError::InvalidAddress(address as u32));
            }
        };

        // 更新轮询索引
        self.update_round_robin();

        // 添加总线延迟
        *delay += self.default_delay;

        // 转发到目标
        let target = route.target.lock().unwrap();
        target.nb_transport_bw(trans, phase, delay)
    }

    fn get_direct_mem_ptr(&self, trans: &TlmGenericPayload) -> Option<DmiData> {
        let address = trans.address();

        // 先检查 DMI 缓存
        if let Some(dmi) = self.get_dmi(address) {
            return Some(dmi);
        }

        // 查找路由并获取 DMI
        let route = self.find_route(address)?;
        let target = route.target.lock().unwrap();
        target.get_direct_mem_ptr(trans)
    }
}

/// 总线桥接器
///
/// 用于连接两个总线或实现地址转换
pub struct TlmBusBridge {
    /// 输出总线
    output_bus: Arc<Mutex<dyn TlmInitiator>>,
    /// 地址偏移
    address_offset: i64,
    /// 延迟补偿
    delay_adjustment: ScTime,
}

impl std::fmt::Debug for TlmBusBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlmBusBridge")
            .field("output_bus", &"<dyn TlmInitiator>")
            .field("address_offset", &self.address_offset)
            .field("delay_adjustment", &self.delay_adjustment)
            .finish()
    }
}

impl TlmBusBridge {
    /// 创建新的总线桥接器
    pub fn new(
        output_bus: Arc<Mutex<dyn TlmInitiator>>,
        address_offset: i64,
        delay_adjustment: ScTime,
    ) -> Self {
        Self {
            output_bus,
            address_offset,
            delay_adjustment,
        }
    }

    /// 转换地址
    fn translate_address(&self, address: u64) -> u64 {
        if self.address_offset >= 0 {
            address.wrapping_add(self.address_offset as u64)
        } else {
            address.wrapping_sub((-self.address_offset) as u64)
        }
    }
}

impl TlmTarget for TlmBusBridge {
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        // 地址转换
        let orig_address = trans.address();
        let new_address = self.translate_address(orig_address);
        trans.set_address(new_address);

        // 延迟调整
        *delay += self.delay_adjustment;

        // 转发到输出总线
        let bus = self.output_bus.lock().unwrap();
        bus.b_transport(trans, delay)
    }

    fn nb_transport_bw(
        &self,
        trans: &mut TlmGenericPayload,
        phase: &mut TlmPhase,
        delay: &mut ScTime,
    ) -> Result<TlmSyncEnum, TlmError> {
        let bus = self.output_bus.lock().unwrap();
        bus.nb_transport_fw(trans, phase, delay)
    }

    fn get_address_ranges(&self) -> Vec<AddressRange> {
        // 桥接器需要根据实际情况配置地址范围
        vec![]
    }
}

/// 简单内存 TLM 包装器
#[derive(Debug)]
pub struct TlmSimpleMemory {
    /// 内存数据
    memory: Vec<u8>,
    /// 访问延迟
    delay: ScTime,
    /// 基地址
    base_addr: u64,
    /// 大小
    size: usize,
    /// DMI 支持
    dmi_enabled: bool,
}

impl TlmSimpleMemory {
    /// 创建新的简单内存
    pub fn new(base_addr: u64, size: usize) -> Self {
        Self {
            memory: vec![0; size],
            delay: ScTime::from_nanoseconds(1),
            base_addr,
            size,
            dmi_enabled: true,
        }
    }

    /// 加载数据到内存
    pub fn load(&mut self, data: &[u8], offset: usize) {
        let len = data.len().min(self.size - offset);
        self.memory[offset..offset + len].copy_from_slice(&data[..len]);
    }

    /// 从内存读取数据
    pub fn read_bytes(&self, offset: usize, size: usize) -> Result<Vec<u8>, TlmError> {
        if offset + size > self.size {
            return Err(TlmError::InvalidAddress(
                (self.base_addr + offset as u64) as u32,
            ));
        }
        Ok(self.memory[offset..offset + size].to_vec())
    }

    /// 写入数据到内存
    pub fn write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), TlmError> {
        if offset + data.len() > self.size {
            return Err(TlmError::InvalidAddress(
                (self.base_addr + offset as u64) as u32,
            ));
        }
        self.memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// 设置延迟
    pub fn set_delay(&mut self, delay: ScTime) {
        self.delay = delay;
    }

    /// 启用/禁用 DMI
    pub fn set_dmi_enabled(&mut self, enabled: bool) {
        self.dmi_enabled = enabled;
    }

    /// 获取内存指针（用于 DMI）
    pub fn get_memory_ptr(&mut self) -> *mut u8 {
        self.memory.as_mut_ptr()
    }
}

impl TlmTarget for TlmSimpleMemory {
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        let addr = trans.address();

        // 检查地址范围
        if addr < self.base_addr
            || addr + trans.data_length() as u64 > self.base_addr + self.size as u64
        {
            trans.set_response_status(TlmResponseStatus::InvalidAddress);
            return Err(TlmError::InvalidAddress(addr as u32));
        }

        let offset = (addr - self.base_addr) as usize;

        match trans.command() {
            TlmCommand::Read => {
                for i in 0..trans.data_length() {
                    trans.data_mut()[i] = self.memory[offset + i];
                }
            }
            TlmCommand::Write => {
                let data = trans.data();
                self.memory[offset..offset + data.len()].copy_from_slice(data);
            }
        }

        // 添加延迟
        *delay += self.delay;

        trans.set_response_status(TlmResponseStatus::Ok);
        trans.set_dmi_allowed(self.dmi_enabled);
        Ok(())
    }

    fn get_address_ranges(&self) -> Vec<AddressRange> {
        vec![AddressRange::new(
            self.base_addr,
            self.base_addr + self.size as u64 - 1,
        )]
    }

    fn get_direct_mem_ptr(&self, _trans: &TlmGenericPayload) -> Option<DmiData> {
        if !self.dmi_enabled {
            return None;
        }

        // 使用裸指针转换来绕过不可变引用限制
        // 这是安全的，因为 DMI 访问需要外部协调同步
        let dmi = DmiData {
            dmi_ptr: self.memory.as_ptr() as *mut u8,
            dmi_size: self.size,
            access_rights: DmiAccessRights::ReadWrite,
            read_latency: self.delay,
            write_latency: self.delay,
            start_address: self.base_addr,
            end_address: self.base_addr + self.size as u64 - 1,
        };

        Some(dmi)
    }
}

impl TlmInterface for TlmSimpleMemory {
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, TlmError> {
        if addr < self.base_addr || addr + size as u64 > self.base_addr + self.size as u64 {
            return Err(TlmError::InvalidAddress(addr as u32));
        }

        let offset = (addr - self.base_addr) as usize;
        Ok(self.memory[offset..offset + size].to_vec())
    }

    fn write(&mut self, addr: u64, data: &[u8]) -> Result<(), TlmError> {
        if addr < self.base_addr || addr + data.len() as u64 > self.base_addr + self.size as u64 {
            return Err(TlmError::InvalidAddress(addr as u32));
        }

        let offset = (addr - self.base_addr) as usize;
        self.memory[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn get_delay(&self) -> ScTime {
        self.delay
    }

    fn set_delay(&mut self, delay: ScTime) {
        self.delay = delay;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_tlm_bus_creation() {
        let bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        assert_eq!(bus.route_count(), 0);
        assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::FixedPriority);
    }

    #[test]
    fn test_tlm_bus_add_route() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory1");

        assert_eq!(bus.route_count(), 1);
    }

    #[test]
    fn test_tlm_bus_b_transport() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory1");

        // 先写入数据
        let mut write_trans =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);
        let mut delay = ScTime::zero();

        assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());

        // 再读取验证
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        delay = ScTime::zero();

        assert!(bus.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data(), &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn test_tlm_bus_invalid_address() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory1");

        // 访问无效地址
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x2000, 4);
        let mut delay = ScTime::zero();

        assert!(bus.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_tlm_simple_memory() {
        let mut mem = TlmSimpleMemory::new(0x1000, 1024);

        // 测试 load
        mem.load(&[0x01, 0x02, 0x03, 0x04], 0);

        // 测试 read_bytes
        let data = mem.read_bytes(0, 4).unwrap();
        assert_eq!(data, vec![0x01, 0x02, 0x03, 0x04]);

        // 测试 write_bytes
        mem.write_bytes(4, &[0x05, 0x06, 0x07, 0x08]).unwrap();

        // 验证
        let data = mem.read_bytes(4, 4).unwrap();
        assert_eq!(data, vec![0x05, 0x06, 0x07, 0x08]);
    }

    #[test]
    fn test_tlm_simple_memory_b_transport() {
        let mut mem = TlmSimpleMemory::new(0x1000, 1024);

        // 写入数据
        let mut write_trans =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        let mut delay = ScTime::zero();

        assert!(mem.b_transport(&mut write_trans, &mut delay).is_ok());

        // 读取数据
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        delay = ScTime::zero();

        assert!(mem.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_tlm_simple_memory_out_of_range() {
        let mut mem = TlmSimpleMemory::new(0x1000, 1024);

        // 尝试访问超出范围的地址
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x2000, 4);
        let mut delay = ScTime::zero();

        assert!(mem.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_bus_remove_route() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory1");

        assert_eq!(bus.route_count(), 1);

        // 移除路由
        assert!(bus.remove_route("memory1"));
        assert_eq!(bus.route_count(), 0);

        // 尝试移除不存在的路由
        assert!(!bus.remove_route("nonexistent"));
    }

    #[test]
    fn test_address_range_overlap() {
        let range1 = AddressRange::new(0x1000, 0x1FFF);
        let range2 = AddressRange::new(0x1800, 0x2800);
        let range3 = AddressRange::new(0x2000, 0x3000);

        assert!(range1.overlaps(&range2));
        assert!(range2.overlaps(&range3));
        assert!(!range1.overlaps(&range3));
    }
}
