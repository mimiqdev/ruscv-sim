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
use crate::blocking::test_support::HostUart;
use crate::blocking::{DevicePath, InTreeHtif, InTreeUart};
use crate::hart_amods::{self, AmoOperation};
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
use crate::peripherals::uart16550::{lsr_bits, reg_offset, Uart16550, FIFO_DEPTH};
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

/// device D5: a host-served UART window has no Rust register state, so the
/// typed `MemoryInterface` view refuses a read and a write instead of reading
/// or mutating a stale object, while the raw device path still reaches the
/// selected target.
#[test]
fn a_host_served_uart_window_refuses_the_typed_view() {
    let target = HostUart::new(SYSTEM_BUS_UART_BASE);
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let mut bus = SystemBus::with_ram_slot(
        RamSlot::native(ram, RAM_BASE, RAM_SIZE),
        uart,
        RAM_BASE,
        RAM_SIZE,
        Some(super::DevicePath::with_host_uart(
            target.clone(),
            Arc::new(InTreeHtif::new()),
        )),
    );
    let transmitted = Arc::new(Mutex::new(Vec::new()));
    let sink = transmitted.clone();
    bus.set_uart_sink(move |byte| sink.lock().unwrap().push(byte));

    let lsr = SYSTEM_BUS_UART_BASE + reg_offset::LSR;
    assert!(
        matches!(bus.read_byte(lsr), Err(MemoryError::InvalidAddress(address)) if address == lsr),
        "a typed read of a host-served UART window was not refused"
    );
    assert!(
        matches!(
            bus.write_byte(SYSTEM_BUS_UART_BASE, b'X'),
            Err(MemoryError::InvalidAddress(address)) if address == SYSTEM_BUS_UART_BASE
        ),
        "a typed write of a host-served UART window was not refused"
    );
    assert_eq!(
        target.transactions(),
        0,
        "the typed view reached the target instead of refusing"
    );
    assert!(
        transmitted.lock().unwrap().is_empty(),
        "a refused typed write published a byte"
    );

    // The window is still served: the raw device path is one transaction to
    // the selected target, and its transmitted byte reaches the same sink.
    let payload = *b"H";
    let request =
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, PhysicalWidth::Byte, &payload).unwrap();
    assert!(
        bus.transact(&request).is_ok(),
        "the raw device path did not reach the host target"
    );
    assert_eq!(
        target.transactions(),
        1,
        "the raw UART write did not reach the host target exactly once"
    );
    assert_eq!(
        *transmitted.lock().unwrap(),
        [b'H'],
        "the host target's transmit byte did not reach the window sink"
    );
}

/// device D5: the typed view refuses only a host-served window.  An in-tree TLM
/// window keeps today's register-file read.
#[test]
fn an_in_tree_uart_window_keeps_the_typed_view() {
    let harness = harness(Route::Tlm);
    let lsr = harness
        .bus
        .lock()
        .unwrap()
        .read_byte(SYSTEM_BUS_UART_BASE + reg_offset::LSR)
        .ok();
    assert_eq!(
        lsr,
        Some(lsr_bits::THRE | lsr_bits::TEMT),
        "an in-tree UART window lost its typed register read"
    );
}

// ---- Device Delay Annotation U1: charged device delay (delay D3–D5) ----

/// One Platform tick in exact femtoseconds, the declared default period.
const ONE_TICK_FS: u64 = 1_000_000;

/// One device-delay bus's shared bus, its RAM slot ledger and the bytes the
/// window sink published.
type DeviceDelayBus = (Arc<Mutex<SystemBus>>, RamSlot, Arc<Mutex<Vec<u8>>>);

/// A bus over a TLM RAM slot that declares one Platform tick per transaction,
/// a selected host UART target and the slot handle its turn ledger is read
/// through.
///
/// The slot declares at least the charged tick, so the host target's annotation
/// is admitted instead of failing as an overshoot (delay D3).
fn device_delay_bus(delay: u64) -> DeviceDelayBus {
    let ram = RamSlot::tlm(
        crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
        RAM_BASE,
        RAM_SIZE,
        crate::blocking::TickPeriod::DEFAULT,
        false,
        false,
    )
    .expect("the declaring RAM target was refused");
    let target = HostUart::new(SYSTEM_BUS_UART_BASE);
    target.set_delay(delay);
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let mut bus = SystemBus::with_ram_slot(
        ram.clone(),
        uart,
        RAM_BASE,
        RAM_SIZE,
        Some(DevicePath::with_host_uart(
            target,
            Arc::new(InTreeHtif::new()),
        )),
    );
    let transmitted = Arc::new(Mutex::new(Vec::new()));
    let sink = transmitted.clone();
    bus.set_uart_sink(move |byte| sink.lock().unwrap().push(byte));
    (Arc::new(Mutex::new(bus)), ram, transmitted)
}

/// delay D4: a Hart device response's delay is charged through the guest-RAM
/// slot's one turn ledger before the window's event is published, and an
/// in-tree device access on the same bus charges nothing.
#[test]
fn a_hart_device_delay_is_charged_through_the_ram_slots_turn_ledger() {
    let (bus, ledger, transmitted) = device_delay_bus(ONE_TICK_FS);
    ledger.begin_transport_turn();
    let payload = *b"H";
    let request =
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, PhysicalWidth::Byte, &payload).unwrap();
    let response = bus.lock().unwrap().transact(&request);
    assert!(
        response.is_ok(),
        "the Hart device write was refused: {response:?}"
    );
    assert_eq!(
        *transmitted.lock().unwrap(),
        [b'H'],
        "the charged access did not publish its one event"
    );
    let pending = ledger.take_transport_turn_delay();
    assert_eq!(
        pending.ticks, 1,
        "the device delay did not enter the turn's pending amount"
    );
    assert_eq!(
        pending.femtoseconds.as_u64(),
        ONE_TICK_FS,
        "the turn's pending amount is not the response's exact delay"
    );
    assert_eq!(
        ledger.consumed_delay().as_u64(),
        ONE_TICK_FS,
        "the device delay did not enter the generation's transport total exactly once"
    );
    assert_eq!(
        ledger.host_elapsed().as_u64(),
        0,
        "a device access reported host elapsed time"
    );

    // The in-tree window on the same slot reports zero, so it charges nothing.
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let in_tree_ledger = RamSlot::tlm(
        crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
        RAM_BASE,
        RAM_SIZE,
        crate::blocking::TickPeriod::DEFAULT,
        false,
        false,
    )
    .expect("the declaring RAM target was refused");
    let mut in_tree = SystemBus::with_ram_slot(
        in_tree_ledger.clone(),
        uart.clone(),
        RAM_BASE,
        RAM_SIZE,
        Some(DevicePath::new(
            Arc::new(InTreeUart::new(uart)),
            Arc::new(InTreeHtif::new()),
        )),
    );
    let published = Arc::new(Mutex::new(Vec::new()));
    let sink = published.clone();
    in_tree.set_uart_sink(move |byte| sink.lock().unwrap().push(byte));
    in_tree_ledger.begin_transport_turn();
    assert!(
        in_tree.transact(&request).is_ok(),
        "the in-tree UART write was refused"
    );
    assert_eq!(
        in_tree_ledger.take_transport_turn_delay().ticks,
        0,
        "an in-tree device access charged the turn's pending amount"
    );
    assert_eq!(
        in_tree_ledger.consumed_delay().as_u64(),
        0,
        "an in-tree device access entered the transport total"
    );
}

/// delay D3/D4: a device delay above the declared maximum is the overshoot host
/// failure, which is returned before the window publishes its event, and the
/// known amount is still accumulated.
#[test]
fn an_above_maximum_device_delay_publishes_nothing_and_keeps_the_known_charge() {
    let (bus, ledger, transmitted) = device_delay_bus(2 * ONE_TICK_FS);
    ledger.begin_transport_turn();
    let payload = *b"H";
    let request =
        PhysicalRequest::data_write(SYSTEM_BUS_UART_BASE, PhysicalWidth::Byte, &payload).unwrap();
    match bus.lock().unwrap().transact(&request) {
        Err(crate::physical::PhysicalBackendError::Host { context }) => {
            assert!(
                context.contains("overshoot")
                    && context.contains("declared per-transaction maximum"),
                "the above-maximum delay did not use the overshoot diagnostic: {context}"
            );
        }
        other => panic!("an above-maximum device delay was admitted: {other:?}"),
    }
    assert!(
        transmitted.lock().unwrap().is_empty(),
        "an overshooting device access still published its event"
    );
    let pending = ledger.take_transport_turn_delay();
    assert_eq!(
        pending.ticks, 2,
        "the failed access did not keep the delay the adapter accumulated"
    );
    assert_eq!(
        ledger.consumed_delay().as_u64(),
        2 * ONE_TICK_FS,
        "the failed access did not preserve the known charge"
    );
}

// ---- Host HTIF Service U1: typed view and run stop (htif D3–D5) ----

use super::observation_fixture as elf;
use crate::blocking::test_support::{HostHtif, HostHtifFault};
use crate::blocking::{DeviceTargets, Transport, ZeroDelayRam};
use crate::machine::{MachineConfig, PlatformKind};

/// A `Write` sink that keeps a run's commit log in memory.
#[derive(Clone)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One TLM-route configuration with the given device selection.
fn tlm_config(devices: DeviceTargets) -> MachineConfig {
    tlm_config_with_ram(Arc::new(ZeroDelayRam::new()), devices)
}

/// The same configuration over an explicit guest-RAM target (delay D3).
fn tlm_config_with_ram(
    ram: Arc<dyn crate::blocking::BlockingTarget>,
    devices: DeviceTargets,
) -> MachineConfig {
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(ram);
    config.devices = devices;
    config
}

/// One run of `code` under `config`, with its commit log captured in memory.
fn run_with_log(
    config: MachineConfig,
    code: &[u32],
    bound: u64,
) -> (super::ExecutionResult, String) {
    let bytes = elf::elf_with_code(code, 0, true, false, 0x4000);
    let log = Arc::new(Mutex::new(Vec::new()));
    let logger = super::CommitLogger::for_test(Box::new(CapturedLog(log.clone())));
    let hooks = super::NativeHooks {
        config: Some(config),
        ..super::NativeHooks::default()
    };
    let result =
        super::run_native_inner(&bytes, Some(bound), None, None, false, Some(logger), hooks)
            .expect("the run completed");
    let text = String::from_utf8(log.lock().unwrap().clone()).expect("the commit log is UTF-8");
    (result, text)
}

/// A guest that writes `value` to the endpoint and then self-loops, so a run
/// with no exit fact reaches its turn bound.
fn endpoint_exit_guest(value: u32) -> Vec<u32> {
    vec![
        elf::lui(8, 0x40008), // 0x4000_8000, the HTIF endpoint
        elf::addi(9, 0, value as i32),
        elf::sd(9, 8, 0),
        elf::beq(0, 0, 0), // self-loop
    ]
}

/// htif D3: on the HTIF window the typed view is unchanged, so a typed dword
/// read still answers zero even when the host target's own read state is not,
/// and a typed dword write still publishes through the sink without reaching
/// the target.  The raw device path still reaches it.
#[test]
fn a_host_htif_window_keeps_the_typed_read_and_publishes_its_typed_write() {
    let target = HostHtif::new(SYSTEM_BUS_HTIF_BASE);
    target.set_read_value(0x77);
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let mut bus = SystemBus::with_ram_slot(
        RamSlot::native(ram, RAM_BASE, RAM_SIZE),
        uart.clone(),
        RAM_BASE,
        RAM_SIZE,
        Some(DevicePath::new(
            Arc::new(InTreeUart::new(uart)),
            target.clone(),
        )),
    );
    let values = Arc::new(Mutex::new(Vec::new()));
    let sink = values.clone();
    bus.set_htif_write_callback(move |value| sink.lock().unwrap().push(value));

    assert_eq!(
        bus.read_dword(SYSTEM_BUS_HTIF_BASE)
            .expect("the typed tohost read was refused"),
        0,
        "the typed tohost read answered the host target's state instead of zero"
    );
    assert_eq!(
        target.transactions(),
        0,
        "the typed tohost read reached the host target"
    );
    bus.write_dword(SYSTEM_BUS_HTIF_BASE, 0xaa)
        .expect("the typed tohost write was refused");
    assert_eq!(
        *values.lock().unwrap(),
        [0xaa],
        "the typed tohost write did not publish exactly one value"
    );
    assert_eq!(
        bus.htif_committed_write_version(),
        1,
        "the typed tohost write did not bump the endpoint's version once"
    );
    assert_eq!(
        target.transactions(),
        0,
        "the typed tohost write reached the host target"
    );

    // The raw device path is still the guest's: one read of the target's own
    // state and one write with its own event.
    let read = PhysicalRequest::data_read(SYSTEM_BUS_HTIF_BASE, PhysicalWidth::Doubleword).unwrap();
    let response = bus
        .transact(&read)
        .expect("the raw tohost read was rejected");
    assert_eq!(
        response.read_bytes(),
        Some(&0x77u64.to_le_bytes()[..]),
        "the raw tohost read did not answer the host target's state"
    );
    let payload = 0xbbu64.to_le_bytes();
    let write =
        PhysicalRequest::data_write(SYSTEM_BUS_HTIF_BASE, PhysicalWidth::Doubleword, &payload)
            .unwrap();
    assert!(
        bus.transact(&write).is_ok(),
        "the raw tohost write was rejected"
    );
    assert_eq!(
        target.transactions(),
        2,
        "the raw device path did not reach the host target exactly twice"
    );
    assert_eq!(
        *values.lock().unwrap(),
        [0xaa, 0xbb],
        "the raw tohost write did not publish its own value after the typed one"
    );
}

/// htif D5: a host-served tohost write that carries an exit word stops the run
/// where the in-tree endpoint stops it, with the same exit code, the same
/// retired instructions, the same final PC and a byte-identical commit log.
#[test]
fn a_host_htif_exit_stops_the_run_like_the_in_tree_endpoint() {
    let code = endpoint_exit_guest(1); // (0 << 1) | 1, the exit word for code 0
    let bound = 64;
    let (in_tree, in_tree_log) = run_with_log(tlm_config(DeviceTargets::none()), &code, bound);

    let target = HostHtif::new(SYSTEM_BUS_HTIF_BASE);
    let (host, host_log) = run_with_log(
        tlm_config(DeviceTargets::with_htif(target.clone())),
        &code,
        bound,
    );

    assert_eq!(
        in_tree.exit_code, 0,
        "the in-tree endpoint did not stop the run as an exit: {in_tree:?}"
    );
    assert_eq!(
        (host.exit_code, host.cycles, host.final_pc),
        (in_tree.exit_code, in_tree.cycles, in_tree.final_pc),
        "the host-served exit stopped at a different code, turn or PC"
    );
    assert_eq!(
        host.cycles, 3,
        "the host-served exit did not stop at the retired store's turn"
    );
    assert_eq!(
        host_log, in_tree_log,
        "the host-served exit's commit log is not byte-identical to the in-tree one"
    );
    assert!(!host.timed_out && host.error.is_none(), "{host:?}");
    assert_eq!(
        target.transactions(),
        1,
        "the host HTIF target did not serve exactly the one tohost write"
    );
    assert_eq!(
        target.reported(),
        [1],
        "the host target did not report the guest's exit word"
    );
}

/// htif D5: a host that accepts the exit write without reporting its event
/// suppresses the exit, because the Platform then has no exit fact.  The run
/// reaches the turn bound a guest with no exit reaches.
#[test]
fn a_host_htif_that_omits_the_event_reaches_the_turn_bound() {
    let bound = 32;
    let exit_code = endpoint_exit_guest(1);

    // The in-tree endpoint stops the run at the store.
    let (in_tree, _) = run_with_log(tlm_config(DeviceTargets::none()), &exit_code, bound);
    assert_eq!((in_tree.exit_code, in_tree.cycles), (0, 3));
    assert!(!in_tree.timed_out);

    let target = HostHtif::new(SYSTEM_BUS_HTIF_BASE);
    target.set_fault(HostHtifFault::DropEvent);
    let (dropped, _) = run_with_log(
        tlm_config(DeviceTargets::with_htif(target.clone())),
        &exit_code,
        bound,
    );
    assert!(
        dropped.timed_out,
        "a host that omitted the event stopped the run as an exit: {dropped:?}"
    );
    assert_eq!(
        dropped.cycles, bound,
        "the run without an exit fact did not reach its turn bound"
    );
    assert!(
        dropped
            .error
            .as_deref()
            .is_some_and(|error| error.contains(&format!("Timeout after {bound} cycles"))),
        "the run without an exit fact did not fail with the turn-limit diagnostic: {dropped:?}"
    );
    assert_eq!(
        target.transactions(),
        1,
        "the host HTIF target did not accept the write whose event it omitted"
    );

    // The bound is the one a guest with no exit reaches: the same guest with a
    // value that carries no exit command.
    let (no_exit, _) = run_with_log(
        tlm_config(DeviceTargets::none()),
        &endpoint_exit_guest(2),
        bound,
    );
    assert!(no_exit.timed_out && no_exit.error.is_some(), "{no_exit:?}");
    assert_eq!(
        dropped.cycles, no_exit.cycles,
        "the omitted-event run ended at a different turn than a guest with no exit"
    );
}

/// delay D5: a host UART access that annotates one tick keeps the run's facts:
/// the same exit code, retired instructions, final PC and a byte-identical
/// commit log as the in-tree window's run.  A host target that annotates zero
/// leaves every one of those facts unchanged too (delay D2).
#[test]
fn a_host_uart_device_delay_keeps_the_run_facts() {
    // One THR byte, then the exit word through the in-tree HTIF endpoint.
    let code = vec![
        elf::lui(5, 0x10000), // 0x1000_0000, the UART window
        elf::addi(6, 0, i32::from(b'H')),
        elf::sb(6, 5, 0),     // THR
        elf::lui(8, 0x40008), // 0x4000_8000, the HTIF endpoint
        elf::addi(9, 0, 1),   // (0 << 1) | 1, the exit word for code 0
        elf::sd(9, 8, 0),
    ];
    let bound = 64;
    let (in_tree, in_tree_log) = run_with_log(
        tlm_config_with_ram(
            crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
            DeviceTargets::none(),
        ),
        &code,
        bound,
    );
    assert_eq!(
        in_tree.exit_code, 0,
        "the in-tree run did not reach the guest's exit: {in_tree:?}"
    );

    for delay in [0, ONE_TICK_FS] {
        let target = HostUart::new(SYSTEM_BUS_UART_BASE);
        target.set_delay(delay);
        let (host, host_log) = run_with_log(
            tlm_config_with_ram(
                crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
                DeviceTargets::with_uart(target.clone()),
            ),
            &code,
            bound,
        );
        assert_eq!(
            (host.exit_code, host.cycles, host.final_pc),
            (in_tree.exit_code, in_tree.cycles, in_tree.final_pc),
            "a {delay} fs device annotation moved the run's exit, turns or final PC"
        );
        assert_eq!(
            host_log, in_tree_log,
            "a {delay} fs device annotation changed the commit log"
        );
        assert!(!host.timed_out && host.error.is_none(), "{host:?}");
        assert_eq!(
            target.transactions(),
            1,
            "the host UART target did not serve exactly the one THR write"
        );
        assert_eq!(
            target.transmitted(),
            b"H".to_vec(),
            "the target lost the byte"
        );
    }
}

/// delay D4/D5: a host HTIF exit write that annotates one tick is charged in the
/// same turn whose event decodes the exit, so the run stops at the same code,
/// turn, PC and byte-identical commit log.
#[test]
fn a_host_htif_device_delay_keeps_the_exit_decoded_from_its_own_turn() {
    let code = endpoint_exit_guest(1); // (0 << 1) | 1, the exit word for code 0
    let bound = 64;
    let (in_tree, in_tree_log) = run_with_log(
        tlm_config_with_ram(
            crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
            DeviceTargets::none(),
        ),
        &code,
        bound,
    );

    let target = HostHtif::new(SYSTEM_BUS_HTIF_BASE);
    target.set_delay(ONE_TICK_FS);
    let (host, host_log) = run_with_log(
        tlm_config_with_ram(
            crate::blocking::test_support::DelayingRam::new(ONE_TICK_FS, 0),
            DeviceTargets::with_htif(target.clone()),
        ),
        &code,
        bound,
    );

    assert_eq!(
        in_tree.exit_code, 0,
        "the in-tree endpoint did not stop the run as an exit: {in_tree:?}"
    );
    assert_eq!(
        (host.exit_code, host.cycles, host.final_pc),
        (in_tree.exit_code, in_tree.cycles, in_tree.final_pc),
        "the charged exit write stopped at a different code, turn or PC"
    );
    assert_eq!(
        host_log, in_tree_log,
        "the charged exit write changed the commit log"
    );
    assert_eq!(
        target.reported(),
        [1],
        "the host target did not report the guest's exit word"
    );
}
