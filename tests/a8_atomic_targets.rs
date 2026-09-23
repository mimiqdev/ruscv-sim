//! Executable A8 T2 native-atomic-target tests (dev-plan §8, task T2).
//!
//! These tests exercise the atomic envelope against the native targets only:
//! `NativeRamBackend` over `SimpleMemory` (the flat facade's domain) and
//! `NativeSystemBusBackend` over `SystemBus` (the native bus facade's domain,
//! carrying RAM, the byte-only UART window, and the HTIF endpoint under the
//! approved D-c policy).  They deliberately do not connect the port to
//! `RiscvCore` or the Hart dispatch; reservation records, `rd` results, and
//! trap mapping are T3 work.  Storage-level committed-write bookkeeping is
//! asserted here through the envelope itself: an LR captures the domain's
//! snapshot and an SC re-checks it inside its one critical section, so SC
//! success/failure is the observable proof that each writer path bumped
//! exactly its committed bytes.
//!
//! Contract rows proven here (dev-plan §5.2 M1, §5.4, §5.5, §5.6, §5.7):
//!
//! 1. RAM executes one locked critical section per envelope: RMW returns the
//!    exact old bytes and commits the Hart transform's result exactly once;
//!    LR returns the old bytes plus a committed-write snapshot; SC commits
//!    one conditional write or nothing.
//! 2. Span validation precedes mutation: end-of-RAM success, one-past-end
//!    and cross-window spans reject, overflowing spans are target rejections,
//!    and a rejected envelope leaves RAM, UART registers/FIFOs, the HTIF
//!    callback, and the exit path untouched.
//! 3. The UART rejects every atomic request on width; the HTIF endpoint
//!    rejects LR/SC on category and wrong-width/wrong-base AMO without a
//!    callback, while one dword RMW at the exact endpoint fires the callback
//!    exactly once (approved D-c).
//! 4. A recording spy proves one locked target-visible transaction per
//!    envelope, and a competing reader never observes the internal read and
//!    write of an RMW as separate values.
//! 5. Arithmetic parity: two different conforming backends driven through
//!    the same Hart-owned transform produce byte-identical results for every
//!    operation and operand pattern; a recorded source audit shows no backend
//!    or target module references ISA operation semantics.
//! 6. Lock poison is a host/backend failure, never a fabricated target
//!    response; an unknown completion stays terminal with its possible
//!    effect retained and no retry.
//! 7. Committed-write bookkeeping is P2a-precise: typed write methods, raw
//!    `write_bytes`, and `load_program` bump after commit; rejected writes
//!    and failed-write suffixes bump nothing; a failed host write's
//!    committed prefix bumps exactly the committed bytes; an unrelated
//!    committed write outside the reserved span never fails an SC.

use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_HTIF_BASE, SYSTEM_BUS_UART_BASE};
use ruscv_sim::hart_amods::{self, AmoOperation};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::{uart16550, Uart16550, FIFO_DEPTH};
use ruscv_sim::physical::{
    AccessWidth, AmoWidth, AtomicAccess, AtomicAccessError, AtomicAccessKind, AtomicBackend,
    AtomicBackendResult, AtomicOrdering, AtomicProtocolError, AtomicRequest,
    AtomicRequestDescriptor, AtomicReservationContext, CommittedWriteSnapshot, ConditionalStatus,
    NativeRamBackend, NativeSystemBusBackend, PhysicalBackendError, PhysicalRequest, PhysicalSpan,
    PhysicalTargetRejectionReason, PhysicalWidth, ValidatedAtomicAccess, ValidatedPhysicalAccess,
};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

const ORDERING: AtomicOrdering = AtomicOrdering { aq: true, rl: true };

type SharedRam = Arc<Mutex<SimpleMemory>>;
type SharedUart = Arc<Mutex<Uart16550>>;
type SharedBus = Arc<Mutex<SystemBus>>;

type RamAtomicPort = ValidatedAtomicAccess<NativeRamBackend>;
type BusAtomicPort = ValidatedAtomicAccess<NativeSystemBusBackend>;

fn ram_atomic_port(base: u64, size: usize) -> (SharedRam, RamAtomicPort) {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    let backend = NativeRamBackend::new(memory.clone(), base, size);
    (memory, ValidatedAtomicAccess::new(backend))
}

fn bus_parts(ram_base: u64, ram_size: usize) -> (SharedRam, SharedUart, SharedBus) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart.clone(),
        ram_base,
        ram_size,
    )));
    (ram, uart, bus)
}

fn bus_atomic_port(
    ram_base: u64,
    ram_size: usize,
) -> (SharedRam, SharedUart, SharedBus, BusAtomicPort) {
    let (ram, uart, bus) = bus_parts(ram_base, ram_size);
    let port = ValidatedAtomicAccess::new(NativeSystemBusBackend::new(bus.clone()));
    (ram, uart, bus, port)
}

fn target_reason(error: AtomicAccessError) -> PhysicalTargetRejectionReason {
    match error {
        AtomicAccessError::TargetRejected(rejection) => rejection.reason,
        other => panic!("expected a target rejection, got {other:?}"),
    }
}

fn assert_atomic_target(
    port: &mut impl AtomicAccess,
    request: AtomicRequest<'_>,
    reason: PhysicalTargetRejectionReason,
) {
    assert_eq!(
        target_reason(port.access_atomic(request).unwrap_err()),
        reason
    );
}

/// Runs one LR through the port and returns its old bytes plus the
/// reservation context a Hart would carry into a later SC.
fn lr(
    port: &mut impl AtomicAccess,
    paddr: u64,
    width: PhysicalWidth,
) -> (Vec<u8>, AtomicReservationContext) {
    let request = AtomicRequest::load_reserved(paddr, width, ORDERING).unwrap();
    let response = port.access_atomic(request).unwrap();
    let old_bytes = response.old_bytes().unwrap().to_vec();
    let snapshot = response
        .snapshot()
        .expect("a conforming LR carries a committed-write snapshot");
    let context = AtomicReservationContext {
        reserved: request.span(),
        snapshot,
    };
    (old_bytes, context)
}

/// Runs one SC through the port and returns its conditional status.
fn sc(
    port: &mut impl AtomicAccess,
    paddr: u64,
    width: PhysicalWidth,
    payload: &[u8],
    context: AtomicReservationContext,
) -> ConditionalStatus {
    let request =
        AtomicRequest::store_conditional(paddr, width, ORDERING, payload, context).unwrap();
    port.access_atomic(request)
        .unwrap()
        .conditional_status()
        .expect("an SC completion carries its conditional status")
}

/// Runs one RMW envelope and returns the exact old span bytes.
fn rmw(
    port: &mut impl AtomicAccess,
    paddr: u64,
    width: AmoWidth,
    operation: AmoOperation,
    operand: &[u8],
) -> Vec<u8> {
    let request = AtomicRequest::rmw(
        paddr,
        width.physical_width(),
        ORDERING,
        operand,
        hart_amods::transform(operation, width),
    )
    .unwrap();
    port.access_atomic(request)
        .unwrap()
        .old_bytes()
        .unwrap()
        .to_vec()
}

fn snapshot(bytes: &[u8]) -> CommittedWriteSnapshot {
    CommittedWriteSnapshot::from_bytes(bytes).expect("snapshot fits the inline representation")
}

// ---------------------------------------------------------------------------
// 1. RAM critical sections: exact old bytes, exactly-once effect, all kinds.
// ---------------------------------------------------------------------------

#[test]
fn ram_rmw_returns_exact_old_bytes_and_commits_the_transform_once() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);
    {
        let mut ram = memory.lock().unwrap();
        ram.write_dword(0x40, 0x0102_0304_0506_0708).unwrap();
    }

    // AMOADD.D: old comes back byte-exact, the transformed result commits.
    let operand = 0x0000_0000_0000_00ffu64.to_le_bytes();
    let old = rmw(
        &mut port,
        0x8040,
        AmoWidth::Doubleword,
        AmoOperation::Add,
        &operand,
    );
    assert_eq!(old, 0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(
        memory.lock().unwrap().read_dword(0x40).unwrap(),
        0x0102_0304_0506_0708 + 0xff
    );

    // AMOSWAP.W on the upper half of the dword: the exact old word comes
    // back and only those four bytes commit.
    let operand = 0xdead_beefu32.to_le_bytes();
    let old = rmw(
        &mut port,
        0x8044,
        AmoWidth::Word,
        AmoOperation::Swap,
        &operand,
    );
    assert_eq!(old, 0x0102_0304u32.to_le_bytes());
    let dword = memory.lock().unwrap().read_dword(0x40).unwrap();
    assert_eq!(dword >> 32, 0x0000_0000_dead_beef);

    // The ordinary raw view sees the committed bytes: one storage domain.
    let mut ordinary =
        ValidatedPhysicalAccess::new(NativeRamBackend::new(memory.clone(), 0x8000, 0x100));
    let read = ordinary
        .access(PhysicalRequest::data_read(0x8044, AccessWidth::Word).unwrap())
        .unwrap();
    assert_eq!(read.read_bytes(), Some(&0xdead_beefu32.to_le_bytes()[..]));
}

#[test]
fn ram_load_reserved_and_store_conditional_roundtrip() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);
    memory
        .lock()
        .unwrap()
        .write_dword(0x10, 0x1122_3344_5566_7788)
        .unwrap();

    let (old, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert_eq!(old, 0x1122_3344_5566_7788u64.to_le_bytes());
    // The snapshot is opaque but non-empty and sized by the reserved span.
    assert_eq!(
        context.snapshot.as_bytes().len(),
        8 * context.reserved.width.bytes()
    );

    let payload = 0xa5a5_a5a5_a5a5_a5a5u64.to_le_bytes();
    assert_eq!(
        sc(
            &mut port,
            0x8010,
            AccessWidth::Doubleword,
            &payload,
            context
        ),
        ConditionalStatus::Success
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(0x10).unwrap(),
        u64::from_le_bytes(payload)
    );
}

#[test]
fn ram_sc_without_or_outside_reservation_fails_without_any_write_or_bookkeeping() {
    const P: u64 = 0x9010;
    const Q: u64 = 0x9020;
    let (memory, mut port) = ram_atomic_port(0x9000, 0x80);
    memory
        .lock()
        .unwrap()
        .write_dword(P - 0x9000, 0x1122)
        .unwrap();
    memory
        .lock()
        .unwrap()
        .write_dword(Q - 0x9000, 0x3344)
        .unwrap();

    let (_, context) = lr(&mut port, P, AccessWidth::Doubleword);
    let no_context_payload = 0xa5a5_a5a5_a5a5_a5a5u64.to_le_bytes();
    let no_context = AtomicRequest::store_conditional(
        P,
        AccessWidth::Doubleword,
        ORDERING,
        &no_context_payload,
        None,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(no_context).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure)
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P - 0x9000).unwrap(),
        0x1122
    );

    let uncovered_payload = 0xdead_beefu64.to_le_bytes();
    let uncovered = AtomicRequest::store_conditional(
        Q,
        AccessWidth::Doubleword,
        ORDERING,
        &uncovered_payload,
        context,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(uncovered).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure)
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(Q - 0x9000).unwrap(),
        0x3344
    );

    // Neither conditional failure committed bytes or bumped bookkeeping: the
    // original covered context remains eligible to commit on the valid span.
    let covered_payload = 0x5566u64.to_le_bytes();
    let covered = AtomicRequest::store_conditional(
        P,
        AccessWidth::Doubleword,
        ORDERING,
        &covered_payload,
        context,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(covered).unwrap().conditional_status(),
        Some(ConditionalStatus::Success)
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(P - 0x9000).unwrap(),
        0x5566
    );
}

#[test]
fn system_bus_ram_sc_checks_valid_target_before_absent_or_uncovered_failure() {
    const P: u64 = 0x9010;
    const Q: u64 = 0x9020;
    let (ram, _uart, bus, mut port) = bus_atomic_port(0x9000, 0x80);
    ram.lock().unwrap().write_dword(P - 0x9000, 0x1122).unwrap();
    ram.lock().unwrap().write_dword(Q - 0x9000, 0x3344).unwrap();
    let (_, context) = lr(&mut port, P, AccessWidth::Doubleword);

    let no_context_payload = 0xa5a5_a5a5_a5a5_a5a5u64.to_le_bytes();
    let no_context = AtomicRequest::store_conditional(
        P,
        AccessWidth::Doubleword,
        ORDERING,
        &no_context_payload,
        None,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(no_context).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure)
    );

    let uncovered_payload = 0xdead_beefu64.to_le_bytes();
    let uncovered = AtomicRequest::store_conditional(
        Q,
        AccessWidth::Doubleword,
        ORDERING,
        &uncovered_payload,
        context,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(uncovered).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure)
    );
    assert_eq!(ram.lock().unwrap().read_dword(P - 0x9000).unwrap(), 0x1122);
    assert_eq!(ram.lock().unwrap().read_dword(Q - 0x9000).unwrap(), 0x3344);

    let covered_payload = 0x5566u64.to_le_bytes();
    let covered = AtomicRequest::store_conditional(
        P,
        AccessWidth::Doubleword,
        ORDERING,
        &covered_payload,
        context,
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(covered).unwrap().conditional_status(),
        Some(ConditionalStatus::Success),
        "conditional failures did not mutate bookkeeping"
    );
    assert_eq!(ram.lock().unwrap().read_dword(P - 0x9000).unwrap(), 0x5566);
    assert_eq!(bus.lock().unwrap().htif_committed_write_version(), 0);
}

#[test]
fn ram_sc_span_inside_wider_reservation_commits_only_its_own_bytes() {
    // The C30 success direction at target level: an SC.W contained in an
    // LR.D's reserved span commits its four bytes only.
    let (memory, mut port) = ram_atomic_port(0x9000, 0x40);
    memory
        .lock()
        .unwrap()
        .write_dword(0x08, 0x1111_2222_3333_4444)
        .unwrap();

    let (old, context) = lr(&mut port, 0x9008, AccessWidth::Doubleword);
    assert_eq!(old.len(), 8);
    assert_eq!(
        sc(
            &mut port,
            0x900c,
            AccessWidth::Word,
            &0xdead_beefu32.to_le_bytes(),
            context
        ),
        ConditionalStatus::Success
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(0x08).unwrap(),
        0xdead_beef_3333_4444
    );
}

// ---------------------------------------------------------------------------
// 2. Span validation: boundary success, negative spans, overflow — all with
//    no partial write.
// ---------------------------------------------------------------------------

#[test]
fn ram_atomic_spans_succeed_at_the_last_valid_offset_and_reject_beyond() {
    let (memory, mut port) = ram_atomic_port(0x1000, 0x20);
    memory.lock().unwrap().write_dword(0x18, 0xee).unwrap();

    // Last valid dword span succeeds.
    let old = rmw(
        &mut port,
        0x1018,
        AmoWidth::Doubleword,
        AmoOperation::Swap,
        &0x55u64.to_le_bytes(),
    );
    assert_eq!(old, 0xeeu64.to_le_bytes());
    // LR/SC succeed at the same boundary.
    let (_, context) = lr(&mut port, 0x1018, AccessWidth::Doubleword);
    assert_eq!(
        sc(&mut port, 0x1018, AccessWidth::Doubleword, &[7; 8], context),
        ConditionalStatus::Success
    );

    // One-past-the-end and cross-boundary spans reject before mutation.
    for paddr in [0x1019, 0x1020] {
        assert_atomic_target(
            &mut port,
            AtomicRequest::rmw(
                paddr,
                AccessWidth::Doubleword,
                ORDERING,
                &[1; 8],
                hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
            )
            .unwrap(),
            PhysicalTargetRejectionReason::Unmapped,
        );
        assert_atomic_target(
            &mut port,
            AtomicRequest::load_reserved(paddr, AccessWidth::Doubleword, ORDERING).unwrap(),
            PhysicalTargetRejectionReason::Unmapped,
        );
    }
    // A word span crossing the end rejects too.
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(0x101e, AccessWidth::Word, ORDERING).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    // Below the base is unmapped.
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(0x0ff8, AccessWidth::Doubleword, ORDERING).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );

    // Nothing mutated: the boundary bytes still hold the SC result.
    assert_eq!(
        memory.lock().unwrap().read_dword(0x18).unwrap(),
        u64::from_le_bytes([7; 8])
    );
}

#[test]
fn wrapping_atomic_spans_are_target_rejections_before_any_backend_mutation() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x40);
    memory.lock().unwrap().write_dword(0, u64::MAX).unwrap();

    for kind_request in [
        AtomicRequest::rmw(
            u64::MAX,
            AccessWidth::Doubleword,
            ORDERING,
            &[1; 8],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
        )
        .unwrap(),
        AtomicRequest::load_reserved(u64::MAX, AccessWidth::Doubleword, ORDERING).unwrap(),
        AtomicRequest::store_conditional(
            u64::MAX,
            AccessWidth::Doubleword,
            ORDERING,
            &[1; 8],
            AtomicReservationContext {
                reserved: PhysicalSpan {
                    paddr: 0x8000,
                    width: AccessWidth::Doubleword,
                },
                snapshot: snapshot(&[9; 64]),
            },
        )
        .unwrap(),
    ] {
        let error = port.access_atomic(kind_request).unwrap_err();
        let AtomicAccessError::TargetRejected(rejection) = error else {
            panic!("a wrapping atomic span must be a target rejection");
        };
        assert_eq!(
            rejection.reason,
            PhysicalTargetRejectionReason::RangeOverflow
        );
        assert_eq!(rejection.request.paddr, u64::MAX);
        assert_eq!(rejection.span.checked_end_inclusive(), None);
    }
    assert_eq!(memory.lock().unwrap().read_dword(0).unwrap(), u64::MAX);
}

#[test]
fn rejected_atomic_requests_leave_ram_uart_htif_and_the_exit_latch_unchanged() {
    let (ram, uart, bus, mut port) = bus_atomic_port(0x8000, 0x80);
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::new(Mutex::new(Vec::new()));
    {
        let callback_copy = callbacks.clone();
        bus.lock()
            .unwrap()
            .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
        let output_copy = output.clone();
        uart.lock()
            .unwrap()
            .set_output_callback(move |byte| output_copy.lock().unwrap().push(byte));
        ram.lock()
            .unwrap()
            .write_dword(0x20, 0xfeed_cafe_0000_0001)
            .unwrap();
        let mut guard = uart.lock().unwrap();
        guard.write_reg(uart16550::reg_offset::MCR, uart16550::mcr_bits::OUT2);
        for byte in 0..4 {
            guard.receive_byte(byte as u8);
        }
    }
    let ram_before = ram.lock().unwrap().read_bytes(0, 0x80).unwrap();
    let rx_before = uart.lock().unwrap().rx_fifo_data().to_vec();
    let iir_before = uart.lock().unwrap().interrupt_id();
    let version_before = bus.lock().unwrap().htif_committed_write_version();

    // RAM-side rejections: out-of-domain SC must reject before its absent
    // reservation could become conditional failure.
    assert_atomic_target(
        &mut port,
        AtomicRequest::store_conditional(
            0x9000,
            AccessWidth::Doubleword,
            ORDERING,
            &[0xaa; 8],
            None,
        )
        .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(0x9000, AccessWidth::Doubleword, ORDERING).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    // UART rejection on width.
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(SYSTEM_BUS_UART_BASE, AccessWidth::Doubleword, ORDERING)
            .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::rmw(
            SYSTEM_BUS_UART_BASE,
            AccessWidth::Word,
            ORDERING,
            &[0xaa; 4],
            hart_amods::transform(AmoOperation::Swap, AmoWidth::Word),
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    // HTIF rejections: no-context SC and LR on category, wrong-width AMO on
    // width. Reservation absence cannot bypass D-c capability validation.
    assert_atomic_target(
        &mut port,
        AtomicRequest::store_conditional(
            SYSTEM_BUS_HTIF_BASE,
            AccessWidth::Doubleword,
            ORDERING,
            &[0xbb; 8],
            None,
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword, ORDERING)
            .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::rmw(
            SYSTEM_BUS_HTIF_BASE,
            AccessWidth::Word,
            ORDERING,
            &[1, 2, 3, 4],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Word),
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    // A completely unmapped target rejects no-context SC too.
    assert_atomic_target(
        &mut port,
        AtomicRequest::store_conditional(0x6000, AccessWidth::Word, ORDERING, &[0xcc; 4], None)
            .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(0x6000, AccessWidth::Word, ORDERING).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );

    // No RAM byte, register, FIFO entry, callback, or bookkeeping changed.
    assert_eq!(ram.lock().unwrap().read_bytes(0, 0x80).unwrap(), ram_before);
    let guard = uart.lock().unwrap();
    assert_eq!(guard.rx_fifo_data(), rx_before.as_slice());
    assert_eq!(guard.interrupt_id(), iir_before);
    assert!(guard.tx_fifo_data().is_empty());
    drop(guard);
    assert!(output.lock().unwrap().is_empty());
    assert!(callbacks.lock().unwrap().is_empty());
    assert_eq!(
        bus.lock().unwrap().htif_committed_write_version(),
        version_before,
        "rejected envelopes never bump the endpoint's bookkeeping"
    );
}

#[test]
fn native_bus_never_stitches_an_atomic_span_across_targets() {
    // RAM ends two bytes below the UART window; a word AMO straddling the
    // boundary is unmapped, not stitched from RAM and UART bytes.
    let (_ram, uart, _bus, mut port) = bus_atomic_port(SYSTEM_BUS_UART_BASE - 2, 2);
    let rx_before = uart.lock().unwrap().rx_fifo_data().to_vec();
    assert_atomic_target(
        &mut port,
        AtomicRequest::rmw(
            SYSTEM_BUS_UART_BASE - 2,
            AccessWidth::Word,
            ORDERING,
            &[1, 2, 3, 4],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Word),
        )
        .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_eq!(uart.lock().unwrap().rx_fifo_data(), rx_before.as_slice());
}

// ---------------------------------------------------------------------------
// 3. UART width rejection and the approved D-c HTIF policy.
// ---------------------------------------------------------------------------

#[test]
fn uart_rejects_every_atomic_kind_on_width_before_any_effect() {
    let (_ram, uart, _bus, mut port) = bus_atomic_port(0x8000, 0x40);
    {
        let mut guard = uart.lock().unwrap();
        guard.write_reg(uart16550::reg_offset::IER, uart16550::ier_bits::ERBFI);
        for byte in 0..FIFO_DEPTH {
            guard.receive_byte(byte as u8);
        }
    }
    let rx_before = uart.lock().unwrap().rx_fifo_data().to_vec();
    let iir_before = uart.lock().unwrap().interrupt_id();

    let fake_context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: SYSTEM_BUS_UART_BASE,
            width: AccessWidth::Doubleword,
        },
        snapshot: snapshot(&[0xaa; 64]),
    };
    let requests = [
        AtomicRequest::load_reserved(SYSTEM_BUS_UART_BASE, AccessWidth::Doubleword, ORDERING)
            .unwrap(),
        AtomicRequest::load_reserved(SYSTEM_BUS_UART_BASE + 0x80, AccessWidth::Word, ORDERING)
            .unwrap(),
        AtomicRequest::store_conditional(
            SYSTEM_BUS_UART_BASE,
            AccessWidth::Doubleword,
            ORDERING,
            &[0x11; 8],
            fake_context,
        )
        .unwrap(),
        AtomicRequest::store_conditional(
            SYSTEM_BUS_UART_BASE,
            AccessWidth::Word,
            ORDERING,
            &[0x44; 4],
            None,
        )
        .unwrap(),
        AtomicRequest::rmw(
            SYSTEM_BUS_UART_BASE,
            AccessWidth::Doubleword,
            ORDERING,
            &[0x22; 8],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
        )
        .unwrap(),
        AtomicRequest::rmw(
            SYSTEM_BUS_UART_BASE + 0x40,
            AccessWidth::Word,
            ORDERING,
            &[0x33; 4],
            hart_amods::transform(AmoOperation::Swap, AmoWidth::Word),
        )
        .unwrap(),
    ];
    for request in requests {
        assert_atomic_target(
            &mut port,
            request,
            PhysicalTargetRejectionReason::UnsupportedWidth,
        );
    }

    // No register, FIFO, or callback-visible effect: the RX FIFO is still
    // full, the IER/MCR writes persist, and no TX byte was produced.
    let guard = uart.lock().unwrap();
    assert_eq!(guard.rx_fifo_data(), rx_before.as_slice());
    assert_eq!(guard.rx_fifo_data().len(), FIFO_DEPTH);
    assert_eq!(guard.interrupt_id(), iir_before);
    assert!(guard.tx_fifo_data().is_empty());
}

#[test]
fn htif_d_c_rejects_lr_and_sc_without_callback_and_allows_one_amo_envelope() {
    let (_ram, _uart, bus, mut port) = bus_atomic_port(0x8000, 0x40);
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let callback_copy = callbacks.clone();
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));

    // LR.D and LR.W at the endpoint reject on category/width before any
    // callback or mutation.
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword, ORDERING)
            .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::load_reserved(SYSTEM_BUS_HTIF_BASE, AccessWidth::Word, ORDERING).unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    // SC.D at the endpoint rejects on category even with a well-formed
    // reservation context; no callback fires.
    let sc_context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: SYSTEM_BUS_HTIF_BASE,
            width: AccessWidth::Doubleword,
        },
        snapshot: snapshot(&[0x5a; 64]),
    };
    assert_atomic_target(
        &mut port,
        AtomicRequest::store_conditional(
            SYSTEM_BUS_HTIF_BASE,
            AccessWidth::Doubleword,
            ORDERING,
            &0x1122_3344_5566_7788u64.to_le_bytes(),
            sc_context,
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::store_conditional(
            SYSTEM_BUS_HTIF_BASE,
            AccessWidth::Doubleword,
            ORDERING,
            &0x8877_6655_4433_2211u64.to_le_bytes(),
            None,
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    // AMO at the wrong width or off the endpoint base rejects too.
    assert_atomic_target(
        &mut port,
        AtomicRequest::rmw(
            SYSTEM_BUS_HTIF_BASE,
            AccessWidth::Word,
            ORDERING,
            &[9; 4],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Word),
        )
        .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    assert_atomic_target(
        &mut port,
        AtomicRequest::rmw(
            SYSTEM_BUS_HTIF_BASE + 8,
            AccessWidth::Doubleword,
            ORDERING,
            &[9; 8],
            hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
        )
        .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert!(callbacks.lock().unwrap().is_empty());
    assert_eq!(bus.lock().unwrap().htif_committed_write_version(), 0);

    // One AMO.D at the exact endpoint: old value is the zero read and the
    // callback fires exactly once with the transformed value.
    let operand = 0x0000_0000_0000_00ffu64.to_le_bytes();
    let old = rmw(
        &mut port,
        SYSTEM_BUS_HTIF_BASE,
        AmoWidth::Doubleword,
        AmoOperation::Add,
        &operand,
    );
    assert_eq!(old, [0u8; 8], "the endpoint's old value is its zero read");
    assert_eq!(*callbacks.lock().unwrap(), vec![0xff]);
    assert_eq!(
        bus.lock().unwrap().htif_committed_write_version(),
        1,
        "the AMO's committed write bumps the endpoint bookkeeping once"
    );

    // A second AMO fires the callback exactly once more (exactly-once, not
    // zero-or-more): AMOOR with the exit-shape payload sets bits.
    let exit_shaped = 0x0000_0000_0000_0003u64.to_le_bytes();
    let old = rmw(
        &mut port,
        SYSTEM_BUS_HTIF_BASE,
        AmoWidth::Doubleword,
        AmoOperation::BitOr,
        &exit_shaped,
    );
    assert_eq!(old, [0u8; 8]);
    assert_eq!(*callbacks.lock().unwrap(), vec![0xff, 0x03]);
    assert_eq!(bus.lock().unwrap().htif_committed_write_version(), 2);
}

#[test]
fn htif_amo_without_a_registered_callback_still_commits_once() {
    let (_ram, _uart, bus, mut port) = bus_atomic_port(0x8000, 0x40);
    let old = rmw(
        &mut port,
        SYSTEM_BUS_HTIF_BASE,
        AmoWidth::Doubleword,
        AmoOperation::Swap,
        &0xdead_beefu64.to_le_bytes(),
    );
    assert_eq!(old, [0u8; 8]);
    assert_eq!(bus.lock().unwrap().htif_committed_write_version(), 1);
}

// ---------------------------------------------------------------------------
// 4. One locked target-visible transaction; internal read/write never
//    separately observable.
// ---------------------------------------------------------------------------

/// A spy that wraps a real atomic backend and records every envelope it sees.
/// The wrapper is the only observation point at this seam, so one recorded
/// call is one target-visible transaction.
struct RecordingAtomicSpy<B> {
    inner: B,
    calls: Vec<AtomicRequestDescriptor>,
}

impl<B> RecordingAtomicSpy<B> {
    fn new(inner: B) -> Self {
        Self {
            inner,
            calls: Vec::new(),
        }
    }
}

impl<B: AtomicBackend> AtomicBackend for RecordingAtomicSpy<B> {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.calls.push(request.descriptor());
        self.inner.transact_atomic(request)
    }
}

#[test]
fn spy_proves_one_target_visible_transaction_per_amo_and_sc() {
    let (memory, _uart, _bus) = bus_parts(0x8000, 0x80);
    let spy = RecordingAtomicSpy::new(NativeSystemBusBackend::new(_bus.clone()));
    let mut port = ValidatedAtomicAccess::new(spy);
    memory.lock().unwrap().write_dword(0x10, 0x77).unwrap();

    // One AMO envelope = exactly one backend transaction.
    let old = rmw(
        &mut port,
        0x8010,
        AmoWidth::Doubleword,
        AmoOperation::Add,
        &1u64.to_le_bytes(),
    );
    assert_eq!(old, 0x77u64.to_le_bytes());
    assert_eq!(port.backend().calls.len(), 1);
    assert_eq!(port.backend().calls[0].kind, AtomicAccessKind::Rmw);

    // One LR envelope = one transaction carrying no write payload.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert_eq!(port.backend().calls.len(), 2);
    assert_eq!(port.backend().calls[1].kind, AtomicAccessKind::LoadReserved);

    // One SC envelope (success and failure) = one transaction each.
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[9; 8], context),
        ConditionalStatus::Success
    );
    assert_eq!(port.backend().calls.len(), 3);
    assert_eq!(
        port.backend().calls[2].kind,
        AtomicAccessKind::StoreConditional
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(0x10).unwrap(),
        u64::from_le_bytes([9; 8])
    );

    // A conditional-failure SC is still exactly one transaction.
    let (_, stale) = lr(&mut port, 0x8018, AccessWidth::Doubleword);
    memory.lock().unwrap().write_dword(0x18, 0x1).unwrap();
    assert_eq!(
        sc(&mut port, 0x8018, AccessWidth::Doubleword, &[2; 8], stale),
        ConditionalStatus::Failure
    );
    assert_eq!(port.backend().calls.len(), 5);
    // The failed SC committed no bytes.
    assert_eq!(memory.lock().unwrap().read_dword(0x18).unwrap(), 1);
}

#[test]
fn competing_reader_never_observes_the_rmw_internal_read_or_write() {
    const PATTERN_A: u64 = 0xaaaa_aaaa_aaaa_aaaa;
    const PATTERN_B: u64 = 0x5555_5555_5555_5555;
    const ITERATIONS: usize = 300;

    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    memory.lock().unwrap().write_dword(0x40, PATTERN_A).unwrap();

    let violations = Arc::new(Mutex::new(Vec::new()));
    let barrier = Arc::new(Barrier::new(2));
    let reader = {
        let memory = memory.clone();
        let violations = violations.clone();
        let barrier = barrier.clone();
        thread::spawn(move || {
            // A competing reader through the ordinary raw port shares the
            // same locking domain, so it can only observe committed states.
            let mut port = ValidatedPhysicalAccess::new(NativeRamBackend::new(memory, 0, 0x100));
            barrier.wait();
            for _ in 0..ITERATIONS * 4 {
                let response = port
                    .access(PhysicalRequest::data_read(0x40, AccessWidth::Doubleword).unwrap())
                    .unwrap();
                let bytes: [u8; 8] = response.read_bytes().unwrap().try_into().unwrap();
                let value = u64::from_le_bytes(bytes);
                if value != PATTERN_A && value != PATTERN_B {
                    violations.lock().unwrap().push(value);
                }
            }
        })
    };

    let mut writer = ValidatedAtomicAccess::new(NativeRamBackend::new(memory.clone(), 0, 0x100));
    barrier.wait();
    for i in 0..ITERATIONS {
        let expected_old = if i % 2 == 0 { PATTERN_A } else { PATTERN_B };
        let operand = if i % 2 == 0 {
            PATTERN_B.to_le_bytes()
        } else {
            PATTERN_A.to_le_bytes()
        };
        // AMOSWAP alternates the span between two full patterns.  The
        // envelope's own old-bytes result is also always one whole pattern:
        // the writer never sees its own section torn either.
        let old = rmw(
            &mut writer,
            0x40,
            AmoWidth::Doubleword,
            AmoOperation::Swap,
            &operand,
        );
        assert_eq!(
            old,
            expected_old.to_le_bytes(),
            "the RMW old value must be a complete committed state"
        );
    }
    reader.join().expect("reader thread panicked");
    assert!(
        violations.lock().unwrap().is_empty(),
        "a competing reader observed a mid-transaction value: {:?}",
        violations.lock().unwrap()
    );
    assert_eq!(memory.lock().unwrap().read_dword(0x40).unwrap(), PATTERN_A);
}

// ---------------------------------------------------------------------------
// 5. Arithmetic parity across conforming backends, and the recorded source
//    audit that no backend implements ISA semantics.
// ---------------------------------------------------------------------------

#[test]
fn hart_transform_results_are_byte_identical_across_conforming_backends() {
    // Two genuinely different conforming backends — the flat RAM backend and
    // the native system-bus backend — execute the same Hart-owned transform.
    // Both must produce byte-identical old bytes and committed results for
    // every operation and operand pattern.
    let patterns: [(u64, u64); 8] = [
        (0x0000_0000_0000_0000, 0x0000_0000_0000_0001),
        (0xffff_ffff_ffff_ffff, 0x0000_0000_0000_0001),
        (0x8000_0000_0000_0000, 0xffff_ffff_ffff_ffff),
        (0x7fff_ffff_ffff_ffff, 0x0000_0000_0000_0001),
        (0x8000_0000_8000_0000, 0x7fff_ffff_7fff_ffff),
        (0xdead_beef_cafe_f00d, 0x1234_5678_9abc_def0),
        (0x0000_0000_ffff_ffff, 0x0000_0000_0000_0001),
        (0xffff_ffff_0000_0000, 0xffff_ffff_ffff_ffff),
    ];

    for width in [AmoWidth::Word, AmoWidth::Doubleword] {
        let bytes = width.bytes();
        for (old_value, operand_value) in patterns {
            for operation in hart_amods::ALL_OPERATIONS {
                // Backend A: the flat native RAM target.
                let (memory_a, mut port_a) = ram_atomic_port(0x8000, 0x100);
                memory_a
                    .lock()
                    .unwrap()
                    .write_dword(0x40, old_value)
                    .unwrap();
                // Backend B: the native bus target, RAM at the same base.
                let (memory_b, _uart, _bus, mut port_b) = bus_atomic_port(0x8000, 0x100);
                memory_b
                    .lock()
                    .unwrap()
                    .write_dword(0x40, old_value)
                    .unwrap();

                let operand = operand_value.to_le_bytes()[..bytes].to_vec();
                let old_a = rmw(&mut port_a, 0x8040, width, operation, &operand);
                let old_b = rmw(&mut port_b, 0x8040, width, operation, &operand);
                assert_eq!(old_a, old_b, "{operation} old bytes diverged");
                assert_eq!(old_a, old_value.to_le_bytes()[..bytes]);

                let committed_a = memory_a.lock().unwrap().read_bytes(0x40, bytes).unwrap();
                let committed_b = memory_b.lock().unwrap().read_bytes(0x40, bytes).unwrap();
                assert_eq!(
                    committed_a, committed_b,
                    "{operation} at width {bytes} committed different bytes across backends"
                );

                // And both equal the one Hart-owned arithmetic result.
                let expected = hart_amods::apply(
                    operation,
                    width,
                    &old_value.to_le_bytes()[..bytes],
                    &operand,
                )
                .unwrap();
                assert_eq!(&committed_a[..], &expected[..bytes]);
            }
        }
    }
}

/// The recorded route audit (dev-plan §8 T2): no backend or target module may
/// reference ISA operation semantics.  Doc comments may name the Hart-owned
/// module for readers, so the scan strips comment lines and then requires
/// that no remaining code mentions the operation vocabulary, the decode
/// field, or the Hart dispatch entry point.  The audit failing means a
/// backend grew its own arithmetic — dev-plan §5.2 M2b is rejected design.
#[test]
fn no_backend_or_target_module_references_isa_operation_semantics() {
    const AUDITED_MODULES: &[&str] = &[
        "src/physical.rs",
        "src/memory/mod.rs",
        "src/executor.rs",
        "src/peripherals/uart16550.rs",
    ];
    const FORBIDDEN_TOKENS: &[&str] = &[
        "hart_amods",
        "AmoOperation",
        "amoswap",
        "amoadd",
        "amoxor",
        "amoand",
        "amoor",
        "amomin",
        "amomax",
        "funct5",
        "execute_amo",
        "Opcode::",
    ];
    for path in AUDITED_MODULES {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot audit {path}: {error}"));
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for token in FORBIDDEN_TOKENS {
            assert!(
                !code.contains(token),
                "{path} references ISA operation semantics token {token:?} in non-comment code"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Failure taxonomy at the targets: lock poison is a host failure; an
//    unknown completion stays terminal and unretried.
// ---------------------------------------------------------------------------

#[test]
fn poisoned_ram_and_bus_locks_are_host_failures_not_target_rejections() {
    // Poison the shared RAM mutex.
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    let poison = memory.clone();
    let _ = thread::spawn(move || {
        let _guard = poison.lock().unwrap();
        panic!("poison the shared RAM lock for the atomic failure test");
    })
    .join();
    let mut port = ValidatedAtomicAccess::new(NativeRamBackend::new(memory, 0, 0x40));
    let error = port
        .access_atomic(
            AtomicRequest::load_reserved(0x08, AccessWidth::Doubleword, ORDERING).unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            AtomicAccessError::BackendFailure(failure)
                if failure.request.paddr == 0x08 && failure.request.width == AccessWidth::Doubleword
        ),
        "lock poison is a host failure, never a fabricated trap or target response"
    );

    // Poison the shared bus mutex.
    let (_ram, _uart, bus) = bus_parts(0x8000, 0x40);
    let poison = bus.clone();
    let _ = thread::spawn(move || {
        let _guard = poison.lock().unwrap();
        panic!("poison the shared bus lock for the atomic failure test");
    })
    .join();
    let mut port = ValidatedAtomicAccess::new(NativeSystemBusBackend::new(bus));
    let error = port
        .access_atomic(
            AtomicRequest::load_reserved(0x8008, AccessWidth::Doubleword, ORDERING).unwrap(),
        )
        .unwrap_err();
    assert!(matches!(error, AtomicAccessError::BackendFailure(_)));
}

#[test]
fn unknown_completion_after_a_possible_effect_is_terminal_and_unretried() {
    // A backend that performs a device-visible effect and then reports an
    // unknown completion: the envelope fails terminally, the effect is not
    // silently undone, and the boundary never retries.
    struct EffectThenUnknown {
        inner: NativeRamBackend,
        calls: usize,
    }
    impl AtomicBackend for EffectThenUnknown {
        fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
            self.calls += 1;
            self.inner.transact_atomic(request)?; // commits the real effect
            Err(PhysicalBackendError::unknown("effect may have committed"))
        }
    }

    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    let backend = EffectThenUnknown {
        inner: NativeRamBackend::new(memory.clone(), 0x8000, 0x40),
        calls: 0,
    };
    let mut port = ValidatedAtomicAccess::new(backend);

    let error = port
        .access_atomic(
            AtomicRequest::rmw(
                0x8010,
                AccessWidth::Doubleword,
                ORDERING,
                &0x0fu64.to_le_bytes(),
                hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
            )
            .unwrap(),
        )
        .unwrap_err();
    assert!(matches!(error, AtomicAccessError::UnknownCompletion(_)));
    assert_eq!(
        port.backend().calls,
        1,
        "unknown completion is never retried"
    );
    // The possible physical effect stays exactly as committed: no fabricated
    // "restored" state exists.
    assert_eq!(memory.lock().unwrap().read_dword(0x10).unwrap(), 0x0f);
}

// ---------------------------------------------------------------------------
// 7. Committed-write bookkeeping (approved P2a-precise): every write path
//    bumps exactly its committed bytes; nothing else bumps.
// ---------------------------------------------------------------------------

#[test]
fn typed_write_methods_bump_only_their_committed_span() {
    // Each typed write method bumps bookkeeping for exactly its own span:
    // an overlapping committed write fails the SC, a non-overlapping one
    // never does.
    type TypedWrite = fn(&mut SimpleMemory, u64);
    let writers: [(&str, TypedWrite); 4] = [
        ("write_byte", |m, a| m.write_byte(a, 0xaa).unwrap()),
        ("write_half", |m, a| m.write_half(a, 0xbbcc).unwrap()),
        ("write_word", |m, a| m.write_word(a, 0xddee_ff00).unwrap()),
        ("write_dword", |m, a| {
            m.write_dword(a, 0x1122_3344_5566_7788).unwrap()
        }),
    ];
    for (name, write) in writers {
        let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

        // An overlapping committed write fails the SC: the overlap check
        // covers every byte of the reserved span exactly, not a coarse
        // block.  (The wider typed methods require aligned offsets, so the
        // overlap write lands at the aligned span start.)
        let (_, context) = lr(&mut port, 0x8048, AccessWidth::Doubleword);
        write(&mut memory.lock().unwrap(), 0x48);
        assert_eq!(
            sc(&mut port, 0x8048, AccessWidth::Doubleword, &[0; 8], context),
            ConditionalStatus::Failure,
            "{name}: an overlapping committed write must fail the SC"
        );

        // The adjacent aligned span outside the reservation never fails it.
        let (_, context) = lr(&mut port, 0x8048, AccessWidth::Doubleword);
        write(&mut memory.lock().unwrap(), 0x50);
        assert_eq!(
            sc(&mut port, 0x8048, AccessWidth::Doubleword, &[1; 8], context),
            ConditionalStatus::Success,
            "{name}: a non-overlapping committed write must not fail the SC"
        );
    }
}

#[test]
fn raw_write_bytes_and_ordinary_port_writes_bump_after_commit() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

    // Raw write_bytes overlapping the reserved span fails the SC.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    memory.lock().unwrap().write_bytes(0x10, &[0xcc]).unwrap();
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[0; 8], context),
        ConditionalStatus::Failure
    );

    // An ordinary raw DataWrite through a second port over the same domain
    // bumps identically (writer W1's storage-level path).
    let mut ordinary =
        ValidatedPhysicalAccess::new(NativeRamBackend::new(memory.clone(), 0x8000, 0x100));
    let (_, context) = lr(&mut port, 0x8020, AccessWidth::Word);
    ordinary
        .access(PhysicalRequest::data_write(0x8020, AccessWidth::Word, &[0xdd; 4]).unwrap())
        .unwrap();
    assert_eq!(
        sc(&mut port, 0x8020, AccessWidth::Word, &[0; 4], context),
        ConditionalStatus::Failure
    );

    // Non-overlap through the same ordinary port: SC succeeds.
    let (_, context) = lr(&mut port, 0x8030, AccessWidth::Doubleword);
    ordinary
        .access(PhysicalRequest::data_write(0x8060, AccessWidth::Doubleword, &[0xee; 8]).unwrap())
        .unwrap();
    assert_eq!(
        sc(&mut port, 0x8030, AccessWidth::Doubleword, &[3; 8], context),
        ConditionalStatus::Success,
        "an unrelated committed write outside the reserved span must not fail SC"
    );
}

#[test]
fn load_program_bumps_exactly_the_bytes_it_committed() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x1000);

    // A small image far from the reservation does not fail the SC.
    let (_, context) = lr(&mut port, 0x8800, AccessWidth::Doubleword);
    memory.lock().unwrap().load_program(&[0xaa; 16], 0);
    assert_eq!(
        sc(&mut port, 0x8800, AccessWidth::Doubleword, &[0; 8], context),
        ConditionalStatus::Success
    );

    // An image covering the reserved span does.
    let (_, context) = lr(&mut port, 0x8800, AccessWidth::Doubleword);
    memory.lock().unwrap().load_program(&vec![0xbb; 0x900], 0);
    assert_eq!(
        sc(&mut port, 0x8800, AccessWidth::Doubleword, &[0; 8], context),
        ConditionalStatus::Failure
    );
}

#[test]
fn rejected_writes_and_failed_write_suffixes_bump_nothing() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

    // A typed write rejected on bounds commits nothing and bumps nothing.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert!(memory.lock().unwrap().write_dword(0x1000, 0xdead).is_err());
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[1; 8], context),
        ConditionalStatus::Success
    );

    // A typed write rejected on alignment commits nothing either.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert!(memory.lock().unwrap().write_word(0x8011, 0xdead).is_err());
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[1; 8], context),
        ConditionalStatus::Success
    );

    // A raw write_bytes rejected on bounds commits no prefix at all.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert!(memory
        .lock()
        .unwrap()
        .write_bytes(0x80, &[9; 0x81])
        .is_err());
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[1; 8], context),
        ConditionalStatus::Success
    );
}

#[test]
fn adjacent_commit_boundaries_preserve_exact_interval_versions() {
    // Regression for the interval-split defect found in review: a commit
    // ending exactly where a later interval starts must not erase that
    // interval's version (a non-overlapping SC must still succeed), and a
    // commit covering two intervals must not leave the later bytes stale
    // (an overlapping SC must still fail).
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

    // Two adjacent dword commits create two adjacent intervals.
    {
        let mut ram = memory.lock().unwrap();
        ram.write_dword(0x00, 0x1111).unwrap();
        ram.write_dword(0x08, 0x2222).unwrap();
    }

    // A word write ending exactly at the second span's start is
    // non-overlapping: the SC must succeed.
    let (_, context) = lr(&mut port, 0x8008, AccessWidth::Doubleword);
    memory.lock().unwrap().write_word(0x04, 0xaabb).unwrap(); // commits [4,8)
    assert_eq!(
        sc(&mut port, 0x8008, AccessWidth::Doubleword, &[1; 8], context),
        ConditionalStatus::Success,
        "a write ending at the reserved span's start must not fail its SC"
    );

    // Same for a byte write ending at the span start.
    let (_, context) = lr(&mut port, 0x8008, AccessWidth::Doubleword);
    memory.lock().unwrap().write_byte(0x07, 0xcc).unwrap(); // commits [7,8)
    assert_eq!(
        sc(&mut port, 0x8008, AccessWidth::Doubleword, &[2; 8], context),
        ConditionalStatus::Success
    );

    // A raw write_bytes overlapping both intervals fails the later span's
    // SC: bytes 8..12 were committed over, so the snapshot must not replay.
    {
        let mut ram = memory.lock().unwrap();
        ram.write_dword(0x10, 0x3333).unwrap();
        ram.write_dword(0x18, 0x4444).unwrap();
    }
    let (_, context) = lr(&mut port, 0x8018, AccessWidth::Doubleword);
    memory
        .lock()
        .unwrap()
        .write_bytes(0x14, &[0xdd; 8])
        .unwrap(); // commits [0x14,0x1c)
    assert_eq!(
        sc(&mut port, 0x8018, AccessWidth::Doubleword, &[3; 8], context),
        ConditionalStatus::Failure,
        "a write overlapping both intervals must fail SC on the later span"
    );
    // The unwritten tail of the second interval kept its version: a fresh
    // LR on only that tail followed by an unrelated write still succeeds.
    let (_, context) = lr(&mut port, 0x801c, AccessWidth::Word);
    memory.lock().unwrap().write_byte(0x60, 0xee).unwrap();
    assert_eq!(
        sc(&mut port, 0x801c, AccessWidth::Word, &[4; 4], context),
        ConditionalStatus::Success
    );
}

#[test]
fn failed_host_write_commits_and_bumps_only_its_prefix() {
    // The facade's write_mem shape: a write_byte loop over the shared handle
    // where a late byte fails (§5.5 W4).  Committed prefix bytes bump; the
    // rejected suffix does not.
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

    let host_write = |memory: &SharedRam, offset: u64, bytes: &[u8]| -> Result<(), ()> {
        let mut guard = memory.lock().unwrap();
        for (i, &byte) in bytes.iter().enumerate() {
            guard.write_byte(offset + i as u64, byte).map_err(|_| ())?;
        }
        Ok(())
    };

    // Prefix lands inside the reserved span → SC fails even though the host
    // write itself failed partway.
    let (_, context) = lr(&mut port, 0x80f8, AccessWidth::Doubleword);
    assert!(host_write(&memory, 0xf8, &[0xaa; 16]).is_err());
    assert_eq!(
        sc(&mut port, 0x80f8, AccessWidth::Doubleword, &[0; 8], context),
        ConditionalStatus::Failure,
        "the committed prefix of a failed host write bumps exactly the committed bytes"
    );

    // The same failed write does not touch an unrelated reservation.
    let (_, context) = lr(&mut port, 0x8040, AccessWidth::Doubleword);
    assert!(host_write(&memory, 0xf8, &[0xbb; 16]).is_err());
    assert_eq!(
        sc(&mut port, 0x8040, AccessWidth::Doubleword, &[1; 8], context),
        ConditionalStatus::Success
    );

    // A host write that fails on its first byte commits no prefix at all.
    let (_, context) = lr(&mut port, 0x8048, AccessWidth::Doubleword);
    assert!(host_write(&memory, 0x200, &[0xcc; 4]).is_err());
    assert_eq!(
        sc(&mut port, 0x8048, AccessWidth::Doubleword, &[2; 8], context),
        ConditionalStatus::Success
    );
}

#[test]
fn committed_amo_and_successful_sc_writes_bump_their_own_span() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);

    // An AMO's committed write invalidates a reservation covering its span.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    rmw(
        &mut port,
        0x8010,
        AmoWidth::Doubleword,
        AmoOperation::Add,
        &1u64.to_le_bytes(),
    );
    assert_eq!(
        sc(&mut port, 0x8010, AccessWidth::Doubleword, &[0; 8], context),
        ConditionalStatus::Failure
    );

    // A successful SC's own commit bumps bookkeeping: replaying the stale
    // LR-time snapshot must fail even though no other writer ran.
    let (_, context) = lr(&mut port, 0x8020, AccessWidth::Doubleword);
    assert_eq!(
        sc(&mut port, 0x8020, AccessWidth::Doubleword, &[5; 8], context),
        ConditionalStatus::Success
    );
    assert_eq!(
        sc(&mut port, 0x8020, AccessWidth::Doubleword, &[6; 8], context),
        ConditionalStatus::Failure,
        "the successful SC's committed write bumps its own span"
    );

    // A non-overlapping AMO never fails the reservation.
    let (_, context) = lr(&mut port, 0x8040, AccessWidth::Doubleword);
    rmw(
        &mut port,
        0x8060,
        AmoWidth::Doubleword,
        AmoOperation::Swap,
        &9u64.to_le_bytes(),
    );
    assert_eq!(
        sc(&mut port, 0x8040, AccessWidth::Doubleword, &[7; 8], context),
        ConditionalStatus::Success
    );
    assert_eq!(memory.lock().unwrap().read_dword(0x60).unwrap(), 9);
}

#[test]
fn conditional_failure_and_malformed_contexts_commit_nothing() {
    let (memory, mut port) = ram_atomic_port(0x8000, 0x100);
    memory.lock().unwrap().write_dword(0x10, 0x42).unwrap();

    // A conditional-failure SC performs no write and bumps nothing: the
    // stored payload must not appear, and a later clean LR→SC still works.
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    memory.lock().unwrap().write_dword(0x10, 0x43).unwrap();
    assert_eq!(
        sc(
            &mut port,
            0x8010,
            AccessWidth::Doubleword,
            &[0xff; 8],
            context
        ),
        ConditionalStatus::Failure
    );
    assert_eq!(memory.lock().unwrap().read_dword(0x10).unwrap(), 0x43);
    let (_, context) = lr(&mut port, 0x8010, AccessWidth::Doubleword);
    assert_eq!(
        sc(
            &mut port,
            0x8010,
            AccessWidth::Doubleword,
            &[0x77; 8],
            context
        ),
        ConditionalStatus::Success
    );

    // A reservation context whose snapshot cannot describe the span is a
    // protocol failure, never a silent conditional outcome.
    let bad_context = AtomicReservationContext {
        reserved: PhysicalSpan {
            paddr: 0x8018,
            width: AccessWidth::Doubleword,
        },
        snapshot: snapshot(&[1, 2, 3]),
    };
    let error = port
        .access_atomic(
            AtomicRequest::store_conditional(
                0x8018,
                AccessWidth::Doubleword,
                ORDERING,
                &[0; 8],
                bad_context,
            )
            .unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            AtomicAccessError::Protocol(AtomicProtocolError::BackendProtocol { .. })
        ),
        "a malformed committed-write snapshot is a protocol failure, got {error:?}"
    );

    // An uncovered context is a valid SC request. The RAM target validates
    // the requested span, then reports conditional failure without a write.
    let uncovered = AtomicRequest::store_conditional(
        0x8020,
        AccessWidth::Doubleword,
        ORDERING,
        &[0; 8],
        AtomicReservationContext {
            reserved: PhysicalSpan {
                paddr: 0x8018,
                width: AccessWidth::Doubleword,
            },
            snapshot: snapshot(&[0; 64]),
        },
    )
    .unwrap();
    assert_eq!(
        port.access_atomic(uncovered).unwrap().conditional_status(),
        Some(ConditionalStatus::Failure)
    );
    assert_eq!(memory.lock().unwrap().read_dword(0x20).unwrap(), 0);

    // A reservation context whose reserved span extends outside this RAM's
    // bookkeeping domain — while still containing the request span — is a
    // protocol failure: the Hart could not have obtained that snapshot here.
    // Reserved 0x80fc..0x8104 contains the request 0x80fc..0x8100, but the
    // domain ends at 0x8100.
    let error = port
        .access_atomic(
            AtomicRequest::store_conditional(
                0x80fc,
                AccessWidth::Word,
                ORDERING,
                &[0; 4],
                AtomicReservationContext {
                    reserved: PhysicalSpan {
                        paddr: 0x80fc,
                        width: AccessWidth::Doubleword,
                    },
                    snapshot: snapshot(&[0; 64]),
                },
            )
            .unwrap(),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            AtomicAccessError::Protocol(AtomicProtocolError::BackendProtocol { .. })
        ),
        "a foreign reservation context is a protocol failure, got {error:?}"
    );
    assert_eq!(
        memory.lock().unwrap().read_dword(0x10).unwrap(),
        u64::from_le_bytes([0x77; 8])
    );
}
