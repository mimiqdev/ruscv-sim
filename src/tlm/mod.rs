//! TLM2.0 抽象层模块
//!
//! 实现 SystemC TLM2.0 风格的接口抽象，用于 RISC-V 模拟器的外部组件通信。
//!
//! # 主要组件
//!
//! - [`phase`]: TLM 传输相位定义 (BEGIN_REQ, END_REQ, BEGIN_RESP, END_RESP)
//! - [`status`]: TLM 响应状态定义 (OK, ERROR, etc.)
//! - [`command`]: TLM 命令类型定义 (Read, Write)
//! - [`time`]: SystemC 风格的时间管理 (sc_time)
//! - [`payload`]: TLM 通用事务载荷结构
//! - [`traits`]: TLM Initiator/Target 接口 trait
//! - [`bus`]: TLM 总线实现，支持多设备互联和路由仲裁
//! - [`error`]: TLM 错误定义
//!
//! # 示例
//!
//! ```
//! use ruscv_sim::tlm::{
//!     TlmBus, TlmSimpleMemory, TlmGenericPayload, TlmCommand,
//!     ArbitrationPolicy, ScTime, AddressRange, TlmInitiator
//! };
//! use std::sync::{Arc, Mutex};
//!
//! // 创建总线和内存
//! let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
//! let memory = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));
//!
//! // 添加路由
//! bus.add_route(
//!     AddressRange::new(0x1000, 0x13FF),
//!     memory.clone(),
//!     0,
//!     "memory"
//! );
//!
//! // 执行读写操作
//! let mut trans = TlmGenericPayload::with_data(
//!     TlmCommand::Write,
//!     0x1000,
//!     vec![0x01, 0x02, 0x03, 0x04]
//! );
//! let mut delay = ScTime::zero();
//! bus.b_transport(&mut trans, &mut delay).unwrap();
//! ```

// 子模块声明
mod bus;
mod command;
mod error;
mod payload;
mod phase;
mod status;
mod time;
mod traits;

// 公开导出
pub use bus::{ArbitrationPolicy, BusRoute, TlmBus, TlmBusBridge, TlmSimpleMemory};
pub use command::TlmCommand;
pub use error::{TlmError, TlmSyncEnum};
pub use payload::{DataExtensionMode, TlmGenericPayload, TlmPayloadBuilder};
pub use phase::TlmPhase;
pub use status::{ErrorCategory, TlmResponseStatus};
pub use time::{ScTime, ScTimeUnit, TlmTime};
pub use traits::{AddressRange, DmiAccessRights, DmiData, TlmInitiator, TlmInterface, TlmTarget};

// 重新导出以兼容旧代码
pub use error::TlmSyncEnum as TlmSync;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// 测试完整的 TLM 读写流程
    #[test]
    fn test_full_read_write_flow() {
        // 创建总线和内存
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let memory = Arc::new(Mutex::new(TlmSimpleMemory::new(0x8000_0000, 4096)));

        bus.add_route(
            AddressRange::new(0x8000_0000, 0x8000_0FFF),
            memory.clone(),
            0,
            "main_memory",
        );

        // 写入数据
        let write_data = vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let mut write_trans =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x8000_0100, write_data.clone());
        let mut delay = ScTime::zero();

        assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());
        assert!(write_trans.is_response_ok());

        // 读取数据
        let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x8000_0100, 8);
        delay = ScTime::zero();

        assert!(bus.b_transport(&mut read_trans, &mut delay).is_ok());
        assert_eq!(read_trans.data(), &write_data[..]);
    }

    /// 测试多设备路由
    #[test]
    fn test_multi_device_routing() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);

        // 创建两个内存设备
        let mem1 = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));
        let mem2 = Arc::new(Mutex::new(TlmSimpleMemory::new(0x2000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), mem1.clone(), 0, "mem1");
        bus.add_route(AddressRange::new(0x2000, 0x23FF), mem2.clone(), 0, "mem2");

        // 写入 mem1
        let mut trans1 = TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0xAA, 0xBB]);
        let mut delay = ScTime::zero();
        bus.b_transport(&mut trans1, &mut delay).unwrap();

        // 写入 mem2
        let mut trans2 = TlmGenericPayload::with_data(TlmCommand::Write, 0x2000, vec![0xCC, 0xDD]);
        delay = ScTime::zero();
        bus.b_transport(&mut trans2, &mut delay).unwrap();

        // 验证 mem1
        let mut read1 = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 2);
        delay = ScTime::zero();
        bus.b_transport(&mut read1, &mut delay).unwrap();
        assert_eq!(read1.data(), &[0xAA, 0xBB]);

        // 验证 mem2
        let mut read2 = TlmGenericPayload::new(TlmCommand::Read, 0x2000, 2);
        delay = ScTime::zero();
        bus.b_transport(&mut read2, &mut delay).unwrap();
        assert_eq!(read2.data(), &[0xCC, 0xDD]);
    }

    /// 测试地址越界错误
    #[test]
    fn test_address_out_of_range() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let memory = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), memory.clone(), 0, "mem");

        // 访问未映射的地址
        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x5000, 4);
        let mut delay = ScTime::zero();

        assert!(bus.b_transport(&mut trans, &mut delay).is_err());
    }

    /// 测试时间延迟累积
    #[test]
    fn test_delay_accumulation() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let memory = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        // 设置总线延迟
        bus.set_default_delay(ScTime::from_nanoseconds(5));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), memory.clone(), 0, "mem");

        let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
        let mut delay = ScTime::from_nanoseconds(3); // 初始延迟

        bus.b_transport(&mut trans, &mut delay).unwrap();

        // 验证延迟累积：初始3ns + 总线5ns + 内存1ns = 9ns
        assert_eq!(delay.to_nanoseconds(), 9);
    }

    /// 测试 DMI 功能
    #[test]
    fn test_dmi_interface() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        let memory = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

        bus.add_route(AddressRange::new(0x1000, 0x13FF), memory.clone(), 0, "mem");

        let trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

        // 获取 DMI 指针
        let dmi = bus.get_direct_mem_ptr(&trans);
        assert!(dmi.is_some());

        let dmi_data = dmi.unwrap();
        assert!(dmi_data.contains(0x1000));
        assert!(dmi_data.contains(0x13FF));
        assert!(!dmi_data.contains(0x1400));
        assert!(dmi_data.allows_read());
        assert!(dmi_data.allows_write());
    }

    /// 测试流式传输
    #[test]
    fn test_streaming_width() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Write, 0x1000, 16);

        assert!(!payload.is_streaming());

        payload.set_streaming_width(4);
        assert!(payload.is_streaming());
        assert_eq!(payload.streaming_width(), 4);
    }

    /// 测试字节使能
    #[test]
    fn test_byte_enable() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Write, 0x1000, 4);

        // 设置字节使能：只使能第0和第2字节
        let byte_enable = vec![0xFF, 0x00, 0xFF, 0x00];
        payload.set_byte_enable(Some(byte_enable.clone()));

        assert_eq!(payload.byte_enable(), Some(&byte_enable[..]));
        assert_eq!(payload.byte_enable_length(), 4);
    }

    /// 测试响应状态
    #[test]
    fn test_response_status_handling() {
        let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

        assert!(payload.is_response_ok());
        assert!(!payload.is_response_error());

        payload.set_response_status(TlmResponseStatus::AddressError);

        assert!(!payload.is_response_ok());
        assert!(payload.is_response_error());
    }

    /// 测试载荷深拷贝
    #[test]
    fn test_payload_deep_copy() {
        let original =
            TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);

        let copy = original.deep_copy();

        assert_eq!(original.command(), copy.command());
        assert_eq!(original.address(), copy.address());
        assert_eq!(original.data(), copy.data());
    }

    /// 测试载荷构建器
    #[test]
    fn test_payload_builder() {
        let payload = TlmPayloadBuilder::new()
            .command(TlmCommand::Write)
            .address(0x2000)
            .data(vec![0x11, 0x22, 0x33, 0x44])
            .dmi_allowed(true)
            .streaming_width(2)
            .build();

        assert_eq!(payload.command(), TlmCommand::Write);
        assert_eq!(payload.address(), 0x2000);
        assert_eq!(payload.data(), &[0x11, 0x22, 0x33, 0x44]);
        assert!(payload.is_dmi_allowed());
        assert_eq!(payload.streaming_width(), 2);
    }

    /// 测试仲裁策略切换
    #[test]
    fn test_arbitration_policy_switching() {
        let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
        assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::FixedPriority);

        bus.set_arbitration_policy(ArbitrationPolicy::RoundRobin);
        assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::RoundRobin);

        bus.set_arbitration_policy(ArbitrationPolicy::LRU);
        assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::LRU);
    }

    /// 测试 DMI 缓存
    #[test]
    fn test_dmi_cache() {
        let bus = TlmBus::new(ArbitrationPolicy::FixedPriority);

        // 创建 DMI 数据
        let mut data = vec![0u8; 1024];
        let dmi = DmiData {
            dmi_ptr: data.as_mut_ptr(),
            dmi_size: 1024,
            access_rights: DmiAccessRights::ReadWrite,
            read_latency: ScTime::from_nanoseconds(1),
            write_latency: ScTime::from_nanoseconds(1),
            start_address: 0x1000,
            end_address: 0x13FF,
        };

        // 注册 DMI
        bus.register_dmi(0x1000, dmi);

        // 获取 DMI
        let retrieved = bus.get_dmi(0x1000);
        assert!(retrieved.is_some());

        // 清空缓存
        bus.invalidate_dmi();

        // 验证已清空
        let after_invalidate = bus.get_dmi(0x1000);
        assert!(after_invalidate.is_none());
    }
}
