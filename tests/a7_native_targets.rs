//! Executable A7 T2 native-target tests.
//!
//! These tests exercise the native raw backend views only.  They deliberately
//! do not connect the port to `RiscvCore`, change the public executor loops, or
//! introduce a legacy atomic bridge; those are T3/T4 work.  The typed
//! `MemoryInterface` view is used beside the raw view to prove that both views
//! retain one RAM/device/locking domain.

use ruscv_sim::executor::{RiscVSimulator, SystemBus, SYSTEM_BUS_HTIF_BASE, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::{uart16550, Uart16550, FIFO_DEPTH};
use ruscv_sim::physical::{
    AccessWidth, NativeRamBackend, NativeSystemBusBackend, PhysicalAccessError, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendResult, PhysicalCompletion, PhysicalRequest,
    PhysicalResponse, PhysicalResponseBytes, PhysicalTargetRejectionReason,
    ValidatedPhysicalAccess,
};
use std::sync::{Arc, Mutex};
use std::thread;

type SharedRam = Arc<Mutex<SimpleMemory>>;
type SharedUart = Arc<Mutex<Uart16550>>;
type SharedBus = Arc<Mutex<SystemBus>>;

type NativeRamPort = ValidatedPhysicalAccess<NativeRamBackend>;
type NativeBusPort = ValidatedPhysicalAccess<NativeSystemBusBackend>;

fn ram_port(base: u64, size: usize) -> (SharedRam, NativeRamPort) {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    let backend = NativeRamBackend::new(memory.clone(), base, size);
    (memory, ValidatedPhysicalAccess::new(backend))
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

fn bus_port(ram_base: u64, ram_size: usize) -> (SharedRam, SharedUart, SharedBus, NativeBusPort) {
    let (ram, uart, bus) = bus_parts(ram_base, ram_size);
    let port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus.clone()));
    (ram, uart, bus, port)
}

fn target_reason(error: PhysicalAccessError) -> PhysicalTargetRejectionReason {
    match error {
        PhysicalAccessError::TargetRejected(rejection) => rejection.reason,
        other => panic!("expected a target rejection, got {other:?}"),
    }
}

fn assert_target(
    port: &mut impl ruscv_sim::physical::PhysicalAccess,
    request: PhysicalRequest<'_>,
    reason: PhysicalTargetRejectionReason,
) {
    assert_eq!(target_reason(port.access(request).unwrap_err()), reason);
}

#[test]
fn native_ram_transfers_raw_bytes_at_all_widths_and_fetches() {
    let (memory, mut port) = ram_port(0x8000, 0x40);
    let cases: &[(u64, AccessWidth, &[u8])] = &[
        (0x8000, AccessWidth::Byte, &[0xd3]),
        (0x8002, AccessWidth::Halfword, &[0x07, 0xa9]),
        (0x8004, AccessWidth::Word, &[0x4e, 0x80, 0x21, 0xfe]),
        (
            0x8008,
            AccessWidth::Doubleword,
            &[0x65, 0x19, 0xa2, 0x03, 0xf4, 0x85, 0x16, 0xe7],
        ),
    ];

    for &(address, width, bytes) in cases {
        let write = PhysicalRequest::data_write(address, width, bytes).unwrap();
        assert!(port.access(write).unwrap().is_write_acknowledgement());
        let read = PhysicalRequest::data_read(address, width).unwrap();
        assert_eq!(port.access(read).unwrap().read_bytes(), Some(bytes));
        let fetch = PhysicalRequest::fetch(address, width).unwrap();
        assert_eq!(port.access(fetch).unwrap().read_bytes(), Some(bytes));
    }

    // Raw RAM has no architectural alignment policy.  The one contiguous
    // transfer below is intentionally unaligned and is still not a byte loop.
    let unaligned = [0x91, 0x02, 0xe7, 0x40, 0x5b, 0x08, 0xc6, 0xaf];
    port.access(PhysicalRequest::data_write(0x8011, AccessWidth::Doubleword, &unaligned).unwrap())
        .unwrap();
    assert_eq!(
        memory.lock().unwrap().read_bytes(0x11, 8).unwrap(),
        unaligned
    );
}

#[test]
fn native_ram_boundaries_reject_one_past_overflow_and_cross_span_without_mutation() {
    let (memory, mut port) = ram_port(0x1000, 0x10);
    port.access(PhysicalRequest::data_write(0x100f, AccessWidth::Byte, &[0x7f]).unwrap())
        .unwrap();
    assert_eq!(
        port.access(PhysicalRequest::data_read(0x100f, AccessWidth::Byte).unwrap())
            .unwrap()
            .read_bytes(),
        Some(&[0x7f][..])
    );
    port.access(
        PhysicalRequest::data_write(0x1008, AccessWidth::Doubleword, &[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap(),
    )
    .unwrap();
    let before = memory.lock().unwrap().read_bytes(0, 16).unwrap();

    assert_target(
        &mut port,
        PhysicalRequest::data_write(0x1010, AccessWidth::Byte, &[0xff]).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_target(
        &mut port,
        PhysicalRequest::data_write(0x100f, AccessWidth::Halfword, &[0xaa, 0xbb]).unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );

    let overflow = port
        .access(PhysicalRequest::data_write(u64::MAX, AccessWidth::Doubleword, &[0; 8]).unwrap())
        .unwrap_err();
    let PhysicalAccessError::TargetRejected(rejection) = overflow else {
        panic!("overflow must remain a target rejection");
    };
    assert_eq!(
        rejection.reason,
        PhysicalTargetRejectionReason::RangeOverflow
    );
    assert_eq!(rejection.request.paddr, u64::MAX);
    assert_eq!(rejection.request.width, AccessWidth::Doubleword);
    assert_eq!(
        rejection.span.checked_end_inclusive(),
        None,
        "the rejected span retains its overflowing request context"
    );

    assert_eq!(memory.lock().unwrap().read_bytes(0, 16).unwrap(), before);
}

#[test]
fn old_typed_and_native_ram_views_share_one_storage_and_locking_domain() {
    let (memory, mut port) = ram_port(0x4000, 0x20);
    assert!(Arc::ptr_eq(port.backend().memory(), &memory));

    {
        let mut old = memory.lock().unwrap();
        old.write_word(8, 0xfeed_cafe).unwrap();
    }
    let old_write = port
        .access(PhysicalRequest::data_read(0x4008, AccessWidth::Word).unwrap())
        .unwrap();
    assert_eq!(old_write.read_bytes(), Some(&[0xfe, 0xca, 0xed, 0xfe][..]));

    let new_bytes = [0x91, 0x02, 0xe7, 0x40, 0x5b, 0x08, 0xc6, 0xaf];
    port.access(PhysicalRequest::data_write(0x4008, AccessWidth::Doubleword, &new_bytes).unwrap())
        .unwrap();
    assert_eq!(
        memory.lock().unwrap().read_dword(8).unwrap(),
        u64::from_le_bytes(new_bytes)
    );
}

#[test]
fn native_system_bus_preserves_ram_first_full_span_and_htif_fallback() {
    let make = |ram_size| {
        let (ram, _uart, bus) = bus_parts(SYSTEM_BUS_HTIF_BASE, ram_size);
        let callbacks = Arc::new(Mutex::new(Vec::new()));
        let callback_copy = callbacks.clone();
        bus.lock()
            .unwrap()
            .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
        let port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus.clone()));
        (ram, bus, callbacks, port)
    };

    let (ram4, _bus4, callbacks4, mut port4) = make(4);
    let value = 0xfeed_face_cafe_babeu64.to_le_bytes();
    port4
        .access(
            PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword, &value)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(*callbacks4.lock().unwrap(), vec![u64::from_le_bytes(value)]);
    assert_eq!(ram4.lock().unwrap().read_bytes(0, 4).unwrap(), vec![0; 4]);

    let (ram8, _bus8, callbacks8, mut port8) = make(8);
    port8
        .access(
            PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword, &value)
                .unwrap(),
        )
        .unwrap();
    assert!(callbacks8.lock().unwrap().is_empty());
    assert_eq!(
        ram8.lock().unwrap().read_dword(0).unwrap(),
        u64::from_le_bytes(value)
    );
}

#[test]
fn native_system_bus_never_stitches_ram_into_uart_or_htif() {
    let (_ram, uart, _bus, mut port) = bus_port(SYSTEM_BUS_UART_BASE - 2, 2);
    let tx_before = uart.lock().unwrap().tx_fifo_data().to_vec();
    assert_target(
        &mut port,
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE - 2, AccessWidth::Word, &[1, 2, 3, 4])
            .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_eq!(uart.lock().unwrap().tx_fifo_data(), tx_before.as_slice());

    let callback = Arc::new(Mutex::new(Vec::new()));
    // Rebuild the bus callback through the shared handle so this is a real
    // side-effect check rather than a response-only assertion.
    let (_ram, _uart, bus) = bus_parts(SYSTEM_BUS_HTIF_BASE - 4, 4);
    let callback_copy = callback.clone();
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
    let mut port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus));
    assert_target(
        &mut port,
        PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE - 4, AccessWidth::Doubleword, &[0; 8])
            .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert!(callback.lock().unwrap().is_empty());
}

#[test]
fn native_htif_requires_the_complete_raw_endpoint_and_does_not_fetch() {
    let (_ram, _uart, bus) = bus_parts(0, 0);
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let callback_copy = callbacks.clone();
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
    let mut port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus.clone()));

    let read = port
        .access(PhysicalRequest::data_read(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword).unwrap())
        .unwrap();
    assert_eq!(read.read_bytes(), Some(&[0; 8][..]));

    let value = 0x0123_4567_89ab_cdefu64.to_le_bytes();
    port.access(
        PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword, &value).unwrap(),
    )
    .unwrap();
    assert_eq!(*callbacks.lock().unwrap(), vec![u64::from_le_bytes(value)]);

    assert_target(
        &mut port,
        PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE + 1, AccessWidth::Word, &[1, 2, 3, 4])
            .unwrap(),
        PhysicalTargetRejectionReason::UnsupportedWidth,
    );
    assert_target(
        &mut port,
        PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE + 1, AccessWidth::Doubleword, &value)
            .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );
    assert_target(
        &mut port,
        PhysicalRequest::fetch(SYSTEM_BUS_HTIF_BASE, AccessWidth::Doubleword).unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_eq!(*callbacks.lock().unwrap(), vec![u64::from_le_bytes(value)]);

    // The legacy typed route is intentionally still start-address-only.  T2
    // tests it separately rather than applying raw-port tightening globally.
    let legacy_callbacks = Arc::new(Mutex::new(Vec::new()));
    let callback_copy = legacy_callbacks.clone();
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
    for offset in 0..8u64 {
        bus.lock()
            .unwrap()
            .write_dword(SYSTEM_BUS_HTIF_BASE + offset, offset + 1)
            .unwrap();
    }
    assert_eq!(
        *legacy_callbacks.lock().unwrap(),
        (1..=8).collect::<Vec<_>>()
    );
}

#[test]
fn native_uart_rejections_preserve_fifo_status_and_output_then_success_has_one_effect() {
    let (_ram, uart, bus) = bus_parts(0, 0);
    let output = Arc::new(Mutex::new(Vec::new()));
    let output_copy = output.clone();
    uart.lock()
        .unwrap()
        .set_output_callback(move |byte| output_copy.lock().unwrap().push(byte));
    {
        let mut guard = uart.lock().unwrap();
        guard.write_reg(uart16550::reg_offset::MCR, uart16550::mcr_bits::OUT2);
        guard.write_reg(
            uart16550::reg_offset::IER,
            uart16550::ier_bits::ERBFI | uart16550::ier_bits::ELSI,
        );
        for byte in 0..FIFO_DEPTH {
            guard.receive_byte(byte as u8);
        }
        // Overflow sets the line-status error while leaving the RX FIFO full.
        guard.receive_byte(0xff);
    }
    let fifo_before = uart.lock().unwrap().rx_fifo_data().to_vec();
    let interrupt_before = uart.lock().unwrap().interrupt_id();
    let tx_before = uart.lock().unwrap().tx_fifo_data().to_vec();
    assert!(interrupt_before & uart16550::iir_bits::NO_INT == 0);

    let mut port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus));
    for (width, payload) in [
        (AccessWidth::Halfword, vec![1, 2]),
        (AccessWidth::Word, vec![1, 2, 3, 4]),
        (AccessWidth::Doubleword, vec![1, 2, 3, 4, 5, 6, 7, 8]),
    ] {
        assert_target(
            &mut port,
            PhysicalRequest::data_read(SYSTEM_BUS_UART_BASE, width).unwrap(),
            PhysicalTargetRejectionReason::UnsupportedWidth,
        );
        assert_target(
            &mut port,
            PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, width, &payload).unwrap(),
            PhysicalTargetRejectionReason::UnsupportedWidth,
        );
    }
    assert_target(
        &mut port,
        PhysicalRequest::fetch(SYSTEM_BUS_UART_BASE, AccessWidth::Byte).unwrap(),
        PhysicalTargetRejectionReason::UnsupportedCategory,
    );
    assert_target(
        &mut port,
        PhysicalRequest::data_write(
            SYSTEM_BUS_UART_BASE + 0xff,
            AccessWidth::Halfword,
            &[0xaa, 0xbb],
        )
        .unwrap(),
        PhysicalTargetRejectionReason::Unmapped,
    );

    let uart_guard = uart.lock().unwrap();
    assert_eq!(uart_guard.rx_fifo_data(), fifo_before.as_slice());
    assert_eq!(uart_guard.interrupt_id(), interrupt_before);
    assert_eq!(uart_guard.tx_fifo_data(), tx_before.as_slice());
    drop(uart_guard);
    assert!(output.lock().unwrap().is_empty());

    let read = port
        .access(PhysicalRequest::data_read(SYSTEM_BUS_UART_BASE, AccessWidth::Byte).unwrap())
        .unwrap();
    assert_eq!(read.read_bytes(), Some(&[0][..]));
    assert_eq!(uart.lock().unwrap().rx_fifo_data().len(), FIFO_DEPTH - 1);

    port.access(
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, AccessWidth::Byte, b"Z").unwrap(),
    )
    .unwrap();
    assert_eq!(*output.lock().unwrap(), vec![b'Z']);
    assert_eq!(uart.lock().unwrap().tx_fifo_data(), b"Z");

    // The public native window remains 0x100 bytes; reserved byte offsets are
    // a successful zero/ignored register operation, not a TLM-sized rejection.
    let reserved = SYSTEM_BUS_UART_BASE + 0x80;
    assert_eq!(
        port.access(PhysicalRequest::data_read(reserved, AccessWidth::Byte).unwrap())
            .unwrap()
            .read_bytes(),
        Some(&[0][..])
    );
    port.access(PhysicalRequest::data_write(reserved, AccessWidth::Byte, &[0xff]).unwrap())
        .unwrap();
    assert_eq!(*output.lock().unwrap(), vec![b'Z']);
}

#[test]
fn native_system_bus_old_and_new_views_share_ram_uart_and_callbacks() {
    let (ram, uart, bus) = bus_parts(0x8000, 0x40);
    let output = Arc::new(Mutex::new(Vec::new()));
    let output_copy = output.clone();
    uart.lock()
        .unwrap()
        .set_output_callback(move |byte| output_copy.lock().unwrap().push(byte));
    assert!(Arc::ptr_eq(
        ValidatedPhysicalAccess::<NativeSystemBusBackend>::new(NativeSystemBusBackend::new(
            bus.clone()
        ))
        .backend()
        .target(),
        &bus
    ));

    let mut port = ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus.clone()));
    bus.lock().unwrap().write_word(0x8008, 0x4433_2211).unwrap();
    assert_eq!(
        port.access(PhysicalRequest::data_read(0x8008, AccessWidth::Word).unwrap())
            .unwrap()
            .read_bytes(),
        Some(&[0x11, 0x22, 0x33, 0x44][..])
    );

    let bytes = 0x8877_6655_4433_2211u64.to_le_bytes();
    port.access(PhysicalRequest::data_write(0x8010, AccessWidth::Doubleword, &bytes).unwrap())
        .unwrap();
    assert_eq!(
        ram.lock().unwrap().read_dword(0x10).unwrap(),
        u64::from_le_bytes(bytes)
    );

    port.access(
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, AccessWidth::Byte, b"!").unwrap(),
    )
    .unwrap();
    assert_eq!(*output.lock().unwrap(), vec![b'!']);
    assert_eq!(uart.lock().unwrap().tx_fifo_data(), b"!");
}

#[test]
fn poisoned_native_locks_are_backend_failures_without_retry_or_target_mutation() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(16)));
    let poison = memory.clone();
    let _ = thread::spawn(move || {
        let _guard = poison.lock().unwrap();
        panic!("poison the native RAM lock for the injected-failure test");
    })
    .join();
    let mut port = ValidatedPhysicalAccess::new(NativeRamBackend::new(memory, 0, 16));
    let error = port
        .access(PhysicalRequest::data_write(0, AccessWidth::Word, &[1, 2, 3, 4]).unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        PhysicalAccessError::BackendFailure(failure)
            if failure.request.paddr == 0 && failure.request.width == AccessWidth::Word
    ));
}

#[derive(Debug, Clone, Copy)]
enum InjectedFailure {
    Host,
    Protocol,
    Unknown,
    ShortResponse,
}

#[derive(Debug)]
struct InjectedBackend {
    failure: InjectedFailure,
    calls: usize,
}

impl PhysicalBackend for InjectedBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls += 1;
        match self.failure {
            InjectedFailure::Host => Err(PhysicalBackendError::host("injected host failure")),
            InjectedFailure::Protocol => {
                Err(PhysicalBackendError::protocol("injected protocol failure"))
            }
            InjectedFailure::Unknown => {
                Err(PhysicalBackendError::unknown("injected unknown completion"))
            }
            InjectedFailure::ShortResponse => Ok(PhysicalResponse::new(
                request.descriptor(),
                PhysicalCompletion::Read(PhysicalResponseBytes::from_slice(&[0xaa])),
            )),
        }
    }
}

#[test]
fn injected_backend_protocol_host_and_unknown_categories_remain_distinct() {
    for (failure, expected) in [
        (InjectedFailure::Host, "host"),
        (InjectedFailure::Protocol, "protocol"),
        (InjectedFailure::Unknown, "unknown"),
    ] {
        let mut port = ValidatedPhysicalAccess::new(InjectedBackend { failure, calls: 0 });
        let error = port
            .access(PhysicalRequest::data_read(0x100, AccessWidth::Word).unwrap())
            .unwrap_err();
        match (expected, error) {
            ("host", PhysicalAccessError::BackendFailure(_))
            | ("protocol", PhysicalAccessError::Protocol(_))
            | ("unknown", PhysicalAccessError::UnknownCompletion(_)) => {}
            (_, other) => panic!("injected {expected} failure was reclassified: {other:?}"),
        }
        assert_eq!(port.backend().calls, 1);
    }
}

#[test]
fn malformed_native_response_is_protocol_failure_after_one_backend_call() {
    let mut port = ValidatedPhysicalAccess::new(InjectedBackend {
        failure: InjectedFailure::ShortResponse,
        calls: 0,
    });
    let error = port
        .access(PhysicalRequest::data_read(0x100, AccessWidth::Word).unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        PhysicalAccessError::Protocol(
            ruscv_sim::physical::PhysicalProtocolError::ResponseLengthMismatch {
                expected: 4,
                actual: 1
            }
        )
    ));
    assert_eq!(port.backend().calls, 1);
}

#[test]
fn empty_host_inspection_keeps_legacy_semantics_and_does_not_touch_native_targets() {
    let simulator = RiscVSimulator::new(16);
    assert_eq!(simulator.read_mem(u64::MAX, 0).unwrap(), Vec::<u8>::new());
    simulator.write_mem(u64::MAX, &[]).unwrap();
    assert_eq!(simulator.read_mem(0, 0).unwrap(), Vec::<u8>::new());
}

#[test]
fn native_backend_error_context_is_preserved_by_the_validated_port() {
    let request = PhysicalRequest::data_read(0x200, AccessWidth::Byte).unwrap();
    let mut backend = InjectedBackend {
        failure: InjectedFailure::Host,
        calls: 0,
    };
    let result = backend.transact(&request);
    assert!(matches!(
        result,
        Err(PhysicalBackendError::Host { context }) if context == "injected host failure"
    ));

    let mut port = ValidatedPhysicalAccess::new(InjectedBackend {
        failure: InjectedFailure::Unknown,
        calls: 0,
    });
    let error = port.access(request).unwrap_err();
    assert!(matches!(
        error,
        PhysicalAccessError::UnknownCompletion(unknown)
            if unknown.context == "injected unknown completion"
    ));
}
