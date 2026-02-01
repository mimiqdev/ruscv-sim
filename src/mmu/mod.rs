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
//! use ruscv_sim::mmu::{Mmu, MmuConfig, TranslationRequest, AccessType};
//! use ruscv_sim::core::PrivilegeMode;
//!
//! // Create MMU with default configuration
//! let mmu = Mmu::new(MmuConfig::default());
//!
//! // Perform address translation
//! let request = TranslationRequest {
//!     vaddr: 0x1000,
//!     access_type: AccessType::Read,
//!     privilege: PrivilegeMode::Supervisor,
//!     satp: 0x8000_0000_0000_0000, // Sv39 mode
//!     mstatus: 0,
//! };
//!
//! match mmu.translate(request) {
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
    pub fn from_satp(satp: u64) -> Self {
        match (satp >> 60) & 0xF {
            0 => Self::Bare,
            8 => Self::Sv39,
            9 => Self::Sv48,
            10 => Self::Sv57,
            _ => Self::Bare,
        }
    }
}

/// SATP (Supervisor Address Translation and Protection) register
#[derive(Debug, Clone, Copy)]
pub struct Satp(pub u64);

impl Satp {
    /// Get translation mode
    pub fn mode(&self) -> TranslationMode {
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
    pub fn paging_enabled(&self) -> bool {
        self.mode() != TranslationMode::Bare
    }
}

/// MMU error types
#[derive(Debug, Error, PartialEq)]
pub enum MmuError {
    #[error("Invalid virtual address: 0x{0:016x}")]
    InvalidVirtualAddress(u64),
    #[error("Page fault: {0}")]
    PageFault(String),
    #[error("Access fault: address 0x{0:016x}")]
    AccessFault(u64),
    #[error("PMP violation: address 0x{0:016x}")]
    PmpViolation(u64),
    #[error("Translation mode not supported: {0:?}")]
    UnsupportedMode(TranslationMode),
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
    pub fn translate(&self, request: TranslationRequest) -> Result<u64, MmuError> {
        // TODO: Implement full translation logic
        // 1. Check SATP mode
        // 2. Check if translation is needed
        // 3. TLB lookup
        // 4. Page table walk on miss
        // 5. PMP check
        // 6. Return physical address

        let satp = Satp(request.satp);

        // Bare mode: no translation
        if !satp.paging_enabled() {
            return Ok(request.vaddr);
        }

        // Select appropriate TLB
        let tlb = match request.access_type {
            AccessType::InstructionFetch => &self.itlb,
            _ => &self.dtlb,
        };

        // TODO: Implement full translation pipeline
        self.translator.translate(request, tlb)
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
        assert_eq!(TranslationMode::from_satp(0), TranslationMode::Bare);
        assert_eq!(TranslationMode::from_satp(8 << 60), TranslationMode::Sv39);
        assert_eq!(TranslationMode::from_satp(9 << 60), TranslationMode::Sv48);
    }

    #[test]
    fn test_satp_parsing() {
        // Sv39 mode, ASID=1, PPN=0x12345
        let satp = Satp((8 << 60) | (1 << 44) | 0x12345);

        assert_eq!(satp.mode(), TranslationMode::Sv39);
        assert_eq!(satp.asid(), 1);
        assert_eq!(satp.ppn(), 0x12345);
        assert_eq!(satp.root_page_table_addr(), 0x12345 << 12);
        assert!(satp.paging_enabled());
    }

    #[test]
    fn test_satp_bare_mode() {
        let satp = Satp(0);

        assert_eq!(satp.mode(), TranslationMode::Bare);
        assert!(!satp.paging_enabled());
    }
}
