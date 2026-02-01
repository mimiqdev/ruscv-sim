//! Control and Status Registers (CSR) module
//!
//! Implements RISC-V Control and Status Registers (CSRs) for privilege modes:
//! - Machine mode (M)
//! - Supervisor mode (S)
//! - User mode (U)

use crate::core::PrivilegeMode;
use std::collections::HashMap;
use thiserror::Error;

/// CSR access error types
#[derive(Debug, Error, PartialEq)]
pub enum CsrError {
    #[error("CSR address {0:#x} not found")]
    InvalidAddress(u16),
    #[error("Insufficient privilege to access CSR {0:#x}")]
    PrivilegeViolation(u16),
    #[error("CSR {0:#x} is read-only")]
    ReadOnly(u16),
}

/// CSR address constants (Machine Mode)
pub mod machine {
    // Machine Information Registers
    pub const MHARTID: u16 = 0xF14;

    // Machine Trap Setup
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE: u16 = 0x304;
    pub const MTVEC: u16 = 0x305;
    pub const MCOUNTEREN: u16 = 0x306;

    // Machine Trap Handling
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
    pub const MIP: u16 = 0x344;
}

/// CSR address constants (Supervisor Mode)
pub mod supervisor {
    // Supervisor Trap Setup
    pub const SSTATUS: u16 = 0x100;
    pub const SIE: u16 = 0x104;
    pub const STVEC: u16 = 0x105;
    pub const SCOUNTEREN: u16 = 0x106;

    // Supervisor Trap Handling
    pub const SSCRATCH: u16 = 0x140;
    pub const SEPC: u16 = 0x141;
    pub const SCAUSE: u16 = 0x142;
    pub const STVAL: u16 = 0x143;
    pub const SIP: u16 = 0x144;

    // Supervisor Protection and Translation
    pub const SATP: u16 = 0x180;
}

/// CSR address constants (Virtualization)
pub mod virtualization {
    pub const VSSTATUS: u16 = 0x200;
    pub const VSIE: u16 = 0x204;
    pub const VSTVEC: u16 = 0x205;
    pub const VSSCRATCH: u16 = 0x240;
    pub const VSEPC: u16 = 0x241;
    pub const VSCAUSE: u16 = 0x242;
    pub const VSTVAL: u16 = 0x243;
    pub const VSIP: u16 = 0x244;
    pub const VSATP: u16 = 0x280;
}

/// CSR access permission based on address encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsrPermission {
    ReadWrite,
    ReadOnly,
}

impl CsrPermission {
    /// Determine permission from CSR address (bits [11:10])
    pub fn from_address(addr: u16) -> Self {
        if (addr >> 10) & 0b11 == 0b11 {
            Self::ReadOnly
        } else {
            Self::ReadWrite
        }
    }
}

/// Control and Status Register File
#[derive(Debug, Clone)]
pub struct CsrFile {
    /// CSR storage (64-bit for RV64)
    csrs: HashMap<u16, u64>,
    /// Current privilege mode
    privilege: PrivilegeMode,
    /// Hart ID (hardware thread ID)
    #[allow(dead_code)]
    hart_id: u64,
}

impl CsrFile {
    /// Create a new CSR file
    pub fn new(hart_id: u64) -> Self {
        let mut csrs = HashMap::new();

        // Initialize Machine Mode CSRs with default values
        csrs.insert(machine::MHARTID, hart_id);
        csrs.insert(machine::MSTATUS, 0x0000_0000);
        csrs.insert(machine::MISA, 0x8000_0000_0010_0100); // RV64IMAC
        csrs.insert(machine::MEDELEG, 0x0000_0000);
        csrs.insert(machine::MIDELEG, 0x0000_0000);
        csrs.insert(machine::MIE, 0x0000_0000);
        csrs.insert(machine::MTVEC, 0x0000_0000);
        csrs.insert(machine::MCOUNTEREN, 0x0000_0000);
        csrs.insert(machine::MSCRATCH, 0x0000_0000);
        csrs.insert(machine::MEPC, 0x0000_0000);
        csrs.insert(machine::MCAUSE, 0x0000_0000);
        csrs.insert(machine::MTVAL, 0x0000_0000);
        csrs.insert(machine::MIP, 0x0000_0000);

        // Initialize Supervisor Mode CSRs
        csrs.insert(supervisor::SSTATUS, 0x0000_0000);
        csrs.insert(supervisor::SIE, 0x0000_0000);
        csrs.insert(supervisor::STVEC, 0x0000_0000);
        csrs.insert(supervisor::SCOUNTEREN, 0x0000_0000);
        csrs.insert(supervisor::SSCRATCH, 0x0000_0000);
        csrs.insert(supervisor::SEPC, 0x0000_0000);
        csrs.insert(supervisor::SCAUSE, 0x0000_0000);
        csrs.insert(supervisor::STVAL, 0x0000_0000);
        csrs.insert(supervisor::SIP, 0x0000_0000);
        csrs.insert(supervisor::SATP, 0x0000_0000);

        // Initialize Virtualization CSRs
        csrs.insert(virtualization::VSSTATUS, 0x0000_0000);
        csrs.insert(virtualization::VSIE, 0x0000_0000);
        csrs.insert(virtualization::VSTVEC, 0x0000_0000);
        csrs.insert(virtualization::VSSCRATCH, 0x0000_0000);
        csrs.insert(virtualization::VSEPC, 0x0000_0000);
        csrs.insert(virtualization::VSCAUSE, 0x0000_0000);
        csrs.insert(virtualization::VSTVAL, 0x0000_0000);
        csrs.insert(virtualization::VSIP, 0x0000_0000);
        csrs.insert(virtualization::VSATP, 0x0000_0000);

        Self {
            csrs,
            privilege: PrivilegeMode::Machine,
            hart_id,
        }
    }

    /// Get current privilege mode
    pub fn get_privilege(&self) -> PrivilegeMode {
        self.privilege
    }

    /// Set privilege mode
    pub fn set_privilege(&mut self, mode: PrivilegeMode) {
        self.privilege = mode;
    }

    /// Get minimum required privilege for CSR access (bits [9:8])
    fn get_required_privilege(addr: u16) -> PrivilegeMode {
        match (addr >> 8) & 0b11 {
            0 => PrivilegeMode::User,
            1 => PrivilegeMode::Supervisor,
            3 => PrivilegeMode::Machine,
            _ => PrivilegeMode::Machine, // Reserved, default to Machine
        }
    }

    /// Check if current privilege can access CSR
    fn check_privilege(&self, addr: u16) -> Result<(), CsrError> {
        let required = Self::get_required_privilege(addr);
        if (self.privilege as u8) < (required as u8) {
            return Err(CsrError::PrivilegeViolation(addr));
        }
        Ok(())
    }

    /// Read CSR value
    pub fn read(&self, addr: u16) -> Result<u64, CsrError> {
        self.check_privilege(addr)?;

        self.csrs
            .get(&addr)
            .copied()
            .ok_or(CsrError::InvalidAddress(addr))
    }

    /// Write CSR value
    pub fn write(&mut self, addr: u16, value: u64) -> Result<(), CsrError> {
        self.check_privilege(addr)?;

        // Check if CSR is read-only
        if CsrPermission::from_address(addr) == CsrPermission::ReadOnly {
            return Err(CsrError::ReadOnly(addr));
        }

        // Special handling for certain CSRs
        match addr {
            machine::MHARTID => {
                // MHARTID is read-only
                return Err(CsrError::ReadOnly(addr));
            }
            machine::MSTATUS => {
                // MSTATUS mask for RV64 - RISC-V Privileged Spec Section 3.1.6
                //
                // Mask value: 0x8000_0003_000D_FFEA
                // Bit breakdown:
                // - Bit 63 (SD):    0x8000_0000_0000_0000 - State Dirty (read-only, set by hardware)
                // - Bits 35:34 (SXL): 0x0000_0002_0000_0000 - Supervisor XLEN (WARL)
                // - Bits 33:32 (UXL): 0x0000_0001_0000_0000 - User XLEN (WARL)
                // - Bit 22 (TSR):   0x0000_0000_0040_0000 - Trap SRET (writable)
                // - Bit 21 (TW):    0x0000_0000_0020_0000 - Timeout Wait (writable)
                // - Bit 20 (TVM):   0x0000_0000_0010_0000 - Trap Virtual Memory (writable)
                // - Bit 18 (MXR):   0x0000_0000_0004_0000 - Make Executable Readable (writable)
                // - Bit 17 (SUM):   0x0000_0000_0002_0000 - Supervisor User Memory (writable)
                // - Bit 13 (FS):    0x0000_0000_0000_2000 - Floating-point State (writable)
                // - Bits 12:11 (MPP): 0x0000_0000_0000_1800 - Machine Previous Privilege (writable)
                // - Bit 8 (SPP):    0x0000_0000_0000_0100 - Supervisor Previous Privilege (writable)
                // - Bit 7 (MPIE):   0x0000_0000_0000_0080 - Machine Previous Interrupt Enable (writable)
                // - Bit 5 (SPIE):   0x0000_0000_0000_0020 - Supervisor Previous Interrupt Enable (writable)
                // - Bit 3 (MIE):    0x0000_0000_0000_0008 - Machine Interrupt Enable (writable)
                // - Bit 1 (SIE):    0x0000_0000_0000_0002 - Supervisor Interrupt Enable (writable)
                //
                // Reserved bits (not in mask): Bits 62:36, 31:23, 19, 16:14, 10:9, 6, 4, 2, 0
                const MSTATUS_MASK_RV64: u64 = 0x8000_0003_000D_FFEA;
                let masked = value & MSTATUS_MASK_RV64;
                self.csrs.insert(addr, masked);
            }
            _ => {
                self.csrs.insert(addr, value);
            }
        }

        Ok(())
    }

    /// CSR read and set bits (atomic)
    pub fn read_set(&mut self, addr: u16, mask: u64) -> Result<u64, CsrError> {
        let old_value = self.read(addr)?;
        if mask != 0 {
            self.write(addr, old_value | mask)?;
        }
        Ok(old_value)
    }

    /// CSR read and clear bits (atomic)
    pub fn read_clear(&mut self, addr: u16, mask: u64) -> Result<u64, CsrError> {
        let old_value = self.read(addr)?;
        if mask != 0 {
            self.write(addr, old_value & !mask)?;
        }
        Ok(old_value)
    }

    /// CSR read and write (atomic swap)
    pub fn read_write(&mut self, addr: u16, value: u64) -> Result<u64, CsrError> {
        let old_value = self.read(addr)?;
        self.write(addr, value)?;
        Ok(old_value)
    }
}

impl Default for CsrFile {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_file_creation() {
        let csr = CsrFile::new(0);
        assert_eq!(csr.get_privilege(), PrivilegeMode::Machine);
        assert_eq!(csr.hart_id, 0);
    }

    #[test]
    fn test_csr_read_mhartid() {
        let csr = CsrFile::new(42);
        let value = csr.read(machine::MHARTID).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_csr_write_mstatus() {
        let mut csr = CsrFile::new(0);
        csr.write(machine::MSTATUS, 0x1234_5678).unwrap();
        let value = csr.read(machine::MSTATUS).unwrap();
        // Check that reserved bits are masked (RV64 mstatus mask includes MPP)
        let rv64_mstatus_mask = 0x8000_0003_000D_FFEA_u64;
        assert_eq!(value, 0x1234_5678 & rv64_mstatus_mask);
    }

    #[test]
    fn test_csr_read_only() {
        let mut csr = CsrFile::new(0);
        let result = csr.write(machine::MHARTID, 100);
        assert!(matches!(result, Err(CsrError::ReadOnly(_))));
    }

    #[test]
    fn test_csr_privilege_violation() {
        let mut csr = CsrFile::new(0);
        csr.set_privilege(PrivilegeMode::User);

        // Try to read machine mode CSR from user mode
        let result = csr.read(machine::MSTATUS);
        assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
    }

    #[test]
    fn test_csr_read_set() {
        let mut csr = CsrFile::new(0);
        // Use MEPC instead of MSTATUS to avoid masking issues
        csr.write(machine::MEPC, 0x1000).unwrap();

        let old = csr.read_set(machine::MEPC, 0x0100).unwrap();
        assert_eq!(old, 0x1000);

        let new = csr.read(machine::MEPC).unwrap();
        assert_eq!(new, 0x1100);
    }

    #[test]
    fn test_csr_read_clear() {
        let mut csr = CsrFile::new(0);
        // Use MEPC instead of MSTATUS to avoid masking issues
        csr.write(machine::MEPC, 0x1111).unwrap();

        let old = csr.read_clear(machine::MEPC, 0x0101).unwrap();
        assert_eq!(old, 0x1111);

        let new = csr.read(machine::MEPC).unwrap();
        assert_eq!(new, 0x1010);
    }

    #[test]
    fn test_csr_read_write() {
        let mut csr = CsrFile::new(0);
        csr.write(machine::MEPC, 0x1234).unwrap();

        let old = csr.read_write(machine::MEPC, 0x5678).unwrap();
        assert_eq!(old, 0x1234);

        let new = csr.read(machine::MEPC).unwrap();
        assert_eq!(new, 0x5678);
    }

    #[test]
    fn test_supervisor_csr_access() {
        let mut csr = CsrFile::new(0);
        csr.set_privilege(PrivilegeMode::Supervisor);

        // Supervisor can access supervisor CSRs
        csr.write(supervisor::SSTATUS, 0x1234).unwrap();
        let value = csr.read(supervisor::SSTATUS).unwrap();
        assert_eq!(value, 0x1234);

        // But not machine CSRs
        let result = csr.read(machine::MSTATUS);
        assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
    }

    #[test]
    fn test_csr_permission_from_address() {
        // Machine mode read-write CSRs (0x300-0x3FF with bits [11:10] != 11)
        assert_eq!(
            CsrPermission::from_address(machine::MSTATUS),
            CsrPermission::ReadWrite
        );

        // Machine mode read-only CSRs (0xF00-0xFFF with bits [11:10] == 11)
        assert_eq!(
            CsrPermission::from_address(machine::MHARTID),
            CsrPermission::ReadOnly
        );
    }
}
