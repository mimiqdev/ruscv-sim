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
use super::{BlockingInitiator, BlockingTarget, Femtoseconds, TickPeriod};
use crate::peripherals::uart16550::{reg_offset, Uart16550, FIFO_DEPTH};
use crate::physical::{PhysicalAccessKind, PhysicalBackendError, PhysicalRequest, PhysicalWidth};
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
