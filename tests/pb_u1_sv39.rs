//! Program-buffer U1: Debug Mode data access through the Hart's physical
//! ports (MPRV/MPP/Sv39, target faults, unknown completion).

use ruscv_sim::core::{DebugExecOutcome, PrivilegeMode, RiscvCore, StepOutcome};
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, NativeRamBackend, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendResult, PhysicalRequest, PhysicalTargetRejectionReason,
    ValidatedPhysicalAccess,
};
use std::sync::{Arc, Mutex};

const PAGE: u64 = 0x1000;
const RAM_SIZE: usize = 0x10_0000;
const BASE: u64 = 0x8000_0000;
const ROOT_PPN: u64 = 0x80000;
const VA_DATA: u64 = 0x2000;
const DATA_PA: u64 = 0x8000_4000;

#[derive(Clone, Copy)]
enum Fail {
    None,
    /// Fail the plain read of this issued address.
    ReadTarget(u64),
    ReadUnknown(u64),
    /// Fail only the atomic (A/D update) access at this issued address.
    AtomicUnknown(u64),
}

struct Backend {
    inner: NativeRamBackend,
    fail: Fail,
}

impl PhysicalBackend for Backend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        match self.fail {
            Fail::ReadTarget(a) if a == request.paddr() => Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                "injected",
            )),
            Fail::ReadUnknown(a) if a == request.paddr() => {
                Err(PhysicalBackendError::unknown("injected unknown"))
            }
            _ => self.inner.transact(request),
        }
    }
}

impl AtomicBackend for Backend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        match self.fail {
            Fail::AtomicUnknown(a) if a == request.paddr() => {
                Err(PhysicalBackendError::unknown("injected unknown"))
            }
            _ => self.inner.transact_atomic(request),
        }
    }
}

fn gp_write(ram: &Arc<Mutex<SimpleMemory>>, guest: u64, value: u64) {
    ram.lock()
        .unwrap()
        .write_dword(guest - BASE, value)
        .unwrap();
}

fn gp_read(ram: &Arc<Mutex<SimpleMemory>>, guest: u64) -> u64 {
    ram.lock().unwrap().read_dword(guest - BASE).unwrap()
}

/// Maps VA_DATA to DATA_PA with `perm`. Returns the leaf PTE's issued address.
fn install_leaf(ram: &Arc<Mutex<SimpleMemory>>, perm: u64) -> u64 {
    let vpn = [
        (VA_DATA >> 12) & 0x1ff,
        (VA_DATA >> 21) & 0x1ff,
        (VA_DATA >> 30) & 0x1ff,
    ];
    gp_write(
        ram,
        ROOT_PPN * PAGE + vpn[2] * 8,
        ((ROOT_PPN + 1) << 10) | 1,
    );
    gp_write(
        ram,
        (ROOT_PPN + 1) * PAGE + vpn[1] * 8,
        ((ROOT_PPN + 2) << 10) | 1,
    );
    let leaf = (ROOT_PPN + 2) * PAGE + vpn[0] * 8;
    gp_write(ram, leaf, ((DATA_PA >> 12) << 10) | perm | 1);
    leaf - BASE
}

fn ld(rd: u32, rs1: u32) -> u32 {
    (rs1 << 15) | (3 << 12) | (rd << 7) | 0x03
}

fn addi(rd: u32, rs1: u32, imm: u32) -> u32 {
    (imm << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

/// A halted Hart with an MPRV=1, MPP=S, Sv39 data view of `VA_DATA`.
/// `fail` is built from the leaf's issued address.
fn halted_sv39(perm: u64, fail: impl Fn(u64) -> Fail) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let leaf = install_leaf(&ram, perm);
    gp_write(&ram, DATA_PA, 0x1122_3344);
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(Backend {
        inner: NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        fail: fail(leaf),
    })));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.reset(0, BASE);
    core.set_haltreq(true);
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::DebugHalted { .. }
    ));
    let csr = &mut core.state_mut().csr;
    csr.write(supervisor::SATP, (8 << 60) | ROOT_PPN).unwrap();
    csr.write(machine::MSTATUS, mstatus::MPRV | (1 << mstatus::MPP_SHIFT))
        .unwrap();
    core.state_mut().regs[1] = VA_DATA;
    (core, ram)
}

#[test]
fn mprv_mpp_sv39_load_translates_and_restores_state() {
    let (mut core, _) = halted_sv39(0x46, |_| Fail::None);
    let dpc = core.dpc();
    assert_eq!(core.debug_execute(ld(5, 1)), DebugExecOutcome::Completed);
    assert_eq!(core.state().regs[5], 0x1122_3344);
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
    assert_eq!(core.dpc(), dpc);
}

#[test]
fn walk_read_target_rejection_is_an_exception_and_not_quarantine() {
    let (mut core, _) = halted_sv39(0x46, Fail::ReadTarget);
    assert_eq!(core.debug_execute(ld(5, 1)), DebugExecOutcome::Exception);
    assert!(core.unresolved_physical_access().is_none());
    assert_eq!(
        core.debug_execute(addi(6, 0, 3)),
        DebugExecOutcome::Completed
    );
    assert_eq!(core.state().regs[6], 3);
    assert!(core.in_debug_mode());
}

#[test]
fn unknown_walk_read_quarantines_the_hart() {
    let (mut core, _) = halted_sv39(0x46, Fail::ReadUnknown);
    assert!(matches!(
        core.debug_execute(ld(5, 1)),
        DebugExecOutcome::Failure(_)
    ));
    assert!(core.unresolved_physical_access().is_some());
    assert!(matches!(
        core.debug_execute(addi(6, 0, 3)),
        DebugExecOutcome::Failure(_)
    ));
    assert_eq!(core.state().regs[6], 0, "nothing executes after quarantine");
}

#[test]
fn unknown_ad_update_quarantines_the_hart() {
    // No A bit, so the walk issues an atomic A/D update on the leaf.
    let (mut core, _) = halted_sv39(0x06, Fail::AtomicUnknown);
    assert!(matches!(
        core.debug_execute(ld(5, 1)),
        DebugExecOutcome::Failure(_)
    ));
    assert!(core.unresolved_physical_access().is_some());
    assert!(matches!(
        core.debug_execute(addi(6, 0, 3)),
        DebugExecOutcome::Failure(_)
    ));
}

#[test]
fn failed_unknown_walk_restores_privilege_and_dpc() {
    let (mut core, ram) = halted_sv39(0x46, Fail::ReadUnknown);
    let dpc = core.dpc();
    let _ = core.debug_execute(ld(5, 1));
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
    assert_eq!(core.dpc(), dpc);
    assert_eq!(gp_read(&ram, DATA_PA), 0x1122_3344);
}
