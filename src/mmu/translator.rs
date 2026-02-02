//! Address translation engine
//!
//! Implements the full address translation pipeline including:
//! - TLB lookup
//! - Page table walk on TLB miss
//! - Permission checking
//! - Accessed/Dirty bit updates

use super::physical::PhysicalMemoryInterface;
use super::sv39::{PageTableWalker, VirtualAddress, WalkResult};
use super::tlb::{Tlb, TlbEntry};
use super::{AccessType, MmuConfig, MmuError, PrivilegeMode, Satp, TranslationMode};

/// Translation request
#[derive(Debug, Clone, Copy)]
pub struct TranslationRequest {
    pub vaddr: u64,
    pub access_type: AccessType,
    pub privilege: PrivilegeMode,
    pub satp: u64,
    pub mstatus: u64,
}

/// Translation result
#[derive(Debug, Clone, Copy)]
pub struct TranslationResult {
    pub paddr: u64,
    pub pte_addr: Option<u64>,
    pub pte_value: Option<u64>,
}

/// Address translator
pub struct AddressTranslator {
    _config: MmuConfig,
}

impl AddressTranslator {
    pub fn new(config: MmuConfig) -> Self {
        Self { _config: config }
    }

    /// Translate a virtual address to physical address with full TLB support
    ///
    /// This is the main translation entry point that:
    /// 1. Checks translation mode (Bare vs Sv39/Sv48)
    /// 2. Performs TLB lookup
    /// 3. On TLB miss, walks page table and updates TLB
    /// 4. Checks permissions
    /// 5. Updates Accessed/Dirty bits in the PTE
    pub fn translate_with_tlb<M: PhysicalMemoryInterface + ?Sized>(
        &self,
        request: TranslationRequest,
        tlb: &mut Tlb,
        memory: &mut M,
    ) -> Result<u64, MmuError> {
        let satp = Satp(request.satp);

        // Check translation mode
        match satp.mode() {
            Some(TranslationMode::Bare) => {
                // No translation - return virtual address as physical
                Ok(request.vaddr)
            }
            Some(TranslationMode::Sv39) => self.translate_sv39_with_tlb(request, satp, tlb, memory),
            Some(TranslationMode::Sv48) => Err(MmuError::UnsupportedMode(TranslationMode::Sv48)),
            Some(TranslationMode::Sv57) => Err(MmuError::UnsupportedMode(TranslationMode::Sv57)),
            None => Err(MmuError::InvalidSatpMode(satp.0)),
        }
    }

    /// Translate using Sv39 with TLB
    fn translate_sv39_with_tlb<M: PhysicalMemoryInterface + ?Sized>(
        &self,
        request: TranslationRequest,
        satp: Satp,
        tlb: &mut Tlb,
        memory: &mut M,
    ) -> Result<u64, MmuError> {
        let vaddr = request.vaddr;
        let vpn = vaddr >> 12;

        // Check TLB first
        if let Some(entry) = tlb.lookup(vpn, satp.asid()) {
            // TLB hit - check permissions
            if !self.check_tlb_permissions(&entry, request.access_type, request.privilege) {
                return Err(MmuError::PageFault(
                    super::PageFaultReason::PermissionDenied {
                        access_type: request.access_type,
                        vaddr,
                    },
                ));
            }

            // Build physical address from TLB entry
            let paddr = (entry.ppn << 12) | (vaddr & 0xFFF);
            return Ok(paddr);
        }

        // TLB miss - perform page table walk
        let va = VirtualAddress::new(vaddr)?;
        let mut walker = PageTableWalker::new(memory, satp.ppn());

        let is_user = matches!(request.privilege, PrivilegeMode::User);

        // Use walk_check_permissions_and_update_ad to update A/D bits
        match walker.walk_check_permissions_and_update_ad(va, request.access_type, is_user) {
            WalkResult::Success {
                paddr,
                pte,
                level,
                pte_addr: _,
            } => {
                // Insert into TLB for future accesses
                let tlb_entry = self.create_tlb_entry(vpn, &pte, satp.asid(), level);
                tlb.insert(vpn, tlb_entry);

                Ok(paddr)
            }
            WalkResult::PageFault { level } => {
                Err(MmuError::PageFault(super::PageFaultReason::PageTableWalk {
                    level,
                    vaddr,
                }))
            }
            WalkResult::AccessFault { level: _ } => Err(MmuError::AccessFault(vaddr)),
        }
    }

    /// Check permissions against TLB entry
    fn check_tlb_permissions(
        &self,
        entry: &TlbEntry,
        access_type: AccessType,
        privilege: PrivilegeMode,
    ) -> bool {
        // Check privilege level
        let is_user = matches!(privilege, PrivilegeMode::User);
        if is_user && !entry.user {
            return false;
        }

        // Check access type permission
        match access_type {
            AccessType::Read => entry.read,
            AccessType::Write => entry.write,
            AccessType::InstructionFetch => entry.execute,
        }
    }

    /// Create TLB entry from PTE
    fn create_tlb_entry(
        &self,
        vpn: u64,
        pte: &super::pte::PageTableEntry,
        asid: u16,
        _level: usize,
    ) -> TlbEntry {
        let ppn = pte.ppn();
        let perms = pte.permissions();

        TlbEntry {
            vpn,
            ppn,
            asid,
            global: pte.is_global(),
            read: perms.read,
            write: perms.write,
            execute: perms.execute,
            user: perms.user,
            accessed: pte.is_accessed(),
            dirty: pte.is_dirty(),
        }
    }

    /// Legacy translate method (for backward compatibility)
    ///
    /// Note: This method does not support actual page table walking
    /// as it doesn't have access to physical memory. Use translate_with_tlb
    /// for full translation support.
    pub fn translate(&self, request: TranslationRequest, _tlb: &Tlb) -> Result<u64, MmuError> {
        let satp = Satp(request.satp);

        // Check translation mode
        match satp.mode() {
            Some(TranslationMode::Bare) => Ok(request.vaddr),
            Some(TranslationMode::Sv39) => {
                // Without memory access, we can only validate the address format
                let _va = VirtualAddress::new(request.vaddr)?;
                // Return passthrough - actual translation requires page table walk
                Ok(request.vaddr)
            }
            Some(TranslationMode::Sv48) => Err(MmuError::UnsupportedMode(TranslationMode::Sv48)),
            Some(TranslationMode::Sv57) => Err(MmuError::UnsupportedMode(TranslationMode::Sv57)),
            None => Err(MmuError::InvalidSatpMode(satp.0)),
        }
    }

    /// Translate for Sv39 with explicit page table walk
    ///
    /// This method performs a full page table walk without TLB caching.
    /// Useful for debug or when TLB is disabled.
    /// Updates Accessed/Dirty bits in the PTE according to RISC-V spec.
    pub fn translate_sv39_walk<M: PhysicalMemoryInterface + ?Sized>(
        &self,
        vaddr: u64,
        satp: u64,
        access_type: AccessType,
        privilege: PrivilegeMode,
        memory: &mut M,
    ) -> Result<u64, MmuError> {
        let satp = Satp(satp);

        if satp.mode() != Some(TranslationMode::Sv39) {
            return Err(MmuError::UnsupportedMode(TranslationMode::Sv39));
        }

        let va = VirtualAddress::new(vaddr)?;
        let mut walker = PageTableWalker::new(memory, satp.ppn());
        let is_user = matches!(privilege, PrivilegeMode::User);

        match walker.walk_check_permissions_and_update_ad(va, access_type, is_user) {
            WalkResult::Success { paddr, .. } => Ok(paddr),
            WalkResult::PageFault { level } => {
                Err(MmuError::PageFault(super::PageFaultReason::PageTableWalk {
                    level,
                    vaddr,
                }))
            }
            WalkResult::AccessFault { .. } => Err(MmuError::AccessFault(vaddr)),
        }
    }

    /// Extract VPN fields from virtual address
    pub fn extract_vpn(vaddr: u64, level: usize) -> u64 {
        const VPN_WIDTH: u32 = 9;
        const PAGE_OFFSET: u32 = 12;

        let shift = PAGE_OFFSET + (level as u32) * VPN_WIDTH;
        (vaddr >> shift) & ((1 << VPN_WIDTH) - 1)
    }

    /// Get page offset
    pub fn page_offset(vaddr: u64) -> u64 {
        vaddr & ((1 << 12) - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmu::physical::PhysicalMemory;
    use crate::mmu::pte::{PagePermissions, PageTableEntry};

    #[test]
    fn test_bare_mode() {
        let config = MmuConfig::default();
        let translator = AddressTranslator::new(config);
        let mut tlb = Tlb::new(64, 4);
        let mut memory = PhysicalMemory::new(0x8000_0000, 0x10000);

        let request = TranslationRequest {
            vaddr: 0x8000_0000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Machine,
            satp: 0, // Bare mode
            mstatus: 0,
        };

        let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
        assert_eq!(result.unwrap(), 0x8000_0000);
    }

    #[test]
    fn test_extract_vpn() {
        // VPN[2] = bits 30-38 (0x4000_0000 has bit 30 set)
        assert_eq!(AddressTranslator::extract_vpn(0x0000_4000_0000, 2), 1);
        // VPN[1] = bits 21-29 (0x0020_0000 has bit 21 set)
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0020_0000, 1), 1);
        // VPN[0] = bits 12-20 (0x0000_1000 has bit 12 set)
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0000_1000, 0), 1);

        // Additional test: verify VPN extraction for different addresses
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0040_0000, 1), 2);
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0080_0000, 1), 4);
        assert_eq!(AddressTranslator::extract_vpn(0x0000_0000_2000, 0), 2);
    }

    #[test]
    fn test_page_offset() {
        assert_eq!(AddressTranslator::page_offset(0x1234), 0x234);
        assert_eq!(AddressTranslator::page_offset(0xABCD_EF12), 0xF12);
    }

    #[test]
    fn test_sv39_translation_4kb_page() {
        // Create physical memory with page table setup
        // Need: root (0x8000_0000), level1 (0x8000_1000), level0 (0x8000_2000), data (0x9000_0000)
        let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

        // Setup a simple 3-level page table
        let root_ppn = 0x80000; // Root at 0x8000_0000
        let level1_ppn = 0x80001; // Level 1 at 0x8000_1000
        let level0_ppn = 0x80002; // Level 0 at 0x8000_2000
        let data_ppn = 0x90000; // Data at 0x9000_0000

        // Root page table (level 2) - one entry pointing to level 1 table
        let root_pte = PageTableEntry::new_pointer(level1_ppn);
        memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

        // Level 1 page table - one entry pointing to level 0 table
        let level1_pte = PageTableEntry::new_pointer(level0_ppn);
        memory
            .write_dword(level1_ppn << 12, level1_pte.bits())
            .unwrap();

        // Level 0 page table - leaf entry for VPN[0] = 1
        // PTE for VPN[0] = 1 is at offset 8 (8 bytes per PTE)
        let level0_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rw(), false);
        memory
            .write_dword((level0_ppn << 12) + 8, level0_pte.bits())
            .unwrap();

        // Create SATP value: Sv39 mode (8 << 60), ASID=0, PPN
        let satp = (8u64 << 60) | root_ppn;

        let config = MmuConfig::default();
        let translator = AddressTranslator::new(config);
        let mut tlb = Tlb::new(64, 4);

        // Translate virtual address 0x1000 (VPN[0]=1)
        let request = TranslationRequest {
            vaddr: 0x1000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Supervisor,
            satp,
            mstatus: 0,
        };

        let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
        assert!(result.is_ok(), "Translation failed: {:?}", result.err());

        // Expected: (data_ppn << 12) | offset = 0x9000_0000 | 0x0 = 0x9000_0000
        assert_eq!(result.unwrap(), 0x9000_0000);

        // Check TLB was populated
        assert_eq!(tlb.stats().hits, 0);
        assert_eq!(tlb.stats().misses, 1);

        // Second access should hit TLB
        let result2 = translator.translate_with_tlb(request, &mut tlb, &mut memory);
        assert_eq!(result2.unwrap(), 0x9000_0000);
        assert_eq!(tlb.stats().hits, 1);
    }

    #[test]
    fn test_sv39_page_fault() {
        // Create memory large enough for root table
        let mut memory = PhysicalMemory::new(0x8000_0000, 0x20000);

        // Create SATP with root PPN pointing to memory with invalid PTE
        // PPN = 0x80000 -> address 0x8000_0000 (all zeros = invalid PTEs)
        let satp = (8u64 << 60) | 0x80000;

        let config = MmuConfig::default();
        let translator = AddressTranslator::new(config);
        let mut tlb = Tlb::new(64, 4);

        // Try to translate - should fail since page table is empty
        let request = TranslationRequest {
            vaddr: 0x1000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Supervisor,
            satp,
            mstatus: 0,
        };

        let result = translator.translate_with_tlb(request, &mut tlb, &mut memory);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MmuError::PageFault(_)));
    }

    #[test]
    fn test_sv39_permission_check() {
        // Need space for root (0x8000_0000) and data (0x9000_0000)
        let mut memory = PhysicalMemory::new(0x8000_0000, 0x1001_0000);

        // Setup simple page table with read-only page (gigapage)
        let root_ppn = 0x80000; // Root at 0x8000_0000
        let data_ppn = 0x90000; // Data at 0x9000_0000

        // Leaf entry at root level (gigapage) - read/execute only
        let root_pte = PageTableEntry::new_leaf(data_ppn, PagePermissions::rx(), false);
        memory.write_dword(root_ppn << 12, root_pte.bits()).unwrap();

        let satp = (8u64 << 60) | root_ppn;
        let config = MmuConfig::default();
        let translator = AddressTranslator::new(config);
        let mut tlb = Tlb::new(64, 4);

        // Write access should fail
        let write_request = TranslationRequest {
            vaddr: 0x1000,
            access_type: AccessType::Write,
            privilege: PrivilegeMode::Supervisor,
            satp,
            mstatus: 0,
        };

        let result = translator.translate_with_tlb(write_request, &mut tlb, &mut memory);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MmuError::PageFault(_)));

        // Read access should succeed
        let read_request = TranslationRequest {
            vaddr: 0x1000,
            access_type: AccessType::Read,
            privilege: PrivilegeMode::Supervisor,
            satp,
            mstatus: 0,
        };

        let result = translator.translate_with_tlb(read_request, &mut tlb, &mut memory);
        assert!(result.is_ok());
    }
}
