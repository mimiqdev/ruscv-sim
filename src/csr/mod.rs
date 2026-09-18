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

    // Machine Counters
    pub const MINSTRET: u16 = 0xB02;

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
    /// Determine permission from CSR address (bits \[11:10\])
    pub fn from_address(addr: u16) -> Self {
        if (addr >> 10) & 0b11 == 0b11 {
            Self::ReadOnly
        } else {
            Self::ReadWrite
        }
    }
}

/// Result of one CSR access.
///
/// `wrote` records the architectural write classification, not whether the
/// resulting value differs from the old value.  This distinction lets the
/// retirement layer give an explicit `minstret` write precedence even when a
/// CSR operation writes the same value back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsrAccess {
    /// CSR address accessed.
    pub addr: u16,
    /// Value observed before the operation.
    pub old_value: u64,
    /// Value observed after the operation (or `old_value` for a read-only access).
    pub new_value: u64,
    /// Whether this access performed an explicit architectural write.
    pub wrote: bool,
}

impl CsrAccess {
    /// Return whether the operation performed an explicit write.
    pub const fn is_write(self) -> bool {
        self.wrote
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
        // RV64IU reset profile: C is fixed at 0, so public fetch remains
        // 32-bit (IALIGN=32).  M/A/F/D dispatch remains available in the
        // component executor; this value is only the architectural MISA reset.
        csrs.insert(machine::MISA, 0x8000_0000_0010_0100);
        csrs.insert(machine::MEDELEG, 0x0000_0000);
        csrs.insert(machine::MIDELEG, 0x0000_0000);
        csrs.insert(machine::MIE, 0x0000_0000);
        csrs.insert(machine::MTVEC, 0x0000_0000);
        csrs.insert(machine::MCOUNTEREN, 0x0000_0000);
        csrs.insert(machine::MINSTRET, 0x0000_0000);
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

        // Check if CSR is read-only before looking up its storage.  This keeps
        // the architectural error for encoded read-only CSRs deterministic.
        if CsrPermission::from_address(addr) == CsrPermission::ReadOnly {
            return Err(CsrError::ReadOnly(addr));
        }

        // Unknown writable addresses must fail without creating a new storage
        // entry.  In particular, a failed CSR instruction must not have a
        // hidden side effect in the backing map.
        if !self.csrs.contains_key(&addr) {
            return Err(CsrError::InvalidAddress(addr));
        }

        // Special handling for certain CSRs
        match addr {
            machine::MHARTID => {
                // MHARTID is read-only
                return Err(CsrError::ReadOnly(addr));
            }
            machine::MISA => {
                // C is WARL-fixed to zero while the public execution path has
                // fixed IALIGN=32.  Do not mask any other extension bits here:
                // the component M/A/F/D dispatch remains available.
                self.csrs.insert(addr, value & !(1u64 << 2));
            }
            machine::MEPC => {
                // With C disabled, IALIGN is 32 and MEPC[1:0] are hardwired
                // to zero for every direct or CSR-mediated write.
                self.csrs.insert(addr, value & !0b11);
            }
            machine::MSTATUS => {
                // MSTATUS mask for RV64 - RISC-V Privileged Spec Section 3.1.6
                //
                // The established Task 1 compatibility mask is retained; bit
                // 17 is included so Task 2 can preserve and clear MPRV during
                // MRET state restoration.
                const MSTATUS_MASK_RV64: u64 = 0x8000_0003_000F_FFEA;
                let masked = value & MSTATUS_MASK_RV64;
                self.csrs.insert(addr, masked);
            }
            _ => {
                self.csrs.insert(addr, value);
            }
        }

        Ok(())
    }

    /// Write a CSR and return the old/new values plus explicit-write fact.
    pub fn write_with_access(&mut self, addr: u16, value: u64) -> Result<CsrAccess, CsrError> {
        let old_value = self.read(addr)?;
        self.write(addr, value)?;
        let new_value = self.read(addr)?;
        Ok(CsrAccess {
            addr,
            old_value,
            new_value,
            wrote: true,
        })
    }

    /// CSR read and set bits with an explicit write classification.
    ///
    /// `perform_write` is intentionally separate from `mask != 0`: CSRRS
    /// determines write intent from the source-register identity, so a
    /// non-zero `rs1` whose value is zero still performs an explicit write.
    pub fn read_set_classified(
        &mut self,
        addr: u16,
        mask: u64,
        perform_write: bool,
    ) -> Result<CsrAccess, CsrError> {
        let old_value = self.read(addr)?;
        if perform_write {
            self.write(addr, old_value | mask)?;
        }
        let new_value = if perform_write {
            self.read(addr)?
        } else {
            old_value
        };
        Ok(CsrAccess {
            addr,
            old_value,
            new_value,
            wrote: perform_write,
        })
    }

    /// CSR read and clear bits with an explicit write classification.
    pub fn read_clear_classified(
        &mut self,
        addr: u16,
        mask: u64,
        perform_write: bool,
    ) -> Result<CsrAccess, CsrError> {
        let old_value = self.read(addr)?;
        if perform_write {
            self.write(addr, old_value & !mask)?;
        }
        let new_value = if perform_write {
            self.read(addr)?
        } else {
            old_value
        };
        Ok(CsrAccess {
            addr,
            old_value,
            new_value,
            wrote: perform_write,
        })
    }

    /// CSR read and set bits (atomic).
    pub fn read_set(&mut self, addr: u16, mask: u64) -> Result<u64, CsrError> {
        Ok(self.read_set_classified(addr, mask, mask != 0)?.old_value)
    }

    /// CSR read and clear bits (atomic).
    pub fn read_clear(&mut self, addr: u16, mask: u64) -> Result<u64, CsrError> {
        Ok(self.read_clear_classified(addr, mask, mask != 0)?.old_value)
    }

    /// CSR read and write (atomic swap).
    pub fn read_write(&mut self, addr: u16, value: u64) -> Result<u64, CsrError> {
        Ok(self.write_with_access(addr, value)?.old_value)
    }

    /// CSR read and set bits, returning the write classification.
    pub fn read_set_with_access(&mut self, addr: u16, mask: u64) -> Result<CsrAccess, CsrError> {
        self.read_set_classified(addr, mask, mask != 0)
    }

    /// CSR read and clear bits, returning the write classification.
    pub fn read_clear_with_access(&mut self, addr: u16, mask: u64) -> Result<CsrAccess, CsrError> {
        self.read_clear_classified(addr, mask, mask != 0)
    }

    /// CSR read and write, returning the write classification.
    pub fn read_write_with_access(&mut self, addr: u16, value: u64) -> Result<CsrAccess, CsrError> {
        self.write_with_access(addr, value)
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
        let rv64_mstatus_mask = 0x8000_0003_000F_FFEA_u64;
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
        assert_eq!(old, 0x1110);

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
