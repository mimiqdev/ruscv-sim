//! Stage 4 public-path Sv39 translation driver tests (dev-plan §7.2).
//!
//! These build a real Hart with the validated fetch/data ports, install Sv39
//! page tables in the same RAM the ports serve, and drive the guest through
//! the public step path. They prove the D3 fault table, the D4 A/D
//! observation rules, and the D9 SUM/MXR/MPRV checks. dev-plan §7 item 2 is
//! one driver test, `sv39_walk_reads_d3_outcomes_and_ad_race_on_the_hart_ports`:
//! walk reads on the data port, the D3 outcomes, and the A/D race, all
//! through the Hart's own ports rather than the component walker.
//!
//! Guest-physical layout inside the one RAM (PTE permission bits: R=0x02,
//! W=0x04, X=0x08, U=0x10, A=0x40, D=0x80; V is ORed by the installer):
//!
//! - 0x8000_0000 root table (PPN 0x80000)
//! - 0x8000_1000 mid table   (PPN 0x80001)
//! - 0x8000_2000 leaf table  (PPN 0x80002)
//! - 0x8000_4000 data page   (PPN 0x80004), VA 0x2000
//! - 0x8000_5000 program page (PPN 0x80005), VA 0x1000
//! - 0x8000_6000 second data page (PPN 0x80006), VA 0x3000, and the
//!   bare-mode page for untranslated M-mode programs.

use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::{ExceptionCause, PrivilegeMode, RiscvCore, StepOutcome, Trap};
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use std::sync::{Arc, Mutex};

const PAGE: u64 = 0x1000;
const RAM_SIZE: usize = 0x10_0000;
/// Guest-physical base of the RAM range, matching the flat image convention.
const BASE: u64 = 0x8000_0000;
/// Program virtual page: VA 0x1000 ..= 0x1fff.
const VA: u64 = 0x1000;
/// First data virtual page: VA 0x2000.
const VA_DATA: u64 = 0x2000;
/// Second data virtual page: VA 0x3000, one page above `VA_DATA`.
const VA_DATA2: u64 = 0x3000;
/// Program guest-physical page (PPN 0x80005).
const MAPPED_PA: u64 = 0x8000_5000;
/// Data guest-physical page (PPN 0x80004).
const DATA_PA: u64 = 0x8000_4000;
/// Second data guest-physical page (PPN 0x80006); also the bare-mode page.
const DATA2_PA: u64 = 0x8000_6000;
/// Root table at guest-physical 0x8000_0000 (PPN 0x80000).
const ROOT_PPN: u64 = 0x80000;
const MTVEC: u64 = 0x40;

fn core_with_ram() -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.reset(0, BASE);
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    (core, ram)
}

/// Installs Sv39 with the standard root and drops the hart into `mode` at
/// `VA`. `satp` is written while the CSR view is still M, because S and U
/// cannot write `satp` once the view matches the hart.
fn enable_sv39(core: &mut RiscvCore, mode: PrivilegeMode) {
    core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    core.state_mut()
        .csr
        .write(supervisor::SATP, crate_satp(ROOT_PPN))
        .unwrap();
    core.state_mut().privilege = mode;
    core.state_mut().csr.set_privilege(mode);
    core.state_mut().pc = VA;
}

/// Writes `mstatus` through the M view, then restores the hart's mode.
fn set_mstatus(core: &mut RiscvCore, mode: PrivilegeMode, value: u64) {
    core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    core.state_mut().csr.write(machine::MSTATUS, value).unwrap();
    core.state_mut().csr.set_privilege(mode);
}

fn crate_satp(ppn: u64) -> u64 {
    (8u64 << 60) | ppn
}

fn gp_write(ram: &Arc<Mutex<SimpleMemory>>, guest: u64, value: u64) {
    ram.lock()
        .unwrap()
        .write_dword(
            guest.checked_sub(BASE).expect("guest address inside RAM"),
            value,
        )
        .unwrap();
}

fn gp_read(ram: &Arc<Mutex<SimpleMemory>>, guest: u64) -> u64 {
    ram.lock()
        .unwrap()
        .read_dword(guest.checked_sub(BASE).expect("guest address inside RAM"))
        .unwrap()
}

/// Installs a 4 KiB leaf mapping `va` to guest-physical page `ppn` with
/// `perm` PTE permission bits, creating the pointer chain from the root.
/// Returns the leaf PTE's guest-physical address.
fn install_leaf(ram: &Arc<Mutex<SimpleMemory>>, va: u64, ppn: u64, perm: u64) -> u64 {
    let vpn = [(va >> 12) & 0x1ff, (va >> 21) & 0x1ff, (va >> 30) & 0x1ff];
    gp_write(
        ram,
        ROOT_PPN * PAGE + vpn[2] * 8,
        ((ROOT_PPN + 1) << 10) | 0x1,
    );
    gp_write(
        ram,
        (ROOT_PPN + 1) * PAGE + vpn[1] * 8,
        ((ROOT_PPN + 2) << 10) | 0x1,
    );
    let pte_addr = (ROOT_PPN + 2) * PAGE + vpn[0] * 8;
    gp_write(ram, pte_addr, (ppn << 10) | perm | 0x1);
    pte_addr
}

fn ld(rd: u8, rs1: u8, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20)
        | (u32::from(rs1) << 15)
        | (3 << 12)
        | (u32::from(rd) << 7)
        | 0x03
}

fn sd(rs2: u8, rs1: u8, imm: i32) -> u32 {
    let imm = (imm as u32) & 0xfff;
    ((imm >> 5) << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (3 << 12)
        | ((imm & 0x1f) << 7)
        | 0x23
}

/// amoswap.d rd, rs2, (rs1): funct5 00001, aq/rl 0, width D.
fn amoswap_d(rd: u8, rs2: u8, rs1: u8) -> u32 {
    (1u32 << 27)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (3 << 12)
        | (u32::from(rd) << 7)
        | 0x2f
}

fn load_value(ram: &Arc<Mutex<SimpleMemory>>, guest_addr: u64) -> u64 {
    gp_read(ram, guest_addr)
}

fn store_program(ram: &Arc<Mutex<SimpleMemory>>, guest_addr: u64, words: &[u32]) {
    let mut ram = ram.lock().unwrap();
    let base = guest_addr.checked_sub(BASE).expect("program inside RAM");
    for (index, word) in words.iter().enumerate() {
        ram.write_word(base + (index as u64) * 4, *word).unwrap();
    }
}

fn machine_csr(core: &RiscvCore, addr: u16) -> u64 {
    let mut csr = core.state().csr.clone();
    csr.set_privilege(PrivilegeMode::Machine);
    csr.read(addr).unwrap()
}

fn trap_of(outcome: &StepOutcome) -> ruscv_sim::core::TrapEntered {
    match outcome {
        StepOutcome::TrapEntered(trap) => *trap,
        other => panic!("expected a trap, got {other:?}"),
    }
}

/// Which completion an injected backend returns for one chosen address.
#[derive(Clone, Copy, Debug)]
enum Injected {
    /// The target rejects the request. A guest sees an access fault.
    Target,
    /// The host cannot complete it. The Hart reports a simulator failure.
    Host,
    /// The backend reports a protocol violation.
    Protocol,
    /// The backend cannot say whether the access happened.
    Unknown,
}

/// RAM that fails one physical address and serves everything else.
///
/// The address is the port offset, which is the guest-physical address
/// minus the RAM base. Failing by address rather than by call count keeps
/// the test pointed at one PTE even when the walk adds an A/D update.
struct InjectedBackend {
    inner: NativeRamBackend,
    fail_at: u64,
    injected: Injected,
}

impl ruscv_sim::physical::PhysicalBackend for InjectedBackend {
    fn transact(
        &mut self,
        request: &ruscv_sim::physical::PhysicalRequest<'_>,
    ) -> ruscv_sim::physical::PhysicalBackendResult {
        if request.paddr() == self.fail_at {
            return Err(injected_error(self.injected));
        }
        self.inner.transact(request)
    }
}

impl ruscv_sim::physical::AtomicBackend for InjectedBackend {
    fn transact_atomic(
        &mut self,
        request: &ruscv_sim::physical::AtomicRequest<'_>,
    ) -> ruscv_sim::physical::AtomicBackendResult {
        if request.paddr() == self.fail_at {
            return Err(injected_error(self.injected));
        }
        self.inner.transact_atomic(request)
    }
}

fn injected_error(injected: Injected) -> ruscv_sim::physical::PhysicalBackendError {
    use ruscv_sim::physical::{PhysicalBackendError, PhysicalTargetRejectionReason};
    match injected {
        Injected::Target => {
            PhysicalBackendError::target(PhysicalTargetRejectionReason::Unmapped, "injected")
        }
        Injected::Host => PhysicalBackendError::host("injected host"),
        Injected::Protocol => PhysicalBackendError::protocol("injected protocol"),
        Injected::Unknown => PhysicalBackendError::unknown("injected unknown"),
    }
}

fn core_with_injected(fail_at: u64, injected: Injected) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(InjectedBackend {
        inner: NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        fail_at,
        injected,
    })));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.reset(0, BASE);
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    (core, ram)
}

/// One physical request a driver port saw, in issue order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortEvent {
    Fetch(u64),
    DataRead(u64),
    DataWrite(u64),
    Lr(u64),
    Sc(u64, Option<ruscv_sim::physical::ConditionalStatus>),
}

/// Where a host write is interposed around the Hart's A/D update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RaceStage {
    /// Before the n-th load-reserved, after the step-2 walk read.
    BeforeLr(usize),
    /// After the n-th load-reserved returned, before its store-conditional.
    AfterLr(usize),
}

type RaceHook = Box<dyn FnMut(RaceStage, &Arc<Mutex<SimpleMemory>>) + Send>;

/// RAM behind one Hart port that logs every request, can fail one issued
/// address, and can run a host write between the Hart's walk read, its
/// load-reserved, and its store-conditional. The host write goes straight to
/// the shared `SimpleMemory`, so it is a committed write the conditional
/// check sees, exactly as another agent's store would be.
struct DriverBackend {
    inner: NativeRamBackend,
    ram: Arc<Mutex<SimpleMemory>>,
    events: Arc<Mutex<Vec<PortEvent>>>,
    fail_at: Option<(u64, Injected)>,
    lrs: usize,
    race: RaceHook,
}

impl ruscv_sim::physical::PhysicalBackend for DriverBackend {
    fn transact(
        &mut self,
        request: &ruscv_sim::physical::PhysicalRequest<'_>,
    ) -> ruscv_sim::physical::PhysicalBackendResult {
        use ruscv_sim::physical::PhysicalAccessKind;
        let descriptor = request.descriptor();
        self.events.lock().unwrap().push(match descriptor.category {
            PhysicalAccessKind::Fetch => PortEvent::Fetch(descriptor.paddr),
            PhysicalAccessKind::DataRead => PortEvent::DataRead(descriptor.paddr),
            PhysicalAccessKind::DataWrite => PortEvent::DataWrite(descriptor.paddr),
        });
        match self.fail_at {
            Some((paddr, injected)) if paddr == descriptor.paddr => Err(injected_error(injected)),
            _ => self.inner.transact(request),
        }
    }
}

impl ruscv_sim::physical::AtomicBackend for DriverBackend {
    fn transact_atomic(
        &mut self,
        request: &ruscv_sim::physical::AtomicRequest<'_>,
    ) -> ruscv_sim::physical::AtomicBackendResult {
        use ruscv_sim::physical::AtomicAccessKind;
        let paddr = request.paddr();
        let is_lr = request.kind() == AtomicAccessKind::LoadReserved;
        if is_lr {
            self.lrs += 1;
            (self.race)(RaceStage::BeforeLr(self.lrs), &self.ram);
        }
        let result = match self.fail_at {
            Some((fail, injected)) if fail == paddr => Err(injected_error(injected)),
            _ => self.inner.transact_atomic(request),
        };
        let event = match request.kind() {
            AtomicAccessKind::LoadReserved => PortEvent::Lr(paddr),
            AtomicAccessKind::StoreConditional => PortEvent::Sc(
                paddr,
                result.as_ref().ok().and_then(|r| r.conditional_status()),
            ),
            AtomicAccessKind::Rmw => panic!("the walk must not use the RMW transform"),
        };
        self.events.lock().unwrap().push(event);
        if is_lr && result.is_ok() {
            (self.race)(RaceStage::AfterLr(self.lrs), &self.ram);
        }
        result
    }
}

/// A Hart whose fetch and data ports are separate traced `DriverBackend`s
/// over one RAM. Only the data port carries the failure and the race.
struct Driver {
    core: RiscvCore,
    ram: Arc<Mutex<SimpleMemory>>,
    fetch: Arc<Mutex<Vec<PortEvent>>>,
    data: Arc<Mutex<Vec<PortEvent>>>,
}

impl Driver {
    fn new(fail_at: Option<(u64, Injected)>, race: RaceHook) -> Self {
        let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
        let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
        let fetch = Arc::new(Mutex::new(Vec::new()));
        let data = Arc::new(Mutex::new(Vec::new()));
        let port = |events: &Arc<Mutex<Vec<PortEvent>>>,
                    fail_at: Option<(u64, Injected)>,
                    race: RaceHook| {
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(DriverBackend {
                inner: NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
                ram: ram.clone(),
                events: events.clone(),
                fail_at,
                lrs: 0,
                race,
            })))
        };
        let fetch_port = port(&fetch, None, Box::new(|_, _| {}));
        let data_port = port(&data, fail_at, race);
        let mut core =
            RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch_port, data_port);
        core.reset(0, BASE);
        core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
        Self {
            core,
            ram,
            fetch,
            data,
        }
    }

    /// Program page R|W|X|A at VA, data page R|W|A at VA_DATA, the guest
    /// word 0x11 at DATA_PA and 0x22 at DATA2_PA, and `insn` at VA in S.
    /// A is already set on both leaves so no A/D update runs unless a case
    /// clears it.
    fn mapped(fail_at: Option<(u64, Injected)>, race: RaceHook, insn: u32, rs1: u64) -> Self {
        let mut driver = Self::new(fail_at, race);
        install_leaf(&driver.ram, VA, 0x80005, 0x4e);
        install_leaf(&driver.ram, VA_DATA, 0x80004, 0x46);
        gp_write(&driver.ram, DATA_PA, 0x11);
        gp_write(&driver.ram, DATA2_PA, 0x22);
        store_program(&driver.ram, MAPPED_PA, &[insn]);
        driver.core.state_mut().regs[1] = rs1;
        driver.core.state_mut().regs[2] = 0x99;
        enable_sv39(&mut driver.core, PrivilegeMode::Supervisor);
        driver
    }

    fn data_events(&self) -> Vec<PortEvent> {
        self.data.lock().unwrap().clone()
    }
}

fn no_race() -> RaceHook {
    Box::new(|_, _| {})
}

/// Issued (port) address of a guest-physical address.
fn issued(guest: u64) -> u64 {
    guest - BASE
}

/// Issued addresses of the three walk reads for `va`.
fn walk_reads(va: u64) -> [PortEvent; 3] {
    [
        PortEvent::DataRead(issued(ROOT_PPN * PAGE + ((va >> 30) & 0x1ff) * 8)),
        PortEvent::DataRead(issued((ROOT_PPN + 1) * PAGE + ((va >> 21) & 0x1ff) * 8)),
        PortEvent::DataRead(issued((ROOT_PPN + 2) * PAGE + ((va >> 12) & 0x1ff) * 8)),
    ]
}

/// dev-plan §7 item 2, as one Hart-level driver test on the validated
/// ports. Every case below steps a `RiscvCore` built with separate traced
/// fetch and data ports; nothing calls the component walker.
///
/// 1. A walk read is a `DataRead` on the data port, distinct from the
///    guest's own data access and from the fetch.
/// 2. The three D3 outcomes: a target rejection of the walk read is an
///    access fault of the original kind; a host, protocol, or unknown
///    completion is a `SimulatorFailure`; an invalid PTE, a reserved PTE, a
///    misaligned superpage, and a non-canonical VA are page faults, and the
///    non-canonical VA is rejected before any walk read.
/// 3. The A/D race through the Hart's data port: a leaf that changes
///    between the walk read and the conditional write gets no write and is
///    re-read (step 2), and a host write between the load-reserved and the
///    store-conditional is never overwritten with stale bytes.
#[test]
fn sv39_walk_reads_d3_outcomes_and_ad_race_on_the_hart_ports() {
    let data_leaf = (ROOT_PPN + 2) * PAGE + ((VA_DATA >> 12) & 0x1ff) * 8;
    let prog_leaf = (ROOT_PPN + 2) * PAGE + ((VA >> 12) & 0x1ff) * 8;

    // ----- 1. Walk reads are data reads on the data port. -----
    let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), VA_DATA);
    let outcome = d.core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(d.core.state().regs[5], 0x11);
    // The fetch port carries only the instruction fetch, at the translated
    // address. Every walk read is on the data port.
    assert_eq!(
        *d.fetch.lock().unwrap(),
        [PortEvent::Fetch(issued(MAPPED_PA))]
    );
    let mut expected = walk_reads(VA).to_vec();
    expected.extend(walk_reads(VA_DATA));
    expected.push(PortEvent::DataRead(issued(DATA_PA)));
    assert_eq!(d.data_events(), expected, "fetch walk, load walk, load");

    // ----- 2a. Target rejection: an access fault of the original kind. -----
    // The port rejects the leaf read of the access's own walk.
    for (insn, va, leaf, cause) in [
        (
            ld(5, 1, 0),
            VA_DATA,
            data_leaf,
            ExceptionCause::LoadAccessFault,
        ),
        (
            sd(2, 1, 0),
            VA_DATA,
            data_leaf,
            ExceptionCause::StoreAccessFault,
        ),
        (0x13, VA, prog_leaf, ExceptionCause::InstructionAccessFault),
    ] {
        let mut d = Driver::mapped(
            Some((issued(leaf), Injected::Target)),
            no_race(),
            insn,
            VA_DATA,
        );
        let trap = trap_of(&d.core.step_outcome());
        assert_eq!(trap.cause, Trap::Exception(cause));
        assert_eq!(trap.mtval, va);
        assert_eq!(gp_read(&d.ram, DATA_PA), 0x11, "{cause:?} wrote nothing");
    }
    // A root table below the RAM base is rejected by the guest-physical
    // conversion before the port: the fetch's walk takes the access fault.
    let mut d = Driver::mapped(None, no_race(), 0x13, 0);
    d.core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    d.core
        .state_mut()
        .csr
        .write(supervisor::SATP, crate_satp(0x1))
        .unwrap();
    d.core
        .state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    let trap = trap_of(&d.core.step_outcome());
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::InstructionAccessFault)
    );
    assert_eq!(trap.mtval, VA);
    assert!(d.data_events().is_empty() && d.fetch.lock().unwrap().is_empty());

    // ----- 2b. Host, protocol, unknown: SimulatorFailure, never a trap. -----
    for (injected, kind) in [
        (
            Injected::Host,
            ruscv_sim::core::SimulatorFailureKind::HostBackend,
        ),
        (
            Injected::Protocol,
            ruscv_sim::core::SimulatorFailureKind::Invariant,
        ),
        (
            Injected::Unknown,
            ruscv_sim::core::SimulatorFailureKind::HostBackend,
        ),
    ] {
        let mut d = Driver::mapped(
            Some((issued(data_leaf), injected)),
            no_race(),
            ld(5, 1, 0),
            VA_DATA,
        );
        let StepOutcome::SimulatorFailure(failure) = d.core.step_outcome() else {
            panic!("{injected:?} must be a SimulatorFailure")
        };
        assert_eq!(failure.kind, kind);
        assert_eq!(machine_csr(&d.core, machine::MCAUSE), 0, "{injected:?}");
    }

    // ----- 2c. Page faults. -----
    let page_fault = |d: &mut Driver, va: u64| {
        let trap = trap_of(&d.core.step_outcome());
        assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
        assert_eq!(trap.mtval, va);
    };
    // Invalid leaf (V = 0).
    let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), VA_DATA);
    let leaf = gp_read(&d.ram, data_leaf);
    gp_write(&d.ram, data_leaf, leaf & !1);
    page_fault(&mut d, VA_DATA);
    // Reserved PTE bits: bit 63 (N, Svnapot absent), bits 62:61 (PBMT,
    // Svpbmt absent), and bits 60:54.
    for bit in [63, 61, 54] {
        let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), VA_DATA);
        let leaf = gp_read(&d.ram, data_leaf);
        gp_write(&d.ram, data_leaf, leaf | (1 << bit));
        page_fault(&mut d, VA_DATA);
    }
    // A level-0 pointer: the walk must end on a leaf.
    let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), VA_DATA);
    gp_write(&d.ram, data_leaf, ((ROOT_PPN + 3) << 10) | 0x1);
    page_fault(&mut d, VA_DATA);
    // Misaligned megapage: a level-1 leaf with PPN[0] != 0.
    let big_va = (1u64 << 21) + 0x1000;
    let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), big_va);
    gp_write(
        &d.ram,
        (ROOT_PPN + 1) * PAGE + ((big_va >> 21) & 0x1ff) * 8,
        (0x80005u64 << 10) | 0x46 | 0x1,
    );
    page_fault(&mut d, big_va);
    // Non-canonical VA: bits 63:39 differ from bit 38. Canonicity belongs to
    // the VA, so the load issues no walk read at all; only the fetch walks.
    let non_canonical = 0x0000_0040_0000_0000;
    let mut d = Driver::mapped(None, no_race(), ld(5, 1, 0), non_canonical);
    page_fault(&mut d, non_canonical);
    assert_eq!(d.data_events(), walk_reads(VA));

    // ----- 3. A/D race on the Hart's data port. -----
    // The data leaf starts with A clear, so the load's walk performs the
    // step-9 update. The program leaf keeps A set, so the only
    // load-reserved is the data walk's.
    let fresh = (0x80004u64 << 10) | 0x06 | 0x1; // R|W|V, A = D = 0
    let remapped = (0x80006u64 << 10) | 0x06 | 0x1; // DATA2_PA, A = D = 0
    let load_walk = |extra: &[PortEvent]| {
        let mut events = walk_reads(VA).to_vec();
        events.extend(walk_reads(VA_DATA));
        events.extend_from_slice(extra);
        events
    };
    let leaf_at = issued(data_leaf);
    use ruscv_sim::physical::ConditionalStatus::{Failure, Success};

    // 3a. The leaf changes between the walk read and the load-reserved. The
    // load-reserved observes the new value, which differs from the checked
    // one, so no store-conditional is issued (no write). The walk resumes
    // at step 2 with that value and updates the new PTE.
    let race: RaceHook = Box::new(move |stage, ram| {
        if stage == RaceStage::BeforeLr(1) {
            gp_write(ram, data_leaf, remapped);
        }
    });
    let mut d = Driver::mapped(None, race, ld(5, 1, 0), VA_DATA);
    gp_write(&d.ram, data_leaf, fresh);
    let outcome = d.core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(d.core.state().regs[5], 0x22, "the load used the new PTE");
    assert_eq!(gp_read(&d.ram, data_leaf), remapped | 0x40);
    assert_eq!(
        d.data_events(),
        load_walk(&[
            PortEvent::Lr(leaf_at),
            PortEvent::Lr(leaf_at),
            PortEvent::Sc(leaf_at, Some(Success)),
            PortEvent::DataRead(issued(DATA2_PA)),
        ])
    );

    // 3b. A host write lands between the load-reserved and the
    // store-conditional. The store-conditional fails and writes nothing;
    // the walk re-reads the PTE (a step-2 DataRead) and updates the new one.
    let race: RaceHook = Box::new(move |stage, ram| {
        if stage == RaceStage::AfterLr(1) {
            gp_write(ram, data_leaf, remapped);
        }
    });
    let mut d = Driver::mapped(None, race, ld(5, 1, 0), VA_DATA);
    gp_write(&d.ram, data_leaf, fresh);
    let outcome = d.core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(d.core.state().regs[5], 0x22);
    assert_eq!(
        gp_read(&d.ram, data_leaf),
        remapped | 0x40,
        "A set on the host's PTE"
    );
    assert_eq!(
        d.data_events(),
        load_walk(&[
            PortEvent::Lr(leaf_at),
            PortEvent::Sc(leaf_at, Some(Failure)),
            PortEvent::DataRead(leaf_at),
            PortEvent::Lr(leaf_at),
            PortEvent::Sc(leaf_at, Some(Success)),
            PortEvent::DataRead(issued(DATA2_PA)),
        ])
    );

    // 3c. The host invalidates the leaf between the load-reserved and the
    // store-conditional. The stale A (and, for the store, D) update is
    // never written over it: the PTE keeps the host's bytes and the access
    // takes a page fault of its own kind after the step-2 re-read.
    for (insn, cause) in [
        (ld(5, 1, 0), ExceptionCause::LoadPageFault),
        (sd(2, 1, 0), ExceptionCause::StorePageFault),
    ] {
        let invalid = fresh & !1;
        let race: RaceHook = Box::new(move |stage, ram| {
            if stage == RaceStage::AfterLr(1) {
                gp_write(ram, data_leaf, invalid);
            }
        });
        let mut d = Driver::mapped(None, race, insn, VA_DATA);
        gp_write(&d.ram, data_leaf, fresh);
        let trap = trap_of(&d.core.step_outcome());
        assert_eq!(trap.cause, Trap::Exception(cause));
        assert_eq!(trap.mtval, VA_DATA);
        assert_eq!(
            gp_read(&d.ram, data_leaf),
            invalid,
            "{cause:?}: stale PTE bytes replaced the host write"
        );
        assert_eq!(gp_read(&d.ram, DATA_PA), 0x11, "{cause:?} wrote no data");
        assert_eq!(
            d.data_events(),
            load_walk(&[
                PortEvent::Lr(leaf_at),
                PortEvent::Sc(leaf_at, Some(Failure)),
                PortEvent::DataRead(leaf_at),
            ]),
            "{cause:?}"
        );
    }
}

#[test]
fn store_with_rs2_equal_rs1_writes_the_architectural_operand() {
    // Regression: the staged translation delta used to live in rs1 for the
    // execution window, so a store whose data register equals its base
    // register stored the delta-adjusted value.  sd x5, 0(x5) must store
    // the architectural x5 (the virtual address), not a physical one.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W data page
    store_program(&ram, MAPPED_PA, &[sd(5, 5, 0), 0x13]); // sd x5, 0(x5)
    core.state_mut().regs[5] = VA_DATA; // x5 is both base and operand
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let outcome = core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(gp_read(&ram, DATA_PA), VA_DATA);
    // The architectural x5 is unchanged.
    assert_eq!(core.state().regs[5], VA_DATA);
}

#[test]
fn amoswap_with_rs2_equal_rs1_swaps_the_architectural_operand() {
    // Regression for the same hole through the AMO envelope: amoswap.d
    // x5, x5, (x5) must swap the architectural x5 into the translated
    // address and return the old value.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x06);
    gp_write(&ram, DATA_PA, 0x77);
    store_program(&ram, MAPPED_PA, &[amoswap_d(5, 5, 5), 0x13]);
    core.state_mut().regs[5] = VA_DATA; // x5 is address and operand
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let outcome = core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(gp_read(&ram, DATA_PA), VA_DATA);
    assert_eq!(core.state().regs[5], 0x77);
}

#[test]
fn mapped_load_and_store_translate_with_ad_bits_and_observations() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    let leaf_addr = install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W data page
    gp_write(&ram, DATA_PA, 0x11);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0), sd(5, 1, 8), 0x13]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    // First instruction: the load sets A only. The fetch walk already set
    // A on the program leaf, so only the data leaf's A is fresh here.
    let transition = core.step_transition(true);
    assert!(matches!(
        transition.outcome,
        StepOutcome::InstructionRetired(_)
    ));
    let Some(Observation::Commit(commit)) = transition.observation else {
        panic!("expected a commit record")
    };
    let effects = &commit.effects.memory;
    // The implicit A/D writes for the fetch's program leaf and the load's
    // data leaf, then the explicit load: three effects, with the fetch's
    // write first because the fetch walk runs before the data walk.
    assert_eq!(effects.len(), 3);
    assert!(effects[0].implicit);
    assert!(effects[1].implicit);
    assert!(!effects[2].implicit);
    // The fetch write targets the program leaf; the load's write targets
    // the data leaf.
    let prog_leaf = (ROOT_PPN + 2) * PAGE + ((VA >> 12) & 0x1ff) * 8;
    assert_eq!(effects[0].guest_address, prog_leaf);
    assert_eq!(effects[1].guest_address, leaf_addr);
    // The A/D effect records the guest-physical PTE address in both
    // address fields. The walk subtracts the image base before it hands
    // the address to the port, so this is not the port-issued address;
    // the explicit effect below records that one.
    assert_eq!(effects[0].issued_address, Some(prog_leaf));
    assert_eq!(effects[1].issued_address, Some(leaf_addr));
    assert_eq!(effects[2].guest_address, VA_DATA);
    let pte_after_load = load_value(&ram, leaf_addr);
    assert_eq!(pte_after_load & 0x40, 0x40, "A set after the load");
    assert_eq!(pte_after_load & 0x80, 0, "D clear after the load");
    assert_eq!(core.state().regs[5], 0x11);

    // Second instruction: the store sets A and D.
    let transition = core.step_transition(true);
    assert!(matches!(
        transition.outcome,
        StepOutcome::InstructionRetired(_)
    ));
    let Some(Observation::Commit(commit)) = transition.observation else {
        panic!("expected a commit record")
    };
    let effects = &commit.effects.memory;
    // The second instruction's journal: the store's own data-leaf A/D write
    // (A was already set by the load, so this one sets D) and the explicit
    // store. The next fetch's program leaf already has A set, so it adds no
    // implicit effect.
    assert_eq!(effects.len(), 2);
    assert!(effects[0].implicit);
    assert_eq!(effects[0].guest_address, leaf_addr);
    assert!(!effects[1].implicit);
    let pte_after_store = load_value(&ram, leaf_addr);
    assert_eq!(pte_after_store & 0xC0, 0xC0, "A and D set after the store");
    assert_eq!(gp_read(&ram, DATA_PA + 8), 0x11);
}

#[test]
fn store_page_fault_keeps_original_va_and_writes_nothing() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x02); // R only data page
    store_program(&ram, MAPPED_PA, &[sd(5, 1, 0), 0x13]);
    core.state_mut().regs[1] = VA_DATA;
    core.state_mut().regs[5] = 0x99;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let outcome = core.step_outcome();
    let trap = trap_of(&outcome);
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::StorePageFault));
    assert_eq!(trap.mtval, VA_DATA);
    assert_eq!(core.state().pc, MTVEC);
    // No partial store and no A/D write: the leaf failed the W check at
    // step 8, before the step-9 A/D update.
    assert_eq!(gp_read(&ram, DATA_PA), 0);
    let leaf = load_value(&ram, (ROOT_PPN + 2) * PAGE + ((VA_DATA >> 12) & 0x1ff) * 8);
    assert_eq!(leaf & 0xC0, 0, "no A or D write for a permission fault");
}

#[test]
fn fetch_page_fault_on_non_executable_leaf_keeps_original_va() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x06); // R|W, no X
    store_program(&ram, MAPPED_PA, &[0x13]);
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    core.state_mut().pc = VA;

    let outcome = core.step_outcome();
    let trap = trap_of(&outcome);
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::InstructionPageFault)
    );
    assert_eq!(trap.mtval, VA);
    // The trapping fetch retires nothing.
    assert_eq!(machine_csr(&core, machine::MINSTRET), 0);
}

#[test]
fn rejected_ad_writeback_is_an_access_fault_of_the_original_kind() {
    // The leaf is readable and its A bit is clear, so the walk reaches the
    // A/D update. The port serves the walk read and rejects the atomic
    // update of the same PTE. The fault is a load access fault, and the
    // leaf stays unchanged because the store-conditional never commits.
    let leaf_addr = (ROOT_PPN + 2) * PAGE + ((VA_DATA >> 12) & 0x1ff) * 8;
    let (mut core, ram) = core_with_injected(leaf_addr - BASE, Injected::Target);
    install_leaf(&ram, VA, 0x80005, 0x4f); // R|W|X|A, so the fetch writes nothing
    install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W, A clear
    gp_write(&ram, DATA_PA, 0x11);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadAccessFault));
    assert_eq!(trap.mtval, VA_DATA);
    assert_eq!(load_value(&ram, leaf_addr) & 0xc0, 0, "A and D stay clear");
    assert_eq!(core.state().regs[5], 0, "the load never wrote its result");
}

#[test]
fn ad_writeback_success_then_access_fault_keeps_the_implicit_effect() {
    // The data leaf translates to a guest-physical page below the RAM base,
    // so the walk and the A/D write-back succeed but the explicit load is
    // rejected by the target (D4: the write stays visible and is recorded in
    // the TrapRecord; the fault is an access fault of the original kind).
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    let leaf_addr = install_leaf(&ram, VA_DATA, 0x1, 0x06); // R|W, PPN 0x1
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let transition = core.step_transition(true);
    let trap = trap_of(&transition.outcome);
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadAccessFault));
    assert_eq!(trap.mtval, VA_DATA);
    let Some(Observation::Trap(record)) = &transition.observation else {
        panic!("expected a trap record")
    };
    // The trap record carries the two committed A/D writes: the data leaf
    // that set A for the load, and the next fetch's program leaf that set
    // A before the explicit access was rejected.
    assert_eq!(record.effects.memory.len(), 2);
    assert!(record.effects.memory.iter().all(|effect| effect.implicit));
    assert!(record
        .effects
        .memory
        .iter()
        .any(|effect| effect.guest_address == leaf_addr));
    let pte = load_value(&ram, leaf_addr);
    assert_eq!(pte & 0x40, 0x40, "A stays set after the faulting load");
    assert_eq!(pte & 0x80, 0, "D stays clear for a load");
}

#[test]
fn sum_mxr_and_umode_permission_checks() {
    // S-mode load of a U=1 page with SUM=0 faults; with SUM=1 it retires.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x16); // R|W|U data page
    gp_write(&ram, DATA_PA, 0x21);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);

    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x16);
    gp_write(&ram, DATA_PA, 0x21);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    set_mstatus(&mut core, PrivilegeMode::Supervisor, mstatus::SUM);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0x21);

    // S-mode fetch of a U=1 page faults regardless of SUM.  The leaf has
    // X and U (R|X|U, no W), so only the U=1 rule can fault it: a broken
    // SUM check would retire the fetch instead.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x1a); // X|U program page
    store_program(&ram, MAPPED_PA, &[0x13]);
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    set_mstatus(&mut core, PrivilegeMode::Supervisor, mstatus::SUM);
    core.state_mut().pc = VA;
    let trap = trap_of(&core.step_outcome());
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::InstructionPageFault)
    );
    assert_eq!(trap.mtval, VA);

    // U-mode load of a U=0 page faults.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x1e); // R|W|X|U program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W, U=0
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::User);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);

    // MXR=0 load of an execute-only page faults; MXR=1 retires.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x08); // X only
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);

    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x08);
    gp_write(&ram, DATA_PA, 0x31);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    set_mstatus(&mut core, PrivilegeMode::Supervisor, mstatus::MXR);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0x31);
}

#[test]
fn mprv_loads_use_mpp_translation_and_fetch_stays_untranslated() {
    // In M with MPRV=1, MPP=S, SUM=1: the load uses S translation and may
    // touch a U=1 page; the fetch stays untranslated.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA_DATA, 0x80004, 0x16); // R|W|U data page
    gp_write(&ram, DATA_PA, 0x55);
    // The M-mode program sits at the bare guest address DATA2_PA.
    store_program(&ram, DATA2_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    core.state_mut().privilege = PrivilegeMode::Machine;
    core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    core.state_mut()
        .csr
        .write(
            machine::MSTATUS,
            mstatus::MPRV | (1u64 << mstatus::MPP_SHIFT) | mstatus::SUM,
        )
        .unwrap();
    core.state_mut()
        .csr
        .write(supervisor::SATP, crate_satp(ROOT_PPN))
        .unwrap();
    core.state_mut().pc = DATA2_PA;

    let outcome = core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );
    assert_eq!(core.state().regs[5], 0x55);

    // With SUM=0 the same load faults (SUM applies under MPRV with MPP=S).
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA_DATA, 0x80004, 0x16);
    gp_write(&ram, DATA_PA, 0x55);
    store_program(&ram, DATA2_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    core.state_mut().privilege = PrivilegeMode::Machine;
    core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    core.state_mut()
        .csr
        .write(
            machine::MSTATUS,
            mstatus::MPRV | (1u64 << mstatus::MPP_SHIFT),
        )
        .unwrap();
    core.state_mut()
        .csr
        .write(supervisor::SATP, crate_satp(ROOT_PPN))
        .unwrap();
    core.state_mut().pc = DATA2_PA;
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);
}

#[test]
fn bare_mode_and_machine_mode_stay_untranslated() {
    let (mut core, ram) = core_with_ram();
    store_program(&ram, DATA2_PA, &[ld(5, 1, 0)]);
    gp_write(&ram, DATA2_PA + 8, 0x77);
    core.state_mut().regs[1] = DATA2_PA + 8;
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.state_mut().pc = DATA2_PA;
    // satp stays Bare; the address is used as the guest physical address.
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0x77);

    // Machine mode with Sv39 active stays untranslated: the program and
    // the load both use bare guest addresses even though satp is Sv39.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    store_program(&ram, DATA2_PA, &[ld(5, 1, 0)]);
    gp_write(&ram, DATA2_PA + 8, 0x88);
    core.state_mut().regs[1] = DATA2_PA + 8;
    enable_sv39(&mut core, PrivilegeMode::Machine);
    core.state_mut().pc = DATA2_PA;
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0x88);
}

#[test]
fn sfence_vma_retires_and_translation_stays_visible_across_it() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W data page
    gp_write(&ram, DATA_PA, 0x33);
    // SFENCE.VMA with both rs1 and rs2 = x0 (global form), then a load that
    // uses the mapping.
    store_program(&ram, MAPPED_PA, &[0x1200_0073, ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(core.state().regs[5], 0x33);
}

#[test]
fn u_mode_sfence_is_illegal() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x18); // X|U program page
    store_program(&ram, MAPPED_PA, &[0x1200_0073]); // sfence.vma x0, x0
    core.state_mut().pc = VA;
    enable_sv39(&mut core, PrivilegeMode::User);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::IllegalInstruction)
    );
    assert_eq!(trap.mtval, 0x1200_0073);
}

#[test]
fn guest_reservation_survives_the_walk_private_lr_sc() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    install_leaf(&ram, VA_DATA, 0x80004, 0x06); // R|W data page, A=0
    install_leaf(&ram, VA_DATA2, 0x80006, 0x06); // R|W second page, A=0
    gp_write(&ram, DATA_PA, 7);
    // lr.d x5, (x1); a load from the still-clean second page whose walk
    // performs its private A/D update; then sc.d must still succeed because
    // the walk-private LR/SC is not the Hart reservation and the A/D write
    // is disjoint from the reserved span.
    let lr = 0x1000_b2af; // lr.d x5, 0(x1)
    let sc = 0x1850_b32f; // sc.d x6, x5, 0(x1)
    store_program(&ram, MAPPED_PA, &[lr, ld(7, 2, 0), sc]);
    core.state_mut().regs[1] = VA_DATA;
    core.state_mut().regs[2] = VA_DATA2;
    core.state_mut().regs[6] = 0xFFFF_FFFF_FFFF_FFFF;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(
        core.state().reservation.is_some(),
        "LR recorded the guest reservation"
    );
    // The intervening load's walk wrote A to the second page's leaf PTE, a
    // committed write disjoint from the reserved data span.
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(core.state().reservation.is_some());
    let outcome = core.step_outcome();
    assert!(matches!(outcome, StepOutcome::InstructionRetired(_)));
    assert_eq!(core.state().regs[6], 0, "SC succeeded");
}
