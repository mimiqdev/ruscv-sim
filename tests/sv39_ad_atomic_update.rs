//! Sv39 step-9 A/D updates (Privileged ISA 20250508 §12.3.2, dev-plan D3/D4).
//!
//! The component walker sets A and D with a walk-private load-reserved and
//! store-conditional on the whole leaf PTE. These tests interpose a host RAM
//! writer and a guest reservation between the pair through one shared
//! `PhysicalMemory`, whose writes use the same committed-write accounting the
//! store-conditional checks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::memory::MemoryError;
use ruscv_sim::mmu::physical::{PhysicalMemory, PhysicalMemoryInterface};
use ruscv_sim::mmu::pte::{PagePermissions, PageTableEntry};
use ruscv_sim::mmu::sv39::{PageTableWalker, VirtualAddress, WalkResult};
use ruscv_sim::mmu::translator::{AddressTranslator, TranslationRequest};
use ruscv_sim::mmu::{AccessType, MmuConfig, MmuError, PageFaultReason, Tlb};
use ruscv_sim::physical::{
    AtomicAccessError, AtomicAccessKind, AtomicAccessResult, AtomicBackendFailure, AtomicOrdering,
    AtomicProtocolError, AtomicRequest, AtomicReservationContext, AtomicTargetRejection,
    AtomicUnknownCompletion, ConditionalStatus, PhysicalTargetRejectionReason, PhysicalWidth,
};

const RAM_BASE: u64 = 0x8000_0000;
const ROOT_PPN: u64 = 0x80000;
const DATA_PPN: u64 = 0x80004;
const OTHER_PPN: u64 = 0x80005;
const VA: u64 = 0x1000;
const LEAF: u64 = ((ROOT_PPN + 2) << 12) + 8;
const SATP: u64 = (8 << 60) | ROOT_PPN;
const A: u64 = 1 << 6;
const D: u64 = 1 << 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Lr(u64),
    Sc(u64, Option<ConditionalStatus>),
    Write(u64),
}

#[derive(Debug, Clone, Copy)]
enum Stage {
    /// Before the walk's n-th load-reserved (1-based).
    BeforeLr(usize),
    /// After the walk's n-th load-reserved returned, before any store-conditional.
    AfterLr(usize),
}

type Hook = Box<dyn FnMut(Stage, &mut PhysicalMemory) + Send + Sync>;

/// Shared RAM with injection points around the walk's atomic pair.
struct Racy {
    ram: PhysicalMemory,
    events: Vec<Event>,
    lrs: usize,
    hook: Hook,
    inject: Option<(AtomicAccessKind, AtomicAccessError)>,
}

impl Racy {
    fn new(leaf: u64) -> Self {
        let mut ram = PhysicalMemory::new(RAM_BASE, 0x10_000);
        ram.write_dword(
            ROOT_PPN << 12,
            PageTableEntry::new_pointer(ROOT_PPN + 1).bits(),
        )
        .unwrap();
        ram.write_dword(
            (ROOT_PPN + 1) << 12,
            PageTableEntry::new_pointer(ROOT_PPN + 2).bits(),
        )
        .unwrap();
        ram.write_dword(LEAF, leaf).unwrap();
        Self {
            ram,
            events: Vec::new(),
            lrs: 0,
            hook: Box::new(|_, _| {}),
            inject: None,
        }
    }

    fn with_hook(
        mut self,
        hook: impl FnMut(Stage, &mut PhysicalMemory) + Send + Sync + 'static,
    ) -> Self {
        self.hook = Box::new(hook);
        self
    }

    fn leaf(&self) -> u64 {
        self.ram.read_dword(LEAF).unwrap()
    }

    fn count(&self, matches: impl Fn(&Event) -> bool) -> usize {
        self.events.iter().filter(|event| matches(event)).count()
    }
}

impl PhysicalMemoryInterface for Racy {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        self.ram.read_byte(paddr)
    }

    fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        self.events.push(Event::Write(paddr));
        self.ram.write_byte(paddr, value)
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        self.ram.read_dword(paddr)
    }

    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        self.events.push(Event::Write(paddr));
        self.ram.write_dword(paddr, value)
    }

    fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
        let kind = request.kind();
        let paddr = request.paddr();
        if kind == AtomicAccessKind::LoadReserved {
            self.lrs += 1;
            (self.hook)(Stage::BeforeLr(self.lrs), &mut self.ram);
        }
        let result = match &self.inject {
            Some((injected, error)) if *injected == kind => Err(error.clone()),
            _ => self.ram.access_atomic(request),
        };
        match kind {
            AtomicAccessKind::LoadReserved => {
                self.events.push(Event::Lr(paddr));
                if result.is_ok() {
                    (self.hook)(Stage::AfterLr(self.lrs), &mut self.ram);
                }
            }
            AtomicAccessKind::StoreConditional => self.events.push(Event::Sc(
                paddr,
                result.as_ref().ok().and_then(|r| r.conditional_status()),
            )),
            AtomicAccessKind::Rmw => panic!("the walk must not use the RMW transform"),
        }
        result
    }
}

fn leaf(perms: PagePermissions) -> u64 {
    PageTableEntry::new_leaf(DATA_PPN, perms, false).bits()
}

fn walk(memory: &mut Racy, access: AccessType) -> Result<u64, MmuError> {
    AddressTranslator::new(MmuConfig::default()).translate_sv39_walk(
        VA,
        SATP,
        access,
        PrivilegeMode::Supervisor,
        memory,
    )
}

fn request(access: AccessType) -> TranslationRequest {
    TranslationRequest {
        vaddr: VA,
        access_type: access,
        privilege: PrivilegeMode::Supervisor,
        satp: SATP,
        mstatus: 0,
    }
}

fn walk_page_fault() -> MmuError {
    MmuError::PageFault(PageFaultReason::PageTableWalk {
        level: 0,
        vaddr: VA,
    })
}

fn guest_lr(ram: &mut PhysicalMemory) -> AtomicReservationContext {
    let lr =
        AtomicRequest::load_reserved(LEAF, PhysicalWidth::Doubleword, AtomicOrdering::default())
            .unwrap();
    let reserved = lr.span();
    let snapshot = ram.access_atomic(lr).unwrap().snapshot().unwrap();
    AtomicReservationContext { reserved, snapshot }
}

fn guest_sc(
    ram: &mut PhysicalMemory,
    value: u64,
    context: AtomicReservationContext,
) -> ConditionalStatus {
    let payload = value.to_le_bytes();
    let sc = AtomicRequest::store_conditional(
        LEAF,
        PhysicalWidth::Doubleword,
        AtomicOrdering::default(),
        &payload,
        context,
    )
    .unwrap();
    ram.access_atomic(sc).unwrap().conditional_status().unwrap()
}

#[test]
fn host_write_between_lr_and_sc_fails_sc_and_rewalks_new_value() {
    let original = leaf(PagePermissions::rw());
    let remapped = PageTableEntry::new_leaf(OTHER_PPN, PagePermissions::rw(), false).bits();
    let mut memory = Racy::new(original).with_hook(move |stage, ram| {
        if let Stage::AfterLr(1) = stage {
            ram.write_dword(LEAF, remapped).unwrap();
        }
    });

    assert_eq!(walk(&mut memory, AccessType::Read), Ok(OTHER_PPN << 12));

    assert_eq!(
        memory.events,
        [
            Event::Lr(LEAF),
            Event::Sc(LEAF, Some(ConditionalStatus::Failure)),
            Event::Lr(LEAF),
            Event::Sc(LEAF, Some(ConditionalStatus::Success)),
        ]
    );
    assert_eq!(
        memory.leaf(),
        remapped | A,
        "A is set on the host's new PTE"
    );
}

#[test]
fn host_invalidation_between_lr_and_sc_is_never_overwritten() {
    for access in [
        AccessType::Read,
        AccessType::Write,
        AccessType::InstructionFetch,
    ] {
        let perms = if access == AccessType::InstructionFetch {
            PagePermissions::rwx()
        } else {
            PagePermissions::rw()
        };
        let original = leaf(perms);
        let invalid = original & !1;
        let mut memory = Racy::new(original).with_hook(move |stage, ram| {
            if let Stage::AfterLr(1) = stage {
                ram.write_dword(LEAF, invalid).unwrap();
            }
        });

        assert_eq!(
            walk(&mut memory, access),
            Err(walk_page_fault()),
            "{access:?}"
        );
        assert_eq!(
            memory.leaf(),
            invalid,
            "stale PTE bytes replaced the host write"
        );
        assert_eq!(
            memory.events,
            [
                Event::Lr(LEAF),
                Event::Sc(LEAF, Some(ConditionalStatus::Failure))
            ]
        );
    }
}

#[test]
fn host_write_before_lr_mismatch_skips_sc_and_rereads_step_2() {
    let original = leaf(PagePermissions::rw());
    let invalid = original & !1;
    let mut memory = Racy::new(original).with_hook(move |stage, ram| {
        if let Stage::BeforeLr(1) = stage {
            ram.write_dword(LEAF, invalid).unwrap();
        }
    });

    assert_eq!(walk(&mut memory, AccessType::Write), Err(walk_page_fault()));
    assert_eq!(memory.events, [Event::Lr(LEAF)], "a mismatch issues no SC");
    assert_eq!(memory.leaf(), invalid);
}

#[test]
fn same_value_host_write_retries_conservatively_and_succeeds() {
    let original = leaf(PagePermissions::rw());
    let mut memory = Racy::new(original).with_hook(move |stage, ram| {
        if let Stage::AfterLr(1) = stage {
            ram.write_dword(LEAF, original).unwrap();
        }
    });

    assert_eq!(walk(&mut memory, AccessType::Write), Ok(DATA_PPN << 12));
    assert_eq!(
        memory.events,
        [
            Event::Lr(LEAF),
            Event::Sc(LEAF, Some(ConditionalStatus::Failure)),
            Event::Lr(LEAF),
            Event::Sc(LEAF, Some(ConditionalStatus::Success)),
        ]
    );
    assert_eq!(memory.leaf(), original | A | D);
}

#[test]
fn walk_lr_leaves_guest_reservation_and_successful_update_invalidates_it() {
    // The guest SC inside the walk's LR/SC window succeeds, so the walk's
    // LR did not consume or replace the guest reservation.
    let original = leaf(PagePermissions::rw());
    let guest = Arc::new(Mutex::new(None));
    let window = Arc::clone(&guest);
    let mut memory = Racy::new(original).with_hook(move |stage, ram| {
        if let Stage::AfterLr(1) = stage {
            let context = window.lock().unwrap().take().unwrap();
            assert_eq!(guest_sc(ram, original, context), ConditionalStatus::Success);
        }
    });
    *guest.lock().unwrap() = Some(guest_lr(&mut memory.ram));
    assert_eq!(walk(&mut memory, AccessType::Read), Ok(DATA_PPN << 12));
    assert_eq!(memory.leaf(), original | A);

    // A committed A update invalidates an overlapping guest reservation.
    let mut memory = Racy::new(original);
    let context = guest_lr(&mut memory.ram);
    assert_eq!(walk(&mut memory, AccessType::Read), Ok(DATA_PPN << 12));
    assert_eq!(
        guest_sc(&mut memory.ram, 0x1234, context),
        ConditionalStatus::Failure
    );
    assert_eq!(memory.leaf(), original | A);
}

#[test]
fn failed_walk_sc_leaves_guest_reservation_intact() {
    let original = leaf(PagePermissions::rw());
    let remapped = PageTableEntry::new_leaf(OTHER_PPN, PagePermissions::rx(), false).bits();
    let guest = Arc::new(Mutex::new(None));
    let window = Arc::clone(&guest);
    // The guest reserves after the host write, then the walk's SC fails
    // against its stale snapshot. The retry's A write commits afterwards, so
    // check the guest reservation in the window between those two events.
    let mut memory = Racy::new(original).with_hook(move |stage, ram| match stage {
        Stage::AfterLr(1) => {
            ram.write_dword(LEAF, remapped).unwrap();
            *window.lock().unwrap() = Some(guest_lr(ram));
        }
        Stage::BeforeLr(2) => {
            let context = window.lock().unwrap().take().unwrap();
            assert_eq!(guest_sc(ram, remapped, context), ConditionalStatus::Success);
        }
        _ => {}
    });

    assert_eq!(walk(&mut memory, AccessType::Read), Ok(OTHER_PPN << 12));
    assert!(guest.lock().unwrap().is_none(), "guest SC ran");
    assert_eq!(memory.leaf(), remapped | A);
}

#[test]
fn retry_cap_is_a_simulator_diagnostic_not_a_page_fault() {
    let original = leaf(PagePermissions::rw());
    let mut memory = Racy::new(original).with_hook(move |stage, ram| {
        if let Stage::AfterLr(_) = stage {
            // Toggle a software (RSW) bit so every SC sees a committed write.
            let value = ram.read_dword(LEAF).unwrap() ^ (1 << 8);
            ram.write_dword(LEAF, value).unwrap();
        }
    });

    assert_eq!(
        walk(&mut memory, AccessType::Write),
        Err(MmuError::WalkAdRetryLimit(VA))
    );
    assert_eq!(memory.count(|e| matches!(e, Event::Lr(_))), 64);
    assert_eq!(
        memory.count(|e| *e == Event::Sc(LEAF, Some(ConditionalStatus::Failure))),
        64
    );
    assert_eq!(memory.leaf() & (A | D), 0, "no A/D write committed");

    let mut memory = Racy::new(original).with_hook(|stage, ram| {
        if let Stage::AfterLr(_) = stage {
            let value = ram.read_dword(LEAF).unwrap();
            ram.write_dword(LEAF, value).unwrap();
        }
    });
    let mut walker = PageTableWalker::new(&mut memory, ROOT_PPN);
    assert!(matches!(
        walker.walk_check_permissions_and_update_ad(
            VirtualAddress::new(VA).unwrap(),
            AccessType::Read,
            false
        ),
        WalkResult::AdUpdateRetryLimit { level: 0 }
    ));
}

#[test]
fn set_bits_issue_no_lr_sc_or_write() {
    for (initial, access) in [
        (leaf(PagePermissions::rw()) | A | D, AccessType::Write),
        (leaf(PagePermissions::rw()) | A | D, AccessType::Read),
        (leaf(PagePermissions::rw()) | A, AccessType::Read),
        (
            leaf(PagePermissions::rwx()) | A,
            AccessType::InstructionFetch,
        ),
    ] {
        let mut memory = Racy::new(initial);
        assert_eq!(walk(&mut memory, access), Ok(DATA_PPN << 12));
        assert_eq!(memory.events, [], "{access:?} on {initial:#x}");
        assert_eq!(memory.leaf(), initial);
    }
}

#[test]
fn d_is_set_only_when_the_store_passes_every_check() {
    // A read-only page faults at step 8: no A, no D, no atomic access.
    let read_only = leaf(PagePermissions::rx());
    let mut memory = Racy::new(read_only);
    assert_eq!(walk(&mut memory, AccessType::Write), Err(walk_page_fault()));
    assert_eq!(memory.events, []);
    assert_eq!(memory.leaf(), read_only);

    // A supervisor page from user mode faults at step 6.
    let mut memory = Racy::new(leaf(PagePermissions::rw()));
    let result = AddressTranslator::new(MmuConfig::default()).translate_sv39_walk(
        VA,
        SATP,
        AccessType::Write,
        PrivilegeMode::User,
        &mut memory,
    );
    assert_eq!(result, Err(walk_page_fault()));
    assert_eq!(memory.events, []);

    // A load sets A only.
    let mut memory = Racy::new(leaf(PagePermissions::rw()));
    assert_eq!(walk(&mut memory, AccessType::Read), Ok(DATA_PPN << 12));
    assert_eq!(memory.leaf(), leaf(PagePermissions::rw()) | A);
}

#[test]
fn ad_write_survives_a_later_fault_of_the_explicit_store() {
    // The leaf maps a page outside RAM: translation commits A/D, then the
    // explicit store itself is rejected by the target.
    let outside = PageTableEntry::new_leaf(0x90000, PagePermissions::rw(), false).bits();
    let mut memory = Racy::new(outside);
    let paddr = walk(&mut memory, AccessType::Write).unwrap();
    assert_eq!(paddr, 0x9000_0000);
    assert!(memory.write_dword(paddr, 1).is_err());
    assert_eq!(memory.leaf(), outside | A | D, "A/D are not rolled back");
}

#[test]
fn store_hitting_a_clean_tlb_entry_sets_d_in_memory() {
    let translator = AddressTranslator::new(MmuConfig::default());
    let mut tlb = Tlb::new(64, 4);
    let original = leaf(PagePermissions::rw());
    let mut memory = Racy::new(original);

    assert_eq!(
        translator.translate_with_tlb(request(AccessType::Read), &mut tlb, &mut memory),
        Ok(DATA_PPN << 12)
    );
    assert_eq!(memory.leaf(), original | A);
    let after_read = memory.events.len();

    // The cached entry has D=0, so the store walks and performs step 9.
    assert_eq!(
        translator.translate_with_tlb(request(AccessType::Write), &mut tlb, &mut memory),
        Ok(DATA_PPN << 12)
    );
    assert_eq!(memory.leaf(), original | A | D);
    assert_eq!(
        memory.events[after_read..],
        [
            Event::Lr(LEAF),
            Event::Sc(LEAF, Some(ConditionalStatus::Success))
        ]
    );

    // The refilled entry is dirty: later stores and loads hit without memory.
    let hits = tlb.stats().hits;
    let events = memory.events.len();
    for access in [AccessType::Write, AccessType::Read] {
        assert_eq!(
            translator.translate_with_tlb(request(access), &mut tlb, &mut memory),
            Ok(DATA_PPN << 12)
        );
    }
    assert_eq!(tlb.stats().hits, hits + 2);
    assert_eq!(memory.events.len(), events);
}

#[test]
fn store_hitting_a_clean_tlb_entry_rewalks_memory_not_the_cache() {
    // Software made the page read-only without SFENCE.VMA; the D update
    // must come from memory, which now faults instead of writing.
    let translator = AddressTranslator::new(MmuConfig::default());
    let mut tlb = Tlb::new(64, 4);
    let mut memory = Racy::new(leaf(PagePermissions::rw()));
    translator
        .translate_with_tlb(request(AccessType::Read), &mut tlb, &mut memory)
        .unwrap();
    let read_only = leaf(PagePermissions::rx()) | A;
    memory.ram.write_dword(LEAF, read_only).unwrap();

    assert_eq!(
        translator.translate_with_tlb(request(AccessType::Write), &mut tlb, &mut memory),
        Err(walk_page_fault())
    );
    assert_eq!(memory.leaf(), read_only);
}

fn rejected(kind: AtomicAccessKind) -> AtomicAccessError {
    let request =
        ruscv_sim::physical::AtomicRequestDescriptor::new(LEAF, PhysicalWidth::Doubleword, kind);
    AtomicAccessError::TargetRejected(AtomicTargetRejection::new(
        request,
        PhysicalTargetRejectionReason::Unmapped,
        "injected",
    ))
}

#[test]
fn lr_and_sc_failures_classify_like_walk_reads() {
    for kind in [
        AtomicAccessKind::LoadReserved,
        AtomicAccessKind::StoreConditional,
    ] {
        let request = ruscv_sim::physical::AtomicRequestDescriptor::new(
            LEAF,
            PhysicalWidth::Doubleword,
            kind,
        );
        let cases = [
            (rejected(kind), MmuError::AccessFault(VA)),
            (
                AtomicAccessError::BackendFailure(AtomicBackendFailure {
                    request,
                    context: "injected".into(),
                }),
                MmuError::WalkHostFault(VA),
            ),
            (
                AtomicAccessError::Protocol(AtomicProtocolError::EmptySnapshot),
                MmuError::WalkProtocolFault(VA),
            ),
            (
                AtomicAccessError::UnknownCompletion(AtomicUnknownCompletion {
                    request,
                    context: "injected".into(),
                }),
                MmuError::WalkUnknownFault(VA),
            ),
        ];
        for (error, expected) in cases {
            for access in [AccessType::Read, AccessType::Write] {
                let original = leaf(PagePermissions::rw());
                let mut memory = Racy::new(original);
                memory.inject = Some((kind, error.clone()));
                assert_eq!(
                    walk(&mut memory, access),
                    Err(expected_error(&expected)),
                    "{kind:?} {access:?}"
                );
                assert_eq!(memory.leaf(), original, "no write on {kind:?} failure");
                let atomics = memory.count(|e| !matches!(e, Event::Write(_)));
                let expected_atomics = if kind == AtomicAccessKind::LoadReserved {
                    1
                } else {
                    2
                };
                assert_eq!(atomics, expected_atomics, "no retry after a failure");
            }
        }
    }
}

fn expected_error(error: &MmuError) -> MmuError {
    match error {
        MmuError::AccessFault(va) => MmuError::AccessFault(*va),
        MmuError::WalkHostFault(va) => MmuError::WalkHostFault(*va),
        MmuError::WalkProtocolFault(va) => MmuError::WalkProtocolFault(*va),
        MmuError::WalkUnknownFault(va) => MmuError::WalkUnknownFault(*va),
        other => panic!("unexpected expectation {other:?}"),
    }
}

/// Page-table memory with no atomic capability, standing in for a region
/// without hardware page-table write support.
struct NoAtomics(HashMap<u64, u64>);

impl PhysicalMemoryInterface for NoAtomics {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        Err(MemoryError::InvalidAddress(paddr))
    }

    fn write_byte(&mut self, paddr: u64, _value: u8) -> Result<(), MemoryError> {
        Err(MemoryError::InvalidAddress(paddr))
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        Ok(self.0.get(&paddr).copied().unwrap_or(0))
    }

    fn write_dword(&mut self, paddr: u64, _value: u64) -> Result<(), MemoryError> {
        panic!("the walk must not write PTEs with an ordinary store at {paddr:#x}")
    }
}

#[test]
fn memory_without_atomic_capability_is_an_access_fault_only_when_step_9_runs() {
    let table = |leaf_bits| {
        NoAtomics(HashMap::from([
            (
                ROOT_PPN << 12,
                PageTableEntry::new_pointer(ROOT_PPN + 1).bits(),
            ),
            (
                (ROOT_PPN + 1) << 12,
                PageTableEntry::new_pointer(ROOT_PPN + 2).bits(),
            ),
            (LEAF, leaf_bits),
        ]))
    };
    let translator = AddressTranslator::new(MmuConfig::default());
    for access in [AccessType::Read, AccessType::Write] {
        let mut memory = table(leaf(PagePermissions::rw()));
        assert_eq!(
            translator.translate_sv39_walk(
                VA,
                SATP,
                access,
                PrivilegeMode::Supervisor,
                &mut memory
            ),
            Err(MmuError::AccessFault(VA))
        );
        let mut memory = table(leaf(PagePermissions::rw()) | A | D);
        assert_eq!(
            translator.translate_sv39_walk(
                VA,
                SATP,
                access,
                PrivilegeMode::Supervisor,
                &mut memory
            ),
            Ok(DATA_PPN << 12)
        );
    }
}
