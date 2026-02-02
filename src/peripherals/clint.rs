//! CLINT (Core Local Interruptor) 实现
//!
//! CLINT 是 RISC-V 架构中的核心本地中断控制器，提供：
//! - mtime: 机器模式时间寄存器
//! - mtimecmp: 机器模式时间比较寄存器（用于定时器中断）
//! - MSIP: 机器模式软件中断挂起寄存器
//!
//! # 线程安全
//!
//! `mtime` 使用 `AtomicU64` 实现，支持多线程安全访问。其他寄存器在单线程模拟环境下
//! 通过 `&mut self` 保证独占访问。如需跨线程共享 CLINT，请使用 `Arc<Mutex<Clint>>`。

use crate::tlm::{
    AddressRange, ScTime, TlmCommand, TlmError, TlmGenericPayload, TlmResponseStatus, TlmTarget,
};
use std::sync::atomic::{AtomicU64, Ordering};

/// CLINT 寄存器偏移
pub mod reg_offset {
    /// MSIP 寄存器基址偏移（每个 Hart 4 字节）
    pub const MSIP_BASE: u64 = 0x0000;
    /// MTIMECMP 寄存器基址偏移（每个 Hart 8 字节）
    pub const MTIMECMP_BASE: u64 = 0x4000;
    /// MTIME 寄存器偏移
    pub const MTIME: u64 = 0xBFF8;
}

/// CLINT 默认内存映射大小
pub const CLINT_SIZE: usize = 0xC000;

/// 定时器中断回调类型
pub type TimerInterruptCallback = Box<dyn FnMut(u32) + Send + Sync>;

/// 软件中断回调类型
pub type SoftwareInterruptCallback = Box<dyn FnMut(u32) + Send + Sync>;

/// CLINT 外设
///
/// # 线程安全
///
/// `mtime` 字段使用 `AtomicU64` 保证多线程安全访问，支持并发读写。
/// 其他寄存器在典型的单线程模拟环境中通过 `&mut self` 确保独占访问。
/// 如需在多线程间共享 CLINT 实例，建议使用 `Arc<Mutex<Clint>>` 包装。
pub struct Clint {
    /// 基地址
    base_addr: u64,
    /// 支持的 Hart 数量
    num_harts: u32,
    /// MTIME 寄存器（64位全局时间，线程安全）
    ///
    /// 使用 `AtomicU64` 实现，支持 `load`/`store` 原子操作。
    /// 在模拟时间推进时使用 `Relaxed` 内存序（性能优先），
    /// 在跨线程访问时建议使用 `SeqCst` 保证顺序一致性。
    mtime: AtomicU64,
    /// MTIMECMP 寄存器（每个 Hart 一个）
    mtimecmp: Vec<u64>,
    /// MSIP 寄存器（每个 Hart 一个）
    msip: Vec<u32>,
    /// 定时器中断挂起状态
    timer_interrupt_pending: Vec<bool>,
    /// 软件中断挂起状态
    software_interrupt_pending: Vec<bool>,
    /// 定时器中断回调
    timer_interrupt_cb: Option<TimerInterruptCallback>,
    /// 软件中断回调
    software_interrupt_cb: Option<SoftwareInterruptCallback>,
    /// 时钟频率（Hz）
    clock_freq: u64,
    /// 上次更新的模拟时间
    pub last_update_time: ScTime,
}

impl std::fmt::Debug for Clint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clint")
            .field("base_addr", &self.base_addr)
            .field("num_harts", &self.num_harts)
            .field("mtime", &self.read_mtime())
            .field("mtimecmp", &self.mtimecmp)
            .field("msip", &self.msip)
            .field("timer_interrupt_pending", &self.timer_interrupt_pending)
            .field(
                "software_interrupt_pending",
                &self.software_interrupt_pending,
            )
            .field("timer_interrupt_cb", &self.timer_interrupt_cb.is_some())
            .field(
                "software_interrupt_cb",
                &self.software_interrupt_cb.is_some(),
            )
            .field("clock_freq", &self.clock_freq)
            .field("last_update_time", &self.last_update_time)
            .finish()
    }
}

impl Clint {
    /// 创建新的 CLINT 实例
    ///
    /// # 参数
    /// - `base_addr`: CLINT 基地址
    /// - `num_harts`: 支持的 Hart 数量
    /// - `clock_freq`: 时钟频率（Hz），默认 10MHz
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::peripherals::Clint;
    ///
    /// let clint = Clint::new(0x0200_0000, 4, 10_000_000);
    /// ```
    pub fn new(base_addr: u64, num_harts: u32, clock_freq: u64) -> Self {
        Self {
            base_addr,
            num_harts,
            mtime: AtomicU64::new(0),
            mtimecmp: vec![u64::MAX; num_harts as usize],
            msip: vec![0; num_harts as usize],
            timer_interrupt_pending: vec![false; num_harts as usize],
            software_interrupt_pending: vec![false; num_harts as usize],
            timer_interrupt_cb: None,
            software_interrupt_cb: None,
            clock_freq,
            last_update_time: ScTime::zero(),
        }
    }

    /// 创建使用默认时钟频率的 CLINT
    pub fn with_default_clock(base_addr: u64, num_harts: u32) -> Self {
        Self::new(base_addr, num_harts, 10_000_000) // 10MHz
    }

    /// 获取基地址
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// 获取支持的 Hart 数量
    pub fn num_harts(&self) -> u32 {
        self.num_harts
    }

    /// 读取 mtime（原子加载）
    ///
    /// 使用 `Ordering::Relaxed` 内存序，适用于单线程模拟环境。
    /// 如需跨线程严格顺序保证，请使用 `read_mtime_atomic`。
    pub fn read_mtime(&self) -> u64 {
        self.mtime.load(Ordering::Relaxed)
    }

    /// 读取 mtime（指定内存序）
    ///
    /// # 参数
    /// - `ordering`: 内存序，跨线程访问建议使用 `Ordering::SeqCst`
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::peripherals::Clint;
    /// use std::sync::atomic::Ordering;
    ///
    /// let clint = Clint::new(0x0200_0000, 4, 10_000_000);
    /// let mtime = clint.read_mtime_atomic(Ordering::SeqCst);
    /// ```
    pub fn read_mtime_atomic(&self, ordering: Ordering) -> u64 {
        self.mtime.load(ordering)
    }

    /// 写入 mtime（原子存储）
    ///
    /// 使用 `Ordering::Relaxed` 内存序，适用于单线程模拟环境。
    /// 写入后会更新定时器中断状态。
    pub fn write_mtime(&mut self, value: u64) {
        self.mtime.store(value, Ordering::Relaxed);
        self.update_timer_interrupts();
    }

    /// 写入 mtime（指定内存序）
    ///
    /// # 参数
    /// - `value`: 要写入的值
    /// - `ordering`: 内存序，跨线程访问建议使用 `Ordering::SeqCst`
    pub fn write_mtime_atomic(&mut self, value: u64, ordering: Ordering) {
        self.mtime.store(value, ordering);
        self.update_timer_interrupts();
    }

    /// 读取 mtimecmp
    pub fn read_mtimecmp(&self, hart_id: u32) -> Option<u64> {
        self.mtimecmp.get(hart_id as usize).copied()
    }

    /// 写入 mtimecmp
    pub fn write_mtimecmp(&mut self, hart_id: u32, value: u64) {
        if let Some(reg) = self.mtimecmp.get_mut(hart_id as usize) {
            *reg = value;
            self.update_timer_interrupt_for_hart(hart_id);
        }
    }

    /// 读取 msip
    pub fn read_msip(&self, hart_id: u32) -> Option<u32> {
        self.msip.get(hart_id as usize).copied()
    }

    /// 写入 msip（只有第0位有效）
    pub fn write_msip(&mut self, hart_id: u32, value: u32) {
        if let Some(reg) = self.msip.get_mut(hart_id as usize) {
            *reg = value & 0x1;
            self.update_software_interrupt_for_hart(hart_id);
        }
    }

    /// 设置定时器中断回调
    pub fn set_timer_interrupt_callback(&mut self, cb: TimerInterruptCallback) {
        self.timer_interrupt_cb = Some(cb);
    }

    /// 设置软件中断回调
    pub fn set_software_interrupt_callback(&mut self, cb: SoftwareInterruptCallback) {
        self.software_interrupt_cb = Some(cb);
    }

    /// 获取定时器中断挂起状态
    pub fn is_timer_interrupt_pending(&self, hart_id: u32) -> bool {
        self.timer_interrupt_pending
            .get(hart_id as usize)
            .copied()
            .unwrap_or(false)
    }

    /// 获取软件中断挂起状态
    pub fn is_software_interrupt_pending(&self, hart_id: u32) -> bool {
        self.software_interrupt_pending
            .get(hart_id as usize)
            .copied()
            .unwrap_or(false)
    }

    /// 清除定时器中断
    pub fn clear_timer_interrupt(&mut self, hart_id: u32) {
        if let Some(pending) = self.timer_interrupt_pending.get_mut(hart_id as usize) {
            *pending = false;
        }
    }

    /// 清除软件中断
    pub fn clear_software_interrupt(&mut self, hart_id: u32) {
        if let Some(pending) = self.software_interrupt_pending.get_mut(hart_id as usize) {
            *pending = false;
        }
        if let Some(reg) = self.msip.get_mut(hart_id as usize) {
            *reg = 0;
        }
    }

    /// 更新 mtime（基于时间差，原子操作）
    ///
    /// 使用 `fetch_add` 原子操作增加 mtime 值。
    /// 此方法线程安全，可在多线程环境中调用。
    pub fn update_mtime(&mut self, current_time: ScTime) {
        let delta_ns = current_time.to_nanoseconds() - self.last_update_time.to_nanoseconds();
        let ticks = (delta_ns * self.clock_freq) / 1_000_000_000;

        if ticks > 0 {
            // 使用 fetch_add 原子操作，Relaxed 序在单线程模拟中足够
            self.mtime.fetch_add(ticks, Ordering::Relaxed);
            self.update_timer_interrupts();
        }

        self.last_update_time = current_time;
    }

    /// 更新所有 Hart 的定时器中断状态
    fn update_timer_interrupts(&mut self) {
        for hart_id in 0..self.num_harts {
            self.update_timer_interrupt_for_hart(hart_id);
        }
    }

    /// 更新指定 Hart 的定时器中断状态
    fn update_timer_interrupt_for_hart(&mut self, hart_id: u32) {
        if let Some(mtimecmp) = self.mtimecmp.get(hart_id as usize) {
            let mtime = self.read_mtime();
            let pending = mtime >= *mtimecmp;

            if let Some(old_pending) = self.timer_interrupt_pending.get_mut(hart_id as usize) {
                // 如果从中断未挂起到挂起，触发回调
                if pending && !*old_pending {
                    if let Some(ref mut cb) = self.timer_interrupt_cb {
                        cb(hart_id);
                    }
                }
                *old_pending = pending;
            }
        }
    }

    /// 更新指定 Hart 的软件中断状态
    fn update_software_interrupt_for_hart(&mut self, hart_id: u32) {
        if let Some(msip) = self.msip.get(hart_id as usize) {
            let pending = (*msip & 0x1) != 0;

            if let Some(old_pending) = self.software_interrupt_pending.get_mut(hart_id as usize) {
                // 如果从中断未挂起到挂起，触发回调
                if pending && !*old_pending {
                    if let Some(ref mut cb) = self.software_interrupt_cb {
                        cb(hart_id);
                    }
                }
                *old_pending = pending;
            }
        }
    }

    /// 读取寄存器（内部实现）
    fn read_reg(&self, offset: u64, size: usize) -> Result<u64, TlmError> {
        match size {
            4 => self.read_reg32(offset).map(|v| v as u64),
            8 => self.read_reg64(offset),
            _ => Err(TlmError::InvalidLength(size)),
        }
    }

    /// 读取 32 位寄存器
    fn read_reg32(&self, offset: u64) -> Result<u32, TlmError> {
        // MSIP 寄存器 (每个 Hart 4 字节)
        if offset < reg_offset::MSIP_BASE + (self.num_harts as u64 * 4) {
            let hart_id = (offset / 4) as u32;
            return Ok(self.read_msip(hart_id).unwrap_or(0));
        }

        let mtime = self.read_mtime();

        // MTIME 低 32 位
        if offset == reg_offset::MTIME {
            return Ok(mtime as u32);
        }
        // MTIME 高 32 位
        if offset == reg_offset::MTIME + 4 {
            return Ok((mtime >> 32) as u32);
        }

        // 未实现或未映射的寄存器
        Ok(0)
    }

    /// 读取 64 位寄存器
    fn read_reg64(&self, offset: u64) -> Result<u64, TlmError> {
        // MTIMECMP 寄存器 (每个 Hart 8 字节)
        if offset >= reg_offset::MTIMECMP_BASE
            && offset < reg_offset::MTIMECMP_BASE + (self.num_harts as u64 * 8)
        {
            let hart_id = ((offset - reg_offset::MTIMECMP_BASE) / 8) as u32;
            return Ok(self.read_mtimecmp(hart_id).unwrap_or(u64::MAX));
        }

        // MTIME 寄存器 - 使用原子读取
        if offset == reg_offset::MTIME {
            return Ok(self.read_mtime());
        }

        // 其他情况，尝试读取两个 32 位值
        let low = self.read_reg32(offset)? as u64;
        let high = self.read_reg32(offset + 4)? as u64;
        Ok(low | (high << 32))
    }

    /// 写入寄存器（内部实现）
    fn write_reg(&mut self, offset: u64, value: u64, size: usize) -> Result<(), TlmError> {
        match size {
            4 => self.write_reg32(offset, value as u32),
            8 => self.write_reg64(offset, value),
            _ => Err(TlmError::InvalidLength(size)),
        }
    }

    /// 写入 32 位寄存器
    fn write_reg32(&mut self, offset: u64, value: u32) -> Result<(), TlmError> {
        // MSIP 寄存器
        if offset < reg_offset::MSIP_BASE + (self.num_harts as u64 * 4) {
            let hart_id = (offset / 4) as u32;
            self.write_msip(hart_id, value);
            return Ok(());
        }

        // MTIME 低 32 位 - 使用原子读取-修改-写入
        if offset == reg_offset::MTIME {
            let old_mtime = self.read_mtime();
            let new_mtime = (old_mtime & !0xFFFF_FFFF) | (value as u64);
            self.write_mtime(new_mtime);
            return Ok(());
        }
        // MTIME 高 32 位
        if offset == reg_offset::MTIME + 4 {
            let old_mtime = self.read_mtime();
            let new_mtime = (old_mtime & 0xFFFF_FFFF) | ((value as u64) << 32);
            self.write_mtime(new_mtime);
            return Ok(());
        }

        Ok(())
    }

    /// 写入 64 位寄存器
    fn write_reg64(&mut self, offset: u64, value: u64) -> Result<(), TlmError> {
        // MTIMECMP 寄存器
        if offset >= reg_offset::MTIMECMP_BASE
            && offset < reg_offset::MTIMECMP_BASE + (self.num_harts as u64 * 8)
        {
            let hart_id = ((offset - reg_offset::MTIMECMP_BASE) / 8) as u32;
            self.write_mtimecmp(hart_id, value);
            return Ok(());
        }

        // MTIME 寄存器
        if offset == reg_offset::MTIME {
            self.write_mtime(value);
            return Ok(());
        }

        // 其他情况，分成两个 32 位写入
        self.write_reg32(offset, value as u32)?;
        self.write_reg32(offset + 4, (value >> 32) as u32)
    }
}

impl TlmTarget for Clint {
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        _delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        let addr = trans.address();

        // 检查地址范围
        if addr < self.base_addr || addr >= self.base_addr + CLINT_SIZE as u64 {
            trans.set_response_status(TlmResponseStatus::InvalidAddress);
            return Err(TlmError::InvalidAddress64(addr));
        }

        let offset = addr - self.base_addr;

        match trans.command() {
            TlmCommand::Read => {
                let size = trans.data_length();
                let value = self.read_reg(offset, size)?;

                // 将值写入数据缓冲区
                for i in 0..size.min(8) {
                    trans.data_mut()[i] = ((value >> (i * 8)) & 0xFF) as u8;
                }

                trans.set_response_status(TlmResponseStatus::Ok);
            }
            TlmCommand::Write => {
                let size = trans.data_length();
                let mut value: u64 = 0;

                // 从数据缓冲区读取值
                for i in 0..size.min(8) {
                    value |= (trans.data()[i] as u64) << (i * 8);
                }

                self.write_reg(offset, value, size)?;
                trans.set_response_status(TlmResponseStatus::Ok);
            }
        }

        Ok(())
    }

    fn get_address_ranges(&self) -> Vec<AddressRange> {
        vec![AddressRange::new(
            self.base_addr,
            self.base_addr + CLINT_SIZE as u64 - 1,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_clint_creation() {
        let clint = Clint::new(0x0200_0000, 4, 10_000_000);
        assert_eq!(clint.base_addr(), 0x0200_0000);
        assert_eq!(clint.num_harts(), 4);
        assert_eq!(clint.read_mtime(), 0);
    }

    #[test]
    fn test_clint_mtime() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 测试 mtime 读写
        clint.write_mtime(0x1234_5678_9ABC_DEF0);
        assert_eq!(clint.read_mtime(), 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn test_clint_mtimecmp() {
        let mut clint = Clint::new(0x0200_0000, 4, 10_000_000);

        // 测试 mtimecmp 读写
        clint.write_mtimecmp(0, 0x1000);
        assert_eq!(clint.read_mtimecmp(0), Some(0x1000));

        clint.write_mtimecmp(3, 0x2000);
        assert_eq!(clint.read_mtimecmp(3), Some(0x2000));

        // 无效的 Hart ID
        assert_eq!(clint.read_mtimecmp(10), None);
    }

    #[test]
    fn test_clint_msip() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 测试 msip 读写（只有第0位有效）
        clint.write_msip(0, 0xFFFFFFFF);
        assert_eq!(clint.read_msip(0), Some(0x1)); // 只有 bit 0

        clint.write_msip(0, 0);
        assert_eq!(clint.read_msip(0), Some(0));
    }

    #[test]
    fn test_clint_timer_interrupt() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 设置 mtimecmp
        clint.write_mtimecmp(0, 100);

        // mtime < mtimecmp，无中断
        clint.write_mtime(50);
        assert!(!clint.is_timer_interrupt_pending(0));

        // mtime >= mtimecmp，有中断
        clint.write_mtime(100);
        assert!(clint.is_timer_interrupt_pending(0));

        // 清除中断
        clint.clear_timer_interrupt(0);
        assert!(!clint.is_timer_interrupt_pending(0));
    }

    #[test]
    fn test_clint_software_interrupt() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 初始无中断
        assert!(!clint.is_software_interrupt_pending(0));

        // 设置 MSIP
        clint.write_msip(0, 1);
        assert!(clint.is_software_interrupt_pending(0));

        // 清除中断
        clint.clear_software_interrupt(0);
        assert!(!clint.is_software_interrupt_pending(0));
        assert_eq!(clint.read_msip(0), Some(0));
    }

    #[test]
    fn test_clint_tlm_read_write() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 测试写入 mtime
        let write_data = vec![0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00]; // 0x12345678
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0200_0000 + reg_offset::MTIME,
            write_data,
        );
        let mut delay = ScTime::zero();

        assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());

        // 测试读取 mtime
        let mut read_trans =
            TlmGenericPayload::new(TlmCommand::Read, 0x0200_0000 + reg_offset::MTIME, 4);
        delay = ScTime::zero();

        assert!(clint.b_transport(&mut read_trans, &mut delay).is_ok());

        let read_value = read_trans.data()[0] as u32
            | ((read_trans.data()[1] as u32) << 8)
            | ((read_trans.data()[2] as u32) << 16)
            | ((read_trans.data()[3] as u32) << 24);
        assert_eq!(read_value, 0x12345678);
    }

    #[test]
    fn test_clint_tlm_mtimecmp() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 测试写入 mtimecmp (Hart 1)
        let write_data = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]; // 0x100
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0200_0000 + reg_offset::MTIMECMP_BASE + 8, // Hart 1
            write_data,
        );
        let mut delay = ScTime::zero();

        assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());
        assert_eq!(clint.read_mtimecmp(1), Some(0x100));
    }

    #[test]
    fn test_clint_address_range() {
        let clint = Clint::new(0x0200_0000, 4, 10_000_000);
        let ranges = clint.get_address_ranges();

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0x0200_0000);
        assert_eq!(ranges[0].end, 0x0200_0000 + CLINT_SIZE as u64 - 1);
    }

    #[test]
    fn test_clint_invalid_address() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 访问无效地址
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x0300_0000, 4);
        let mut delay = ScTime::zero();

        assert!(clint.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_clint_update_mtime() {
        let mut clint = Clint::new(0x0200_0000, 1, 1_000_000); // 1MHz

        // 初始时间
        let start_time = ScTime::from_microseconds(100);
        clint.last_update_time = start_time;

        // 更新 1 毫秒
        let current_time = ScTime::from_milliseconds(1);
        clint.update_mtime(current_time);

        // 1MHz 时钟，实际时间差 = 1ms - 100us = 900us = 900 ticks
        assert_eq!(clint.read_mtime(), 900);
    }

    // ==================== 边界测试 ====================

    #[test]
    fn test_clint_invalid_hart_id() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 无效的 Hart ID 应该返回 None
        assert_eq!(clint.read_mtimecmp(100), None);
        assert_eq!(clint.read_msip(100), None);

        // 写入无效 Hart ID 不应该 panic
        clint.write_mtimecmp(100, 0x1234);
        clint.write_msip(100, 0x1);

        // 状态不应该改变
        assert_eq!(clint.read_mtimecmp(0), Some(u64::MAX));
        assert_eq!(clint.read_mtimecmp(1), Some(u64::MAX));
    }

    #[test]
    fn test_clint_address_out_of_bounds() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 地址刚好在边界内应该成功（CLINT_SIZE = 0xC000）
        let mut trans = TlmGenericPayload::new(
            TlmCommand::Read,
            0x0200_0000 + CLINT_SIZE as u64 - 8, // 读取8字节需要地址在 0xBFF8-0xBFFF
            8,
        );
        let mut delay = ScTime::zero();
        // 读取 mtime 寄存器（边界内）
        assert!(clint.b_transport(&mut trans, &mut delay).is_ok());

        // 地址刚好超出边界应该失败
        let mut trans =
            TlmGenericPayload::new(TlmCommand::Read, 0x0200_0000 + CLINT_SIZE as u64, 1);
        delay = ScTime::zero();
        assert!(clint.b_transport(&mut trans, &mut delay).is_err());

        // 地址远远超出边界应该失败
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0xFFFF_0000, 4);
        delay = ScTime::zero();
        assert!(clint.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_clint_max_mtime_value() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 测试最大值 u64::MAX
        clint.write_mtime(u64::MAX);
        assert_eq!(clint.read_mtime(), u64::MAX);

        // mtimecmp = 0, mtime = MAX 应该产生中断
        clint.write_mtimecmp(0, 0);
        assert!(clint.is_timer_interrupt_pending(0));

        // 测试 mtimecmp = MAX
        clint.clear_timer_interrupt(0);
        clint.write_mtimecmp(0, u64::MAX);
        // mtime = MAX >= mtimecmp = MAX，应该产生中断
        assert!(clint.is_timer_interrupt_pending(0));

        // 测试 mtime = 0, mtimecmp = MAX，无中断
        clint.clear_timer_interrupt(0);
        clint.write_mtime(0);
        assert!(!clint.is_timer_interrupt_pending(0));
    }

    #[test]
    fn test_clint_mtime_rollover() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 设置 mtime 接近最大值
        clint.write_mtime(u64::MAX - 100);
        clint.write_mtimecmp(0, u64::MAX);

        // 此时 mtime < mtimecmp，无中断
        assert!(!clint.is_timer_interrupt_pending(0));

        // 测试 update_mtime 的 wrapping_add 行为
        clint.last_update_time = ScTime::zero();
        // 模拟大量时间经过，触发 wrapping_add
        let future_time = ScTime::from_seconds(1); // 1秒 = 10,000,000 ticks at 10MHz
        clint.update_mtime(future_time);

        // mtime 应该回绕 (MAX - 100 + 10_000_000) mod 2^64
        let expected = (u64::MAX - 100).wrapping_add(10_000_000);
        assert_eq!(clint.read_mtime(), expected);
    }

    #[test]
    fn test_clint_invalid_data_length() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 无效的数据长度应该返回错误
        let mut trans = TlmGenericPayload::new(
            TlmCommand::Read,
            0x0200_0000 + reg_offset::MTIME,
            3, // 既不是 4 也不是 8
        );
        let mut delay = ScTime::zero();

        assert!(clint.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_clint_max_harts_boundary() {
        // 测试较大 Hart 数量边界
        // 注意：u32::MAX 会导致内存分配失败，测试合理的较大值即可
        let clint = Clint::new(0x0200_0000, 4096, 10_000_000);
        assert_eq!(clint.num_harts(), 4096);

        // 访问最后一个有效 Hart
        assert_eq!(clint.read_mtimecmp(4095), Some(u64::MAX));
        assert_eq!(clint.read_msip(4095), Some(0));

        // 访问超出范围
        assert_eq!(clint.read_mtimecmp(4096), None);
        assert_eq!(clint.read_msip(4096), None);
    }

    #[test]
    fn test_clint_atomic_mtime_operations() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 测试原子写入
        clint.write_mtime_atomic(0x1234_5678_9ABC_DEF0, Ordering::SeqCst);
        assert_eq!(
            clint.read_mtime_atomic(Ordering::SeqCst),
            0x1234_5678_9ABC_DEF0
        );

        // 测试多线程安全（概念性测试）
        clint.write_mtime(0);
        for i in 0..1000 {
            clint.write_mtime(i);
            assert_eq!(clint.read_mtime(), i);
        }
    }

    #[test]
    fn test_clint_partial_mtime_write() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 先设置完整 64 位值
        clint.write_mtime(0x1234_5678_9ABC_DEF0);

        // 写入低 32 位
        let write_data = vec![0x00, 0x11, 0x22, 0x33];
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0200_0000 + reg_offset::MTIME,
            write_data,
        );
        let mut delay = ScTime::zero();
        assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());

        // 验证只有低 32 位被改变
        let expected = 0x1234_5678_9ABC_DEF0 & !0xFFFF_FFFF | 0x3322_1100;
        assert_eq!(clint.read_mtime(), expected);

        // 写入高 32 位
        let write_data = vec![0x00, 0xAA, 0xBB, 0xCC];
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0200_0000 + reg_offset::MTIME + 4,
            write_data,
        );
        delay = ScTime::zero();
        assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());

        // 验证高 32 位被改变
        let expected = (0xCCBB_AA00u64 << 32) | (expected & 0xFFFF_FFFF);
        assert_eq!(clint.read_mtime(), expected);
    }

    #[test]
    fn test_clint_unaligned_address_access() {
        let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);

        // 未对齐的 MTIMECMP 访问（每个 Hart 8 字节，应该对齐到 8 字节边界）
        // 但我们允许读取任意偏移
        let write_data = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x0200_0000 + reg_offset::MTIMECMP_BASE + 4, // 从第4字节开始
            write_data,
        );
        let mut delay = ScTime::zero();
        assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());

        // 读取 Hart 0 的 mtimecmp（应该从0开始读8字节）
        let mut read_trans =
            TlmGenericPayload::new(TlmCommand::Read, 0x0200_0000 + reg_offset::MTIMECMP_BASE, 8);
        delay = ScTime::zero();
        assert!(clint.b_transport(&mut read_trans, &mut delay).is_ok());

        // 验证数据（低4字节应该是0，因为我们从偏移4写入了0x100）
        // 实际上这会作为 Hart 0 的高4字节写入
        assert_eq!(clint.read_mtimecmp(0), Some(0x100));
    }

    #[test]
    fn test_clint_msip_boundary_bits() {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        // 测试 MSIP 只有 bit 0 有效
        clint.write_msip(0, 0xFFFFFFFF);
        assert_eq!(clint.read_msip(0), Some(0x1));

        // 写入各种边界值
        clint.write_msip(0, 0xFFFFFFFE);
        assert_eq!(clint.read_msip(0), Some(0x0));

        clint.write_msip(0, 0x1);
        assert_eq!(clint.read_msip(0), Some(0x1));
    }

    #[test]
    fn test_clint_zero_harts() {
        // 创建 0 个 Hart 的 CLINT
        let clint = Clint::new(0x0200_0000, 0, 10_000_000);
        assert_eq!(clint.num_harts(), 0);

        // 任何 Hart ID 都应该是无效的
        assert_eq!(clint.read_mtimecmp(0), None);
        assert_eq!(clint.read_msip(0), None);
        assert!(!clint.is_timer_interrupt_pending(0));
        assert!(!clint.is_software_interrupt_pending(0));
    }
}
