//! 外设集成测试 (20+ tests)
//!
//! 测试 CLINT、PLIC、UART 等外设功能

use ruscv_sim::peripherals::*;
use ruscv_sim::tlm::*;
use std::sync::{Arc, Mutex};

// ============================================================================
// CLINT 测试 (8 tests)
// ============================================================================

#[test]
fn test_clint_basic_creation() {
    let clint = Clint::new(0x0200_0000, 4, 10_000_000);
    assert_eq!(clint.base_addr(), 0x0200_0000);
    assert_eq!(clint.num_harts(), 4);
    assert_eq!(clint.read_mtime(), 0);
}

#[test]
fn test_clint_mtime_operations() {
    let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);
    
    // Test mtime read/write
    clint.write_mtime(0x1234_5678_9ABC_DEF0);
    assert_eq!(clint.read_mtime(), 0x1234_5678_9ABC_DEF0);
    
    // Test increment
    clint.write_mtime(100);
    assert_eq!(clint.read_mtime(), 100);
}

#[test]
fn test_clint_mtimecmp_operations() {
    let mut clint = Clint::new(0x0200_0000, 4, 10_000_000);
    
    // Test mtimecmp for different harts
    clint.write_mtimecmp(0, 0x1000);
    assert_eq!(clint.read_mtimecmp(0), Some(0x1000));
    
    clint.write_mtimecmp(3, 0x2000);
    assert_eq!(clint.read_mtimecmp(3), Some(0x2000));
    
    // Invalid hart ID
    assert_eq!(clint.read_mtimecmp(10), None);
}

#[test]
fn test_clint_msip_operations() {
    let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);
    
    // Only bit 0 is valid for MSIP
    clint.write_msip(0, 0xFFFFFFFF);
    assert_eq!(clint.read_msip(0), Some(0x1));
    
    clint.write_msip(0, 0);
    assert_eq!(clint.read_msip(0), Some(0));
    
    // Set bit 0
    clint.write_msip(1, 1);
    assert_eq!(clint.read_msip(1), Some(1));
}

#[test]
fn test_clint_timer_interrupt_generation() {
    let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);
    
    // Set mtimecmp
    clint.write_mtimecmp(0, 100);
    
    // mtime < mtimecmp, no interrupt
    clint.write_mtime(50);
    assert!(!clint.is_timer_interrupt_pending(0));
    
    // mtime >= mtimecmp, interrupt should be pending
    clint.write_mtime(100);
    assert!(clint.is_timer_interrupt_pending(0));
    
    // Clear interrupt
    clint.clear_timer_interrupt(0);
    assert!(!clint.is_timer_interrupt_pending(0));
}

#[test]
fn test_clint_software_interrupt_generation() {
    let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);
    
    // Initially no interrupt
    assert!(!clint.is_software_interrupt_pending(0));
    
    // Set MSIP
    clint.write_msip(0, 1);
    assert!(clint.is_software_interrupt_pending(0));
    
    // Clear interrupt
    clint.clear_software_interrupt(0);
    assert!(!clint.is_software_interrupt_pending(0));
    assert_eq!(clint.read_msip(0), Some(0));
}

#[test]
fn test_clint_time_update() {
    let mut clint = Clint::new(0x0200_0000, 1, 1_000_000); // 1MHz
    
    // Set initial time
    clint.last_update_time = ScTime::zero();
    
    // Update 1ms
    let current_time = ScTime::from_milliseconds(1);
    clint.update_mtime(current_time);
    
    // 1MHz clock, 1ms = 1000 ticks
    assert_eq!(clint.read_mtime(), 1000);
}

#[test]
fn test_clint_tlm_interface() {
    let mut clint = Clint::new(0x0200_0000, 2, 10_000_000);
    
    // Test write via TLM
    let write_data = vec![0x78, 0x56, 0x34, 0x12];
    let mut write_trans = TlmGenericPayload::with_data(
        TlmCommand::Write,
        0x0200_0000 + clint::reg_offset::MTIME,
        write_data,
    );
    let mut delay = ScTime::zero();
    
    assert!(clint.b_transport(&mut write_trans, &mut delay).is_ok());
    
    // Test read via TLM
    let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x0200_0000 + clint::reg_offset::MTIME, 4);
    delay = ScTime::zero();
    
    assert!(clint.b_transport(&mut read_trans, &mut delay).is_ok());
    
    let value = u32::from_le_bytes([
        read_trans.data()[0],
        read_trans.data()[1],
        read_trans.data()[2],
        read_trans.data()[3],
    ]);
    assert_eq!(value, 0x12345678);
}

// ============================================================================
// PLIC 测试 (7 tests)
// ============================================================================

#[test]
fn test_plic_basic_creation() {
    let plic = Plic::new(0x0C00_0000, 64, 8);
    assert_eq!(plic.base_addr(), 0x0C00_0000);
    assert_eq!(plic.num_sources(), 64);
    assert_eq!(plic.num_contexts(), 8);
}

#[test]
fn test_plic_priority_configuration() {
    let mut plic = Plic::new(0x0C00_0000, 64, 2);
    
    // Default priority is 0
    assert_eq!(plic.read_priority(1), 0);
    
    // Set priority
    plic.write_priority(1, 5);
    assert_eq!(plic.read_priority(1), 5);
    
    // Priority is capped at MAX_PRIORITY
    plic.write_priority(2, 100);
    assert_eq!(plic.read_priority(2), MAX_PRIORITY);
    
    // Interrupt source 0 is reserved
    plic.write_priority(0, 5);
    assert_eq!(plic.read_priority(0), 0);
}

#[test]
fn test_plic_pending_and_enable() {
    let mut plic = Plic::new(0x0C00_0000, 32, 2);
    
    // Initially no pending interrupts
    assert!(!plic.is_pending(1));
    
    // Trigger interrupt
    plic.trigger_interrupt(1);
    assert!(plic.is_pending(1));
    
    // Clear pending
    plic.clear_pending(1);
    assert!(!plic.is_pending(1));
    
    // Test enable
    assert_eq!(plic.read_enable(0, 0), 0);
    plic.write_enable(0, 0, 0x2); // Enable interrupt 1
    assert_eq!(plic.read_enable(0, 0), 0x2);
}

#[test]
fn test_plic_threshold_masking() {
    let mut plic = Plic::new(0x0C00_0000, 32, 2);
    
    // Configure interrupt
    plic.write_priority(1, 3);
    plic.write_enable(0, 0, 1 << 1);
    
    // Set threshold higher than interrupt priority
    plic.write_threshold(0, 5);
    plic.trigger_interrupt(1);
    
    // Cannot claim due to threshold
    let claimed = plic.find_highest_priority_interrupt(0);
    assert_eq!(claimed, 0);
    
    // Lower threshold
    plic.write_threshold(0, 2);
    let claimed = plic.claim_interrupt(0);
    assert_eq!(claimed, 1);
}

#[test]
fn test_plic_claim_complete_cycle() {
    let mut plic = Plic::new(0x0C00_0000, 32, 2);
    
    // Setup interrupt
    plic.write_priority(5, 3);
    plic.write_enable(0, 0, 1 << 5);
    
    // Trigger and claim
    plic.trigger_interrupt(5);
    let claimed = plic.claim_interrupt(0);
    assert_eq!(claimed, 5);
    
    // Pending cleared after claim
    assert!(!plic.is_pending(5));
    
    // Complete
    plic.complete_interrupt(0, 5);
    assert_eq!(plic.get_claimed(0), None);
}

#[test]
fn test_plic_priority_arbitration() {
    let mut plic = Plic::new(0x0C00_0000, 32, 2);
    
    // Set different priorities
    plic.write_priority(1, 1);
    plic.write_priority(2, 3);
    plic.write_priority(3, 2);
    
    // Enable all
    plic.write_enable(0, 0, 0xFF);
    
    // Trigger multiple
    plic.trigger_interrupt(1);
    plic.trigger_interrupt(2);
    plic.trigger_interrupt(3);
    
    // Should return highest priority (ID=2, priority=3)
    let claimed = plic.claim_interrupt(0);
    assert_eq!(claimed, 2);
}

#[test]
fn test_plic_tlm_interface() {
    let mut plic = Plic::new(0x0C00_0000, 32, 2);
    
    // Write priority
    let write_data = vec![0x05, 0x00, 0x00, 0x00];
    let mut write_trans = TlmGenericPayload::with_data(
        TlmCommand::Write,
        0x0C00_0000 + 4, // Source 1 priority
        write_data,
    );
    let mut delay = ScTime::zero();
    
    assert!(plic.b_transport(&mut write_trans, &mut delay).is_ok());
    assert_eq!(plic.read_priority(1), 5);
    
    // Read priority
    let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x0C00_0000 + 4, 4);
    delay = ScTime::zero();
    
    assert!(plic.b_transport(&mut read_trans, &mut delay).is_ok());
    assert_eq!(read_trans.data()[0], 5);
}

// ============================================================================
// UART 16550 测试 (8 tests)
// ============================================================================

#[test]
fn test_uart_basic_creation() {
    let uart = Uart16550::new(0x1000_0000);
    assert_eq!(uart.base_addr(), 0x1000_0000);
    assert!(!uart.fifo_enabled());
}

#[test]
fn test_uart_fifo_enable() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // FIFO initially disabled
    assert!(!uart.fifo_enabled());
    
    // Enable FIFO
    uart.write_reg(uart16550::reg_offset::IIR_FCR, uart16550::fcr_bits::FIFO_ENABLE);
    assert!(uart.fifo_enabled());
}

#[test]
fn test_uart_receive_and_read() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Receive data
    uart.receive_byte(0x41); // 'A'
    uart.receive_byte(0x42); // 'B'
    
    // Check FIFO
    assert_eq!(uart.rx_fifo_data(), &[0x41, 0x42]);
    
    // Read RBR
    let byte1 = uart.read_reg(uart16550::reg_offset::RBR_THR);
    assert_eq!(byte1, 0x41);
    
    let byte2 = uart.read_reg(uart16550::reg_offset::RBR_THR);
    assert_eq!(byte2, 0x42);
}

#[test]
fn test_uart_transmit() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Write to THR
    uart.write_reg(uart16550::reg_offset::RBR_THR, 0x41);
    uart.write_reg(uart16550::reg_offset::RBR_THR, 0x42);
    
    // Check TX FIFO
    assert_eq!(uart.tx_fifo_data(), &[0x41, 0x42]);
}

#[test]
fn test_uart_dlab_mode() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Set DLAB
    uart.write_reg(uart16550::reg_offset::LCR, uart16550::lcr_bits::DLAB);
    assert!(uart.dlab());
    
    // Access DLL/DLM
    uart.write_reg(uart16550::reg_offset::RBR_THR, 0x0C); // DLL
    uart.write_reg(uart16550::reg_offset::IER, 0x00);     // DLM
    
    assert_eq!(uart.read_reg(uart16550::reg_offset::RBR_THR), 0x0C);
    assert_eq!(uart.read_reg(uart16550::reg_offset::IER), 0x00);
}

#[test]
fn test_uart_baud_rate_calculation() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Set DLAB
    uart.write_reg(uart16550::reg_offset::LCR, uart16550::lcr_bits::DLAB);
    
    // Divisor = 12 (9600 bps with 1.8432MHz base clock)
    uart.write_reg(uart16550::reg_offset::RBR_THR, 12);
    uart.write_reg(uart16550::reg_offset::IER, 0);
    
    assert_eq!(uart.baud_rate(), 9600);
}

#[test]
fn test_uart_interrupt_handling() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Initially no interrupt
    assert!(!uart.interrupt_pending());
    
    // Enable receive interrupt and OUT2
    uart.write_reg(uart16550::reg_offset::MCR, uart16550::mcr_bits::OUT2);
    uart.write_reg(uart16550::reg_offset::IER, uart16550::ier_bits::ERBFI);
    
    // Still no interrupt (no data)
    assert!(!uart.interrupt_pending());
    
    // Receive data
    uart.receive_byte(0x41);
    
    // Now interrupt should be pending
    assert!(uart.interrupt_pending());
    
    // Check interrupt ID
    let iir = uart.read_reg(uart16550::reg_offset::IIR_FCR);
    assert_eq!(iir & uart16550::iir_bits::ID_MASK, uart16550::iir_bits::RECEIVE_DATA);
}

#[test]
fn test_uart_tlm_interface() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Receive data
    uart.receive_byte(0x42);
    
    // Read via TLM
    let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000_0000, 1);
    let mut delay = ScTime::zero();
    
    assert!(uart.b_transport(&mut read_trans, &mut delay).is_ok());
    assert_eq!(read_trans.data()[0], 0x42);
    
    // Write via TLM
    let write_data = vec![0x55];
    let mut write_trans = TlmGenericPayload::with_data(
        TlmCommand::Write,
        0x1000_0000,
        write_data,
    );
    delay = ScTime::zero();
    
    assert!(uart.b_transport(&mut write_trans, &mut delay).is_ok());
    assert_eq!(uart.tx_fifo_data(), &[0x55]);
}

// ============================================================================
// 综合外设测试 (补充到 20+ 个测试)
// ============================================================================

#[test]
fn test_platform_configurations() {
    let hifive1 = PlatformConfig::hifive1();
    assert_eq!(hifive1.clint_base, 0x0200_0000);
    assert_eq!(hifive1.num_harts, 1);
    
    let qemu = PlatformConfig::qemu_virt();
    assert_eq!(qemu.num_harts, 4);
    assert_eq!(qemu.plic_sources, 96);
    
    let default = PlatformConfig::default();
    assert_eq!(default.num_harts, 1);
}

#[test]
fn test_clint_mtimecmp_array() {
    let mut clint = Clint::new(0x0200_0000, 8, 10_000_000);
    
    // Write to all harts
    for i in 0..8 {
        clint.write_mtimecmp(i, i as u64 * 1000);
    }
    
    // Verify all
    for i in 0..8 {
        assert_eq!(clint.read_mtimecmp(i), Some(i as u64 * 1000));
    }
}

#[test]
fn test_plic_multi_context() {
    let mut plic = Plic::new(0x0C00_0000, 32, 4);
    
    // Configure different enables for different contexts
    plic.write_enable(0, 0, 0xFF); // Context 0: enable sources 0-7
    plic.write_enable(1, 0, 0xF0); // Context 1: enable sources 4-7
    plic.write_enable(2, 0, 0x0F); // Context 2: enable sources 0-3
    
    assert_eq!(plic.read_enable(0, 0), 0xFE); // 源 0 被强制禁用
    assert_eq!(plic.read_enable(1, 0), 0xF0); // bit 4-7 使能，bit 0 本来就是 0
    assert_eq!(plic.read_enable(2, 0), 0x0E); // 源 0 被强制禁用
}

#[test]
fn test_uart_fifo_reset() {
    let mut uart = Uart16550::new(0x1000_0000);
    
    // Fill FIFOs
    for i in 0..FIFO_DEPTH {
        uart.receive_byte(i as u8);
        uart.write_reg(uart16550::reg_offset::RBR_THR, i as u8);
    }
    
    assert_eq!(uart.rx_fifo_data().len(), FIFO_DEPTH);
    assert_eq!(uart.tx_fifo_data().len(), FIFO_DEPTH);
    
    // Reset FIFOs
    uart.write_reg(
        uart16550::reg_offset::IIR_FCR,
        uart16550::fcr_bits::FIFO_ENABLE
            | uart16550::fcr_bits::RCVR_FIFO_RESET
            | uart16550::fcr_bits::XMIT_FIFO_RESET,
    );
    
    assert!(uart.rx_fifo.is_empty());
    assert!(uart.tx_fifo.is_empty());
}

#[test]
fn test_peripheral_error_display() {
    let err = PeripheralError::InvalidAddress(0x1234);
    assert!(err.to_string().contains("0x0000000000001234"));
    
    let err = PeripheralError::InvalidParameter("test".to_string());
    assert!(err.to_string().contains("test"));
    
    let err = PeripheralError::AccessDenied;
    assert_eq!(err.to_string(), "Access denied");
}
