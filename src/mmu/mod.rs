//! RISC-V Memory Management Unit (MMU)
//!
//! Implements the RISC-V memory management subsystem including:
//! - Sv39/Sv48 page table translation
//! - TLB (Translation Lookaside Buffer) with LRU replacement
//! - PMP (Physical Memory Protection)
//! - MMIO (Memory-Mapped I/O) support
//!
//! # Architecture Overview
//!
//! ```text
//! Virtual Address                    Physical Address
//!       │                                    ▲
//!       ▼                                    │
//! ┌─────────────┐      ┌──────────┐     ┌──────────┐
//! │   ITLB      │      │          │     │  PMP     │
//! │  (64 ent)   │──────►   PTW    ├─────►  Check   ├────► Physical Memory
//! └─────────────┘      │ (Sv39)   │     └──────────┘
//! ┌─────────────┐      │          │
//! │   DTLB      │      └──────────┘
//! │  (64 ent)   │
//! └─────────────┘
//! ```
//!
//! # Example Usage
//!
//! ```rust
//! use ruscv_sim::mmu::{Mmu, MmuConfig, TranslationRequest, AccessType, PhysicalMemory};
//! use ruscv_sim::core::PrivilegeMode;
//!
//! // Create MMU with default configuration
//! let mut mmu = Mmu::new(MmuConfig::default());
//!
//! // Create physical memory
//! let mut memory = PhysicalMemory::new(0x8000_0000, 0x10000);
//!
//! // Perform address translation
//! let request = TranslationRequest {
//!     vaddr: 0x1000,
//!     access_type: AccessType::Read,
//!     privilege: PrivilegeMode::Supervisor,
//!     satp: 0, // Bare mode - no translation
//!     mstatus: 0,
//! };
//!
//! match mmu.translate(request, &mut memory) {
//!     Ok(paddr) => println!("Translated to: 0x{:x}", paddr),
//!     Err(e) => println!("Translation failed: {:?}", e),
//! }
//! ```

use crate::core::PrivilegeMode;
use thiserror::Error;

pub mod physical;
pub mod pte;
pub mod sv39;
pub mod tlb;
pub mod translator;

pub use physical::{MemoryAttributes, MemoryRegion, MemoryRegionType, PhysicalMemory};
pub use pte::{PagePermissions, PageTableEntry};
pub use sv39::Sv39;
pub use tlb::{Tlb, TlbEntry, TlbStats};
pub use translator::{AddressTranslator, TranslationRequest, TranslationResult};

/// MMU configuration
#[derive(Debug, Clone, Copy)]
pub struct MmuConfig {
    /// TLB size (entries per TLB)
    pub tlb_size: usize,
    /// TLB associativity
    pub tlb_ways: usize,
    /// Enable Sv48 support
    pub enable_sv48: bool,
    /// Number of PMP entries
    pub pmp_entries: usize,
}

impl Default for MmuConfig {
    fn default() -> Self {
        Self {
            tlb_size: 64,
            tlb_ways: 4,
            enable_sv48: true,
            pmp_entries: 16,
        }
    }
}

/// Memory access type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    /// Instruction fetch
    InstructionFetch,
    /// Data read
    Read,
    /// Data write
    Write,
}

/// Address translation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationMode {
    /// No translation (bare metal mode)
    Bare = 0,
    /// Sv39: 39-bit virtual addresses
    Sv39 = 8,
    /// Sv48: 48-bit virtual addresses
    Sv48 = 9,
    /// Sv57: 57-bit virtual addresses (not implemented)
    Sv57 = 10,
}

impl TranslationMode {
    /// Parse mode from SATP register value
    ///
    /// Returns `Some(mode)` for valid modes (0, 8, 9, 10),
    /// or `None` for invalid/unrecognized modes.
    ///
    /// # Arguments
    /// * `satp` - SATP register value
    ///
    /// # Returns
    /// * `Some(TranslationMode)` - Valid translation mode
    /// * `None` - Invalid mode value
    pub fn from_satp(satp: u64) -> Option<Self> {
        match (satp >> 60) & 0xF {
            0 => Some(Self::Bare),
            8 => Some(Self::Sv39),
            9 => Some(Self::Sv48),
            10 => Some(Self::Sv57),
            _ => None,
        }
    }
}

/// SATP (Supervisor Address Translation and Protection) register
#[derive(Debug, Clone, Copy)]
pub struct Satp(pub u64);

impl Satp {
    /// Get translation mode
    ///
    /// Returns `Some(mode)` for valid modes, or `None` for invalid modes.
    pub fn mode(&self) -> Option<TranslationMode> {
        TranslationMode::from_satp(self.0)
    }

    /// Get ASID (Address Space Identifier)
    pub fn asid(&self) -> u16 {
        ((self.0 >> 44) & 0xFFFF) as u16
    }

    /// Get root page table PPN (Physical Page Number)
    pub fn ppn(&self) -> u64 {
        self.0 & ((1 << 44) - 1)
    }

    /// Get root page table physical address
    pub fn root_page_table_addr(&self) -> u64 {
        self.ppn() << 12
    }

    /// Check if paging is enabled
    ///
    /// Returns true if the translation mode is not Bare (i.e., Sv39, Sv48, etc.)
    /// Returns false for Bare mode or invalid modes.
    pub fn paging_enabled(&self) -> bool {
        matches!(
            self.mode(),
            Some(TranslationMode::Sv39) | Some(TranslationMode::Sv48) | Some(TranslationMode::Sv57)
        )
    }

    /// Check if the SATP configuration is valid
    ///
    /// Returns false if the mode field contains an invalid value.
    pub fn is_valid(&self) -> bool {
        self.mode().is_some()
    }
}

/// Page fault reason - used to avoid heap allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultReason {
    /// Permission denied for the requested access type
    PermissionDenied { access_type: AccessType, vaddr: u64 },
    /// Page fault at specific level during page table walk
    PageTableWalk { level: usize, vaddr: u64 },
    /// User mode access to supervisor page
    UserAccessToSupervisorPage { vaddr: u64 },
    /// Access to non-accessed page (A bit not set)
    NotAccessed { vaddr: u64 },
    /// Write to non-dirty page (D bit not set for write)
    NotDirty { vaddr: u64 },
}

impl core::fmt::Display for PageFaultReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PageFaultReason::PermissionDenied { access_type, vaddr } => {
                write!(
                    f,
                    "Permission denied for access {:?} at address 0x{:016x}",
                    access_type, vaddr
                )
            }
            PageFaultReason::PageTableWalk { level, vaddr } => {
                write!(
                    f,
                    "Page fault at level {} for address 0x{:016x}",
                    level, vaddr
                )
            }
            PageFaultReason::UserAccessToSupervisorPage { vaddr } => {
                write!(
                    f,
                    "User mode access to supervisor page at address 0x{:016x}",
                    vaddr
                )
            }
            PageFaultReason::NotAccessed { vaddr } => {
                write!(f, "Page not accessed (A=0) at address 0x{:016x}", vaddr)
            }
            PageFaultReason::NotDirty { vaddr } => {
                write!(
                    f,
                    "Page not dirty (D=0) for write at address 0x{:016x}",
                    vaddr
                )
            }
        }
    }
}

/// MMU error types
#[derive(Debug, Error, PartialEq)]
pub enum MmuError {
    #[error("Invalid virtual address: 0x{0:016x}")]
    InvalidVirtualAddress(u64),
    #[error("Page fault: {0}")]
    PageFault(PageFaultReason),
    #[error("Access fault: address 0x{0:016x}")]
    AccessFault(u64),
    #[error("PMP violation: address 0x{0:016x}")]
    PmpViolation(u64),
    #[error("Translation mode not supported: {0:?}")]
    UnsupportedMode(TranslationMode),
    #[error("Invalid SATP mode: {0}")]
    InvalidSatpMode(u64),
}

/// Main MMU structure
pub struct Mmu {
    _config: MmuConfig,
    /// Instruction TLB
    itlb: Tlb,
    /// Data TLB
    dtlb: Tlb,
    /// Address translator
    translator: AddressTranslator,
}

impl Mmu {
    /// Create a new MMU with the given configuration
    pub fn new(config: MmuConfig) -> Self {
        let itlb = Tlb::new(config.tlb_size, config.tlb_ways);
        let dtlb = Tlb::new(config.tlb_size, config.tlb_ways);
        let translator = AddressTranslator::new(config);

        Self {
            _config: config,
            itlb,
            dtlb,
            translator,
        }
    }

    /// Translate a virtual address to physical address
    ///
    /// This is the main translation entry point that performs:
    /// 1. SATP mode check (Bare vs Sv39/Sv48)
    /// 2. TLB lookup
    /// 3. Page table walk on TLB miss (updates A/D bits)
    /// 4. Returns physical address
    ///
    /// # Arguments
    /// * `request` - Translation request with vaddr, access type, privilege, etc.
    /// * `memory` - Physical memory interface for page table walks
    ///
    /// # Returns
    /// * `Ok(paddr)` - Translated physical address
    /// * `Err(MmuError)` - Translation error (page fault, access fault, etc.)
    pub fn translate<M: physical::PhysicalMemoryInterface + ?Sized>(
        &mut self,
        request: TranslationRequest,
        memory: &mut M,
    ) -> Result<u64, MmuError> {
        let satp = Satp(request.satp);

        // Bare mode: no translation
        if !satp.paging_enabled() {
            return Ok(request.vaddr);
        }

        // Select appropriate TLB and perform translation
        match request.access_type {
            AccessType::InstructionFetch => {
                self.translator
                    .translate_with_tlb(request, &mut self.itlb, memory)
            }
            _ => self
                .translator
                .translate_with_tlb(request, &mut self.dtlb, memory),
        }
    }

    /// Flush TLB entries (SFENCE.VMA implementation)
    pub fn flush_tlb(&mut self, vaddr: Option<u64>, asid: Option<u16>) {
        match (vaddr, asid) {
            (Some(va), Some(a)) => {
                // Flush specific address and ASID
                self.itlb.flush_asid_va(a, va);
                self.dtlb.flush_asid_va(a, va);
            }
            (Some(va), None) => {
                // Flush specific address (all ASIDs)
                self.itlb.flush_va(va);
                self.dtlb.flush_va(va);
            }
            (None, Some(a)) => {
                // Flush all entries for ASID
                self.itlb.flush_asid(a);
                self.dtlb.flush_asid(a);
            }
            (None, None) => {
                // Flush all entries
                self.itlb.flush_all();
                self.dtlb.flush_all();
            }
        }
    }

    /// Get TLB statistics
    pub fn tlb_stats(&self) -> (TlbStats, TlbStats) {
        (self.itlb.stats(), self.dtlb.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmu_creation() {
        let config = MmuConfig::default();
        let mmu = Mmu::new(config);

        let (itlb_stats, dtlb_stats) = mmu.tlb_stats();
        assert_eq!(itlb_stats.accesses, 0);
        assert_eq!(dtlb_stats.accesses, 0);
    }

    #[test]
    fn test_translation_mode_parsing() {
        assert_eq!(TranslationMode::from_satp(0), Some(TranslationMode::Bare));
        assert_eq!(
            TranslationMode::from_satp(8 << 60),
            Some(TranslationMode::Sv39)
        );
        assert_eq!(
            TranslationMode::from_satp(9 << 60),
            Some(TranslationMode::Sv48)
        );
        // Invalid mode returns None
        assert_eq!(TranslationMode::from_satp(15 << 60), None);
    }

    #[test]
    fn test_satp_parsing() {
        // Sv39 mode, ASID=1, PPN=0x12345
        let satp = Satp((8 << 60) | (1 << 44) | 0x12345);

        assert_eq!(satp.mode(), Some(TranslationMode::Sv39));
        assert_eq!(satp.asid(), 1);
        assert_eq!(satp.ppn(), 0x12345);
        assert_eq!(satp.root_page_table_addr(), 0x12345 << 12);
        assert!(satp.paging_enabled());
        assert!(satp.is_valid());
    }

    #[test]
    fn test_satp_bare_mode() {
        let satp = Satp(0);

        assert_eq!(satp.mode(), Some(TranslationMode::Bare));
        assert!(!satp.paging_enabled());
        assert!(satp.is_valid());
    }

    #[test]
    fn test_satp_invalid_mode() {
        // Invalid mode (15)
        let satp = Satp(15 << 60);

        assert_eq!(satp.mode(), None);
        assert!(!satp.paging_enabled());
        assert!(!satp.is_valid());
    }
}
