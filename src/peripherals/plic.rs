//! PLIC (Platform-Level Interrupt Controller) 实现
//!
//! PLIC 是 RISC-V 架构中的平台级中断控制器，用于管理外部中断：
//! - 中断源（最多 1023 个）
//! - 中断优先级
//! - 中断使能（按目标 Hart 和特权模式）
//! - 中断阈值
//! - 中断完成通知

use crate::tlm::{
    AddressRange, ScTime, TlmCommand, TlmError, TlmGenericPayload, TlmResponseStatus, TlmTarget,
};

/// PLIC 寄存器偏移
pub mod reg_offset {
    /// 中断优先级基址（每个中断源 4 字节）
    pub const PRIORITY_BASE: u64 = 0x0000;
    /// 中断优先级大小
    pub const PRIORITY_SIZE: u64 = 0x1000;
    /// 中断挂起位（每个位对应一个中断源）
    pub const PENDING_BASE: u64 = 0x1000;
    /// 中断使能基址（每个 Hart 128 字节）
    pub const ENABLE_BASE: u64 = 0x2000;
    /// 中断使能步长（每个 Hart）
    pub const ENABLE_STRIDE: u64 = 0x80;
    /// 优先级阈值基址（每个 Hart 上下文）
    pub const THRESHOLD_BASE: u64 = 0x200000;
    /// 中断声明/完成基址（每个 Hart 上下文）
    pub const CLAIM_COMPLETE_BASE: u64 = 0x200004;
    /// 上下文步长
    pub const CONTEXT_STRIDE: u64 = 0x1000;
}

/// 最大中断源数量（1-1023，0 保留为"无中断"）
pub const MAX_INTERRUPT_SOURCES: usize = 1024;
/// 最大优先级
pub const MAX_PRIORITY: u32 = 7;
/// PLIC 默认内存映射大小
pub const PLIC_SIZE: usize = 0x400000;

/// 中断请求信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptRequest {
    /// 中断 ID
    pub id: u32,
    /// 优先级
    pub priority: u32,
}

impl PartialOrd for InterruptRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InterruptRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // 优先级高的更大，相同优先级时ID小的更大（ID小的更优先）
        // 根据 RISC-V PLIC 规范：当两个或更多中断源具有相同优先级时，
        // 中断 ID 较小的中断优先于中断 ID 较大的中断（Smaller ID takes precedence）
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.id.cmp(&self.id)) // 反转ID比较顺序：ID小的优先级更高
    }
}

/// PLIC 上下文（对应一个 Hart + 特权模式组合）
#[derive(Debug, Clone)]
pub struct PlicContext {
    /// 优先级阈值（低于此值的中断被屏蔽）
    threshold: u32,
    /// 中断使能位图
    enabled: [u32; 32], // 1024 位 = 32 个 u32
    /// 已声明的中断（当前正在处理）
    claimed: Option<u32>,
}

impl PlicContext {
    /// 创建新的上下文
    fn new() -> Self {
        Self {
            threshold: 0,
            enabled: [0; 32],
            claimed: None,
        }
    }

    /// 检查中断是否使能
    fn is_enabled(&self, irq_id: u32) -> bool {
        let word = (irq_id / 32) as usize;
        let bit = irq_id % 32;
        word < 32 && (self.enabled[word] >> bit) & 1 != 0
    }

    /// 设置中断使能
    #[allow(dead_code)]
    fn set_enabled(&mut self, irq_id: u32, enable: bool) {
        let word = (irq_id / 32) as usize;
        let bit = irq_id % 32;
        if word < 32 {
            if enable {
                self.enabled[word] |= 1 << bit;
            } else {
                self.enabled[word] &= !(1 << bit);
            }
        }
    }
}

/// PLIC 外设
pub struct Plic {
    /// 基地址
    base_addr: u64,
    /// 支持的中断源数量
    num_sources: u32,
    /// 支持的上下文数量（Hart × 模式）
    num_contexts: u32,
    /// 中断优先级
    priorities: Vec<u32>,
    /// 中断挂起位
    pending: Vec<u32>,
    /// 上下文数组
    contexts: Vec<PlicContext>,
    /// 中断通知回调 (context_id, irq_id)
    interrupt_callback: Option<Box<dyn FnMut(u32, u32) + Send + Sync>>,
}

impl std::fmt::Debug for Plic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plic")
            .field("base_addr", &self.base_addr)
            .field("num_sources", &self.num_sources)
            .field("num_contexts", &self.num_contexts)
            .field("priorities", &self.priorities)
            .field("pending", &self.pending)
            .field("contexts", &self.contexts)
            .field("interrupt_callback", &self.interrupt_callback.is_some())
            .finish()
    }
}

impl Plic {
    /// 创建新的 PLIC 实例
    ///
    /// # 参数
    /// - `base_addr`: PLIC 基地址
    /// - `num_sources`: 中断源数量（1-1024）
    /// - `num_contexts`: 上下文数量（通常 = Hart数量 × 模式数）
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::peripherals::Plic;
    ///
    /// let plic = Plic::new(0x0C00_0000, 64, 8);
    /// ```
    pub fn new(base_addr: u64, num_sources: u32, num_contexts: u32) -> Self {
        let num_sources = num_sources.min(MAX_INTERRUPT_SOURCES as u32).max(1);
        let num_contexts = num_contexts.max(1);

        Self {
            base_addr,
            num_sources,
            num_contexts,
            priorities: vec![0; num_sources as usize],
            pending: vec![0; num_sources.div_ceil(32) as usize],
            contexts: vec![PlicContext::new(); num_contexts as usize],
            interrupt_callback: None,
        }
    }

    /// 创建默认配置的 PLIC（适用于单核）
    pub fn with_default_config(base_addr: u64) -> Self {
        Self::new(base_addr, 32, 2) // M-mode 和 S-mode
    }

    /// 获取基地址
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// 获取中断源数量
    pub fn num_sources(&self) -> u32 {
        self.num_sources
    }

    /// 获取上下文数量
    pub fn num_contexts(&self) -> u32 {
        self.num_contexts
    }

    /// 读取中断优先级
    pub fn read_priority(&self, irq_id: u32) -> u32 {
        if irq_id > 0 && irq_id < self.num_sources {
            self.priorities[irq_id as usize]
        } else {
            0
        }
    }

    /// 写入中断优先级
    pub fn write_priority(&mut self, irq_id: u32, priority: u32) {
        if irq_id > 0 && irq_id < self.num_sources {
            // 优先级通常限制为 0-7
            self.priorities[irq_id as usize] = priority.min(MAX_PRIORITY);
        }
    }

    /// 读取中断挂起状态
    pub fn is_pending(&self, irq_id: u32) -> bool {
        if irq_id > 0 && irq_id < self.num_sources {
            let word = (irq_id / 32) as usize;
            let bit = irq_id % 32;
            (self.pending[word] >> bit) & 1 != 0
        } else {
            false
        }
    }

    /// 读取挂起位寄存器
    pub fn read_pending_reg(&self, word_idx: usize) -> u32 {
        self.pending.get(word_idx).copied().unwrap_or(0)
    }

    /// 触发外部中断（由外部设备调用）
    pub fn trigger_interrupt(&mut self, irq_id: u32) {
        if irq_id > 0 && irq_id < self.num_sources {
            let word = (irq_id / 32) as usize;
            let bit = irq_id % 32;
            self.pending[word] |= 1 << bit;
            self.process_interrupts();
        }
    }

    /// 清除中断挂起状态
    pub fn clear_pending(&mut self, irq_id: u32) {
        if irq_id > 0 && irq_id < self.num_sources {
            let word = (irq_id / 32) as usize;
            let bit = irq_id % 32;
            self.pending[word] &= !(1 << bit);
        }
    }

    /// 读取中断使能状态
    pub fn read_enable(&self, context_id: u32, word_idx: usize) -> u32 {
        if let Some(ctx) = self.contexts.get(context_id as usize) {
            ctx.enabled.get(word_idx).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// 写入中断使能状态
    pub fn write_enable(&mut self, context_id: u32, word_idx: usize, value: u32) {
        if let Some(ctx) = self.contexts.get_mut(context_id as usize) {
            if word_idx < 32 {
                // 确保中断源 0 永远禁用
                let masked_value = if word_idx == 0 { value & !0x1 } else { value };
                ctx.enabled[word_idx] = masked_value;
            }
        }
    }

    /// 读取优先级阈值
    pub fn read_threshold(&self, context_id: u32) -> u32 {
        self.contexts
            .get(context_id as usize)
            .map(|ctx| ctx.threshold)
            .unwrap_or(0)
    }

    /// 写入优先级阈值
    pub fn write_threshold(&mut self, context_id: u32, threshold: u32) {
        if let Some(ctx) = self.contexts.get_mut(context_id as usize) {
            ctx.threshold = threshold.min(MAX_PRIORITY);
        }
    }

    /// 声明中断（读取最高优先级的中断 ID）
    pub fn claim_interrupt(&mut self, context_id: u32) -> u32 {
        let irq_id = self.find_highest_priority_interrupt(context_id);

        if irq_id > 0 {
            // 记录已声明的中断
            if let Some(ctx) = self.contexts.get_mut(context_id as usize) {
                ctx.claimed = Some(irq_id);
            }
            // 清除挂起位
            self.clear_pending(irq_id);
        }

        irq_id
    }

    /// 完成中断（写回中断 ID）
    pub fn complete_interrupt(&mut self, context_id: u32, irq_id: u32) {
        if let Some(ctx) = self.contexts.get_mut(context_id as usize) {
            // 清除已声明状态
            if ctx.claimed == Some(irq_id) {
                ctx.claimed = None;
            }
        }
        // 重新处理中断（可能有新的或重新触发的中断）
        self.process_interrupts();
    }

    /// 查找最高优先级的中断
    pub fn find_highest_priority_interrupt(&self, context_id: u32) -> u32 {
        let ctx = match self.contexts.get(context_id as usize) {
            Some(c) => c,
            None => return 0,
        };

        let mut highest_irq: u32 = 0;
        let mut highest_priority: u32 = 0;

        // 遍历所有中断源
        for irq_id in 1..self.num_sources {
            // 检查是否挂起且使能
            if self.is_pending(irq_id) && ctx.is_enabled(irq_id) {
                let priority = self.priorities[irq_id as usize];
                // 优先级必须大于阈值
                if priority > ctx.threshold && priority > highest_priority {
                    highest_priority = priority;
                    highest_irq = irq_id;
                }
            }
        }

        highest_irq
    }

    /// 处理所有上下文的中断
    fn process_interrupts(&mut self) {
        for context_id in 0..self.num_contexts {
            let irq_id = self.find_highest_priority_interrupt(context_id);
            if irq_id > 0 {
                if let Some(ref mut cb) = self.interrupt_callback {
                    cb(context_id, irq_id);
                }
            }
        }
    }

    /// 设置中断回调
    pub fn set_interrupt_callback(&mut self, cb: impl FnMut(u32, u32) + Send + Sync + 'static) {
        self.interrupt_callback = Some(Box::new(cb));
    }

    /// 获取当前声明的中断
    pub fn get_claimed(&self, context_id: u32) -> Option<u32> {
        self.contexts
            .get(context_id as usize)
            .and_then(|ctx| ctx.claimed)
    }

    /// 计算寄存器地址
    fn calculate_reg_addr(&self, offset: u64) -> RegAddr {
        // 优先级寄存器 (0x0000 - 0x0FFF)
        if offset < reg_offset::PRIORITY_SIZE {
            let irq_id = (offset / 4) as u32;
            return RegAddr::Priority(irq_id);
        }

        // 挂起寄存器 (0x1000 - 0x1FFF)
        if (reg_offset::PENDING_BASE..reg_offset::ENABLE_BASE).contains(&offset) {
            let word_idx = ((offset - reg_offset::PENDING_BASE) / 4) as usize;
            return RegAddr::Pending(word_idx);
        }

        // 使能寄存器 (0x2000 - 0x1FFFF)
        if (reg_offset::ENABLE_BASE..reg_offset::THRESHOLD_BASE).contains(&offset) {
            let rel_offset = offset - reg_offset::ENABLE_BASE;
            let context_id = (rel_offset / reg_offset::ENABLE_STRIDE) as u32;
            let word_idx = ((rel_offset % reg_offset::ENABLE_STRIDE) / 4) as usize;
            return RegAddr::Enable(context_id, word_idx);
        }

        // 上下文空间 (0x200000+)
        if offset >= reg_offset::THRESHOLD_BASE {
            let rel_offset = offset - reg_offset::THRESHOLD_BASE;
            let context_id = (rel_offset / reg_offset::CONTEXT_STRIDE) as u32;
            let ctx_offset = rel_offset % reg_offset::CONTEXT_STRIDE;

            if ctx_offset == 0 {
                return RegAddr::Threshold(context_id);
            } else if ctx_offset == 4 {
                return RegAddr::ClaimComplete(context_id);
            }
        }

        RegAddr::Invalid
    }

    /// 读取寄存器
    fn read_reg(&self, offset: u64, size: usize) -> Result<u64, TlmError> {
        if size == 4 {
            match self.calculate_reg_addr(offset) {
                RegAddr::Priority(irq_id) => Ok(self.read_priority(irq_id) as u64),
                RegAddr::Pending(word_idx) => Ok(self.read_pending_reg(word_idx) as u64),
                RegAddr::Enable(context_id, word_idx) => {
                    Ok(self.read_enable(context_id, word_idx) as u64)
                }
                RegAddr::Threshold(context_id) => Ok(self.read_threshold(context_id) as u64),
                RegAddr::ClaimComplete(_context_id) => {
                    // Claim 是可变操作，这里只返回 0
                    Ok(0)
                }
                RegAddr::Invalid => Ok(0),
            }
        } else {
            Err(TlmError::InvalidLength(size))
        }
    }

    /// 写入寄存器
    fn write_reg(&mut self, offset: u64, value: u32) -> Result<(), TlmError> {
        match self.calculate_reg_addr(offset) {
            RegAddr::Priority(irq_id) => {
                self.write_priority(irq_id, value);
                Ok(())
            }
            RegAddr::Pending(_) => {
                // 挂起位是只读的，忽略写操作
                Ok(())
            }
            RegAddr::Enable(context_id, word_idx) => {
                self.write_enable(context_id, word_idx, value);
                Ok(())
            }
            RegAddr::Threshold(context_id) => {
                self.write_threshold(context_id, value);
                Ok(())
            }
            RegAddr::ClaimComplete(context_id) => {
                // 完成中断
                self.complete_interrupt(context_id, value);
                Ok(())
            }
            RegAddr::Invalid => Ok(()),
        }
    }
}

/// 寄存器地址类型
#[derive(Debug, Clone, Copy)]
enum RegAddr {
    Priority(u32),
    Pending(usize),
    Enable(u32, usize),
    Threshold(u32),
    ClaimComplete(u32),
    Invalid,
}

impl TlmTarget for Plic {
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        _delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        let addr = trans.address();

        // 检查地址范围
        if addr < self.base_addr || addr >= self.base_addr + PLIC_SIZE as u64 {
            trans.set_response_status(TlmResponseStatus::InvalidAddress);
            return Err(TlmError::InvalidAddress64(addr));
        }

        let offset = addr - self.base_addr;

        match trans.command() {
            TlmCommand::Read => {
                // Claim 操作需要在可变上下文中执行
                let is_claim = matches!(
                    self.calculate_reg_addr(offset),
                    RegAddr::ClaimComplete(context_id) if context_id < self.num_contexts
                );

                if is_claim {
                    // 处理 Claim
                    let context_id =
                        ((offset - reg_offset::THRESHOLD_BASE) / reg_offset::CONTEXT_STRIDE) as u32;
                    let irq_id = self.claim_interrupt(context_id);

                    for i in 0..trans.data_length().min(4) {
                        trans.data_mut()[i] = ((irq_id >> (i * 8)) & 0xFF) as u8;
                    }
                } else {
                    let value = self.read_reg(offset, trans.data_length())?;
                    for i in 0..trans.data_length().min(8) {
                        trans.data_mut()[i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }

                trans.set_response_status(TlmResponseStatus::Ok);
            }
            TlmCommand::Write => {
                let mut value: u32 = 0;
                for i in 0..trans.data_length().min(4) {
                    value |= (trans.data()[i] as u32) << (i * 8);
                }

                self.write_reg(offset, value)?;
                trans.set_response_status(TlmResponseStatus::Ok);
            }
        }

        Ok(())
    }

    fn get_address_ranges(&self) -> Vec<AddressRange> {
        vec![AddressRange::new(
            self.base_addr,
            self.base_addr + PLIC_SIZE as u64 - 1,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plic_creation() {
        let plic = Plic::new(0x0C00_0000, 64, 8);
        assert_eq!(plic.base_addr(), 0x0C00_0000);
        assert_eq!(plic.num_sources(), 64);
        assert_eq!(plic.num_contexts(), 8);
    }

    #[test]
    fn test_plic_priority() {
        let mut plic = Plic::new(0x0C00_0000, 64, 2);

        // 默认优先级为 0
        assert_eq!(plic.read_priority(1), 0);

        // 设置优先级
        plic.write_priority(1, 5);
        assert_eq!(plic.read_priority(1), 5);

        // 优先级被限制在最大值
        plic.write_priority(2, 100);
        assert_eq!(plic.read_priority(2), MAX_PRIORITY);

        // 中断源 0 保留
        plic.write_priority(0, 5);
        assert_eq!(plic.read_priority(0), 0);
    }

    #[test]
    fn test_plic_pending() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 初始无挂起中断
        assert!(!plic.is_pending(1));

        // 触发中断
        plic.trigger_interrupt(1);
        assert!(plic.is_pending(1));

        // 清除挂起
        plic.clear_pending(1);
        assert!(!plic.is_pending(1));
    }

    #[test]
    fn test_plic_enable() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 初始禁用
        assert_eq!(plic.read_enable(0, 0), 0);

        // 使能中断 1
        plic.write_enable(0, 0, 0x2); // bit 1
        assert_eq!(plic.read_enable(0, 0), 0x2);

        // 中断源 0 始终禁用
        plic.write_enable(0, 0, 0x1);
        assert_eq!(plic.read_enable(0, 0), 0);
    }

    #[test]
    fn test_plic_threshold() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 默认阈值为 0
        assert_eq!(plic.read_threshold(0), 0);

        // 设置阈值
        plic.write_threshold(0, 5);
        assert_eq!(plic.read_threshold(0), 5);
    }

    #[test]
    fn test_plic_claim_complete() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 设置优先级和使能
        plic.write_priority(5, 3);
        plic.write_enable(0, 0, 1 << 5);

        // 触发中断
        plic.trigger_interrupt(5);

        // 声明中断
        let claimed = plic.claim_interrupt(0);
        assert_eq!(claimed, 5);

        // 声明后挂起位清除
        assert!(!plic.is_pending(5));

        // 完成中断
        plic.complete_interrupt(0, 5);
        assert_eq!(plic.get_claimed(0), None);
    }

    #[test]
    fn test_plic_priority_arbitration() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 设置不同优先级
        plic.write_priority(1, 1);
        plic.write_priority(2, 3);
        plic.write_priority(3, 2);

        // 使能所有中断
        plic.write_enable(0, 0, 0xFF);

        // 触发多个中断
        plic.trigger_interrupt(1);
        plic.trigger_interrupt(2);
        plic.trigger_interrupt(3);

        // 应该返回最高优先级的中断 (ID=2, priority=3)
        let claimed = plic.claim_interrupt(0);
        assert_eq!(claimed, 2);
    }

    #[test]
    fn test_plic_threshold_masking() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 设置中断优先级
        plic.write_priority(1, 3);
        plic.write_enable(0, 0, 1 << 1);

        // 设置阈值高于中断优先级
        plic.write_threshold(0, 5);

        // 触发中断
        plic.trigger_interrupt(1);

        // 由于阈值高于优先级，无法声明中断
        let claimed = plic.find_highest_priority_interrupt(0);
        assert_eq!(claimed, 0);

        // 降低阈值
        plic.write_threshold(0, 2);
        let claimed = plic.claim_interrupt(0);
        assert_eq!(claimed, 1);
    }

    #[test]
    fn test_plic_interrupt_request_ordering() {
        let req1 = InterruptRequest { id: 1, priority: 5 };
        let req2 = InterruptRequest { id: 2, priority: 3 };
        let req3 = InterruptRequest { id: 3, priority: 5 }; // 相同优先级，ID 更大

        // 按优先级降序：优先级数值越大，优先级越高
        assert!(req1 > req2); // 优先级 5 > 3，所以 req1 > req2

        // 相同优先级时，ID 小的优先级更高（RISC-V PLIC 规范）
        // ID 1 < ID 3，所以 req1 的优先级高于 req3
        assert!(req1 > req3); // 相同优先级，ID 1 < 3，所以 req1 > req3
    }

    #[test]
    fn test_plic_tlm_read_write() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 写入优先级
        let write_data = vec![0x05, 0x00, 0x00, 0x00]; // 优先级 5
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0C00_0000 + 4, // 中断源 1 的优先级
            write_data,
        );
        let mut delay = ScTime::zero();

        assert!(plic.b_transport(&mut write_trans, &mut delay).is_ok());
        assert_eq!(plic.read_priority(1), 5);

        // 读取优先级
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x0C00_0000 + 4, 4);
        delay = ScTime::zero();

        assert!(plic.b_transport(&mut read_trans, &mut delay).is_ok());

        let value = read_trans.data()[0] as u32;
        assert_eq!(value, 5);
    }

    #[test]
    fn test_plic_tlm_claim() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 配置中断
        plic.write_priority(5, 3);
        plic.write_enable(0, 0, 1 << 5);
        plic.trigger_interrupt(5);

        // Claim 中断
        let mut claim_trans = TlmGenericPayload::new(
            TlmCommand::Read,
            0x0C00_0000 + reg_offset::THRESHOLD_BASE + 4, // Context 0 Claim
            4,
        );
        let mut delay = ScTime::zero();

        assert!(plic.b_transport(&mut claim_trans, &mut delay).is_ok());

        let irq_id = claim_trans.data()[0] as u32;
        assert_eq!(irq_id, 5);
    }

    #[test]
    fn test_plic_tlm_complete() {
        let mut plic = Plic::new(0x0C00_0000, 32, 2);

        // 模拟已声明的中断
        plic.write_priority(3, 2);
        plic.write_enable(0, 0, 1 << 3);
        plic.trigger_interrupt(3);
        plic.claim_interrupt(0);

        // Complete 中断
        let complete_data = vec![0x03, 0x00, 0x00, 0x00]; // 完成中断 3
        let mut complete_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0C00_0000 + reg_offset::THRESHOLD_BASE + 4, // Context 0 Complete
            complete_data,
        );
        let mut delay = ScTime::zero();

        assert!(plic.b_transport(&mut complete_trans, &mut delay).is_ok());
        assert_eq!(plic.get_claimed(0), None);
    }

    #[test]
    fn test_plic_address_range() {
        let plic = Plic::new(0x0C00_0000, 32, 2);
        let ranges = plic.get_address_ranges();

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0x0C00_0000);
        assert_eq!(ranges[0].end, 0x0C00_0000 + PLIC_SIZE as u64 - 1);
    }
}
