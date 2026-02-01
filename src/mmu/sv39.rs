//! Sv39 page table implementation
//!
//! Sv39 is the standard 39-bit virtual memory system for RISC-V RV64.
//! It uses a three-level page table with 4KB pages.
//!
//! # Virtual Address Format (Sv39)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ 38      30 │ 29      21 │ 20      12 │ 11                               0 │
//! ├────────────┼────────────┼────────────┼─────────────────────────────────────┤
//! │   VPN[2]   │   VPN[1]   │   VPN[0]   │          Page Offset               │
//! │  (9 bits)  │  (9 bits)  │  (9 bits)  │            (12 bits)               │
//! └────────────┴────────────┴────────────┴─────────────────────────────────────┘
//! ```
//!
//! # Page Table Entry Format
//!
//! See [`PageTableEntry`](super::pte::PageTableEntry) for details.
//!
//! # Page Sizes
//!
//! | Level | Page Size | Description |
//! |-------|-----------|-------------|
//! | 0     | 4KB       | Standard page |
//! | 1     | 2MB       | Megapage |
//! | 2     | 1GB       | Gigapage |

use super::pte::{PagePermissions, PageTableEntry};
use super::MmuError;

/// Sv39 virtual address
#[derive(Debug, Clone, Copy)]
pub struct VirtualAddress(u64);

impl VirtualAddress {
    /// Width of each VPN field in bits
    pub const VPN_WIDTH: u32 = 9;
    /// Width of page offset in bits
    pub const PAGE_OFFSET_WIDTH: u32 = 12;
    /// Number of page table levels
    pub const LEVELS: usize = 3;
    /// Number of bits in virtual address
    pub const VA_BITS: u32 = 39;
    /// Mask for valid virtual address bits
    pub const VA_MASK: u64 = (1 << Self::VA_BITS) - 1;

    /// Create a new virtual address
    ///
    /// Validates that bits [63:39] are all 0 or all 1 (sign extension).
    ///
    /// # Arguments
    /// * `addr` - Raw 64-bit address
    ///
    /// # Returns
    /// Ok(VirtualAddress) if valid, Err(MmuError) if invalid
    ///
    /// # Example
    /// ```
    /// use ruscv_sim::mmu::sv39::VirtualAddress;
    /// let va = VirtualAddress::new(0x1000).unwrap();
    /// assert_eq!(va.page_offset(), 0);
    /// ```
    pub fn new(addr: u64) -> Result<Self, MmuError> {
        // Check sign extension for Sv39:
        // - Sv39 uses bits [38:0] (39 bits total)
        // - Bit 38 is the sign bit
        // - Bits [63:39] (25 bits) must be sign extension of bit 38
        const HIGH_BITS_MASK: u64 = 0x1FFFFFF; // 25 ones
        
        let sign_bit = (addr >> 38) & 1;
        let high_bits = (addr >> 39) & HIGH_BITS_MASK;
        
        if sign_bit == 0 && high_bits != 0 {
            // Positive address but high bits not zero
            return Err(MmuError::InvalidVirtualAddress(addr));
        }
        if sign_bit == 1 && high_bits != HIGH_BITS_MASK {
            // Negative address but high bits not all ones
            return Err(MmuError::InvalidVirtualAddress(addr));
        }
        Ok(Self(addr))
    }

    /// Create a virtual address without validation
    ///
    /// # Safety
    /// Caller must ensure the address is valid Sv39 format
    pub const fn new_unchecked(addr: u64) -> Self {
        Self(addr)
    }

    /// Get the raw address bits
    pub const fn bits(&self) -> u64 {
        self.0
    }

    /// Get the page offset (bits [11:0])
    pub fn page_offset(&self) -> u64 {
        self.0 & ((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }

    /// Get the VPN at the specified level
    ///
    /// # Arguments
    /// * `level` - Page table level (0, 1, or 2)
    ///
    /// # Panics
    /// Panics if level >= LEVELS
    pub fn vpn(&self, level: usize) -> u64 {
        assert!(level < Self::LEVELS, "Invalid page table level: {}", level);
        let shift = Self::PAGE_OFFSET_WIDTH + (level as u32) * Self::VPN_WIDTH;
        (self.0 >> shift) & ((1 << Self::VPN_WIDTH) - 1)
    }

    /// Get all VPNs as [VPN0, VPN1, VPN2]
    /// 
    /// Note: This ordering matches the level index used in page table walks
    /// - vpns[0] = VPN0 (level 0)
    /// - vpns[1] = VPN1 (level 1)  
    /// - vpns[2] = VPN2 (level 2)
    pub fn vpns(&self) -> [u64; 3] {
        [self.vpn(0), self.vpn(1), self.vpn(2)]
    }

    /// Get the page-aligned virtual address
    pub fn page_aligned(&self) -> u64 {
        self.0 & !((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }

    /// Get the virtual page number (VPN)
    ///
    /// This is the address shifted right by page offset bits
    pub fn vpn_all(&self) -> u64 {
        self.0 >> Self::PAGE_OFFSET_WIDTH
    }
}

/// Result of a page table walk
#[derive(Debug, Clone, Copy)]
pub enum WalkResult {
    /// Successful translation
    Success {
        /// Physical address
        paddr: u64,
        /// Page table entry
        pte: PageTableEntry,
        /// Level where the leaf was found
        level: usize,
        /// Address of the PTE (for updating A/D bits)
        pte_addr: u64,
    },
    /// Page fault - invalid PTE
    PageFault {
        /// Level where the fault occurred
        level: usize,
    },
    /// Access fault during walk
    AccessFault {
        /// Level where the fault occurred
        level: usize,
    },
}

/// Sv39 page table operations
pub struct Sv39;

impl Sv39 {
    /// Page table entry size in bytes
    pub const PTE_SIZE: u64 = 8;
    /// Number of entries per page table
    pub const ENTRIES_PER_TABLE: usize = 512;
    /// Page table size in bytes
    pub const PAGE_TABLE_SIZE: usize = 4096;

    /// Calculate the address of a PTE in the page table
    ///
    /// # Arguments
    /// * `table_ppn` - Physical Page Number of the page table
    /// * `vpn` - Virtual Page Number index at this level
    ///
    /// # Returns
    /// Physical address of the PTE
    pub fn pte_address(table_ppn: u64, vpn: u64) -> u64 {
        (table_ppn << 12) + (vpn * Self::PTE_SIZE)
    }

    /// Build physical address from PTE and virtual address
    ///
    /// # Arguments
    /// * `pte` - Page table entry (leaf)
    /// * `vaddr` - Virtual address
    /// * `level` - Page table level where leaf was found
    ///
    /// # Returns
    /// The translated physical address
    pub fn build_physical_address(pte: &PageTableEntry, vaddr: VirtualAddress, level: usize) -> u64 {
        let offset = vaddr.page_offset();

        match level {
            0 => {
                // 4KB page: use all PPN bits
                let ppn = pte.ppn();
                (ppn << 12) | offset
            }
            1 => {
                // 2MB megapage: VPN[0] becomes part of offset
                let ppn = pte.ppn();
                let vpn0 = vaddr.vpn(0);
                ((ppn << 12) | (vpn0 << 12)) | offset
            }
            2 => {
                // 1GB gigapage: VPN[1:0] become part of offset
                let ppn = pte.ppn();
                let vpn0 = vaddr.vpn(0);
                let vpn1 = vaddr.vpn(1);
                ((ppn << 12) | (vpn1 << 21) | (vpn0 << 12)) | offset
            }
            _ => panic!("Invalid page table level: {}", level),
        }
    }

    /// Check if page size is valid for level
    pub const fn is_valid_page_size(level: usize) -> bool {
        matches!(level, 0 | 1 | 2)
    }

    /// Get page size for level
    ///
    /// # Arguments
    /// * `level` - Page table level (0, 1, or 2)
    ///
    /// # Returns
    /// Page size in bytes
    pub const fn page_size(level: usize) -> u64 {
        match level {
            0 => 4096,                    // 4KB
            1 => 2 * 1024 * 1024,         // 2MB
            2 => 1024 * 1024 * 1024,      // 1GB
            _ => panic!("Invalid level"),
        }
    }

    /// Get page mask for level
    ///
    /// Returns a mask for the bits that are part of the offset at this level
    pub const fn page_mask(level: usize) -> u64 {
        Self::page_size(level) - 1
    }

    /// Check if an address is aligned to the page size for the given level
    pub fn is_aligned(addr: u64, level: usize) -> bool {
        addr & Self::page_mask(level) == 0
    }

    /// Align address down to page boundary for the given level
    pub fn align_down(addr: u64, level: usize) -> u64 {
        addr & !Self::page_mask(level)
    }

    /// Align address up to page boundary for the given level
    pub fn align_up(addr: u64, level: usize) -> u64 {
        Self::align_down(addr + Self::page_mask(level), level)
    }
}

/// Sv39 page table walker
///
/// Performs page table walks to translate virtual addresses to physical addresses.
pub struct PageTableWalker<'a, M: super::physical::PhysicalMemoryInterface + ?Sized> {
    /// Physical memory interface for reading page tables
    memory: &'a M,
    /// Root page table PPN
    root_ppn: u64,
}

impl<'a, M: super::physical::PhysicalMemoryInterface + ?Sized> PageTableWalker<'a, M> {
    /// Create a new page table walker
    ///
    /// # Arguments
    /// * `memory` - Physical memory interface
    /// * `root_ppn` - Physical Page Number of the root page table
    pub fn new(memory: &'a M, root_ppn: u64) -> Self {
        Self { memory, root_ppn }
    }

    /// Walk the page table to translate a virtual address
    ///
    /// # Arguments
    /// * `vaddr` - Virtual address to translate
    ///
    /// # Returns
    /// WalkResult indicating success or type of fault
    pub fn walk(&self, vaddr: VirtualAddress) -> WalkResult {
        let vpns = vaddr.vpns();
        let mut ppn = self.root_ppn;

        // Walk from level 2 down to level 0
        for level in (0..VirtualAddress::LEVELS).rev() {
            let pte_addr = Sv39::pte_address(ppn, vpns[level]);
            
            #[cfg(test)]
            eprintln!("Walk level {}: ppn={:x}, vpn={:x}, pte_addr={:016x}", level, ppn, vpns[level], pte_addr);

            // Read the PTE from physical memory
            let pte_val = match self.memory.read_dword(pte_addr) {
                Ok(val) => {
                    #[cfg(test)]
                    eprintln!("Read PTE at level {}: addr={:016x}, val={:016x}", level, pte_addr, val);
                    val
                }
                Err(e) => {
                    #[cfg(test)]
                    eprintln!("AccessFault at level {}: addr={:016x}, error={:?}", level, pte_addr, e);
                    return WalkResult::AccessFault { level };
                }
            };

            let pte = PageTableEntry::from_raw(pte_val);
            
            #[cfg(test)]
            eprintln!("PTE: valid={}, leaf={}, perms_ok={}, bits={:016x}", 
                     pte.is_valid(), pte.is_leaf(), pte.has_valid_permissions(), pte.bits());

            // Check if valid
            if !pte.is_valid() {
                #[cfg(test)]
                eprintln!("PageFault: PTE not valid");
                return WalkResult::PageFault { level };
            }

            // Check if leaf
            if pte.is_leaf() {
                #[cfg(test)]
                eprintln!("Found leaf at level {}", level);
                // Check for reserved permission combination (W=1, R=0)
                if !pte.has_valid_permissions() {
                    return WalkResult::PageFault { level };
                }

                // Build physical address
                let paddr = Sv39::build_physical_address(&pte, vaddr, level);

                return WalkResult::Success {
                    paddr,
                    pte,
                    level,
                    pte_addr,
                };
            }

            // Not a leaf, continue to next level
            ppn = pte.ppn();
        }

        // If we get here, we walked all levels without finding a leaf
        WalkResult::PageFault { level: 0 }
    }

    /// Walk the page table and check permissions
    ///
    /// Similar to walk(), but also checks if the requested access type
    /// is allowed by the page permissions.
    ///
    /// Note: In RISC-V, supervisor mode can access user pages (U=1).
    /// The SSTATUS.SUM bit controls whether supervisor can access user pages,
    /// but for simplicity we allow it here.
    ///
    /// # Arguments
    /// * `vaddr` - Virtual address to translate
    /// * `access_type` - Type of access (read/write/execute)
    /// * `is_user` - Whether this is a user-mode access
    pub fn walk_check_permissions(
        &self,
        vaddr: VirtualAddress,
        access_type: super::AccessType,
        is_user: bool,
    ) -> WalkResult {
        match self.walk(vaddr) {
            WalkResult::Success {
                paddr,
                pte,
                level,
                pte_addr,
            } => {
                let perms = pte.permissions();

                // Check user permission: user mode can only access user pages (U=1)
                // Supervisor mode can access all pages (both U=0 and U=1)
                if is_user && !perms.user {
                    return WalkResult::PageFault { level };
                }

                // Check access type permission
                let allowed = match access_type {
                    super::AccessType::Read => perms.read,
                    super::AccessType::Write => perms.write,
                    super::AccessType::InstructionFetch => perms.execute,
                };

                if !allowed {
                    return WalkResult::PageFault { level };
                }

                WalkResult::Success {
                    paddr,
                    pte,
                    level,
                    pte_addr,
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::physical::{PhysicalMemory, PhysicalMemoryInterface};
    use crate::mmu::pte::flags;

    #[test]
    fn test_virtual_address() {
        let va = VirtualAddress::new(0x1000).unwrap();
        assert_eq!(va.page_offset(), 0);
        assert_eq!(va.vpn(0), 1);

        let va = VirtualAddress::new(0x1234).unwrap();
        assert_eq!(va.page_offset(), 0x234);
    }

    #[test]
    fn test_invalid_virtual_address() {
        // Invalid sign extension - bits [63:39] not all 0 or all 1
        let result = VirtualAddress::new(0x0000_0080_0000_0000);
        assert!(result.is_err());

        // Valid: all zeros in high bits
        assert!(VirtualAddress::new(0x0000_0000_0000_0000).is_ok());
        // Valid: all ones in high bits (sign extended negative)
        assert!(VirtualAddress::new(0xFFFF_FFFF_FFFF_FFFF).is_ok());
    }

    #[test]
    fn test_vpn_extraction() {
        // 0x0040_0000 = VPN2=0, VPN1=2, VPN0=0
        let va = VirtualAddress::new(0x0040_0000).unwrap();
        assert_eq!(va.vpn(2), 0);
        assert_eq!(va.vpn(1), 2);
        assert_eq!(va.vpn(0), 0);

        // Check all VPNs
        let vpns = va.vpns();
        assert_eq!(vpns, [0, 2, 0]);
    }

    #[test]
    fn test_pte_address() {
        // Root PPN = 0x80000, VPN2 = 0 for address 0x0020_0000
        let root_ppn = 0x80000;
        let va = VirtualAddress::new(0x0020_0000).unwrap(); // VPN2=0, VPN1=1
        let pte_addr = Sv39::pte_address(root_ppn, va.vpn(2));

        // Expected: (0x80000 << 12) + (0 * 8) = 0x8000_0000
        assert_eq!(pte_addr, 0x80000 << 12);

        // Test with address that has VPN2 = 1 (0x4000_0000 and above)
        let va2 = VirtualAddress::new(0x4000_0000).unwrap(); // VPN2=1
        let pte_addr2 = Sv39::pte_address(root_ppn, va2.vpn(2));
        // Expected: (0x80000 << 12) + (1 * 8) = 0x8000_0008
        assert_eq!(pte_addr2, (0x80000 << 12) + 8);
    }

    #[test]
    fn test_build_physical_address_4kb() {
        // Level 0 (4KB page)
        let pte = PageTableEntry::new_leaf(0x12345, PagePermissions::rwx(), false);
        let va = VirtualAddress::new(0xABC).unwrap();
        let paddr = Sv39::build_physical_address(&pte, va, 0);

        // Expected: (0x12345 << 12) | 0xABC
        assert_eq!(paddr, (0x12345 << 12) | 0xABC);
    }

    #[test]
    fn test_build_physical_address_2mb() {
        // Level 1 (2MB page)
        let pte = PageTableEntry::new_leaf(0x100, PagePermissions::rwx(), false);
        // VA with VPN0=5, offset=0xABC
        let va = VirtualAddress::new((5 << 12) | 0xABC).unwrap();
        let paddr = Sv39::build_physical_address(&pte, va, 1);

        // For 2MB page: PPN full + VPN0 as part of offset
        // Expected: (0x100 << 12) | (5 << 12) | 0xABC = (0x100 << 12) | (5 << 12) | 0xABC
        assert_eq!(paddr, (0x100 << 12) | (5 << 12) | 0xABC);
    }

    #[test]
    fn test_build_physical_address_1gb() {
        // Level 2 (1GB page)
        let pte = PageTableEntry::new_leaf(0x10, PagePermissions::rwx(), false);
        // VA with VPN1=3, VPN0=5, offset=0xABC
        let va = VirtualAddress::new((3 << 21) | (5 << 12) | 0xABC).unwrap();
        let paddr = Sv39::build_physical_address(&pte, va, 2);

        // For 1GB page: PPN full + VPN1 + VPN0 as part of offset
        assert_eq!(paddr, (0x10 << 12) | (3 << 21) | (5 << 12) | 0xABC);
    }

    #[test]
    fn test_page_sizes() {
        assert_eq!(Sv39::page_size(0), 4096);
        assert_eq!(Sv39::page_size(1), 2 * 1024 * 1024);
        assert_eq!(Sv39::page_size(2), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_page_alignment() {
        assert!(Sv39::is_aligned(0x1000, 0));
        assert!(!Sv39::is_aligned(0x1001, 0));

        assert!(Sv39::is_aligned(0x20_0000, 1));
        assert!(!Sv39::is_aligned(0x20_0001, 1));

        assert!(Sv39::is_aligned(0x4000_0000, 2));
        assert!(!Sv39::is_aligned(0x4000_0001, 2));
    }

    #[test]
    fn test_page_alignment_helpers() {
        assert_eq!(Sv39::align_down(0x1234, 0), 0x1000);
        assert_eq!(Sv39::align_up(0x1234, 0), 0x2000);

        assert_eq!(Sv39::align_down(0x1000, 0), 0x1000);
        assert_eq!(Sv39::align_up(0x1000, 0), 0x1000);
    }

    #[test]
    fn test_trait_object_access() {
        use crate::mmu::physical::PhysicalMemoryInterface;
        
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x10000);
        
        // Write data through trait object
        let iface: &mut dyn PhysicalMemoryInterface = &mut mem;
        iface.write_dword(0x8000_0000, 0x1234_5678_9ABC_DEF0).unwrap();
        
        // Read back through trait object
        let val = iface.read_dword(0x8000_0000).unwrap();
        assert_eq!(val, 0x1234_5678_9ABC_DEF0);
    }
    
    #[test]
    fn test_trait_object_address_passing() {
        use crate::mmu::physical::PhysicalMemoryInterface;
        
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x10000);
        
        // Test address passing through trait object
        let iface: &dyn PhysicalMemoryInterface = &mem;
        
        // Direct call
        let direct_result = mem.read_dword(0x8000_0000);
        println!("Direct call result: {:?}", direct_result);
        
        // Trait object call with same address
        let trait_result = iface.read_dword(0x8000_0000);
        println!("Trait object call result: {:?}", trait_result);
        
        // Both should succeed
        assert!(direct_result.is_ok(), "Direct call failed: {:?}", direct_result);
        assert!(trait_result.is_ok(), "Trait object call failed: {:?}", trait_result);
    }

    #[test]
    fn test_page_table_walk_basic() {
        use crate::mmu::physical::PhysicalMemoryInterface;
        
        // Set up physical memory with a simple page table
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x10000);

        // Root page table at PPN = 0x80000 (physical address 0x8000_0000)
        // Note: PPN = PA >> 12, so 0x8000_0000 >> 12 = 0x80000
        let root_ppn = 0x80000u64;

        // Create a level 2 entry pointing to a 1GB page
        // PTE: Valid, Readable, Writable, PPN = 0x90000
        let pte = PageTableEntry::new_leaf(0x90000, PagePermissions::rw(), false);
        let pte_bytes = pte.bits().to_le_bytes();

        // Write PTE at index 0 (VPN2 = 0)
        // PTE address = (0x80000 << 12) + (0 * 8) = 0x8000_0000
        mem.write_bytes(0x8000_0000u64, &pte_bytes).unwrap();

        // Create walker and test - explicitly cast to trait object
        let walker = PageTableWalker::new(&mem as &dyn PhysicalMemoryInterface, root_ppn);

        // Test VA = 0 (VPN2=0, VPN1=0, VPN0=0, offset=0)
        let va = VirtualAddress::new(0).unwrap();
        match walker.walk(va) {
            WalkResult::Success { paddr, level, .. } => {
                // For 1GB page at PPN 0x90000, VA 0 -> PA 0x9000_0000
                assert_eq!(paddr, 0x9000_0000);
                assert_eq!(level, 2);
            }
            other => panic!("Expected Success, got {:?}", other),
        }

        // Test VA with offset
        let va = VirtualAddress::new(0x1234).unwrap();
        match walker.walk(va) {
            WalkResult::Success { paddr, .. } => {
                assert_eq!(paddr, 0x9000_1234);
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_page_table_walk_invalid_pte() {
        // Create memory large enough for root table at 0x8000_0000
        let mem = PhysicalMemory::new(0x8000_0000, 0x20000);
        let root_ppn = 0x80000; // Root at 0x8000_0000

        // Don't write any PTE - all entries are invalid (0)

        let walker = PageTableWalker::new(&mem as &dyn PhysicalMemoryInterface, root_ppn);
        let va = VirtualAddress::new(0).unwrap();

        match walker.walk(va) {
            WalkResult::PageFault { level } => {
                assert_eq!(level, 2); // Fault at level 2
            }
            other => panic!("Expected PageFault, got {:?}", other),
        }
    }

    #[test]
    fn test_page_table_walk_multi_level() {
        // Set up a 3-level page table
        // Need space for: root (0x8000_0000), level1 (0x8000_1000), level0 (0x8000_2000), and data (0x9000_0000)
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

        let root_ppn = 0x80000; // Root at 0x8000_0000
        let level1_ppn = 0x80001; // Level 1 table at 0x8000_1000
        let level0_ppn = 0x80002; // Level 0 table at 0x8000_2000

        // Root entry 0 -> level 1 table
        let root_pte = PageTableEntry::new_pointer(level1_ppn);
        mem.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

        // Level 1 entry 0 -> level 0 table
        let level1_pte = PageTableEntry::new_pointer(level0_ppn);
        mem.write_dword(level1_ppn << 12, level1_pte.bits()).unwrap();

        // Level 0 entry 0 -> 4KB page at 0x9000_0000
        let level0_pte = PageTableEntry::new_leaf(0x90000, PagePermissions::rwx(), false);
        mem.write_dword(level0_ppn << 12, level0_pte.bits()).unwrap();

        let walker = PageTableWalker::new(&mem as &dyn PhysicalMemoryInterface, root_ppn);

        // Test VA = 0
        let va = VirtualAddress::new(0).unwrap();
        match walker.walk(va) {
            WalkResult::Success { paddr, level, .. } => {
                assert_eq!(paddr, 0x9000_0000); // Data at PPN 0x90000
                assert_eq!(level, 0); // Found at level 0
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_walk_with_permission_check() {
        // Need space for root (0x8000_0000) and data (0x9000_0000)
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1001_0000);
        let root_ppn = 0x80000; // Root at 0x8000_0000

        // Create a user-mode executable page (gigapage)
        let pte = PageTableEntry::new_leaf(0x90000, PagePermissions::user_rx(), false);
        mem.write_dword(root_ppn << 12, pte.bits()).unwrap();

        let walker = PageTableWalker::new(&mem as &dyn PhysicalMemoryInterface, root_ppn);
        let va = VirtualAddress::new(0).unwrap();

        // User execute should succeed (page is user+execute)
        match walker.walk_check_permissions(va, super::super::AccessType::InstructionFetch, true) {
            WalkResult::Success { .. } => {}
            other => panic!("Expected Success for user execute, got {:?}", other),
        }

        // User write should fail (page is RX, not RW)
        match walker.walk_check_permissions(va, super::super::AccessType::Write, true) {
            WalkResult::PageFault { .. } => {}
            other => panic!("Expected PageFault for user write, got {:?}", other),
        }

        // Supervisor read from user page should succeed (RISC-V allows this)
        // Note: SSTATUS.SUM bit controls this, but we allow it by default
        match walker.walk_check_permissions(va, super::super::AccessType::Read, false) {
            WalkResult::Success { .. } => {}
            other => panic!("Expected Success for supervisor read from user page, got {:?}", other),
        }
    }
}
