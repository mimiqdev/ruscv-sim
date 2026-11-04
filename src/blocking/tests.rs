//! Adapter parity and negative-control tests for the blocking RAM route.

use super::test_support::{Call, Mutation, ScriptTarget};
use super::*;
use crate::memory::{MemoryError, SimpleMemory};
use crate::physical::{
    AtomicAccess, AtomicAccessError, AtomicOrdering, AtomicRequest, PhysicalAccessError,
    PhysicalProtocolError, PhysicalRequest, PhysicalTargetRejectionReason, PhysicalWidth,
    ValidatedPhysicalAccess,
};
use crate::ram::RamSlot;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

const BASE: u64 = 0x8000_0000;
const SIZE: usize = 0x400;

fn native_slot(size: usize) -> RamSlot {
    RamSlot::native(Arc::new(Mutex::new(SimpleMemory::new(size))), BASE, size)
}

fn tlm_slot(target: &Arc<ScriptTarget>, size: usize) -> RamSlot {
    RamSlot::tlm(target.clone(), BASE, size)
}

#[test]
fn widths_and_raw_bytes_match_the_native_ram_route() {
    let mut native = ValidatedPhysicalAccess::new(native_slot(SIZE));
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let mut tlm = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));

    for width in [1usize, 2, 4, 8] {
        let physical_width = PhysicalWidth::from_bytes(width).unwrap();
        for offset in [0u64, 8, 0x20] {
            let paddr = BASE + offset;
            let pattern: Vec<u8> = (0..width)
                .map(|index| {
                    0xa5u8
                        .wrapping_add((index * 17) as u8)
                        .wrapping_add(offset as u8)
                })
                .collect();

            let write = PhysicalRequest::data_write(paddr, physical_width, &pattern).unwrap();
            assert!(native.access(write).unwrap().is_write_acknowledgement());
            let write = PhysicalRequest::data_write(paddr, physical_width, &pattern).unwrap();
            assert!(tlm.access(write).unwrap().is_write_acknowledgement());

            let read = PhysicalRequest::data_read(paddr, physical_width).unwrap();
            let native_bytes = native.access(read).unwrap().read_bytes().unwrap().to_vec();
            let read = PhysicalRequest::data_read(paddr, physical_width).unwrap();
            let tlm_bytes = tlm.access(read).unwrap().read_bytes().unwrap().to_vec();

            assert_eq!(native_bytes, pattern, "width {width} at {offset:#x}");
            assert_eq!(
                tlm_bytes, native_bytes,
                "width {width} at {offset:#x}: raw bytes differ from native"
            );
        }
    }

    // Fetch is the same raw-byte mapping with the Fetch extension.
    let fetch = PhysicalRequest::fetch(BASE, PhysicalWidth::Word).unwrap();
    let native_bytes = native.access(fetch).unwrap().read_bytes().unwrap().to_vec();
    let fetch = PhysicalRequest::fetch(BASE, PhysicalWidth::Word).unwrap();
    let tlm_bytes = tlm.access(fetch).unwrap().read_bytes().unwrap().to_vec();
    assert_eq!(tlm_bytes, native_bytes);
    let calls = target.log();
    assert_eq!(calls.last().unwrap().access_kind, PhysicalAccessKind::Fetch);
}

#[test]
fn every_response_status_maps_to_the_contract_category() {
    let expected = [
        (BlockingResponseStatus::Ok, None),
        (BlockingResponseStatus::AddressError, Some("target")),
        (BlockingResponseStatus::CommandError, Some("target")),
        (BlockingResponseStatus::BurstError, Some("target")),
        (BlockingResponseStatus::ByteEnableError, Some("target")),
        (BlockingResponseStatus::GenericError, Some("host")),
    ];
    for (status, category) in expected {
        let target = Arc::new(ScriptTarget::new(BASE, SIZE));
        target.set_status(status);
        let mut port = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));
        let request = PhysicalRequest::data_read(BASE + 8, PhysicalWidth::Word).unwrap();
        let result = port.access(request);
        match category {
            None => assert!(result.is_ok(), "{status:?} was not a completion"),
            Some("target") => {
                let error = result.unwrap_err();
                assert!(
                    matches!(
                        &error,
                        PhysicalAccessError::TargetRejected(rejection)
                            if rejection.reason == PhysicalTargetRejectionReason::TargetError
                    ),
                    "{status:?} was not a guest-visible target fault: {error:?}"
                );
                assert!(
                    error.to_string().contains(status.name()),
                    "{status:?} lost its IEEE name: {error}"
                );
            }
            Some("host") => {
                let error = result.unwrap_err();
                assert!(
                    matches!(&error, PhysicalAccessError::BackendFailure(_)),
                    "{status:?} was not a host failure: {error:?}"
                );
                assert!(
                    error.to_string().contains(status.name()),
                    "{status:?} lost its IEEE name: {error}"
                );
            }
            Some(other) => panic!("unknown expected category {other}"),
        }
    }

    // A left-incomplete payload is a protocol failure, never a completion.
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    target.set_status(BlockingResponseStatus::Incomplete);
    let mut port = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));
    let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    assert!(
        matches!(
            port.access(request),
            Err(PhysicalAccessError::Protocol(
                PhysicalProtocolError::BackendProtocol { .. }
            ))
        ),
        "an incomplete payload was accepted as a completion"
    );
}

#[test]
fn an_incomplete_or_modified_payload_is_not_a_completion() {
    for mutation in [Mutation::Address, Mutation::Command, Mutation::Length] {
        let target = Arc::new(ScriptTarget::new(BASE, SIZE));
        target.set_mutation(mutation);
        let mut port = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));
        let request =
            PhysicalRequest::data_write(BASE + 8, PhysicalWidth::Word, &[1, 2, 3, 4]).unwrap();
        let result = port.access(request);
        assert!(result.is_err(), "{mutation:?} was accepted: {result:?}");
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                PhysicalAccessError::Protocol(PhysicalProtocolError::BackendProtocol { .. })
            ),
            "{mutation:?} was not a protocol failure: {error:?}"
        );
        assert!(
            error.to_string().contains("changed the request payload"),
            "{mutation:?} lost its protocol classification: {error}"
        );
    }
}

#[test]
fn one_access_issues_exactly_one_transaction() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let mut port = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));
    let payload = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let write = PhysicalRequest::data_write(BASE + 8, PhysicalWidth::Doubleword, &payload).unwrap();
    port.access(write).unwrap();
    let read = PhysicalRequest::data_read(BASE + 8, PhysicalWidth::Byte).unwrap();
    port.access(read).unwrap();
    let fetch = PhysicalRequest::fetch(BASE + 16, PhysicalWidth::Word).unwrap();
    port.access(fetch).unwrap();

    assert_eq!(
        target.log(),
        vec![
            Call {
                address: BASE + 8,
                command: BlockingCommand::Write,
                length: 8,
                access_kind: PhysicalAccessKind::DataWrite,
                initiator: BlockingInitiator::Hart,
            },
            Call {
                address: BASE + 8,
                command: BlockingCommand::Read,
                length: 1,
                access_kind: PhysicalAccessKind::DataRead,
                initiator: BlockingInitiator::Hart,
            },
            Call {
                address: BASE + 16,
                command: BlockingCommand::Read,
                length: 4,
                access_kind: PhysicalAccessKind::Fetch,
                initiator: BlockingInitiator::Hart,
            },
        ],
        "one physical access must become exactly one blocking transaction"
    );
    assert_eq!(target.transactions.load(Ordering::SeqCst), 3);
}

#[test]
fn an_unknown_completion_blocks_further_access_until_reset() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    target.set_unknown(true);
    let request =
        PhysicalRequest::data_write(BASE + 8, PhysicalWidth::Word, &[1, 2, 3, 4]).unwrap();
    assert!(matches!(
        port.access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);
    assert!(slot.poison_reason().is_some());

    // No access may reach the target again before an acknowledged reset.
    let request = PhysicalRequest::data_read(BASE + 8, PhysicalWidth::Word).unwrap();
    assert!(matches!(
        port.access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        1,
        "a poisoned transport issued another access"
    );

    // A refused reset cannot unpoison the transport.
    target.set_unknown(false);
    target.set_reset_ack(false);
    assert!(slot.begin_window().is_err());
    let request = PhysicalRequest::data_read(BASE + 8, PhysicalWidth::Word).unwrap();
    assert!(matches!(
        port.access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);

    // An acknowledged reset unpoisons it; its debug zero-fill is not a
    // transaction.
    target.set_reset_ack(true);
    slot.begin_window().unwrap();
    assert_eq!(target.resets.load(Ordering::SeqCst), 2);
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);
    let request =
        PhysicalRequest::data_write(BASE + 8, PhysicalWidth::Word, &[9, 8, 7, 6]).unwrap();
    port.access(request).unwrap();
    assert_eq!(target.transactions.load(Ordering::SeqCst), 2);
    assert_eq!(target.bytes_at(BASE + 8, 4), [9, 8, 7, 6]);
}

#[test]
fn a_target_panic_is_an_unknown_completion() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    target.set_panic(true);
    let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    assert!(matches!(
        port.access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert!(slot.poison_reason().is_some());
    target.set_panic(false);
    let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    assert!(matches!(
        port.access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);
    slot.begin_window().unwrap();
    let request = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    assert!(port.access(request).is_ok());
}

#[test]
fn a_non_zero_annotated_delay_is_an_adapter_failure() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let mut port = ValidatedPhysicalAccess::new(tlm_slot(&target, SIZE));
    target.set_delay(1);
    let request = PhysicalRequest::data_write(BASE, PhysicalWidth::Byte, &[1]).unwrap();
    let error = port.access(request).unwrap_err();
    assert!(
        matches!(error, PhysicalAccessError::BackendFailure(_)),
        "an annotated delay was not an adapter failure: {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("delay conversion is not implemented"),
        "{error}"
    );
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);
    target.set_delay(0);
    let request = PhysicalRequest::data_write(BASE, PhysicalWidth::Byte, &[2]).unwrap();
    assert!(port.access(request).unwrap().is_write_acknowledgement());
}

#[test]
fn completed_writes_are_recorded_for_committed_write_visibility() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert_eq!(slot.committed_write_version(), Some(0));
    let write = PhysicalRequest::data_write(BASE, PhysicalWidth::Word, &[1, 2, 3, 4]).unwrap();
    port.access(write).unwrap();
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a completed write must be recorded"
    );
    let read = PhysicalRequest::data_read(BASE, PhysicalWidth::Word).unwrap();
    port.access(read).unwrap();
    assert_eq!(slot.committed_write_version(), Some(1));
    target.set_status(BlockingResponseStatus::GenericError);
    let write = PhysicalRequest::data_write(BASE, PhysicalWidth::Word, &[5, 6, 7, 8]).unwrap();
    port.access(write).unwrap_err();
    assert_eq!(
        slot.committed_write_version(),
        Some(1),
        "a rejected write is not a committed write"
    );
    assert_eq!(target.bytes_at(BASE, 4), [1, 2, 3, 4]);
}

#[test]
fn atomics_fail_closed_without_a_transaction() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    let request =
        AtomicRequest::load_reserved(BASE + 8, PhysicalWidth::Word, AtomicOrdering::default())
            .unwrap();
    let error = port.access_atomic(request).unwrap_err();
    assert!(
        matches!(
            &error,
            AtomicAccessError::TargetRejected(rejection)
                if rejection.reason == PhysicalTargetRejectionReason::UnsupportedCategory
        ),
        "an atomic was not refused with the access-fault cause: {error:?}"
    );
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "an atomic reached the blocking target"
    );

    // A poisoned transport reports unknown completion before the capability
    // rejection, because no access may be attempted at all.
    target.set_unknown(true);
    let write = PhysicalRequest::data_write(BASE, PhysicalWidth::Word, &[0; 4]).unwrap();
    let mut port = ValidatedPhysicalAccess::new(slot.clone());
    assert!(matches!(
        port.access(write),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    let request =
        AtomicRequest::load_reserved(BASE + 8, PhysicalWidth::Word, AtomicOrdering::default())
            .unwrap();
    assert!(matches!(
        port.access_atomic(request),
        Err(AtomicAccessError::UnknownCompletion(_))
    ));
    assert_eq!(target.transactions.load(Ordering::SeqCst), 1);
}

#[test]
fn host_views_use_debug_transport_not_blocking_transactions() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    slot.program(8, &[1, 2, 3, 4]).unwrap();
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "installation used b_transport"
    );
    assert_eq!(slot.host_read(8, 4).unwrap(), vec![1, 2, 3, 4]);
    assert_eq!(slot.read_word(8).unwrap(), 0x0403_0201);
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "a host view used b_transport"
    );
    assert!(target.debug_requests.load(Ordering::SeqCst) >= 2);
    let before = target.transactions.load(Ordering::SeqCst);
    slot.begin_window().unwrap();
    assert_eq!(target.transactions.load(Ordering::SeqCst), before);
    assert_eq!(slot.host_read(0, 4).unwrap(), vec![0, 0, 0, 0]);
}

#[test]
fn a_failed_or_short_debug_transport_is_a_host_failure() {
    let target = Arc::new(ScriptTarget::new(BASE, SIZE));
    let slot = tlm_slot(&target, SIZE);
    target.set_debug_short(true);
    let error = slot.host_read(0, 4).unwrap_err();
    assert!(
        matches!(&error, MemoryError::Backend(_)),
        "a short debug transport was not a host failure: {error:?}"
    );
    assert!(error.to_string().contains("transferred"));
    assert!(slot.program(0, &[1, 2, 3]).is_err());
    target.set_debug_short(false);
    slot.program(0, &[1, 2, 3]).unwrap();
    assert_eq!(slot.host_read(0, 3).unwrap(), vec![1, 2, 3]);
}

#[test]
fn femtoseconds_arithmetic_is_checked() {
    assert_eq!(
        Femtoseconds::new(3).checked_add(Femtoseconds::new(4)),
        Some(Femtoseconds::new(7))
    );
    assert_eq!(
        Femtoseconds::new(u64::MAX).checked_add(Femtoseconds::new(1)),
        None
    );
    assert_eq!(Femtoseconds::new(u64::MAX).checked_mul(2), None);
    assert_eq!(
        Femtoseconds::new(1_000).checked_mul(1_000),
        Some(Femtoseconds::new(1_000_000))
    );
}
