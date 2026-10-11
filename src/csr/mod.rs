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
    pub const MVENDORID: u16 = 0xF11;
    pub const MARCHID: u16 = 0xF12;
    pub const MIMPID: u16 = 0xF13;
    pub const MHARTID: u16 = 0xF14;
    pub const MCONFIGPTR: u16 = 0xF15;

    // Machine Trap Setup
    pub const MSTATUS: u16 = 0x300;
    pub const MISA: u16 = 0x301;
    pub const MEDELEG: u16 = 0x302;
    pub const MIDELEG: u16 = 0x303;
    pub const MIE: u16 = 0x304;
    pub const MTVEC: u16 = 0x305;
    pub const MCOUNTEREN: u16 = 0x306;

    // Machine Configuration
    pub const MENVCFG: u16 = 0x30A;

    // Machine Counters
    pub const MCYCLE: u16 = 0xB00;
    pub const MINSTRET: u16 = 0xB02;

    // Machine Trap Handling
    pub const MSCRATCH: u16 = 0x340;
    pub const MEPC: u16 = 0x341;
    pub const MCAUSE: u16 = 0x342;
    pub const MTVAL: u16 = 0x343;
    pub const MIP: u16 = 0x344;

    // Machine Memory Protection
    pub const PMPCFG0: u16 = 0x3A0;
    pub const PMPADDR0: u16 = 0x3B0;
}

/// CSR address constants (Supervisor Mode)
pub mod supervisor {
    // Supervisor Trap Setup
    pub const SSTATUS: u16 = 0x100;
    pub const SIE: u16 = 0x104;
    pub const STVEC: u16 = 0x105;
    pub const SCOUNTEREN: u16 = 0x106;

    // Supervisor Configuration
    pub const SENVCFG: u16 = 0x10A;

    // Supervisor Trap Handling
    pub const SSCRATCH: u16 = 0x140;
    pub const SEPC: u16 = 0x141;
    pub const SCAUSE: u16 = 0x142;
    pub const STVAL: u16 = 0x143;
    pub const SIP: u16 = 0x144;

    // Supervisor Protection and Translation
    pub const SATP: u16 = 0x180;
}

/// Unprivileged counter addresses.  `cycle` and `instret` are read-only
/// shadows of `mcycle` and `minstret`.  `time` is absent because this
/// profile has no `mtime`.
pub mod unprivileged {
    pub const CYCLE: u16 = 0xC00;
    pub const TIME: u16 = 0xC01;
    pub const INSTRET: u16 = 0xC02;
}

/// Hypervisor (VS) CSR address constants.
///
/// The H extension is not part of this profile, so none of these addresses
/// exists in [`CsrFile`]; every access is `CsrError::InvalidAddress`.
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

/// `mstatus` field masks for the Stage 4 RV64 M/S/U profile.
pub mod mstatus {
    pub const SIE: u64 = 1 << 1;
    pub const MIE: u64 = 1 << 3;
    pub const SPIE: u64 = 1 << 5;
    pub const MPIE: u64 = 1 << 7;
    pub const SPP: u64 = 1 << 8;
    pub const MPP: u64 = 0b11 << 11;
    pub const MPP_SHIFT: u32 = 11;
    pub const MPRV: u64 = 1 << 17;
    pub const SUM: u64 = 1 << 18;
    pub const MXR: u64 = 1 << 19;
    pub const TVM: u64 = 1 << 20;
    pub const TW: u64 = 1 << 21;
    pub const TSR: u64 = 1 << 22;
    /// UXL = 2 (bits 33:32) and SXL = 2 (bits 35:34), both read-only.
    pub const XLEN_FIELDS: u64 = 0x0000_000A_0000_0000;
    /// Every field software can change through the `mstatus` address.
    pub const WRITABLE: u64 =
        SIE | MIE | SPIE | MPIE | SPP | MPP | MPRV | SUM | MXR | TVM | TW | TSR;
    /// Fields of the SXLEN-64 `sstatus` layout: SIE, SPIE, UBE, SPP, VS, FS,
    /// XS, SUM, MXR, UXL, and SD.
    pub const SSTATUS_VIEW: u64 = 0x8000_0003_000D_E762;
    /// `sstatus` fields that are writable in this profile.
    pub const SSTATUS_WRITABLE: u64 = SIE | SPIE | SPP | SUM | MXR;
    /// Reset value: everything 0 except the read-only SXL and UXL.
    pub const RESET: u64 = XLEN_FIELDS;
}

/// `misa` for RV64IMA plus S and U.  No bit is writable.
pub const MISA_VALUE: u64 = 0x8000_0000_0014_1101;

/// Writable bits of `mip` and `mie`: SSIP/SSIE, STIP/STIE, SEIP/SEIE.
pub const SUPERVISOR_INTERRUPTS: u64 = 0x222;

/// `satp` MODE encodings this profile implements.
pub const SATP_MODE_BARE: u64 = 0;
/// `satp` MODE encoding for Sv39.
pub const SATP_MODE_SV39: u64 = 8;
/// `satp` ASID field (bits 59:44), read-only zero because ASIDLEN is 0.
const SATP_ASID_MASK: u64 = 0xFFFF << 44;

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
    /// Internal subscribed transition journal; absent during ordinary execution.
    observed_writes: Option<Vec<CsrAccess>>,
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

        // Stored registers.  Views (`sstatus`, `sie`, `sip`) and the
        // read-only-zero sets (PMP, identity, HPM) have no storage entry.
        csrs.insert(machine::MHARTID, hart_id);
        csrs.insert(machine::MSTATUS, mstatus::RESET);
        csrs.insert(machine::MISA, MISA_VALUE);
        csrs.insert(machine::MEDELEG, 0);
        csrs.insert(machine::MIDELEG, 0);
        csrs.insert(machine::MIE, 0);
        csrs.insert(machine::MTVEC, 0);
        csrs.insert(machine::MCOUNTEREN, 0);
        csrs.insert(machine::MENVCFG, 0);
        csrs.insert(machine::MCYCLE, 0);
        csrs.insert(machine::MINSTRET, 0);
        csrs.insert(machine::MSCRATCH, 0);
        csrs.insert(machine::MEPC, 0);
        csrs.insert(machine::MCAUSE, 0);
        csrs.insert(machine::MTVAL, 0);
        csrs.insert(machine::MIP, 0);

        csrs.insert(supervisor::STVEC, 0);
        csrs.insert(supervisor::SCOUNTEREN, 0);
        csrs.insert(supervisor::SENVCFG, 0);
        csrs.insert(supervisor::SSCRATCH, 0);
        csrs.insert(supervisor::SEPC, 0);
        csrs.insert(supervisor::SCAUSE, 0);
        csrs.insert(supervisor::STVAL, 0);
        csrs.insert(supervisor::SATP, 0);

        Self {
            csrs,
            observed_writes: None,
            privilege: PrivilegeMode::Machine,
            hart_id,
        }
    }

    /// Return whether `addr` is a register that always reads zero and
    /// ignores writes in this profile.
    fn is_read_only_zero(addr: u16) -> bool {
        matches!(addr,
            // Even pmpcfg0..pmpcfg14; odd numbers do not exist on RV64.
            0x3A0..=0x3AF if addr.is_multiple_of(2))
            || matches!(addr,
                // pmpaddr0..pmpaddr63
                0x3B0..=0x3EF
                // mhpmcounter3..mhpmcounter31
                | 0xB03..=0xB1F
                // mhpmevent3..mhpmevent31
                | 0x323..=0x33F
                | machine::MVENDORID
                | machine::MARCHID
                | machine::MIMPID
                | machine::MCONFIGPTR)
    }

    fn stored(&self, addr: u16) -> u64 {
        self.csrs.get(&addr).copied().unwrap_or(0)
    }

    /// Architectural value at `addr`, or `None` when the address does not
    /// exist.  This applies no privilege check.
    fn view(&self, addr: u16) -> Option<u64> {
        match addr {
            supervisor::SSTATUS => Some(self.stored(machine::MSTATUS) & mstatus::SSTATUS_VIEW),
            supervisor::SIE => Some(self.stored(machine::MIE) & self.stored(machine::MIDELEG)),
            supervisor::SIP => Some(self.stored(machine::MIP) & self.stored(machine::MIDELEG)),
            unprivileged::CYCLE => Some(self.stored(machine::MCYCLE)),
            unprivileged::INSTRET => Some(self.stored(machine::MINSTRET)),
            _ if Self::is_read_only_zero(addr) => Some(0),
            _ => self.csrs.get(&addr).copied(),
        }
    }

    /// Legalize a write of `value` through `addr`.  Returns the backing
    /// register and the value it holds after the write.
    fn legalize(&self, addr: u16, value: u64) -> (u16, u64) {
        let old = self.stored(addr);
        match addr {
            machine::MSTATUS => (machine::MSTATUS, Self::legal_mstatus(old, value)),
            supervisor::SSTATUS => {
                let current = self.stored(machine::MSTATUS);
                let merged =
                    (current & !mstatus::SSTATUS_WRITABLE) | (value & mstatus::SSTATUS_WRITABLE);
                (machine::MSTATUS, Self::legal_mstatus(current, merged))
            }
            // No extension can be disabled, and none can be enabled.
            machine::MISA => (addr, MISA_VALUE),
            // No trap delegation (M1), and both counter-enable registers are
            // read-only zero in this profile.
            machine::MEDELEG | machine::MIDELEG | machine::MCOUNTEREN | supervisor::SCOUNTEREN => {
                (addr, old)
            }
            machine::MIE | machine::MIP => (addr, value & SUPERVISOR_INTERRUPTS),
            supervisor::SIE | supervisor::SIP => {
                let target = if addr == supervisor::SIE {
                    machine::MIE
                } else {
                    machine::MIP
                };
                let delegated = self.stored(machine::MIDELEG) & SUPERVISOR_INTERRUPTS;
                let current = self.stored(target);
                (target, (current & !delegated) | (value & delegated))
            }
            // IALIGN is fixed at 32, so bits 1:0 of both EPCs read zero.
            machine::MEPC | supervisor::SEPC => (addr, value & !0b11),
            machine::MTVEC | supervisor::STVEC => {
                // MODE is WARL over Direct (0) and Vectored (1).
                let mode = if value & 0b11 <= 1 {
                    value & 0b11
                } else {
                    old & 0b11
                };
                (addr, (value & !0b11) | mode)
            }
            supervisor::SATP => match value >> 60 {
                SATP_MODE_BARE | SATP_MODE_SV39 => (addr, value & !SATP_ASID_MASK),
                // An unsupported MODE leaves the whole register unchanged.
                _ => (addr, old),
            },
            // Only FIOM (bit 0) is implemented in either register.
            machine::MENVCFG | supervisor::SENVCFG => (addr, value & 1),
            _ => (addr, value),
        }
    }

    /// Apply the `mstatus` WARL rules to a candidate value.
    fn legal_mstatus(old: u64, value: u64) -> u64 {
        let mut next = value & mstatus::WRITABLE;
        // MPP holds U (0), S (1), or M (3); 2 is reserved and keeps the
        // previous legal value.
        if (value & mstatus::MPP) >> mstatus::MPP_SHIFT == 2 {
            next = (next & !mstatus::MPP) | (old & mstatus::MPP);
        }
        next | mstatus::XLEN_FIELDS
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
        // mcounteren is read-only zero, so CY and IR stay clear.  A read of
        // either shadow from S or U is therefore illegal.  Machine mode is
        // not gated by mcounteren.
        if self.privilege != PrivilegeMode::Machine
            && matches!(addr, unprivileged::CYCLE | unprivileged::INSTRET)
        {
            return Err(CsrError::PrivilegeViolation(addr));
        }
        // TVM = 1 makes an S-mode access to satp illegal.
        if addr == supervisor::SATP
            && self.privilege == PrivilegeMode::Supervisor
            && self.mstatus() & mstatus::TVM != 0
        {
            return Err(CsrError::PrivilegeViolation(addr));
        }
        Ok(())
    }

    /// The whole `mstatus` register, independent of the current privilege.
    ///
    /// The Hart reads TVM, TSR, TW, and the trap-return fields through this
    /// path because those checks apply in modes that cannot read `mstatus`.
    pub(crate) fn mstatus(&self) -> u64 {
        self.stored(machine::MSTATUS)
    }

    /// The whole `mip` register, independent of the current privilege.
    ///
    /// Interrupt sampling runs in S and U, which cannot read `mip`.
    pub(crate) fn mip(&self) -> u64 {
        self.stored(machine::MIP)
    }

    /// The whole `mie` register, independent of the current privilege.
    pub(crate) fn mie(&self) -> u64 {
        self.stored(machine::MIE)
    }

    /// Write `mstatus` as a Hart-owned transition (trap entry or return),
    /// applying the field rules but not the current privilege view.
    pub(crate) fn set_mstatus(&mut self, value: u64) {
        let old_value = self.stored(machine::MSTATUS);
        let new_value = Self::legal_mstatus(old_value, value);
        self.csrs.insert(machine::MSTATUS, new_value);
        if let Some(writes) = &mut self.observed_writes {
            writes.push(CsrAccess {
                addr: machine::MSTATUS,
                old_value,
                new_value,
                wrote: true,
            });
        }
    }

    /// Read CSR value
    pub fn read(&self, addr: u16) -> Result<u64, CsrError> {
        self.check_privilege(addr)?;
        self.view(addr).ok_or(CsrError::InvalidAddress(addr))
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
        let Some(old_value) = self.view(addr) else {
            return Err(CsrError::InvalidAddress(addr));
        };

        if !Self::is_read_only_zero(addr) {
            let (target, legal) = self.legalize(addr, value);
            self.csrs.insert(target, legal);
        }

        let new_value = self.view(addr).unwrap_or(0);
        if let Some(writes) = &mut self.observed_writes {
            writes.push(CsrAccess {
                addr,
                old_value,
                new_value,
                wrote: true,
            });
        }
        Ok(())
    }

    pub(crate) fn begin_observation(&mut self, enabled: bool) {
        self.observed_writes = enabled.then(Vec::new);
    }

    pub(crate) fn take_observed_writes(&mut self) -> Vec<CsrAccess> {
        self.observed_writes.take().unwrap_or_default()
    }

    pub(crate) fn hart_id(&self) -> u64 {
        self.hart_id
    }

    pub(crate) fn architectural_value(&self, addr: u16) -> u64 {
        self.view(addr).unwrap_or(0)
    }

    /// Return the authoritative Machine-mode retirement counter without
    /// applying the current privilege view.
    ///
    /// Retirement is a Hart-owned architectural transition.  A User or
    /// Supervisor instruction may retire and increment `minstret` even though
    /// it cannot read or write the Machine CSR through a CSR instruction, so
    /// the core uses this narrow internal inspection path rather than changing
    /// the CSR privilege view around a retirement update.
    pub(crate) fn minstret_value(&self) -> u64 {
        self.stored(machine::MINSTRET)
    }

    /// Increment one Machine counter by one, wrapping at 64 bits.
    fn increment_stored(&mut self, addr: u16) -> u64 {
        let value = self.csrs.entry(addr).or_insert(0);
        let old_value = *value;
        *value = value.wrapping_add(1);
        if let Some(writes) = &mut self.observed_writes {
            writes.push(CsrAccess {
                addr,
                old_value,
                new_value: *value,
                wrote: true,
            });
        }
        *value
    }

    /// Increment `mcycle` for one completed Hart turn.
    pub(crate) fn increment_mcycle(&mut self) -> u64 {
        self.increment_stored(machine::MCYCLE)
    }

    /// Increment the single authoritative `minstret` storage for one retired
    /// instruction.
    pub(crate) fn increment_minstret(&mut self) -> u64 {
        self.increment_stored(machine::MINSTRET)
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
        // 0x1234_5678 sets MIE, SPIE, MPP = 2 (reserved, so MPP stays 0),
        // SUM, TVM, TW, and bits outside the writable set.  SXL and UXL
        // read 2.
        assert_eq!(value, 0x0000_000A_0034_0028);
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
        // Bits 2, 4, 12 are not sstatus fields and VS is read-only 0; SPIE
        // sticks and UXL reads 2.
        csr.write(supervisor::SSTATUS, 0x1234).unwrap();
        let value = csr.read(supervisor::SSTATUS).unwrap();
        assert_eq!(value, 0x0000_0002_0000_0020);

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
