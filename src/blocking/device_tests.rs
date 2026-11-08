//! Device-target rows of the TLM-Modeled Devices contract's unit U1.
//!
//! These tests drive the two in-tree device targets through the device
//! adapter, so the event each window reports and the register answer it gives
//! are checked where the Platform sees them.  The bad-target protocol proofs
//! belong to unit U2; the one exception here is the RAM adapter's rejection of
//! a device event, because only a device window may publish one (device D2).

use super::adapter::TlmRamAdapter;
use super::device::{BlockingDeviceEvent, DeviceWindow, TlmDeviceAdapter};
use super::in_tree::{InTreeHtif, InTreeUart};
use super::test_support::{HostHtif, HostHtifFault, HostUart, HostUartFault};
use super::{
    BlockingDebug, BlockingInitiator, BlockingResponseStatus, BlockingTarget, BlockingTransaction,
    DeviceTargets, Femtoseconds, TickPeriod,
};
use crate::hart_amods::{self, AmoOperation};
use crate::peripherals::uart16550::{reg_offset, Uart16550, FIFO_DEPTH};
use crate::physical::{
    AmoWidth, AtomicOrdering, AtomicRequest, PhysicalAccessKind, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalTargetRejectionReason, PhysicalWidth,
};
use std::sync::{Arc, Mutex};

const UART_BASE: u64 = 0x1000_0000;
const HTIF_BASE: u64 = 0x4000_8000;
const RAM_BASE: u64 = 0x8000_0000;

fn uart_target() -> (Arc<Mutex<Uart16550>>, TlmDeviceAdapter) {
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let adapter =
        TlmDeviceAdapter::new(DeviceWindow::Uart, Arc::new(InTreeUart::new(uart.clone())));
    (uart, adapter)
}

fn htif_target() -> TlmDeviceAdapter {
    TlmDeviceAdapter::new(DeviceWindow::Htif, Arc::new(InTreeHtif::new()))
}

fn uart_write(adapter: &TlmDeviceAdapter, offset: u64, byte: u8) -> Vec<BlockingDeviceEvent> {
    let payload = [byte];
    let request =
        PhysicalRequest::data_write(UART_BASE + offset, PhysicalWidth::Byte, &payload).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    assert!(
        outcome.response.is_ok(),
        "the in-tree UART rejected a valid byte write: {:?}",
        outcome.response
    );
    outcome.event.into_iter().collect()
}

fn uart_read(adapter: &TlmDeviceAdapter, offset: u64) -> (u8, Vec<BlockingDeviceEvent>) {
    let request = PhysicalRequest::data_read(UART_BASE + offset, PhysicalWidth::Byte).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    let response = outcome
        .response
        .unwrap_or_else(|error| panic!("the in-tree UART rejected a valid byte read: {error:?}"));
    let bytes = response.read_bytes().expect("a UART read returns its byte");
    (bytes[0], outcome.event.into_iter().collect())
}

#[test]
fn an_accepted_thr_write_reports_one_uart_transmit() {
    let (uart, adapter) = uart_target();
    let events = uart_write(&adapter, reg_offset::RBR_THR, b'A');
    assert_eq!(
        events,
        [BlockingDeviceEvent::UartTransmit(b'A')],
        "one accepted THR write must report exactly one transmit event"
    );
    let uart = uart.lock().unwrap();
    assert_eq!(
        uart.tx_fifo_data(),
        [b'A'],
        "the reported byte did not reach the transmit FIFO"
    );
}

#[test]
fn a_full_transmit_fifo_still_reports_the_byte() {
    let (uart, adapter) = uart_target();
    for index in 0..FIFO_DEPTH {
        let events = uart_write(&adapter, reg_offset::RBR_THR, b'a' + index as u8);
        assert_eq!(events.len(), 1, "a queued byte reported no transmit event");
    }
    assert_eq!(uart.lock().unwrap().tx_fifo_data().len(), FIFO_DEPTH);

    // device D4: the byte left the guest even though the FIFO could not queue
    // it, so the one publication site still publishes it exactly once.
    let events = uart_write(&adapter, reg_offset::RBR_THR, b'Z');
    assert_eq!(
        events,
        [BlockingDeviceEvent::UartTransmit(b'Z')],
        "a THR write with a full transmit FIFO must still report one transmit event"
    );
    assert_eq!(
        uart.lock().unwrap().tx_fifo_data().len(),
        FIFO_DEPTH,
        "the full FIFO accepted a seventeenth byte"
    );
}

#[test]
fn a_dlab_write_and_a_read_report_no_event() {
    let (uart, adapter) = uart_target();
    // LCR bit 7 selects the divisor latch, so offset 0 writes DLL.
    assert!(uart_write(&adapter, reg_offset::LCR, 0x80).is_empty());
    assert!(
        uart_write(&adapter, reg_offset::RBR_THR, 0x2a).is_empty(),
        "a divisor-latch write reported a transmit event"
    );
    assert_eq!(
        uart.lock().unwrap().read_reg(reg_offset::DLL),
        0x2a,
        "the divisor-latch write did not reach DLL"
    );
    assert!(
        uart.lock().unwrap().tx_fifo_data().is_empty(),
        "a divisor-latch write reached the transmit FIFO"
    );

    let (status, events) = uart_read(&adapter, reg_offset::LSR);
    assert!(
        status & 0x20 != 0,
        "the transmit holding register did not read as empty"
    );
    assert!(
        events.is_empty(),
        "a UART register read reported a device event"
    );
}

#[test]
fn the_htif_target_reads_zero_and_reports_its_write_value() {
    let adapter = htif_target();
    let request = PhysicalRequest::data_read(HTIF_BASE, PhysicalWidth::Doubleword).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    let response = outcome
        .response
        .unwrap_or_else(|error| panic!("the in-tree HTIF rejected its dword read: {error:?}"));
    assert_eq!(
        response.read_bytes(),
        Some(&[0u8; 8][..]),
        "the HTIF dword read must answer zero on the device route"
    );
    assert!(
        outcome.event.is_none(),
        "the HTIF read reported a write event"
    );

    let value = 0x0102_0304_0506_0708u64;
    let payload = value.to_le_bytes();
    let request =
        PhysicalRequest::data_write(HTIF_BASE, PhysicalWidth::Doubleword, &payload).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    assert!(
        outcome.response.is_ok(),
        "the in-tree HTIF rejected its dword write: {:?}",
        outcome.response
    );
    assert_eq!(
        outcome.event,
        Some(BlockingDeviceEvent::HtifWrite(value)),
        "one accepted HTIF dword write must report its value exactly once"
    );
}

#[test]
fn a_ram_target_that_reports_a_device_event_is_a_protocol_failure() {
    let target = Arc::new(super::test_support::ScriptTarget::new(RAM_BASE, 0x1000));
    target.set_device_event(BlockingDeviceEvent::UartTransmit(b'x'));
    let adapter = TlmRamAdapter::with_tick_period(
        target.clone() as Arc<dyn BlockingTarget>,
        TickPeriod::DEFAULT,
        false,
        false,
    )
    .expect("the scripted target's declared maximum is representable");
    let request = PhysicalRequest::data_read(RAM_BASE, PhysicalWidth::Doubleword).unwrap();
    let error = adapter
        .transact(&request, BlockingInitiator::Hart, 0)
        .expect_err("a RAM target's device event was accepted");
    assert!(
        matches!(error, PhysicalBackendError::Protocol { .. }),
        "a RAM target's device event was not a protocol failure: {error:?}"
    );
    let PhysicalBackendError::Protocol { context } = &error else {
        unreachable!("checked above");
    };
    assert!(
        context.contains("only a device window publishes one"),
        "the failure does not name the device-event rule: {context}"
    );
    assert_eq!(
        target
            .transactions
            .load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the access was retried instead of failing once"
    );
    assert_eq!(
        adapter.consumed_delay(),
        Femtoseconds::ZERO,
        "the rejected access charged delay"
    );
}

/// A RAM target that completes atomics normally and then reports a device
/// event, so the atomic path's rejection is proven on its own.
struct EventReportingRam {
    inner: super::ZeroDelayRam,
}

impl BlockingTarget for EventReportingRam {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::ZERO
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.inner.b_transport(transaction);
        if transaction.status() == BlockingResponseStatus::Ok {
            transaction.report_device_event(BlockingDeviceEvent::UartTransmit(b'x'));
        }
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.inner.transport_reset()
    }
}

/// device D2: the RAM adapter's atomic path rejects a device event too, and
/// the rejection is the device-event rule, not the atomic-completion shape.
#[test]
fn an_atomic_ram_completion_that_reports_a_device_event_is_a_protocol_failure() {
    let target = Arc::new(EventReportingRam {
        inner: super::ZeroDelayRam::new(),
    });
    let adapter = TlmRamAdapter::with_tick_period(
        target as Arc<dyn BlockingTarget>,
        TickPeriod::DEFAULT,
        false,
        false,
    )
    .expect("the zero-delay target's declared maximum is representable");
    let operand = 1u64.to_le_bytes();
    let request = AtomicRequest::rmw(
        RAM_BASE,
        AmoWidth::Doubleword.physical_width(),
        AtomicOrdering { aq: true, rl: true },
        &operand,
        hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
    )
    .unwrap();
    let error = adapter
        .transact_atomic(&request, 0)
        .expect_err("an atomic RAM completion's device event was accepted");
    assert!(
        matches!(error, PhysicalBackendError::Protocol { .. }),
        "an atomic RAM target's device event was not a protocol failure: {error:?}"
    );
    let PhysicalBackendError::Protocol { context } = &error else {
        unreachable!("checked above");
    };
    assert!(
        context.contains("only a device window publishes one"),
        "the failure does not name the device-event rule: {context}"
    );
}

#[test]
fn a_device_read_answers_the_window_register_bytes() {
    let (_uart, adapter) = uart_target();
    let request =
        PhysicalRequest::data_read(UART_BASE + reg_offset::LSR, PhysicalWidth::Byte).unwrap();
    assert_eq!(
        request.category(),
        PhysicalAccessKind::DataRead,
        "the read request was built with the wrong category"
    );
    let (status, _) = uart_read(&adapter, reg_offset::LSR);
    assert_eq!(
        status & 0x40,
        0x40,
        "the transmitter-empty bit was not set at reset"
    );
}

// ---- TLM-Modeled Devices U2: deliberately misbehaving host targets ----
//
// The in-tree targets cannot be made to misbehave, so every response rule that
// only a foreign target can break is proven here against a host UART fixture
// the crate does not own (device D2, D4, D5).

/// The device adapter over the host UART fixture.
fn host_uart_adapter(target: &Arc<HostUart>) -> TlmDeviceAdapter {
    TlmDeviceAdapter::new(DeviceWindow::Uart, target.clone())
}

/// One byte write to the host UART fixture's transmit holding register.
fn thr_write<'a>(payload: &'a [u8; 1]) -> PhysicalRequest<'a> {
    PhysicalRequest::data_write(UART_BASE, PhysicalWidth::Byte, payload).unwrap()
}

/// True when the adapter classified the access as a protocol failure.
fn is_protocol(response: &PhysicalBackendResult) -> bool {
    matches!(response, Err(PhysicalBackendError::Protocol { .. }))
}

#[test]
fn a_device_target_that_reports_a_second_event_is_a_protocol_failure() {
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::SecondEvent);
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "a second device event was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected access still carried an event to publication"
    );
}

#[test]
fn a_device_target_that_reports_another_windows_event_is_a_protocol_failure() {
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::WrongWindowEvent);
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "another window's device event was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected access still carried an event to publication"
    );
}

#[test]
fn a_device_target_that_changes_the_payload_is_a_protocol_failure() {
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::ChangedPayload);
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "a changed request payload was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected access still carried an event to publication"
    );
}

#[test]
fn a_device_target_that_reports_host_elapsed_time_is_a_protocol_failure() {
    // device D4: host time may not pass inside a device callback.
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::Elapsed(1_000_000));
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "host elapsed time on a device transaction was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected access still carried an event to publication"
    );
}

#[test]
fn a_device_target_that_sets_the_direct_memory_hint_is_a_protocol_failure() {
    // device D5: a device window never offers direct memory, and the device
    // path never issues a query, so the hint can only be a protocol failure.
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::DmiHint);
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "the direct-memory hint on a device transaction was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected access still carried an event to publication"
    );
}

#[test]
fn a_device_event_beside_a_fault_is_a_protocol_failure() {
    // device D4: only an accepted access publishes an event, so a transmit
    // beside a target fault is the access's protocol failure.
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::StatusWithEvent(
        BlockingResponseStatus::AddressError,
    ));
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "an event beside a target fault was accepted: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a faulted access still carried an event to publication"
    );
}

#[test]
fn a_device_fault_without_an_event_is_a_target_fault() {
    // device D4: the fault keeps the normalized target classification and
    // publishes nothing, because no event was reported at all.
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::Status(BlockingResponseStatus::AddressError));
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        matches!(
            &outcome.response,
            Err(PhysicalBackendError::Target {
                reason: PhysicalTargetRejectionReason::TargetError,
                ..
            })
        ),
        "a device target fault lost its guest-visible classification: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a faulted access carried an event to publication"
    );
}

#[test]
fn an_unknown_completion_takes_precedence_over_a_device_event() {
    // device D4: the outcome-unknown classification runs first, so a response
    // that would otherwise be an accepted transmit is an unknown completion
    // with no event.
    let target = HostUart::new(UART_BASE);
    target.set_fault(HostUartFault::Unknown);
    let payload = *b"H";
    let outcome =
        host_uart_adapter(&target).transact(&thr_write(&payload), BlockingInitiator::Hart);
    assert!(
        matches!(&outcome.response, Err(PhysicalBackendError::Unknown { .. })),
        "an outcome-unknown device response was not an unknown completion: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "an unknown completion published a device event"
    );
}

// ---- Host HTIF Service U1: the host HTIF target (htif D2–D4, D10) ----
//
// The HTIF endpoint's own selection, its read and write answers, and the two
// response rules this contract adds.  The Machine-level rows live in
// `src/machine/tests.rs`, the typed-view rows in `src/executor_device_tests.rs`
// and the run stop in `src/executor_device_tests.rs`.

/// The device adapter over the host HTIF fixture.
fn host_htif_adapter(target: &Arc<HostHtif>) -> TlmDeviceAdapter {
    TlmDeviceAdapter::new(DeviceWindow::Htif, target.clone())
}

/// One tohost read of the endpoint.
fn dword_read() -> PhysicalRequest<'static> {
    PhysicalRequest::data_read(HTIF_BASE, PhysicalWidth::Doubleword).unwrap()
}

/// device D2: the selection names each window independently, and `none` names
/// no window at all.
#[test]
fn the_device_selection_names_each_window_independently() {
    let selection = DeviceTargets::none();
    assert!(selection.uart().is_none() && selection.htif().is_none());

    let uart = HostUart::new(UART_BASE);
    let selection = DeviceTargets::with_uart(uart.clone());
    assert!(selection.uart().is_some() && selection.htif().is_none());

    let htif = HostHtif::new(HTIF_BASE);
    let selection = DeviceTargets::with_htif(htif.clone());
    assert!(selection.uart().is_none() && selection.htif().is_some());

    // htif D2: one selection may name a host target for each window.
    let selection = DeviceTargets::with_uart_and_htif(uart.clone(), htif.clone());
    assert!(selection.uart().is_some() && selection.htif().is_some());
    let debug = format!("{selection:?}");
    assert!(
        debug.contains("uart: true") && debug.contains("htif: true"),
        "the selection's Debug form does not name both windows: {debug}"
    );
}

/// device D3/D4, htif D3: the host target answers the same two requests the
/// in-tree endpoint answers: its own read state and exactly one event carrying
/// the request's eight data bytes.
#[test]
fn a_host_htif_target_reads_its_state_and_reports_the_writes_value() {
    let target = HostHtif::new(HTIF_BASE);
    target.set_read_value(0x1122_3344_5566_7788);
    let adapter = host_htif_adapter(&target);

    let outcome = adapter.transact(&dword_read(), BlockingInitiator::Hart);
    let response = outcome
        .response
        .unwrap_or_else(|error| panic!("the host HTIF rejected its dword read: {error:?}"));
    assert_eq!(
        response.read_bytes(),
        Some(&0x1122_3344_5566_7788u64.to_le_bytes()[..]),
        "the host HTIF target's read state did not reach the response"
    );
    assert!(
        outcome.event.is_none(),
        "the host HTIF target's read published an event"
    );

    let value = 0x0102_0304_0506_0708u64;
    let payload = value.to_le_bytes();
    let request =
        PhysicalRequest::data_write(HTIF_BASE, PhysicalWidth::Doubleword, &payload).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    assert!(
        outcome.response.is_ok(),
        "the host HTIF rejected its dword write: {:?}",
        outcome.response
    );
    assert_eq!(
        outcome.event,
        Some(BlockingDeviceEvent::HtifWrite(value)),
        "one accepted host HTIF write must report the request's value exactly once"
    );
    assert_eq!(
        target.reported(),
        [value],
        "the host target did not report the guest's eight bytes"
    );
    assert_eq!(
        target.transactions(),
        2,
        "the host HTIF target did not serve exactly the read and the write"
    );
}

/// htif D10 rule 1: a read yields neither a UART transmit byte nor an HTIF
/// write, so an event the target reports on a read is that access's protocol
/// failure on both windows.
#[test]
fn a_device_event_on_a_read_is_a_protocol_failure_on_both_windows() {
    // The UART window: one register read with a transmit event beside it.
    let uart = HostUart::new(UART_BASE);
    uart.set_lsr(0x61);
    uart.set_fault(HostUartFault::EventOnRead);
    let request =
        PhysicalRequest::data_read(UART_BASE + reg_offset::LSR, PhysicalWidth::Byte).unwrap();
    let outcome = host_uart_adapter(&uart).transact(&request, BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "a UART transmit event on a read was accepted: {:?}",
        outcome.response
    );
    let PhysicalBackendError::Protocol { context } = outcome.response.unwrap_err() else {
        unreachable!("checked above");
    };
    assert!(
        context.contains("a read yields no device event"),
        "the UART read event was not refused by the read rule: {context}"
    );
    assert!(
        outcome.event.is_none(),
        "a rejected UART read still carried an event to publication"
    );

    // The HTIF window: one dword read with its window's write event beside it.
    let htif = HostHtif::new(HTIF_BASE);
    htif.set_read_value(0x5a);
    htif.set_fault(HostHtifFault::EventOnRead);
    let outcome = host_htif_adapter(&htif).transact(&dword_read(), BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "an HTIF write event on a read was accepted: {:?}",
        outcome.response
    );
    let PhysicalBackendError::Protocol { context } = outcome.response.unwrap_err() else {
        unreachable!("checked above");
    };
    assert!(
        context.contains("a read yields no device event"),
        "the HTIF read event was not refused by the read rule: {context}"
    );
    assert!(
        outcome.event.is_none(),
        "a rejected HTIF read still carried an event to publication"
    );
    assert_eq!(
        htif.reported(),
        [0x5a],
        "the fixture did not report the read event the rule refused"
    );
}

/// htif D10 rule 2: the one window whose event carries a value reports it only
/// when it is the request's own eight data bytes.
#[test]
fn a_host_htif_value_other_than_the_request_is_a_protocol_failure() {
    let target = HostHtif::new(HTIF_BASE);
    target.set_fault(HostHtifFault::WrongValue(0xdead_beef));
    let value = 0x0102_0304_0506_0708u64;
    let payload = value.to_le_bytes();
    let request =
        PhysicalRequest::data_write(HTIF_BASE, PhysicalWidth::Doubleword, &payload).unwrap();
    let outcome = host_htif_adapter(&target).transact(&request, BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "a reported HTIF value other than the request's was accepted: {:?}",
        outcome.response
    );
    let PhysicalBackendError::Protocol { context } = outcome.response.unwrap_err() else {
        unreachable!("checked above");
    };
    assert!(
        context.contains("must equal the request's eight data bytes"),
        "the write was not refused by the value rule: {context}"
    );
    assert!(
        context.contains("0x0102030405060708"),
        "the value rule's diagnostic does not name the request's bytes: {context}"
    );
    assert!(
        outcome.event.is_none(),
        "a rejected HTIF write still carried its value to publication"
    );
    assert_eq!(
        target.reported(),
        [0xdead_beef],
        "the fixture did not report the wrong value the rule refused"
    );
}

/// htif D10 rule 2: a target that rewrites its buffer through `data_mut` cannot
/// make that value the request's, because the rule compares with the request
/// captured before the target ran.
#[test]
fn a_host_htif_that_rewrites_its_buffer_is_refused_the_request_value() {
    let target = HostHtif::new(HTIF_BASE);
    target.set_fault(HostHtifFault::RewriteBuffer(0x0711));
    let value = 0x0011_2233_4455_6677u64;
    let payload = value.to_le_bytes();
    let request =
        PhysicalRequest::data_write(HTIF_BASE, PhysicalWidth::Doubleword, &payload).unwrap();
    let outcome = host_htif_adapter(&target).transact(&request, BlockingInitiator::Hart);
    assert!(
        is_protocol(&outcome.response),
        "a rewritten buffer became the reported value: {:?}",
        outcome.response
    );
    assert!(
        outcome.event.is_none(),
        "a rejected HTIF write still carried its value to publication"
    );
}

/// htif D10: both rules leave the in-tree targets unchanged, and the UART
/// transmit byte is the byte the register write accepted rather than the
/// request's data.
#[test]
fn the_value_rule_leaves_the_in_tree_endpoint_and_the_uart_byte_alone() {
    // The in-tree endpoint reports the request's eight bytes, so it passes.
    let adapter = htif_target();
    let value = 0xff00_ff00_ff00_ff00u64;
    let payload = value.to_le_bytes();
    let request =
        PhysicalRequest::data_write(HTIF_BASE, PhysicalWidth::Doubleword, &payload).unwrap();
    let outcome = adapter.transact(&request, BlockingInitiator::Hart);
    assert!(
        outcome.response.is_ok(),
        "the value rule refused the in-tree endpoint: {:?}",
        outcome.response
    );
    assert_eq!(outcome.event, Some(BlockingDeviceEvent::HtifWrite(value)));

    // A UART target that reports a byte other than the written one stays
    // accepted: the transmit byte is the register write's, not the request's.
    let uart = HostUart::new(UART_BASE);
    uart.set_fault(HostUartFault::WrongTransmitByte(b'Z'));
    let payload = *b"A";
    let request = PhysicalRequest::data_write(UART_BASE, PhysicalWidth::Byte, &payload).unwrap();
    let outcome = host_uart_adapter(&uart).transact(&request, BlockingInitiator::Hart);
    assert!(
        outcome.response.is_ok(),
        "the value rule compared the UART transmit byte with the request: {:?}",
        outcome.response
    );
    assert_eq!(
        outcome.event,
        Some(BlockingDeviceEvent::UartTransmit(b'Z')),
        "the UART transmit byte was replaced instead of published"
    );
}
