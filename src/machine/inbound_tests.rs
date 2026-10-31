//! U2: the Platform inbound path for DM system bus access.
//!
//! `docs/dev-plan.md` §4 U2, with the 2026-10-30 ADR-0002 §9 amendment and
//! §3 ownership. Each test names one contract sentence. RAM/MMIO routing and
//! reservation visibility are checked at the Platform boundary; the DM
//! boundary checks drive `sbcs`/`sbaddress`/`sbdata` through
//! [`DebugModule`] and hand the started request to the same Platform port.
//! Machine admission (U3) is not part of this unit.

use super::{InboundPort, SbaAccessError};
use crate::core::{RiscvCore, SharedPhysicalAccess};
use crate::dm::{DebugModule, SbaAccessSize, SbaFault, SbaRequest};
use crate::elf::LoadImage;
use crate::executor::observation_fixture as elf;
use crate::executor::{SystemBus, SYSTEM_BUS_CLINT_BASE, SYSTEM_BUS_UART_BASE};
use crate::machine::platform::Platform;
use crate::machine::{MachineConfig, OwnedMachine, PlatformEvent, PlatformKind};
use crate::memory::{MemoryInterface, SimpleMemory};
use crate::peripherals::Uart16550;
use crate::physical::{
    AmoTransform, AmoWidth, AtomicAccess, AtomicAccessResult, AtomicOrdering, AtomicRequest,
    ConditionalStatus, NativeRamBackend, NativeSystemBusBackend, PhysicalAccess,
    PhysicalAccessResult, PhysicalBackend, PhysicalBackendError, PhysicalBackendResult,
    PhysicalDataAccess, PhysicalRequest, PhysicalResponse, PhysicalWidth, ValidatedPhysicalAccess,
};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::Duration;

/// Debug Spec 1.0 §3.14 register addresses.
const DMCONTROL: u8 = 0x10;
const SBCS: u8 = 0x38;
const SBADDRESS0: u8 = 0x39;
const SBADDRESS1: u8 = 0x3a;
const SBDATA0: u8 = 0x3c;
const SBDATA1: u8 = 0x3d;

const DMACTIVE: u32 = 1;
const SBBUSY: u32 = 1 << 21;
const SBREADONADDR: u32 = 1 << 20;
const SBERROR_SHIFT: u32 = 12;

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;
/// A RAM offset outside the placed image and the `.tohost` section.
const RAM_OFFSET: u64 = 0x2000;

const ORDERING: AtomicOrdering = AtomicOrdering {
    aq: false,
    rl: false,
};

fn read(address: u64, size: SbaAccessSize) -> SbaRequest {
    SbaRequest {
        address,
        size,
        write: None,
    }
}

fn write(address: u64, size: SbaAccessSize, value: u64) -> SbaRequest {
    SbaRequest {
        address,
        size,
        write: Some(value),
    }
}

fn bad_address() -> SbaAccessError {
    SbaAccessError::Target(SbaFault::BadAddress)
}

/// The four selected `sbaccess` widths with one distinct value each.
const WIDTHS: [(SbaAccessSize, u64, u64); 4] = [
    (SbaAccessSize::Byte, 0x2000, 0xab),
    (SbaAccessSize::Half, 0x2002, 0xbeef),
    (SbaAccessSize::Word, 0x2004, 0xdead_beef),
    (SbaAccessSize::DoubleWord, 0x2008, 0x0123_4567_89ab_cdef),
];

fn ram_inbound() -> (Arc<Mutex<SimpleMemory>>, InboundPort) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let port: SharedPhysicalAccess = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), RAM_BASE, RAM_SIZE),
    )));
    (ram, InboundPort::new(port))
}

/// One native bus behind its own validated inbound port: the backend split
/// the Platform composes over the bus and its devices.
struct Native {
    ram: Arc<Mutex<SimpleMemory>>,
    uart: Arc<Mutex<Uart16550>>,
    bus: Arc<Mutex<SystemBus>>,
    inbound: InboundPort,
}

fn native() -> Native {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart.clone(),
        RAM_BASE,
        RAM_SIZE,
    )));
    let port: SharedPhysicalAccess = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    Native {
        ram,
        uart,
        bus,
        inbound: InboundPort::new(port),
    }
}

/// A real Platform over the fixture image. Flat addresses are storage
/// offsets; native addresses are bus addresses (contract §3 routing).
fn platform(kind: PlatformKind) -> Platform {
    let image = LoadImage::parse(&elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4000))
        .expect("fixture ELF");
    Platform::build(&MachineConfig::new(kind), &image).expect("Platform")
}

/// The physical address of one RAM offset in this Platform's address form.
fn platform_address(kind: PlatformKind, offset: u64) -> u64 {
    match kind {
        PlatformKind::Native => RAM_BASE + offset,
        PlatformKind::Flat => offset,
    }
}

fn assert_raw_widths(inbound: &InboundPort, ram: &Arc<Mutex<SimpleMemory>>) {
    for (size, offset, value) in WIDTHS {
        let address = RAM_BASE + offset;
        let len = usize::try_from(size.bytes()).expect("1/2/4/8-byte width");
        assert_eq!(
            inbound.access(write(address, size, value)),
            Ok(0),
            "{size:?} write"
        );
        assert_eq!(
            ram.lock().unwrap().read_bytes(offset, len).unwrap(),
            value.to_le_bytes()[..len].to_vec(),
            "{size:?} commits the low little-endian bytes"
        );
        assert_eq!(
            inbound.access(read(address, size)),
            Ok(value),
            "{size:?} read returns the raw value"
        );
    }
}

#[test]
fn inbound_ram_accesses_transfer_raw_bytes_at_every_width() {
    let (ram, inbound) = ram_inbound();
    assert_raw_widths(&inbound, &ram);
}

#[test]
fn native_bus_ram_accesses_use_the_same_raw_widths() {
    let native = native();
    assert_raw_widths(&native.inbound, &native.ram);
}

/// U2 §4: an MMIO read performs the device's defined successful-read side
/// effect; there is no side-effect-free peek.
#[test]
fn native_mmio_accesses_keep_their_device_side_effects() {
    let native = native();
    native.uart.lock().unwrap().receive_byte(0x41);
    assert_eq!(
        native
            .inbound
            .access(read(SYSTEM_BUS_UART_BASE, SbaAccessSize::Byte)),
        Ok(0x41),
        "an SBA read of RBR returns the queued byte"
    );
    assert!(
        native.uart.lock().unwrap().inspect().rx_fifo.is_empty(),
        "the successful read popped the receive FIFO; it was not a peek"
    );
    assert_eq!(
        native
            .inbound
            .access(read(SYSTEM_BUS_UART_BASE, SbaAccessSize::Byte)),
        Ok(0),
        "the byte is consumed once"
    );

    assert_eq!(
        native
            .inbound
            .access(write(SYSTEM_BUS_UART_BASE, SbaAccessSize::Byte, 0x5a)),
        Ok(0)
    );
    assert_eq!(
        native.uart.lock().unwrap().inspect().tx_fifo,
        vec![0x5a],
        "the SBA write reached the transmitter"
    );

    // `mtime` is a CLINT register, not RAM. Its 4- and 8-byte accesses are the
    // device's own read-modify-write halves, and both widths are routed.
    let mtime = SYSTEM_BUS_CLINT_BASE + 0xbff8;
    assert_eq!(
        native
            .inbound
            .access(write(mtime, SbaAccessSize::Word, 0x89ab_cdef)),
        Ok(0)
    );
    assert_eq!(
        native.bus.lock().unwrap().mtime() & 0xffff_ffff,
        0x89ab_cdef
    );
    assert_eq!(
        native.inbound.access(write(
            mtime,
            SbaAccessSize::DoubleWord,
            0x1122_3344_5566_7788
        )),
        Ok(0)
    );
    assert_eq!(native.bus.lock().unwrap().mtime(), 0x1122_3344_5566_7788);
    assert_eq!(
        native
            .inbound
            .access(read(mtime, SbaAccessSize::DoubleWord)),
        Ok(0x1122_3344_5566_7788)
    );
    assert_eq!(
        native.inbound.access(read(mtime + 4, SbaAccessSize::Word)),
        Ok(0x1122_3344)
    );
}

/// U2 §4 and ADR-0002 §5: all-or-nothing. A refused span leaves RAM and the
/// target device unchanged.
#[test]
fn refused_spans_are_bad_address_and_commit_nothing() {
    let (ram, inbound) = ram_inbound();
    let tail = u64::try_from(RAM_SIZE).unwrap() - 8;
    ram.lock().unwrap().write_bytes(tail, &[0x77; 8]).unwrap();

    let crossing = RAM_BASE + u64::try_from(RAM_SIZE).unwrap() - 4;
    assert_eq!(
        inbound.access(write(crossing, SbaAccessSize::DoubleWord, 0)),
        Err(bad_address()),
        "a span crossing the end of RAM is a target refusal"
    );
    assert_eq!(
        ram.lock().unwrap().read_bytes(tail, 8).unwrap(),
        vec![0x77; 8],
        "a refused write commits no byte prefix"
    );
    assert_eq!(
        inbound.access(read(RAM_BASE - 8, SbaAccessSize::DoubleWord)),
        Err(bad_address()),
        "a span below the RAM window is a target refusal"
    );

    let native = native();
    assert_eq!(
        native
            .inbound
            .access(write(SYSTEM_BUS_UART_BASE, SbaAccessSize::DoubleWord, 0x5a)),
        Err(bad_address()),
        "the byte-only UART rejects an eight-byte span"
    );
    let inspection = native.uart.lock().unwrap().inspect();
    assert!(inspection.tx_fifo.is_empty(), "no partial device update");
    assert_eq!(
        inspection.registers[0], 0,
        "the refused write did not reach an IER/THR register"
    );

    // A refused read also performs no device read side effect: a queued byte
    // stays queued, so the debugger cannot lose data through a failed access.
    native.uart.lock().unwrap().receive_byte(0x41);
    assert_eq!(
        native
            .inbound
            .access(read(SYSTEM_BUS_UART_BASE, SbaAccessSize::Word)),
        Err(bad_address()),
        "the byte-only UART rejects a four-byte read"
    );
    assert_eq!(
        native.uart.lock().unwrap().inspect().rx_fifo,
        vec![0x41],
        "the refused read consumed no receive byte"
    );
}

/// Simulator-side failures keep their existing classification. None of them
/// may become a target fault, a guest trap, or a fabricated `sberror`.
#[test]
fn non_target_failures_stay_simulator_side() {
    enum Backend {
        Host,
        Unknown,
        Malformed,
    }
    impl PhysicalBackend for Backend {
        fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
            match self {
                Self::Host => Err(PhysicalBackendError::host("host backend refused")),
                Self::Unknown => Err(PhysicalBackendError::unknown("write may have landed")),
                // Two bytes for a word request: the validated port rejects it.
                Self::Malformed => Ok(PhysicalResponse::read_for(request, &[0x00, 0x01])),
            }
        }
    }

    for (backend, expected) in [
        (
            Backend::Host,
            SbaAccessError::Simulator("host backend refused".into()),
        ),
        (
            Backend::Unknown,
            SbaAccessError::UnknownCompletion("write may have landed".into()),
        ),
        (
            Backend::Malformed,
            SbaAccessError::Simulator("physical response has 2 bytes, expected 4".into()),
        ),
    ] {
        let port: SharedPhysicalAccess =
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(backend)));
        let inbound = InboundPort::new(port);
        assert_eq!(
            inbound.access(read(RAM_BASE, SbaAccessSize::Word)),
            Err(expected)
        );
    }
}

fn load_reserved(
    port: &mut dyn PhysicalDataAccess,
    address: u64,
    width: PhysicalWidth,
) -> crate::physical::AtomicReservationContext {
    let request = AtomicRequest::load_reserved(address, width, ORDERING).unwrap();
    let response = port.access_atomic(request).unwrap();
    crate::physical::AtomicReservationContext {
        reserved: request.span(),
        snapshot: response
            .snapshot()
            .expect("a conforming LR carries a committed-write snapshot"),
    }
}

fn store_conditional(
    port: &mut dyn PhysicalDataAccess,
    address: u64,
    width: PhysicalWidth,
    payload: &[u8],
    context: crate::physical::AtomicReservationContext,
) -> ConditionalStatus {
    let request = AtomicRequest::store_conditional(address, width, ORDERING, payload, context)
        .expect("valid SC envelope");
    port.access_atomic(request)
        .expect("SC completes")
        .conditional_status()
        .expect("SC carries a conditional status")
}

/// U2 §4 and ADR-0002 §9: a committed SBA write is a competing write for the
/// Hart's reservation, a non-overlapping or refused one is not. The Hart-side
/// envelope runs through the Platform data port, which is the object the Hart
/// itself uses; architectural SC result evaluation is A8-proven.
#[test]
fn committed_sba_write_invalidates_an_overlapping_hart_reservation() {
    for kind in [PlatformKind::Flat, PlatformKind::Native] {
        let platform = platform(kind);
        let address = platform_address(kind, RAM_OFFSET);
        let offset = RAM_OFFSET;

        let mut hart = platform.data.lock().unwrap();
        let context = load_reserved(&mut *hart, address, PhysicalWidth::Word);
        drop(hart);
        // A committed write to a different word does not invalidate it: the
        // bookkeeping is per covered byte, not a global writer counter.
        assert_eq!(
            platform.sba_access(write(address + 8, SbaAccessSize::Word, 0x1111)),
            Ok(0),
            "{kind:?} non-overlapping SBA write"
        );
        let mut hart = platform.data.lock().unwrap();
        assert_eq!(
            store_conditional(
                &mut *hart,
                address,
                PhysicalWidth::Word,
                &0x2222_u32.to_le_bytes(),
                context
            ),
            ConditionalStatus::Success,
            "{kind:?}: a non-overlapping SBA write leaves the reservation valid"
        );
        drop(hart);

        let mut hart = platform.data.lock().unwrap();
        let context = load_reserved(&mut *hart, address, PhysicalWidth::Word);
        drop(hart);
        // A halving overlap still commits bytes inside the reserved span.
        assert_eq!(
            platform.sba_access(write(address + 2, SbaAccessSize::Half, 0x3333)),
            Ok(0),
            "{kind:?} overlapping SBA write"
        );
        let mut hart = platform.data.lock().unwrap();
        assert_eq!(
            store_conditional(
                &mut *hart,
                address,
                PhysicalWidth::Word,
                &0x4444_u32.to_le_bytes(),
                context
            ),
            ConditionalStatus::Failure,
            "{kind:?}: a committed SBA write invalidates the overlapping reservation"
        );
        drop(hart);
        assert_eq!(
            platform.ram.lock().unwrap().read_bytes(offset, 4).unwrap(),
            vec![0x22, 0x22, 0x33, 0x33],
            "{kind:?}: the failed SC commits nothing and the SBA bytes stay"
        );

        // A refused write that overlaps the reserved bytes is the dangerous
        // case: if the backend recorded the write before rejecting it, the
        // reservation would die and the SC would fail for the wrong reason.
        let end_word = u64::try_from(platform.ram.lock().unwrap().size()).unwrap() - 4;
        let end_address = platform_address(kind, end_word);
        assert_eq!(
            platform.sba_access(write(end_address, SbaAccessSize::Word, 0x1111_2222)),
            Ok(0)
        );
        let mut hart = platform.data.lock().unwrap();
        let context = load_reserved(&mut *hart, end_address, PhysicalWidth::Word);
        drop(hart);
        assert_eq!(
            platform.sba_access(write(end_address, SbaAccessSize::DoubleWord, 0)),
            Err(bad_address()),
            "{kind:?}: a span crossing the end of RAM is refused"
        );
        assert_eq!(
            platform.ram.lock().unwrap().read_word(end_word).unwrap(),
            0x1111_2222,
            "{kind:?}: the refused overlapping write committed no byte"
        );
        let mut hart = platform.data.lock().unwrap();
        assert_eq!(
            store_conditional(
                &mut *hart,
                end_address,
                PhysicalWidth::Word,
                &0x5555_u32.to_le_bytes(),
                context
            ),
            ConditionalStatus::Success,
            "{kind:?}: a refused SBA write invalidates nothing"
        );
    }
}

/// U2 §4: the Platform composes one RAM/device domain. An SBA write is
/// visible to the Hart-facing RAM view, and a native MMIO write is a Platform
/// effect with debug provenance.
#[test]
fn platform_inbound_path_shares_the_platform_domain() {
    for kind in [PlatformKind::Flat, PlatformKind::Native] {
        let platform = platform(kind);
        let address = platform_address(kind, RAM_OFFSET);
        assert_eq!(
            platform.sba_access(write(address, SbaAccessSize::Word, 0xfeed_face)),
            Ok(0)
        );
        assert_eq!(
            platform.ram.lock().unwrap().read_word(RAM_OFFSET).unwrap(),
            0xfeed_face,
            "{kind:?}: the SBA write reached the Platform RAM"
        );
        assert_eq!(
            platform.sba_access(read(address, SbaAccessSize::Word)),
            Ok(0xfeed_face)
        );
        let _ = platform.take_events().unwrap();
        let memory = u64::try_from(platform.ram.lock().unwrap().size()).unwrap();
        assert_eq!(
            platform.sba_access(read(address + memory, SbaAccessSize::Word)),
            Err(bad_address())
        );
        assert!(
            platform.take_events().unwrap().is_empty(),
            "{kind:?}: a refused access raises no Platform event"
        );
    }

    let platform = platform(PlatformKind::Native);
    assert_eq!(
        platform.sba_access(write(
            SYSTEM_BUS_UART_BASE,
            SbaAccessSize::Byte,
            u64::from(b'Q')
        )),
        Ok(0)
    );
    assert_eq!(
        platform.take_events().unwrap(),
        vec![PlatformEvent::UartTransmit(b'Q')],
        "the SBA MMIO write is a committed Platform effect"
    );
}

/// A Hart port that fails the test if the inbound path ever uses it.
struct ForbiddenPort;

impl PhysicalAccess for ForbiddenPort {
    fn access(&mut self, _request: PhysicalRequest<'_>) -> PhysicalAccessResult {
        panic!("a system bus access reached the Hart's physical port");
    }
}

impl AtomicAccess for ForbiddenPort {
    fn access_atomic(&mut self, _request: AtomicRequest<'_>) -> AtomicAccessResult {
        panic!("a system bus access reached the Hart's atomic port");
    }
}

/// U2 §3/U2 §4: SBA performs no Hart translation, privilege, PMP or trap work.
/// It still completes while both the Hart's installed data port and the
/// Platform's installed Hart-facing data port refuse every request, and it
/// changes no Hart state. The replacement covers the ports installed on the
/// core and Platform after construction; constructor independence follows from
/// the composition this test reaches through `Platform::sba_access`.
#[test]
fn a_system_bus_access_uses_only_the_platform_inbound_port() {
    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4000))
            .expect("fixture ELF"),
    );
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image).expect("Machine");
    let before = (owned.state().pc, owned.state().privilege);
    owned.test_replace_data(Arc::new(Mutex::new(ForbiddenPort)));
    owned.test_replace_platform_data(Arc::new(Mutex::new(ForbiddenPort)));

    let address = RAM_BASE + RAM_OFFSET;
    assert_eq!(
        owned.test_sba_access(write(
            address,
            SbaAccessSize::DoubleWord,
            0x1234_5678_9abc_def0
        )),
        Ok(0)
    );
    assert_eq!(
        owned.test_sba_access(read(address, SbaAccessSize::DoubleWord)),
        Ok(0x1234_5678_9abc_def0)
    );
    assert_eq!(
        (owned.state().pc, owned.state().privilege),
        before,
        "no trap, PC or privilege change"
    );
}

/// Coordination for the atomic-envelope exclusion test.
///
/// The AMO transform runs inside the backend's critical section. The inbound
/// backend signals when an SBA transaction has reached it, which is the last
/// point before the shared storage/device lock; if the two paths share that
/// lock, the SBA transaction cannot finish while the transform is active.
/// The signal proves backend entry during the active transform. It does not
/// prove that the reader reached the lock acquisition: a reader descheduled
/// between the signal and the wrapped call can escape the observation window,
/// so this is a coordinated concurrency regression test with that scheduling
/// assumption, not an unconditional lock-contention proof.
struct Probe {
    state: Mutex<ProbeState>,
    changed: Condvar,
}

#[derive(Default)]
struct ProbeState {
    entered: bool,
    reached: bool,
    done: bool,
    interposed: bool,
}

static PROBE: LazyLock<Probe> = LazyLock::new(|| Probe {
    state: Mutex::new(ProbeState::default()),
    changed: Condvar::new(),
});

/// An inbound backend that signals when one SBA transaction reaches it.
///
/// The signal is emitted immediately before the wrapped backend acquires the
/// shared storage/device lock, so the transform can distinguish "the SBA
/// transaction is at the contested boundary" from "the reader thread has not
/// run yet".
struct SignalEntry<B> {
    backend: B,
}

impl<B: PhysicalBackend> PhysicalBackend for SignalEntry<B> {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let mut state = PROBE.state.lock().unwrap();
        state.reached = true;
        PROBE.changed.notify_all();
        drop(state);
        self.backend.transact(request)
    }
}

impl Probe {
    fn reset() {
        *PROBE.state.lock().unwrap() = ProbeState::default();
    }

    /// `AmoTransform` body: hold the critical section open for the SBA reader.
    ///
    /// Coordination failures are hard failures, not silent passes. The
    /// non-completion wait is the evidence: if the SBA transaction shares the
    /// domain, it cannot set `done` until this transform returns.
    fn interpose(_old: &[u8], operand: &[u8]) -> [u8; 8] {
        let mut result = [0; 8];
        result[..operand.len()].copy_from_slice(operand);
        let mut state = PROBE.state.lock().unwrap();
        state.entered = true;
        PROBE.changed.notify_all();
        let (state, reached) = PROBE
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.reached)
            .unwrap();
        assert!(
            !reached.timed_out(),
            "the SBA transaction never reached the inbound backend"
        );
        let (mut state, _) = PROBE
            .changed
            .wait_timeout_while(state, Duration::from_millis(250), |state| !state.done)
            .unwrap();
        state.interposed = state.done;
        PROBE.changed.notify_all();
        result
    }
}

/// U2 §3 and ADR-0002 §6: an SBA transaction must not observe or expose the
/// intermediate state of a Hart AMO/SC.
///
/// The AMO runs through the Platform data port and its transform holds the
/// critical section open. The reader reaches an inbound port over the same
/// RAM storage domain and signals from the last point before the shared lock.
/// It must then stay blocked until the AMO commits, and it must return that
/// committed value; a transaction that completed inside the section is
/// recorded as interposition and fails the test.
///
/// The test builds its own inbound view over the Platform's RAM object. For
/// the native configuration that is the RAM the bus also targets, not the
/// Platform's bus-backed inbound port; the shared-domain and reservation
/// tests cover that composition. The join deadline below only bounds how long
/// the parent waits before reporting; scope teardown still waits for a blocked
/// reader, so a true deadlock is terminated by the outer test timeout.
#[test]
fn an_sba_transaction_cannot_interpose_inside_a_hart_atomic_envelope() {
    for kind in [PlatformKind::Flat, PlatformKind::Native] {
        let platform = platform(kind);
        let address = platform_address(kind, RAM_OFFSET);
        let previous = 0x0bad_f00d_u32;
        let written = 0xcafe_1234_u32;
        assert_eq!(
            platform.sba_access(write(address, SbaAccessSize::Word, u64::from(previous))),
            Ok(0)
        );

        // The test wraps its own inbound view over the Platform's RAM object.
        // That is the same storage lock domain the Platform's inbound port
        // uses, and it is the only place where the "reached the boundary"
        // signal can be emitted without product-only test hooks.
        let ram = platform.ram.clone();
        let size = ram.lock().unwrap().size();
        let base = match kind {
            PlatformKind::Native => RAM_BASE,
            PlatformKind::Flat => 0,
        };
        let inbound_port = {
            let backend = SignalEntry {
                backend: NativeRamBackend::new(ram, base, size),
            };
            let port: SharedPhysicalAccess =
                Arc::new(Mutex::new(ValidatedPhysicalAccess::new(backend)));
            InboundPort::new(port)
        };

        Probe::reset();
        let inbound = std::thread::scope(|scope| {
            let inbound_port = &inbound_port;
            let reader = scope.spawn(|| {
                let state = PROBE.state.lock().unwrap();
                let (state, entered) = PROBE
                    .changed
                    .wait_timeout_while(state, Duration::from_secs(5), |state| !state.entered)
                    .unwrap();
                assert!(
                    !entered.timed_out(),
                    "the AMO never entered its critical section"
                );
                drop(state);
                let value = inbound_port.access(read(address, SbaAccessSize::Word));
                let mut state = PROBE.state.lock().unwrap();
                state.done = true;
                PROBE.changed.notify_all();
                value
            });

            let operand = written.to_le_bytes();
            let request = AtomicRequest::rmw(
                address,
                PhysicalWidth::Word,
                ORDERING,
                &operand,
                AmoTransform::new(AmoWidth::Word, Probe::interpose),
            )
            .expect("valid RMW envelope");
            let response = platform
                .data
                .lock()
                .unwrap()
                .access_atomic(request)
                .expect("AMO completes");
            assert_eq!(
                response.old_bytes().unwrap().to_vec(),
                previous.to_le_bytes().to_vec(),
                "{kind:?}: the AMO returned the old value"
            );
            // Diagnostic deadline only: it reports a reader that never
            // completed, while scope teardown below still waits for it.
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !reader.is_finished() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{kind:?}: the SBA read never completed after the AMO"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            reader.join().unwrap()
        });

        assert_eq!(
            inbound,
            Ok(u64::from(written)),
            "{kind:?}: the blocked SBA read observed the committed AMO result"
        );
        let state = PROBE.state.lock().unwrap();
        assert!(
            !state.interposed,
            "{kind:?}: an SBA read completed inside the AMO critical section"
        );
        assert!(state.done, "{kind:?}: the SBA read completed after the AMO");
        drop(state);
        assert_eq!(
            platform.ram.lock().unwrap().read_word(RAM_OFFSET).unwrap(),
            written,
            "{kind:?}: the AMO committed exactly once"
        );
    }
}

/// One active DM over a Hart whose physical ports fail the test if used.
struct DmHarness {
    dm: DebugModule,
    hart: RiscvCore,
    inbound: InboundPort,
}

impl DmHarness {
    fn new(inbound: InboundPort) -> Self {
        let memory: Arc<Mutex<SimpleMemory>> = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
        let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = memory;
        let hart = RiscvCore::new_with_physical_ports(
            typed.clone(),
            typed,
            Arc::new(Mutex::new(ForbiddenPort)),
            Arc::new(Mutex::new(ForbiddenPort)),
        );
        let mut harness = Self {
            dm: DebugModule::new(),
            hart,
            inbound,
        };
        harness.write(DMCONTROL, DMACTIVE);
        harness
    }

    fn read(&mut self, addr: u8) -> u32 {
        self.dm.dmi_read(&mut self.hart, addr)
    }

    fn write(&mut self, addr: u8, value: u32) {
        self.dm.dmi_write(&mut self.hart, addr, value)
    }

    fn sbcs(&mut self) -> u32 {
        self.read(SBCS)
    }

    fn sberror(&mut self) -> u32 {
        (self.sbcs() >> SBERROR_SHIFT) & 0b111
    }

    fn set_address(&mut self, address: u64) {
        self.write(SBADDRESS1, (address >> 32) as u32);
        self.write(SBADDRESS0, address as u32);
    }

    /// Hands every started request to the Platform port. Returns how many
    /// requests the DM started.
    fn service(&mut self) -> usize {
        let mut started = 0;
        while let Some(request) = self.dm.next_sba_request() {
            match self.inbound.access(request) {
                Ok(value) => self.dm.complete_sba_access(Ok(value)),
                Err(SbaAccessError::Target(fault)) => self.dm.complete_sba_access(Err(fault)),
                Err(other) => panic!("unexpected SBA path failure: {other:?}"),
            }
            started += 1;
        }
        started
    }
}

/// U2 §4: `sberror` 2/3/4 at the DM boundary. A target refusal is a bad
/// address from the same validated RAM backend the Platform uses; alignment
/// and an unsupported `sbaccess` are decided in the DM before any target is
/// reached. The harness drives a real `DebugModule`, not `Platform::build`;
/// the Platform construction has its own tests above.
#[test]
fn dm_maps_the_platform_refusal_and_its_own_prechecks_to_sberror() {
    let (_, inbound) = ram_inbound();
    let mut harness = DmHarness::new(inbound);
    let before = (harness.hart.state().pc, harness.hart.state().privilege);

    // One past the end of RAM: the Platform refuses the span.
    harness.write(SBCS, 2 << 17);
    harness.set_address(RAM_BASE + u64::try_from(RAM_SIZE).unwrap());
    harness.write(SBDATA1, 0);
    harness.write(SBDATA0, 0x1234);
    assert_eq!(harness.service(), 1, "the request reached the Platform");
    assert_eq!(
        harness.sberror(),
        2,
        "a refused span is sberror bad address"
    );
    assert_eq!(
        harness.sbcs() & SBBUSY,
        0,
        "the completed result clears busy"
    );

    // W1C and a retry: the port is still usable. `sbcs` is an ordinary
    // register, so the write also restores the word `sbaccess`.
    harness.write(SBCS, (2 << 17) | (2 << SBERROR_SHIFT));
    assert_eq!(harness.sberror(), 0);
    harness.set_address(RAM_BASE + RAM_OFFSET);
    harness.write(SBDATA0, 0xbeef);
    assert_eq!(harness.service(), 1);
    assert_eq!(harness.sberror(), 0);
    assert_eq!(
        harness
            .inbound
            .access(read(RAM_BASE + RAM_OFFSET, SbaAccessSize::Word)),
        Ok(0xbeef),
        "the retried write reached the Platform target"
    );

    // Misaligned address: no request is started.
    harness.write(SBCS, SBREADONADDR | (2 << 17));
    harness.set_address(RAM_BASE + RAM_OFFSET + 1);
    assert_eq!(
        harness.service(),
        0,
        "alignment is decided before the Platform"
    );
    assert_eq!(
        harness.sberror(),
        3,
        "a misaligned span is sberror alignment"
    );

    // `sbaccess` 4 (128-bit) is not advertised: no request is started.
    harness.write(SBCS, (4 << 17) | (3 << SBERROR_SHIFT));
    harness.write(SBDATA0, 0);
    assert_eq!(
        harness.service(),
        0,
        "an unsupported access size is decided before the Platform"
    );
    assert_eq!(harness.sberror(), 4, "an unsupported size is sberror size");

    assert_eq!(
        (harness.hart.state().pc, harness.hart.state().privilege),
        before,
        "no guest trap, PC or privilege change"
    );
}
