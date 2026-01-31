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
#[derive(Debug, Clone, Copy)]
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
        self.reserved << 8 | (flags as u32) << 3 | (self.frm as u32)
    }

    /// Write FCSR value
    pub fn write(&mut self, value: u32) {
        self.frm = (value & 0x7) as u8;
        self.flags = FpFlags::from_bits((value >> 3) as u8).expect("Invalid FCSR flags");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcsr_new() {
        let fcsr = Fcsr::new();
        assert_eq!(fcsr.rounding_mode(), 0);
        assert!(!fcsr.has_flags());
    }

    #[test]
    fn test_fcsr_reset() {
        let mut fcsr = Fcsr::new();
        fcsr.set_rounding_mode(3);
        fcsr.set_flag(FpFlags::NV);
        fcsr.reset();
        assert_eq!(fcsr.rounding_mode(), 0);
        assert!(!fcsr.has_flags());
    }

    #[test]
    fn test_fcsr_rounding_mode() {
        let mut fcsr = Fcsr::new();
        fcsr.set_rounding_mode(4);
        assert_eq!(fcsr.rounding_mode(), 4);

        // Test that only lower 3 bits are used
        fcsr.set_rounding_mode(0xFF);
        assert_eq!(fcsr.rounding_mode(), 7);
    }

    #[test]
    fn test_fcsr_flags() {
        let mut fcsr = Fcsr::new();

        fcsr.set_flag(FpFlags::NV);
        assert!(fcsr.flags().contains(FpFlags::NV));
        assert!(fcsr.has_flags());

        fcsr.set_flag(FpFlags::DZ);
        assert!(fcsr.flags().contains(FpFlags::DZ));

        fcsr.clear_flags();
        assert!(!fcsr.has_flags());
    }

    #[test]
    fn test_fcsr_read_write() {
        let mut fcsr = Fcsr::new();
        fcsr.set_rounding_mode(3);
        fcsr.set_flag(FpFlags::NV | FpFlags::NX);

        let value = fcsr.read();

        let mut fcsr2 = Fcsr::new();
        fcsr2.write(value);

        assert_eq!(fcsr2.rounding_mode(), 3);
        assert!(fcsr2.flags().contains(FpFlags::NV));
        assert!(fcsr2.flags().contains(FpFlags::NX));
    }

    #[test]
    fn test_fcsr_default_nan() {
        let fcsr = Fcsr::new();
        let nan = fcsr.default_nan();
        assert_eq!(nan, 0x7FC0_0000_FFFF_FFFFu64);
    }

    #[test]
    fn test_rounding_mode_from_frm() {
        assert_eq!(RoundingMode::from_frm(0), RoundingMode::RNE);
        assert_eq!(RoundingMode::from_frm(1), RoundingMode::RTZ);
        assert_eq!(RoundingMode::from_frm(2), RoundingMode::RDN);
        assert_eq!(RoundingMode::from_frm(3), RoundingMode::RUP);
        assert_eq!(RoundingMode::from_frm(4), RoundingMode::RMM);
        assert_eq!(RoundingMode::from_frm(5), RoundingMode::RNE); // Reserved
        assert_eq!(RoundingMode::from_frm(7), RoundingMode::RNE); // Reserved
    }

    #[test]
    fn test_rounding_mode_apply() {
        assert_eq!(RoundingMode::RNE.apply(2.5), 2.5);
        assert_eq!(RoundingMode::RTZ.apply(2.7), 2.0);
        assert_eq!(RoundingMode::RTZ.apply(-2.7), -2.0);
        assert_eq!(RoundingMode::RDN.apply(2.7), 2.0);
        assert_eq!(RoundingMode::RDN.apply(-2.7), -3.0);
        assert_eq!(RoundingMode::RUP.apply(2.2), 3.0);
        assert_eq!(RoundingMode::RUP.apply(-2.7), -2.0);
        assert_eq!(RoundingMode::RMM.apply(2.5), 2.5);
    }

    #[test]
    fn test_fcsr_display_no_flags() {
        let fcsr = Fcsr::new();
        let output = format!("{}", fcsr);
        assert!(output.contains("none"));
    }

    #[test]
    fn test_fcsr_display_with_flags() {
        let mut fcsr = Fcsr::new();
        fcsr.set_flag(FpFlags::NV | FpFlags::DZ | FpFlags::OF | FpFlags::UF | FpFlags::NX);
        let output = format!("{}", fcsr);
        assert!(output.contains("NV"));
        assert!(output.contains("DZ"));
        assert!(output.contains("OF"));
        assert!(output.contains("UF"));
        assert!(output.contains("NX"));
    }
}
