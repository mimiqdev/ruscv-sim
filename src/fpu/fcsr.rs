//! FCSR (Floating-Point Control and Status Register)
//!
//! RISC-V FCSR contains the floating-point dynamic rounding mode,
//! floating-point exception flags, and accrued exception flags.
//!
//! FCSR layout (for RV64):
/// |  31:8   |  7  |  6  |  5  |  4  |  3  |  2  |  1  |  0  |
/// |  NZOQA  | NX  | OF  | UF  | DZ  | NV  | 0   | Rounding Mode (frm) |
///
/// Accrued Exception Flags (read-only, sticky):
/// - NV: Invalid Operation
/// - DZ: Divide by Zero
/// - OF: Overflow
/// - UF: Underflow
/// - NX: Inexact
///
/// Dynamic Rounding Mode (frm):
/// - 000: RNE (Round to Nearest, ties to Even)
/// - 001: RTZ (Round Towards Zero)
/// - 010: RDN (Round Down, towards -∞)
/// - 011: RUP (Round Up, towards +∞)
/// - 100: RMM (Round to Nearest, ties to Max Magnitude)
/// - 101-111: Reserved (raise invalid operation exception)

use crate::core::PrivilegeMode;
use crate::csr::{Csr, CsrFile};
use crate::execute::ExecuteError;
use bitflags::bitflags;
use std::fmt;

bitflags! {
    /// Floating-Point Exception Flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FpFlags: u8 {
        const NV = 1 << 0;  // Invalid Operation
        const DZ = 1 << 1;  // Divide by Zero
        const OF = 1 << 2;  // Overflow
        const UF = 1 << 3;  // Underflow
        const NX = 1 << 4;  // Inexact
    }
}

/// FCSR Register
#[derive(Debug, Clone, Copy, Default)]
pub struct Fcsr {
    /// Rounding mode (bits 2:0)
    frm: u8,
    /// Accrued exception flags (bits 7:3)
    flags: FpFlags,
    /// Non-standard reserved bits (bits 31:8)
    reserved: u32,
}

impl Fcsr {
    /// Create a new FCSR with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset FCSR to default state
    pub fn reset(&mut self) {
        self.frm = 0; // Default to RNE
        self.flags = FpFlags::empty();
        self.reserved = 0;
    }

    /// Get rounding mode
    pub fn rounding_mode(&self) -> u8 {
        self.frm
    }

    /// Set rounding mode
    pub fn set_rounding_mode(&mut self, frm: u8) {
        self.frm = frm & 0x7;
    }

    /// Get exception flags
    pub fn flags(&self) -> FpFlags {
        self.flags
    }

    /// Set an exception flag
    pub fn set_flag(&mut self, flag: FpFlags) {
        self.flags.insert(flag);
    }

    /// Clear all flags
    pub fn clear_flags(&mut self) {
        self.flags = FpFlags::empty();
    }

    /// Check if any flag is set
    pub fn has_flags(&self) -> bool {
        !self.flags.is_empty()
    }

    /// Read FCSR value
    pub fn read(&self) -> u32 {
        let flags: u8 = self.flags.bits();
        (self.reserved as u32) << 8 | (flags as u32) << 3 | (self.frm as u32)
    }

    /// Write FCSR value
    pub fn write(&mut self, value: u32) {
        self.frm = (value & 0x7) as u8;
        self.flags = FpFlags::from_bits((value >> 3) as u8 & 0x1F);
        self.reserved = value >> 8;
    }

    /// Get NaN boxed default value for operations
    pub fn default_nan(&self) -> u64 {
        0x7FC0_0000_FFFF_FFFFu64
    }
}

impl Default for Fcsr {
    fn default() -> Self {
        Self {
            frm: 0,
            flags: FpFlags::empty(),
            reserved: 0,
        }
    }
}

/// FCSR CSR implementation
pub const FCSR: Csr = Csr {
    addr: 0x003,
    name: "fcsr",
    read: |_, csr| {
        let fcsr: &Fcsr = csr.read_custom();
        Ok(fcsr.read() as u64)
    },
    write: |_, csr, value| {
        let fcsr: &mut Fcsr = csr.write_custom();
        fcsr.write(value as u32);
        Ok(())
    },
};

/// FRM CSR (Floating-Point Rounding Mode, addr 0x002)
pub const FRM: Csr = Csr {
    addr: 0x002,
    name: "frm",
    read: |_, csr| {
        let fcsr: &Fcsr = csr.read_custom();
        Ok(fcsr.rounding_mode() as u64)
    },
    write: |_, csr, value| {
        let fcsr: &mut Fcsr = csr.write_custom();
        fcsf.set_rounding_mode(value as u8);
        Ok(())
    },
};

/// FFLAGS CSR (Floating-Point Accrued Exceptions, addr 0x001)
pub const FFLAGS: Csr = Csr {
    addr: 0x001,
    name: "fflags",
    read: |_, csr| {
        let fcsr: &Fcsr = csr.read_custom();
        Ok(fcsr.flags().bits() as u64)
    },
    write: |_, csr, value| {
        let fcsr: &mut Fcsr = csr.write_custom();
        let mut flags = FpFlags::from_bits(value as u8 & 0x1F);
        flags.set(FpFlags::NV, value & 1 != 0);
        flags.set(FpFlags::DZ, value & 2 != 0);
        flags.set(FpFlags::OF, value & 4 != 0);
        flags.set(FpFlags::UF, value & 8 != 0);
        flags.set(FpFlags::NX, value & 16 != 0);
        fcsr.set_flag(flags);
        Ok(())
    },
};

/// Rounding mode enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingMode {
    RNE = 0, // Round to Nearest, ties to Even
    RTZ = 1, // Round Towards Zero
    RDN = 2, // Round Down (towards -∞)
    RUP = 3, // Round Up (towards +∞)
    RMM = 4, // Round to Nearest, ties to Max Magnitude
}

impl RoundingMode {
    /// Get rounding mode from FCSR value
    pub fn from_frm(frm: u8) -> Self {
        match frm & 0x7 {
            0 => Self::RNE,
            1 => Self::RTZ,
            2 => Self::RDN,
            3 => Self::RUP,
            4 => Self::RMM,
            _ => Self::RNE, // Reserved, use RNE
        }
    }

    /// Apply rounding to a f32 value
    pub fn apply(self, value: f32) -> f32 {
        match self {
            Self::RNE => value,
            Self::RTZ => value.trunc(),
            Self::RDN => value.floor(),
            Self::RUP => value.ceil(),
            Self::RMM => value,
        }
    }
}

/// Convert FCSR flags to string for debugging
impl fmt::Display for Fcsr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "fcsr: frm={}, flags={}",
            self.frm,
            if self.flags.is_empty() {
                "none".to_string()
            } else {
                let mut parts = Vec::new();
                if self.flags.contains(FpFlags::NV) {
                    parts.push("NV");
                }
                if self.flags.contains(FpFlags::DZ) {
                    parts.push("DZ");
                }
                if self.flags.contains(FpFlags::OF) {
                    parts.push("OF");
                }
                if self.flags.contains(FpFlags::UF) {
                    parts.push("UF");
                }
                if self.flags.contains(FpFlags::NX) {
                    parts.push("NX");
                }
                parts.join(",")
            }
        )
    }
}
