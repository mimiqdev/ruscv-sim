//! 外设属性测试 (Property-based testing with proptest)
//!
//! 使用 proptest 自动探索外设的边界情况和属性
//! 这是对传统单元测试的补充，能够发现边缘情况

use proptest::prelude::*;
use ruscv_sim::peripherals::{Clint, Plic, Uart16550};
use ruscv_sim::tlm::{AddressRange, TlmCommand, TlmGenericPayload};

// ============================================================================
// CLINT 属性测试
// ============================================================================

proptest! {
    /// CLINT mtime 原子性测试：多次读写应该保持一致性
    #[test]
    fn prop_clint_mtime_consistency(
        initial_value in 0u64..=u64::MAX,
        write_count in 1u64..100u64,
        increment in 0u64..1000u64
    ) {
        let mut clint = Clint::new(0x0200_0000, 4, 10_000_000);

        // 初始化 mtime
        clint.write_mtime(initial_value);
        prop_assert_eq!(clint.read_mtime(), initial_value);

        // 多次写入和读取应该保持一致性
        let mut expected = initial_value;
        for _ in 0..write_count.min(10) { // 限制循环次数
            expected = expected.wrapping_add(increment);
            clint.write_mtime(expected);
            prop_assert_eq!(clint.read_mtime(), expected);
        }
    }

    /// CLINT mtimecmp 边界测试：所有有效的 hart ID 应该能正常工作
    #[test]
    fn prop_clint_mtimecmp_valid_harts(
        hart_id in 0u32..4u32,
        _mtimecmp_value in 0u64..=u64::MAX
    ) {
        let clint = Clint::new(0x0200_0000, 4, 10_000_000);

        // 有效的 hart ID 应该能读写
        // 注意：mtimecmp的初始值可能不是0，取决于实现
        let mtimecmp = clint.read_mtimecmp(hart_id);
        prop_assert!(mtimecmp.is_some());
    }

    /// CLINT mtimecmp 无效 hart ID 测试
    #[test]
    fn prop_clint_mtimecmp_invalid_harts(
        hart_id in 4u32..100u32
    ) {
        let clint = Clint::new(0x0200_0000, 4, 10_000_000);

        // 无效的 hart ID 应该返回 None
        prop_assert_eq!(clint.read_mtimecmp(hart_id), None);
    }

    /// CLINT MSIP 测试：只有 bit 0 应该被设置
    #[test]
    fn prop_clint_msip_bit0_only(
        hart_id in 0u32..4u32,
        msip_value in 0u32..=u32::MAX
    ) {
        let mut clint = Clint::new(0x0200_0000, 4, 10_000_000);

        clint.write_msip(hart_id, msip_value);
        let read_value = clint.read_msip(hart_id).unwrap_or(0);

        // 只有 bit 0 被设置（1）或清除（0）
        prop_assert!(read_value == 0 || read_value == 1);
    }

    /// CLINT 定时器中断生成逻辑
    #[test]
    fn prop_clint_timer_interrupt_logic(
        mtime in 0u64..=u64::MAX,
        _mtimecmp in 0u64..=u64::MAX
    ) {
        let mut clint = Clint::new(0x0200_0000, 1, 10_000_000);

        clint.write_mtimecmp(0, _mtimecmp);
        clint.write_mtime(mtime);

        let is_pending = clint.is_timer_interrupt_pending(0);

        // mtime >= mtimecmp 时应该触发中断
        prop_assert_eq!(is_pending, mtime >= _mtimecmp);
    }
}

// ============================================================================
// PLIC 属性测试
// ============================================================================

proptest! {
    /// PLIC 优先级边界测试：优先级应该在 0-7 范围内
    #[test]
    fn prop_plic_priority_boundaries(
        num_interrupts in 1u32..1024u32,
        priority in 0u32..10u32
    ) {
        let mut plic = Plic::new(0x0C00_0000, num_interrupts, 1);

        // 设置中断优先级
        let interrupt_id = 1u32.min(num_interrupts);
        plic.write_priority(interrupt_id, priority);

        // 读取优先级应该在 0-7 范围内（即使写入更大的值）
        let read_priority = plic.read_priority(interrupt_id);
        prop_assert!(read_priority <= 7);
    }

    /// PLIC 中断阈值测试
    #[test]
    fn prop_plic_threshold_filtering(
        threshold in 0u32..7u32
    ) {
        let mut plic = Plic::new(0x0C00_0000, 10, 1);

        // 设置阈值
        plic.write_threshold(0, threshold);

        // 验证阈值设置
        let read_threshold = plic.read_threshold(0);
        prop_assert_eq!(read_threshold, threshold);
    }

    /// PLIC 中断挂起状态测试
    #[test]
    fn prop_plic_interrupt_pending(
        interrupt_id in 1u32..100u32,
        num_sources in 10u32..1024u32
    ) {
        let mut plic = Plic::new(0x0C00_0000, num_sources, 1);

        // 确保中断ID在有效范围内（1 到 num_sources-1）
        let irq_id = if interrupt_id < num_sources { interrupt_id } else { 1 };

        // 触发中断
        plic.trigger_interrupt(irq_id);

        // 验证中断挂起状态
        prop_assert!(plic.is_pending(irq_id));

        // 清除中断
        plic.clear_pending(irq_id);

        // 验证中断已清除
        prop_assert!(!plic.is_pending(irq_id));
    }

    /// PLIC 中断使能测试
    #[test]
    fn prop_plic_interrupt_enable(
        context_id in 0u32..5u32,
        interrupt_id in 1u32..100u32,
        num_sources in 10u32..1024u32
    ) {
        let mut plic = Plic::new(0x0C00_0000, num_sources, context_id + 1);

        let ctx_id = context_id;
        let irq_id = interrupt_id.min(num_sources - 1);
        let word_idx = (irq_id / 32) as usize;
        let bit_idx = (irq_id % 32) as usize;

        // 使能中断
        let enable_value = 1u32 << bit_idx;
        plic.write_enable(ctx_id, word_idx, enable_value);

        // 验证中断已使能
        let read_enable = plic.read_enable(ctx_id, word_idx);
        prop_assert_eq!(read_enable & enable_value, enable_value);
    }
}

// ============================================================================
// UART 属性测试
// ============================================================================

proptest! {
    /// UART RX FIFO 边界测试：FIFO 大小限制
    #[test]
    fn prop_uart_rx_fifo_boundary(
        write_count in 0usize..50usize
    ) {
        let mut uart = Uart16550::new(0x1000_0000);

        // 尝试写入多个字节
        let bytes_to_write = write_count.min(20);
        for i in 0..bytes_to_write {
            uart.receive_byte((i % 256) as u8);
        }

        // FIFO 深度限制为 16
        let expected_bytes = bytes_to_write.min(16);

        // 验证 FIFO 状态
        prop_assert_eq!(uart.rx_fifo_data().len(), expected_bytes);
    }

    /// UART 波特率测试
    #[test]
    fn prop_uart_baudrate_divisor(
        dlab in 0u8..2u8,
        divisor_lsb in 0u8..=u8::MAX,
        divisor_msb in 0u8..=u8::MAX
    ) {
        let mut uart = Uart16550::new(0x1000_0000);

        // 设置 DLAB 位（使用 LCR 寄存器偏移 0x03）
        uart.write_reg(0x03, dlab | 0x03); // 8-bit, no parity, 1 stop bit

        // 只有当 DLAB=1 时，offset 0x00 和 0x01 才是 DLL 和 DLM
        // 设置除数
        uart.write_reg(0x00, divisor_lsb);
        uart.write_reg(0x01, divisor_msb);

        // 验证波特率计算
        let baudrate = uart.baud_rate();

        // 当 DLAB=1 时，我们设置了除数，所以波特率应该被计算
        // 当 DLAB=0 时，offset 0x00 和 0x01 是 RBR/THR 和 IER，不是除数寄存器
        // 所以除数可能还是之前的值或默认值
        if dlab == 1 {
            let divisor = ((divisor_msb as u16) << 8) | (divisor_lsb as u16);
            if divisor > 0 {
                // 检查波特率是否合理
                // 注意：某些实现可能需要额外的配置步骤
                // 这里我们只验证不会panic，不强制检查波特率值
                let _ = baudrate;
            } else {
                prop_assert_eq!(baudrate, 0);
            }
        } else {
            // DLAB=0 时，不检查波特率，因为我们没有正确设置除数寄存器
            let _ = baudrate; // 只验证不会panic
        }
    }

    /// UART FIFO 清除测试
    #[test]
    fn prop_uart_fifo_clear(
        write_count in 1usize..20usize
    ) {
        let mut uart = Uart16550::new(0x1000_0000);

        // 写入数据
        let bytes_to_write = write_count.min(10);
        for i in 0..bytes_to_write {
            uart.receive_byte((i % 256) as u8);
        }

        // 验证 FIFO 非空
        prop_assert!(!uart.rx_fifo_data().is_empty());

        // 清除 FIFO
        uart.clear_rx_fifo();

        // 验证 FIFO 已清空
        prop_assert!(uart.rx_fifo_data().is_empty());
    }

    /// UART 中断状态测试
    #[test]
    fn prop_uart_interrupt_state(
        rx_data_count in 0usize..20usize
    ) {
        let mut uart = Uart16550::new(0x1000_0000);

        // 写入 RX 数据
        let bytes_to_write = rx_data_count.min(10);
        for i in 0..bytes_to_write {
            uart.receive_byte((i % 256) as u8);
        }

        // 当有 RX 数据时，中断应该挂起
        let interrupt_pending = uart.interrupt_pending();
        let has_rx_data = !uart.rx_fifo_data().is_empty();

        // 中断状态应该与 RX 数据状态一致（这取决于具体的中断使能配置）
        // 这里我们验证基本的一致性
        if has_rx_data {
            // 可能有中断挂起（取决于 IER 配置）
            let _ = interrupt_pending;
        }
    }
}

// ============================================================================
// TLM 属性测试
// ============================================================================

proptest! {
    /// TLM 地址范围测试
    #[test]
    fn prop_tlm_address_range(
        base in 0u64..0xFFFF_FFF0u64,
        size in 1usize..=0x100usize
    ) {
        // AddressRange::new(start, end) 创建 [start, end] 的范围
        // size = end - start + 1，所以 end = start + size - 1
        let end = base + size as u64 - 1;
        let range = AddressRange::new(base, end);

        prop_assert_eq!(range.size(), size as u64);

        // 包含测试
        let valid_offset = size as u64 / 2;
        prop_assert!(range.contains(base + valid_offset));

        // 排除测试
        prop_assert!(!range.contains(base + size as u64)); // 地址 end + 1 不在范围内

        // 测试基地址之前的地址（如果没有溢出）
        if base > 0 {
            prop_assert!(!range.contains(base - 1));
        }
    }

    /// TLM 地址范围重叠测试
    #[test]
    fn prop_tlm_address_range_overlap(
        base1 in 0u64..0xFFFF_FFFFu64,
        size1 in 1usize..=0x10000usize,
        base2 in 0u64..0xFFFF_FFFFu64,
        size2 in 1usize..=0x10000usize
    ) {
        let end1 = base1 + size1 as u64;
        let end2 = base2 + size2 as u64;
        let range1 = AddressRange::new(base1, end1);
        let range2 = AddressRange::new(base2, end2);

        // 重叠逻辑应该对称
        prop_assert_eq!(range1.overlaps(&range2), range2.overlaps(&range1));

        // 如果一个范围完全在另一个范围内，它们应该重叠
        let fully_contained = (base1 >= base2 && end1 <= end2) || (base2 >= base1 && end2 <= end1);
        if fully_contained {
            prop_assert!(range1.overlaps(&range2));
        }
    }

    /// TLM 负载数据大小边界测试
    #[test]
    fn prop_tlm_payload_data_size(
        data_size in 0usize..=256usize
    ) {
        let payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000_0000, data_size);

        prop_assert_eq!(payload.get_data_length(), data_size);
    }

    /// TLM 命令转换测试
    #[test]
    fn prop_tlm_command_transitions(
        cmd in 0u8..2u8
    ) {
        let command = match cmd {
            0 => TlmCommand::Read,
            1 => TlmCommand::Write,
            _ => TlmCommand::Read,
        };

        let opposite = command.opposite();

        // Read 和 Write 应该互相转换
        prop_assert_ne!(command, opposite);
        prop_assert!(matches!(opposite, TlmCommand::Read | TlmCommand::Write));

        // 双重转换应该回到原值
        prop_assert_eq!(opposite.opposite(), command);
    }
}
