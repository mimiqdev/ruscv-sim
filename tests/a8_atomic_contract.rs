//! Executable A8 T1 contract tests (dev-plan §8, task T1).
//!
//! These tests cover only the atomic operation envelope vocabulary in
//! `src/physical.rs` and the single Hart-owned AMO arithmetic module
//! (`src/hart_amods.rs`).  They are target-free by scope: no RAM, UART, HTIF,
//! or executor wiring is exercised.  The spy backends record every observed
//! call before returning a deliberately planned result, so malformed
//! envelopes, malformed responses, and retried completions cannot be mistaken
//! for target faults or successful transactions.
//!
//! Contract rows proven here (dev-plan §5.2 M1, §5.7):
//!
//! 1. Atomic envelope widths are 4/8 only; byte/halfword envelopes are
//!    malformed request input, never target refusals.
//! 2. Payload rules: RMW operand bytes and transform, SC write payload and
//!    reservation context (span + snapshot), LR carries nothing extra.
//! 3. Every AMO operation (all nine) applies through the one Hart-owned
//!    arithmetic module at both widths; the envelope carries the transform
//!    opaquely and the vocabulary adds no arithmetic of its own.
//! 4. Response binding and completion-kind validation.
//! 5. One envelope is exactly one backend call, with no retry path.
//! 6. Unknown completion is terminal: no retry, effects stay unresolved.
//! 7. Malformed envelopes are rejected before backend dispatch.
//! 8. An ordinary fetch/read/write pair cannot represent an AMO or an SC.

use ruscv_sim::hart_amods::{self, AmoArithmeticError, AmoOperation};
use ruscv_sim::physical::{
    AccessWidth, AmoWidth, AtomicAccessError, AtomicAccessKind, AtomicBackend, AtomicBackendResult,
    AtomicOrdering, AtomicProtocolError, AtomicRequest, AtomicRequestDescriptor,
    AtomicReservationContext, AtomicResponse, AtomicResponseCompletion, CommittedWriteSnapshot,
    ConditionalStatus, PhysicalBackend, PhysicalBackendError, PhysicalBackendResult,
    PhysicalRequest, PhysicalResponse, PhysicalResponseBytes, PhysicalResponseCompletion,
    PhysicalSpan, PhysicalTargetRejectionReason, ValidatedAtomicAccess, ValidatedPhysicalAccess,
    MAX_COMMITTED_WRITE_SNAPSHOT_BYTES, MAX_PHYSICAL_ACCESS_BYTES,
};
use std::collections::VecDeque;

const AQ_RL: AtomicOrdering = AtomicOrdering { aq: true, rl: true };

// ---------------------------------------------------------------------------
// Spy backend: records calls, replans deliberately selected completions.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedAtomicCall {
    descriptor: AtomicRequestDescriptor,
    ordering: AtomicOrdering,
    operand: Option<Vec<u8>>,
    transform_width: Option<usize>,
    store_payload: Option<Vec<u8>>,
    reservation: Option<(PhysicalSpan, Vec<u8>)>,
}

#[derive(Debug, Default)]
struct AtomicSpyBackend {
    calls: Vec<ObservedAtomicCall>,
    planned: VecDeque<AtomicBackendResult>,
}

impl AtomicSpyBackend {
    fn push(&mut self, result: AtomicBackendResult) {
        self.planned.push_back(result);
    }

    fn observe(&mut self, request: &AtomicRequest<'_>) {
        self.calls.push(ObservedAtomicCall {
            descriptor: request.descriptor(),
            ordering: request.ordering(),
            operand: request.operand_bytes().map(<[u8]>::to_vec),
            transform_width: request
                .transform()
                .map(|transform| transform.width().bytes()),
            store_payload: request.store_payload().map(<[u8]>::to_vec),
            reservation: request
                .reservation()
                .map(|context| (context.reserved, context.snapshot.as_bytes().to_vec())),
        });
    }
}

impl AtomicBackend for AtomicSpyBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.observe(request);
        self.planned
            .pop_front()
            .expect("the test must plan every atomic backend completion")
    }
}

/// An RMW backend that models the M1 critical section: it applies the
/// request's Hart transform to planned old bytes, records the bytes it wrote
/// (the only Hart-visible effect), and answers with the exact old bytes.
struct CriticalSectionRmwBackend {
    calls: Vec<ObservedAtomicCall>,
    old_bytes: Vec<u8>,
    written: Vec<[u8; MAX_PHYSICAL_ACCESS_BYTES]>,
}

impl CriticalSectionRmwBackend {
    fn new(old_bytes: &[u8]) -> Self {
        Self {
            calls: Vec::new(),
            old_bytes: old_bytes.to_vec(),
            written: Vec::new(),
        }
    }
}

impl AtomicBackend for CriticalSectionRmwBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        let descriptor = request.descriptor();
        let ordering = request.ordering();
        let operand = request.operand_bytes().map(<[u8]>::to_vec);
        let transform_width = request
            .transform()
            .map(|transform| transform.width().bytes());
        self.calls.push(ObservedAtomicCall {
            descriptor,
            ordering,
            operand,
            transform_width,
            store_payload: None,
            reservation: None,
        });
        let transform = request
            .transform()
            .expect("validated RMW envelopes always carry a transform");
        let operand = request
            .operand_bytes()
            .expect("validated RMW envelopes always carry operand bytes");
        // One critical section: read span → apply Hart transform → write.
        let new_bytes = transform.apply(&self.old_bytes, operand);
        self.written.push(new_bytes);
        Ok(AtomicResponse::rmw_for(request, &self.old_bytes))
    }
}

fn snapshot(bytes: &[u8]) -> CommittedWriteSnapshot {
    CommittedWriteSnapshot::from_bytes(bytes).expect("snapshot fits the inline representation")
}

fn word_rmw(paddr: u64, operand: &[u8], operation: AmoOperation) -> AtomicRequest<'_> {
    AtomicRequest::rmw(
        paddr,
        AccessWidth::Word,
        AQ_RL,
        operand,
        hart_amods::transform(operation, AmoWidth::Word),
    )
    .expect("a well-formed word RMW envelope")
}

// ---------------------------------------------------------------------------
// 1. Widths: 4/8 only.
// ---------------------------------------------------------------------------

#[test]
fn atomic_envelope_widths_are_limited_to_word_and_doubleword() {
    for width in [AccessWidth::Byte, AccessWidth::Halfword] {
        let assert_unsupported = |error: AtomicProtocolError| match error {
            AtomicProtocolError::UnsupportedAtomicWidth { requested } => {
                assert_eq!(requested, width.bytes());
            }
            other => panic!("byte/halfword envelopes are malformed input, got {other:?}"),
        };
        assert_unsupported(
            AtomicRequest::load_reserved(0x1000, width, AQ_RL)
                .expect_err("byte/halfword LR is malformed"),
        );
        // The width rule fires before the transform/operand rules: even a
        // width-mismatched transform cannot mask it.
        assert_unsupported(
            AtomicRequest::rmw(
                0x1000,
                width,
                AQ_RL,
                &[0xaa; 8],
                hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
            )
            .expect_err("byte/halfword RMW is malformed"),
        );
        let context = AtomicReservationContext {
            reserved: PhysicalSpan {
                paddr: 0x1000,
                width: AccessWidth::Word,
            },
            snapshot: snapshot(&[1, 2, 3, 4]),
        };
        assert_unsupported(
            AtomicRequest::store_conditional(0x1000, width, AQ_RL, &[0xaa; 8], context)
                .expect_err("byte/halfword SC is malformed"),
        );
    }

    // Word and Doubleword envelopes are the valid atomic widths: one of each
    // kind dispatches at each width.
    let mut backend = AtomicSpyBackend::default();
    let word_operand = [0x11, 0x22, 0x33, 0x44];
    let doubleword_operand = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let word_rm = word_rmw(0x2000, &word_operand, AmoOperation::Add);
    let doubleword_rm = AtomicRequest::rmw(
        0x2008,
        AccessWidth::Doubleword,
        AQ_RL,
        &doubleword_operand,
        hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
    )
    .unwrap();
    let word_lr = AtomicRequest::load_reserved(0x2010, AccessWidth::Word, AQ_RL).unwrap();
    let doubleword_lr =
        AtomicRequest::load_reserved(0x2018, AccessWidth::Doubleword, AQ_RL).unwrap();
    let word_context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: 0x2010,
            width: AccessWidth::Word,
        },
        snapshot: snapshot(&[0x0a]),
    };
    let word_sc = AtomicRequest::store_conditional(
        0x2010,
        AccessWidth::Word,
        AQ_RL,
        &word_operand,
        word_context,
    )
    .unwrap();
    let doubleword_context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: 0x2018,
            width: AccessWidth::Doubleword,
        },
        snapshot: snapshot(&[0x0b]),
    };
    let doubleword_sc = AtomicRequest::store_conditional(
        0x2018,
        AccessWidth::Doubleword,
        AQ_RL,
        &doubleword_operand,
        doubleword_context,
    )
    .unwrap();

    backend.push(Ok(AtomicResponse::rmw_for(&word_rm, &word_operand)));
    backend.push(Ok(AtomicResponse::rmw_for(
        &doubleword_rm,
        &doubleword_operand,
    )));
    backend.push(Ok(AtomicResponse::load_reserved_for(
        &word_lr,
        &word_operand,
        snapshot(&[1, 2, 3, 4]),
    )));
    backend.push(Ok(AtomicResponse::load_reserved_for(
        &doubleword_lr,
        &doubleword_operand,
        snapshot(&[5, 6, 7, 8, 9, 10, 11, 12]),
    )));
    backend.push(Ok(AtomicResponse::store_conditional_for(
        &word_sc,
        ConditionalStatus::Success,
    )));
    backend.push(Ok(AtomicResponse::store_conditional_for(
        &doubleword_sc,
        ConditionalStatus::Failure,
    )));

    let mut port = ValidatedAtomicAccess::new(backend);
    assert_eq!(
        port.access_atomic(word_rm).unwrap().old_bytes(),
        Some(&word_operand[..])
    );
    assert_eq!(
        port.access_atomic(doubleword_rm).unwrap().old_bytes(),
        Some(&doubleword_operand[..])
    );
    assert!(port.access_atomic(word_lr).unwrap().snapshot().is_some());
    assert!(port
        .access_atomic(doubleword_lr)
        .unwrap()
        .snapshot()
        .is_some());
    assert_eq!(
        port.access_atomic(word_sc).unwrap().conditional_status(),
        Some(ConditionalStatus::Success)
    );
    assert_eq!(
        port.access_atomic(doubleword_sc)
            .unwrap()
            .conditional_status(),
        Some(ConditionalStatus::Failure)
    );

    assert_eq!(port.backend().calls.len(), 6);
}

// ---------------------------------------------------------------------------
// 2. Payload rules, rejected before any backend dispatch.
// ---------------------------------------------------------------------------

#[test]
fn envelope_payload_rules_fail_before_backend_dispatch() {
    // No completion is planned anywhere in this test: any backend call would
    // panic and fail it.  Malformed envelopes cannot even be constructed, so
    // the spy must stay completely untouched.
    let backend = AtomicSpyBackend::default();

    // RMW operand must be exactly width bytes.
    assert!(matches!(
        AtomicRequest::rmw(
            0x3000,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Word),
        ),
        Err(AtomicProtocolError::PayloadLength {
            kind: AtomicAccessKind::Rmw,
            expected: 4,
            actual: 3,
        })
    ));
    assert!(matches!(
        AtomicRequest::rmw(
            0x3000,
            AccessWidth::Doubleword,
            AtomicOrdering::default(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
        ),
        Err(AtomicProtocolError::PayloadLength {
            kind: AtomicAccessKind::Rmw,
            expected: 8,
            actual: 9,
        })
    ));

    // The transform's encoded width must equal the envelope width.
    assert!(matches!(
        AtomicRequest::rmw(
            0x3000,
            AccessWidth::Doubleword,
            AtomicOrdering::default(),
            &[1, 2, 3, 4, 5, 6, 7, 8],
            hart_amods::transform(AmoOperation::Swap, AmoWidth::Word),
        ),
        Err(AtomicProtocolError::TransformWidthMismatch {
            expected: 8,
            actual: 4,
        })
    ));

    // Store-conditional write payload must be exactly width bytes.
    assert!(matches!(
        AtomicRequest::store_conditional(
            0x3000,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3],
            word_context_at(0x3000, &[7]),
        ),
        Err(AtomicProtocolError::PayloadLength {
            kind: AtomicAccessKind::StoreConditional,
            expected: 4,
            actual: 3,
        })
    ));
    // A store-conditional always requires a reservation context by
    // signature: the type system leaves no way to omit it.

    // The echoed snapshot must describe at least one covered block.
    assert!(matches!(
        AtomicRequest::store_conditional(
            0x3000,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3, 4],
            word_context_at(0x3000, &[]),
        ),
        Err(AtomicProtocolError::EmptySnapshot)
    ));

    // The reserved span itself must be an atomic width and must not wrap.
    assert!(matches!(
        AtomicRequest::store_conditional(
            0x3000,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3, 4],
            AtomicReservationContext {
                reserved: PhysicalSpan {
                    paddr: 0x3000,
                    width: AccessWidth::Byte,
                },
                snapshot: snapshot(&[7]),
            },
        ),
        Err(AtomicProtocolError::UnsupportedAtomicWidth { requested: 1 })
    ));
    assert!(matches!(
        AtomicRequest::store_conditional(
            u64::MAX,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3, 4],
            AtomicReservationContext {
                reserved: PhysicalSpan {
                    paddr: u64::MAX,
                    width: AccessWidth::Word,
                },
                snapshot: snapshot(&[7]),
            },
        ),
        Err(AtomicProtocolError::ReservationSpanOverflow { .. })
    ));

    // Span containment: the SC span must be contained in the reserved span.
    // Equal and interior spans are valid (the C30 shapes at vocabulary
    // level); extending past either end is not.
    let reserved_word = word_context_at(0x4000, &[9]);
    assert!(
        AtomicRequest::store_conditional(
            0x4000,
            AccessWidth::Word,
            AQ_RL,
            &[1, 2, 3, 4],
            reserved_word
        )
        .is_ok(),
        "SC.W covered by an equal LR.W span"
    );
    let reserved_doubleword = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: 0x4000,
            width: AccessWidth::Doubleword,
        },
        snapshot: snapshot(&[10]),
    };
    assert!(
        AtomicRequest::store_conditional(
            0x4004,
            AccessWidth::Word,
            AQ_RL,
            &[1, 2, 3, 4],
            reserved_doubleword,
        )
        .is_ok(),
        "SC.W@(p+4) covered by an LR.D span (C30 success direction)"
    );
    let cases: [(u64, AccessWidth, &[u8]); 3] = [
        (0x4004, AccessWidth::Word, &[1, 2, 3, 4]), // SC.W@(p+4) not covered by LR.W@p
        (0x3ffc, AccessWidth::Word, &[1, 2, 3, 4]), // SC.W@(p-4) below the reserved span
        (0x4000, AccessWidth::Doubleword, &[1, 2, 3, 4, 5, 6, 7, 8]), // LR.W@p cannot cover SC.D@p
    ];
    for (paddr, width, payload) in cases {
        let context = word_context_at(0x4000, &[9]);
        assert!(matches!(
            AtomicRequest::store_conditional(paddr, width, AQ_RL, payload, context,),
            Err(AtomicProtocolError::ReservationSpanMismatch {
                reserved,
                requested,
            }) if reserved.width == AccessWidth::Word
                && reserved.paddr == 0x4000
                && requested.paddr == paddr
                && requested.width == width,
        ));
    }

    // Every rejection above happened at the constructors; the spy stayed
    // untouched.
    assert!(backend.calls.is_empty());
}

fn word_context_at(paddr: u64, snapshot_bytes: &[u8]) -> AtomicReservationContext {
    AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr,
            width: AccessWidth::Word,
        },
        snapshot: snapshot(snapshot_bytes),
    }
}

// ---------------------------------------------------------------------------
// 3. Every AMO operation applies through the one Hart-owned arithmetic module.
// ---------------------------------------------------------------------------

/// Independent reference model used only by this test as an oracle.
fn reference_amo(operation: AmoOperation, width: AmoWidth, old: u64, operand: u64) -> u64 {
    match width {
        AmoWidth::Word => {
            let old = old as u32;
            let operand = operand as u32;
            let result: u32 = match operation {
                AmoOperation::Swap => operand,
                AmoOperation::Add => old.wrapping_add(operand),
                AmoOperation::BitAnd => old & operand,
                AmoOperation::BitOr => old | operand,
                AmoOperation::BitXor => old ^ operand,
                AmoOperation::Min => (old as i32).min(operand as i32) as u32,
                AmoOperation::Minu => old.min(operand),
                AmoOperation::Max => (old as i32).max(operand as i32) as u32,
                AmoOperation::Maxu => old.max(operand),
            };
            result as u64
        }
        AmoWidth::Doubleword => match operation {
            AmoOperation::Swap => operand,
            AmoOperation::Add => old.wrapping_add(operand),
            AmoOperation::BitAnd => old & operand,
            AmoOperation::BitOr => old | operand,
            AmoOperation::BitXor => old ^ operand,
            AmoOperation::Min => (old as i64).min(operand as i64) as u64,
            AmoOperation::Minu => old.min(operand),
            AmoOperation::Max => (old as i64).max(operand as i64) as u64,
            AmoOperation::Maxu => old.max(operand),
        },
    }
}

fn span_bytes(value: u64, width: AmoWidth) -> Vec<u8> {
    value.to_le_bytes()[..width.bytes()].to_vec()
}

fn value_from_le(bytes: &[u8]) -> u64 {
    let mut padded = [0u8; 8];
    padded[..bytes.len()].copy_from_slice(bytes);
    u64::from_le_bytes(padded)
}

#[test]
fn hart_arithmetic_module_covers_every_amo_operation_at_both_widths() {
    let widths = [AmoWidth::Word, AmoWidth::Doubleword];
    // Patterns chosen to hit wrapping, sign boundaries, and the signed vs
    // unsigned min/max split.
    let patterns: [(u64, u64); 8] = [
        (0x0000_0000_0000_0000, 0x0000_0000_0000_0001),
        (0xffff_ffff_ffff_ffff, 0x0000_0000_0000_0001), // add wraps
        (0x8000_0000_0000_0000, 0x0000_0000_0000_0001), // i64::MIN vs 1
        (0x7fff_ffff_ffff_ffff, 0xffff_ffff_ffff_ffff), // i64::MAX vs -1
        (0x8000_0000_8000_0000, 0x7fff_ffff_7fff_ffff),
        (0xdead_beef_cafe_f00d, 0x1234_5678_9abc_def0),
        (0x0000_0000_ffff_ffff, 0x0000_0000_0000_0001), // W sign boundary
        (0xffff_ffff_0000_0000, 0x0000_0000_ffff_ffff),
    ];

    for width in widths {
        let bytes = width.bytes();
        for (old_value, operand_value) in patterns {
            let old_bytes = span_bytes(old_value, width);
            let operand_bytes = span_bytes(operand_value, width);
            for operation in hart_amods::ALL_OPERATIONS {
                // The direct entry point and the envelope-carried transform
                // must be the same one Hart-owned implementation.
                let direct = hart_amods::apply(operation, width, &old_bytes, &operand_bytes)
                    .expect("exact-width inputs");
                let carried =
                    hart_amods::transform(operation, width).apply(&old_bytes, &operand_bytes);
                assert_eq!(
                    direct, carried,
                    "{operation} at width {bytes}: transform() must route through the one core"
                );

                let expected = reference_amo(operation, width, old_value, operand_value);
                let actual = value_from_le(&direct[..bytes]);
                assert_eq!(
                    actual, expected,
                    "{operation} at width {bytes}: old={old_value:#x} operand={operand_value:#x}"
                );
                // Trailing bytes beyond the span are zero, never stale data.
                assert!(
                    direct[bytes..].iter().all(|byte| *byte == 0),
                    "{operation} at width {bytes}: trailing bytes must stay zero"
                );
            }
        }
    }

    // Direct-entry input-length failures are typed, not guessed.
    assert!(matches!(
        hart_amods::apply(AmoOperation::Add, AmoWidth::Word, &[1, 2, 3], &[1, 2, 3, 4]),
        Err(AmoArithmeticError::OldSpanLength {
            expected: 4,
            actual: 3,
        })
    ));
    assert!(matches!(
        hart_amods::apply(AmoOperation::Add, AmoWidth::Word, &[1, 2, 3, 4], &[1, 2]),
        Err(AmoArithmeticError::OperandLength {
            expected: 4,
            actual: 2,
        })
    ));
}

#[test]
fn rmw_envelope_applies_the_hart_transform_in_one_transaction() {
    for width in [AmoWidth::Word, AmoWidth::Doubleword] {
        let bytes = width.bytes();
        let old_value = 0x0102_0304_0506_0708;
        let operand_value = 0x21;
        let old_bytes = span_bytes(old_value, width);
        let operand_bytes = span_bytes(operand_value, width);

        for operation in hart_amods::ALL_OPERATIONS {
            let mut port = ValidatedAtomicAccess::new(CriticalSectionRmwBackend::new(&old_bytes));
            let request = AtomicRequest::rmw(
                0x5000,
                width.physical_width(),
                AtomicOrdering {
                    aq: true,
                    rl: false,
                },
                &operand_bytes,
                hart_amods::transform(operation, width),
            )
            .unwrap();

            let response = port.access_atomic(request).unwrap();
            // The envelope returns the exact old bytes and nothing else.
            assert_eq!(response.old_bytes(), Some(&old_bytes[..]));
            assert_eq!(response.conditional_status(), None);
            assert_eq!(response.snapshot(), None);
            assert_eq!(response.binding(), request.descriptor());

            let backend = port.into_backend();
            // Exactly one backend call and one critical-section write.
            assert_eq!(backend.calls.len(), 1);
            assert_eq!(backend.written.len(), 1);
            // The written bytes are the transform output computed inside the
            // single transaction from the request's operand and old span.
            let expected_new =
                hart_amods::apply(operation, width, &old_bytes, &operand_bytes).unwrap();
            assert_eq!(backend.written[0], expected_new);
            let expected_value = reference_amo(operation, width, old_value, operand_value);
            assert_eq!(
                value_from_le(&backend.written[0][..bytes]),
                expected_value,
                "{operation} at width {bytes}"
            );
            // The observed call carried the transform and operand but no
            // store payload: an RMW is not a store.
            let observed = &backend.calls[0];
            assert_eq!(observed.descriptor.kind, AtomicAccessKind::Rmw);
            assert_eq!(observed.operand.as_deref(), Some(&operand_bytes[..]));
            assert_eq!(observed.store_payload, None);
            assert_eq!(observed.reservation, None);
        }
    }
}

// ---------------------------------------------------------------------------
// 4. LR snapshot echo and conditional status.
// ---------------------------------------------------------------------------

#[test]
fn load_reserved_response_snapshot_echoes_into_a_store_conditional_context() {
    let old_bytes = [0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10];
    let lr = AtomicRequest::load_reserved(0x6000, AccessWidth::Doubleword, AQ_RL).unwrap();
    let lr_snapshot = snapshot(&[0xa5, 0x5a, 0x01]);

    let mut backend = AtomicSpyBackend::default();
    backend.push(Ok(AtomicResponse::load_reserved_for(
        &lr,
        &old_bytes,
        lr_snapshot,
    )));
    // The echoed snapshot rides back through the SC context unchanged.
    let sc_context = AtomicReservationContext {
        reserved: lr.span(),
        snapshot: lr_snapshot,
    };
    let sc = AtomicRequest::store_conditional(
        0x6000,
        AccessWidth::Doubleword,
        AQ_RL,
        &[0x11; 8],
        sc_context,
    )
    .unwrap();
    backend.push(Ok(AtomicResponse::store_conditional_for(
        &sc,
        ConditionalStatus::Success,
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    let lr_response = port.access_atomic(lr).unwrap();
    assert_eq!(lr_response.old_bytes(), Some(&old_bytes[..]));
    assert_eq!(lr_response.snapshot(), Some(lr_snapshot));
    assert_eq!(lr_response.conditional_status(), None);
    assert_eq!(
        port.access_atomic(sc).unwrap().conditional_status(),
        Some(ConditionalStatus::Success)
    );

    let backend = port.into_backend();
    // The SC call carried the exact reserved span and the exact LR-time
    // snapshot bytes.
    let observed_sc = &backend.calls[1];
    assert_eq!(
        observed_sc.reservation,
        Some((lr.span(), lr_snapshot.as_bytes().to_vec()))
    );
    assert_eq!(observed_sc.store_payload.as_deref(), Some(&[0x11; 8][..]));
    assert_eq!(observed_sc.transform_width, None);
}

#[test]
fn load_reserved_response_requires_nonempty_snapshot_and_exact_old_bytes() {
    let lr = AtomicRequest::load_reserved(0x6100, AccessWidth::Word, AQ_RL).unwrap();
    let mut backend = AtomicSpyBackend::default();
    // A conforming LR always covers at least one block, so an empty snapshot
    // is a malformed response, not a success.
    backend.push(Ok(AtomicResponse::new(
        lr.descriptor(),
        AtomicResponseCompletion::LoadReserved {
            old_bytes: PhysicalResponseBytes::from_slice(&[1, 2, 3, 4]),
            snapshot: CommittedWriteSnapshot::from_bytes(&[]).unwrap(),
        },
    )));
    // Short old bytes are a length failure.
    let short = AtomicRequest::load_reserved(0x6104, AccessWidth::Word, AQ_RL).unwrap();
    backend.push(Ok(AtomicResponse::load_reserved_for(
        &short,
        &[1, 2, 3],
        snapshot(&[1]),
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    assert!(matches!(
        port.access_atomic(lr),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::EmptySnapshot
        ))
    ));
    assert!(matches!(
        port.access_atomic(short),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseLengthMismatch {
                expected: 4,
                actual: 3,
            }
        ))
    ));
    assert_eq!(port.backend().calls.len(), 2);
}

#[test]
fn store_conditional_failure_is_a_successful_envelope_not_an_error() {
    let context = word_context_at(0x6200, &[0x42]);
    let sc = AtomicRequest::store_conditional(
        0x6200,
        AccessWidth::Word,
        AtomicOrdering::default(),
        &[0xde, 0xad, 0xbe, 0xef],
        context,
    )
    .unwrap();

    let mut backend = AtomicSpyBackend::default();
    backend.push(Ok(AtomicResponse::store_conditional_for(
        &sc,
        ConditionalStatus::Failure,
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    // A conditional failure is a complete envelope transaction: Ok with a
    // Failure status, no old bytes, and no retry.
    let response = port.access_atomic(sc).unwrap();
    assert_eq!(
        response.conditional_status(),
        Some(ConditionalStatus::Failure)
    );
    assert_eq!(response.old_bytes(), None);
    assert_eq!(response.snapshot(), None);
    assert_eq!(port.backend().calls.len(), 1);
}

// ---------------------------------------------------------------------------
// 5. Binding and completion validation.
// ---------------------------------------------------------------------------

#[test]
fn atomic_response_binding_and_completion_kind_are_validated() {
    let rmw = word_rmw(0x7000, &[1, 2, 3, 4], AmoOperation::Swap);
    let wrong_paddr =
        AtomicRequestDescriptor::new(0x7004, AccessWidth::Word, AtomicAccessKind::Rmw);
    let wrong_width =
        AtomicRequestDescriptor::new(0x7000, AccessWidth::Doubleword, AtomicAccessKind::Rmw);
    let wrong_kind =
        AtomicRequestDescriptor::new(0x7000, AccessWidth::Word, AtomicAccessKind::LoadReserved);

    let mut backend = AtomicSpyBackend::default();
    for binding in [wrong_paddr, wrong_width, wrong_kind] {
        let completion = AtomicResponse::rmw_for(&rmw, &[9, 9, 9, 9]).completion();
        backend.push(Ok(AtomicResponse::new(binding, completion)));
    }
    // Right binding, wrong completion kind.
    backend.push(Ok(AtomicResponse::new(
        rmw.descriptor(),
        AtomicResponseCompletion::StoreConditional(ConditionalStatus::Success),
    )));
    backend.push(Ok(AtomicResponse::new(
        rmw.descriptor(),
        AtomicResponseCompletion::LoadReserved {
            old_bytes: PhysicalResponseBytes::from_slice(&[1, 2, 3, 4]),
            snapshot: snapshot(&[1]),
        },
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    for _ in 0..3 {
        assert!(matches!(
            port.access_atomic(rmw),
            Err(AtomicAccessError::Protocol(
                AtomicProtocolError::ResponseBindingMismatch { expected, actual }
            )) if expected == rmw.descriptor() && actual != rmw.descriptor(),
        ));
    }
    assert!(matches!(
        port.access_atomic(rmw),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseCompletionMismatch {
                expected: AtomicAccessKind::Rmw,
                actual: AtomicAccessKind::StoreConditional,
            }
        ))
    ));
    assert!(matches!(
        port.access_atomic(rmw),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponseCompletionMismatch {
                expected: AtomicAccessKind::Rmw,
                actual: AtomicAccessKind::LoadReserved,
            }
        ))
    ));
    // Exactly one backend call per submitted envelope: five envelopes, five
    // calls, no retries.
    assert_eq!(port.backend().calls.len(), 5);
}

#[test]
fn padded_and_truncated_rmw_old_bytes_are_protocol_failures() {
    let rmw = word_rmw(0x7100, &[1, 2, 3, 4], AmoOperation::BitXor);
    let mut backend = AtomicSpyBackend::default();
    // Reported 4 bytes but only 3 supplied.
    backend.push(Ok(AtomicResponse::new(
        rmw.descriptor(),
        AtomicResponseCompletion::Rmw(PhysicalResponseBytes::with_reported_len(
            &[0xaa, 0xbb, 0xcc],
            4,
        )),
    )));
    // Reported 4 bytes but 5 supplied.
    backend.push(Ok(AtomicResponse::new(
        rmw.descriptor(),
        AtomicResponseCompletion::Rmw(PhysicalResponseBytes::with_reported_len(
            &[0xaa, 0xbb, 0xcc, 0xdd, 0xee],
            4,
        )),
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    assert!(matches!(
        port.access_atomic(rmw),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponsePayloadLengthMismatch {
                reported: 4,
                supplied: 3,
            }
        ))
    ));
    assert!(matches!(
        port.access_atomic(rmw),
        Err(AtomicAccessError::Protocol(
            AtomicProtocolError::ResponsePayloadLengthMismatch {
                reported: 4,
                supplied: 5,
            }
        ))
    ));
    assert_eq!(port.backend().calls.len(), 2);
}

// ---------------------------------------------------------------------------
// 6. Taxonomy: target rejection, host failure, backend protocol.
// ---------------------------------------------------------------------------

#[test]
fn atomic_taxonomy_preserves_target_host_and_backend_protocol_categories() {
    let rmw = word_rmw(0x7200, &[1, 2, 3, 4], AmoOperation::BitOr);

    let mut backend = AtomicSpyBackend::default();
    backend.push(Err(PhysicalBackendError::target(
        PhysicalTargetRejectionReason::UnsupportedWidth,
        "device accepts byte writes only",
    )));
    backend.push(Err(PhysicalBackendError::host("RAM lock poisoned")));
    backend.push(Err(PhysicalBackendError::protocol(
        "backend response state was contradictory",
    )));
    let mut port = ValidatedAtomicAccess::new(backend);

    let error = port.access_atomic(rmw).unwrap_err();
    let AtomicAccessError::TargetRejected(rejection) = error else {
        panic!("a valid-width target refusal must remain a target rejection");
    };
    assert_eq!(rejection.request, rmw.descriptor());
    assert_eq!(rejection.span, rmw.span());
    assert_eq!(
        rejection.reason,
        PhysicalTargetRejectionReason::UnsupportedWidth
    );
    assert_eq!(rejection.context, "device accepts byte writes only");

    let error = port.access_atomic(rmw).unwrap_err();
    let AtomicAccessError::BackendFailure(failure) = error else {
        panic!("host/backend failure must retain its simulator-side category");
    };
    assert_eq!(failure.request, rmw.descriptor());
    assert_eq!(failure.context, "RAM lock poisoned");

    let error = port.access_atomic(rmw).unwrap_err();
    assert!(matches!(
        error,
        AtomicAccessError::Protocol(AtomicProtocolError::BackendProtocol { context })
            if context == "backend response state was contradictory"
    ));
    assert_eq!(port.backend().calls.len(), 3);
}

#[test]
fn wrapping_envelope_span_is_target_rejected_before_backend_dispatch() {
    let request = AtomicRequest::load_reserved(u64::MAX, AccessWidth::Doubleword, AQ_RL).unwrap();
    assert_eq!(request.span().checked_end_inclusive(), None);
    let mut port = ValidatedAtomicAccess::new(AtomicSpyBackend::default());

    let error = port.access_atomic(request).unwrap_err();
    let AtomicAccessError::TargetRejected(rejection) = error else {
        panic!("a wrapping atomic span must be a target rejection");
    };
    assert_eq!(rejection.request.paddr, u64::MAX);
    assert_eq!(
        rejection.reason,
        PhysicalTargetRejectionReason::RangeOverflow
    );
    assert!(port.backend().calls.is_empty());
}

// ---------------------------------------------------------------------------
// 7. Unknown completion is terminal; one envelope is one backend call.
// ---------------------------------------------------------------------------

#[test]
fn atomic_unknown_completion_is_terminal_and_never_retried() {
    // The backend begins a device-visible effect and only then reports an
    // unknown completion: the run fails terminally with the uncertainty
    // retained, nothing is retried, and no restored claim exists.
    #[derive(Debug, Default)]
    struct EffectThenUnknown {
        effects: Vec<String>,
        calls: usize,
    }
    impl AtomicBackend for EffectThenUnknown {
        fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
            self.calls += 1;
            if let Some(payload) = request.store_payload() {
                self.effects.push(format!("wrote {} bytes", payload.len()));
            }
            Err(PhysicalBackendError::unknown(
                "device callback may have committed",
            ))
        }
    }

    let context = word_context_at(0x7300, &[0x77]);
    let sc = AtomicRequest::store_conditional(
        0x7300,
        AccessWidth::Word,
        AtomicOrdering::default(),
        &[0x5a, 0xa5, 0x5a, 0xa5],
        context,
    )
    .unwrap();

    let mut port = ValidatedAtomicAccess::new(EffectThenUnknown::default());
    let error = port.access_atomic(sc).unwrap_err();
    let AtomicAccessError::UnknownCompletion(unknown) = error else {
        panic!("unknown completion must remain a terminal simulator failure");
    };
    assert_eq!(unknown.request, sc.descriptor());
    assert_eq!(unknown.context, "device callback may have committed");

    // Terminal: exactly one backend call, the possible effect stays recorded
    // (never silently undone), and the boundary provides no retry path.
    let backend = port.into_backend();
    assert_eq!(backend.calls, 1);
    assert_eq!(backend.effects.len(), 1);
}

#[test]
fn planned_unknown_completion_consumes_exactly_one_backend_call() {
    let rmw = word_rmw(0x7400, &[1, 2, 3, 4], AmoOperation::Minu);
    let mut backend = AtomicSpyBackend::default();
    backend.push(Err(PhysicalBackendError::unknown(
        "device callback may have committed",
    )));
    // A second planned result would make an accidental retry observable; the
    // validated boundary must consume exactly one backend call.
    let mut port = ValidatedAtomicAccess::new(backend);

    let error = port.access_atomic(rmw).unwrap_err();
    let AtomicAccessError::UnknownCompletion(unknown) = error else {
        panic!("unknown completion must remain a terminal simulator failure");
    };
    assert_eq!(unknown.request, rmw.descriptor());
    let backend = port.into_backend();
    assert_eq!(backend.calls.len(), 1);
    assert!(backend.planned.is_empty());
}

#[test]
fn aq_and_rl_combinations_are_informational_and_excluded_from_binding() {
    let orderings = [
        AtomicOrdering {
            aq: false,
            rl: false,
        },
        AtomicOrdering {
            aq: true,
            rl: false,
        },
        AtomicOrdering {
            aq: false,
            rl: true,
        },
        AtomicOrdering { aq: true, rl: true },
    ];
    let mut backend = AtomicSpyBackend::default();
    let lr = AtomicRequest::load_reserved(0x7500, AccessWidth::Word, AQ_RL).unwrap();
    for _ in orderings {
        backend.push(Ok(AtomicResponse::load_reserved_for(
            &lr,
            &[0x0f, 0xf0, 0x33, 0xcc],
            snapshot(&[3]),
        )));
    }
    let mut port = ValidatedAtomicAccess::new(backend);

    for ordering in orderings {
        let request = AtomicRequest::load_reserved(0x7500, AccessWidth::Word, ordering).unwrap();
        assert_eq!(request.ordering(), ordering);
        // The response binding ignores the informational ordering bits.
        assert_eq!(request.descriptor(), lr.descriptor());
        port.access_atomic(request).unwrap();
    }

    let backend = port.into_backend();
    assert_eq!(backend.calls.len(), 4);
    for observed in &backend.calls {
        assert_eq!(
            observed.descriptor,
            AtomicRequestDescriptor::new(0x7500, AccessWidth::Word, AtomicAccessKind::LoadReserved)
        );
    }
    assert_eq!(
        backend.calls[0].ordering,
        AtomicOrdering {
            aq: false,
            rl: false
        }
    );
    assert_eq!(backend.calls[3].ordering, AQ_RL);
}

// ---------------------------------------------------------------------------
// 8. Prohibition self-check: ordinary read/write pairs cannot represent an
//    AMO or an SC.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedOrdinaryCall {
    descriptor: ruscv_sim::physical::PhysicalRequestDescriptor,
    payload: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
struct OrdinarySpyBackend {
    calls: Vec<ObservedOrdinaryCall>,
    planned: VecDeque<PhysicalBackendResult>,
}

impl OrdinarySpyBackend {
    fn push(&mut self, result: PhysicalBackendResult) {
        self.planned.push_back(result);
    }
}

impl PhysicalBackend for OrdinarySpyBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.push(ObservedOrdinaryCall {
            descriptor: request.descriptor(),
            payload: request.payload().map(<[u8]>::to_vec),
        });
        self.planned
            .pop_front()
            .expect("the test must plan every ordinary backend completion")
    }
}

#[test]
fn ordinary_read_write_pairs_cannot_represent_amo_or_sc() {
    // Compile-time self-check, enforced by exhaustiveness: the ordinary
    // completion vocabulary has no conditional status, no old-value result,
    // and no committed-write snapshot.  If anyone adds such a variant to the
    // ordinary family, this match stops compiling.
    let ordinary_shapes = |completion: PhysicalResponseCompletion| match completion {
        PhysicalResponseCompletion::Read(_) => "raw bytes only",
        PhysicalResponseCompletion::WriteAcknowledgement => "unconditional ack only",
    };
    // The atomic completion vocabulary has exactly the three indivisible
    // events; an exhaustive match proves no hidden ordinary fall-back exists.
    let atomic_shapes = |completion: AtomicResponseCompletion| match completion {
        AtomicResponseCompletion::Rmw(_) => "old bytes + one transformed write",
        AtomicResponseCompletion::LoadReserved { .. } => "old bytes + committed-write snapshot",
        AtomicResponseCompletion::StoreConditional(_) => "conditional status only",
    };

    // The ordinary write acknowledgement is unconditional: it always means
    // committed bytes and can never mean "no write happened", which only the
    // atomic conditional status can express.
    let ordinary_write =
        PhysicalRequest::data_write(0x8000, AccessWidth::Word, &[1, 2, 3, 4]).unwrap();
    let mut ordinary_backend = OrdinarySpyBackend::default();
    ordinary_backend.push(Ok(PhysicalResponse::write_ack_for(&ordinary_write)));
    let mut ordinary_port = ValidatedPhysicalAccess::new(ordinary_backend);
    let ack = ordinary_port.access(ordinary_write).unwrap();
    assert!(ack.is_write_acknowledgement());
    assert_eq!(ack.read_bytes(), None);
    assert_eq!(
        ordinary_shapes(ack.completion()),
        "unconditional ack only",
        "an ordinary store cannot fail conditionally and stay a success"
    );

    // An ordinary read can never carry the LR snapshot: a read completion is
    // raw bytes only.
    let ordinary_read = PhysicalRequest::data_read(0x8000, AccessWidth::Word).unwrap();
    ordinary_port
        .backend_mut()
        .push(Ok(PhysicalResponse::read_for(
            &ordinary_read,
            &[1, 2, 3, 4],
        )));
    let read = ordinary_port.access(ordinary_read).unwrap();
    assert_eq!(
        ordinary_shapes(read.completion()),
        "raw bytes only",
        "an ordinary read cannot carry a committed-write snapshot"
    );

    // Behavioral difference: an ordinary read→write pair is two separately
    // bound backend transactions whose write bytes are fixed by the host
    // before the read result exists.  One RMW envelope is one bound
    // transaction whose written bytes are computed inside it from the old
    // span; the request carries no write payload at all.
    let rmw = word_rmw(0x8000, &[0x21, 0x43, 0x65, 0x87], AmoOperation::Add);
    assert_eq!(rmw.store_payload(), None, "an RMW is not a store");
    assert!(rmw.transform().is_some());
    assert!(rmw.operand_bytes().is_some());
    let lr = AtomicRequest::load_reserved(0x8100, AccessWidth::Word, AQ_RL).unwrap();
    assert_eq!(lr.operand_bytes(), None);
    assert_eq!(lr.store_payload(), None);
    assert_eq!(lr.reservation(), None, "an LR carries no reservation");
    let sc = AtomicRequest::store_conditional(
        0x8200,
        AccessWidth::Word,
        AQ_RL,
        &[1, 2, 3, 4],
        word_context_at(0x8200, &[1]),
    )
    .unwrap();
    assert!(sc.transform().is_none(), "an SC carries no transform");
    assert_eq!(sc.operand_bytes(), None);
    assert!(sc.reservation().is_some());

    let mut atomic_backend = AtomicSpyBackend::default();
    let old = [0x10, 0x20, 0x30, 0x40];
    atomic_backend.push(Ok(AtomicResponse::rmw_for(&rmw, &old)));
    let mut atomic_port = ValidatedAtomicAccess::new(atomic_backend);
    let rmw_response = atomic_port.access_atomic(rmw).unwrap();
    assert_eq!(atomic_port.backend().calls.len(), 1);
    assert_eq!(rmw_response.binding(), rmw.descriptor());
    assert_eq!(
        atomic_shapes(rmw_response.completion()),
        "old bytes + one transformed write"
    );

    // The same address serviced by the ordinary family took two calls with
    // two bindings; the vocabulary offers no constructor that turns either
    // ordinary request or response into its atomic counterpart.
    let ordinary_backend = ordinary_port.into_backend();
    assert_eq!(ordinary_backend.calls.len(), 2);
    assert_ne!(
        ordinary_backend.calls[0].descriptor,
        ordinary_backend.calls[1].descriptor
    );
}

// ---------------------------------------------------------------------------
// 9. Committed-write snapshots are bounded, opaque, and comparable.
// ---------------------------------------------------------------------------

#[test]
fn committed_write_snapshots_are_bounded_opaque_and_copyable() {
    let bytes = [0u8; MAX_COMMITTED_WRITE_SNAPSHOT_BYTES];
    let max = CommittedWriteSnapshot::from_bytes(&bytes).unwrap();
    assert_eq!(max.len(), MAX_COMMITTED_WRITE_SNAPSHOT_BYTES);
    assert!(!max.is_empty());
    let copy = max;
    assert_eq!(copy, max);
    assert_eq!(copy.as_bytes(), &bytes[..]);

    match CommittedWriteSnapshot::from_bytes(&[0u8; MAX_COMMITTED_WRITE_SNAPSHOT_BYTES + 1]) {
        Err(AtomicProtocolError::SnapshotTooLong { requested, max }) => {
            assert_eq!(requested, MAX_COMMITTED_WRITE_SNAPSHOT_BYTES + 1);
            assert_eq!(max, MAX_COMMITTED_WRITE_SNAPSHOT_BYTES);
        }
        other => panic!("oversized snapshots must be rejected, got {other:?}"),
    }

    let a = snapshot(&[1, 2, 3]);
    let b = snapshot(&[1, 2, 4]);
    assert_ne!(a, b, "snapshots are value-comparable for the echo check");
    assert_eq!(a.as_bytes(), &[1, 2, 3]);

    // Empty snapshots are constructible (a backend may model degenerate
    // bookkeeping internally) but never valid inside an envelope.
    let empty = CommittedWriteSnapshot::from_bytes(&[]).unwrap();
    assert!(empty.is_empty());
    assert!(matches!(
        AtomicRequest::store_conditional(
            0x9000,
            AccessWidth::Word,
            AtomicOrdering::default(),
            &[1, 2, 3, 4],
            word_context_with_snapshot(0x9000, empty),
        ),
        Err(AtomicProtocolError::EmptySnapshot)
    ));
}

fn word_context_with_snapshot(
    paddr: u64,
    snapshot: CommittedWriteSnapshot,
) -> AtomicReservationContext {
    AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr,
            width: AccessWidth::Word,
        },
        snapshot,
    }
}
