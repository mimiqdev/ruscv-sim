//! Executable A7 T1 contract tests.
//!
//! These tests cover only the transport-neutral non-atomic vocabulary and its
//! validation boundary.  They intentionally do not connect `RiscvCore`,
//! `SystemBus`, `RiscVSimulator`, native devices, MMU/TLM, or any AMO/LR/SC
//! path.  The fake backend records the request before returning a deliberately
//! selected result so malformed requests and malformed responses cannot be
//! mistaken for target faults or successful accesses.

use ruscv_sim::{
    AccessCategory, AccessWidth, PhysicalAccessError, PhysicalBackend, PhysicalBackendError,
    PhysicalBackendResult, PhysicalCompletion, PhysicalCompletionKind, PhysicalProtocolError,
    PhysicalRequest, PhysicalRequestDescriptor, PhysicalResponse, PhysicalResponseBytes,
    PhysicalTargetRejectionReason, ValidatedPhysicalAccess,
};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedCall {
    descriptor: PhysicalRequestDescriptor,
    payload: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
struct SpyBackend {
    calls: Vec<ObservedCall>,
    planned: VecDeque<PhysicalBackendResult>,
}

impl SpyBackend {
    fn push(&mut self, result: PhysicalBackendResult) {
        self.planned.push_back(result);
    }
}

impl PhysicalBackend for SpyBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.push(ObservedCall {
            descriptor: request.descriptor(),
            payload: request.payload().map(|payload| payload.to_vec()),
        });
        self.planned
            .pop_front()
            .expect("the test must plan every backend completion")
    }
}

fn bytes_for(width: AccessWidth) -> &'static [u8] {
    match width {
        AccessWidth::Byte => &[0xd3],
        AccessWidth::Halfword => &[0xd3, 0x07],
        AccessWidth::Word => &[0xd3, 0x07, 0xa9, 0x4e],
        AccessWidth::Doubleword => &[0xd3, 0x07, 0xa9, 0x4e, 0x80, 0x21, 0xfe, 0x65],
    }
}

#[test]
fn complete_fetch_read_and_write_ack_cover_all_explicit_widths() {
    let mut backend = SpyBackend::default();
    let widths = [
        AccessWidth::Byte,
        AccessWidth::Halfword,
        AccessWidth::Word,
        AccessWidth::Doubleword,
    ];

    for (index, width) in widths.into_iter().enumerate() {
        let request = PhysicalRequest::data_read(0x1000 + index as u64 * 0x10, width).unwrap();
        backend.push(Ok(PhysicalResponse::read_for(&request, bytes_for(width))));
    }

    let fetch = PhysicalRequest::fetch(0x2000, AccessWidth::Word).unwrap();
    backend.push(Ok(PhysicalResponse::read_for(
        &fetch,
        &[0x13, 0x37, 0xc0, 0xde],
    )));

    let write_payloads: [&[u8]; 4] = [
        &[0x91],
        &[0x91, 0x02],
        &[0x91, 0x02, 0xe7, 0x40],
        &[0x91, 0x02, 0xe7, 0x40, 0x5b, 0x08, 0xc6, 0xaf],
    ];
    for (index, (width, payload)) in widths.into_iter().zip(write_payloads).enumerate() {
        let request =
            PhysicalRequest::data_write(0x3000 + index as u64 * 0x10, width, payload).unwrap();
        backend.push(Ok(PhysicalResponse::write_ack_for(&request)));
    }

    let mut port = ValidatedPhysicalAccess::new(backend);

    for (index, width) in widths.into_iter().enumerate() {
        let request = PhysicalRequest::data_read(0x1000 + index as u64 * 0x10, width).unwrap();
        let response = port.access(request).unwrap();
        assert_eq!(response.read_bytes(), Some(bytes_for(width)));
        assert!(!response.is_write_acknowledgement());
    }

    let response = port.access(fetch).unwrap();
    assert_eq!(response.read_bytes(), Some(&[0x13, 0x37, 0xc0, 0xde][..]));

    for (index, (width, payload)) in widths.into_iter().zip(write_payloads).enumerate() {
        let request =
            PhysicalRequest::data_write(0x3000 + index as u64 * 0x10, width, payload).unwrap();
        let response = port.access(request).unwrap();
        assert!(response.is_write_acknowledgement());
        assert_eq!(response.read_bytes(), None);
    }

    let backend = port.into_backend();
    assert_eq!(backend.calls.len(), 9);
    for (index, width) in widths.into_iter().enumerate() {
        assert_eq!(
            backend.calls[index].descriptor,
            PhysicalRequestDescriptor::new(
                0x1000 + index as u64 * 0x10,
                width,
                AccessCategory::DataRead,
            )
        );
        assert_eq!(backend.calls[index].payload, None);
    }
    assert_eq!(backend.calls[4].descriptor.category, AccessCategory::Fetch);
    assert_eq!(backend.calls[4].descriptor.width, AccessWidth::Word);
    for (index, payload) in write_payloads.into_iter().enumerate() {
        assert_eq!(backend.calls[5 + index].payload, Some(payload.to_vec()));
        assert_eq!(
            backend.calls[5 + index].descriptor.category,
            AccessCategory::DataWrite
        );
    }
}

#[test]
fn asymmetric_bytes_are_transferred_in_physical_address_order() {
    let read_bytes = [0x80, 0x01, 0xfe, 0x7f, 0x22, 0xc3, 0x04, 0xb5];
    let write_bytes = [0x19, 0xa2, 0x03, 0xf4, 0x85, 0x16, 0xe7, 0x68];
    let request = PhysicalRequest::data_read(0x4400, AccessWidth::Doubleword).unwrap();

    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::read_for(&request, &read_bytes)));
    let write = PhysicalRequest::data_write(0x4480, AccessWidth::Doubleword, &write_bytes).unwrap();
    backend.push(Ok(PhysicalResponse::write_ack_for(&write)));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let response = port.access(request).unwrap();
    assert_eq!(response.read_bytes(), Some(&read_bytes[..]));
    let response = port.access(write).unwrap();
    assert!(response.is_write_acknowledgement());

    let backend = port.into_backend();
    assert_eq!(backend.calls[0].payload, None);
    assert_eq!(backend.calls[1].payload, Some(write_bytes.to_vec()));
    assert_eq!(
        backend.calls[0].descriptor.paddr, 0x4400,
        "raw byte order does not substitute a host integer layout"
    );
}

#[test]
fn malformed_width_and_payload_requests_fail_before_backend_dispatch() {
    let mut backend = SpyBackend::default();
    // No completion is planned: any backend call would panic and fail this test.
    let port = ValidatedPhysicalAccess::new(backend);

    assert!(matches!(
        AccessWidth::try_from(0usize),
        Err(PhysicalProtocolError::ZeroWidth)
    ));
    assert!(matches!(
        AccessWidth::try_from(3usize),
        Err(PhysicalProtocolError::InvalidWidth { requested: 3 })
    ));
    assert!(matches!(
        PhysicalRequest::from_parts(AccessCategory::DataRead, 0x10, 0, None),
        Err(PhysicalProtocolError::ZeroWidth)
    ));
    assert!(matches!(
        PhysicalRequest::from_parts(AccessCategory::DataRead, 0x10, 3, None),
        Err(PhysicalProtocolError::InvalidWidth { requested: 3 })
    ));

    assert!(matches!(
        PhysicalRequest::data_read(0x10, AccessWidth::Word).and_then(|_| {
            PhysicalRequest::new(
                AccessCategory::DataRead,
                0x10,
                AccessWidth::Word,
                Some(&[0xaa, 0xbb]),
            )
        }),
        Err(PhysicalProtocolError::UnexpectedPayload {
            category: AccessCategory::DataRead,
            actual: 2
        })
    ));
    assert!(matches!(
        PhysicalRequest::fetch(0x10, AccessWidth::Word).and_then(|_| {
            PhysicalRequest::new(
                AccessCategory::Fetch,
                0x10,
                AccessWidth::Word,
                Some(&[0xaa]),
            )
        }),
        Err(PhysicalProtocolError::UnexpectedPayload {
            category: AccessCategory::Fetch,
            actual: 1
        })
    ));
    assert!(matches!(
        PhysicalRequest::new(AccessCategory::DataWrite, 0x10, AccessWidth::Word, None,),
        Err(PhysicalProtocolError::PayloadLength {
            category: AccessCategory::DataWrite,
            expected: 4,
            actual: 0
        })
    ));
    assert!(matches!(
        PhysicalRequest::data_write(0x10, AccessWidth::Word, &[1, 2, 3]),
        Err(PhysicalProtocolError::PayloadLength {
            expected: 4,
            actual: 3,
            ..
        })
    ));
    assert!(matches!(
        PhysicalRequest::data_write(0x10, AccessWidth::Word, &[1, 2, 3, 4, 5]),
        Err(PhysicalProtocolError::PayloadLength {
            expected: 4,
            actual: 5,
            ..
        })
    ));

    // The constructor invariants make these malformed values impossible to
    // submit to the port; the spy therefore remains untouched.
    assert!(port.backend().calls.is_empty());
    // Keep the binding mutable in this test to make the no-dispatch proof
    // explicit if the backend implementation changes later.
    backend = port.into_backend();
    assert!(backend.calls.is_empty());
}

#[test]
fn final_nonwrapping_byte_is_valid_but_overflowing_span_is_target_rejected() {
    let final_byte = PhysicalRequest::data_read(u64::MAX, AccessWidth::Byte).unwrap();
    assert_eq!(final_byte.span().checked_end_inclusive(), Some(u64::MAX));
    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::read_for(&final_byte, &[0x7f])));
    let mut port = ValidatedPhysicalAccess::new(backend);
    assert_eq!(
        port.access(final_byte).unwrap().read_bytes(),
        Some(&[0x7f][..])
    );
    assert_eq!(port.backend().calls.len(), 1);

    let request = PhysicalRequest::data_read(u64::MAX, AccessWidth::Doubleword).unwrap();
    assert_eq!(request.span().checked_end_inclusive(), None);

    let mut port = ValidatedPhysicalAccess::new(SpyBackend::default());
    let error = port.access(request).unwrap_err();
    let PhysicalAccessError::TargetRejected(rejection) = error else {
        panic!("a wrapping physical span must be a target rejection");
    };
    assert_eq!(rejection.request.paddr, u64::MAX);
    assert_eq!(rejection.request.width, AccessWidth::Doubleword);
    assert_eq!(rejection.request.category, AccessCategory::DataRead);
    assert_eq!(rejection.span, request.span());
    assert_eq!(
        rejection.reason,
        PhysicalTargetRejectionReason::RangeOverflow
    );
    assert!(!rejection.context.is_empty());
    assert!(port.backend().calls.is_empty());
}

#[test]
fn valid_target_refusal_preserves_request_span_category_width_and_context() {
    let request = PhysicalRequest::data_write(
        0x5000,
        AccessWidth::Doubleword,
        &[0x91, 0x02, 0xe7, 0x40, 0x5b, 0x08, 0xc6, 0xaf],
    )
    .unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Err(PhysicalBackendError::target(
        PhysicalTargetRejectionReason::UnsupportedWidth,
        "device accepts byte writes only",
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let error = port.access(request).unwrap_err();
    let PhysicalAccessError::TargetRejected(rejection) = error else {
        panic!("a target refusal must not become a protocol or host failure");
    };
    assert_eq!(rejection.request, request.descriptor());
    assert_eq!(rejection.span, request.span());
    assert_eq!(rejection.span.checked_end_inclusive(), Some(0x5007));
    assert_eq!(rejection.request.width, AccessWidth::Doubleword);
    assert_eq!(rejection.request.category, AccessCategory::DataWrite);
    assert_eq!(
        rejection.reason,
        PhysicalTargetRejectionReason::UnsupportedWidth
    );
    assert_eq!(rejection.context, "device accepts byte writes only");
    assert_eq!(port.backend().calls.len(), 1);
}

#[test]
fn host_failure_is_typed_and_never_guest_target_rejection() {
    let request = PhysicalRequest::fetch(0x6000, AccessWidth::Word).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Err(PhysicalBackendError::host("RAM lock poisoned")));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let error = port.access(request).unwrap_err();
    let PhysicalAccessError::BackendFailure(failure) = error else {
        panic!("host/backend failure must retain its simulator-side category");
    };
    assert_eq!(failure.request, request.descriptor());
    assert_eq!(failure.context, "RAM lock poisoned");
    assert_eq!(port.backend().calls.len(), 1);
}

#[test]
fn short_and_long_read_responses_are_protocol_failures_not_success() {
    let short_request = PhysicalRequest::data_read(0x7000, AccessWidth::Word).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::read_for(
        &short_request,
        &[0xa1, 0xb2, 0xc3],
    )));
    let long_request = PhysicalRequest::fetch(0x7004, AccessWidth::Word).unwrap();
    backend.push(Ok(PhysicalResponse::new(
        long_request.descriptor(),
        PhysicalCompletion::Read(PhysicalResponseBytes::with_reported_len(
            &[0x01, 0x02, 0x03, 0x04, 0x05],
            5,
        )),
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let short = port.access(short_request).unwrap_err();
    assert!(matches!(
        short,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseLengthMismatch {
            expected: 4,
            actual: 3
        })
    ));
    let long = port.access(long_request).unwrap_err();
    assert!(matches!(
        long,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseLengthMismatch {
            expected: 4,
            actual: 5
        })
    ));
    assert_eq!(port.backend().calls.len(), 2);
}

#[test]
fn reported_length_cannot_pad_or_truncate_supplied_response_bytes() {
    let padded_request = PhysicalRequest::data_read(0x7010, AccessWidth::Word).unwrap();
    let truncated_request = PhysicalRequest::fetch(0x7014, AccessWidth::Word).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::new(
        padded_request.descriptor(),
        PhysicalCompletion::Read(PhysicalResponseBytes::with_reported_len(&[0xaa], 4)),
    )));
    backend.push(Ok(PhysicalResponse::new(
        truncated_request.descriptor(),
        PhysicalCompletion::Read(PhysicalResponseBytes::with_reported_len(
            &[0xaa, 0xbb, 0xcc, 0xdd, 0xee],
            4,
        )),
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let padded = port.access(padded_request).unwrap_err();
    assert!(matches!(
        padded,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponsePayloadLengthMismatch {
            reported: 4,
            supplied: 1
        })
    ));
    let truncated = port.access(truncated_request).unwrap_err();
    assert!(matches!(
        truncated,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponsePayloadLengthMismatch {
            reported: 4,
            supplied: 5
        })
    ));
    assert_eq!(port.backend().calls.len(), 2);
}

#[test]
fn wrong_category_and_mismatched_request_binding_are_protocol_failures() {
    let request = PhysicalRequest::data_read(0x8000, AccessWidth::Word).unwrap();
    let wrong_category =
        PhysicalRequestDescriptor::new(request.paddr(), request.width(), AccessCategory::Fetch);
    let mismatched_address =
        PhysicalRequestDescriptor::new(request.paddr() + 4, request.width(), request.category());
    let mismatched_width =
        PhysicalRequestDescriptor::new(request.paddr(), AccessWidth::Halfword, request.category());
    let bytes = PhysicalResponseBytes::from_slice(&[0x91, 0x02, 0xe7, 0x40]);
    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::new(
        wrong_category,
        PhysicalCompletion::Read(bytes),
    )));
    backend.push(Ok(PhysicalResponse::new(
        mismatched_address,
        PhysicalCompletion::Read(bytes),
    )));
    backend.push(Ok(PhysicalResponse::new(
        mismatched_width,
        PhysicalCompletion::Read(bytes),
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let wrong_category_error = port.access(request).unwrap_err();
    let PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseBindingMismatch {
        expected,
        actual,
    }) = wrong_category_error
    else {
        panic!("a response with the wrong category must be rejected");
    };
    assert_eq!(expected.category, AccessCategory::DataRead);
    assert_eq!(actual.category, AccessCategory::Fetch);

    let mismatched_error = port.access(request).unwrap_err();
    assert!(matches!(
        mismatched_error,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseBindingMismatch {
            expected,
            actual
        }) if expected.paddr == 0x8000 && actual.paddr == 0x8004
    ));

    let width_error = port.access(request).unwrap_err();
    assert!(matches!(
        width_error,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseBindingMismatch {
            expected,
            actual
        }) if expected.width == AccessWidth::Word && actual.width == AccessWidth::Halfword
    ));
}

#[test]
fn contradictory_completion_kinds_are_protocol_failures() {
    let write = PhysicalRequest::data_write(0x9000, AccessWidth::Word, &[1, 2, 3, 4]).unwrap();
    let read = PhysicalRequest::data_read(0x9004, AccessWidth::Word).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Ok(PhysicalResponse::new(
        write.descriptor(),
        PhysicalCompletion::Read(PhysicalResponseBytes::from_slice(&[1, 2, 3, 4])),
    )));
    backend.push(Ok(PhysicalResponse::new(
        read.descriptor(),
        PhysicalCompletion::WriteAcknowledgement,
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let write_error = port.access(write).unwrap_err();
    assert!(matches!(
        write_error,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseCompletionMismatch {
            expected: PhysicalCompletionKind::WriteAcknowledgement,
            actual: PhysicalCompletionKind::Read,
        })
    ));
    let read_error = port.access(read).unwrap_err();
    assert!(matches!(
        read_error,
        PhysicalAccessError::Protocol(PhysicalProtocolError::ResponseCompletionMismatch {
            expected: PhysicalCompletionKind::Read,
            actual: PhysicalCompletionKind::WriteAcknowledgement,
        })
    ));
}

#[test]
fn backend_protocol_failure_is_typed_and_not_guessed_from_text() {
    let request = PhysicalRequest::data_read(0xa000, AccessWidth::Halfword).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Err(PhysicalBackendError::protocol(
        "backend response state was contradictory",
    )));
    let mut port = ValidatedPhysicalAccess::new(backend);

    let error = port.access(request).unwrap_err();
    assert!(matches!(
        error,
        PhysicalAccessError::Protocol(PhysicalProtocolError::BackendProtocol { context })
            if context == "backend response state was contradictory"
    ));
}

#[test]
fn unknown_completion_is_terminal_and_is_never_retried_or_mapped() {
    let request = PhysicalRequest::data_write(0xb000, AccessWidth::Byte, &[0x5a]).unwrap();
    let mut backend = SpyBackend::default();
    backend.push(Err(PhysicalBackendError::unknown(
        "device callback may have committed",
    )));
    // A second planned result would make an accidental retry observable; the
    // validated boundary must consume exactly one backend call.
    let mut port = ValidatedPhysicalAccess::new(backend);

    let error = port.access(request).unwrap_err();
    let PhysicalAccessError::UnknownCompletion(unknown) = error else {
        panic!("unknown completion must remain a terminal simulator failure");
    };
    assert_eq!(unknown.request, request.descriptor());
    assert_eq!(unknown.context, "device callback may have committed");
    assert_eq!(port.backend().calls.len(), 1);
    assert_eq!(port.backend().planned.len(), 0);
}
