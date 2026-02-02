//! Address translation integration tests
//!
//! Tests the complete address translation pipeline:
//! - MMU with ITLB/DTLB
//! - AddressTranslator with TLB integration
//! - Page table walk on TLB miss
//! - SFENCE.VMA (TLB flush)
//! - SATP mode switching

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::mmu::physical::PhysicalMemory;
use ruscv_sim::mmu::pte::{PagePermissions, PageTableEntry};
use ruscv_sim::mmu::translator::{AddressTranslator, TranslationRequest};
use ruscv_sim::mmu::{AccessType, Mmu, MmuConfig, MmuError, Satp, Tlb, TranslationMode};

/// Setup a simple 3-level page table for testing
/// Uses root_ppn as the base, with level1 at root_ppn + 1, level0 at root_ppn + 2
fn setup_page_table(memory: &mut PhysicalMemory, root_ppn: u64) {
    // Level 1 and Level 0 table locations (consecutive to root)
    let level1_ppn = root_ppn + 1;
    let level0_ppn = root_ppn + 2;

    // Level 2 (root) - pointer to level 1 at VPN[2]=0
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    // Level 1 - pointer to level 0 at VPN[1]=0
    let level1_pte = PageTableEntry::new_pointer(level0_ppn);
    memory
        .write_dword(level1_ppn << 12, level1_pte.bits())
        .unwrap();

    // Level 0 - multiple entries
    // Entry 0: RWX page at PPN 0x90000 (address 0x9000_0000)
    let pte0 = PageTableEntry::new_leaf(0x90000, PagePermissions::rwx(), false);
    memory.write_dword(level0_ppn << 12, pte0.bits()).unwrap();

    // Entry 1: RW page at PPN 0x90001 (address 0x9000_1000)
    let pte1 = PageTableEntry::new_leaf(0x90001, PagePermissions::rw(), false);
    memory
        .write_dword((level0_ppn << 12) + 8, pte1.bits())
        .unwrap();

    // Entry 2: RX page at PPN 0x90002 (address 0x9000_2000)
    let pte2 = PageTableEntry::new_leaf(0x90002, PagePermissions::rx(), false);
    memory
        .write_dword((level0_ppn << 12) + 16, pte2.bits())
        .unwrap();

    // Entry 3: User RWX page at PPN 0x90003 (address 0x9000_3000)
    let pte3 = PageTableEntry::new_leaf(0x90003, PagePermissions::user_rw(), false);
    memory
        .write_dword((level0_ppn << 12) + 24, pte3.bits())
        .unwrap();
}

#[test]
fn test_mmu_creation() {
    let config = MmuConfig::default();
    let mmu = Mmu::new(config);

    let (itlb_stats, dtlb_stats) = mmu.tlb_stats();
    assert_eq!(itlb_stats.accesses, 0);
    assert_eq!(dtlb_stats.accesses, 0);
}

#[test]
fn test_bare_mode_translation() {
    // Memory: base 0x8000_0000, size 0x1001_0000 (covers up to 0x9001_0000)
    let memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Bare mode: SATP = 0
    let request = TranslationRequest {
        vaddr: 0x8000_0000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Machine,
        satp: 0,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0x8000_0000); // Identity mapping in bare mode
}

#[test]
fn test_sv39_translation_with_tlb_miss() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    // Root PPN: 0x80000 = address 0x8000_0000
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Sv39 mode with root PPN
    let satp = (8u64 << 60) | root_ppn;

    let request = TranslationRequest {
        vaddr: 0x1000, // VPN[0] = 1
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // First access - TLB miss
    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_ok());
    // PPN 0x90001 -> address 0x9000_1000
    assert_eq!(result.unwrap(), 0x9000_1000);

    // Check TLB stats
    let stats = tlb.stats();
    assert_eq!(stats.accesses, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 0);
}

#[test]
fn test_sv39_translation_with_tlb_hit() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // First access - TLB miss
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);

    // Second access - TLB hit
    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0x9000_1000);

    // Check TLB stats
    let stats = tlb.stats();
    assert_eq!(stats.accesses, 2);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 1);
}

#[test]
fn test_translation_instruction_vs_data() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let mmu = Mmu::new(config);

    let satp = (8u64 << 60) | root_ppn;

    // Instruction fetch
    let ifetch_request = TranslationRequest {
        vaddr: 0x0000, // VPN[0] = 0, RWX page at 0x9000_0000
        access_type: AccessType::InstructionFetch,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // Data read
    let read_request = TranslationRequest {
        vaddr: 0x1000, // VPN[0] = 1, RW page at 0x9000_1000
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // Both should succeed
    let result1 = mmu.translate(ifetch_request);
    let result2 = mmu.translate(read_request);

    // Note: Current MMU.translate doesn't use physical memory for translation
    // This tests the API - full translation requires translate_with_tlb
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[test]
fn test_tlb_flush_all() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // First access - TLB miss
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 1);

    // Flush all TLB entries
    tlb.flush_all();

    // Next access should miss again
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 2);
}

#[test]
fn test_tlb_flush_va() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;

    // Access two different pages
    let request1 = TranslationRequest {
        vaddr: 0x0000, // VPN 0
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let request2 = TranslationRequest {
        vaddr: 0x1000, // VPN 1
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // Populate TLB
    let _ = translator.translate_with_tlb(request1, &mut tlb, &memory);
    let _ = translator.translate_with_tlb(request2, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 2);

    // Flush specific VA (VPN 0)
    tlb.flush_va(0); // VPN = 0

    // Access VPN 0 again - should miss
    let _ = translator.translate_with_tlb(request1, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 3);

    // Access VPN 1 again - should hit
    let _ = translator.translate_with_tlb(request2, &mut tlb, &memory);
    assert_eq!(tlb.stats().hits, 1);
}

#[test]
fn test_tlb_flush_asid() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Sv39 mode with ASID 1 and root PPN
    let satp = (8u64 << 60) | (1u64 << 44) | root_ppn;

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // First access - TLB miss
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 1);

    // Flush TLB for ASID 1
    tlb.flush_asid(1);

    // Next access should miss again
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 2);
}

#[test]
fn test_tlb_flush_asid_va() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Sv39 mode with ASID 1 and root PPN
    let satp = (8u64 << 60) | (1u64 << 44) | root_ppn;

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    // First access - TLB miss
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 1);

    // Flush TLB for ASID 1, VPN 1
    tlb.flush_asid_va(1, 1);

    // Next access should miss again
    let _ = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert_eq!(tlb.stats().misses, 2);
}

#[test]
fn test_translation_permission_violation() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;

    // Try to write to RX page (VPN[0] = 2 is RX)
    let write_request = TranslationRequest {
        vaddr: 0x2000, // VPN[0] = 2, RX page
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(write_request, &mut tlb, &memory);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MmuError::PageFault(_)));
}

#[test]
fn test_translation_user_access_violation() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;

    // Try user access to supervisor page (VPN[0] = 0 is S-only)
    let user_request = TranslationRequest {
        vaddr: 0x0000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::User, // User mode
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(user_request, &mut tlb, &memory);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MmuError::PageFault(_)));
}

#[test]
fn test_translation_user_access_allowed() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    setup_page_table(&mut memory, root_ppn);

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;

    // User access to user page (VPN[0] = 3 is user accessible)
    let user_request = TranslationRequest {
        vaddr: 0x3000, // VPN[0] = 3, User page
        access_type: AccessType::Read,
        privilege: PrivilegeMode::User,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(user_request, &mut tlb, &memory);
    assert!(result.is_ok());
    // PPN 0x90003 -> address 0x9000_3000
    assert_eq!(result.unwrap(), 0x9000_3000);
}

#[test]
fn test_satp_parsing() {
    // Bare mode
    let satp = Satp(0);
    assert_eq!(satp.mode(), TranslationMode::Bare);
    assert!(!satp.paging_enabled());
    assert_eq!(satp.asid(), 0);
    assert_eq!(satp.ppn(), 0);

    // Sv39 mode
    let satp = Satp((8u64 << 60) | (0x1234 << 44) | 0xABCDEF);
    assert_eq!(satp.mode(), TranslationMode::Sv39);
    assert!(satp.paging_enabled());
    assert_eq!(satp.asid(), 0x1234);
    assert_eq!(satp.ppn(), 0xABCDEF);
    assert_eq!(satp.root_page_table_addr(), 0xABCDEF << 12);

    // Sv48 mode
    let satp = Satp(9u64 << 60);
    assert_eq!(satp.mode(), TranslationMode::Sv48);
}

#[test]
fn test_translation_mode_from_satp() {
    assert_eq!(TranslationMode::from_satp(0), TranslationMode::Bare);
    assert_eq!(TranslationMode::from_satp(8 << 60), TranslationMode::Sv39);
    assert_eq!(TranslationMode::from_satp(9 << 60), TranslationMode::Sv48);
    assert_eq!(TranslationMode::from_satp(10 << 60), TranslationMode::Sv57);
    assert_eq!(TranslationMode::from_satp(15 << 60), TranslationMode::Bare); // Invalid mode
}

#[test]
fn test_mmu_flush_tlb_all() {
    let config = MmuConfig::default();
    let mut mmu = Mmu::new(config);

    // Populate TLB with some entries through manual insertion
    // (In real use, this would happen through translation)

    // Flush all
    mmu.flush_tlb(None, None);

    // TLB flush count should be incremented
    let (itlb_stats, _dtlb_stats) = mmu.tlb_stats();
    assert_eq!(itlb_stats.flushes, 1); // Flush count incremented
}

#[test]
fn test_mmu_flush_tlb_asid() {
    let config = MmuConfig::default();
    let mut mmu = Mmu::new(config);

    // Flush specific ASID
    mmu.flush_tlb(None, Some(1));
}

#[test]
fn test_mmu_flush_tlb_va() {
    let config = MmuConfig::default();
    let mut mmu = Mmu::new(config);

    // Flush specific VA
    mmu.flush_tlb(Some(0x1000), None);
}

#[test]
fn test_mmu_flush_tlb_asid_va() {
    let config = MmuConfig::default();
    let mut mmu = Mmu::new(config);

    // Flush specific ASID + VA
    mmu.flush_tlb(Some(0x1000), Some(1));
}

#[test]
fn test_sv48_not_supported() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Sv48 mode
    let satp = 9u64 << 60;

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        MmuError::UnsupportedMode(TranslationMode::Sv48)
    ));
}

#[test]
fn test_translation_invalid_virtual_address() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    // Sv39 mode with root PPN 0x80000
    let satp = (8u64 << 60) | 0x80000;

    // Invalid virtual address (bad sign extension)
    let request = TranslationRequest {
        vaddr: 0x0000_0080_0000_0000, // Bit 39 set but high bits not all 1
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        MmuError::InvalidVirtualAddress(_)
    ));
}

#[test]
fn test_translation_large_address() {
    // Memory: base 0x8000_0000, size 0x1001_0000
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    // Root PPN: 0x80000 = address 0x8000_0000
    let root_ppn = 0x80000;

    // Setup page table with high VPN
    // Level 1 and Level 0 at consecutive pages
    let level1_ppn = root_ppn + 1;
    let level0_ppn = root_ppn + 2;

    // Root entry for VPN[2] = 1
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory
        .write_dword((root_ppn << 12) + 8, root_pte.bits())
        .unwrap();

    // Level 1 for VPN[1] = 2
    let level1_pte = PageTableEntry::new_pointer(level0_ppn);
    memory
        .write_dword((level1_ppn << 12) + 16, level1_pte.bits())
        .unwrap();

    // Level 0 for VPN[0] = 3
    // Leaf at PPN 0xA0000 (address 0xA000_0000) - within memory range
    let level0_pte = PageTableEntry::new_leaf(0xA0000, PagePermissions::rwx(), false);
    memory
        .write_dword((level0_ppn << 12) + 24, level0_pte.bits())
        .unwrap();

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);

    let satp = (8u64 << 60) | root_ppn;

    // VA with VPN[2]=1, VPN[1]=2, VPN[0]=3, offset=0xABC
    // VA = (1 << 30) | (2 << 21) | (3 << 12) | 0xABC = 0x4040_3ABC
    let vaddr = (1u64 << 30) | (2 << 21) | (3 << 12) | 0xABC;
    let request = TranslationRequest {
        vaddr,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &memory);
    assert!(result.is_ok());
    // PPN 0xA0000 -> address 0xA000_0000
    assert_eq!(result.unwrap(), (0xA0000 << 12) | 0xABC);
}

#[test]
fn test_mmu_config_default() {
    let config = MmuConfig::default();
    assert_eq!(config.tlb_size, 64);
    assert_eq!(config.tlb_ways, 4);
    assert!(config.enable_sv48);
    assert_eq!(config.pmp_entries, 16);
}

#[test]
fn test_mmu_custom_config() {
    let config = MmuConfig {
        tlb_size: 128,
        tlb_ways: 8,
        enable_sv48: false,
        pmp_entries: 8,
    };

    let mmu = Mmu::new(config);
    let (itlb_stats, dtlb_stats) = mmu.tlb_stats();
    // Both TLBs should be initialized
    assert_eq!(itlb_stats.accesses, 0);
    assert_eq!(dtlb_stats.accesses, 0);
}
