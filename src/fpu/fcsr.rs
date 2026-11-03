//! FCSR (Floating-Point Control and Status Register)
//!
//! RISC-V FCSR holds the dynamic rounding mode and the five accrued
//! exception flags (unprivileged specification 20250508, "F" chapter).
//!
//! FCSR layout (RV64):
//!
//! ```text
//! | 63:8 |  7  |  6  |  5  |  4  |  3  |  2  |  1  |  0  |
//! | zero |    frm    | NV  | DZ  | OF  | UF  | NX  |
//! ```
//!
//! `frm` occupies bits 7:5 and `fflags` bits 4:0.  Within `fflags` the
//! architectural order is `NX` (bit 0), `UF` (bit 1), `OF` (bit 2), `DZ`
//! (bit 3) and `NV` (bit 4).  Bits 63:8 are reserved: they read zero and
//! ignore writes (contract D5).
//!
//! Accrued Exception Flags (sticky):
//! - NX (bit 0): Inexact
//! - UF (bit 1): Underflow
//! - OF (bit 2): Overflow
//! - DZ (bit 3): Divide by Zero
//! - NV (bit 4): Invalid Operation
//!
//! Dynamic Rounding Mode (`fcsr` bits 7:5, `frm` CSR bits 2:0):
//! - 000: RNE (Round to Nearest, ties to Even)
//! - 001: RTZ (Round Towards Zero)
//! - 010: RDN (Round Down, towards -∞)
//! - 011: RUP (Round Up, towards +∞)
//! - 100: RMM (Round to Nearest, ties to Max Magnitude)
//! - 101-111: Reserved (raise invalid operation exception)
use bitflags::bitflags;
use std::fmt;

bitflags! {
    /// Floating-point accrued exception flags, in architectural `fflags` bit
    /// order (F: `fflags[0]` is NX through `fflags[4]` NV).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FpFlags: u8 {
        const NX = 1 << 0;  // Inexact
        const UF = 1 << 1;  // Underflow
        const OF = 1 << 2;  // Overflow
        const DZ = 1 << 3;  // Divide by Zero
        const NV = 1 << 4;  // Invalid Operation
    }
}

/// FCSR Register
///
/// `fcsr` is eight architectural bits: `frm` (7:5) and the five accrued
/// exception flags `fflags` (4:0).  Bits 63:8 are reserved, read zero, and
/// ignore writes (contract D5), so they have no storage here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fcsr {
    /// Rounding mode, the `frm` CSR value (2:0)
    frm: u8,
    /// Accrued exception flags, the `fflags` CSR value (4:0)
    flags: FpFlags,
}

impl Default for Fcsr {
    fn default() -> Self {
        Self {
            frm: 0,
            flags: FpFlags::empty(),
        }
    }
}

impl Fcsr {
    /// Mask of the architectural `fcsr` bits.
    pub const MASK: u32 = 0xFF;
    /// Mask of the five accrued exception flags.
    pub const FFLAGS_MASK: u8 = 0x1F;
    /// Mask of the three rounding-mode bits.
    pub const FRM_MASK: u8 = 0x7;
    /// `frm` starts at bit 5 of `fcsr`.
    const FRM_SHIFT: u32 = 5;

    /// Create a new FCSR with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset FCSR to default state
    pub fn reset(&mut self) {
        self.frm = 0; // Default to RNE
        self.flags = FpFlags::empty();
    }

    /// Get rounding mode
    pub fn rounding_mode(&self) -> u8 {
        self.frm
    }

    /// Set rounding mode
    pub fn set_rounding_mode(&mut self, frm: u8) {
        self.frm = frm & Self::FRM_MASK;
    }

    /// Architectural `frm` bits (2:0).
    pub fn frm_bits(&self) -> u8 {
        self.frm
    }

    /// Write the architectural `frm` bits (2:0); reserved bits ignore value.
    pub fn write_frm_bits(&mut self, value: u8) {
        self.frm = value & Self::FRM_MASK;
    }

    /// Architectural `fflags` bits (4:0).
    pub fn flags_bits(&self) -> u8 {
        self.flags.bits()
    }

    /// Write the architectural `fflags` bits (4:0); reserved bits are dropped.
    pub fn write_flags_bits(&mut self, value: u8) {
        self.flags = FpFlags::from_bits_truncate(value & Self::FFLAGS_MASK);
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

    /// Read the architectural `fcsr` value: `frm` in bits 7:5 and `fflags`
    /// in bits 4:0.
    ///
    /// Bits 63:8 are reserved and read zero.
    pub fn read(&self) -> u32 {
        u32::from(self.frm) << Self::FRM_SHIFT | u32::from(self.flags.bits())
    }

    /// Write the `fcsr` address: `frm` takes bits 7:5 and `fflags` bits 4:0.
    /// Every bit above 7 is reserved and ignored.
    pub fn write(&mut self, value: u32) {
        let value = (value & Self::MASK) as u8;
        self.frm = (value >> Self::FRM_SHIFT) & Self::FRM_MASK;
        self.flags = FpFlags::from_bits_truncate(value & Self::FFLAGS_MASK);
    }

    /// The default NaN for operations: the canonical NaN, NaN boxed for
    /// single precision (contract D4/D5).
    pub fn default_nan(&self) -> u64 {
        crate::fpu::nan_boxing::canonical_nan_f32()
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
    fn test_fcsr_reserved_bits_read_zero_and_ignore_writes() {
        let mut fcsr = Fcsr::new();
        // Bits above 7 are reserved: they neither round-trip nor panic.
        fcsr.write(0xFFFF_FFFF);
        assert_eq!(fcsr.read(), Fcsr::MASK);
        assert_eq!(fcsr.frm_bits(), Fcsr::FRM_MASK);
        assert_eq!(fcsr.flags_bits(), Fcsr::FFLAGS_MASK);

        fcsr.write(Fcsr::MASK | 0x8000_0000);
        assert_eq!(fcsr.read(), Fcsr::MASK);
    }

    #[test]
    fn test_fcsr_aliases_members() {
        let mut fcsr = Fcsr::new();
        // `frm` is bits 7:5 of `fcsr`.
        fcsr.write_frm_bits(0b1111_1010);
        assert_eq!(fcsr.frm_bits(), 0b010);
        assert_eq!(fcsr.read(), 0b010 << 5);
        // `fflags` is bits 4:0 of `fcsr`.
        fcsr.write_flags_bits(0b1111_1110);
        assert_eq!(fcsr.flags_bits(), 0b1_1110);
        assert_eq!(fcsr.read(), 0b010 << 5 | 0b1_1110);
        // Writing one view leaves the other unchanged.
        fcsr.write(0);
        assert_eq!(fcsr.frm_bits(), 0);
        assert_eq!(fcsr.flags_bits(), 0);
    }

    #[test]
    fn test_fcsr_field_layout_and_flag_order() {
        // Architectural layout: `frm` 7:5, `fflags` 4:0, and the flags from
        // NX at bit 0 to NV at bit 4.
        assert_eq!(FpFlags::NX.bits(), 1 << 0);
        assert_eq!(FpFlags::UF.bits(), 1 << 1);
        assert_eq!(FpFlags::OF.bits(), 1 << 2);
        assert_eq!(FpFlags::DZ.bits(), 1 << 3);
        assert_eq!(FpFlags::NV.bits(), 1 << 4);
        assert_eq!(FpFlags::all().bits(), Fcsr::FFLAGS_MASK);

        let mut fcsr = Fcsr::new();
        fcsr.write_frm_bits(5);
        fcsr.write_flags_bits(FpFlags::NX.bits() | FpFlags::OF.bits() | FpFlags::NV.bits());
        assert_eq!(fcsr.read(), 0b101 << 5 | 0b1_0101);

        // Reading `fcsr` back recovers each field and each named flag.
        let mut restored = Fcsr::new();
        restored.write(fcsr.read());
        assert_eq!(restored.frm_bits(), 5);
        assert!(restored.flags().contains(FpFlags::NX));
        assert!(restored.flags().contains(FpFlags::OF));
        assert!(restored.flags().contains(FpFlags::NV));
        assert!(!restored.flags().contains(FpFlags::UF));
        assert!(!restored.flags().contains(FpFlags::DZ));
    }

    #[test]
    fn test_fcsr_default_nan_is_the_boxed_canonical_nan() {
        let nan = Fcsr::new().default_nan();
        assert_eq!(nan, crate::fpu::nan_boxing::canonical_nan_f32());
        assert_eq!(nan as u32, crate::fpu::nan_boxing::CANONICAL_NAN_F32);
        assert!(crate::fpu::nan_boxing::is_nan_boxed(nan));
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
