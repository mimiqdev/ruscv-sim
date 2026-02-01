//! Sv39 page table implementation

use super::pte::{PageTableEntry, flags};
use super::{MmuError, AccessType, PrivilegeMode};

/// Sv39 virtual address
#[derive(Debug, Clone, Copy)]
pub struct VirtualAddress(u64);

impl VirtualAddress {
    pub const VPN_WIDTH: u32 = 9;
    pub const PAGE_OFFSET_WIDTH: u32 = 12;
    pub const LEVELS: usize = 3;
    pub const VA_BITS: u32 = 39;

    pub fn new(addr: u64) -> Result<Self, MmuError> {
        // Check sign extension
        let sign_bits = addr >> Self::VA_BITS;
        if sign_bits != 0 && sign_bits != 0x1F_FFFF {
            return Err(MmuError::InvalidVirtualAddress(addr));
        }
        Ok(Self(addr))
    }

    pub fn bits(&self) -> u64 {
        self.0
    }

    pub fn page_offset(&self) -> u64 {
        self.0 & ((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }

    pub fn vpn(&self, level: usize) -> u64 {
        assert!(level < Self::LEVELS);
        let shift = Self::PAGE_OFFSET_WIDTH + (level as u32) * Self::VPN_WIDTH;
        (self.0 >> shift) & ((1 << Self::VPN_WIDTH) - 1)
    }

    pub fn vpns(&self) -> [u64; 3] {
        [self.vpn(2), self.vpn(1), self.vpn(0)]
    }

    pub fn page_aligned(&self) -> u64 {
        self.0 & !((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }
}

/// Sv39 page table walker
pub struct Sv39;

impl Sv39 {
    /// Page table entry size in bytes
    pub const PTE_SIZE: u64 = 8;
    /// Number of entries per page table
    pub const ENTRIES_PER_TABLE: usize = 512;

    /// Calculate PTE address
    pub fn pte_address(root_ppn: u64, vaddr: VirtualAddress, level: usize) -> u64 {
        let vpn = vaddr.vpn(level);
        (root_ppn << 12) + (vpn * Self::PTE_SIZE)
    }

    /// Build physical address from PTE and virtual address
    pub fn build_physical_address(pte: &PageTableEntry, vaddr: VirtualAddress, level: usize) -> u64 {
        let ppn = pte.ppn();
        let offset = vaddr.page_offset();
        
        match level {
            0 => {
                // 4KB page: use all PPN bits
                (ppn << 12) | offset
            }
            1 => {
                // 2MB megapage: VPN[0] becomes part of offset
                let vpn0 = vaddr.vpn(0);
                ((ppn << 12) | (vpn0 << 12)) | offset
            }
            2 => {
                // 1GB gigapage: VPN[1:0] become part of offset
                let vpn0 = vaddr.vpn(0);
                let vpn1 = vaddr.vpn(1);
                ((ppn << 12) | (vpn1 << 21) | (vpn0 << 12)) | offset
            }
            _ => panic!("Invalid page table level: {}", level),
        }
    }

    /// Check if page size is valid for level
    pub fn is_valid_page_size(level: usize) -> bool {
        matches!(level, 0 | 1 | 2)
    }

    /// Get page size for level
    pub fn page_size(level: usize) -> u64 {
        match level {
            0 => 4096,           // 4KB
            1 => 2 * 1024 * 1024, // 2MB
            2 => 1024 * 1024 * 1024, // 1GB
            _ => panic!("Invalid level"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Invalid sign extension
        let result = VirtualAddress::new(0x0000_0080_0000_0000);
        assert!(result.is_err());
    }

    #[test]
    fn test_vpn_extraction() {
        // 0x0040_0000 = VPN2=0, VPN1=2, VPN0=0
        let va = VirtualAddress::new(0x0040_0000).unwrap();
        assert_eq!(va.vpn(2), 0);
        assert_eq!(va.vpn(1), 2);
        assert_eq!(va.vpn(0), 0);
    }

    #[test]
    fn test_pte_address() {
        // Root PPN = 0x80000, VPN2 = 1
        let root_ppn = 0x80000;
        let va = VirtualAddress::new(0x0020_0000).unwrap(); // VPN2=1
        let pte_addr = Sv39::pte_address(root_ppn, va, 2);
        
        // Expected: (0x80000 << 12) + (1 * 8) = 0x8000_0008
        assert_eq!(pte_addr, (0x80000 << 12) + 8);
    }

    #[test]
    fn test_build_physical_address_4kb() {
        // Level 0 (4KB page)
        let pte = PageTableEntry::new_leaf(0x12345, super::super::pte::PagePermissions::rwx(), false);
        let va = VirtualAddress::new(0xABC).unwrap();
        let paddr = Sv39::build_physical_address(&pte, va, 0);
        
        // Expected: (0x12345 << 12) | 0xABC
        assert_eq!(paddr, (0x12345 << 12) | 0xABC);
    }

    #[test]
    fn test_page_sizes() {
        assert_eq!(Sv39::page_size(0), 4096);
        assert_eq!(Sv39::page_size(1), 2 * 1024 * 1024);
        assert_eq!(Sv39::page_size(2), 1024 * 1024 * 1024);
    }
}
