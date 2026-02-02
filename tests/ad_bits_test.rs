//! A/D (Accessed/Dirty) bit integration tests
//!
//! End-to-end tests verifying A/D bit memory writeback behavior:
//! - A bit is set on any access (read/write/execute)
//! - D bit is set on write access
//! - PTE is updated in physical memory
//! - Multiple accesses don't reset A/D bits
//! - Works with different page sizes (4KB, 2MB, 1GB)

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::mmu::physical::PhysicalMemory;
use ruscv_sim::mmu::pte::{PagePermissions, PageTableEntry};
use ruscv_sim::mmu::translator::{AddressTranslator, TranslationRequest};
use ruscv_sim::mmu::{AccessType, MmuConfig, Tlb};

/// Helper function to create SATP value for Sv39 mode
fn make_satv39(root_ppn: u64, asid: u16) -> u64 {
    (8u64 << 60) | ((asid as u64) << 44) | root_ppn
}

/// Helper to read PTE from physical memory and check A/D bits
fn read_pte_bits(memory: &PhysicalMemory, pte_addr: u64) -> (bool, bool) {
    let pte_val = memory.read_dword(pte_addr).unwrap();
    let pte = PageTableEntry::from_raw(pte_val);
    (pte.is_accessed(), pte.is_dirty())
}

/// Setup 3-level page table for 4KB page testing
/// Returns the PTE addresses for each level
fn setup_4kb_page_table(
    memory: &mut PhysicalMemory,
    root_ppn: u64,
    data_ppn: u64,
    perms: PagePermissions,
) -> (u64, u64, u64, u64) {
    let level1_ppn = root_ppn + 1;
    let level0_ppn = root_ppn + 2;

    // Root (level 2) - pointer to level 1 at VPN[2]=0
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    let root_pte_addr = root_ppn << 12;
    memory.write_dword(root_pte_addr, root_pte.bits()).unwrap();

    // Level 1 - pointer to level 0 at VPN[1]=0
    let level1_pte = PageTableEntry::new_pointer(level0_ppn);
    let level1_pte_addr = level1_ppn << 12;
    memory
        .write_dword(level1_pte_addr, level1_pte.bits())
        .unwrap();

    // Level 0 - leaf entry for VPN[0]=1 (VA 0x1000)
    // PTE at offset 8 (VPN[0]=1, each PTE is 8 bytes)
    let level0_pte = PageTableEntry::new_leaf(data_ppn, perms, false);
    let level0_pte_addr = (level0_ppn << 12) + 8;
    memory
        .write_dword(level0_pte_addr, level0_pte.bits())
        .unwrap();

    let data_pte_addr = data_ppn << 12;

    (
        root_pte_addr,
        level1_pte_addr,
        level0_pte_addr,
        data_pte_addr,
    )
}

/// Setup 2-level page table for 2MB megapage testing
fn setup_2mb_page_table(
    memory: &mut PhysicalMemory,
    root_ppn: u64,
    data_ppn: u64,
    perms: PagePermissions,
) -> (u64, u64) {
    let level1_ppn = root_ppn + 1;

    // Root (level 2) - pointer to level 1 at VPN[2]=0
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    let root_pte_addr = root_ppn << 12;
    memory.write_dword(root_pte_addr, root_pte.bits()).unwrap();

    // Level 1 - leaf entry for VPN[1]=1 (megapage, 2MB)
    // VA = VPN[1]=1, VPN[0]=any -> base address 0x0020_0000
    // PTE at offset 8 (VPN[1]=1, each PTE is 8 bytes)
    let level1_pte = PageTableEntry::new_leaf(data_ppn, perms, false);
    let level1_pte_addr = (level1_ppn << 12) + 8;
    memory
        .write_dword(level1_pte_addr, level1_pte.bits())
        .unwrap();

    (root_pte_addr, level1_pte_addr)
}

/// Setup 1-level page table for 1GB gigapage testing
fn setup_1gb_page_table(
    memory: &mut PhysicalMemory,
    root_ppn: u64,
    data_ppn: u64,
    perms: PagePermissions,
) -> u64 {
    // Root (level 2) - leaf entry for VPN[2]=0 (gigapage, 1GB)
    // PTE at offset 0 (VPN[2]=0)
    let root_pte = PageTableEntry::new_leaf(data_ppn, perms, false);
    let root_pte_addr = root_ppn << 12;
    memory.write_dword(root_pte_addr, root_pte.bits()).unwrap();

    root_pte_addr
}

#[test]
fn test_ad_bits_4kb_read_sets_accessed_bit() {
    // Memory: base 0x8000_0000, covers root + level1 + level0 + data
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Setup page table with RW permissions, A/D bits initially clear
    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    // Verify initial PTE has A=0, D=0
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(!a, "Initial A bit should be 0");
    assert!(!d, "Initial D bit should be 0");

    // Perform read access
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    let request = TranslationRequest {
        vaddr: 0x1000, // VPN[0] = 1
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Translation should succeed");

    // Verify A bit is now set, D bit still clear
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after read access");
    assert!(!d, "D bit should still be 0 after read access");
}

#[test]
fn test_ad_bits_4kb_write_sets_accessed_and_dirty_bits() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    // Perform write access
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Translation should succeed");

    // Verify both A and D bits are set
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after write access");
    assert!(d, "D bit should be set after write access");
}

#[test]
fn test_ad_bits_4kb_execute_sets_accessed_bit() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Use RX (read-execute) permissions for code page
    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rx());

    // Perform instruction fetch
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::InstructionFetch,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Translation should succeed");

    // Verify A bit is set, D bit is not set (no write)
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after instruction fetch");
    assert!(!d, "D bit should not be set after instruction fetch");
}

#[test]
fn test_ad_bits_multiple_accesses_preserve_bits() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    // First write access - sets A and D
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let _ = translator.translate_with_tlb(request, &mut tlb, &mut memory);

    let (a1, d1) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a1 && d1, "A and D should be set after first write");

    // Second write access - bits should remain set
    // Need to clear TLB to force page table walk again
    tlb.flush_all();

    let request2 = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let _ = translator.translate_with_tlb(request2, &mut tlb, &mut memory);

    let (a2, d2) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a2, "A bit should remain set");
    assert!(d2, "D bit should remain set");

    // Multiple read accesses - bits should remain set
    for _ in 0..5 {
        tlb.flush_all();
        let read_request = TranslationRequest {
            vaddr: 0x1000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Supervisor,
            satp,
            mstatus: 0,
        };
        let _ = translator.translate_with_tlb(read_request, &mut tlb, &mut memory);
    }

    let (a3, d3) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a3, "A bit should remain set after multiple reads");
    assert!(d3, "D bit should remain set after multiple reads");
}

#[test]
fn test_ad_bits_2mb_megapage() {
    // Memory for root + level1 + data
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    let (_, level1_pte_addr) =
        setup_2mb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    // Verify initial PTE has A=0, D=0
    let (a, d) = read_pte_bits(&memory, level1_pte_addr);
    assert!(!a, "Initial A bit should be 0 for megapage");
    assert!(!d, "Initial D bit should be 0 for megapage");

    // Perform read access
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    // VA for VPN[1]=1, VPN[0]=0: (1 << 21) = 0x0020_0000
    let request = TranslationRequest {
        vaddr: 0x0020_0000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Translation should succeed for megapage");

    // Verify A bit is set, D bit still clear
    let (a, d) = read_pte_bits(&memory, level1_pte_addr);
    assert!(a, "A bit should be set after read to megapage");
    assert!(!d, "D bit should still be 0 after read to megapage");

    // Perform write access
    tlb.flush_all();
    let write_request = TranslationRequest {
        vaddr: 0x0020_0100, // Different offset in same megapage
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(write_request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Write translation should succeed");

    // Verify both bits are set
    let (a, d) = read_pte_bits(&memory, level1_pte_addr);
    assert!(a, "A bit should be set after write to megapage");
    assert!(d, "D bit should be set after write to megapage");
}

#[test]
fn test_ad_bits_1gb_gigapage() {
    // Memory for root + data
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    let root_pte_addr =
        setup_1gb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    // Verify initial PTE has A=0, D=0
    let (a, d) = read_pte_bits(&memory, root_pte_addr);
    assert!(!a, "Initial A bit should be 0 for gigapage");
    assert!(!d, "Initial D bit should be 0 for gigapage");

    // Perform read access
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    // VA for VPN[2]=0: base address 0x0
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Translation should succeed for gigapage");

    // Verify A bit is set, D bit still clear
    let (a, d) = read_pte_bits(&memory, root_pte_addr);
    assert!(a, "A bit should be set after read to gigapage");
    assert!(!d, "D bit should still be 0 after read to gigapage");

    // Perform write access to different offset
    tlb.flush_all();
    let write_request = TranslationRequest {
        vaddr: 0x2000, // Different offset in same gigapage
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(write_request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "Write translation should succeed");

    // Verify both bits are set
    let (a, d) = read_pte_bits(&memory, root_pte_addr);
    assert!(a, "A bit should be set after write to gigapage");
    assert!(d, "D bit should be set after write to gigapage");
}

#[test]
fn test_ad_bits_read_only_page_no_dirty() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Setup read-only page (no write permission)
    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rx());

    // Verify initial PTE has A=0, D=0
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(!a && !d, "Initial A and D bits should be 0");

    // Perform read access
    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok());

    // Verify A bit is set, D bit still clear
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after read");
    assert!(!d, "D bit should still be 0 (read-only page)");
}

#[test]
fn test_ad_bits_tlb_hit_does_not_recheck_pte() {
    // This test verifies that when we have a TLB hit, we don't re-check the PTE
    // (which means the A/D bit update in PTE only happens on TLB miss)
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::rw());

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    // First access - TLB miss, PTE is updated
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let _ = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    let (a1, _d1) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a1, "A bit should be set after first access");

    // Clear A/D bits manually in memory (simulating OS clearing them)
    let pte_val = memory.read_dword(level0_pte_addr).unwrap();
    let cleared_pte = pte_val & !((1 << 6) | (1 << 7)); // Clear A (bit 6) and D (bit 7)
    memory.write_dword(level0_pte_addr, cleared_pte).unwrap();

    // Verify bits are cleared
    let (a_cleared, d_cleared) = read_pte_bits(&memory, level0_pte_addr);
    assert!(!a_cleared && !d_cleared, "A/D bits should be cleared");

    // Second access - TLB hit, should not update PTE
    let request2 = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };

    let _ = translator.translate_with_tlb(request2, &mut tlb, &mut memory);

    // Verify A/D bits are still cleared (TLB hit, no PTE update)
    let (a2, d2) = read_pte_bits(&memory, level0_pte_addr);
    assert!(!a2, "A bit should still be clear (TLB hit, no PTE read)");
    assert!(!d2, "D bit should still be clear");
}

#[test]
fn test_ad_bits_user_access() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Setup user-accessible page
    let (_, _, level0_pte_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn, PagePermissions::user_rw());

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let mut tlb = Tlb::new(64, 4);
    let satp = make_satv39(root_ppn, 0);

    // User read access
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Read,
        privilege: PrivilegeMode::User,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "User read should succeed");

    // Verify A bit is set
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after user read");
    assert!(!d, "D bit should not be set after read");

    // User write access
    tlb.flush_all();
    let write_request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::User,
        satp,
        mstatus: 0,
    };

    let result = translator.translate_with_tlb(write_request, &mut tlb, &mut memory);
    assert!(result.is_ok(), "User write should succeed");

    // Verify both bits are set
    let (a, d) = read_pte_bits(&memory, level0_pte_addr);
    assert!(a, "A bit should be set after user write");
    assert!(d, "D bit should be set after user write");
}

#[test]
fn test_ad_bits_all_page_sizes_consistency() {
    // Test that A/D bit behavior is consistent across all page sizes
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
    let root_ppn = 0x80000;

    let config = MmuConfig::default();
    let translator = AddressTranslator::new(config);
    let satp = make_satv39(root_ppn, 0);

    // Setup all three page sizes
    // 4KB page at VA 0x1000 (VPN[0]=1)
    let data_ppn_4k = 0x90000;
    let (_, _, pte_4k_addr, _) =
        setup_4kb_page_table(&mut memory, root_ppn, data_ppn_4k, PagePermissions::rw());

    // 2MB megapage at VA 0x4000_0000 (VPN[2]=1, VPN[1]=0)
    // Need separate root setup
    let data_ppn_2m = 0xA0000;
    let level1_ppn = root_ppn + 10; // Use different level1 table
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory
        .write_dword((root_ppn << 12) + 8, root_pte.bits())
        .unwrap();
    let pte_2m_addr = level1_ppn << 12;
    let level1_pte = PageTableEntry::new_leaf(data_ppn_2m, PagePermissions::rw(), false);
    memory.write_dword(pte_2m_addr, level1_pte.bits()).unwrap();

    // 1GB gigapage at VA 0x8000_0000 (VPN[2]=2)
    let data_ppn_1g = 0xB0000;
    let pte_1g_addr = (root_ppn << 12) + 16;
    let root_pte_giga = PageTableEntry::new_leaf(data_ppn_1g, PagePermissions::rw(), false);
    memory
        .write_dword(pte_1g_addr, root_pte_giga.bits())
        .unwrap();

    // Test 4KB page
    let mut tlb = Tlb::new(64, 4);
    let request = TranslationRequest {
        vaddr: 0x1000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let _ = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    let (a_4k, d_4k) = read_pte_bits(&memory, pte_4k_addr);
    assert!(a_4k && d_4k, "4KB page: A and D should be set");

    // Test 2MB megapage (VA = (1 << 30) = 0x4000_0000)
    tlb.flush_all();
    let request = TranslationRequest {
        vaddr: 0x4000_0000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let _ = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    let (a_2m, d_2m) = read_pte_bits(&memory, pte_2m_addr);
    assert!(a_2m && d_2m, "2MB megapage: A and D should be set");

    // Test 1GB gigapage (VA = (2 << 30) = 0x8000_0000)
    tlb.flush_all();
    let request = TranslationRequest {
        vaddr: 0x8000_0000,
        access_type: AccessType::Write,
        privilege: PrivilegeMode::Supervisor,
        satp,
        mstatus: 0,
    };
    let _ = translator.translate_with_tlb(request, &mut tlb, &mut memory);
    let (a_1g, d_1g) = read_pte_bits(&memory, pte_1g_addr);
    assert!(a_1g && d_1g, "1GB gigapage: A and D should be set");
}
