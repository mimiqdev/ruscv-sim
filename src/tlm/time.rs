//! TLM2.0 时间管理
//!
//! 实现 SystemC 风格的 sc_time 类型和 TLM 时间操作

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// SystemC 风格的时间单位
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScTimeUnit {
    /// 飞秒 (10^-15 秒)
    Femtosecond,
    /// 皮秒 (10^-12 秒)
    Picosecond,
    /// 纳秒 (10^-9 秒)
    Nanosecond,
    /// 微秒 (10^-6 秒)
    Microsecond,
    /// 毫秒 (10^-3 秒)
    Millisecond,
    /// 秒
    Second,
}

impl ScTimeUnit {
    /// 转换为皮秒的乘数
    const fn to_picoseconds(&self) -> u64 {
        match self {
            ScTimeUnit::Femtosecond => 0, // 小于1ps，舍入为0
            ScTimeUnit::Picosecond => 1,
            ScTimeUnit::Nanosecond => 1_000,
            ScTimeUnit::Microsecond => 1_000_000,
            ScTimeUnit::Millisecond => 1_000_000_000,
            ScTimeUnit::Second => 1_000_000_000_000,
        }
    }

    /// 获取单位的短名称
    pub fn short_name(&self) -> &'static str {
        match self {
            ScTimeUnit::Femtosecond => "fs",
            ScTimeUnit::Picosecond => "ps",
            ScTimeUnit::Nanosecond => "ns",
            ScTimeUnit::Microsecond => "us",
            ScTimeUnit::Millisecond => "ms",
            ScTimeUnit::Second => "s",
        }
    }

    /// 获取单位的完整名称
    pub fn full_name(&self) -> &'static str {
        match self {
            ScTimeUnit::Femtosecond => "femtosecond",
            ScTimeUnit::Picosecond => "picosecond",
            ScTimeUnit::Nanosecond => "nanosecond",
            ScTimeUnit::Microsecond => "microsecond",
            ScTimeUnit::Millisecond => "millisecond",
            ScTimeUnit::Second => "second",
        }
    }
}

impl fmt::Display for ScTimeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short_name())
    }
}

/// SystemC 风格的时间值
/// 
/// 内部以皮秒为单位存储，提供高精度时间表示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScTime {
    /// 时间值（皮秒）
    value_ps: u64,
}

impl ScTime {
    /// 创建新的时间值
    /// 
    /// # 参数
    /// - `value`: 时间数值
    /// - `unit`: 时间单位
    /// 
    /// # 示例
    /// ```
    /// use ruscv_sim::tlm::{ScTime, ScTimeUnit};
    /// 
    /// let t = ScTime::new(100, ScTimeUnit::Nanosecond);
    /// assert_eq!(t.to_picoseconds(), 100_000);
    /// ```
    pub fn new(value: u64, unit: ScTimeUnit) -> Self {
        let value_ps = value.saturating_mul(unit.to_picoseconds());
        Self { value_ps }
    }

    /// 从零皮秒创建时间
    pub const fn zero() -> Self {
        Self { value_ps: 0 }
    }

    /// 从皮秒创建时间
    pub const fn from_picoseconds(ps: u64) -> Self {
        Self { value_ps: ps }
    }

    /// 从纳秒创建时间
    pub const fn from_nanoseconds(ns: u64) -> Self {
        Self { value_ps: ns.saturating_mul(1_000) }
    }

    /// 从微秒创建时间
    pub const fn from_microseconds(us: u64) -> Self {
        Self { value_ps: us.saturating_mul(1_000_000) }
    }

    /// 从毫秒创建时间
    pub const fn from_milliseconds(ms: u64) -> Self {
        Self { value_ps: ms.saturating_mul(1_000_000_000) }
    }

    /// 从秒创建时间
    pub const fn from_seconds(s: u64) -> Self {
        Self { value_ps: s.saturating_mul(1_000_000_000_000) }
    }

    /// 转换为皮秒
    pub const fn to_picoseconds(&self) -> u64 {
        self.value_ps
    }

    /// 转换为纳秒（截断）
    pub const fn to_nanoseconds(&self) -> u64 {
        self.value_ps / 1_000
    }

    /// 转换为微秒（截断）
    pub const fn to_microseconds(&self) -> u64 {
        self.value_ps / 1_000_000
    }

    /// 转换为毫秒（截断）
    pub const fn to_milliseconds(&self) -> u64 {
        self.value_ps / 1_000_000_000
    }

    /// 转换为秒（截断）
    pub const fn to_seconds(&self) -> u64 {
        self.value_ps / 1_000_000_000_000
    }

    /// 转换为浮点秒
    pub fn to_seconds_f64(&self) -> f64 {
        self.value_ps as f64 / 1_000_000_000_000.0
    }

    /// 检查是否为零
    pub const fn is_zero(&self) -> bool {
        self.value_ps == 0
    }

    /// 获取格式化字符串（自动选择单位）
    pub fn format_auto(&self) -> String {
        let ps = self.value_ps;
        if ps >= 1_000_000_000_000 {
            format!("{} s", ps / 1_000_000_000_000)
        } else if ps >= 1_000_000_000 {
            format!("{} ms", ps / 1_000_000_000)
        } else if ps >= 1_000_000 {
            format!("{} us", ps / 1_000_000)
        } else if ps >= 1_000 {
            format!("{} ns", ps / 1_000)
        } else {
            format!("{} ps", ps)
        }
    }
}

impl Default for ScTime {
    fn default() -> Self {
        Self::zero()
    }
}

impl PartialOrd for ScTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.value_ps.cmp(&other.value_ps))
    }
}

impl Ord for ScTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value_ps.cmp(&other.value_ps)
    }
}

impl Add for ScTime {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            value_ps: self.value_ps.saturating_add(rhs.value_ps),
        }
    }
}

impl AddAssign for ScTime {
    fn add_assign(&mut self, rhs: Self) {
        self.value_ps = self.value_ps.saturating_add(rhs.value_ps);
    }
}

impl Sub for ScTime {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            value_ps: self.value_ps.saturating_sub(rhs.value_ps),
        }
    }
}

impl SubAssign for ScTime {
    fn sub_assign(&mut self, rhs: Self) {
        self.value_ps = self.value_ps.saturating_sub(rhs.value_ps);
    }
}

impl fmt::Display for ScTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_auto())
    }
}

/// 兼容旧版本的 TlmTime 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlmTime {
    /// 皮秒
    Ps(u64),
    /// 纳秒
    Ns(u64),
    /// 微秒
    Us(u64),
    /// 毫秒
    Ms(u64),
    /// 秒
    S(u64),
}

impl TlmTime {
    /// 转换为皮秒
    pub fn to_ps(&self) -> u64 {
        match self {
            TlmTime::Ps(v) => *v,
            TlmTime::Ns(v) => v.saturating_mul(1_000),
            TlmTime::Us(v) => v.saturating_mul(1_000_000),
            TlmTime::Ms(v) => v.saturating_mul(1_000_000_000),
            TlmTime::S(v) => v.saturating_mul(1_000_000_000_000),
        }
    }

    /// 转换为 ScTime
    pub fn to_sc_time(&self) -> ScTime {
        ScTime::from_picoseconds(self.to_ps())
    }
}

impl From<TlmTime> for ScTime {
    fn from(t: TlmTime) -> Self {
        t.to_sc_time()
    }
}

impl From<ScTime> for TlmTime {
    fn from(t: ScTime) -> Self {
        TlmTime::Ps(t.to_picoseconds())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sc_time_creation() {
        let t1 = ScTime::new(100, ScTimeUnit::Nanosecond);
        assert_eq!(t1.to_picoseconds(), 100_000);
        assert_eq!(t1.to_nanoseconds(), 100);

        let t2 = ScTime::from_nanoseconds(50);
        assert_eq!(t2.to_picoseconds(), 50_000);

        let t3 = ScTime::from_microseconds(1);
        assert_eq!(t3.to_nanoseconds(), 1_000);
    }

    #[test]
    fn test_sc_time_zero() {
        let t = ScTime::zero();
        assert!(t.is_zero());
        assert_eq!(t.to_picoseconds(), 0);
    }

    #[test]
    fn test_sc_time_arithmetic() {
        let t1 = ScTime::from_nanoseconds(100);
        let t2 = ScTime::from_nanoseconds(50);

        let t3 = t1 + t2;
        assert_eq!(t3.to_nanoseconds(), 150);

        let t4 = t1 - t2;
        assert_eq!(t4.to_nanoseconds(), 50);
    }

    #[test]
    fn test_sc_time_comparison() {
        let t1 = ScTime::from_nanoseconds(100);
        let t2 = ScTime::from_nanoseconds(50);
        let t3 = ScTime::from_nanoseconds(100);

        assert!(t1 > t2);
        assert!(t2 < t1);
        assert_eq!(t1, t3);
    }

    #[test]
    fn test_sc_time_format() {
        let t1 = ScTime::from_picoseconds(500);
        assert_eq!(t1.format_auto(), "500 ps");

        let t2 = ScTime::from_nanoseconds(10);
        assert_eq!(t2.format_auto(), "10 ns");

        let t3 = ScTime::from_microseconds(5);
        assert_eq!(t3.format_auto(), "5 us");
    }

    #[test]
    fn test_time_unit() {
        assert_eq!(ScTimeUnit::Nanosecond.short_name(), "ns");
        assert_eq!(ScTimeUnit::Microsecond.full_name(), "microsecond");
        assert_eq!(ScTimeUnit::Picosecond.to_picoseconds(), 1);
        assert_eq!(ScTimeUnit::Nanosecond.to_picoseconds(), 1_000);
    }

    #[test]
    fn test_tlm_time_compat() {
        let t = TlmTime::Ns(100);
        assert_eq!(t.to_ps(), 100_000);

        let sc: ScTime = t.into();
        assert_eq!(sc.to_nanoseconds(), 100);

        let t2: TlmTime = sc.into();
        assert_eq!(t2.to_ps(), 100_000);
    }
}
