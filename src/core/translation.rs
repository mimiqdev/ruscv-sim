//! Hart-owned Sv39 address translation on the public physical data port.
//!
//! Every translated fetch, load, store, and AMO effective address walks the
//! three-level Sv39 page table through the same validated data port the
//! explicit access uses (dev-plan D3). Walk reads are `DataRead` requests at
//! the guest-physical PTE address; the step-9 A/D update is a walk-private
//! load-reserved/store-conditional envelope whose reservation is never the
//! Hart's architectural reservation (ADR-0002 §5). No translation is cached
//! on the public path (M4), so SFENCE.VMA needs no action here.
//!
//! Fault classification follows the D3 table. Page faults keep the original
//! virtual address in `mtval`; a target-rejected walk read or A/D write-back
//! is an access fault of the original access kind; host, protocol, and unknown
//! completions are `SimulatorFailure`, never guest traps. A lost A/D race
//! restarts the walk. The specification gives that restart no budget.

use super::{
    hart_issued_paddr, ExceptionCause, PrivilegeMode, RiscvCore, SimulatorFailureKind, StepOutcome,
    Trap,
};
use crate::core::observation::{AtomicEffect, MemoryEffect};
use crate::csr::{mstatus, supervisor, SATP_MODE_SV39};
use crate::mmu::pte::{flags, PageTableEntry};
use crate::mmu::sv39::{Sv39, VirtualAddress};
use crate::physical::{
    AtomicAccessError, AtomicOrdering, AtomicRequest, AtomicReservationContext, ConditionalStatus,
    PhysicalAccessError, PhysicalDataAccess, PhysicalRequest, PhysicalWidth,
};

const LEVELS: usize = 3;

/// The original access kind a translation serves. It selects the fault cause
/// and is retained through every failure path (ADR-0001 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AccessKind {
    Fetch,
    Load,
    Store,
}

impl AccessKind {
    fn page_fault(self) -> ExceptionCause {
        match self {
            Self::Fetch => ExceptionCause::InstructionPageFault,
            Self::Load => ExceptionCause::LoadPageFault,
            Self::Store => ExceptionCause::StorePageFault,
        }
    }

    fn access_fault(self) -> ExceptionCause {
        match self {
            Self::Fetch => ExceptionCause::InstructionAccessFault,
            Self::Load => ExceptionCause::LoadAccessFault,
            Self::Store => ExceptionCause::StoreAccessFault,
        }
    }
}

/// One walk's outcome before Hart-level classification.
enum Walk {
    Translated(u64),
    PageFault,
    /// The target rejected a walk read or the A/D write-back.
    AccessFault,
    /// A simulator-side failure that must never become a guest trap.
    Failure(SimulatorFailureKind, String),
}

/// The step-9 atomic conditional update of one leaf PTE.
enum AdUpdate {
    Committed,
    /// Nothing was written; the walk resumes at step 2 for the same table
    /// and level. `pte` carries the re-read value when the walk-private LR
    /// already observed the newer PTE.
    Retry {
        pte: Option<u64>,
    },
    Fault(Walk),
}

impl RiscvCore {
    /// Effective privilege for one access kind (privileged specification
    /// `mstatus` MPRV). Only an M-mode load or store adopts MPP; fetch never
    /// does, and below M the bit is unreachable because every transition out
    /// of M clears it.
    fn translation_privilege(&self, access: AccessKind) -> PrivilegeMode {
        let status = self.state.csr.mstatus();
        if access != AccessKind::Fetch
            && self.state.privilege == PrivilegeMode::Machine
            && status & mstatus::MPRV != 0
        {
            match (status >> mstatus::MPP_SHIFT) & 0b11 {
                0 => PrivilegeMode::User,
                1 => PrivilegeMode::Supervisor,
                _ => PrivilegeMode::Machine,
            }
        } else {
            self.state.privilege
        }
    }

    /// True when Sv39 translation applies at the effective privilege. Bare
    /// mode (`satp` MODE 0) and M-mode effective privilege keep the
    /// untranslated guest address (D2, D9).
    pub(super) fn translation_active(&self, privilege: PrivilegeMode) -> bool {
        privilege != PrivilegeMode::Machine
            && self.state.csr.architectural_value(supervisor::SATP) >> 60 == SATP_MODE_SV39
    }

    /// Translates one virtual address, or completes the architectural failure
    /// path. `faulting_pc` and `instruction` feed trap entry exactly as the
    /// surrounding execution step would.
    pub(super) fn translate_address(
        &mut self,
        address: u64,
        access: AccessKind,
        faulting_pc: u64,
        instruction: Option<u32>,
    ) -> Result<u64, StepOutcome> {
        let privilege = self.translation_privilege(access);
        if !self.translation_active(privilege) {
            return Ok(address);
        }
        match self.walk(address, access, privilege) {
            Walk::Translated(paddr) => Ok(paddr),
            Walk::PageFault => Err(self.enter_trap(
                Trap::Exception(access.page_fault()),
                faulting_pc,
                address,
                instruction,
            )),
            Walk::AccessFault => Err(self.enter_trap(
                Trap::Exception(access.access_fault()),
                faulting_pc,
                address,
                instruction,
            )),
            Walk::Failure(kind, message) => Err(self.simulator_failure(faulting_pc, kind, message)),
        }
    }

    fn walk(&mut self, address: u64, access: AccessKind, privilege: PrivilegeMode) -> Walk {
        // Step 1: canonicity is a property of the virtual address and is
        // checked before any walk read (D3).
        let va = match VirtualAddress::new(address) {
            Ok(va) => va,
            Err(_) => return Walk::PageFault,
        };
        let Some(port) = self.data_access.clone() else {
            return Walk::Failure(
                SimulatorFailureKind::HostBackend,
                "Sv39 translation requires the physical data port".into(),
            );
        };
        let mut port = match port.lock() {
            Ok(port) => port,
            Err(_) => {
                return Walk::Failure(
                    SimulatorFailureKind::HostBackend,
                    "failed to lock physical page-table access".into(),
                )
            }
        };
        let status = self.state.csr.mstatus();
        let satp = self.state.csr.architectural_value(supervisor::SATP);
        let vpns = va.vpns();
        let mut table_ppn = satp & ((1 << 44) - 1);
        let mut level = LEVELS - 1;
        let mut pte_value =
            match self.walk_read(&mut *port, Sv39::pte_address(table_ppn, vpns[level])) {
                Ok(value) => value,
                Err(walk) => return walk,
            };
        loop {
            let pte = PageTableEntry::from_raw(pte_value);
            if !pte.valid_sv39_at_level(level) {
                return Walk::PageFault;
            }
            if !pte.is_leaf() {
                // A non-leaf pointer at level 0 is a page fault; the walker
                // must end on a leaf (D3).
                if level == 0 {
                    return Walk::PageFault;
                }
                table_ppn = pte.ppn();
                level -= 1;
                pte_value =
                    match self.walk_read(&mut *port, Sv39::pte_address(table_ppn, vpns[level])) {
                        Ok(value) => value,
                        Err(walk) => return walk,
                    };
                continue;
            }
            // Leaf: permission checks (steps 6 and 8).
            if !leaf_permits(&pte, access, privilege, status) {
                return Walk::PageFault;
            }
            // Step 9: hardware sets A, and D for a store-class access.
            let updated = pte_value
                | flags::A
                | if access == AccessKind::Store {
                    flags::D
                } else {
                    0
                };
            if updated == pte_value {
                return Walk::Translated(Sv39::build_physical_address(&pte, va, level));
            }
            let pte_addr = Sv39::pte_address(table_ppn, vpns[level]);
            match self.update_leaf_ad(&mut *port, pte_addr, pte_value, updated) {
                AdUpdate::Committed => {
                    return Walk::Translated(Sv39::build_physical_address(&pte, va, level))
                }
                AdUpdate::Retry { pte } => {
                    pte_value = match pte {
                        Some(value) => value,
                        None => {
                            // A failed store-conditional wrote nothing; the
                            // PTE may have changed. Re-read step 2.
                            match self.walk_read(&mut *port, pte_addr) {
                                Ok(value) => value,
                                Err(walk) => return walk,
                            }
                        }
                    };
                }
                AdUpdate::Fault(walk) => return walk,
            }
        }
    }

    /// One step-2 walk read at a guest-physical PTE address (D3). The
    /// address follows the same guest-physical-to-storage conversion every
    /// explicit access uses (D7): `satp` PPN and PTE PPNs are guest physical
    /// page numbers, and the port expects the issued offset.
    fn walk_read(
        &self,
        port: &mut (dyn PhysicalDataAccess + Send),
        pte_addr: u64,
    ) -> Result<u64, Walk> {
        let issued = match hart_issued_paddr(
            pte_addr,
            self.base_addr,
            self.physical_storage_alignment,
            PhysicalWidth::Doubleword,
        ) {
            Ok(issued) => issued,
            Err(_) => return Err(Walk::AccessFault),
        };
        let request = match PhysicalRequest::data_read(issued, PhysicalWidth::Doubleword) {
            Ok(request) => request,
            Err(error) => {
                return Err(Walk::Failure(
                    SimulatorFailureKind::Invariant,
                    format!("malformed Sv39 walk read: {error}"),
                ))
            }
        };
        let response = match port.access(request) {
            Ok(response) => response,
            Err(error) => return Err(classify_walk_read(error)),
        };
        let bytes = response.read_bytes().ok_or_else(|| {
            Walk::Failure(
                SimulatorFailureKind::Invariant,
                "Sv39 walk read returned no byte completion".into(),
            )
        })?;
        if bytes.len() != PhysicalWidth::Doubleword.bytes() {
            return Err(Walk::Failure(
                SimulatorFailureKind::Invariant,
                format!("Sv39 walk read returned {} bytes, expected 8", bytes.len()),
            ));
        }
        Ok(u64::from_le_bytes(
            bytes[..8]
                .try_into()
                .expect("the length check fixes the count"),
        ))
    }

    /// The step-9 atomic conditional update of one leaf PTE. The
    /// load-reserved/store-conditional pair is private to this walk; its
    /// snapshot is never the Hart reservation.
    fn update_leaf_ad(
        &self,
        port: &mut (dyn PhysicalDataAccess + Send),
        pte_addr: u64,
        checked: u64,
        updated: u64,
    ) -> AdUpdate {
        let issued = match hart_issued_paddr(
            pte_addr,
            self.base_addr,
            self.physical_storage_alignment,
            PhysicalWidth::Doubleword,
        ) {
            Ok(issued) => issued,
            Err(_) => return AdUpdate::Fault(Walk::AccessFault),
        };
        let lr = match AtomicRequest::load_reserved(
            issued,
            PhysicalWidth::Doubleword,
            AtomicOrdering::default(),
        ) {
            Ok(request) => request,
            Err(error) => {
                return AdUpdate::Fault(Walk::Failure(
                    SimulatorFailureKind::Invariant,
                    format!("malformed Sv39 A/D load-reserved: {error}"),
                ))
            }
        };
        let reserved = lr.span();
        let loaded = match port.access_atomic(lr) {
            Ok(response) => response,
            Err(error) => return AdUpdate::Fault(classify_ad_update(error)),
        };
        let (Some(bytes), Some(snapshot)) = (loaded.old_bytes(), loaded.snapshot()) else {
            return AdUpdate::Fault(Walk::Failure(
                SimulatorFailureKind::Invariant,
                "Sv39 A/D load-reserved returned an incomplete completion".into(),
            ));
        };
        let Ok(bytes) = <[u8; 8]>::try_from(bytes) else {
            return AdUpdate::Fault(Walk::Failure(
                SimulatorFailureKind::Invariant,
                "Sv39 A/D load-reserved returned the wrong byte count".into(),
            ));
        };
        let current = u64::from_le_bytes(bytes);
        if current != checked {
            return AdUpdate::Retry { pte: Some(current) };
        }
        let payload = updated.to_le_bytes();
        let sc = match AtomicRequest::store_conditional(
            issued,
            PhysicalWidth::Doubleword,
            AtomicOrdering::default(),
            &payload,
            AtomicReservationContext { reserved, snapshot },
        ) {
            Ok(request) => request,
            Err(error) => {
                return AdUpdate::Fault(Walk::Failure(
                    SimulatorFailureKind::Invariant,
                    format!("malformed Sv39 A/D store-conditional: {error}"),
                ))
            }
        };
        let stored = match port.access_atomic(sc) {
            Ok(response) => response,
            Err(error) => return AdUpdate::Fault(classify_ad_update(error)),
        };
        match stored.conditional_status() {
            Some(ConditionalStatus::Success) => {
                self.record_ad_effect(pte_addr, current, updated);
                AdUpdate::Committed
            }
            Some(ConditionalStatus::Failure) => AdUpdate::Retry { pte: None },
            None => AdUpdate::Fault(Walk::Failure(
                SimulatorFailureKind::Invariant,
                "Sv39 A/D store-conditional returned no conditional status".into(),
            )),
        }
    }

    /// A committed A/D write is a visible memory effect even when the
    /// instruction later faults (D4, ADR-0001 §2). The effect carries
    /// `implicit: true` and is journaled before the explicit access.
    fn record_ad_effect(&self, pte_addr: u64, old: u64, new: u64) {
        if let Some(journal) = &self.memory_journal {
            journal.borrow_mut().push(MemoryEffect {
                guest_address: pte_addr,
                issued_address: Some(pte_addr),
                width: PhysicalWidth::Doubleword,
                read: Some(old.to_le_bytes()),
                write: Some(new.to_le_bytes()),
                atomic: Some(AtomicEffect {
                    kind: crate::physical::AtomicAccessKind::StoreConditional,
                    ordering: AtomicOrdering::default(),
                    conditional: Some(ConditionalStatus::Success),
                    indivisible: true,
                }),
                implicit: true,
            });
        }
    }
}

fn classify_walk_read(error: PhysicalAccessError) -> Walk {
    match error {
        PhysicalAccessError::TargetRejected(_) => Walk::AccessFault,
        PhysicalAccessError::BackendFailure(failure) => Walk::Failure(
            SimulatorFailureKind::HostBackend,
            format!("Sv39 walk read: {failure}"),
        ),
        PhysicalAccessError::Protocol(error) => Walk::Failure(
            SimulatorFailureKind::Invariant,
            format!("Sv39 walk read protocol failure: {error}"),
        ),
        PhysicalAccessError::UnknownCompletion(unknown) => Walk::Failure(
            SimulatorFailureKind::HostBackend,
            format!("Sv39 walk read: {unknown}"),
        ),
    }
}

fn classify_ad_update(error: AtomicAccessError) -> Walk {
    match error {
        AtomicAccessError::TargetRejected(_) => Walk::AccessFault,
        AtomicAccessError::BackendFailure(failure) => Walk::Failure(
            SimulatorFailureKind::HostBackend,
            format!("Sv39 A/D update: {failure}"),
        ),
        AtomicAccessError::Protocol(error) => Walk::Failure(
            SimulatorFailureKind::Invariant,
            format!("Sv39 A/D update protocol failure: {error}"),
        ),
        AtomicAccessError::UnknownCompletion(unknown) => Walk::Failure(
            SimulatorFailureKind::HostBackend,
            format!("Sv39 A/D update: {unknown}"),
        ),
    }
}

/// Steps 6 and 8 of the Sv39 walk. The component walker and TLB use the same
/// predicate, so SUM and MXR cannot drift between the two.
fn leaf_permits(
    pte: &PageTableEntry,
    access: AccessKind,
    privilege: PrivilegeMode,
    status: u64,
) -> bool {
    let access = match access {
        AccessKind::Fetch => crate::mmu::AccessType::InstructionFetch,
        AccessKind::Load => crate::mmu::AccessType::Read,
        AccessKind::Store => crate::mmu::AccessType::Write,
    };
    crate::mmu::pte::leaf_permits(pte.permissions(), access, privilege, status)
}
