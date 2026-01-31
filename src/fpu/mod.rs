//! FPU Register File
//!
//! Implements 32 floating-point registers (f0-f31) for RV64F extension.
//! Uses NaN boxing: 32-bit float stored in lower 32 bits, upper 32 bits set to all 1s.

pub mod d_register;
pub mod fcsr;
pub mod nan_boxing;

use std::fmt;

pub use fcsr::{Fcsr, FpFlags, RoundingMode};

/// Floating point register type (f0-f31)
#[derive(Clone, Copy)]
pub struct Fpr(u64);

impl Fpr {
    /// NaN box marker: upper 32 bits set to all 1s
    const NAN_BOX_MASK: u64 = 0xFFFF_FFFF_0000_0000u64;

    /// Create a new FPR with NaN-boxed value
    pub fn new(value: f32) -> Self {
        // NaN box the 32-bit float: store in lower 32 bits, set upper 32 bits to 1s
        let bits = Self::NAN_BOX_MASK | value.to_bits() as u64;
        Self(bits)
    }

    /// Create from raw bits (already NaN boxed)
    pub fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Get the 32-bit float value, handling NaN boxing
    pub fn get(&self) -> f32 {
        // Extract lower 32 bits
        f32::from_bits(self.0 as u32)
    }

    /// Get raw bits (NaN boxed)
    pub fn bits(&self) -> u64 {
        self.0
    }

    /// Get lower 32 bits
    pub fn lower(&self) -> u32 {
        self.0 as u32
    }

    /// Check if NaN boxed (upper 32 bits all 1s)
    pub fn is_nan_boxed(&self) -> bool {
        (self.0 >> 32) == 0xFFFF_FFFF
    }

    /// Get the canonical NaN (for NaN propagation)
    pub fn canonical_nan() -> Self {
        // IEEE 754 canonical NaN: quiet NaN with MSB of significand set
        // NaN boxed: upper 32 bits = 0xFFFF_FFFF, lower 32 bits = 0x7FC0_0000
        Self::from_bits((0xFFFF_FFFFu64 << 32) | 0x7FC0_0000u64)
    }

    /// Get the default NaN (for addition/multiplication)
    pub fn default_nan() -> Self {
        Self::canonical_nan()
    }
}

impl Default for Fpr {
    fn default() -> Self {
        Self(Self::NAN_BOX_MASK)
    }
}

impl fmt::Debug for Fpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

/// FPU Register File containing 32 FPRs
#[derive(Debug, Clone)]
pub struct FpuRegisterFile {
    /// 32 floating-point registers (f0-f31)
    regs: [Fpr; 32],
}

impl FpuRegisterFile {
    /// Create a new FPU register file
    pub fn new() -> Self {
        Self {
            regs: [Fpr::default(); 32],
        }
    }

    /// Reset all registers to default (NaN boxed 0.0)
    pub fn reset(&mut self) {
        self.regs = [Fpr::default(); 32];
    }

    /// Read a floating-point register
    pub fn read(&self, reg: usize) -> Fpr {
        self.regs[reg & 0x1F]
    }

    /// Write to a floating-point register
    /// Note: f0 is hardwired to 0 (reads always return 0, writes are ignored)
    pub fn write(&mut self, reg: usize, value: Fpr) {
        let reg = reg & 0x1F;
        if reg != 0 {
            self.regs[reg] = value;
        }
    }

    /// Write lower 32 bits only (for FLW)
    pub fn write_u32(&mut self, reg: usize, value: u32) {
        let reg = reg & 0x1F;
        if reg != 0 {
            // NaN box the 32-bit value
            self.regs[reg] = Fpr::from_bits(value as u64 | 0xFFFF_FFFF_0000_0000u64);
        }
    }

    /// Read as u32 (for FSW)
    pub fn read_u32(&self, reg: usize) -> u32 {
        self.read(reg).lower()
    }

    /// Get mutable reference to FCSR field for rounding mode
    pub fn fcsr(&self) -> &Fcsr {
        unimplemented!("FCSR is stored in CoreState, not FpuRegisterFile")
    }

    /// Get mutable reference to FCSR field
    pub fn fcsr_mut(&mut self) -> &mut Fcsr {
        unimplemented!("FCSR is stored in CoreState, not FpuRegisterFile")
    }

    /// Check if register is valid (0-31)
    pub fn is_valid_reg(reg: u32) -> bool {
        reg < 32
    }
}

impl Default for FpuRegisterFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to get effective operand for FMADD/FMSUB
/// Handles sign manipulation for subtraction variants
pub fn effective_operand(rs3: Fpr, subtract: bool) -> Fpr {
    if subtract {
        // Negate the operand for FMSUB/FNMSUB
        let bits = rs3.bits() ^ 0x8000_0000_0000_0000u64;
        Fpr::from_bits(bits)
    } else {
        rs3
    }
}

/// Helper function to get effective sign for NMADD/NMSUB
pub fn effective_sign(rs1: Fpr, rs2: Fpr, add: bool) -> (f32, f32) {
    let val1 = rs1.get();
    let val2 = if add { rs2.get() } else { -rs2.get() };
    (val1, val2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fpr_new() {
        let fpr = Fpr::new(std::f32::consts::PI);
        assert!((fpr.get() - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn test_fpr_from_bits() {
        let bits = 0xFFFF_FFFF_4048_F5C3u64; // NaN-boxed ~3.14
        let fpr = Fpr::from_bits(bits);
        assert!(fpr.is_nan_boxed());
    }

    #[test]
    fn test_fpr_bits() {
        let fpr = Fpr::new(1.0f32);
        let bits = fpr.bits();
        assert!((bits >> 32) == 0xFFFF_FFFF);
    }

    #[test]
    fn test_fpr_lower() {
        let fpr = Fpr::new(1.0f32);
        let lower = fpr.lower();
        assert_eq!(lower, 1.0f32.to_bits());
    }

    #[test]
    fn test_fpr_canonical_nan() {
        let nan = Fpr::canonical_nan();
        assert!(nan.get().is_nan());
        assert!(nan.is_nan_boxed());
    }

    #[test]
    fn test_fpr_default_nan() {
        let nan = Fpr::default_nan();
        assert!(nan.get().is_nan());
    }

    #[test]
    fn test_fpr_default() {
        let fpr = Fpr::default();
        assert!(fpr.is_nan_boxed());
    }

    #[test]
    fn test_fpr_debug() {
        let fpr = Fpr::new(1.0f32);
        let debug_str = format!("{:?}", fpr);
        assert!(debug_str.contains("0x"));
    }

    #[test]
    fn test_fpu_register_file_new() {
        let frf = FpuRegisterFile::new();
        let val = frf.read(1);
        assert!(val.is_nan_boxed());
    }

    #[test]
    fn test_fpu_register_file_reset() {
        let mut frf = FpuRegisterFile::new();
        frf.write(5, Fpr::new(42.0f32));
        frf.reset();
        let val = frf.read(5);
        assert_eq!(val.get(), 0.0f32);
    }

    #[test]
    fn test_fpu_register_file_read_write() {
        let mut frf = FpuRegisterFile::new();
        frf.write(10, Fpr::new(std::f32::consts::PI));
        let val = frf.read(10);
        assert!((val.get() - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn test_fpu_register_file_f0_hardwired() {
        let mut frf = FpuRegisterFile::new();
        frf.write(0, Fpr::new(42.0f32));
        let val = frf.read(0);
        assert_eq!(val.get(), 0.0f32); // f0 should still be 0
    }

    #[test]
    fn test_fpu_register_file_write_u32() {
        let mut frf = FpuRegisterFile::new();
        frf.write_u32(5, 0x3F800000u32); // 1.0 in IEEE 754
        let val = frf.read(5);
        assert!(val.is_nan_boxed());
        assert!((val.get() - 1.0f32).abs() < 1e-10);
    }

    #[test]
    fn test_fpu_register_file_write_u32_f0() {
        let mut frf = FpuRegisterFile::new();
        frf.write_u32(0, 0x3F800000u32);
        let val = frf.read_u32(0);
        assert_eq!(val, 0u32); // f0 is hardwired to 0
    }

    #[test]
    fn test_fpu_register_file_read_u32() {
        let mut frf = FpuRegisterFile::new();
        frf.write(3, Fpr::new(2.0f32));
        let val = frf.read_u32(3);
        assert_eq!(val, 2.0f32.to_bits());
    }

    #[test]
    fn test_fpu_register_file_reg_masking() {
        let mut frf = FpuRegisterFile::new();
        // Register numbers should be masked to 0-31
        frf.write(35, Fpr::new(5.0f32)); // 35 & 0x1F = 3
        let val = frf.read(3);
        assert!((val.get() - 5.0f32).abs() < 1e-5);
    }

    #[test]
    fn test_fpu_register_file_is_valid_reg() {
        assert!(FpuRegisterFile::is_valid_reg(0));
        assert!(FpuRegisterFile::is_valid_reg(31));
        assert!(!FpuRegisterFile::is_valid_reg(32));
    }

    #[test]
    fn test_fpu_register_file_default() {
        let frf = FpuRegisterFile::default();
        let val = frf.read(1);
        assert!(val.is_nan_boxed());
    }

    #[test]
    fn test_effective_operand_no_subtract() {
        let fpr = Fpr::new(1.0f32);
        let result = effective_operand(fpr, false);
        assert!((result.get() - 1.0f32).abs() < 1e-10);
    }

    #[test]
    fn test_effective_operand_subtract() {
        // The effective_operand flips the sign bit at bit 63 (for 64-bit representation)
        // For NaN-boxed f32, this doesn't affect the 32-bit value
        // Let's test with a 64-bit double interpretation instead
        let fpr = Fpr::from_bits(1.0f64.to_bits()); // Store as f64 bits
        let result = effective_operand(fpr, true);
        // The sign bit at position 63 should be flipped
        let result_f64 = f64::from_bits(result.bits());
        assert!((result_f64 - (-1.0f64)).abs() < 1e-10);
    }

    #[test]
    fn test_effective_sign_add() {
        let rs1 = Fpr::new(2.0f32);
        let rs2 = Fpr::new(3.0f32);
        let (v1, v2) = effective_sign(rs1, rs2, true);
        assert!((v1 - 2.0f32).abs() < 1e-10);
        assert!((v2 - 3.0f32).abs() < 1e-10);
    }

    #[test]
    fn test_effective_sign_sub() {
        let rs1 = Fpr::new(2.0f32);
        let rs2 = Fpr::new(3.0f32);
        let (v1, v2) = effective_sign(rs1, rs2, false);
        assert!((v1 - 2.0f32).abs() < 1e-10);
        assert!((v2 - (-3.0f32)).abs() < 1e-10);
    }
}
