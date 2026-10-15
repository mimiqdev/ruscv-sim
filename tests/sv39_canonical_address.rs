//! A non-canonical Sv39 address is a page fault before any cache lookup or walk.
//!
//! These tests do not establish public-path translation. They exercise the
//! component translators and `Mmu` through their existing APIs.
use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::memory::MemoryError;
use ruscv_sim::mmu::physical::PhysicalMemoryInterface;
use ruscv_sim::mmu::pte::{PagePermissions, PageTableEntry};
use ruscv_sim::mmu::sv39::VirtualAddress;
use ruscv_sim::mmu::translator::{AddressTranslator, TranslationRequest};
use ruscv_sim::mmu::{AccessType, Mmu, MmuConfig, MmuError, PageFaultReason, Tlb, TlbEntry};

const SV39_MODE: u64 = 8 << 60;
const ROOT_PPN: u64 = 0x80000;

struct PanicOnAccess;

impl PhysicalMemoryInterface for PanicOnAccess {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        panic!("canonical check touched physical byte 0x{paddr:x}");
    }

    fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        panic!("canonical check wrote physical byte 0x{paddr:x} = 0x{value:x}");
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        panic!("canonical check read PTE at 0x{paddr:x}");
    }

    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        panic!("canonical check wrote PTE at 0x{paddr:x} = 0x{value:x}");
    }
}

struct RecordingMemory {
    words: Vec<(u64, u64)>,
    writes: Vec<(u64, u64)>,
}

impl RecordingMemory {
    fn with_words(words: Vec<(u64, u64)>) -> Self {
        Self {
            words,
            writes: Vec::new(),
        }
    }
}

impl PhysicalMemoryInterface for RecordingMemory {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        Err(MemoryError::InvalidAddress(paddr))
    }

    fn write_byte(&mut self, paddr: u64, _value: u8) -> Result<(), MemoryError> {
        Err(MemoryError::InvalidAddress(paddr))
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        self.words
            .iter()
            .find(|(addr, _)| *addr == paddr)
            .map(|(_, value)| *value)
            .ok_or(MemoryError::InvalidAddress(paddr))
    }

    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        self.writes.push((paddr, value));
        Ok(())
    }
}

fn request(vaddr: u64, access: AccessType) -> TranslationRequest {
    TranslationRequest {
        vaddr,
        access_type: access,
        privilege: PrivilegeMode::Supervisor,
        satp: SV39_MODE | ROOT_PPN,
        mstatus: 0,
    }
}

fn accesses() -> [AccessType; 3] {
    [
        AccessType::InstructionFetch,
        AccessType::Read,
        AccessType::Write,
    ]
}

fn assert_stats_unchanged(tlb: &Tlb, before: ruscv_sim::mmu::TlbStats, vaddr: u64) {
    let after = tlb.stats();
    assert_eq!(
        after.accesses, before.accesses,
        "0x{vaddr:016x} looked up the TLB"
    );
    assert_eq!(after.hits, before.hits);
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.flushes, before.flushes);
}

fn assert_noncanonical(result: Result<u64, MmuError>, vaddr: u64, access: AccessType) {
    assert_eq!(
        result,
        Err(MmuError::PageFault(PageFaultReason::NonCanonicalAddress {
            access_type: access,
            vaddr,
        })),
        "0x{vaddr:016x} {:?} did not report a non-canonical page fault",
        access
    );
}

fn malformed_addresses() -> Vec<u64> {
    let mut addresses = Vec::new();
    for offset in [0, 0xABC] {
        addresses.push((1u64 << 38) | offset);
        addresses.push((!(1u64 << 38) & !0xFFF) | offset);
        for bit in 39..=63 {
            addresses.push((1u64 << bit) | offset);
            addresses.push((!(1u64 << bit) & !0xFFF) | offset);
        }
    }
    addresses
}

fn permissive_entry(vpn: u64) -> TlbEntry {
    TlbEntry {
        vpn,
        ppn: 0x200,
        asid: 0,
        global: true,
        read: true,
        write: true,
        execute: true,
        // An S-mode access of a U=1 page faults unless SUM is set, and an
        // S-mode fetch faults even then. This entry is an S page so the
        // canonicity cases are not permission faults.
        user: false,
        accessed: true,
        dirty: true,
    }
}

fn preset_ad_leaves() -> Vec<(u64, u64)> {
    let leaf = PageTableEntry::new_leaf(0x40000, PagePermissions::rwx(), true);
    let bits = leaf.bits() | (1 << 6) | (1 << 7);
    [0, 255, 256, 511]
        .map(|index| ((ROOT_PPN << 12) + index * 8, bits))
        .to_vec()
}

#[test]
fn cached_translation_page_faults_before_tlb_or_walk() {
    let translator = AddressTranslator::new(MmuConfig::default());
    for vaddr in malformed_addresses() {
        for access in accesses() {
            let mut tlb = Tlb::new(64, 4);
            tlb.insert(vaddr >> 12, permissive_entry(vaddr >> 12));
            let before = tlb.stats();
            let mut memory = PanicOnAccess;
            let result =
                translator.translate_with_tlb(request(vaddr, access), &mut tlb, &mut memory);
            assert_noncanonical(result, vaddr, access);
            assert_stats_unchanged(&tlb, before, vaddr);
        }
    }
}

#[test]
fn explicit_walk_page_faults_before_pte_access() {
    let translator = AddressTranslator::new(MmuConfig::default());
    for vaddr in malformed_addresses() {
        for access in accesses() {
            let mut memory = PanicOnAccess;
            let result = translator.translate_sv39_walk(
                vaddr,
                SV39_MODE | ROOT_PPN,
                access,
                PrivilegeMode::Supervisor,
                0,
                &mut memory,
            );
            assert_noncanonical(result, vaddr, access);
        }
    }
}

#[test]
fn legacy_translator_page_faults_without_memory() {
    let translator = AddressTranslator::new(MmuConfig::default());
    let tlb = Tlb::new(64, 4);
    for vaddr in malformed_addresses() {
        for access in accesses() {
            let result = translator.translate(request(vaddr, access), &tlb);
            assert_noncanonical(result, vaddr, access);
        }
    }
}

#[test]
fn mmu_page_faults_before_either_tlb_or_walk() {
    for vaddr in malformed_addresses() {
        for access in accesses() {
            let mut mmu = Mmu::new(MmuConfig::default());
            let mut memory = PanicOnAccess;
            let result = mmu.translate(request(vaddr, access), &mut memory);
            assert_noncanonical(result, vaddr, access);
        }
    }
}

#[test]
fn canonical_boundaries_translate_and_malformed_neighbors_do_not() {
    let translator = AddressTranslator::new(MmuConfig::default());
    let leaves = preset_ad_leaves();
    let canonical = [
        (0x0000_0000_0000_0ABC, 0x4000_0ABC),
        (0x0000_003F_FFFF_FFFF, 0x7FFF_FFFF),
        (0xFFFF_FFC0_0000_0000, 0x4000_0000),
        (0xFFFF_FFFF_FFFF_FABC, 0x7FFF_FABC),
    ];

    for (vaddr, _) in canonical {
        let mut tlb = Tlb::new(64, 4);
        tlb.insert(vaddr >> 12, permissive_entry(vaddr >> 12));
        let seeded = tlb.stats();
        let mut memory = PanicOnAccess;
        let result =
            translator.translate_with_tlb(request(vaddr, AccessType::Write), &mut tlb, &mut memory);
        assert_eq!(result, Ok((0x200 << 12) | (vaddr & 0xFFF)));
        let after = tlb.stats();
        assert_eq!(after.hits, seeded.hits + 1);
        assert_eq!(after.misses, seeded.misses);
        assert_eq!(after.accesses, seeded.accesses + 1);
    }

    for (vaddr, paddr) in canonical {
        let mut memory = RecordingMemory::with_words(leaves.clone());
        let result = translator.translate_sv39_walk(
            vaddr,
            SV39_MODE | ROOT_PPN,
            AccessType::Write,
            PrivilegeMode::Supervisor,
            0,
            &mut memory,
        );
        assert_eq!(result, Ok(paddr));
        assert!(memory.writes.is_empty(), "preset A and D were rewritten");
    }

    let bit38_only = 1u64 << 38;
    let bit38_clear_with_high_ones = !(1u64 << 38);
    for vaddr in [bit38_only | 0xABC, bit38_clear_with_high_ones | 0xABC] {
        let mut tlb = Tlb::new(64, 4);
        tlb.insert(vaddr >> 12, permissive_entry(vaddr >> 12));
        let before = tlb.stats();
        let mut memory = PanicOnAccess;
        let result =
            translator.translate_with_tlb(request(vaddr, AccessType::Read), &mut tlb, &mut memory);
        assert_noncanonical(result, vaddr, AccessType::Read);
        assert_stats_unchanged(&tlb, before, vaddr);
    }
}

#[test]
fn bare_passthrough_parser_and_mode_errors_keep_their_precedence() {
    let translator = AddressTranslator::new(MmuConfig::default());
    let malformed = 0x0000_0080_0000_0000;

    let mut bare = request(malformed, AccessType::Read);
    bare.satp = 0;
    let mut tlb = Tlb::new(64, 4);
    let mut memory = PanicOnAccess;
    assert_eq!(
        translator.translate_with_tlb(bare, &mut tlb, &mut memory),
        Ok(malformed)
    );
    assert_eq!(translator.translate(bare, &tlb), Ok(malformed));

    let mut mmu = Mmu::new(MmuConfig::default());
    assert_eq!(mmu.translate(bare, &mut memory), Ok(malformed));

    assert!(matches!(
        VirtualAddress::new(malformed),
        Err(MmuError::InvalidVirtualAddress(addr)) if addr == malformed
    ));
    assert!(VirtualAddress::new(0x1000).is_ok());
    assert!(VirtualAddress::new(0xFFFF_FFFF_FFFF_FFFF).is_ok());

    let modes = [9u64 << 60, 10u64 << 60, 15u64 << 60];
    for mode in modes {
        let mut request = request(malformed, AccessType::InstructionFetch);
        request.satp = mode;
        let mut tlb = Tlb::new(64, 4);
        let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
        let legacy = translator.translate(request, &tlb);
        assert_eq!(legacy, result);
        let expected = match mode >> 60 {
            9 => MmuError::UnsupportedMode(ruscv_sim::mmu::TranslationMode::Sv48),
            10 => MmuError::UnsupportedMode(ruscv_sim::mmu::TranslationMode::Sv57),
            _ => MmuError::InvalidSatpMode(mode),
        };
        assert_eq!(result, Err(expected));
        assert_eq!(tlb.stats().accesses, 0);
    }

    let walk = translator.translate_sv39_walk(
        malformed,
        9u64 << 60,
        AccessType::Read,
        PrivilegeMode::Supervisor,
        0,
        &mut memory,
    );
    assert_eq!(
        walk,
        Err(MmuError::UnsupportedMode(
            ruscv_sim::mmu::TranslationMode::Sv39
        ))
    );
}
