//! Stage 4 public-path Sv39 translation driver tests (dev-plan §7.2).
//!
//! These build a real Hart with the validated fetch/data ports, install Sv39
//! page tables in the same RAM the ports serve, and drive the guest through
//! the public step path. They prove the D3 fault table, the D4 A/D
//! observation rules, the D9 SUM/MXR/MPRV checks, and that a walk read is a
//! physical data read on the data port, distinct from the guest access.
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

/// The same RAM behind one traced port shared by fetch and data, so every
/// physical request the Hart issues is visible in one ordered vector.
#[allow(clippy::type_complexity)]
fn core_with_traced_ram() -> (
    RiscvCore,
    Arc<Mutex<SimpleMemory>>,
    Arc<Mutex<Vec<ruscv_sim::PhysicalRequestDescriptor>>>,
) {
    use ruscv_sim::physical::{
        AtomicBackend, AtomicBackendResult, AtomicRequest, PhysicalBackend, PhysicalBackendResult,
        PhysicalRequest,
    };

    struct Trace {
        inner: NativeRamBackend,
        calls: Arc<Mutex<Vec<ruscv_sim::PhysicalRequestDescriptor>>>,
    }
    impl PhysicalBackend for Trace {
        fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
            self.calls.lock().unwrap().push(request.descriptor());
            self.inner.transact(request)
        }
    }
    impl AtomicBackend for Trace {
        fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
            self.inner.transact_atomic(request)
        }
    }

    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(Trace {
        inner: NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        calls: calls.clone(),
    })));
    let fetch = port.clone();
    let data = port;
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.reset(0, BASE);
    core.state_mut().csr.write(machine::MTVEC, MTVEC).unwrap();
    (core, ram, calls)
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

#[test]
fn walk_reads_are_data_reads_on_the_data_port_distinct_from_the_access() {
    let (mut core, ram, calls) = core_with_traced_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0), 0x13]);
    core.state_mut().regs[1] = VA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let outcome = core.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::InstructionRetired(_)),
        "{outcome:?}"
    );

    let calls = calls.lock().unwrap();
    // The fetch walk reads three PTEs on the data port, the load walk reads
    // the same three, and the explicit load is one more DataRead: seven in
    // total. The instruction fetch itself is one Fetch-category request on
    // the same port, distinct from all seven.
    let data_reads: Vec<_> = calls
        .iter()
        .filter(|call| call.category == ruscv_sim::physical::PhysicalAccessKind::DataRead)
        .collect();
    assert_eq!(data_reads.len(), 7, "{calls:#x?}");
    // The walk reads land at the storage offsets hart_issued_paddr issues,
    // which are the guest-physical PTE addresses minus the RAM base.
    let root_read = ROOT_PPN * PAGE + (VA >> 30) * 8 - BASE;
    let mid_read = (ROOT_PPN + 1) * PAGE + ((VA >> 21) & 0x1ff) * 8 - BASE;
    let leaf_read = (ROOT_PPN + 2) * PAGE + ((VA >> 12) & 0x1ff) * 8 - BASE;
    assert_eq!(data_reads[0].paddr, root_read);
    assert_eq!(data_reads[1].paddr, mid_read);
    assert_eq!(data_reads[2].paddr, leaf_read);
    assert_eq!(data_reads[3].paddr, root_read);
    assert_eq!(data_reads[4].paddr, mid_read);
    assert_eq!(data_reads[5].paddr, leaf_read);
    // The explicit load is the last data read, at the translated address.
    assert_eq!(data_reads[6].paddr, MAPPED_PA - BASE);
    let fetches: Vec<_> = calls
        .iter()
        .filter(|call| call.category == ruscv_sim::physical::PhysicalAccessKind::Fetch)
        .collect();
    assert_eq!(fetches.len(), 1);
    assert_eq!(fetches[0].paddr, MAPPED_PA - BASE);
    // The load reads the two words at its address: the ld encoding and the
    // nop that follows it, both zero-extended and little endian.
    assert_eq!(
        core.state().regs[5],
        (0x13u64) << 32 | u64::from(ld(5, 1, 0))
    );
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
    // The implicit A/D write for the data leaf, the implicit A/D write for
    // the next fetch's program leaf, then the explicit load: three effects,
    // with the data-leaf write first because the data walk runs before the
    // executor issues the access and before the next fetch begins.
    assert_eq!(effects.len(), 3);
    assert!(effects[0].implicit);
    assert!(!effects[2].implicit);
    // The two implicit writes are the next fetch's program leaf and the
    // data leaf, in walk order.
    let prog_leaf = (ROOT_PPN + 2) * PAGE + ((VA >> 12) & 0x1ff) * 8;
    assert_eq!(effects[0].guest_address, prog_leaf);
    assert_eq!(effects[1].guest_address, leaf_addr);
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
fn noncanonical_va_is_a_page_fault_of_the_original_kind() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = 0x0000_0040_0000_0000; // bits 63:39 != bit 38
    enable_sv39(&mut core, PrivilegeMode::Supervisor);

    let outcome = core.step_outcome();
    let trap = trap_of(&outcome);
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, 0x0000_0040_0000_0000);
}

#[test]
fn invalid_reserved_and_pointer_only_walks_page_fault() {
    // Invalid PTE (V=0) at the root: the instruction fetch's own walk is the
    // first to see it.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    gp_write(&ram, ROOT_PPN * PAGE, 0);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::InstructionPageFault)
    );
    assert_eq!(trap.mtval, VA);

    // Reserved PTE bit 63 set on the leaf the load's walk visits.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x06);
    let leaf_addr = (ROOT_PPN + 2) * PAGE + ((VA_DATA >> 12) & 0x1ff) * 8;
    let leaf = load_value(&ram, leaf_addr);
    gp_write(&ram, leaf_addr, leaf | (1 << 63));
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);

    // Pointer-only walk: the level-0 entry of the data page is a pointer.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    install_leaf(&ram, VA_DATA, 0x80004, 0x06);
    let leaf_addr = (ROOT_PPN + 2) * PAGE + ((VA_DATA >> 12) & 0x1ff) * 8;
    let pointer = ((ROOT_PPN + 3) << 10) | 0x1;
    gp_write(&ram, leaf_addr, pointer);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA_DATA;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, VA_DATA);
}

#[test]
fn misaligned_superpage_page_faults_instead_of_translating() {
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e); // R|W|X program page, vpn1 = 0
                                           // A level-1 leaf for the vpn1 = 1 region whose PPN[0] is nonzero:
                                           // misaligned megapage. The program page's vpn1 = 0 region keeps its
                                           // 4 KiB mapping, so the fetch completes.
    let big_va = (1u64 << 21) + 0x1000;
    let vpn1 = (big_va >> 21) & 0x1ff;
    let pte_addr = (ROOT_PPN + 1) * PAGE + vpn1 * 8;
    let leaf = (0x80005u64 << 10) | 0x06 | 0x1;
    gp_write(&ram, pte_addr, leaf);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = big_va;
    enable_sv39(&mut core, PrivilegeMode::Supervisor);
    let trap = trap_of(&core.step_outcome());
    assert_eq!(trap.cause, Trap::Exception(ExceptionCause::LoadPageFault));
    assert_eq!(trap.mtval, big_va);
}

#[test]
fn walk_read_target_rejection_is_an_access_fault_of_the_original_kind() {
    // The root PTE read lands below the flat RAM range, so the
    // guest-physical conversion rejects it. The fetch is the first access
    // to walk, so it takes the instruction access fault.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x0e);
    core.state_mut().csr.set_privilege(PrivilegeMode::Machine);
    // satp PPN 0x1 places the root table at guest-physical 0x1000, below
    // the RAM base the hart subtracts.
    core.state_mut()
        .csr
        .write(supervisor::SATP, crate_satp(0x1))
        .unwrap();
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    store_program(&ram, MAPPED_PA, &[ld(5, 1, 0)]);
    core.state_mut().regs[1] = VA;
    core.state_mut().pc = VA;
    let trap = trap_of(&core.step_outcome());
    assert_eq!(
        trap.cause,
        Trap::Exception(ExceptionCause::InstructionAccessFault)
    );
    assert_eq!(trap.mtval, VA);
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

    // S-mode fetch of a U=1 page faults regardless of SUM.
    let (mut core, ram) = core_with_ram();
    install_leaf(&ram, VA, 0x80005, 0x16); // R|W|U, no X
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
