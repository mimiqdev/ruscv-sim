//! UART 16550 实现
//!
//! 实现标准的 NS16550D UART 控制器，提供：
//! - 收发 FIFO
//! - 可编程波特率
//! - 中断支持
//! - 流控支持（RTS/CTS）

use crate::tlm::{
    AddressRange, ScTime, TlmCommand, TlmError, TlmGenericPayload, TlmResponseStatus, TlmTarget,
};

/// UART 寄存器偏移
pub mod reg_offset {
    /// 接收缓冲寄存器（读）/ 发送保持寄存器（写）
    pub const RBR_THR: u64 = 0x00;
    /// 中断使能寄存器
    pub const IER: u64 = 0x01;
    /// 中断标识寄存器（读）/ FIFO 控制寄存器（写）
    pub const IIR_FCR: u64 = 0x02;
    /// 线路控制寄存器
    pub const LCR: u64 = 0x03;
    /// 调制解调器控制寄存器
    pub const MCR: u64 = 0x04;
    /// 线路状态寄存器
    pub const LSR: u64 = 0x05;
    /// 调制解调器状态寄存器
    pub const MSR: u64 = 0x06;
    /// 暂存寄存器
    pub const SCR: u64 = 0x07;
    /// 除数锁存器低字节（DLAB=1）
    pub const DLL: u64 = 0x00;
    /// 除数锁存器高字节（DLAB=1）
    pub const DLM: u64 = 0x01;
}

/// UART 默认内存映射大小
pub const UART_SIZE: usize = 8;

/// FIFO 深度
pub const FIFO_DEPTH: usize = 16;

/// 线路状态寄存器位
pub mod lsr_bits {
    /// 数据就绪
    pub const DR: u8 = 0x01;
    /// 溢出错误
    pub const OE: u8 = 0x02;
    /// 奇偶校验错误
    pub const PE: u8 = 0x04;
    /// 帧错误
    pub const FE: u8 = 0x08;
    /// 间隔信号检测
    pub const BI: u8 = 0x10;
    /// 发送保持寄存器空
    pub const THRE: u8 = 0x20;
    /// 发送器空
    pub const TEMT: u8 = 0x40;
    /// 接收 FIFO 错误
    pub const RFE: u8 = 0x80;
}

/// 中断使能寄存器位
pub mod ier_bits {
    /// 接收数据可用中断
    pub const ERBFI: u8 = 0x01;
    /// 发送保持寄存器空中断
    pub const ETBEI: u8 = 0x02;
    /// 接收线路状态中断
    pub const ELSI: u8 = 0x04;
    /// 调制解调器状态中断
    pub const EDSSI: u8 = 0x08;
}

/// 中断标识寄存器编码
pub mod iir_bits {
    /// 中断挂起（0=有中断，1=无中断）
    pub const NO_INT: u8 = 0x01;
    /// 中断 ID 位
    pub const ID_MASK: u8 = 0x0E;
    /// FIFO 使能位
    pub const FIFO_EN: u8 = 0xC0;
    
    /// 中断类型
    pub const MODEM_STATUS: u8 = 0x00;
    pub const TRANSMIT_EMPTY: u8 = 0x02;
    pub const RECEIVE_DATA: u8 = 0x04;
    pub const LINE_STATUS: u8 = 0x06;
    pub const CHARACTER_TIMEOUT: u8 = 0x0C;
}

/// FIFO 控制寄存器位
pub mod fcr_bits {
    /// FIFO 使能
    pub const FIFO_ENABLE: u8 = 0x01;
    /// 接收 FIFO 复位
    pub const RCVR_FIFO_RESET: u8 = 0x02;
    /// 发送 FIFO 复位
    pub const XMIT_FIFO_RESET: u8 = 0x04;
    /// DMA 模式选择
    pub const DMA_MODE_SELECT: u8 = 0x08;
    /// 接收触发级别（00=1, 01=4, 10=8, 11=14）
    pub const RCVR_TRIGGER_LSB: u8 = 0x40;
    pub const RCVR_TRIGGER_MSB: u8 = 0x80;
}

/// 线路控制寄存器位
pub mod lcr_bits {
    /// 字长度选择位 0
    pub const WLS0: u8 = 0x01;
    /// 字长度选择位 1
    pub const WLS1: u8 = 0x02;
    /// 停止位数量
    pub const STB: u8 = 0x04;
    /// 奇偶校验使能
    pub const PEN: u8 = 0x08;
    /// 偶校验选择
    pub const EPS: u8 = 0x10;
    /// 强制奇偶校验
    pub const SP: u8 = 0x20;
    /// 设置间隔
    pub const SB: u8 = 0x40;
    /// 除数锁存访问位
    pub const DLAB: u8 = 0x80;
}

/// 调制解调器控制寄存器位
pub mod mcr_bits {
    /// DTR
    pub const DTR: u8 = 0x01;
    /// RTS
    pub const RTS: u8 = 0x02;
    /// OUT1
    pub const OUT1: u8 = 0x04;
    /// OUT2（通常用于中断使能）
    pub const OUT2: u8 = 0x08;
    /// 环回模式
    pub const LOOP: u8 = 0x10;
}

/// UART 16550 外设
pub struct Uart16550 {
    /// 基地址
    base_addr: u64,
    /// 接收 FIFO
    pub rx_fifo: Vec<u8>,
    /// 发送 FIFO
    pub tx_fifo: Vec<u8>,
    /// 中断使能寄存器
    ier: u8,
    /// FIFO 控制寄存器
    fcr: u8,
    /// 线路控制寄存器
    lcr: u8,
    /// 调制解调器控制寄存器
    mcr: u8,
    /// 线路状态寄存器
    lsr: u8,
    /// 调制解调器状态寄存器
    msr: u8,
    /// 暂存寄存器
    scr: u8,
    /// 除数锁存器低字节
    dll: u8,
    /// 除数锁存器高字节
    dlm: u8,
    /// 接收触发级别
    rx_trigger: u8,
    /// 字节接收回调
    rx_callback: Option<Box<dyn FnMut(u8) + Send + Sync>>,
    /// 中断回调
    interrupt_callback: Option<Box<dyn FnMut() + Send + Sync>>,
    /// 输出回调（用于发送数据到外部）
    output_callback: Option<Box<dyn FnMut(u8) + Send + Sync>>,
}

impl std::fmt::Debug for Uart16550 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Uart16550")
            .field("base_addr", &self.base_addr)
            .field("rx_fifo", &self.rx_fifo)
            .field("tx_fifo", &self.tx_fifo)
            .field("ier", &self.ier)
            .field("fcr", &self.fcr)
            .field("lcr", &self.lcr)
            .field("mcr", &self.mcr)
            .field("lsr", &self.lsr)
            .field("msr", &self.msr)
            .field("scr", &self.scr)
            .field("dll", &self.dll)
            .field("dlm", &self.dlm)
            .field("rx_trigger", &self.rx_trigger)
            .field("rx_callback", &self.rx_callback.is_some())
            .field("interrupt_callback", &self.interrupt_callback.is_some())
            .field("output_callback", &self.output_callback.is_some())
            .finish()
    }
}

impl Uart16550 {
    /// 创建新的 UART 16550 实例
    ///
    /// # 参数
    /// - `base_addr`: UART 基地址
    ///
    /// # 示例
    /// ```
    /// use ruscv_sim::peripherals::Uart16550;
    ///
    /// let uart = Uart16550::new(0x1000_0000);
    /// ```
    pub fn new(base_addr: u64) -> Self {
        let mut uart = Self {
            base_addr,
            rx_fifo: Vec::with_capacity(FIFO_DEPTH),
            tx_fifo: Vec::with_capacity(FIFO_DEPTH),
            ier: 0,
            fcr: 0,
            lcr: 0,
            mcr: 0,
            lsr: lsr_bits::THRE | lsr_bits::TEMT, // 初始发送器空
            msr: 0,
            scr: 0,
            dll: 0,
            dlm: 0,
            rx_trigger: 1,
            rx_callback: None,
            interrupt_callback: None,
            output_callback: None,
        };
        uart.update_lsr();
        uart
    }

    /// 获取基地址
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// 检查是否启用 FIFO
    pub fn fifo_enabled(&self) -> bool {
        (self.fcr & fcr_bits::FIFO_ENABLE) != 0
    }

    /// 检查 DLAB
    pub fn dlab(&self) -> bool {
        (self.lcr & lcr_bits::DLAB) != 0
    }

    /// 更新线路状态寄存器
    fn update_lsr(&mut self) {
        // 数据就绪位
        if !self.rx_fifo.is_empty() {
            self.lsr |= lsr_bits::DR;
        } else {
            self.lsr &= !lsr_bits::DR;
        }

        // 发送器空位
        if self.tx_fifo.is_empty() {
            self.lsr |= lsr_bits::TEMT;
        } else {
            self.lsr &= !lsr_bits::TEMT;
        }

        // 发送保持寄存器空位
        if self.tx_fifo.len() < FIFO_DEPTH {
            self.lsr |= lsr_bits::THRE;
        } else {
            self.lsr &= !lsr_bits::THRE;
        }
    }

    /// 检查是否有中断挂起
    pub fn interrupt_pending(&self) -> bool {
        // OUT2 必须置位才允许中断
        if (self.mcr & mcr_bits::OUT2) == 0 {
            return false;
        }

        // 线路状态中断
        if (self.ier & ier_bits::ELSI) != 0 && (self.lsr & 0x1E) != 0 {
            return true;
        }

        // 接收数据可用中断
        if (self.ier & ier_bits::ERBFI) != 0 && (self.lsr & lsr_bits::DR) != 0 {
            return true;
        }

        // 发送保持寄存器空中断
        if (self.ier & ier_bits::ETBEI) != 0 && (self.lsr & lsr_bits::THRE) != 0 {
            return true;
        }

        // 调制解调器状态中断
        if (self.ier & ier_bits::EDSSI) != 0 {
            // 简化实现：假设有调制解调器状态变化
            return false;
        }

        false
    }

    /// 获取中断标识
    pub fn interrupt_id(&self) -> u8 {
        if !self.interrupt_pending() {
            return iir_bits::NO_INT;
        }

        let mut iir = 0u8;

        // 设置 FIFO 使能位
        if self.fifo_enabled() {
            iir |= iir_bits::FIFO_EN;
        }

        // 优先级：线路状态 > 接收数据 > 发送空 > 调制解调器状态
        if (self.ier & ier_bits::ELSI) != 0 && (self.lsr & 0x1E) != 0 {
            iir |= iir_bits::LINE_STATUS;
        } else if (self.ier & ier_bits::ERBFI) != 0 && (self.lsr & lsr_bits::DR) != 0 {
            if self.fifo_enabled()
                && self.rx_fifo.len() >= self.rx_trigger as usize
                && self.rx_trigger == 14
            {
                iir |= iir_bits::CHARACTER_TIMEOUT;
            } else {
                iir |= iir_bits::RECEIVE_DATA;
            }
        } else if (self.ier & ier_bits::ETBEI) != 0 && (self.lsr & lsr_bits::THRE) != 0 {
            iir |= iir_bits::TRANSMIT_EMPTY;
        } else if (self.ier & ier_bits::EDSSI) != 0 {
            iir |= iir_bits::MODEM_STATUS;
        } else {
            iir |= iir_bits::NO_INT;
        }

        iir
    }

    /// 接收一个字节（外部调用）
    pub fn receive_byte(&mut self, byte: u8) {
        if self.rx_fifo.len() < FIFO_DEPTH {
            self.rx_fifo.push(byte);
            self.update_lsr();
            self.check_interrupt();
        } else {
            // FIFO 溢出
            self.lsr |= lsr_bits::OE;
            if self.fifo_enabled() {
                self.lsr |= lsr_bits::RFE;
            }
        }
    }

    /// 读取接收缓冲寄存器
    fn read_rbr(&mut self) -> u8 {
        if !self.rx_fifo.is_empty() {
            let byte = self.rx_fifo.remove(0);
            self.update_lsr();
            self.check_interrupt();
            byte
        } else {
            0
        }
    }

    /// 写入发送保持寄存器
    fn write_thr(&mut self, byte: u8) {
        // 调用输出回调
        if let Some(ref mut cb) = self.output_callback {
            cb(byte);
        }

        if self.tx_fifo.len() < FIFO_DEPTH {
            self.tx_fifo.push(byte);
            self.update_lsr();
            self.check_interrupt();
        }
    }

    /// 检查并触发中断
    fn check_interrupt(&mut self) {
        if self.interrupt_pending() {
            if let Some(ref mut cb) = self.interrupt_callback {
                cb();
            }
        }
    }

    /// 设置字节接收回调
    pub fn set_rx_callback(&mut self, cb: impl FnMut(u8) + Send + Sync + 'static) {
        self.rx_callback = Some(Box::new(cb));
    }

    /// 设置中断回调
    pub fn set_interrupt_callback(&mut self, cb: impl FnMut() + Send + Sync + 'static) {
        self.interrupt_callback = Some(Box::new(cb));
    }

    /// 设置输出回调（发送数据时调用）
    pub fn set_output_callback(&mut self, cb: impl FnMut(u8) + Send + Sync + 'static) {
        self.output_callback = Some(Box::new(cb));
    }

    /// 获取波特率
    pub fn baud_rate(&self) -> u32 {
        let divisor = ((self.dlm as u16) << 8) | (self.dll as u16);
        if divisor == 0 {
            0
        } else {
            // 假设基频 1.8432 MHz
            1_843_200 / (divisor as u32 * 16)
        }
    }

    /// 获取接收 FIFO 数据（用于测试）
    pub fn rx_fifo_data(&self) -> &[u8] {
        &self.rx_fifo
    }

    /// 获取发送 FIFO 数据（用于测试）
    pub fn tx_fifo_data(&self) -> &[u8] {
        &self.tx_fifo
    }

    /// 清空接收 FIFO
    pub fn clear_rx_fifo(&mut self) {
        self.rx_fifo.clear();
        self.update_lsr();
    }

    /// 清空发送 FIFO
    pub fn clear_tx_fifo(&mut self) {
        self.tx_fifo.clear();
        self.update_lsr();
    }

    /// 读取寄存器
    pub fn read_reg(&mut self, offset: u64) -> u8 {
        match offset {
            reg_offset::RBR_THR => {
                if self.dlab() {
                    self.dll
                } else {
                    self.read_rbr()
                }
            }
            reg_offset::IER => {
                if self.dlab() {
                    self.dlm
                } else {
                    self.ier
                }
            }
            reg_offset::IIR_FCR => self.interrupt_id(),
            reg_offset::LCR => self.lcr,
            reg_offset::MCR => self.mcr,
            reg_offset::LSR => {
                let lsr = self.lsr;
                // 读取 LSR 清除某些位
                self.lsr &= !(lsr_bits::OE | lsr_bits::PE | lsr_bits::FE | lsr_bits::BI);
                lsr
            }
            reg_offset::MSR => {
                // 简化实现，返回固定值
                0x00
            }
            reg_offset::SCR => self.scr,
            _ => 0,
        }
    }

    /// 写入寄存器
    pub fn write_reg(&mut self, offset: u64, value: u8) {
        match offset {
            reg_offset::RBR_THR => {
                if self.dlab() {
                    self.dll = value;
                } else {
                    self.write_thr(value);
                }
            }
            reg_offset::IER => {
                if self.dlab() {
                    self.dlm = value;
                } else {
                    self.ier = value & 0x0F;
                    self.check_interrupt();
                }
            }
            reg_offset::IIR_FCR => {
                // FCR 是只写的
                self.fcr = value;
                
                // 处理 FIFO 复位
                if value & fcr_bits::RCVR_FIFO_RESET != 0 {
                    self.rx_fifo.clear();
                    self.update_lsr();
                }
                if value & fcr_bits::XMIT_FIFO_RESET != 0 {
                    self.tx_fifo.clear();
                    self.update_lsr();
                }
                
                // 接收触发级别
                let trigger = (value & (fcr_bits::RCVR_TRIGGER_MSB | fcr_bits::RCVR_TRIGGER_LSB)) >> 6;
                self.rx_trigger = match trigger {
                    0 => 1,
                    1 => 4,
                    2 => 8,
                    3 => 14,
                    _ => 1,
                };
            }
            reg_offset::LCR => {
                self.lcr = value;
            }
            reg_offset::MCR => {
                self.mcr = value;
                self.check_interrupt(); // OUT2 变化可能影响中断
            }
            reg_offset::LSR => {
                // LSR 是只读的，忽略写入
            }
            reg_offset::MSR => {
                // MSR 是只读的，忽略写入
            }
            reg_offset::SCR => {
                self.scr = value;
            }
            _ => {}
        }
    }
}

impl TlmTarget for Uart16550 {
    fn b_transport(
        &mut self,
        trans: &mut TlmGenericPayload,
        _delay: &mut ScTime,
    ) -> Result<(), TlmError> {
        let addr = trans.address();
        
        // 检查地址范围
        if addr < self.base_addr || addr >= self.base_addr + UART_SIZE as u64 {
            trans.set_response_status(TlmResponseStatus::InvalidAddress);
            return Err(TlmError::InvalidAddress64(addr));
        }

        let offset = addr - self.base_addr;

        match trans.command() {
            TlmCommand::Read => {
                let value = self.read_reg(offset);
                trans.data_mut()[0] = value;
                trans.set_response_status(TlmResponseStatus::Ok);
            }
            TlmCommand::Write => {
                let value = trans.data().get(0).copied().unwrap_or(0);
                self.write_reg(offset, value);
                trans.set_response_status(TlmResponseStatus::Ok);
            }
        }

        Ok(())
    }

    fn get_address_ranges(&self) -> Vec<AddressRange> {
        vec![AddressRange::new(
            self.base_addr,
            self.base_addr + UART_SIZE as u64 - 1,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_creation() {
        let uart = Uart16550::new(0x1000_0000);
        assert_eq!(uart.base_addr(), 0x1000_0000);
        assert!(!uart.fifo_enabled());
    }

    #[test]
    fn test_uart_fifo_enable() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 初始 FIFO 禁用
        assert!(!uart.fifo_enabled());
        
        // 使能 FIFO
        uart.write_reg(reg_offset::IIR_FCR, fcr_bits::FIFO_ENABLE);
        assert!(uart.fifo_enabled());
    }

    #[test]
    fn test_uart_receive() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 接收数据
        uart.receive_byte(0x41); // 'A'
        uart.receive_byte(0x42); // 'B'
        
        // 检查 FIFO
        assert_eq!(uart.rx_fifo_data(), &[0x41, 0x42]);
        
        // 检查 LSR
        assert!((uart.lsr & lsr_bits::DR) != 0);
    }

    #[test]
    fn test_uart_read_rbr() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 接收数据
        uart.receive_byte(0x41);
        uart.receive_byte(0x42);
        
        // 读取 RBR
        let byte1 = uart.read_reg(reg_offset::RBR_THR);
        assert_eq!(byte1, 0x41);
        
        let byte2 = uart.read_reg(reg_offset::RBR_THR);
        assert_eq!(byte2, 0x42);
    }

    #[test]
    fn test_uart_write_thr() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 写入 THR
        uart.write_reg(reg_offset::RBR_THR, 0x41);
        uart.write_reg(reg_offset::RBR_THR, 0x42);
        
        // 检查发送 FIFO
        assert_eq!(uart.tx_fifo_data(), &[0x41, 0x42]);
    }

    #[test]
    fn test_uart_dlab() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 设置 DLAB
        uart.write_reg(reg_offset::LCR, lcr_bits::DLAB);
        assert!(uart.dlab());
        
        // DLAB=1 时，访问 DLL/DLM
        uart.write_reg(reg_offset::RBR_THR, 0x0C); // DLL
        uart.write_reg(reg_offset::IER, 0x00);     // DLM
        
        assert_eq!(uart.read_reg(reg_offset::RBR_THR), 0x0C);
        assert_eq!(uart.read_reg(reg_offset::IER), 0x00);
        
        // 清除 DLAB
        uart.write_reg(reg_offset::LCR, 0);
        assert!(!uart.dlab());
    }

    #[test]
    fn test_uart_baud_rate() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 设置 DLAB
        uart.write_reg(reg_offset::LCR, lcr_bits::DLAB);
        
        // 设置除数 = 12 (9600 bps with 1.8432MHz)
        uart.write_reg(reg_offset::RBR_THR, 12); // DLL
        uart.write_reg(reg_offset::IER, 0);      // DLM
        
        assert_eq!(uart.baud_rate(), 9600);
    }

    #[test]
    fn test_uart_interrupt_enable() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 初始无中断使能
        assert!(!uart.interrupt_pending());
        
        // 使能接收中断，但 OUT2 未使能
        uart.write_reg(reg_offset::IER, ier_bits::ERBFI);
        assert!(!uart.interrupt_pending());
        
        // 使能 OUT2
        uart.write_reg(reg_offset::MCR, mcr_bits::OUT2);
        
        // 接收数据
        uart.receive_byte(0x41);
        
        // 现在应该有中断
        assert!(uart.interrupt_pending());
    }

    #[test]
    fn test_uart_interrupt_id() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 无中断
        assert_eq!(uart.interrupt_id() & iir_bits::NO_INT, iir_bits::NO_INT);
        
        // 配置并触发接收中断
        uart.write_reg(reg_offset::MCR, mcr_bits::OUT2);
        uart.write_reg(reg_offset::IER, ier_bits::ERBFI);
        uart.receive_byte(0x41);
        
        let iir = uart.interrupt_id();
        assert_eq!(iir & iir_bits::NO_INT, 0); // 有中断
        assert_eq!(iir & iir_bits::ID_MASK, iir_bits::RECEIVE_DATA);
    }

    #[test]
    fn test_uart_fifo_reset() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 填充 FIFO
        uart.receive_byte(0x41);
        uart.receive_byte(0x42);
        uart.write_reg(reg_offset::RBR_THR, 0x43);
        
        // 复位接收 FIFO
        uart.write_reg(reg_offset::IIR_FCR, fcr_bits::FIFO_ENABLE | fcr_bits::RCVR_FIFO_RESET);
        assert!(uart.rx_fifo.is_empty());
        
        // 复位发送 FIFO
        uart.write_reg(reg_offset::IIR_FCR, fcr_bits::FIFO_ENABLE | fcr_bits::XMIT_FIFO_RESET);
        assert!(uart.tx_fifo.is_empty());
    }

    #[test]
    fn test_uart_tlm_read_write() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 写入 IER
        let write_data = vec![ier_bits::ERBFI];
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x1000_0001, // IER 偏移
            write_data,
        );
        let mut delay = ScTime::zero();
        
        assert!(uart.b_transport(&mut write_trans, &mut delay).is_ok());
        
        // 读取 IER
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000_0001, 1);
        delay = ScTime::zero();
        
        assert!(uart.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data()[0], ier_bits::ERBFI);
    }

    #[test]
    fn test_uart_tlm_receive() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 接收数据
        uart.receive_byte(0x42);
        
        // 读取 RBR
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000_0000, 1);
        let mut delay = ScTime::zero();
        
        assert!(uart.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data()[0], 0x42);
    }

    #[test]
    fn test_uart_tlm_transmit() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 写入 THR
        let write_data = vec![0x55];
        let mut write_trans = TlmGenericPayload::with_data(
            TlmCommand::Write,
            0x1000_0000, // THR 偏移
            write_data,
        );
        let mut delay = ScTime::zero();
        
        assert!(uart.b_transport(&mut write_trans, &mut delay).is_ok());
        assert_eq!(uart.tx_fifo_data(), &[0x55]);
    }

    #[test]
    fn test_uart_lsr_read_clears_errors() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 模拟错误
        uart.lsr |= lsr_bits::OE | lsr_bits::PE;
        
        // 读取 LSR
        let _ = uart.read_reg(reg_offset::LSR);
        
        // 错误位被清除
        assert!((uart.lsr & (lsr_bits::OE | lsr_bits::PE)) == 0);
    }

    #[test]
    fn test_uart_address_range() {
        let uart = Uart16550::new(0x1000_0000);
        let ranges = uart.get_address_ranges();
        
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0x1000_0000);
        assert_eq!(ranges[0].end, 0x1000_0007);
    }

    #[test]
    fn test_uart_invalid_address() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 访问无效地址
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x2000_0000, 1);
        let mut delay = ScTime::zero();
        
        assert!(uart.b_transport(&mut trans, &mut delay).is_err());
    }

    #[test]
    fn test_uart_output_callback() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();
        
        uart.set_output_callback(move |byte| {
            received_clone.lock().unwrap().push(byte);
        });
        
        // 写入数据
        uart.write_reg(reg_offset::RBR_THR, 0x41);
        uart.write_reg(reg_offset::RBR_THR, 0x42);
        
        // 检查回调接收到的数据
        let data = received.lock().unwrap();
        assert_eq!(&*data, &[0x41, 0x42]);
    }

    #[test]
    fn test_uart_interrupt_callback() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        let interrupted = std::sync::Arc::new(std::sync::Mutex::new(false));
        let interrupted_clone = interrupted.clone();
        
        uart.set_interrupt_callback(move || {
            *interrupted_clone.lock().unwrap() = true;
        });
        
        // 配置中断
        uart.write_reg(reg_offset::MCR, mcr_bits::OUT2);
        uart.write_reg(reg_offset::IER, ier_bits::ERBFI);
        
        // 接收数据，应该触发中断
        uart.receive_byte(0x41);
        
        assert!(*interrupted.lock().unwrap());
    }

    #[test]
    fn test_uart_fifo_overflow() {
        let mut uart = Uart16550::new(0x1000_0000);
        
        // 填满 FIFO
        for i in 0..FIFO_DEPTH {
            uart.receive_byte(i as u8);
        }
        
        // 再接收一个，应该产生溢出错误
        uart.receive_byte(0xFF);
        
        assert!((uart.lsr & lsr_bits::OE) != 0);
    }
}
