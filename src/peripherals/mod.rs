//! 外设模块
//!
//! 实现 RISC-V 平台常用的外设：
//!
//! - [`clint`]: Core Local Interruptor (本地定时器和软件中断)
//! - [`plic`]: Platform-Level Interrupt Controller (平台级中断控制器)
//! - [`uart16550`]: UART 16550 串口控制器
//!
//! # 示例
//!
//! ```
//! use ruscv_sim::peripherals::{Clint, Plic, Uart16550};
//! use ruscv_sim::tlm::{TlmBus, ArbitrationPolicy, AddressRange};
//! use std::sync::{Arc, Mutex};
//!
//! // 创建外设
//! let clint = Arc::new(Mutex::new(Clint::new(0x0200_0000, 4, 10_000_000)));
//! let plic = Arc::new(Mutex::new(Plic::new(0x0C00_0000, 32, 8)));
//! let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
//!
//! // 添加到总线
//! let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
//! bus.add_route(AddressRange::new(0x0200_0000, 0x0200_BFFF), clint, 0, "clint");
//! bus.add_route(AddressRange::new(0x0C00_0000, 0x0C3F_FFFF), plic, 0, "plic");
//! bus.add_route(AddressRange::new(0x1000_0000, 0x1000_0007), uart, 0, "uart");
//! ```

// 子模块声明
pub mod clint;
pub mod plic;
pub mod uart16550;

// 公开导出
pub use clint::{Clint, CLINT_SIZE};
pub use plic::{Plic, PLIC_SIZE, MAX_INTERRUPT_SOURCES, MAX_PRIORITY};
pub use uart16550::{Uart16550, UART_SIZE, FIFO_DEPTH};

/// 外设错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeripheralError {
    /// 无效地址
    InvalidAddress(u64),
    /// 无效参数
    InvalidParameter(String),
    /// 访问权限错误
    AccessDenied,
    /// 未实现的功能
    NotImplemented,
    /// 设备忙
    Busy,
    /// 超时
    Timeout,
}

impl std::fmt::Display for PeripheralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeripheralError::InvalidAddress(addr) => {
                write!(f, "Invalid peripheral address: 0x{:016x}", addr)
            }
            PeripheralError::InvalidParameter(param) => {
                write!(f, "Invalid parameter: {}", param)
            }
            PeripheralError::AccessDenied => write!(f, "Access denied"),
            PeripheralError::NotImplemented => write!(f, "Feature not implemented"),
            PeripheralError::Busy => write!(f, "Peripheral busy"),
            PeripheralError::Timeout => write!(f, "Operation timeout"),
        }
    }
}

impl std::error::Error for PeripheralError {}

/// 外设 trait
/// 
/// 定义外设的通用接口
pub trait Peripheral: Send {
    /// 获取外设名称
    fn name(&self) -> &str;
    /// 获取基地址
    fn base_addr(&self) -> u64;
    /// 获取内存映射大小
    fn size(&self) -> usize;
    /// 复位外设
    fn reset(&mut self);
    /// 检查是否产生中断
    fn interrupt_pending(&self) -> bool;
    /// 获取中断号（如果有）
    fn interrupt_id(&self) -> Option<u32>;
}

/// 标准 RISC-V 平台外设配置
/// 
/// 提供常见 RISC-V 平台的默认外设配置
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    /// CLINT 基地址
    pub clint_base: u64,
    /// PLIC 基地址
    pub plic_base: u64,
    /// UART 基地址
    pub uart_base: u64,
    /// Hart 数量
    pub num_harts: u32,
    /// PLIC 中断源数量
    pub plic_sources: u32,
}

impl PlatformConfig {
    /// SiFive HiFive1 配置
    pub fn hifive1() -> Self {
        Self {
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
            uart_base: 0x1001_3000,
            num_harts: 1,
            plic_sources: 52,
        }
    }

    /// QEMU Virt 配置
    pub fn qemu_virt() -> Self {
        Self {
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
            uart_base: 0x1000_0000,
            num_harts: 4,
            plic_sources: 96,
        }
    }

    /// 创建默认配置（单核）
    pub fn default_single_hart() -> Self {
        Self {
            clint_base: 0x0200_0000,
            plic_base: 0x0C00_0000,
            uart_base: 0x1000_0000,
            num_harts: 1,
            plic_sources: 32,
        }
    }
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self::default_single_hart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tlm::{
        AddressRange, ArbitrationPolicy, ScTime, TlmBus, TlmCommand, TlmGenericPayload,
        TlmInitiator,
    };
    use std::sync::{Arc, Mutex};

    /// 测试平台配置
    #[test]
    fn test_platform_config() {
        let hifive1 = PlatformConfig::hifive1();
        assert_eq!(hifive1.clint_base, 0x0200_0000);
        assert_eq!(hifive1.plic_base, 0x0C00_0000);
        assert_eq!(hifive1.num_harts, 1);

        let qemu = PlatformConfig::qemu_virt();
        assert_eq!(qemu.num_harts, 4);
        assert_eq!(qemu.plic_sources, 96);
    }

    /// 测试完整外设系统集成
    #[test]
    fn test_peripheral_integration() {
        let config = PlatformConfig::default_single_hart();
        
        // 创建外设
        let clint = Arc::new(Mutex::new(Clint::new(
            config.clint_base,
            config.num_harts,
            10_000_000,
        )));
        let plic = Arc::new(Mutex::new(Plic::new(
            config.plic_base,
            config.plic_sources,
            config.num_harts * 2, // M-mode + S-mode
        )));
        let uart = Arc::new(Mutex::new(Uart16550::new(config.uart_base)));

        // 创建总线并添加路由
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        bus.add_route(
            AddressRange::new(config.clint_base, config.clint_base + CLINT_SIZE as u64 - 1),
            clint.clone(),
            0,
            "clint",
        );
        bus.add_route(
            AddressRange::new(config.plic_base, config.plic_base + PLIC_SIZE as u64 - 1),
            plic.clone(),
            0,
            "plic",
        );
        bus.add_route(
            AddressRange::new(config.uart_base, config.uart_base + UART_SIZE as u64 - 1),
            uart.clone(),
            0,
            "uart",
        );

        // 测试 CLINT 访问
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            config.clint_base + 0x4000, // mtimecmp
            vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
        let mut delay = ScTime::zero();
        assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());

        // 测试 PLIC 访问
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            config.plic_base + 4, // 中断源 1 优先级
            vec![0x05, 0x00, 0x00, 0x00],
        );
        delay = ScTime::zero();
        assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());

        // 测试 UART 访问
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            config.uart_base + 1, // IER
            vec![0x01],
        );
        delay = ScTime::zero();
        assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());
    }

    /// 测试 CLINT 和 PLIC 中断交互
    #[test]
    fn test_interrupt_integration() {
        use crate::peripherals::clint::reg_offset as clint_offset;
        use crate::peripherals::plic::reg_offset as plic_offset;

        let config = PlatformConfig::default_single_hart();
        
        let clint = Arc::new(Mutex::new(Clint::new(
            config.clint_base,
            config.num_harts,
            10_000_000,
        )));
        let plic = Arc::new(Mutex::new(Plic::new(
            config.plic_base,
            32,
            2,
        )));

        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        bus.add_route(
            AddressRange::new(config.clint_base, config.clint_base + CLINT_SIZE as u64 - 1),
            clint.clone(),
            0,
            "clint",
        );
        bus.add_route(
            AddressRange::new(config.plic_base, config.plic_base + PLIC_SIZE as u64 - 1),
            plic.clone(),
            0,
            "plic",
        );

        // 配置 CLINT 定时器中断
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            config.clint_base + clint_offset::MTIMECMP_BASE,
            vec![0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // mtimecmp = 100
        );
        let mut delay = ScTime::zero();
        bus.b_transport(&mut write_trans, &mut delay).unwrap();

        // 设置 mtime 触发中断
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            config.clint_base + clint_offset::MTIME,
            vec![0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // mtime = 100
        );
        delay = ScTime::zero();
        bus.b_transport(&mut write_trans, &mut delay).unwrap();

        // 验证 CLINT 中断
        {
            let clint_guard = clint.lock().unwrap();
            assert!(clint_guard.is_timer_interrupt_pending(0));
        }

        // 配置 PLIC 中断
        {
            let mut plic_guard = plic.lock().unwrap();
            plic_guard.write_priority(10, 5);
            plic_guard.write_enable(0, 0, 1 << 10);
            plic_guard.trigger_interrupt(10);
        }

        // 声明 PLIC 中断（读取上下文 0 的 claim/complete 寄存器）
        let mut read_trans = TlmGenericPayload::new(
            TlmCommand::Read,
            config.plic_base + plic_offset::CLAIM_COMPLETE_BASE,
            4,
        );
        delay = ScTime::zero();
        bus.b_transport(&mut read_trans, &mut delay).unwrap();

        let irq_id = read_trans.data()[0] as u32;
        assert_eq!(irq_id, 10);
    }

    /// 测试 UART 通过 PLIC 产生中断
    #[test]
    fn test_uart_plic_integration() {
        let config = PlatformConfig::default_single_hart();
        
        let uart = Arc::new(Mutex::new(Uart16550::new(config.uart_base)));
        let plic = Arc::new(Mutex::new(Plic::new(config.plic_base, 32, 2)));

        // 设置 UART 中断回调，触发 PLIC 中断
        {
            let plic_clone = plic.clone();
            let mut uart_guard = uart.lock().unwrap();
            uart_guard.set_interrupt_callback(move || {
                let mut plic_guard = plic_clone.lock().unwrap();
                plic_guard.trigger_interrupt(10); // UART 中断源 ID
            });
        }

        // 配置 UART 产生中断
        {
            let mut uart_guard = uart.lock().unwrap();
            uart_guard.write_reg(uart16550::reg_offset::MCR, uart16550::mcr_bits::OUT2);
            uart_guard.write_reg(uart16550::reg_offset::IER, uart16550::ier_bits::ERBFI);
        }

        // 配置 PLIC
        {
            let mut plic_guard = plic.lock().unwrap();
            plic_guard.write_priority(10, 5);
            plic_guard.write_enable(0, 0, 1 << 10);
        }

        // UART 接收数据，应该触发 PLIC 中断
        {
            let mut uart_guard = uart.lock().unwrap();
            uart_guard.receive_byte(0x41);
        }

        // 验证 PLIC 中有挂起的中断
        let plic_guard = plic.lock().unwrap();
        assert!(plic_guard.is_pending(10));
    }

    /// 测试外设错误类型
    #[test]
    fn test_peripheral_error() {
        let err = PeripheralError::InvalidAddress(0x1234);
        assert!(err.to_string().contains("0x0000000000001234"));

        let err = PeripheralError::InvalidParameter("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = PeripheralError::AccessDenied;
        assert!(err.to_string().contains("Access denied"));
    }
}
