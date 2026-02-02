//! TLM2.0 集成测试 (30 tests)
//!
//! 测试 TLM 总线、路由、内存等核心功能

use ruscv_sim::tlm::*;
use std::sync::{Arc, Mutex};

// ============================================================================
// TLM 基础类型测试 (10 tests)
// ============================================================================

#[test]
fn test_tlm_phase_transitions() {
    // Test complete phase transition cycle
    assert_eq!(TlmPhase::BeginReq.next(), Some(TlmPhase::EndReq));
    assert_eq!(TlmPhase::EndReq.next(), Some(TlmPhase::BeginResp));
    assert_eq!(TlmPhase::BeginResp.next(), Some(TlmPhase::EndResp));
    assert_eq!(TlmPhase::EndResp.next(), None);
}

#[test]
fn test_tlm_phase_reverse_transitions() {
    assert_eq!(TlmPhase::EndResp.prev(), Some(TlmPhase::BeginResp));
    assert_eq!(TlmPhase::BeginResp.prev(), Some(TlmPhase::EndReq));
    assert_eq!(TlmPhase::EndReq.prev(), Some(TlmPhase::BeginReq));
    assert_eq!(TlmPhase::BeginReq.prev(), None);
}

#[test]
fn test_tlm_phase_classification() {
    // Request phases
    assert!(TlmPhase::BeginReq.is_request());
    assert!(TlmPhase::EndReq.is_request());
    assert!(!TlmPhase::BeginResp.is_request());

    // Response phases
    assert!(TlmPhase::BeginResp.is_response());
    assert!(TlmPhase::EndResp.is_response());

    // Begin/End phases
    assert!(TlmPhase::BeginReq.is_begin());
    assert!(TlmPhase::BeginResp.is_begin());
    assert!(TlmPhase::EndReq.is_end());
    assert!(TlmPhase::EndResp.is_end());
}

#[test]
fn test_tlm_response_status() {
    assert!(TlmResponseStatus::Ok.is_ok());
    assert!(!TlmResponseStatus::Ok.is_error());

    assert!(TlmResponseStatus::AddressError.is_error());
    assert!(!TlmResponseStatus::AddressError.is_ok());

    // Error categories
    assert_eq!(
        TlmResponseStatus::InvalidAddress.error_category(),
        Some(ErrorCategory::Address)
    );
    assert_eq!(
        TlmResponseStatus::CommandError.error_category(),
        Some(ErrorCategory::Command)
    );
    assert_eq!(
        TlmResponseStatus::Timeout.error_category(),
        Some(ErrorCategory::Timing)
    );
}

#[test]
fn test_tlm_command() {
    assert!(TlmCommand::Read.is_read());
    assert!(!TlmCommand::Read.is_write());

    assert!(TlmCommand::Write.is_write());
    assert!(!TlmCommand::Write.is_read());

    assert_eq!(TlmCommand::Read.opposite(), TlmCommand::Write);
    assert_eq!(TlmCommand::Write.opposite(), TlmCommand::Read);
}

#[test]
fn test_sc_time_arithmetic() {
    let t1 = ScTime::from_nanoseconds(100);
    let t2 = ScTime::from_nanoseconds(50);

    let t3 = t1 + t2;
    assert_eq!(t3.to_nanoseconds(), 150);

    let t4 = t1 - t2;
    assert_eq!(t4.to_nanoseconds(), 50);

    // Subtraction overflow protection
    let t5 = t2 - t1;
    assert_eq!(t5.to_nanoseconds(), 0);
}

#[test]
fn test_sc_time_comparison() {
    let t1 = ScTime::from_nanoseconds(100);
    let t2 = ScTime::from_nanoseconds(50);
    let t3 = ScTime::from_nanoseconds(100);

    assert!(t1 > t2);
    assert!(t2 < t1);
    assert_eq!(t1, t3);
    assert!(t1 >= t3);
    assert!(t1 <= t3);
}

#[test]
fn test_sc_time_format() {
    assert_eq!(ScTime::from_picoseconds(500).format_auto(), "500 ps");
    assert_eq!(ScTime::from_nanoseconds(10).format_auto(), "10 ns");
    assert_eq!(ScTime::from_microseconds(5).format_auto(), "5 us");
    assert_eq!(ScTime::from_milliseconds(2).format_auto(), "2 ms");
    assert_eq!(ScTime::from_seconds(1).format_auto(), "1 s");
}

#[test]
fn test_tlm_sync_enum() {
    assert!(TlmSyncEnum::Accept.is_accept());
    assert!(TlmSyncEnum::Wait.is_wait());
    assert!(TlmSyncEnum::Release.is_release());
    assert!(TlmSyncEnum::Update.is_update());

    assert!(!TlmSyncEnum::Accept.is_wait());
    assert!(!TlmSyncEnum::Wait.is_accept());
}

#[test]
fn test_tlm_time_compat() {
    // Test backward compatibility with TlmTime
    let legacy = TlmTime::Ns(100);
    assert_eq!(legacy.to_ps(), 100_000);

    let sc: ScTime = legacy.into();
    assert_eq!(sc.to_nanoseconds(), 100);

    let back_to_legacy: TlmTime = sc.into();
    assert_eq!(back_to_legacy.to_ps(), 100_000);
}

// ============================================================================
// TLM Payload 测试 (10 tests)
// ============================================================================

#[test]
fn test_payload_creation() {
    let payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
    assert_eq!(payload.command(), TlmCommand::Read);
    assert_eq!(payload.address(), 0x1000);
    assert_eq!(payload.data_length(), 4);
    assert!(payload.is_response_ok());
}

#[test]
fn test_payload_with_data() {
    let data = vec![0x01, 0x02, 0x03, 0x04];
    let payload = TlmGenericPayload::with_data(TlmCommand::Write, 0x2000, data.clone());

    assert_eq!(payload.data(), &data[..]);
    assert_eq!(payload.command(), TlmCommand::Write);
}

#[test]
fn test_payload_modification() {
    let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

    payload.set_command(TlmCommand::Write);
    assert_eq!(payload.command(), TlmCommand::Write);

    payload.set_address(0x2000);
    assert_eq!(payload.address(), 0x2000);

    payload.set_data_length(8);
    assert_eq!(payload.data_length(), 8);
}

#[test]
fn test_payload_byte_enable() {
    let mut payload = TlmGenericPayload::new(TlmCommand::Write, 0x1000, 4);

    let byte_enable = vec![0xFF, 0x00, 0xFF, 0x00];
    payload.set_byte_enable(Some(byte_enable.clone()));

    assert_eq!(payload.byte_enable(), Some(&byte_enable[..]));
    assert_eq!(payload.byte_enable_length(), 4);
}

#[test]
fn test_payload_streaming() {
    let mut payload = TlmGenericPayload::new(TlmCommand::Write, 0x1000, 16);

    assert!(!payload.is_streaming());

    payload.set_streaming_width(4);
    assert!(payload.is_streaming());
    assert_eq!(payload.streaming_width(), 4);
}

#[test]
fn test_payload_dmi() {
    let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

    assert!(!payload.is_dmi_allowed());

    payload.set_dmi_allowed(true);
    assert!(payload.is_dmi_allowed());
}

#[test]
fn test_payload_response_handling() {
    let mut payload = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

    assert!(payload.is_response_ok());
    assert!(!payload.is_response_error());

    payload.set_response_status(TlmResponseStatus::AddressError);

    assert!(!payload.is_response_ok());
    assert!(payload.is_response_error());
}

#[test]
fn test_payload_reset() {
    let mut payload =
        TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);
    payload.set_response_status(TlmResponseStatus::Ok);

    payload.reset();

    assert_eq!(payload.command(), TlmCommand::Read);
    assert_eq!(payload.address(), 0);
    assert!(payload.is_response_ok());
}

#[test]
fn test_payload_deep_copy() {
    let original =
        TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);

    let copy = original.deep_copy();

    assert_eq!(original.command(), copy.command());
    assert_eq!(original.address(), copy.address());
    assert_eq!(original.data(), copy.data());
}

#[test]
fn test_payload_builder() {
    let payload = TlmPayloadBuilder::new()
        .command(TlmCommand::Write)
        .address(0x3000)
        .data(vec![0xAA, 0xBB, 0xCC, 0xDD])
        .dmi_allowed(true)
        .streaming_width(2)
        .extension_mode(DataExtensionMode::Atomic)
        .build();

    assert_eq!(payload.command(), TlmCommand::Write);
    assert_eq!(payload.address(), 0x3000);
    assert_eq!(payload.data(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    assert!(payload.is_dmi_allowed());
    assert_eq!(payload.streaming_width(), 2);
    assert_eq!(payload.extension_mode(), DataExtensionMode::Atomic);
}

// ============================================================================
// TLM 总线测试 (10 tests)
// ============================================================================

#[test]
fn test_bus_creation() {
    let bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    assert_eq!(bus.route_count(), 0);
    assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::FixedPriority);

    let bus = TlmBus::default();
    assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::FixedPriority);
}

#[test]
fn test_bus_add_remove_route() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory1");

    assert_eq!(bus.route_count(), 1);

    // Remove route
    assert!(bus.remove_route("memory1"));
    assert_eq!(bus.route_count(), 0);

    // Remove non-existent route
    assert!(!bus.remove_route("nonexistent"));
}

#[test]
fn test_bus_simple_read_write() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory");

    // Write data
    let mut write_trans =
        TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0x01, 0x02, 0x03, 0x04]);
    let mut delay = ScTime::zero();

    assert!(bus.b_transport(&mut write_trans, &mut delay).is_ok());

    // Read back
    let mut read_trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
    delay = ScTime::zero();

    assert!(bus.b_transport(&mut read_trans, &mut delay).is_ok());
    assert_eq!(read_trans.data(), &[0x01, 0x02, 0x03, 0x04]);
}

#[test]
fn test_bus_invalid_address() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "memory");

    // Access unmapped address
    let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x5000, 4);
    let mut delay = ScTime::zero();

    assert!(bus.b_transport(&mut trans, &mut delay).is_err());
}

#[test]
fn test_bus_multi_device_routing() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);

    let mem1 = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));
    let mem2 = Arc::new(Mutex::new(TlmSimpleMemory::new(0x2000, 1024)));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem1.clone(), 0, "mem1");
    bus.add_route(AddressRange::new(0x2000, 0x23FF), mem2.clone(), 0, "mem2");

    // Write to mem1
    let mut trans1 = TlmGenericPayload::with_data(TlmCommand::Write, 0x1000, vec![0xAA, 0xBB]);
    let mut delay = ScTime::zero();
    bus.b_transport(&mut trans1, &mut delay).unwrap();

    // Write to mem2
    let mut trans2 = TlmGenericPayload::with_data(TlmCommand::Write, 0x2000, vec![0xCC, 0xDD]);
    delay = ScTime::zero();
    bus.b_transport(&mut trans2, &mut delay).unwrap();

    // Verify mem1
    let mut read1 = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 2);
    delay = ScTime::zero();
    bus.b_transport(&mut read1, &mut delay).unwrap();
    assert_eq!(read1.data(), &[0xAA, 0xBB]);

    // Verify mem2
    let mut read2 = TlmGenericPayload::new(TlmCommand::Read, 0x2000, 2);
    delay = ScTime::zero();
    bus.b_transport(&mut read2, &mut delay).unwrap();
    assert_eq!(read2.data(), &[0xCC, 0xDD]);
}

#[test]
fn test_bus_delay_accumulation() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

    bus.set_default_delay(ScTime::from_nanoseconds(5));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "mem");

    let mut trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);
    let mut delay = ScTime::from_nanoseconds(3);

    bus.b_transport(&mut trans, &mut delay).unwrap();

    // Initial 3ns + bus 5ns + memory 1ns = 9ns
    assert_eq!(delay.to_nanoseconds(), 9);
}

#[test]
fn test_bus_dmi_interface() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    let mem = Arc::new(Mutex::new(TlmSimpleMemory::new(0x1000, 1024)));

    bus.add_route(AddressRange::new(0x1000, 0x13FF), mem.clone(), 0, "mem");

    let trans = TlmGenericPayload::new(TlmCommand::Read, 0x1000, 4);

    let dmi = bus.get_direct_mem_ptr(&trans);
    assert!(dmi.is_some());

    let dmi_data = dmi.unwrap();
    assert!(dmi_data.contains(0x1000));
    assert!(dmi_data.contains(0x13FF));
    assert!(!dmi_data.contains(0x1400));
}

#[test]
fn test_bus_arbitration_policies() {
    let mut bus = TlmBus::new(ArbitrationPolicy::FixedPriority);
    assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::FixedPriority);

    bus.set_arbitration_policy(ArbitrationPolicy::RoundRobin);
    assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::RoundRobin);

    bus.set_arbitration_policy(ArbitrationPolicy::LRU);
    assert_eq!(bus.arbitration_policy(), ArbitrationPolicy::LRU);
}

#[test]
fn test_bus_dmi_cache() {
    let bus = TlmBus::new(ArbitrationPolicy::FixedPriority);

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

    bus.register_dmi(0x1000, dmi);

    let retrieved = bus.get_dmi(0x1000);
    assert!(retrieved.is_some());

    bus.invalidate_dmi();

    let after_invalidate = bus.get_dmi(0x1000);
    assert!(after_invalidate.is_none());
}

// ============================================================================
// 地址范围测试 (补充到 30 个测试)
// ============================================================================

#[test]
fn test_address_range_creation() {
    let range = AddressRange::new(0x1000, 0x1FFF);
    assert_eq!(range.start, 0x1000);
    assert_eq!(range.end, 0x1FFF);
}

#[test]
fn test_address_range_contains() {
    let range = AddressRange::new(0x1000, 0x1FFF);

    assert!(range.contains(0x1000));
    assert!(range.contains(0x1FFF));
    assert!(range.contains(0x1500));
    assert!(!range.contains(0x0FFF));
    assert!(!range.contains(0x2000));
}

#[test]
fn test_address_range_size() {
    let range = AddressRange::new(0x1000, 0x1FFF);
    assert_eq!(range.size(), 0x1000); // 4096 bytes

    let range2 = AddressRange::new(0x1000, 0x1000);
    assert_eq!(range2.size(), 1); // Single byte
}

#[test]
fn test_address_range_overlap() {
    let range1 = AddressRange::new(0x1000, 0x1FFF);
    let range2 = AddressRange::new(0x1800, 0x2800);
    let range3 = AddressRange::new(0x2000, 0x3000);
    let range4 = AddressRange::new(0x3000, 0x4000);

    assert!(range1.overlaps(&range2));
    assert!(range2.overlaps(&range1));
    assert!(range2.overlaps(&range3));
    assert!(!range1.overlaps(&range3));
    assert!(!range1.overlaps(&range4));
}

#[test]
fn test_dmi_data_access_rights() {
    let mut data = vec![0u8; 1024];

    let dmi_ro = DmiData {
        dmi_ptr: data.as_mut_ptr(),
        dmi_size: 1024,
        access_rights: DmiAccessRights::ReadOnly,
        read_latency: ScTime::zero(),
        write_latency: ScTime::zero(),
        start_address: 0x1000,
        end_address: 0x13FF,
    };

    assert!(dmi_ro.allows_read());
    assert!(!dmi_ro.allows_write());

    let dmi_wo = DmiData {
        dmi_ptr: data.as_mut_ptr(),
        dmi_size: 1024,
        access_rights: DmiAccessRights::WriteOnly,
        read_latency: ScTime::zero(),
        write_latency: ScTime::zero(),
        start_address: 0x1000,
        end_address: 0x13FF,
    };

    assert!(!dmi_wo.allows_read());
    assert!(dmi_wo.allows_write());
}
