//! D (Double Precision) Register Support for RV64D
//!
//! Extends the FPR to support 64-bit double precision floating-point values.
//! RV64D uses the same f0-f31 registers as RV64F, but stores full 64-bit values.
//! No NaN boxing is needed for double precision since it's already 64 bits.

use std::fmt;

/// Double precision floating-point register (f0-f31)
/// Stores 64-bit IEEE 754 double precision values
#[derive(Clone, Copy, Default)]
pub struct Dpr(u64);

impl Dpr {
    /// Create a new DPR from a f64 value
    pub fn new(value: f64) -> Self {
        Self(value.to_bits())
    }

    /// Create from raw bits
    pub fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Get the f64 value
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Get raw bits
    pub fn bits(&self) -> u64 {
        self.0
    }

    /// Get the canonical NaN for double precision
    pub fn canonical_nan() -> Self {
        // IEEE 754 canonical NaN: quiet NaN with MSB of significand set
        Self::from_bits(0x7FF8_0000_0000_0000u64)
    }

    /// Get the default NaN for operations
    pub fn default_nan() -> Self {
        Self::canonical_nan()
    }
}

impl fmt::Debug for Dpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

/// Helper functions for D extension
impl Dpr {
    /// Check if the value is a quiet NaN
    pub fn is_quiet_nan(&self) -> bool {
        self.get().is_nan()
    }

    /// Check if the value is signaling NaN
    pub fn is_signaling_nan(&self) -> bool {
        let bits = self.0;
        // Quiet NaN has the MSB of significand set (bit 51)
        // Signaling NaN has it clear
        let exp = (bits >> 52) & 0x7FF;
        let significand = bits & 0x000F_FFFF_FFFF_FFFF_u64;
        exp == 0x7FF && significand != 0 && (significand & (1 << 51)) == 0
    }

    /// Check if positive zero
    pub fn is_positive_zero(&self) -> bool {
        self.0 == 0
    }

    /// Check if negative zero
    pub fn is_negative_zero(&self) -> bool {
        self.0 == 0x8000_0000_0000_0000u64
    }
}

/// Extension trait for FpuRegisterFile to support D extension operations
pub trait DRegisterFile {
    /// Read a double precision value from register
    fn read_d(&self, reg: usize) -> Dpr;

    /// Write a double precision value to register
    fn write_d(&mut self, reg: usize, value: Dpr);

    /// Read lower 64 bits (for FLD)
    fn read_u64(&self, reg: usize) -> u64;

    /// Write 64 bits (for FSD)
    fn write_u64(&mut self, reg: usize, value: u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpr_new() {
        let value: f64 = std::f64::consts::PI;
        let dpr = Dpr::new(value);
        assert!((dpr.get() - value).abs() < 1e-10);
    }

    #[test]
    fn test_dpr_bits() {
        let value: f64 = std::f64::consts::E;
        let dpr = Dpr::new(value);
        assert_eq!(dpr.bits(), value.to_bits());
    }

    #[test]
    fn test_dpr_from_bits() {
        let raw_bits: u64 = std::f64::consts::PI.to_bits();
        let dpr = Dpr::from_bits(raw_bits);
        assert_eq!(dpr.get(), std::f64::consts::PI);
    }

    #[test]
    fn test_dpr_canonical_nan() {
        let nan = Dpr::canonical_nan();
        assert!(nan.get().is_nan());
    }

    #[test]
    fn test_dpr_default_nan() {
        let nan = Dpr::default_nan();
        assert!(nan.get().is_nan());
    }

    #[test]
    fn test_dpr_special_values() {
        // Positive zero
        let pos_zero = Dpr::new(0.0);
        assert!(pos_zero.is_positive_zero());

        // Negative zero
        let neg_zero = Dpr::new(-0.0);
        assert!(neg_zero.is_negative_zero());

        // Infinity
        let inf = Dpr::new(f64::INFINITY);
        assert_eq!(inf.get(), f64::INFINITY);

        // Negative infinity
        let neg_inf = Dpr::new(f64::NEG_INFINITY);
        assert_eq!(neg_inf.get(), f64::NEG_INFINITY);

        // NaN
        let nan = Dpr::new(f64::NAN);
        assert!(nan.get().is_nan());
    }

    #[test]
    fn test_dpr_quiet_nan() {
        let nan = Dpr::canonical_nan();
        assert!(nan.is_quiet_nan());
    }

    #[test]
    fn test_dpr_signaling_nan() {
        // Signaling NaN - MSB of significand clear
        let signaling_nan = Dpr::from_bits(0x7FF0_0000_0000_0001u64);
        assert!(signaling_nan.is_signaling_nan());
    }

    #[test]
    fn test_dpr_debug() {
        let dpr = Dpr::new(1.0);
        let debug_str = format!("{:?}", dpr);
        assert!(debug_str.contains("0x"));
    }

    #[test]
    fn test_dpr_default() {
        let dpr = Dpr::default();
        assert_eq!(dpr.bits(), 0);
    }

    #[test]
    fn test_dpr_clone_copy() {
        let dpr1 = Dpr::new(42.0);
        let dpr2 = dpr1; // Copy
        #[allow(clippy::clone_on_copy)]
        let dpr3 = dpr1.clone(); // Intentionally testing clone
        assert_eq!(dpr1.bits(), dpr2.bits());
        assert_eq!(dpr1.bits(), dpr3.bits());
    }
}
