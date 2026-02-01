//! Page Table Entry (PTE) definitions and operations
//!
//! Implements Sv39/Sv48 page table entry format as defined in the RISC-V
//! Privileged Architecture Specification.
//!
//! # Sv39 Page Table Entry Format
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                           Sv39 Page Table Entry (64-bit)                    │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │ 63 │ 62-54 │ 53-28 │ 27-19 │ 18-10 │ 9 │ 8 │ 7 │ 6 │ 5 │ 4 │ 3 │ 2 │ 1 │ 0 │
//! ├────┼───────┼───────┼───────┼───────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┤
//! │ N  │  RSW  │  PPN2 │  PPN1 │  PPN0 │RSW│ D │ A │ G │ U │ X │ W │ R │ V │
//! └────┴───────┴───────┴───────┴───────┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘
//!
//! Bit definitions:
//! - V (0): Valid - entry is valid
//! - R (1): Read - page is readable
//! - W (2): Write - page is writable
//! - X (3): Execute - page is executable
//! - U (4): User - user mode can access
//! - G (5): Global - global mapping
//! - A (6): Accessed - page has been accessed
//! - D (7): Dirty - page has been written
//! - RSW (8-9): Reserved for Software
//! - PPN[2:0] (10-53): Physical Page Number
//! ```

/// Page Table Entry (64-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageTableEntry(u64);

/// Page table entry flags
pub mod flags {
    /// Valid bit
    pub const V: u64 = 1 << 0;
    /// Read permission bit
    pub const R: u64 = 1 << 1;
    /// Write permission bit
    pub const W: u64 = 1 << 2;
    /// Execute permission bit
    pub const X: u64 = 1 << 3;
    /// User mode access bit
    pub const U: u64 = 1 << 4;
    /// Global mapping bit
    pub const G: u64 = 1 << 5;
    /// Accessed bit
    pub const A: u64 = 1 << 6;
    /// Dirty bit
    pub const D: u64 = 1 << 7;
    /// Reserved for Software mask (bits 8-9)
    pub const RSW_MASK: u64 = 0b11 << 8;
    /// Physical Page Number mask (bits 10-53)
    pub const PPN_MASK: u64 = 0xFFFFFFFFFFF << 10;
}

/// Page permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PagePermissions {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
    /// User mode access
    pub user: bool,
}

impl PagePermissions {
    /// All permissions (R/W/X/U)
    pub const fn all() -> Self {
        Self {
            read: true,
            write: true,
            execute: true,
            user: true,
        }
    }

    /// No permissions
    pub const fn none() -> Self {
        Self {
            read: false,
            write: false,
            execute: false,
            user: false,
        }
    }

    /// Read-write permissions
    pub const fn rw() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
            user: false,
        }
    }

    /// Read-execute permissions (code segment)
    pub const fn rx() -> Self {
        Self {
            read: true,
            write: false,
            execute: true,
            user: false,
        }
    }

    /// Read-write-execute permissions
    pub const fn rwx() -> Self {
        Self {
            read: true,
            write: true,
            execute: true,
            user: false,
        }
    }

    /// User read-write permissions
    pub const fn user_rw() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
            user: true,
        }
    }

    /// User read-execute permissions
    pub const fn user_rx() -> Self {
        Self {
            read: true,
            write: false,
            execute: true,
            user: true,
        }
    }
}

impl PageTableEntry {
    const PPN0_SHIFT: u32 = 10;
    const PPN1_SHIFT: u32 = 19;
    const PPN2_SHIFT: u32 = 28;
    const PPN0_MASK: u64 = 0x1FF; // 9 bits
    const PPN1_MASK: u64 = 0x1FF; // 9 bits
    const PPN2_MASK: u64 = 0x3FFFF; // 18 bits

    /// Create a PTE from raw bits
    pub const fn from_raw(bits: u64) -> Self {
        Self(bits)
    }

    /// Get the raw bits of the PTE
    pub const fn bits(&self) -> u64 {
        self.0
    }

    /// Check if the entry is valid (V bit set)
    pub const fn is_valid(&self) -> bool {
        self.0 & flags::V != 0
    }

    /// Check if this is a leaf entry (R, W, or X bit set)
    ///
    /// A leaf entry points to a physical page rather than another level
    /// of the page table.
    pub const fn is_leaf(&self) -> bool {
        self.0 & (flags::R | flags::W | flags::X) != 0
    }

    /// Check if this is a pointer to another level of the page table
    pub const fn is_pointer(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }

    /// Check if this is a valid leaf entry
    pub const fn is_valid_leaf(&self) -> bool {
        self.is_valid() && self.is_leaf()
    }

    /// Get PPN0 (Physical Page Number level 0)
    ///
    /// Used for 4KB page addressing
    pub fn ppn0(&self) -> u64 {
        (self.0 >> Self::PPN0_SHIFT) & Self::PPN0_MASK
    }

    /// Get PPN1 (Physical Page Number level 1)
    ///
    /// Used for 2MB megapage addressing
    pub fn ppn1(&self) -> u64 {
        (self.0 >> Self::PPN1_SHIFT) & Self::PPN1_MASK
    }

    /// Get PPN2 (Physical Page Number level 2)
    ///
    /// Used for 1GB gigapage addressing
    pub fn ppn2(&self) -> u64 {
        (self.0 >> Self::PPN2_SHIFT) & Self::PPN2_MASK
    }

    /// Get the full Physical Page Number
    ///
    /// Combines PPN2, PPN1, and PPN0 into a single 44-bit value
    pub fn ppn(&self) -> u64 {
        self.0 >> 10
    }

    /// Get the physical address (page-aligned)
    ///
    /// Returns the physical address by shifting the PPN left by 12 bits
    pub fn physical_address(&self) -> u64 {
        self.ppn() << 12
    }

    /// Get the permissions from this PTE
    pub fn permissions(&self) -> PagePermissions {
        PagePermissions {
            read: self.0 & flags::R != 0,
            write: self.0 & flags::W != 0,
            execute: self.0 & flags::X != 0,
            user: self.0 & flags::U != 0,
        }
    }

    /// Check if the global bit is set
    pub const fn is_global(&self) -> bool {
        self.0 & flags::G != 0
    }

    /// Check if the accessed bit is set
    pub const fn is_accessed(&self) -> bool {
        self.0 & flags::A != 0
    }

    /// Check if the dirty bit is set
    pub const fn is_dirty(&self) -> bool {
        self.0 & flags::D != 0
    }

    /// Set the accessed bit
    pub fn set_accessed(&mut self) {
        self.0 |= flags::A;
    }

    /// Set the dirty bit
    pub fn set_dirty(&mut self) {
        self.0 |= flags::D;
    }

    /// Clear the accessed bit
    pub fn clear_accessed(&mut self) {
        self.0 &= !flags::A;
    }

    /// Clear the dirty bit
    pub fn clear_dirty(&mut self) {
        self.0 &= !flags::D;
    }

    /// Create a new leaf PTE
    ///
    /// # Arguments
    /// * `ppn` - Physical Page Number
    /// * `perms` - Page permissions
    /// * `global` - Whether this is a global mapping
    pub fn new_leaf(ppn: u64, perms: PagePermissions, global: bool) -> Self {
        let mut bits = (ppn << 10) | flags::V;
        if perms.read {
            bits |= flags::R;
        }
        if perms.write {
            bits |= flags::W;
        }
        if perms.execute {
            bits |= flags::X;
        }
        if perms.user {
            bits |= flags::U;
        }
        if global {
            bits |= flags::G;
        }
        Self(bits)
    }

    /// Create a new pointer PTE (non-leaf)
    ///
    /// # Arguments
    /// * `ppn` - Physical Page Number of the next level page table
    pub fn new_pointer(ppn: u64) -> Self {
        Self((ppn << 10) | flags::V)
    }

    /// Check if the permissions are valid
    ///
    /// RISC-V spec: W=1, R=0 is a reserved combination
    pub const fn has_valid_permissions(&self) -> bool {
        !(self.0 & flags::W != 0 && self.0 & flags::R == 0)
    }

    /// Check if this PTE allows read access
    pub const fn readable(&self) -> bool {
        self.0 & flags::R != 0
    }

    /// Check if this PTE allows write access
    pub const fn writable(&self) -> bool {
        self.0 & flags::W != 0
    }

    /// Check if this PTE allows execute access
    pub const fn executable(&self) -> bool {
        self.0 & flags::X != 0
    }

    /// Check if this PTE allows user access
    pub const fn user_accessible(&self) -> bool {
        self.0 & flags::U != 0
    }

    /// Get the RSW (Reserved for Software) field
    pub fn rsw(&self) -> u64 {
        (self.0 & flags::RSW_MASK) >> 8
    }

    /// Set the RSW (Reserved for Software) field
    ///
    /// # Arguments
    /// * `value` - 2-bit value to store in RSW field
    pub fn set_rsw(&mut self, value: u64) {
        self.0 = (self.0 & !flags::RSW_MASK) | ((value & 0x3) << 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pte_flags() {
        let pte = PageTableEntry::from_raw(flags::V | flags::R | flags::W);
        assert!(pte.is_valid());
        assert!(pte.is_leaf());
        let p = pte.permissions();
        assert!(p.read);
        assert!(p.write);
        assert!(!p.execute);
    }

    #[test]
    fn test_pte_pointer() {
        let pte = PageTableEntry::new_pointer(0x12345);
        assert!(pte.is_valid());
        assert!(pte.is_pointer());
        assert!(!pte.is_leaf());
        assert_eq!(pte.ppn(), 0x12345);
    }

    #[test]
    fn test_pte_leaf() {
        let perms = PagePermissions::rwx();
        let pte = PageTableEntry::new_leaf(0xABCDE, perms, false);
        assert!(pte.is_valid());
        assert!(pte.is_leaf());
        assert_eq!(pte.ppn(), 0xABCDE);
        let p = pte.permissions();
        assert!(p.read && p.write && p.execute);
    }

    #[test]
    fn test_pte_accessed_dirty() {
        let mut pte = PageTableEntry::new_leaf(0x1000, PagePermissions::rw(), false);
        assert!(!pte.is_accessed());
        assert!(!pte.is_dirty());
        pte.set_accessed();
        assert!(pte.is_accessed());
        pte.set_dirty();
        assert!(pte.is_dirty());
        pte.clear_accessed();
        assert!(!pte.is_accessed());
        pte.clear_dirty();
        assert!(!pte.is_dirty());
    }

    #[test]
    fn test_invalid_permissions() {
        // W=1, R=0 is reserved
        let pte = PageTableEntry::from_raw(flags::V | flags::W);
        assert!(!pte.has_valid_permissions());
    }

    #[test]
    fn test_pte_global() {
        let pte = PageTableEntry::new_leaf(0x1000, PagePermissions::rwx(), true);
        assert!(pte.is_global());

        let pte2 = PageTableEntry::new_leaf(0x1000, PagePermissions::rwx(), false);
        assert!(!pte2.is_global());
    }

    #[test]
    fn test_pte_user() {
        let pte = PageTableEntry::new_leaf(0x1000, PagePermissions::user_rw(), false);
        assert!(pte.user_accessible());
        assert!(pte.permissions().user);
    }

    #[test]
    fn test_pte_ppn_extraction() {
        // Create PTE with specific PPN values
        // PPN2 = 0x123, PPN1 = 0x45, PPN0 = 0x67
        // Full PPN = (0x123 << 18) | (0x45 << 9) | 0x67 = 0x48A2B67 >> 10 ... wait
        // Let's use simpler values
        let pte = PageTableEntry::from_raw((0x12345 << 10) | flags::V | flags::R);

        assert_eq!(pte.ppn(), 0x12345);
        assert_eq!(pte.physical_address(), 0x12345 << 12);
    }

    #[test]
    fn test_pte_rsw() {
        let mut pte = PageTableEntry::new_leaf(0x1000, PagePermissions::rw(), false);
        assert_eq!(pte.rsw(), 0);

        pte.set_rsw(3);
        assert_eq!(pte.rsw(), 3);

        pte.set_rsw(1);
        assert_eq!(pte.rsw(), 1);
    }

    #[test]
    fn test_permission_helpers() {
        let pte = PageTableEntry::new_leaf(0x1000, PagePermissions::rwx(), false);
        assert!(pte.readable());
        assert!(pte.writable());
        assert!(pte.executable());

        let pte_rx = PageTableEntry::new_leaf(0x1000, PagePermissions::rx(), false);
        assert!(pte_rx.readable());
        assert!(!pte_rx.writable());
        assert!(pte_rx.executable());
    }

    #[test]
    fn test_pte_is_valid_leaf() {
        let leaf = PageTableEntry::new_leaf(0x1000, PagePermissions::rw(), false);
        assert!(leaf.is_valid_leaf());

        let ptr = PageTableEntry::new_pointer(0x1000);
        assert!(!ptr.is_valid_leaf());

        let invalid = PageTableEntry::from_raw(0);
        assert!(!invalid.is_valid_leaf());
    }
}
