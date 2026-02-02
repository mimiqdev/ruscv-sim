//! Sv39 page table walk integration tests
//!
//! Tests Sv39 address translation including:
//! - 4KB page translation (3-level walk)
//! - 2MB megapage translation (2-level walk)
//! - 1GB gigapage translation (1-level walk)
//! - Page fault handling
//! - Permission checking
//! - Accessed/Dirty bits

use ruscv_sim::mmu::physical::PhysicalMemory;
use ruscv_sim::mmu::pte::{flags, PagePermissions, PageTableEntry};
use ruscv_sim::mmu::sv39::{PageTableWalker, Sv39, VirtualAddress, WalkResult};

/// Helper: Setup a 3-level page table for 4KB page translation
fn setup_4kb_page_table(memory: &mut PhysicalMemory, root_ppn: u64, data_ppn: u64, vpn: [u64; 3]) {
    let level1_ppn = root_ppn + 1; // 0x80001 -> 0x8000_1000
    let level0_ppn = root_ppn + 2; // 0x80002 -> 0x8000_2000

    // Level 2 (root) - pointer to level 1
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory
        .write_dword((root_ppn << 12) + vpn[2] * 8, root_pte.bits())
        .unwrap();

    // Level 1 - pointer to level 0
    let level1_pte = PageTableEntry::new_pointer(level0_ppn);
    memory
        .write_dword((level1_ppn << 12) + vpn[1] * 8, level1_pte.bits())
        .unwrap();

    // Level 0 - leaf entry
    let level0_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rwx(), false);
    memory
        .write_dword((level0_ppn << 12) + vpn[0] * 8, level0_pte.bits())
        .unwrap();
}

#[test]
fn test_sv39_4kb_page_translation() {
    // Memory: base 0x8000_0000, size 0x1001_0000 (covers 0x8000_0000 to 0x9001_0000)
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000; // Root at 0x8000_0000
    let data_ppn = 0x90000; // Data at 0x9000_0000

    // Virtual address: VPN[2]=0, VPN[1]=0, VPN[0]=1
    // VA = 0x1000
    let vpn = [1u64, 0, 0];
    setup_4kb_page_table(&mut memory, root_ppn, data_ppn, vpn);

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    match walker.walk(va) {
        WalkResult::Success { paddr, level, .. } => {
            assert_eq!(level, 0); // 4KB page at level 0
            assert_eq!(paddr, data_ppn << 12); // offset = 0
        }
        other => panic!("Expected Success, got {:?}", other),
    }
}

#[test]
fn test_sv39_4kb_page_with_offset() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // VPN[2]=0, VPN[1]=0, VPN[0]=5
    let vpn = [5u64, 0, 0];
    setup_4kb_page_table(&mut memory, root_ppn, data_ppn, vpn);

    let walker = PageTableWalker::new(&memory, root_ppn);
    // VA = (5 << 12) | 0xABC = 0x5000 + 0xABC = 0x5ABC
    let va = VirtualAddress::new(0x5ABC).unwrap();

    match walker.walk(va) {
        WalkResult::Success { paddr, level, .. } => {
            assert_eq!(level, 0);
            assert_eq!(paddr, (data_ppn << 12) | 0xABC);
        }
        other => panic!("Expected Success, got {:?}", other),
    }
}

#[test]
fn test_sv39_2mb_megapage() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let level1_ppn = 0x80001; // Level1 at 0x8000_1000
    let data_ppn = 0x90000; // Megapage at 0x9000_0000

    // Level 2 (root) - pointer to level 1
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    // Level 1 - leaf entry for 2MB megapage
    // VPN[1] = 1, so virtual addresses with VPN[1]=1 will map
    let level1_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rw(), false);
    memory
        .write_dword((level1_ppn << 12) + 8, level1_pte.bits())
        .unwrap(); // VPN[1] = 1

    let walker = PageTableWalker::new(&memory, root_ppn);
    // VA with VPN[2]=0, VPN[1]=1, VPN[0]=3, offset=0xABC
    // VA = (1 << 21) | (3 << 12) | 0xABC = 0x0020_0000 + 0x3000 + 0xABC = 0x0020_3ABC
    let va = VirtualAddress::new(0x0020_3ABC).unwrap();

    match walker.walk(va) {
        WalkResult::Success { paddr, level, .. } => {
            assert_eq!(level, 1); // 2MB page at level 1
                                  // For megapage: paddr = (PPN << 12) | (VPN[0] << 12) | offset
                                  // = (0x90000 << 12) | (3 << 12) | 0xABC = 0x9000_3000 + 0xABC = 0x9003_3ABC
            let expected = (data_ppn << 12) | (3 << 12) | 0xABC;
            assert_eq!(paddr, expected);
        }
        other => panic!("Expected Success, got {:?}", other),
    }
}

#[test]
fn test_sv39_1gb_gigapage() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let data_ppn = 0x90000; // Gigapage at 0x9000_0000

    // Level 2 (root) - leaf entry for 1GB gigapage
    // VPN[2] = 2
    let root_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rx(), false);
    memory
        .write_dword((root_ppn << 12) + 16, root_pte.bits())
        .unwrap(); // VPN[2] = 2

    let walker = PageTableWalker::new(&memory, root_ppn);
    // VA with VPN[2]=2, VPN[1]=5, VPN[0]=3, offset=0xABC
    // VA = (2 << 30) | (5 << 21) | (3 << 12) | 0xABC = 0x8000_0000 + 0x00A0_0000 + 0x3000 + 0xABC
    let va = VirtualAddress::new((2 << 30) | (5 << 21) | (3 << 12) | 0xABC).unwrap();

    match walker.walk(va) {
        WalkResult::Success { paddr, level, .. } => {
            assert_eq!(level, 2); // 1GB page at level 2
                                  // For gigapage: paddr = (PPN << 12) | (VPN[1] << 21) | (VPN[0] << 12) | offset
            let expected = (data_ppn << 12) | (5 << 21) | (3 << 12) | 0xABC;
            assert_eq!(paddr, expected);
        }
        other => panic!("Expected Success, got {:?}", other),
    }
}

#[test]
fn test_sv39_invalid_pte() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;

    // Write invalid (non-existent) PTE at root level
    // PTE = 0 means invalid (V bit not set)
    memory.write_dword(root_ppn << 12, 0).unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    match walker.walk(va) {
        WalkResult::PageFault { level } => {
            assert_eq!(level, 2); // Fault at root level
        }
        other => panic!("Expected PageFault, got {:?}", other),
    }
}

#[test]
fn test_sv39_reserved_permission() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;

    // Write PTE with reserved permission: W=1, R=0
    // This is invalid according to RISC-V spec
    let reserved_pte = PageTableEntry::from_raw(flags::V | flags::W);
    memory
        .write_dword(root_ppn << 12, reserved_pte.bits())
        .unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    match walker.walk(va) {
        WalkResult::PageFault { level } => {
            assert_eq!(level, 2);
        }
        other => panic!(
            "Expected PageFault for reserved permission, got {:?}",
            other
        ),
    }
}

#[test]
fn test_sv39_walk_all_levels_no_leaf() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let level1_ppn = 0x80001;
    let level0_ppn = 0x80002;

    // Setup all pointer entries (no leaf)
    let root_pte = PageTableEntry::new_pointer(level1_ppn);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    let level1_pte = PageTableEntry::new_pointer(level0_ppn);
    memory
        .write_dword(level1_ppn << 12, level1_pte.bits())
        .unwrap();

    // Level 0 has a pointer (not a leaf) - this is invalid
    let level0_pte = PageTableEntry::new_pointer(0x1234);
    memory
        .write_dword(level0_ppn << 12, level0_pte.bits())
        .unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    // Should page fault after walking all levels without finding a leaf
    match walker.walk(va) {
        WalkResult::PageFault { level } => {
            assert_eq!(level, 0); // Fault at level 0 after full walk
        }
        other => panic!("Expected PageFault, got {:?}", other),
    }
}

#[test]
fn test_sv39_permission_check_read() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Create read-only page
    let root_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rx(), false);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    // Read access should succeed
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Read, false) {
        WalkResult::Success { .. } => {}
        other => panic!("Expected Success for read, got {:?}", other),
    }

    // Write access should fail
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Write, false) {
        WalkResult::PageFault { .. } => {}
        other => panic!("Expected PageFault for write on RX page, got {:?}", other),
    }
}

#[test]
fn test_sv39_permission_check_user() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Create supervisor-only page
    let root_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rwx(), false);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    // Supervisor access should succeed
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Read, false) {
        WalkResult::Success { .. } => {}
        other => panic!("Expected Success for supervisor access, got {:?}", other),
    }

    // User access should fail
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Read, true) {
        WalkResult::PageFault { .. } => {}
        other => panic!(
            "Expected PageFault for user access on S-only page, got {:?}",
            other
        ),
    }
}

#[test]
fn test_sv39_user_page() {
    let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

    let root_ppn = 0x80000;
    let data_ppn = 0x90000;

    // Create user-accessible page
    let root_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::user_rw(), false);
    memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

    let walker = PageTableWalker::new(&memory, root_ppn);
    let va = VirtualAddress::new(0x1000).unwrap();

    // User access should succeed
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Read, true) {
        WalkResult::Success { .. } => {}
        other => panic!(
            "Expected Success for user access on U page, got {:?}",
            other
        ),
    }

    // User write should succeed
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::Write, true) {
        WalkResult::Success { .. } => {}
        other => panic!("Expected Success for user write on U page, got {:?}", other),
    }

    // User execute should fail (no X permission)
    match walker.walk_check_permissions(va, ruscv_sim::mmu::AccessType::InstructionFetch, true) {
        WalkResult::PageFault { .. } => {}
        other => panic!(
            "Expected PageFault for user execute on non-X page, got {:?}",
            other
        ),
    }
}

#[test]
fn test_sv39_pte_address_calculation() {
    // Test the PTE address calculation
    let root_ppn = 0x80000;
    let vpn = 5u64;

    let pte_addr = Sv39::pte_address(root_ppn, vpn);
    // Expected: (0x80000 << 12) + (5 * 8) = 0x8000_0000 + 40 = 0x8000_0028
    assert_eq!(pte_addr, (root_ppn << 12) + vpn * 8);
}

#[test]
fn test_sv39_page_size_helpers() {
    assert_eq!(Sv39::page_size(0), 4096);
    assert_eq!(Sv39::page_size(1), 2 * 1024 * 1024);
    assert_eq!(Sv39::page_size(2), 1024 * 1024 * 1024);
}

#[test]
fn test_sv39_alignment_helpers() {
    // 4KB alignment
    assert!(Sv39::is_aligned(0x1000, 0));
    assert!(!Sv39::is_aligned(0x1001, 0));
    assert_eq!(Sv39::align_down(0x1234, 0), 0x1000);
    assert_eq!(Sv39::align_up(0x1234, 0), 0x2000);

    // 2MB alignment
    assert!(Sv39::is_aligned(0x20_0000, 1));
    assert!(!Sv39::is_aligned(0x20_0001, 1));
    assert_eq!(Sv39::align_down(0x30_0000, 1), 0x20_0000);

    // 1GB alignment
    assert!(Sv39::is_aligned(0x4000_0000, 2));
    assert!(!Sv39::is_aligned(0x4000_0001, 2));
}

#[test]
fn test_virtual_address_validation() {
    // Valid addresses (sign extension correct)
    assert!(VirtualAddress::new(0).is_ok());
    assert!(VirtualAddress::new(0x1000).is_ok());
    assert!(VirtualAddress::new(0x0000_003F_FFFF_FFFF).is_ok()); // Max positive Sv39 address

    // Invalid addresses (bad sign extension)
    assert!(VirtualAddress::new(0x0000_0040_0000_0000).is_err()); // Bit 38 set but high bits not 1
    assert!(VirtualAddress::new(0x0000_0080_0000_0000).is_err()); // Bits not all 0 or all 1
}

#[test]
fn test_virtual_address_vpns() {
    // Valid Sv39 address: VPN[2]=1, VPN[1]=1, VPN[0]=1, offset=0
    // VA = (1 << 30) | (1 << 21) | (1 << 12) = 0x0040_0020_1000
    // But this has bit 38 = 1, so we need sign extension (high bits = 1)
    // Use a valid positive address instead: VPN[2]=0, VPN[1]=1, VPN[0]=1
    let va = VirtualAddress::new((1 << 21) | (1 << 12)).unwrap();
    assert_eq!(va.vpn(2), 0);
    assert_eq!(va.vpn(1), 1);
    assert_eq!(va.vpn(0), 1);

    // Check all VPNs - returns [VPN0, VPN1, VPN2]
    let vpns = va.vpns();
    assert_eq!(vpns, [1, 1, 0]); // [VPN0, VPN1, VPN2]
}

#[test]
fn test_virtual_address_page_offset() {
    let va = VirtualAddress::new(0x1234).unwrap();
    assert_eq!(va.page_offset(), 0x234);

    let va = VirtualAddress::new(0xABCD_EF12).unwrap();
    assert_eq!(va.page_offset(), 0xF12);
}

#[test]
fn test_sv39_build_physical_address_4kb() {
    // Level 0: 4KB page
    let pte = PageTableEntry::new_leaf(0x12345, PagePermissions::rwx(), false);
    let va = VirtualAddress::new(0xABC).unwrap();
    let paddr = Sv39::build_physical_address(&pte, va, 0);

    // 4KB: use all PPN bits
    assert_eq!(paddr, (0x12345 << 12) | 0xABC);
}

#[test]
fn test_sv39_build_physical_address_2mb() {
    // Level 1: 2MB page
    let pte = PageTableEntry::new_leaf(0x100, PagePermissions::rwx(), false);
    // VA with VPN0=5, offset=0xABC
    let va = VirtualAddress::new((5 << 12) | 0xABC).unwrap();
    let paddr = Sv39::build_physical_address(&pte, va, 1);

    // 2MB: PPN + VPN0 as part of offset
    assert_eq!(paddr, (0x100 << 12) | (5 << 12) | 0xABC);
}

#[test]
fn test_sv39_build_physical_address_1gb() {
    // Level 2: 1GB page
    let pte = PageTableEntry::new_leaf(0x10, PagePermissions::rwx(), false);
    // VA with VPN1=3, VPN0=5, offset=0xABC
    let va = VirtualAddress::new((3 << 21) | (5 << 12) | 0xABC).unwrap();
    let paddr = Sv39::build_physical_address(&pte, va, 2);

    // 1GB: PPN + VPN1 + VPN0 as part of offset
    assert_eq!(paddr, (0x10 << 12) | (3 << 21) | (5 << 12) | 0xABC);
}
