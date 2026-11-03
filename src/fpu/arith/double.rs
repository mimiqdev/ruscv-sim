//! Double-precision (`D` extension) arithmetic.
//!
//! Every function takes and returns raw 64-bit IEEE 754 patterns and an
//! explicit [`Rounding`] mode where rounding applies, and returns the exact
//! exception flags. NaN results of arithmetic, square root, fused
//! multiply-add and conversion are the canonical NaN; sign injection,
//! classification and comparisons preserve or inspect payloads.
//!
//! Portions of this module are a Rust port of Berkeley SoftFloat Release 3e
//! ("This C source file is part of the SoftFloat IEEE Floating-Point
//! Arithmetic Package, Release 3e, by John R. Hauser"; Copyright 2011-2018 The
//! Regents of the University of California). See
//! `tools/fpu/SOFTFLOAT-LICENSE.txt` for the copyright notice, conditions and
//! disclaimer.

use super::common::*;

const SIGN: u64 = 0x8000_0000_0000_0000;
const EXP_MASK: u64 = 0x7ff0_0000_0000_0000;
const FRAC_MASK: u64 = 0x000f_ffff_ffff_ffff;
pub(crate) const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;

#[inline]
fn sign(ui: u64) -> bool {
    ui & SIGN != 0
}

#[inline]
fn exp(ui: u64) -> i32 {
    ((ui & EXP_MASK) >> 52) as i32
}

#[inline]
fn frac(ui: u64) -> u64 {
    ui & FRAC_MASK
}

#[inline]
fn pack(sign: bool, exp: u32, sig: u64) -> u64 {
    ((sign as u64) << 63) + ((exp as u64) << 52) + sig
}

/// True when the bits encode a quiet or signaling NaN.
pub fn is_nan(ui: u64) -> bool {
    ui & EXP_MASK == EXP_MASK && ui & FRAC_MASK != 0
}

/// True when the bits encode a signaling NaN.
pub fn is_signaling_nan(ui: u64) -> bool {
    is_nan(ui) && ui & 0x0008_0000_0000_0000 == 0
}

fn nan_result(flags: &mut FpFlags, operands: &[u64]) -> u64 {
    if operands.iter().any(|&ui| is_signaling_nan(ui)) {
        *flags |= FpFlags::NV;
    }
    CANONICAL_NAN
}

/// `softfloat_normSubnormalF64Sig`: `(exp, sig)` with the significand
/// normalized to bit 52.
fn norm_subnormal_sig(sig: u64) -> (i32, u64) {
    let shift_dist = count_leading_zeros64(sig) as i32 - 11;
    (1 - shift_dist, sig << shift_dist)
}

/// `softfloat_roundPackToF64`, with tininess detected after rounding.
fn round_pack(sign: bool, mut exp: i32, mut sig: u64, rm: Rounding, flags: &mut FpFlags) -> u64 {
    let round_near_even = rm == Rounding::Rne;
    let mut round_increment: u64 = 0x200;
    if !round_near_even && rm != Rounding::Rmm {
        round_increment = if rm == if sign { Rounding::Rdn } else { Rounding::Rup } {
            0x3ff
        } else {
            0
        };
    }
    let mut round_bits = sig & 0x3ff;

    if exp < 0 {
        let is_tiny = exp < -1 || (sig as u128 + round_increment as u128) < 0x8000_0000_0000_0000;
        sig = shift_right_jam64(sig, (-exp) as u32);
        exp = 0;
        round_bits = sig & 0x3ff;
        if is_tiny && round_bits != 0 {
            *flags |= FpFlags::UF;
        }
    } else if exp >= 0x7fd
        && (exp > 0x7fd || sig as u128 + round_increment as u128 >= 0x8000_0000_0000_0000)
    {
        *flags |= FpFlags::OF | FpFlags::NX;
        return pack(sign, 0x7ff, 0) - ((round_increment == 0) as u64);
    }

    sig = (sig + round_increment) >> 10;
    if round_bits != 0 {
        *flags |= FpFlags::NX;
    }
    sig &= !((round_bits ^ 0x200 == 0 && round_near_even) as u64);
    if sig == 0 {
        exp = 0;
    }
    pack(sign, exp as u32, sig)
}

/// `softfloat_normRoundPackToF64`.
fn norm_round_pack(sign: bool, mut exp: i32, sig: u64, rm: Rounding, flags: &mut FpFlags) -> u64 {
    let shift_dist = count_leading_zeros64(sig) as i32 - 1;
    exp -= shift_dist;
    if shift_dist >= 10 && (0..0x7fd).contains(&exp) {
        pack(
            sign,
            if sig != 0 { exp as u32 } else { 0 },
            sig << (shift_dist - 10),
        )
    } else {
        round_pack(sign, exp, sig << shift_dist, rm, flags)
    }
}

/// `softfloat_addMagsF64`: both operands have the same sign.
fn add_mags(ui_a: u64, ui_b: u64, sign_z: bool, rm: Rounding, flags: &mut FpFlags) -> u64 {
    let exp_a = exp(ui_a);
    let sig_a = frac(ui_a);
    let exp_b = exp(ui_b);
    let sig_b = frac(ui_b);
    let exp_diff = exp_a - exp_b;

    if exp_diff == 0 {
        if exp_a == 0 {
            return ui_a.wrapping_add(sig_b);
        }
        if exp_a == 0x7ff {
            if sig_a | sig_b != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        let mut sig_z = 0x0020_0000_0000_0000 + sig_a + sig_b;
        sig_z <<= 9;
        return round_pack(sign_z, exp_a, sig_z, rm, flags);
    }

    let mut sig_a = sig_a << 9;
    let mut sig_b = sig_b << 9;
    let exp_z;
    if exp_diff < 0 {
        if exp_b == 0x7ff {
            if sig_b != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return pack(sign_z, 0x7ff, 0);
        }
        exp_z = exp_b;
        if exp_a != 0 {
            sig_a += 0x2000_0000_0000_0000;
        } else {
            sig_a <<= 1;
        }
        sig_a = shift_right_jam64(sig_a, (-exp_diff) as u32);
    } else {
        if exp_a == 0x7ff {
            if sig_a != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        exp_z = exp_a;
        if exp_b != 0 {
            sig_b += 0x2000_0000_0000_0000;
        } else {
            sig_b <<= 1;
        }
        sig_b = shift_right_jam64(sig_b, exp_diff as u32);
    }
    let mut sig_z = 0x2000_0000_0000_0000 + sig_a + sig_b;
    let mut exp_z = exp_z;
    if sig_z < 0x4000_0000_0000_0000 {
        exp_z -= 1;
        sig_z <<= 1;
    }
    round_pack(sign_z, exp_z, sig_z, rm, flags)
}

/// `softfloat_subMagsF64`: the operands have opposite signs.
fn sub_mags(ui_a: u64, ui_b: u64, mut sign_z: bool, rm: Rounding, flags: &mut FpFlags) -> u64 {
    let exp_a0 = exp(ui_a);
    let sig_a0 = frac(ui_a);
    let exp_b0 = exp(ui_b);
    let sig_b0 = frac(ui_b);
    let exp_diff = exp_a0 - exp_b0;

    if exp_diff == 0 {
        if exp_a0 == 0x7ff {
            if sig_a0 | sig_b0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            *flags |= FpFlags::NV;
            return CANONICAL_NAN;
        }
        let sig_diff = sig_a0 as i64 - sig_b0 as i64;
        if sig_diff == 0 {
            return pack(rm == Rounding::Rdn, 0, 0);
        }
        let exp_a = if exp_a0 != 0 { exp_a0 - 1 } else { 0 };
        let sig_diff = if sig_diff < 0 {
            sign_z = !sign_z;
            (-sig_diff) as u64
        } else {
            sig_diff as u64
        };
        let mut shift_dist = count_leading_zeros64(sig_diff) as i32 - 11;
        let mut exp_z = exp_a - shift_dist;
        if exp_z < 0 {
            shift_dist = exp_a;
            exp_z = 0;
        }
        return pack(sign_z, exp_z as u32, sig_diff << shift_dist);
    }

    let sig_a = sig_a0 << 10;
    let sig_b = sig_b0 << 10;
    let (exp_z, sig_z);
    if exp_diff < 0 {
        sign_z = !sign_z;
        if exp_b0 == 0x7ff {
            if sig_b0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return pack(sign_z, 0x7ff, 0);
        }
        let mut sa = sig_a;
        if exp_a0 != 0 {
            sa += 0x4000_0000_0000_0000;
        } else {
            sa += sa;
        }
        sa = shift_right_jam64(sa, (-exp_diff) as u32);
        let sb = sig_b | 0x4000_0000_0000_0000;
        exp_z = exp_b0;
        sig_z = sb.wrapping_sub(sa);
    } else {
        if exp_a0 == 0x7ff {
            if sig_a0 != 0 {
                return nan_result(flags, &[ui_a, ui_b]);
            }
            return ui_a;
        }
        let mut sb = sig_b;
        if exp_b0 != 0 {
            sb += 0x4000_0000_0000_0000;
        } else {
            sb += sb;
        }
        sb = shift_right_jam64(sb, exp_diff as u32);
        exp_z = exp_a0;
        sig_z = (sig_a | 0x4000_0000_0000_0000).wrapping_sub(sb);
    }
    norm_round_pack(sign_z, exp_z - 1, sig_z, rm, flags)
}

/// `FADD.D`: `a + b`.
pub fn add(a: u64, b: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let z = if sign_a ^ sign(b) {
        sub_mags(a, b, sign_a, rm, &mut flags)
    } else {
        add_mags(a, b, sign_a, rm, &mut flags)
    };
    (z, flags)
}

/// `FSUB.D`: `a - b`.
pub fn sub(a: u64, b: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let z = if sign_a ^ sign(b) {
        add_mags(a, b, sign_a, rm, &mut flags)
    } else {
        sub_mags(a, b, sign_a, rm, &mut flags)
    };
    (z, flags)
}

/// `FMUL.D`: `a * b`.
pub fn mul(a: u64, b: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);
    let sign_b = sign(b);
    let mut exp_b = exp(b);
    let mut sig_b = frac(b);
    let sign_z = sign_a ^ sign_b;

    let mag_bits: i64;
    if exp_a == 0x7ff {
        if sig_a != 0 || (exp_b == 0x7ff && sig_b != 0) {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        mag_bits = exp_b as i64 | sig_b as i64;
        return inf_arg(sign_z, mag_bits, &mut flags);
    }
    if exp_b == 0x7ff {
        if sig_b != 0 {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        mag_bits = exp_a as i64 | sig_a as i64;
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
    let mut exp_z = exp_a + exp_b - 0x3ff;
    let sig_a = (sig_a | 0x0010_0000_0000_0000) << 10;
    let sig_b = (sig_b | 0x0010_0000_0000_0000) << 11;
    let (hi, lo) = mul64_to128(sig_a, sig_b);
    let mut sig_z = hi | (lo != 0) as u64;
    if sig_z < 0x4000_0000_0000_0000 {
        exp_z -= 1;
        sig_z <<= 1;
    }
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

fn inf_arg(sign_z: bool, mag_bits: i64, flags: &mut FpFlags) -> (u64, FpFlags) {
    let mut f = *flags;
    let z = if mag_bits == 0 {
        f |= FpFlags::NV;
        CANONICAL_NAN
    } else {
        pack(sign_z, 0x7ff, 0)
    };
    (z, f)
}

/// `FDIV.D`: `a / b`.
pub fn div(a: u64, b: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);
    let sign_b = sign(b);
    let mut exp_b = exp(b);
    let mut sig_b = frac(b);
    let sign_z = sign_a ^ sign_b;

    if exp_a == 0x7ff {
        if sig_a != 0 {
            return (nan_result(&mut flags, &[a, b]), flags);
        }
        if exp_b == 0x7ff {
            if sig_b != 0 {
                return (nan_result(&mut flags, &[a, b]), flags);
            }
            flags |= FpFlags::NV;
            return (CANONICAL_NAN, flags);
        }
        return (pack(sign_z, 0x7ff, 0), flags);
    }
    if exp_b == 0x7ff {
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
            return (pack(sign_z, 0x7ff, 0), flags);
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
    let mut exp_z = exp_a - exp_b + 0x3fe;
    let mut sig_a = (sig_a | 0x0010_0000_0000_0000) as u128;
    let sig_b = (sig_b | 0x0010_0000_0000_0000) as u128;
    if sig_a < sig_b {
        sig_a <<= 1;
        exp_z -= 1;
    }
    let num = sig_a << 62;
    let sig_z = (num / sig_b) as u64 | (!num.is_multiple_of(sig_b)) as u64;
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

/// `FSQRT.D`.
pub fn sqrt(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_a = sign(a);
    let mut exp_a = exp(a);
    let mut sig_a = frac(a);

    if exp_a == 0x7ff {
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

    // value = sig * 2^(exp_a - 1023 - 52); see `single::sqrt` for the shared
    // derivation of the round_pack scale.
    let e = exp_a - 0x3ff - 52;
    let parity = (e & 1) as u32;
    let sig2 = ((sig_a | 0x0010_0000_0000_0000) as u128) << parity;
    let len = 128 - sig2.leading_zeros();
    let k = 62 - (len - 1) / 2;
    let half_exp = (e - parity as i32) / 2;
    let exp_z = 1084 + half_exp - k as i32;
    let (mut sig_z, rem) = isqrt_u128(sig2 << (2 * k));
    sig_z |= (rem != 0) as u64;
    (round_pack(false, exp_z, sig_z, rm, &mut flags), flags)
}

/// `FMADD.D`: `a * b + c`, fused with a single rounding.
pub fn fmadd(a: u64, b: u64, c: u64, rm: Rounding) -> (u64, FpFlags) {
    mul_add(a, b, c, rm)
}

/// `FMSUB.D`: `a * b - c`, fused with a single rounding.
pub fn fmsub(a: u64, b: u64, c: u64, rm: Rounding) -> (u64, FpFlags) {
    mul_add(a, b, c ^ SIGN, rm)
}

/// `FNMSUB.D`: `-(a * b) + c`, fused with a single rounding.
pub fn fnmsub(a: u64, b: u64, c: u64, rm: Rounding) -> (u64, FpFlags) {
    mul_add(a ^ SIGN, b, c, rm)
}

/// `FNMADD.D`: `-(a * b) - c`, fused with a single rounding.
pub fn fnmadd(a: u64, b: u64, c: u64, rm: Rounding) -> (u64, FpFlags) {
    mul_add(a ^ SIGN, b, c ^ SIGN, rm)
}

/// `softfloat_mulAddF64` with `op = 0`.
fn mul_add(ui_a: u64, ui_b: u64, ui_c: u64, rm: Rounding) -> (u64, FpFlags) {
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

    let mag_bits: i64;
    if exp_a == 0x7ff {
        if sig_a != 0 || (exp_b == 0x7ff && sig_b != 0) {
            return (nan_result(&mut flags, &[ui_a, ui_b, ui_c]), flags);
        }
        mag_bits = exp_b as i64 | sig_b as i64;
        return inf_prod_arg(sign_z, mag_bits, exp_c, sig_c, sign_c, ui_c, &mut flags);
    }
    if exp_b == 0x7ff {
        if sig_b != 0 {
            return (nan_result(&mut flags, &[ui_a, ui_b, ui_c]), flags);
        }
        mag_bits = exp_a as i64 | sig_a as i64;
        return inf_prod_arg(sign_z, mag_bits, exp_c, sig_c, sign_c, ui_c, &mut flags);
    }
    if exp_c == 0x7ff {
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

    let mut exp_z = exp_a + exp_b - 0x3fe;
    let sig_a = (sig_a | 0x0010_0000_0000_0000) << 10;
    let sig_b = (sig_b | 0x0010_0000_0000_0000) << 10;
    let (mut hi, mut lo) = mul64_to128(sig_a, sig_b);
    if hi < 0x2000_0000_0000_0000 {
        exp_z -= 1;
        let (h, l) = add128(hi, lo, hi, lo);
        hi = h;
        lo = l;
    }

    if exp_c == 0 {
        if sig_c == 0 {
            exp_z -= 1;
            let sig_z = (hi << 1) | (lo != 0) as u64;
            return (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags);
        }
        let (e, s) = norm_subnormal_sig(sig_c);
        exp_c = e;
        sig_c = s;
    }
    let sig_c = (sig_c | 0x0010_0000_0000_0000) << 9;
    let exp_diff = exp_z - exp_c;
    let (mut hi_c, mut lo_c) = (0u64, 0u64);

    if exp_diff < 0 {
        exp_z = exp_c;
        if sign_z == sign_c || exp_diff < -1 {
            hi = shift_right_jam64(hi, (-exp_diff) as u32);
        } else {
            let (h, l) = short_shift_right_jam128(hi, lo, 1);
            hi = h;
            lo = l;
        }
    } else if exp_diff != 0 {
        let (h, l) = shift_right_jam128(sig_c, 0, exp_diff as u32);
        hi_c = h;
        lo_c = l;
    }

    if sign_z == sign_c {
        let mut sig_z;
        if exp_diff <= 0 {
            sig_z = sig_c.wrapping_add(hi) | (lo != 0) as u64;
        } else {
            let (h, l) = add128(hi, lo, hi_c, lo_c);
            hi = h;
            lo = l;
            sig_z = hi | (lo != 0) as u64;
        }
        if sig_z < 0x4000_0000_0000_0000 {
            exp_z -= 1;
            sig_z <<= 1;
        }
        return (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags);
    }

    let mut sig_z;
    if exp_diff < 0 {
        sign_z = sign_c;
        let (h, l) = sub128(sig_c, 0, hi, lo);
        hi = h;
        lo = l;
    } else if exp_diff == 0 {
        hi = hi.wrapping_sub(sig_c);
        if hi | lo == 0 {
            return complete_cancellation(rm);
        }
        if hi & 0x8000_0000_0000_0000 != 0 {
            sign_z = !sign_z;
            let (h, l) = sub128(0, 0, hi, lo);
            hi = h;
            lo = l;
        }
    } else {
        let (h, l) = sub128(hi, lo, hi_c, lo_c);
        hi = h;
        lo = l;
    }
    if hi == 0 {
        exp_z -= 64;
        hi = lo;
        lo = 0;
    }
    let shift_dist = count_leading_zeros64(hi) as i32 - 1;
    exp_z -= shift_dist;
    if shift_dist < 0 {
        sig_z = short_shift_right_jam64(hi, (-shift_dist) as u32);
    } else {
        let (h, l) = short_shift_left128(hi, lo, shift_dist as u32);
        hi = h;
        lo = l;
        sig_z = hi;
    }
    sig_z |= (lo != 0) as u64;
    (round_pack(sign_z, exp_z, sig_z, rm, &mut flags), flags)
}

fn inf_prod_arg(
    sign_z: bool,
    mag_bits: i64,
    exp_c: i32,
    sig_c: u64,
    sign_c: bool,
    ui_c: u64,
    flags: &mut FpFlags,
) -> (u64, FpFlags) {
    let mut f = *flags;
    if mag_bits != 0 {
        let inf = pack(sign_z, 0x7ff, 0);
        if exp_c != 0x7ff {
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
    ui_c: u64,
    exp_c: i32,
    sig_c: u64,
    sign_c: bool,
    sign_z: bool,
    rm: Rounding,
) -> (u64, FpFlags) {
    if exp_c == 0 && sig_c == 0 && sign_z != sign_c {
        return complete_cancellation(rm);
    }
    (ui_c, FpFlags::empty())
}

fn complete_cancellation(rm: Rounding) -> (u64, FpFlags) {
    (pack(rm == Rounding::Rdn, 0, 0), FpFlags::empty())
}

/// `FMIN.D` (RISC-V F version 2.2 rules).
pub fn min(a: u64, b: u64) -> (u64, FpFlags) {
    min_max(a, b, true)
}

/// `FMAX.D` (RISC-V F version 2.2 rules).
pub fn max(a: u64, b: u64) -> (u64, FpFlags) {
    min_max(a, b, false)
}

fn min_max(a: u64, b: u64, is_min: bool) -> (u64, FpFlags) {
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
    if a & !SIGN != 0 {
        // Equal non-zero operands, including equal infinities: either operand
        // is the result. Only the zero pair below is special.
        return (a, flags);
    }
    // Equal zeros; only the sign is bitwise different.
    let both_neg = sign(a) && sign(b);
    let any_neg = sign(a) || sign(b);
    let negative = if is_min { any_neg } else { both_neg };
    (pack(negative, 0, 0), flags)
}

fn lt_bits(a: u64, b: u64) -> bool {
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

/// `FEQ.D`: quiet compare; only signaling NaNs raise `NV`.
pub fn eq(a: u64, b: u64) -> (bool, FpFlags) {
    let mut flags = FpFlags::empty();
    if is_nan(a) || is_nan(b) {
        if is_signaling_nan(a) || is_signaling_nan(b) {
            flags |= FpFlags::NV;
        }
        return (false, flags);
    }
    ((a == b) || (a & !SIGN == 0 && b & !SIGN == 0), flags)
}

/// `FLT.D`: signaling compare; any NaN raises `NV`.
pub fn lt(a: u64, b: u64) -> (bool, FpFlags) {
    let mut flags = FpFlags::empty();
    if is_nan(a) || is_nan(b) {
        flags |= FpFlags::NV;
        return (false, flags);
    }
    (lt_bits(a, b), flags)
}

/// `FLE.D`: signaling compare; any NaN raises `NV`.
pub fn le(a: u64, b: u64) -> (bool, FpFlags) {
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

/// `FCLASS.D`.
pub fn classify(a: u64) -> u64 {
    let s = sign(a);
    let e = exp(a);
    let f = frac(a);
    if e == 0x7ff {
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

/// `FSGNJ.D`.
pub fn fsgnj(a: u64, b: u64) -> u64 {
    (a & !SIGN) | (b & SIGN)
}

/// `FSGNJN.D`.
pub fn fsgnjn(a: u64, b: u64) -> u64 {
    (a & !SIGN) | (!b & SIGN)
}

/// `FSGNJX.D`.
pub fn fsgnjx(a: u64, b: u64) -> u64 {
    (a & !SIGN) | ((a ^ b) & SIGN)
}

/// `FCVT.D.S`: convert single-precision bits to double.
pub fn from_single(a: u32) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_z = a >> 31 != 0;
    let mut exp_a = ((a >> 23) & 0xff) as i32;
    let mut frac_a = a & 0x007f_ffff;
    if exp_a == 0xff {
        if frac_a != 0 {
            if a & 0x0040_0000 == 0 {
                flags |= FpFlags::NV;
            }
            return (CANONICAL_NAN, flags);
        }
        return (pack(sign_z, 0x7ff, 0), flags);
    }
    if exp_a == 0 {
        if frac_a == 0 {
            return (pack(sign_z, 0, 0), flags);
        }
        let shift_dist = frac_a.leading_zeros() as i32 - 8;
        exp_a = (1 - shift_dist) - 1;
        frac_a <<= shift_dist;
    }
    (
        pack(sign_z, (exp_a + 0x380) as u32, (frac_a as u64) << 29),
        flags,
    )
}

/// `FCVT.W.D`. The result is sign-extended to 64 bits.
pub fn to_i32(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let sig_a = frac(a);
    if exp_a == 0x7ff && sig_a != 0 {
        return (IntSat::I32.nan(), FpFlags::NV);
    }
    let mut sig = if exp_a != 0 {
        sig_a | 0x0010_0000_0000_0000
    } else {
        sig_a
    };
    let shift_dist = 0x427 - exp_a;
    if shift_dist > 0 {
        sig = shift_right_jam64(sig, shift_dist as u32);
    }
    round_to_i32(sign_a, sig, IntSat::I32, rm)
}

/// `FCVT.WU.D`. The result is sign-extended to 64 bits.
pub fn to_ui32(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let sig_a = frac(a);
    if exp_a == 0x7ff && sig_a != 0 {
        return (IntSat::Ui32.nan(), FpFlags::NV);
    }
    let mut sig = if exp_a != 0 {
        sig_a | 0x0010_0000_0000_0000
    } else {
        sig_a
    };
    let shift_dist = 0x427 - exp_a;
    if shift_dist > 0 {
        sig = shift_right_jam64(sig, shift_dist as u32);
    }
    round_to_i32(sign_a, sig, IntSat::Ui32, rm)
}

/// `FCVT.L.D`.
pub fn to_i64(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let sig_a = frac(a);
    let sig = if exp_a != 0 {
        sig_a | 0x0010_0000_0000_0000
    } else {
        sig_a
    };
    let shift_dist = 0x433 - exp_a;
    let (sig, extra) = if shift_dist <= 0 {
        if shift_dist < -11 {
            let is_nan = exp_a == 0x7ff && sig_a != 0;
            return (invalid_int_result(sign_a, is_nan, IntSat::I64), FpFlags::NV);
        }
        (sig << (-shift_dist) as u32, 0)
    } else {
        shift_right_jam64_extra(sig, 0, shift_dist as u32)
    };
    round_to_i64(sign_a, sig, extra, IntSat::I64, rm)
}

/// `FCVT.LU.D`.
pub fn to_ui64(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let sign_a = sign(a);
    let exp_a = exp(a);
    let sig_a = frac(a);
    let sig = if exp_a != 0 {
        sig_a | 0x0010_0000_0000_0000
    } else {
        sig_a
    };
    let shift_dist = 0x433 - exp_a;
    let (sig, extra) = if shift_dist <= 0 {
        if shift_dist < -11 {
            let is_nan = exp_a == 0x7ff && sig_a != 0;
            return (
                invalid_int_result(sign_a, is_nan, IntSat::Ui64),
                FpFlags::NV,
            );
        }
        (sig << (-shift_dist) as u32, 0)
    } else {
        shift_right_jam64_extra(sig, 0, shift_dist as u32)
    };
    round_to_i64(sign_a, sig, extra, IntSat::Ui64, rm)
}

/// `FCVT.D.W`.
pub fn from_i32(a: i32, _rm: Rounding) -> (u64, FpFlags) {
    let flags = FpFlags::empty();
    if a == 0 {
        return (0, flags);
    }
    let sign_z = a < 0;
    let abs_a = if sign_z {
        (-(a as i64)) as u32
    } else {
        a as u32
    };
    let shift_dist = count_leading_zeros32(abs_a) as i32 + 21;
    (
        pack(
            sign_z,
            (0x432 - shift_dist) as u32,
            (abs_a as u64) << shift_dist,
        ),
        flags,
    )
}

/// `FCVT.D.WU`.
pub fn from_ui32(a: u32, _rm: Rounding) -> (u64, FpFlags) {
    let flags = FpFlags::empty();
    if a == 0 {
        return (0, flags);
    }
    let shift_dist = count_leading_zeros32(a) as i32 + 21;
    (
        pack(false, (0x432 - shift_dist) as u32, (a as u64) << shift_dist),
        flags,
    )
}

/// `FCVT.D.L`.
pub fn from_i64(a: i64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    let sign_z = a < 0;
    if a & 0x7fff_ffff_ffff_ffff == 0 {
        return (if sign_z { pack(true, 0x43e, 0) } else { 0 }, flags);
    }
    let abs_a = if sign_z {
        (a as u64).wrapping_neg()
    } else {
        a as u64
    };
    let z = norm_round_pack(sign_z, 0x43c, abs_a, rm, &mut flags);
    (z, flags)
}

/// `FCVT.D.LU`.
pub fn from_ui64(a: u64, rm: Rounding) -> (u64, FpFlags) {
    let mut flags = FpFlags::empty();
    if a == 0 {
        return (0, flags);
    }
    let z = if a & 0x8000_0000_0000_0000 != 0 {
        round_pack(false, 0x43d, short_shift_right_jam64(a, 1), rm, &mut flags)
    } else {
        norm_round_pack(false, 0x43c, a, rm, &mut flags)
    };
    (z, flags)
}
