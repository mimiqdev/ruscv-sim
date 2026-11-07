//! Device-route rows of the TLM-Modeled Devices contract's unit U1.
//!
//! These tests drive one [`SystemBus`] with and without a selected device
//! path, so the native branch and the device target answer the same registers,
//! publish the same one event through the same window sink, and reject the
//! same width, category and start-address shapes.
//!
//! The machine-level sequence equality between the two routes lives in
//! `src/machine/tests.rs`; the in-tree targets' own rows live in
//! `src/blocking/device_tests.rs`.

use super::{SystemBus, SYSTEM_BUS_HTIF_BASE, SYSTEM_BUS_UART_BASE};
use crate::blocking::{DevicePath, InTreeHtif, InTreeUart};
use crate::hart_amods::{self, AmoOperation};
use crate::memory::{MemoryInterface, SimpleMemory};
use crate::peripherals::uart16550::{reg_offset, Uart16550, FIFO_DEPTH};
use crate::physical::{
    AmoWidth, AtomicAccessError, AtomicOrdering, AtomicRequest, AtomicResponse,
    NativeSystemBusBackend, PhysicalBackend, PhysicalBackendResult, PhysicalRequest, PhysicalWidth,
    ValidatedAtomicAccess,
};
use crate::ram::RamSlot;
use std::sync::{Arc, Mutex};

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;
const ORDERING: AtomicOrdering = AtomicOrdering { aq: true, rl: true };

/// Which route the bus is driven on: the native branches or a selected device
/// path over the in-tree targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Native,
    Tlm,
}

struct Harness {
    bus: Arc<Mutex<SystemBus>>,
    transmitted: Arc<Mutex<Vec<u8>>>,
    htif: Arc<Mutex<Vec<u64>>>,
}

fn harness(route: Route) -> Harness {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let transmitted = Arc::new(Mutex::new(Vec::new()));
    let htif = Arc::new(Mutex::new(Vec::new()));
    let devices = match route {
        Route::Native => None,
        Route::Tlm => Some(DevicePath::new(
            Arc::new(InTreeUart::new(uart.clone())),
            Arc::new(InTreeHtif::new()),
        )),
    };
    let mut bus = SystemBus::with_ram_slot(
        RamSlot::native(ram, RAM_BASE, RAM_SIZE),
        uart,
        RAM_BASE,
        RAM_SIZE,
        devices,
    );
    let sink = transmitted.clone();
    bus.set_uart_sink(move |byte| sink.lock().unwrap().push(byte));
    let values = htif.clone();
    bus.set_htif_write_callback(move |value| values.lock().unwrap().push(value));
    Harness {
        bus: Arc::new(Mutex::new(bus)),
        transmitted,
        htif,
    }
}

impl Harness {
    fn raw(&self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.bus.lock().unwrap().transact(request)
    }

    fn read_byte(&self, address: u64) -> PhysicalBackendResult {
        let request = PhysicalRequest::data_read(address, PhysicalWidth::Byte).unwrap();
        self.raw(&request)
    }

    fn write_byte(&self, address: u64, value: u8) -> PhysicalBackendResult {
        let payload = [value];
        let request = PhysicalRequest::data_write(address, PhysicalWidth::Byte, &payload).unwrap();
        self.raw(&request)
    }

    fn read_dword(&self, address: u64) -> PhysicalBackendResult {
        let request = PhysicalRequest::data_read(address, PhysicalWidth::Doubleword).unwrap();
        self.raw(&request)
    }

    fn write_dword(&self, address: u64, value: u64) -> PhysicalBackendResult {
        let payload = value.to_le_bytes();
        let request =
            PhysicalRequest::data_write(address, PhysicalWidth::Doubleword, &payload).unwrap();
        self.raw(&request)
    }

    fn atomic(&self, request: AtomicRequest<'_>) -> Result<AtomicResponse, AtomicAccessError> {
        let mut port = ValidatedAtomicAccess::new(NativeSystemBusBackend::new(self.bus.clone()));
        port.access_atomic(request)
    }

    fn rmw(&self, address: u64, value: u64) -> Result<AtomicResponse, AtomicAccessError> {
        let operand = value.to_le_bytes();
        self.atomic(
            AtomicRequest::rmw(
                address,
                AmoWidth::Doubleword.physical_width(),
                ORDERING,
                &operand,
                hart_amods::transform(AmoOperation::Add, AmoWidth::Doubleword),
            )
            .unwrap(),
        )
    }

    fn transmitted(&self) -> Vec<u8> {
        self.transmitted.lock().unwrap().clone()
    }

    fn htif_values(&self) -> Vec<u64> {
        self.htif.lock().unwrap().clone()
    }

    fn htif_version(&self) -> u64 {
        self.bus.lock().unwrap().htif_committed_write_version()
    }
}

/// The routes under test, in the order the assertions report them.
const ROUTES: [Route; 2] = [Route::Native, Route::Tlm];

#[test]
fn a_raw_uart_write_publishes_one_byte_on_both_routes() {
    for route in ROUTES {
        let harness = harness(route);
        assert!(
            harness.write_byte(SYSTEM_BUS_UART_BASE, b'A').is_ok(),
            "{route:?}: the raw UART write was rejected"
        );
        assert_eq!(
            harness.transmitted(),
            [b'A'],
            "{route:?}: one raw THR write did not publish exactly one byte"
        );
        assert!(
            harness.htif_values().is_empty(),
            "{route:?}: a UART write reached the HTIF sink"
        );
    }
}

#[test]
fn a_full_transmit_fifo_still_publishes_the_seventeenth_byte() {
    for route in ROUTES {
        let harness = harness(route);
        for index in 0..FIFO_DEPTH {
            assert!(harness
                .write_byte(SYSTEM_BUS_UART_BASE, b'a' + index as u8)
                .is_ok());
        }
        assert_eq!(harness.transmitted().len(), FIFO_DEPTH);
        assert!(harness.write_byte(SYSTEM_BUS_UART_BASE, b'Z').is_ok());
        assert_eq!(
            harness.transmitted().len(),
            FIFO_DEPTH + 1,
            "{route:?}: a THR write with a full transmit FIFO published no byte"
        );
        assert_eq!(
            *harness.transmitted().last().unwrap(),
            b'Z',
            "{route:?}: the published byte was not the written one"
        );
    }
}

#[test]
fn the_typed_byte_write_publishes_one_byte_on_both_routes() {
    for route in ROUTES {
        let harness = harness(route);
        harness
            .bus
            .lock()
            .unwrap()
            .write_byte(SYSTEM_BUS_UART_BASE, b'B')
            .expect("the typed byte write was rejected");
        assert_eq!(
            harness.transmitted(),
            [b'B'],
            "{route:?}: the typed byte write did not publish exactly one byte"
        );
        // A typed read is a host view: it publishes nothing.
        let byte = harness
            .bus
            .lock()
            .unwrap()
            .read_byte(SYSTEM_BUS_UART_BASE + reg_offset::LSR)
            .expect("the typed LSR read was rejected");
        assert_ne!(byte & 0x20, 0, "{route:?}: LSR lost THRE");
        assert_eq!(
            harness.transmitted(),
            [b'B'],
            "{route:?}: a typed UART read published a byte"
        );
    }
}

#[test]
fn a_dlab_write_publishes_nothing_on_both_routes() {
    for route in ROUTES {
        let harness = harness(route);
        assert!(harness
            .write_byte(SYSTEM_BUS_UART_BASE + reg_offset::LCR, 0x80)
            .is_ok());
        assert!(harness.write_byte(SYSTEM_BUS_UART_BASE, 0x2a).is_ok());
        assert!(
            harness.transmitted().is_empty(),
            "{route:?}: a divisor-latch write published a transmit event"
        );
    }
}

#[test]
fn the_htif_dword_read_answers_zero_and_its_write_publishes_once() {
    for route in ROUTES {
        let harness = harness(route);
        let response = harness
            .read_dword(SYSTEM_BUS_HTIF_BASE)
            .expect("the HTIF dword read was rejected");
        assert_eq!(
            response.read_bytes(),
            Some(&[0u8; 8][..]),
            "{route:?}: the HTIF dword read did not answer zero"
        );
        assert!(harness.htif_values().is_empty());
        assert!(harness
            .write_dword(SYSTEM_BUS_HTIF_BASE, 0x1122_3344_5566_7788)
            .is_ok());
        assert_eq!(
            harness.htif_values(),
            [0x1122_3344_5566_7788],
            "{route:?}: one HTIF dword write did not deliver exactly one value"
        );
        assert_eq!(
            harness.htif_version(),
            1,
            "{route:?}: the endpoint's committed-write version did not bump once"
        );
    }
}

#[test]
fn the_typed_htif_write_and_the_rmw_envelope_publish_one_value_each() {
    for route in ROUTES {
        let harness = harness(route);
        harness
            .bus
            .lock()
            .unwrap()
            .write_dword(SYSTEM_BUS_HTIF_BASE, 0xaa)
            .expect("the typed HTIF dword write was rejected");
        assert_eq!(harness.htif_values(), [0xaa]);

        let response = harness
            .rmw(SYSTEM_BUS_HTIF_BASE, 5)
            .unwrap_or_else(|error| {
                panic!("{route:?}: the HTIF RMW envelope was rejected: {error:?}")
            });
        assert_eq!(
            response.old_bytes(),
            Some(&[0u8; 8][..]),
            "{route:?}: the envelope's old value was not the endpoint's zero read"
        );
        assert_eq!(
            harness.htif_values(),
            [0xaa, 5],
            "{route:?}: the RMW envelope did not publish its write side exactly once"
        );
        assert_eq!(
            harness.htif_version(),
            2,
            "{route:?}: the envelope's write did not bump the version exactly once"
        );
    }
}

#[test]
fn the_uart_policy_answers_are_the_same_on_both_routes() {
    let mut answers = Vec::new();
    for route in ROUTES {
        let harness = harness(route);
        let halfword_read =
            PhysicalRequest::data_read(SYSTEM_BUS_UART_BASE, PhysicalWidth::Halfword).unwrap();
        let halfword_write =
            PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, PhysicalWidth::Halfword, &[0, 0])
                .unwrap();
        let fetch = PhysicalRequest::fetch(SYSTEM_BUS_UART_BASE, PhysicalWidth::Byte).unwrap();
        answers.push((
            format!("{:?}", harness.raw(&halfword_read)),
            format!("{:?}", harness.raw(&halfword_write)),
            format!("{:?}", harness.raw(&fetch)),
            format!("{:?}", harness.rmw(SYSTEM_BUS_UART_BASE, 1)),
        ));
        assert!(
            harness.transmitted().is_empty(),
            "{route:?}: a rejected UART access published a byte"
        );
    }
    assert_eq!(
        answers[0], answers[1],
        "the UART window's width, fetch and atomic answers diverged between routes"
    );
}

#[test]
fn the_htif_policy_answers_are_the_same_on_both_routes() {
    let mut answers = Vec::new();
    for route in ROUTES {
        let harness = harness(route);
        let offset_read =
            PhysicalRequest::data_read(SYSTEM_BUS_HTIF_BASE + 4, PhysicalWidth::Doubleword)
                .unwrap();
        let halfword =
            PhysicalRequest::data_read(SYSTEM_BUS_HTIF_BASE, PhysicalWidth::Halfword).unwrap();
        let fetch =
            PhysicalRequest::fetch(SYSTEM_BUS_HTIF_BASE, PhysicalWidth::Doubleword).unwrap();
        let load_reserved =
            AtomicRequest::load_reserved(SYSTEM_BUS_HTIF_BASE, PhysicalWidth::Doubleword, ORDERING)
                .unwrap();
        let store_conditional = AtomicRequest::store_conditional(
            SYSTEM_BUS_HTIF_BASE,
            PhysicalWidth::Doubleword,
            ORDERING,
            &[1, 0, 0, 0, 0, 0, 0, 0],
            None,
        )
        .unwrap();
        answers.push((
            format!("{:?}", harness.raw(&offset_read)),
            format!("{:?}", harness.raw(&halfword)),
            format!("{:?}", harness.raw(&fetch)),
            format!("{:?}", harness.atomic(load_reserved)),
            format!("{:?}", harness.atomic(store_conditional)),
            format!("{:?}", harness.rmw(SYSTEM_BUS_HTIF_BASE + 4, 1)),
        ));
        assert!(
            harness.htif_values().is_empty(),
            "{route:?}: a rejected HTIF access published a value"
        );
    }
    assert_eq!(
        answers[0], answers[1],
        "the HTIF endpoint's start-address, width, fetch and atomic answers diverged between routes"
    );
}

#[test]
fn a_rejected_access_leaves_the_uart_register_file_untouched_on_both_routes() {
    for route in ROUTES {
        let harness = harness(route);
        let halfword_write = PhysicalRequest::data_write(
            SYSTEM_BUS_UART_BASE + reg_offset::SCR,
            PhysicalWidth::Halfword,
            &[7, 7],
        )
        .unwrap();
        assert!(harness.raw(&halfword_write).is_err());
        let scratch = harness
            .read_byte(SYSTEM_BUS_UART_BASE + reg_offset::SCR)
            .expect("the SCR read was rejected");
        assert_eq!(
            scratch.read_bytes(),
            Some(&[0u8][..]),
            "{route:?}: a rejected halfword write reached the UART registers"
        );
        assert!(harness.transmitted().is_empty());
    }
}
