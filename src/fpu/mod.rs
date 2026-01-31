//! FPU Register File
//!
//! Implements 32 floating-point registers (f0-f31) for RV64F extension.
//! Uses NaN boxing: 32-bit float stored in lower 32 bits, upper 32 bits set to all 1s.

pub mod fcsr;

use crate::decode::InstructionFormat;
use crate::execute::ExecuteError;
use crate::memory::{MemoryError, MemoryInterface};
use std::fmt;

pub use fcsr::{Fcsr, FpFlags, RoundingMode};

/// Floating point register type (f0-f31)
#[derive(Clone, Copy, Default)]
pub struct Fpr(u64);

impl Fpr {
    /// Create a new FPR with NaN-boxed value
    pub fn new(value: f32) -> Self {
        // NaN box the 32-bit float: store in lower 32 bits, set upper 32 bits to 1s
        let bits = value.to_bits() as u64 | 0xFFFF_FFFF_0000_0000u64;
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
        Self::from_bits(0x7FC0_0000_FFFF_FFFFu64)
    }

    /// Get the default NaN (for addition/multiplication)
    pub fn default_nan() -> Self {
        Self::from_bits(0x7FC0_0000_FFFF_FFFFu64)
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
    let mut val1 = rs1.get();
    let mut val2 = rs2.get();
    if !add {
        val2 = -val2;
    }
    (val1, val2)
}
