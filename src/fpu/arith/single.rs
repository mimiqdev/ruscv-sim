//! Single-precision (`F` extension) arithmetic.
//!
//! Every function takes and returns raw 32-bit IEEE 754 patterns and an
//! explicit [`Rounding`] mode where rounding applies, and returns the exact
//! exception flags. NaN results of arithmetic, square root, fused
//! multiply-add and conversion are the canonical NaN; sign injection,
//! classification and comparisons preserve or inspect payloads.

use super::common::*;

const SIGN: u32 = 0x8000_0000;
const EXP_MASK: u32 = 0x7f80_0000;
const FRAC_MASK: u32 = 0x007f_ffff;
const CANONICAL_NAN: u32 = 0x7fc0_0000;

#[inline]
fn sign(ui: u32) -> bool {
    ui & SIGN != 0
}

#[inline]
fn exp(ui: u32) -> i32 {
    ((ui & EXP_MASK) >> 23) as i32
}

#[inline]
fn frac(ui: u32) -> u32 {
    ui & FRAC_MASK
}

#[inline]
fn pack(sign: bool, exp: u32, sig: u32) -> u32 {
    ((sign as u32) << 31) + (exp << 23) + sig
}

/// True when the bits encode a quiet or signaling NaN.
pub fn is_nan(ui: u32) -> bool {
    ui & EXP_MASK == EXP_MASK && ui & FRAC_MASK != 0
}

/// True when the bits encode a signaling NaN.
pub fn is_signaling_nan(ui: u32) -> bool {
    is_nan(ui) && ui & 0x0040_0000 == 0
}

fn nan_result(flags: &mut FpFlags, operands: &[u32]) -> u32 {
    if operands.iter().any(|&ui| is_signaling_nan(ui)) {
        *flags |= FpFlags::NV;
    }
    CANONICAL_NAN
}

/// `softfloat_normSubnormalF32Sig`: returns `(exp, sig)` with the significand
/// normalized to bit 23.
fn norm_subnormal_sig(sig: u32) -> (i32, u32) {
    let shift_dist = count_leading_zeros32(sig) as i32 - 8;
    (1 - shift_dist, sig << shift_dist)
}

/// `softfloat_roundPackToF32`, with tininess detected after rounding.
fn round_pack(sign: bool, mut exp: i32, mut sig: u32, rm: Rounding, flags: &mut FpFlags) -> u32 {
    let round_near_even = rm == Rounding::Rne;
    let mut round_increment: u32 = 0x40;
    if !round_near_even && rm != Rounding::Rmm {
        round_increment = if rm == if sign { Rounding::Rdn } else { Rounding::Rup } {
            0x7f
        } else {
            0
        };
    }
    let mut round_bits = sig & 0x7f;

    if exp < 0 {
        // Tininess after rounding: the result is tiny when it stays below the
        // smallest normal even after the round-up increment.
        let is_tiny = exp < -1 || (sig as u64 + round_increment as u64) < 0x8000_0000;
        sig = shift_right_jam32(sig, (-exp) as u32);
        exp = 0;
        round_bits = sig & 0x7f;
        if is_tiny && round_bits != 0 {
            *flags |= FpFlags::UF;
        }
    } else if exp >= 0xfd && (exp > 0xfd || sig as u64 + round_increment as u64 >= 0x8000_0000) {
        *flags |= FpFlags::OF | FpFlags::NX;
        return pack(sign, 0xff, 0) - ((round_increment == 0) as u32);
    }

    sig = (sig + round_increment) >> 7;
    if round_bits != 0 {
        *flags |= FpFlags::NX;
    }
    sig &= !((round_bits ^ 0x40 == 0 && round_near_even) as u32);
    if sig == 0 {
        exp = 0;
    }
    pack(sign, exp as u32, sig)
}

/// `softfloat_normRoundPackToF32`.
fn norm_round_pack(sign: bool, mut exp: i32, sig: u32, rm: Rounding, flags: &mut FpFlags) -> u32 {
    let shift_dist = count_leading_zeros32(sig) as i32 - 1;
    exp -= shift_dist;
    if shift_dist >= 7 && (0..0xfd).contains(&exp) {
        pack(
            sign,
            if sig != 0 { exp as u32 } else { 0 },
            sig << (shift_dist - 7),
        )
    } else {
        round_pack(sign, exp, sig << shift_dist, rm, flags)
    }
}

/// `softfloat_addMagsF32`: both operands have the same sign.
fn add_mags(ui_a: u32, ui_b: u32, rm: Rounding, flags: &mut FpFlags) -> u32 {
    let exp_a = exp(ui_a);
    let sig_a = frac(ui_a);
    let exp_b = exp(ui_b);
    let sig_b = frac(ui_b);
    let exp_diff = exp_a - exp_b;

    if exp_diff == 0 {
        if exp_a == 0 {
            return ui_a.wrapping_add(sig_b);
        }
        if exp_a == 0xff {
            if sig_a | sig_b != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        let sign_z = sign(ui_a);
        let exp_z = exp_a;
        let sig_z = 0x0100_0000 + sig_a + sig_b;
        if sig_z & 1 == 0 && exp_z < 0xfe {
            return pack(sign_z, exp_z as u32, sig_z >> 1);
        }
        return round_pack(sign_z, exp_z, sig_z << 6, rm, flags);
    }

    let sign_z = sign(ui_a);
    let mut sig_a = sig_a << 6;
    let mut sig_b = sig_b << 6;
    let exp_z;
    if exp_diff < 0 {
        if exp_b == 0xff {
            if sig_b != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return pack(sign_z, 0xff, 0);
        }
        if exp_a != 0 {
            sig_a += 0x2000_0000;
        } else {
            sig_a += sig_a;
        }
        sig_a = shift_right_jam32(sig_a, (-exp_diff) as u32);
        exp_z = exp_b;
    } else {
        if exp_a == 0xff {
            if sig_a != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        if exp_b != 0 {
            sig_b += 0x2000_0000;
        } else {
            sig_b += sig_b;
        }
        sig_b = shift_right_jam32(sig_b, exp_diff as u32);
        exp_z = exp_a;
    }
    let mut sig_z = 0x2000_0000 + sig_a + sig_b;
    let mut exp_z = exp_z;
    if sig_z < 0x4000_0000 {
        exp_z -= 1;
        sig_z <<= 1;
    }
    round_pack(sign_z, exp_z, sig_z, rm, flags)
}

/// `softfloat_subMagsF32`: the operands have opposite signs.
fn sub_mags(ui_a: u32, ui_b: u32, rm: Rounding, flags: &mut FpFlags) -> u32 {
    let exp_a0 = exp(ui_a);
    let sig_a0 = frac(ui_a);
    let exp_b = exp(ui_b);
    let sig_b0 = frac(ui_b);
    let exp_diff = exp_a0 - exp_b;

    if exp_diff == 0 {
        if exp_a0 == 0xff {
            if sig_a0 | sig_b0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            *flags |= FpFlags::NV;
            return CANONICAL_NAN;
        }
        let sig_diff = sig_a0 as i32 - sig_b0 as i32;
        if sig_diff == 0 {
            return pack(rm == Rounding::Rdn, 0, 0);
        }
        let exp_a = if exp_a0 != 0 { exp_a0 - 1 } else { 0 };
        let mut sign_z = sign(ui_a);
        let sig_diff = if sig_diff < 0 {
            sign_z = !sign_z;
            -sig_diff as u32
        } else {
            sig_diff as u32
        };
        let mut shift_dist = count_leading_zeros32(sig_diff) as i32 - 8;
        let mut exp_z = exp_a - shift_dist;
        if exp_z < 0 {
            shift_dist = exp_a;
            exp_z = 0;
        }
        return pack(sign_z, exp_z as u32, sig_diff << shift_dist);
    }

    let mut sign_z = sign(ui_a);
    let sig_a = sig_a0 << 7;
    let sig_b = sig_b0 << 7;
    let mut exp_diff = exp_diff;
    let exp_z;
    let sig_x;
    let sig_y;
    if exp_diff < 0 {
        sign_z = !sign_z;
        if exp_b == 0xff {
            if sig_b0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return pack(sign_z, 0xff, 0);
        }
        exp_z = exp_b - 1;
        sig_x = sig_b | 0x4000_0000;
        sig_y = sig_a + if exp_a0 != 0 { 0x4000_0000 } else { sig_a };
        exp_diff = -exp_diff;
    } else {
        if exp_a0 == 0xff {
            if sig_a0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        exp_z = exp_a0 - 1;
        sig_x = sig_a | 0x4000_0000;
        sig_y = sig_b + if exp_b != 0 { 0x4000_0000 } else { sig_b };
    }
    norm_round_pack(
        sign_z,
        exp_z,
        sig_x - shift_right_jam32(sig_y, exp_diff as u32),
        rm,
        flags,
    )
}

/// `FADD.S`: `a + b`.
pub fn add(a: u32, b: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let z = if sign(a ^ b) {
        sub_mags(a, b, rm, &mut flags)
    } else {
        add_mags(a, b, rm, &mut flags)
    };
    (z, flags)
}

/// `FSUB.S`: `a - b`.
pub fn sub(a: u32, b: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let z = if sign(a ^ b) {
        add_mags(a, b, rm, &mut flags)
    } else {
        sub_mags(a, b, rm, &mut flags)
    };
    (z, flags)
}

/// `FMUL.S`: `a * b`.
pub fn mul(a: u32, b: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);
    let sign_b = sign(b);
    let mut exp_b = exp(b);
    let mut sig_b = frac(b);
    let sign_z = sign_a ^ sign_b;

    let mag_bits;
    if exp_a == 0xff {
        if sig_a != 0 || (exp_b == 0xff && sig_b != 0) {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        mag_bits = exp_b | sig_b as i32;
        return inf_arg(sign_z, mag_bits, &mut flags);
    }
    if exp_b == 0xff {
        if sig_b != 0 {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        mag_bits = exp_a | sig_a as i32;
        return inf_arg(sign_z, mag_bits, &mut flags);
    }
    if exp_a == 0 {
        if sig_a == 0 {
            return (pack(sign_z, 0, 0), flags);
        }
        let (e, s) = norm_subnormal_sig(sig_a);
        exp_a = e;
        sig_a = s;
    }
    if exp_b == 0 {
        if sig_b == 0 {
            return (pack(sign_z, 0, 0), flags);
        }
        let (e, s) = norm_subnormal_sig(sig_b);
        exp_b = e;
        sig_b = s;
    }
    let mut exp_z = exp_a + exp_b - 0x7f;
    let sig_a = (sig_a | 0x0080_0000) << 7;
    let sig_b = (sig_b | 0x0080_0000) << 8;
    let mut sig_z = short_shift_right_jam64(sig_a as u64 * sig_b as u64, 32) as u32;
    if sig_z < 0x4000_0000 {
        exp_z -= 1;
        sig_z <<= 1;
    }
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

fn inf_arg(sign_z: bool, mag_bits: i32, flags: &mut FpFlags) -> (u32, FpFlags) {
    let mut f = *flags;
    let z = if mag_bits == 0 {
        f |= FpFlags::NV;
        CANONICAL_NAN
    } else {
        pack(sign_z, 0xff, 0)
    };
    (z, f)
}

/// `FDIV.S`: `a / b`.
pub fn div(a: u32, b: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);
    let sign_b = sign(b);
    let mut exp_b = exp(b);
    let mut sig_b = frac(b);
    let sign_z = sign_a ^ sign_b;

    if exp_a == 0xff {
        if sig_a != 0 {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        if exp_b == 0xff {
            if sig_b != 0 {
                return (nan_result(&mut flags, &[a, b]), flags);
            }
            flags |= FpFlags::NV;
            return (CANONICAL_NAN, flags);
        }
        return (pack(sign_z, 0xff, 0), flags);
    }
    if exp_b == 0xff {
        if sig_b != 0 {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        return (pack(sign_z, 0, 0), flags);
    }
    if exp_b == 0 {
        if sig_b == 0 {
            if exp_a == 0 && sig_a == 0 {
                flags |= FpFlags::NV;
                return (CANONICAL_NAN, flags);
            }
            flags |= FpFlags::DZ;
            return (pack(sign_z, 0xff, 0), flags);
        }
        let (e, s) = norm_subnormal_sig(sig_b);
        exp_b = e;
        sig_b = s;
    }
    if exp_a == 0 {
        if sig_a == 0 {
            return (pack(sign_z, 0, 0), flags);
        }
        let (e, s) = norm_subnormal_sig(sig_a);
        exp_a = e;
        sig_a = s;
    }
    let mut exp_z = exp_a - exp_b + 0x7e;
    let mut sig_a = (sig_a | 0x0080_0000) as u128;
    let sig_b = (sig_b | 0x0080_0000) as u128;
    // Normalize the ratio to [1, 2) so the quotient has its leading bit at
    // bit 30 (the position `round_pack` expects for the 24-bit significand
    // plus 7 guard bits).
    if sig_a < sig_b {
        sig_a <<= 1;
        exp_z -= 1;
    }
    let num = sig_a << 30;
    let sig_z = (num / sig_b) as u32 | (!num.is_multiple_of(sig_b)) as u32;
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

/// `FSQRT.S`.
pub fn sqrt(a: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);

    if exp_a == 0xff {
        if sig_a != 0 {
            return (nan_result(&mut flags, &[a]), flags);
        }
        if !sign_a {
            return (a, flags);
        }
        flags |= FpFlags::NV;
        return (CANONICAL_NAN, flags);
    }
    if sign_a {
        if exp_a == 0 && sig_a == 0 {
            return (a, flags);
        }
        flags |= FpFlags::NV;
        return (CANONICAL_NAN, flags);
    }
    if exp_a == 0 {
        if sig_a == 0 {
            return (a, flags);
        }
        let (e, s) = norm_subnormal_sig(sig_a);
        exp_a = e;
        sig_a = s;
    }

    // value = sig * 2^(exp_a - 127 - 23). `parity` makes the exponent even so
    // the square root exponent is an integer; `k` scales sqrt(sig) to the
    // 31-bit round_pack layout (24-bit significand plus 7 guard bits).
    let e = exp_a - 0x7f - 23;
    let parity = (e & 1) as u32;
    let sig2 = ((sig_a | 0x0080_0000) as u128) << parity;
    let len = 128 - sig2.leading_zeros();
    let k = 30 - (len - 1) / 2;
    let half_exp = (e - parity as i32) / 2;
    let exp_z = 156 + half_exp - k as i32;
    let (mut sig_z, rem) = isqrt_u128(sig2 << (2 * k));
    sig_z |= (rem != 0) as u64;
    (
        round_pack(false, exp_z, sig_z as u32, rm, &mut flags),
        flags,
    )
}

/// `FMADD.S`: `a * b + c`, fused with a single rounding.
///
/// The FMSUB/FNMSUB/FNMADD variants are this operation with an exactly
/// negated product or addend; [`fmsub`], [`fnmsub`] and [`fnmadd`] apply the
/// required sign changes.
pub fn fmadd(a: u32, b: u32, c: u32, rm: Rounding) -> (u32, FpFlags) {
    mul_add(a, b, c, rm)
}

/// `FMSUB.S`: `a * b - c`, fused with a single rounding.
pub fn fmsub(a: u32, b: u32, c: u32, rm: Rounding) -> (u32, FpFlags) {
    mul_add(a, b, c ^ SIGN, rm)
}

/// `FNMSUB.S`: `-(a * b) + c`, fused with a single rounding.
pub fn fnmsub(a: u32, b: u32, c: u32, rm: Rounding) -> (u32, FpFlags) {
    mul_add(a ^ SIGN, b, c, rm)
}

/// `FNMADD.S`: `-(a * b) - c`, fused with a single rounding.
pub fn fnmadd(a: u32, b: u32, c: u32, rm: Rounding) -> (u32, FpFlags) {
    mul_add(a ^ SIGN, b, c ^ SIGN, rm)
}

/// `softfloat_mulAddF32` with `op = 0` (the sign changes for the other three
/// instructions are exact bit flips applied by the wrappers above).
fn mul_add(ui_a: u32, ui_b: u32, ui_c: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(ui_a);
    let mut exp_a = exp(ui_a);
    let mut sig_a = frac(ui_a);
    let sign_b = sign(ui_b);
    let mut exp_b = exp(ui_b);
    let mut sig_b = frac(ui_b);
    let sign_c = sign(ui_c);
    let mut exp_c = exp(ui_c);
    let mut sig_c = frac(ui_c);

    let mut sign_z = sign_a ^ sign_b;

    let mag_bits;
    if exp_a == 0xff {
        if sig_a != 0 || (exp_b == 0xff && sig_b != 0) {
            return (nan_result(&mut flags, &[ui_a, ui_b, ui_c]), flags);
        }
        mag_bits = exp_b | sig_b as i32;
        return inf_prod_arg(sign_z, mag_bits, exp_c, sig_c, sign_c, ui_c, &mut flags);
    }
    if exp_b == 0xff {
        if sig_b != 0 {
            return (nan_result(&mut flags, &[ui_a, ui_b, ui_c]), flags);
        }
        mag_bits = exp_a | sig_a as i32;
        return inf_prod_arg(sign_z, mag_bits, exp_c, sig_c, sign_c, ui_c, &mut flags);
    }
    if exp_c == 0xff {
        if sig_c != 0 {
            return (nan_result(&mut flags, &[ui_c]), flags);
        }
        return (ui_c, flags);
    }
    if exp_a == 0 {
        if sig_a == 0 {
            return zero_prod(ui_c, exp_c, sig_c, sign_c, sign_z, rm);
        }
        let (e, s) = norm_subnormal_sig(sig_a);
        exp_a = e;
        sig_a = s;
    }
    if exp_b == 0 {
        if sig_b == 0 {
            return zero_prod(ui_c, exp_c, sig_c, sign_c, sign_z, rm);
        }
        let (e, s) = norm_subnormal_sig(sig_b);
        exp_b = e;
        sig_b = s;
    }

    let mut exp_prod = exp_a + exp_b - 0x7e;
    let sig_a = (sig_a | 0x0080_0000) << 7;
    let sig_b = (sig_b | 0x0080_0000) << 7;
    let mut sig_prod = sig_a as u64 * sig_b as u64;
    if sig_prod < 0x2000_0000_0000_0000 {
        exp_prod -= 1;
        sig_prod <<= 1;
    }

    if exp_c == 0 {
        if sig_c == 0 {
            let sig_z = short_shift_right_jam64(sig_prod, 31) as u32;
            return (
                round_pack(sign_z, exp_prod - 1, sig_z, rm, &mut flags),
                flags,
            );
        }
        let (e, s) = norm_subnormal_sig(sig_c);
        exp_c = e;
        sig_c = s;
    }
    let sig_c = (sig_c | 0x0080_0000) << 6;
    let exp_diff = exp_prod - exp_c;

    if sign_z == sign_c {
        if exp_diff <= 0 {
            let mut s = (sig_c as u64 + shift_right_jam64(sig_prod, (32 - exp_diff) as u32)) as u32;
            let mut e = exp_c;
            if s < 0x4000_0000 {
                e -= 1;
                s <<= 1;
            }
            return (round_pack(sign_z, e, s, rm, &mut flags), flags);
        }
        let sig64_z = sig_prod + shift_right_jam64((sig_c as u64) << 32, exp_diff as u32);
        let mut s = short_shift_right_jam64(sig64_z, 32) as u32;
        let mut e = exp_prod;
        if s < 0x4000_0000 {
            e -= 1;
            s <<= 1;
        }
        return (round_pack(sign_z, e, s, rm, &mut flags), flags);
    }

    let sig64_c = (sig_c as u64) << 32;
    let mut sig64_z;
    let mut exp_z = exp_prod;
    if exp_diff < 0 {
        sign_z = sign_c;
        exp_z = exp_c;
        sig64_z = sig64_c.wrapping_sub(shift_right_jam64(sig_prod, (-exp_diff) as u32));
    } else if exp_diff == 0 {
        sig64_z = sig_prod.wrapping_sub(sig64_c);
        if sig64_z == 0 {
            return complete_cancellation(rm);
        }
        if sig64_z & 0x8000_0000_0000_0000 != 0 {
            sign_z = !sign_z;
            sig64_z = 0u64.wrapping_sub(sig64_z);
        }
    } else {
        sig64_z = sig_prod.wrapping_sub(shift_right_jam64(sig64_c, exp_diff as u32));
    }
    let mut shift_dist = count_leading_zeros64(sig64_z) as i32 - 1;
    exp_z -= shift_dist;
    shift_dist -= 32;
    let sig_z = if shift_dist < 0 {
        short_shift_right_jam64(sig64_z, (-shift_dist) as u32) as u32
    } else {
        (sig64_z as u32) << shift_dist
    };
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

fn inf_prod_arg(
    sign_z: bool,
    mag_bits: i32,
    exp_c: i32,
    sig_c: u32,
    sign_c: bool,
    ui_c: u32,
    flags: &mut FpFlags,
) -> (u32, FpFlags) {
    let mut f = *flags;
    if mag_bits != 0 {
        let inf = pack(sign_z, 0xff, 0);
        if exp_c != 0xff {
            return (inf, f);
        }
        if sig_c != 0 {
            return (nan_result(&mut f, &[ui_c]), f);
        }
        if sign_z == sign_c {
            return (inf, f);
        }
    }
    f |= FpFlags::NV;
    (CANONICAL_NAN, f)
}

fn zero_prod(
    ui_c: u32,
    exp_c: i32,
    sig_c: u32,
    sign_c: bool,
    sign_z: bool,
    rm: Rounding,
) -> (u32, FpFlags) {
    if exp_c | sig_c as i32 == 0 && sign_z != sign_c {
        return complete_cancellation(rm);
    }
    (ui_c, FpFlags::empty())
}

fn complete_cancellation(rm: Rounding) -> (u32, FpFlags) {
    (pack(rm == Rounding::Rdn, 0, 0), FpFlags::empty())
}

/// `FMIN.S` (RISC-V F version 2.2: `-0 < +0`, one NaN returns the other
/// operand, both NaNs return the canonical NaN).
pub fn min(a: u32, b: u32) -> (u32, FpFlags) {
    min_max(a, b, true)
}

/// `FMAX.S` (RISC-V F version 2.2 rules).
pub fn max(a: u32, b: u32) -> (u32, FpFlags) {
    min_max(a, b, false)
}

fn min_max(a: u32, b: u32, is_min: bool) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let (nan_a, nan_b) = (is_nan(a), is_nan(b));
    if nan_a || nan_b {
        if is_signaling_nan(a) || is_signaling_nan(b) {
            flags |= FpFlags::NV;
        }
        if nan_a && nan_b {
            return (CANONICAL_NAN, flags);
        }
        return (if nan_a { b } else { a }, flags);
    }
    let (a_lt_b, b_lt_a) = (lt_bits(a, b), lt_bits(b, a));
    let z = if is_min {
        if a_lt_b {
            Some(a)
        } else if b_lt_a {
            Some(b)
        } else {
            None
        }
    } else if a_lt_b {
        Some(b)
    } else if b_lt_a {
        Some(a)
    } else {
        None
    };
    if let Some(z) = z {
        return (z, flags);
    }
    // Equal values; only the signed zeros are bitwise different.
    let both_neg = sign(a) && sign(b);
    let any_neg = sign(a) || sign(b);
    let negative = if is_min { any_neg } else { both_neg };
    (pack(negative, 0, 0), flags)
}

/// Magnitude comparison for non-NaN operands, treating `-0 == +0`.
fn lt_bits(a: u32, b: u32) -> bool {
    let (a_neg, b_neg) = (sign(a), sign(b));
    let (ma, mb) = (a & !SIGN, b & !SIGN);
    if a_neg == b_neg {
        if a_neg {
            ma > mb
        } else {
            ma < mb
        }
    } else {
        a_neg && !(ma == 0 && mb == 0)
    }
}

/// `FEQ.S`: quiet compare; only signaling NaNs raise `NV`.
pub fn eq(a: u32, b: u32) -> (bool, FpFlags) {
    let mut flags = FpFlags::empty();
    if is_nan(a) || is_nan(b) {
        if is_signaling_nan(a) || is_signaling_nan(b) {
            flags |= FpFlags::NV;
        }
        return (false, flags);
    }
    ((a == b) || (a & !SIGN == 0 && b & !SIGN == 0), flags)
}

/// `FLT.S`: signaling compare; any NaN raises `NV`.
pub fn lt(a: u32, b: u32) -> (bool, FpFlags) {
    let mut flags = FpFlags::empty();
    if is_nan(a) || is_nan(b) {
        flags |= FpFlags::NV;
        return (false, flags);
    }
    (lt_bits(a, b), flags)
}

/// `FLE.S`: signaling compare; any NaN raises `NV`.
pub fn le(a: u32, b: u32) -> (bool, FpFlags) {
    let mut flags = FpFlags::empty();
    if is_nan(a) || is_nan(b) {
        flags |= FpFlags::NV;
        return (false, flags);
    }
    let (a_neg, b_neg) = (sign(a), sign(b));
    let (ma, mb) = (a & !SIGN, b & !SIGN);
    let z = if a_neg == b_neg {
        if a_neg {
            ma >= mb
        } else {
            ma <= mb
        }
    } else {
        a_neg || (ma == 0 && mb == 0)
    };
    (z, flags)
}

/// `FCLASS.S`.
pub fn classify(a: u32) -> u32 {
    let s = sign(a);
    let e = exp(a);
    let f = frac(a);
    if e == 0xff {
        if f == 0 {
            return if s { 1 << 0 } else { 1 << 7 };
        }
        return if is_signaling_nan(a) { 1 << 8 } else { 1 << 9 };
    }
    if e == 0 {
        if f == 0 {
            return if s { 1 << 3 } else { 1 << 4 };
        }
        return if s { 1 << 2 } else { 1 << 5 };
    }
    if s {
        1 << 1
    } else {
        1 << 6
    }
}

/// `FSGNJ.S`.
pub fn fsgnj(a: u32, b: u32) -> u32 {
    (a & !SIGN) | (b & SIGN)
}

/// `FSGNJN.S`.
pub fn fsgnjn(a: u32, b: u32) -> u32 {
    (a & !SIGN) | (!b & SIGN)
}

/// `FSGNJX.S`.
pub fn fsgnjx(a: u32, b: u32) -> u32 {
    (a & !SIGN) | ((a ^ b) & SIGN)
}

/// `FCVT.S.D`.
pub fn from_double(a: u64, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_z = (a >> 63) != 0;
    let exp_a = ((a >> 52) & 0x7ff) as i32;
    let frac_a = a & 0x000f_ffff_ffff_ffff;
    if exp_a == 0x7ff {
        if frac_a != 0 {
            let s = super::double::is_signaling_nan(a);
            if s {
                flags |= FpFlags::NV;
            }
            return (CANONICAL_NAN, flags);
        }
        return (pack(sign_z, 0xff, 0), flags);
    }
    let frac32 = short_shift_right_jam64(frac_a, 22) as u32;
    if exp_a | frac32 as i32 == 0 {
        return (pack(sign_z, 0, 0), flags);
    }
    (
        round_pack(sign_z, exp_a - 0x381, frac32 | 0x4000_0000, rm, &mut flags),
        flags,
    )
}

/// `FCVT.W.S`. The result is sign-extended to 64 bits.
pub fn to_i32(a: u32, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let mut sig = frac(a);
    if exp_a == 0xff && sig != 0 {
        return (IntSat::I32.nan(), FpFlags::NV);
    }
    if exp_a != 0 {
        sig |= 0x0080_0000;
    }
    let mut sig64 = (sig as u64) << 32;
    let shift_dist = 0xaa - exp_a;
    if shift_dist > 0 {
        sig64 = shift_right_jam64(sig64, shift_dist as u32);
    }
    round_to_i32(sign_a, sig64, IntSat::I32, rm)
}

/// `FCVT.WU.S`. The result is sign-extended to 64 bits.
pub fn to_ui32(a: u32, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let mut sig = frac(a);
    if exp_a == 0xff && sig != 0 {
        return (IntSat::Ui32.nan(), FpFlags::NV);
    }
    if exp_a != 0 {
        sig |= 0x0080_0000;
    }
    let mut sig64 = (sig as u64) << 32;
    let shift_dist = 0xaa - exp_a;
    if shift_dist > 0 {
        sig64 = shift_right_jam64(sig64, shift_dist as u32);
    }
    round_to_i32(sign_a, sig64, IntSat::Ui32, rm)
}

/// `FCVT.L.S`.
pub fn to_i64(a: u32, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let mut sig = frac(a);
    let shift_dist = 0xbe - exp_a;
    if shift_dist < 0 {
        let is_nan = exp_a == 0xff && sig != 0;
        return (invalid_int_result(sign_a, is_nan, IntSat::I64), FpFlags::NV);
    }
    if exp_a != 0 {
        sig |= 0x0080_0000;
    }
    let mut sig64 = (sig as u64) << 40;
    let mut extra = 0;
    if shift_dist != 0 {
        let (s, e) = shift_right_jam64_extra(sig64, 0, shift_dist as u32);
        sig64 = s;
        extra = e;
    }
    round_to_i64(sign_a, sig64, extra, IntSat::I64, rm)
}

/// `FCVT.LU.S`.
pub fn to_ui64(a: u32, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let mut sig = frac(a);
    let shift_dist = 0xbe - exp_a;
    if shift_dist < 0 {
        let is_nan = exp_a == 0xff && sig != 0;
        return (
            invalid_int_result(sign_a, is_nan, IntSat::Ui64),
            FpFlags::NV,
        );
    }
    if exp_a != 0 {
        sig |= 0x0080_0000;
    }
    let mut sig64 = (sig as u64) << 40;
    let mut extra = 0;
    if shift_dist != 0 {
        let (s, e) = shift_right_jam64_extra(sig64, 0, shift_dist as u32);
        sig64 = s;
        extra = e;
    }
    round_to_i64(sign_a, sig64, extra, IntSat::Ui64, rm)
}

/// `FCVT.S.W`.
pub fn from_i32(a: i32, rm: Rounding) -> (u32, FpFlags) {
    let sign_z = a < 0;
    if a & 0x7fff_ffff == 0 {
        return (
            if sign_z { pack(true, 0x9e, 0) } else { 0 },
            FpFlags::empty(),
        );
    }
    let abs_a = if sign_z {
        (-(a as i64)) as u32
    } else {
        a as u32
    };
    let mut flags = FpFlags::empty();
    let z = norm_round_pack(sign_z, 0x9c, abs_a, rm, &mut flags);
    (z, flags)
}

/// `FCVT.S.WU`.
pub fn from_ui32(a: u32, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    if a == 0 {
        return (0, flags);
    }
    let z = if a & 0x8000_0000 != 0 {
        round_pack(false, 0x9d, (a >> 1) | (a & 1), rm, &mut flags)
    } else {
        norm_round_pack(false, 0x9c, a, rm, &mut flags)
    };
    (z, flags)
}

/// `FCVT.S.L`.
pub fn from_i64(a: i64, rm: Rounding) -> (u32, FpFlags) {
    let sign_z = a < 0;
    let abs_a = if sign_z {
        (a as u64).wrapping_neg()
    } else {
        a as u64
    };
    let mut flags = FpFlags::empty();
    let shift_dist = count_leading_zeros64(abs_a) as i32 - 40;
    if shift_dist >= 0 {
        if a == 0 {
            return (0, flags);
        }
        return (
            pack(
                sign_z,
                (0x95 - shift_dist) as u32,
                (abs_a as u32) << shift_dist,
            ),
            flags,
        );
    }
    let shift_dist = shift_dist + 7;
    let sig = if shift_dist < 0 {
        short_shift_right_jam64(abs_a, (-shift_dist) as u32) as u32
    } else {
        (abs_a as u32) << shift_dist
    };
    let z = round_pack(sign_z, 0x9c - shift_dist, sig, rm, &mut flags);
    (z, flags)
}

/// `FCVT.S.LU`.
pub fn from_ui64(a: u64, rm: Rounding) -> (u32, FpFlags) {
    let mut flags = FpFlags::empty();
    let shift_dist = count_leading_zeros64(a) as i32 - 40;
    if shift_dist >= 0 {
        if a == 0 {
            return (0, flags);
        }
        return (
            pack(false, (0x95 - shift_dist) as u32, (a as u32) << shift_dist),
            flags,
        );
    }
    let shift_dist = shift_dist + 7;
    let sig = if shift_dist < 0 {
        short_shift_right_jam64(a, (-shift_dist) as u32) as u32
    } else {
        (a as u32) << shift_dist
    };
    let z = round_pack(false, 0x9c - shift_dist, sig, rm, &mut flags);
    (z, flags)
}
