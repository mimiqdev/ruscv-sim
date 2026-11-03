//! FPU Register File
//!
//! Implements the 32 floating-point registers (`f0`-`f31`) of an FP profile.
//! Every register is an ordinary 64-bit register; a 32-bit write sets the
//! upper 32 bits to all ones (NaN boxing), and a computational
//! single-precision read of a value that is not boxed yields the canonical
//! NaN (contract D5).

pub mod arith;
pub mod d_register;
pub mod fcsr;
pub mod nan_boxing;

use std::fmt;

pub use fcsr::{Fcsr, FpFlags, RoundingMode};
pub use nan_boxing::{CANONICAL_NAN_F32, NAN_BOX_MASK};

/// Floating point register type (f0-f31)
///
/// Every FPR is an ordinary 64-bit register; `f0` has no special role
/// (contract D5).  The register holds an f64 bit pattern, or an f32 value in
/// the low 32 bits with the upper 32 bits set to all ones (NaN boxing).
#[derive(Clone, Copy)]
pub struct Fpr(u64);

impl Fpr {
    /// Architectural reset value of every FPR (contract §3: "FPRs zero").
    pub const ZERO: Self = Self(0);

    /// Create a new FPR with NaN-boxed value
    pub fn new(value: f32) -> Self {
        // NaN box the 32-bit float: store in lower 32 bits, set upper 32 bits to 1s
        let bits = NAN_BOX_MASK | value.to_bits() as u64;
        Self(bits)
    }

    /// Create from raw bits (already NaN boxed)
    pub fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Computational single-precision read (contract D5).
    ///
    /// Every non-transfer single-precision operand read yields the canonical
    /// NaN `0x7fc00000` when the upper 32 bits of the register are not all
    /// ones.  Sign injection preserves the payload of a correctly boxed
    /// operand, which this read returns bit for bit.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.as_single_bits())
    }

    /// Computational single-precision read as bits (contract D5).
    pub fn as_single_bits(&self) -> u32 {
        nan_boxing::unbox_single_bits(self.0)
    }

    /// Raw low 32 bits, unchanged (contract D5).
    ///
    /// This is the transfer read used by FSW, FMV.X.W and a 32-bit debugger
    /// read.  It does not apply NaN-boxing: a value that is not boxed keeps
    /// whatever the low 32 bits hold.
    pub fn raw_low_u32(&self) -> u32 {
        self.0 as u32
    }

    /// Get lower 32 bits, unchanged (contract D5).
    ///
    /// Compatibility alias of [`Self::raw_low_u32`].
    pub fn lower(&self) -> u32 {
        self.raw_low_u32()
    }

    /// Get raw bits (NaN boxed)
    pub fn bits(&self) -> u64 {
        self.0
    }

    /// Check if NaN boxed (upper 32 bits all 1s)
    pub fn is_nan_boxed(&self) -> bool {
        nan_boxing::is_nan_boxed(self.0)
    }

    /// Get the canonical NaN (for NaN propagation), NaN boxed for single
    /// precision (contract D4/D5).
    pub fn canonical_nan() -> Self {
        Self(NAN_BOX_MASK | u64::from(CANONICAL_NAN_F32))
    }

    /// The default NaN an arithmetic operation produces: the canonical NaN
    /// (contract D4/D5).
    pub fn default_nan() -> Self {
        Self::canonical_nan()
    }
}

impl Default for Fpr {
    fn default() -> Self {
        Self::ZERO
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
    /// Subscribed write identities (including equal-value writes), not a sink.
    observed_writes: Option<u32>,
}

impl FpuRegisterFile {
    /// Create a new FPU register file.  Every FPR is architecturally zero
    /// until software writes it (contract §3).
    pub fn new() -> Self {
        Self {
            regs: [Fpr::ZERO; 32],
            observed_writes: None,
        }
    }

    /// Reset all registers to the architectural reset value, zero.
    pub fn reset(&mut self) {
        self.regs = [Fpr::ZERO; 32];
        self.observed_writes = None;
    }

    pub(crate) fn begin_observation(&mut self, enabled: bool) {
        self.observed_writes = enabled.then_some(0);
    }

    pub(crate) fn observed_writes(&self) -> u32 {
        self.observed_writes.unwrap_or(0)
    }

    fn observe_write(&mut self, index: usize) {
        if let Some(writes) = &mut self.observed_writes {
            *writes |= 1 << index;
        }
    }

    /// Read a floating-point register.  `f0` is an ordinary register and has
    /// no hard-wired value (contract D5).
    pub fn read(&self, reg: usize) -> Fpr {
        self.regs[reg & 0x1F]
    }

    /// Write a 64-bit floating-point register value.
    ///
    /// `f0` is an ordinary register; the write is not ignored and is
    /// observed like any other (contract D5).
    pub fn write(&mut self, reg: usize, value: Fpr) {
        let reg = reg & 0x1F;
        self.observe_write(reg);
        self.regs[reg] = value;
    }

    /// Write a 32-bit value: every 32-bit write NaN-boxes (contract D5).
    pub fn write_u32(&mut self, reg: usize, value: u32) {
        self.write(reg, Fpr::from_bits(NAN_BOX_MASK | u64::from(value)));
    }

    /// Computational single-precision read (contract D5).
    ///
    /// An FPR whose upper 32 bits are not all ones yields the canonical NaN
    /// `0x7fc00000`.
    pub fn read_single_bits(&self, reg: usize) -> u32 {
        self.read(reg).as_single_bits()
    }

    /// Raw low 32 bits, unchanged (contract D5).
    ///
    /// Used by FSW, FMV.X.W and a 32-bit debugger read, which move the low
    /// half of the register without applying NaN boxing.
    pub fn read_raw_u32(&self, reg: usize) -> u32 {
        self.read(reg).raw_low_u32()
    }

    /// Raw low 32 bits of a register.
    ///
    /// Compatibility alias of [`Self::read_raw_u32`]: it is the transfer
    /// read, not the computational single-precision read.
    pub fn read_u32(&self, reg: usize) -> u32 {
        self.read_raw_u32(reg)
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
        let lower = fpr.raw_low_u32();
        assert_eq!(lower, 1.0f32.to_bits());
    }

    #[test]
    fn test_fpr_canonical_nan() {
        let nan = Fpr::canonical_nan();
        assert!(nan.get().is_nan());
        assert!(nan.is_nan_boxed());
    }

    #[test]
    fn test_fpr_default_nan_is_the_canonical_nan() {
        assert_eq!(Fpr::default_nan().bits(), Fpr::canonical_nan().bits());
    }

    #[test]
    fn test_fpr_default_is_architectural_zero() {
        let fpr = Fpr::default();
        assert_eq!(fpr.bits(), 0);
        // Zero is not a NaN box, so a computational single read yields the
        // canonical NaN (contract D5).
        assert_eq!(fpr.as_single_bits(), CANONICAL_NAN_F32);
        assert_eq!(fpr.raw_low_u32(), 0);
    }

    #[test]
    fn test_fpr_unboxed_computational_read_yields_canonical_nan() {
        let fpr = Fpr::from_bits(0x3FF0_0000_0000_0000); // 1.0 as an f64
        assert!(!fpr.is_nan_boxed());
        assert_eq!(fpr.get().to_bits(), CANONICAL_NAN_F32);
        assert_eq!(fpr.as_single_bits(), CANONICAL_NAN_F32);
        // A transfer read keeps the low 32 bits unchanged.
        assert_eq!(fpr.raw_low_u32(), 0);
        assert_eq!(fpr.raw_low_u32(), 0);
        assert_eq!(fpr.lower(), 0);
        assert_eq!(fpr.bits(), 0x3FF0_0000_0000_0000);
    }

    #[test]
    fn test_fpr_boxed_computational_read_preserves_payload() {
        // A NaN-boxed signaling NaN keeps its payload on a computational read.
        let fpr = Fpr::from_bits(NAN_BOX_MASK | 0x7F80_0001);
        assert_eq!(fpr.as_single_bits(), 0x7F80_0001);
        assert_eq!(fpr.raw_low_u32(), 0x7F80_0001);
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
        // Architectural reset state is zero for every FPR (contract §3).
        for reg in 0..32 {
            assert_eq!(frf.read(reg).bits(), 0, "f{reg}");
        }
    }

    #[test]
    fn test_fpu_register_file_reset() {
        let mut frf = FpuRegisterFile::new();
        frf.write(5, Fpr::new(42.0f32));
        frf.write_u32(0, 0x3F80_0000);
        frf.reset();
        assert_eq!(frf.read(5).bits(), 0);
        assert_eq!(frf.read(0).bits(), 0);
    }

    #[test]
    fn test_fpu_register_file_read_write() {
        let mut frf = FpuRegisterFile::new();
        frf.write(10, Fpr::new(std::f32::consts::PI));
        let val = frf.read(10);
        assert!((val.get() - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn test_fpu_register_file_f0_is_ordinary() {
        let mut frf = FpuRegisterFile::new();
        frf.write(0, Fpr::new(42.0f32));
        assert_eq!(frf.read(0).get(), 42.0f32);
        frf.write_u32(0, 0x3F80_0000);
        assert_eq!(frf.read(0).get(), 1.0f32);
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
        let val = frf.read_raw_u32(0);
        assert_eq!(val, 0x3F80_0000); // f0 is an ordinary register
        assert!(frf.read(0).is_nan_boxed());
    }

    #[test]
    fn test_fpu_register_file_read_u32() {
        let mut frf = FpuRegisterFile::new();
        frf.write(3, Fpr::new(2.0f32));
        let val = frf.read_raw_u32(3);
        assert_eq!(val, 2.0f32.to_bits());
        assert_eq!(frf.read_u32(3), val);
        assert_eq!(frf.read_single_bits(3), 2.0f32.to_bits());
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
        assert_eq!(frf.read(1).bits(), 0);
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
