//! Page Table Entry (PTE) definitions and operations

/// Page Table Entry (64-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PageTableEntry(u64);

/// Page table entry flags
pub mod flags {
    pub const V: u64 = 1 << 0;
    pub const R: u64 = 1 << 1;
    pub const W: u64 = 1 << 2;
    pub const X: u64 = 1 << 3;
    pub const U: u64 = 1 << 4;
    pub const G: u64 = 1 << 5;
    pub const A: u64 = 1 << 6;
    pub const D: u64 = 1 << 7;
    pub const RSW_MASK: u64 = 0b11 << 8;
}

/// Page permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PagePermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub user: bool,
}

impl PagePermissions {
    pub const fn all() -> Self {
        Self { read: true, write: true, execute: true, user: true }
    }
    pub const fn none() -> Self {
        Self { read: false, write: false, execute: false, user: false }
    }
    pub const fn rw() -> Self {
        Self { read: true, write: true, execute: false, user: false }
    }
    pub const fn rx() -> Self {
        Self { read: true, write: false, execute: true, user: false }
    }
    pub const fn rwx() -> Self {
        Self { read: true, write: true, execute: true, user: false }
    }
}

impl PageTableEntry {
    const PPN0_SHIFT: u32 = 10;
    const PPN1_SHIFT: u32 = 19;
    const PPN2_SHIFT: u32 = 28;
    const PPN_MASK: u64 = 0x1FF;

    pub const fn from_raw(bits: u64) -> Self {
        Self(bits)
    }
    pub const fn bits(&self) -> u64 {
        self.0
    }
    pub const fn is_valid(&self) -> bool {
        self.0 & flags::V != 0
    }
    pub const fn is_leaf(&self) -> bool {
        self.0 & (flags::R | flags::W | flags::X) != 0
    }
    pub const fn is_pointer(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }
    pub const fn is_valid_leaf(&self) -> bool {
        self.is_valid() && self.is_leaf()
    }
    pub fn ppn0(&self) -> u64 {
        (self.0 >> Self::PPN0_SHIFT) & Self::PPN_MASK
    }
    pub fn ppn1(&self) -> u64 {
        (self.0 >> Self::PPN1_SHIFT) & Self::PPN_MASK
    }
    pub fn ppn2(&self) -> u64 {
        (self.0 >> Self::PPN2_SHIFT) & ((1 << 26) - 1)
    }
    pub fn ppn(&self) -> u64 {
        self.0 >> 10
    }
    pub fn physical_address(&self) -> u64 {
        self.ppn() << 12
    }
    pub fn permissions(&self) -> PagePermissions {
        PagePermissions {
            read: self.0 & flags::R != 0,
            write: self.0 & flags::W != 0,
            execute: self.0 & flags::X != 0,
            user: self.0 & flags::U != 0,
        }
    }
    pub const fn is_global(&self) -> bool {
        self.0 & flags::G != 0
    }
    pub const fn is_accessed(&self) -> bool {
        self.0 & flags::A != 0
    }
    pub const fn is_dirty(&self) -> bool {
        self.0 & flags::D != 0
    }
    pub fn set_accessed(&mut self) {
        self.0 |= flags::A;
    }
    pub fn set_dirty(&mut self) {
        self.0 |= flags::D;
    }
    pub fn new_leaf(ppn: u64, perms: PagePermissions, global: bool) -> Self {
        let mut bits = (ppn << 10) | flags::V;
        if perms.read { bits |= flags::R; }
        if perms.write { bits |= flags::W; }
        if perms.execute { bits |= flags::X; }
        if perms.user { bits |= flags::U; }
        if global { bits |= flags::G; }
        Self(bits)
    }
    pub fn new_pointer(ppn: u64) -> Self {
        Self((ppn << 10) | flags::V)
    }
    pub const fn has_valid_permissions(&self) -> bool {
        !(self.0 & flags::W != 0 && self.0 & flags::R == 0)
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
    }

    #[test]
    fn test_invalid_permissions() {
        // W=1, R=0 is reserved
        let pte = PageTableEntry::from_raw(flags::V | flags::W);
        assert!(!pte.has_valid_permissions());
    }
}
