//! Shared rounding modes, exception flags and integer helpers.
//!
//! The helpers mirror the Berkeley SoftFloat 3e primitives of the same names
//! (`softfloat_shiftRightJam*`, `softfloat_shortShiftRightJam*`,
//! `softfloat_countLeadingZeros*`, the 128-bit add/sub/shift primitives and
//! `softfloat_roundToI32`/`I64`/`UI32`/`UI64`). Keep them structurally close
//! to the reference so a reviewer can diff them.
//!
//! Portions of this module are a Rust port of Berkeley SoftFloat Release 3e
//! ("This C source file is part of the SoftFloat IEEE Floating-Point
//! Arithmetic Package, Release 3e, by John R. Hauser"; Copyright 2011-2018 The
//! Regents of the University of California). See
//! `tools/fpu/SOFTFLOAT-LICENSE.txt` for the copyright notice, conditions and
//! disclaimer.

use bitflags::bitflags;

/// IEEE 754-2008 rounding direction, using the RISC-V `rm` encoding order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Round to nearest, ties to even (`rm = 0`).
    Rne,
    /// Round towards zero (`rm = 1`).
    Rtz,
    /// Round down, towards negative infinity (`rm = 2`).
    Rdn,
    /// Round up, towards positive infinity (`rm = 3`).
    Rup,
    /// Round to nearest, ties to maximum magnitude (`rm = 4`).
    Rmm,
}

impl Rounding {
    /// Decode a resolved 3-bit rounding-mode value.
    ///
    /// Returns `None` for encodings 5, 6 and 7. Callers must resolve a static
    /// `rm` first: `rm = 7` selects the dynamic mode from `frm`, so it never
    /// reaches this function as 7. Contract D5 makes a reserved static `rm`
    /// (5 or 6) and a dynamic mode whose `frm` holds 5, 6 or 7 an
    /// illegal-instruction.
    pub const fn from_encoding(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::Rne),
            1 => Some(Self::Rtz),
            2 => Some(Self::Rdn),
            3 => Some(Self::Rup),
            4 => Some(Self::Rmm),
            _ => None,
        }
    }
}

bitflags! {
    /// The five IEEE 754 exception flags, in architectural `fflags` bit
    /// order (F: `fflags[0]` is `NX` through `fflags[4]` `NV`).
    ///
    /// The bit layout deliberately matches `fflags`/`fcsr[4:0]`, so the
    /// execution layer can accrue [`FpFlags::bits()`] directly.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FpFlags: u8 {
        /// Inexact.
        const NX = 1 << 0;
        /// Underflow.
        const UF = 1 << 1;
        /// Overflow.
        const OF = 1 << 2;
        /// Divide by zero.
        const DZ = 1 << 3;
        /// Invalid operation.
        const NV = 1 << 4;
    }
}

/// `softfloat_shiftRightJam32`.
pub(crate) fn shift_right_jam32(a: u32, dist: u32) -> u32 {
    if dist < 31 {
        (a >> dist) | (((a << ((-(dist as i32)) & 31)) != 0) as u32)
    } else {
        (a != 0) as u32
    }
}

/// `softfloat_shiftRightJam64`.
pub(crate) fn shift_right_jam64(a: u64, dist: u32) -> u64 {
    if dist < 63 {
        (a >> dist) | (((a << ((-(dist as i32)) & 63)) != 0) as u64)
    } else {
        (a != 0) as u64
    }
}

/// `softfloat_shortShiftRightJam64`.
pub(crate) fn short_shift_right_jam64(a: u64, dist: u32) -> u64 {
    (a >> dist) | (((a & ((1u64 << dist) - 1)) != 0) as u64)
}

/// `softfloat_shiftRightJam64Extra`.
pub(crate) fn shift_right_jam64_extra(a: u64, extra: u64, dist: u32) -> (u64, u64) {
    let (v, mut e) = if dist < 64 {
        (a >> dist, a << ((-(dist as i32)) & 63))
    } else {
        (0, if dist == 64 { a } else { (a != 0) as u64 })
    };
    e |= (extra != 0) as u64;
    (v, e)
}

/// `softfloat_countLeadingZeros32`.
pub(crate) fn count_leading_zeros32(a: u32) -> u32 {
    a.leading_zeros()
}

/// `softfloat_countLeadingZeros64`.
pub(crate) fn count_leading_zeros64(a: u64) -> u32 {
    a.leading_zeros()
}

/// `softfloat_mul64To128`, returning `(high, low)`.
pub(crate) fn mul64_to128(a: u64, b: u64) -> (u64, u64) {
    let p = (a as u128) * (b as u128);
    ((p >> 64) as u64, p as u64)
}

/// `softfloat_add128`, returning `(high, low)`.
pub(crate) fn add128(a64: u64, a0: u64, b64: u64, b0: u64) -> (u64, u64) {
    let (v0, carry) = a0.overflowing_add(b0);
    (a64.wrapping_add(b64).wrapping_add(carry as u64), v0)
}

/// `softfloat_sub128`, returning `(high, low)`.
pub(crate) fn sub128(a64: u64, a0: u64, b64: u64, b0: u64) -> (u64, u64) {
    let (v0, borrow) = a0.overflowing_sub(b0);
    (a64.wrapping_sub(b64).wrapping_sub(borrow as u64), v0)
}

/// `softfloat_shortShiftLeft128`, returning `(high, low)`.
pub(crate) fn short_shift_left128(a64: u64, a0: u64, dist: u32) -> (u64, u64) {
    ((a64 << dist) | (a0 >> ((-(dist as i32)) & 63)), a0 << dist)
}

/// `softfloat_shortShiftRightJam128`, returning `(high, low)`.
pub(crate) fn short_shift_right_jam128(a64: u64, a0: u64, dist: u32) -> (u64, u64) {
    let neg = (-(dist as i32)) & 63;
    (
        a64 >> dist,
        (a64 << neg) | (a0 >> dist) | (((a0 << neg) != 0) as u64),
    )
}

/// `softfloat_shiftRightJam128`, returning `(high, low)`.
pub(crate) fn shift_right_jam128(a64: u64, a0: u64, dist: u32) -> (u64, u64) {
    if dist < 64 {
        let neg = (-(dist as i32)) & 63;
        (
            a64 >> dist,
            (a64 << neg) | (a0 >> dist) | (((a0 << neg) != 0) as u64),
        )
    } else if dist < 127 {
        let d = dist & 63;
        (
            0,
            (a64 >> d) | ((((a64 & ((1u64 << d) - 1)) | a0) != 0) as u64),
        )
    } else {
        (0, ((a64 | a0) != 0) as u64)
    }
}

/// The RISC-V-specified result of an invalid floating-point-to-integer
/// conversion: out-of-range and NaN inputs saturate, independent of `rm`.
///
/// The 32-bit results are sign-extended to 64 bits as the RV64 F/D manual
/// requires (`FCVT.W[U].S` writes `sext(result[31:0])`).
#[derive(Clone, Copy)]
pub(crate) enum IntSat {
    /// Signed 32-bit destination.
    I32,
    /// Unsigned 32-bit destination, sign-extended into the u64 result.
    Ui32,
    /// Signed 64-bit destination.
    I64,
    /// Unsigned 64-bit destination.
    Ui64,
}

impl IntSat {
    /// Result for `+inf`, a positively out-of-range value, or a NaN.
    pub(crate) const fn positive(self) -> u64 {
        match self {
            Self::I32 => 0x0000_0000_7fff_ffff,
            Self::Ui32 => 0xffff_ffff_ffff_ffff,
            Self::I64 => 0x7fff_ffff_ffff_ffff,
            Self::Ui64 => 0xffff_ffff_ffff_ffff,
        }
    }

    /// Result for `-inf` or a negatively out-of-range value.
    pub(crate) const fn negative(self) -> u64 {
        match self {
            Self::I32 => 0xffff_ffff_8000_0000,
            Self::Ui32 => 0,
            Self::I64 => 0x8000_0000_0000_0000,
            Self::Ui64 => 0,
        }
    }

    /// Result for a NaN: the saturation table always uses the positive end.
    pub(crate) const fn nan(self) -> u64 {
        self.positive()
    }

    const fn is_32_bit(self) -> bool {
        matches!(self, Self::I32 | Self::Ui32)
    }
}

/// `softfloat_roundToI32` / `softfloat_roundToUI32`.
///
/// `sig` is the magnitude with a 12-bit fraction field, as produced by the
/// conversion wrappers. Returns the (sign-extended) result and exact flags.
pub(crate) fn round_to_i32(sign: bool, sig: u64, sat: IntSat, rm: Rounding) -> (u64, FpFlags) {
    debug_assert!(sat.is_32_bit());
    let mut flags = FpFlags::empty();
    let round_increment: u64 = match rm {
        Rounding::Rne | Rounding::Rmm => 0x800,
        Rounding::Rdn if sign => 0xfff,
        Rounding::Rup if !sign => 0xfff,
        _ => 0,
    };
    let round_bits = sig & 0xfff;
    let sig = sig.wrapping_add(round_increment);
    if sig & 0xffff_f000_0000_0000 != 0 {
        return (invalid_int_result(sign, false, sat), FpFlags::NV);
    }
    let mut z = (sig >> 12) as u32;
    if round_bits == 0x800 && rm == Rounding::Rne {
        z &= !1;
    }
    let z_i32 = (if sign { 0u32.wrapping_sub(z) } else { z }) as i32;
    let invalid = match sat {
        IntSat::I32 => z_i32 != 0 && ((z_i32 < 0) != sign),
        IntSat::Ui32 => sign && z != 0,
        _ => unreachable!(),
    };
    if invalid {
        return (invalid_int_result(sign, false, sat), FpFlags::NV);
    }
    if round_bits != 0 {
        flags |= FpFlags::NX;
    }
    let result = match sat {
        IntSat::I32 => (z_i32 as i64) as u64,
        IntSat::Ui32 => (z as i32 as i64) as u64,
        _ => unreachable!(),
    };
    (result, flags)
}

/// `softfloat_roundToI64` / `softfloat_roundToUI64`.
pub(crate) fn round_to_i64(
    sign: bool,
    sig: u64,
    sig_extra: u64,
    sat: IntSat,
    rm: Rounding,
) -> (u64, FpFlags) {
    debug_assert!(!sat.is_32_bit());
    let mut flags = FpFlags::empty();
    let mut sig = sig;
    let round_near = matches!(rm, Rounding::Rne | Rounding::Rmm);
    let increment = if round_near {
        sig_extra >= 0x8000_0000_0000_0000
    } else {
        sig_extra != 0
            && match rm {
                Rounding::Rdn if sign => true,
                Rounding::Rup if !sign => true,
                _ => false,
            }
    };
    if increment {
        sig = sig.wrapping_add(1);
        if sig == 0 {
            return (invalid_int_result(sign, false, sat), FpFlags::NV);
        }
        if sig_extra == 0x8000_0000_0000_0000 && rm == Rounding::Rne {
            sig &= !1;
        }
    }
    let result = if sign { (0u64).wrapping_sub(sig) } else { sig };
    let invalid = match sat {
        // The signed destination cannot hold a magnitude that negates into a
        // positive value.
        IntSat::I64 => result != 0 && ((result as i64) < 0) != sign,
        IntSat::Ui64 => sign && sig != 0,
        _ => unreachable!(),
    };
    if invalid {
        return (invalid_int_result(sign, false, sat), FpFlags::NV);
    }
    if sig_extra != 0 {
        flags |= FpFlags::NX;
    }
    (result, flags)
}

/// Result of an invalid conversion; `is_nan` selects the positive end.
pub(crate) fn invalid_int_result(sign: bool, is_nan: bool, sat: IntSat) -> u64 {
    if is_nan {
        sat.nan()
    } else if sign {
        sat.negative()
    } else {
        sat.positive()
    }
}

/// Integer square root of `x`, returning `(floor(sqrt(x)), x - floor^2)`.
///
/// Used by `FSQRT.S`/`FSQRT.D` to obtain the truncated square root and an
/// exact sticky term, so the result is rounded once by the format's
/// round-pack step.
pub(crate) fn isqrt_u128(x: u128) -> (u64, u128) {
    if x == 0 {
        return (0, 0);
    }
    // Newton iteration from a power-of-two upper bound.
    let mut r: u128 = 1 << (128 - x.leading_zeros()).div_ceil(2);
    loop {
        let next = (r + x / r) >> 1;
        if next >= r {
            break;
        }
        r = next;
    }
    while r * r > x {
        r -= 1;
    }
    (r as u64, x - r * r)
}
