//! Focused unit tests for the F/D arithmetic core.
//!
//! The bulk of the arithmetic verification is the pinned reference-vector
//! test in `tests/fpu_arith.rs`. These tests pin the RISC-V-specific rules
//! that a generic IEEE reference does not decide on its own (version 2.2
//! `FMIN`/`FMAX`, NaN canonicalization, the `FCVT` saturation table,
//! `FCLASS`, sign injection), plus the contract's negative controls:
//! results must be rounded to the format (not to an integer), the explicit
//! rounding mode must be honoured, `RMM` must differ from `RNE` on ties,
//! fused multiply-add must round once, `FEQ` must not raise `NV` for a quiet
//! NaN, `FCVT.WU.S` of a negative value must saturate to zero, and reserved
//! rounding encodings must be rejected.

use super::common::isqrt_u128;
use super::{double, single, FpFlags, Rounding};

const ZERO: FpFlags = FpFlags::empty();
const NV: FpFlags = FpFlags::NV;
const NX: FpFlags = FpFlags::NX;

const QNAN32: u32 = 0x7fc0_0000;
const QNAN64: u64 = 0x7ff8_0000_0000_0000;
const SNAN32: u32 = 0x7f80_0001;
const SNAN64: u64 = 0x7ff0_0000_0000_0001;

#[test]
fn rounding_encodings_and_flag_layouts() {
    assert_eq!(Rounding::from_encoding(0), Some(Rounding::Rne));
    assert_eq!(Rounding::from_encoding(1), Some(Rounding::Rtz));
    assert_eq!(Rounding::from_encoding(2), Some(Rounding::Rdn));
    assert_eq!(Rounding::from_encoding(3), Some(Rounding::Rup));
    assert_eq!(Rounding::from_encoding(4), Some(Rounding::Rmm));
    // Reserved resolved mode values 5-7. A static `rm` of 7 selects the
    // dynamic mode and must be resolved through `frm` before this call.
    for reserved in 5..=7 {
        assert_eq!(Rounding::from_encoding(reserved), None);
    }
    // The flag layout is the architectural `fflags` layout: bit 0 NX through
    // bit 4 NV (F: "Floating-point control and status register").
    assert_eq!(FpFlags::NX.bits(), 1 << 0);
    assert_eq!(FpFlags::UF.bits(), 1 << 1);
    assert_eq!(FpFlags::OF.bits(), 1 << 2);
    assert_eq!(FpFlags::DZ.bits(), 1 << 3);
    assert_eq!(FpFlags::NV.bits(), 1 << 4);
}

#[test]
fn integer_square_root_is_exact() {
    for x in [
        0u128,
        1,
        2,
        3,
        4,
        15,
        16,
        17,
        (1 << 53) - 1,
        1 << 100,
        (1 << 100) + 12345,
        (1 << 126) - 1,
        u128::MAX,
    ] {
        let (r, rem) = isqrt_u128(x);
        let r = r as u128;
        assert!(r * r <= x, "sqrt too large for {x}");
        if r < u64::MAX as u128 {
            assert!((r + 1) * (r + 1) > x, "sqrt too small for {x}");
        }
        assert_eq!(rem, x - r * r);
    }
}

#[test]
fn fmin_fmax_use_version_2_2_rules() {
    // FMIN/FMAX: -0 < +0, one NaN returns the other operand, both NaNs return
    // the canonical NaN, signaling NaN raises NV even when the result is the
    // non-NaN operand.
    let pos0 = 0x0000_0000u32;
    let neg0 = 0x8000_0000u32;
    let one = 0x3f80_0000u32;
    let neg_one = 0xbf80_0000u32;
    let two = 0x4000_0000u32;
    let neg_two = 0xc000_0000u32;
    let inf = 0x7f80_0000u32;
    let neg_inf = 0xff80_0000u32;
    let qnan_payload = 0x7fc0_0001u32;

    assert_eq!(single::min(pos0, neg0), (neg0, ZERO));
    assert_eq!(single::min(neg0, pos0), (neg0, ZERO));
    assert_eq!(single::max(pos0, neg0), (pos0, ZERO));
    assert_eq!(single::max(neg0, pos0), (pos0, ZERO));
    assert_eq!(single::min(neg0, neg0), (neg0, ZERO));
    assert_eq!(single::max(pos0, pos0), (pos0, ZERO));

    assert_eq!(single::min(one, two), (one, ZERO));
    assert_eq!(single::max(one, two), (two, ZERO));
    assert_eq!(single::min(neg_one, neg_two), (neg_two, ZERO));
    assert_eq!(single::max(neg_one, neg_two), (neg_one, ZERO));
    assert_eq!(single::min(neg_inf, neg_one), (neg_inf, ZERO));
    assert_eq!(single::max(inf, one), (inf, ZERO));

    assert_eq!(single::min(qnan_payload, one), (one, ZERO));
    assert_eq!(single::min(one, qnan_payload), (one, ZERO));
    assert_eq!(single::max(qnan_payload, one), (one, ZERO));
    assert_eq!(single::min(SNAN32, one), (one, NV));
    assert_eq!(single::max(one, SNAN32), (one, NV));
    assert_eq!(single::min(qnan_payload, SNAN32), (QNAN32, NV));
    assert_eq!(single::max(SNAN32, qnan_payload), (QNAN32, NV));
    assert_eq!(single::min(qnan_payload, qnan_payload), (QNAN32, ZERO));
    assert_eq!(single::max(qnan_payload, qnan_payload), (QNAN32, ZERO));

    let dpos0 = 0x0000_0000_0000_0000u64;
    let dneg0 = 0x8000_0000_0000_0000u64;
    let done = 0x3ff0_0000_0000_0000u64;
    let dneg_one = 0xbff0_0000_0000_0000u64;
    let dqnan_payload = 0x7ff8_0000_0000_0001u64;
    let dsnan = 0x7ff0_0000_0000_0001u64;
    assert_eq!(double::min(dpos0, dneg0), (dneg0, ZERO));
    assert_eq!(double::max(dpos0, dneg0), (dpos0, ZERO));
    assert_eq!(double::min(done, dneg_one), (dneg_one, ZERO));
    assert_eq!(double::max(done, dneg_one), (done, ZERO));
    assert_eq!(double::min(dqnan_payload, done), (done, ZERO));
    assert_eq!(double::max(dsnan, done), (done, NV));
    assert_eq!(double::min(dqnan_payload, dsnan), (QNAN64, NV));
    assert_eq!(double::max(dqnan_payload, dqnan_payload), (QNAN64, ZERO));
}

#[test]
fn min_max_equal_operands_return_the_operand() {
    // Equal non-zero values (not just zeros) must be returned unchanged.
    let one = 0x3f80_0000u32;
    let neg_one = 0xbf80_0000u32;
    let inf = 0x7f80_0000u32;
    let neg_inf = 0xff80_0000u32;
    let min_sub = 0x0000_0001u32;
    for value in [one, neg_one, inf, neg_inf, min_sub, 0x0080_0000] {
        assert_eq!(single::min(value, value), (value, ZERO));
        assert_eq!(single::max(value, value), (value, ZERO));
    }
    let done = 0x3ff0_0000_0000_0000u64;
    let dneg_one = 0xbff0_0000_0000_0000u64;
    let dinf = 0x7ff0_0000_0000_0000u64;
    let dmin_sub = 0x0000_0000_0000_0001u64;
    for value in [done, dneg_one, dinf, dmin_sub, 0x0010_0000_0000_0000] {
        assert_eq!(double::min(value, value), (value, ZERO));
        assert_eq!(double::max(value, value), (value, ZERO));
    }
}

#[test]
fn fclass_bits() {
    // FCLASS bits 0..9 in the manual's order, including bits 8 (signaling
    // NaN) and 9 (quiet NaN).
    let classes32: [(u32, u32); 10] = [
        (0xff80_0000, 1 << 0), // -infinity
        (0xbf80_0000, 1 << 1), // negative normal
        (0x807f_ffff, 1 << 2), // negative subnormal
        (0x8000_0000, 1 << 3), // negative zero
        (0x0000_0000, 1 << 4), // positive zero
        (0x007f_ffff, 1 << 5), // positive subnormal
        (0x3f80_0000, 1 << 6), // positive normal
        (0x7f80_0000, 1 << 7), // positive infinity
        (SNAN32, 1 << 8),      // signaling NaN
        (QNAN32, 1 << 9),      // quiet NaN
    ];
    for (bits, want) in classes32 {
        assert_eq!(single::classify(bits), want, "fclass.s of {bits:#010x}");
    }
    let classes64: [(u64, u64); 10] = [
        (0xfff0_0000_0000_0000, 1 << 0),
        (0xbff0_0000_0000_0000, 1 << 1),
        (0x800f_ffff_ffff_ffff, 1 << 2),
        (0x8000_0000_0000_0000, 1 << 3),
        (0x0000_0000_0000_0000, 1 << 4),
        (0x000f_ffff_ffff_ffff, 1 << 5),
        (0x3ff0_0000_0000_0000, 1 << 6),
        (0x7ff0_0000_0000_0000, 1 << 7),
        (SNAN64, 1 << 8),
        (0x7ff8_0000_0000_0001, 1 << 9),
    ];
    for (bits, want) in classes64 {
        assert_eq!(double::classify(bits), want, "fclass.d of {bits:#018x}");
    }
}

#[test]
fn sign_injection_preserves_nan_payloads() {
    // FSGNJ/FSGNJN/FSGNJX are not arithmetic: they take the payload from rs1
    // and only rewrite the sign bit, and never raise flags.
    let payload = 0x7fc0_0001u32;
    let negative = 0x8000_0000u32;
    assert_eq!(single::fsgnj(payload, negative), 0xffc0_0001);
    assert_eq!(single::fsgnjn(payload, negative), payload);
    assert_eq!(single::fsgnjx(payload, negative), 0xffc0_0001);
    assert_eq!(single::fsgnj(SNAN32, 0), SNAN32);
    assert_eq!(single::fsgnj(0x3f80_0000, negative), 0xbf80_0000);
    assert_eq!(single::fsgnjx(0x8000_0000, 0x8000_0000), 0x0000_0000);

    let dpayload = 0x7ff8_0000_0000_0001u64;
    assert_eq!(
        double::fsgnj(dpayload, 0x8000_0000_0000_0000),
        0xfff8_0000_0000_0001
    );
    assert_eq!(double::fsgnj(dpayload, 0), dpayload);
    assert_eq!(double::fsgnjn(dpayload, 0), 0xfff8_0000_0000_0001);
}

#[test]
fn arithmetic_rounds_to_the_format_not_to_an_integer() {
    // Negative control: rounding a result to an integer instead of to the
    // destination format. 1.5 + 0.25 must be 1.75, not 2.0.
    assert_eq!(
        single::add(0x3fc0_0000, 0x3e80_0000, Rounding::Rne),
        (0x3fe0_0000, ZERO)
    );
    assert_eq!(
        double::add(0x3ff8_0000_0000_0000, 0x3fd0_0000_0000_0000, Rounding::Rne).0,
        0x3ffc_0000_0000_0000
    );
}

#[test]
fn explicit_rounding_mode_is_honoured() {
    // Negative controls: ignore the static `rm` and treat RMM as RNE.
    // 1.0 + 2^-24 is an exact tie between 1.0 and 1.0 + 2^-23.
    let one = 0x3f80_0000u32;
    let half_ulp = 0x3380_0000u32; // 2^-24
    let up = 0x3f80_0001u32;
    assert_eq!(single::add(one, half_ulp, Rounding::Rne), (one, NX));
    assert_eq!(single::add(one, half_ulp, Rounding::Rtz), (one, NX));
    assert_eq!(single::add(one, half_ulp, Rounding::Rdn), (one, NX));
    assert_eq!(single::add(one, half_ulp, Rounding::Rup), (up, NX));
    assert_eq!(single::add(one, half_ulp, Rounding::Rmm), (up, NX));
    // RMM and RNE differ exactly on a tie.
    assert_ne!(
        single::add(one, half_ulp, Rounding::Rmm),
        single::add(one, half_ulp, Rounding::Rne)
    );

    // Same rule for the double format.
    let done = 0x3ff0_0000_0000_0000u64;
    let dhalf_ulp = 0x3ca0_0000_0000_0000u64; // 2^-53
    assert_eq!(double::add(done, dhalf_ulp, Rounding::Rne).0, done);
    assert_eq!(double::add(done, dhalf_ulp, Rounding::Rup).0, done + 1);
    assert_eq!(double::add(done, dhalf_ulp, Rounding::Rmm).0, done + 1);
}

#[test]
fn fused_multiply_add_rounds_once() {
    // Negative control: an unfused FMA rounds the product and then the sum.
    // fused (1+2^-23) * (1-2^-24) - 1 = 2^-24 - 2^-47 (exact, no flags);
    // unfused rounds the product to 1+2^-24 (NX) and then subtracts 1.
    let a = 0x3f80_0001u32;
    let b = 0x3f7f_ffffu32;
    let minus_one = 0xbf80_0000u32;
    let fused = single::fmadd(a, b, minus_one, Rounding::Rne);
    assert_eq!(fused, (0x337f_fffe, ZERO));
    let (product, product_flags) = single::mul(a, b, Rounding::Rne);
    assert_eq!(product_flags, NX);
    let (unfused, _) = single::sub(product, 0x3f80_0000, Rounding::Rne);
    assert_eq!(unfused, 0x0000_0000);
    assert_ne!(fused.0, unfused);

    // 0 * infinity + qNaN must still raise NV (F/D 2.2, FMA paragraph).
    assert_eq!(
        single::fmadd(0x7f80_0000, 0x0000_0000, QNAN32, Rounding::Rne),
        (QNAN32, NV)
    );
    assert_eq!(
        double::fmadd(
            0x7ff0_0000_0000_0000,
            0x0000_0000_0000_0000,
            QNAN64,
            Rounding::Rne
        ),
        (QNAN64, NV)
    );
}

#[test]
fn fma_keeps_a_subnormal_addend_when_the_product_is_zero() {
    // Regression: the addend-is-zero test must inspect the full fraction; a
    // truncated 32-bit test misreads subnormals whose low word is zero.
    let addend = 0x8000_0001_0000_0000u64;
    let one = 0x3ff0_0000_0000_0000u64;
    for rm in [
        Rounding::Rne,
        Rounding::Rtz,
        Rounding::Rdn,
        Rounding::Rup,
        Rounding::Rmm,
    ] {
        assert_eq!(double::fmadd(0, one, addend, rm), (addend, ZERO));
        assert_eq!(
            double::fmsub(0, one, addend ^ 0x8000_0000_0000_0000, rm),
            (addend, ZERO)
        );
    }
    let addend32 = 0x0000_0001u32;
    for rm in [Rounding::Rne, Rounding::Rdn, Rounding::Rmm] {
        assert_eq!(
            single::fmadd(0, 0x3f80_0000, addend32, rm),
            (addend32, ZERO)
        );
    }
}

#[test]
fn architectural_flag_bits_are_reported_in_behavior() {
    // The reviewer-facing table: the flag set is returned in architectural
    // `fflags` bit positions (NX=1, UF=2, OF=4, DZ=8, NV=16).
    let one = 0x3f80_0000u32;
    assert_eq!(single::div(one, 0x0000_0000, Rounding::Rne).1.bits(), 0x08);
    assert_eq!(
        single::div(0x0000_0000, 0x0000_0000, Rounding::Rne)
            .1
            .bits(),
        0x10
    );
    assert_eq!(single::add(one, 0x3380_0000, Rounding::Rne).1.bits(), 0x01);
    assert_eq!(
        single::mul(0x7f7f_ffff, 0x4000_0000, Rounding::Rne)
            .1
            .bits(),
        0x05
    );
    assert_eq!(
        single::mul(0x0080_0000, 0x3f7f_ffff, Rounding::Rne)
            .1
            .bits(),
        0x03
    );
    assert_eq!(single::add(0x7fc0_0001, one, Rounding::Rne).1.bits(), 0x00);
    assert_eq!(single::add(SNAN32, one, Rounding::Rne).1.bits(), 0x10);
}

#[test]
fn arithmetic_nan_results_are_canonical() {
    let payload = 0x7fc0_0001u32;
    assert_eq!(
        single::add(payload, 0x3f80_0000, Rounding::Rne),
        (QNAN32, ZERO)
    );
    assert_eq!(
        single::add(SNAN32, 0x3f80_0000, Rounding::Rne),
        (QNAN32, NV)
    );
    assert_eq!(single::sqrt(SNAN32, Rounding::Rne), (QNAN32, NV));
    assert_eq!(single::sqrt(payload, Rounding::Rne), (QNAN32, ZERO));
    assert_eq!(single::sqrt(0xff80_0000, Rounding::Rne), (QNAN32, NV));
    assert_eq!(single::sqrt(0xbf80_0000, Rounding::Rne), (QNAN32, NV));
    assert_eq!(single::sqrt(0xbf80_0000, Rounding::Rtz), (QNAN32, NV));
    assert_eq!(
        single::sqrt(0x8000_0000, Rounding::Rne),
        (0x8000_0000, ZERO)
    );
    assert_eq!(
        single::mul(0x7f80_0000, 0x0000_0000, Rounding::Rne),
        (QNAN32, NV)
    );
    assert_eq!(
        single::div(0x0000_0000, 0x0000_0000, Rounding::Rne),
        (QNAN32, NV)
    );
    assert_eq!(
        single::div(0x7f80_0000, 0x7f80_0000, Rounding::Rne),
        (QNAN32, NV)
    );
    assert_eq!(
        double::sub(0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000, Rounding::Rne),
        (QNAN64, NV)
    );
    assert_eq!(double::sqrt(SNAN64, Rounding::Rne), (QNAN64, NV));
}

#[test]
fn tininess_is_detected_after_rounding() {
    // 2^-126 * (1 - 2^-24) is below the smallest normal but rounds up to
    // 2^-126 under RNE: after-rounding tininess still flags UF.
    let min_normal = 0x0080_0000u32;
    let below_one = 0x3f7f_ffffu32;
    assert_eq!(
        single::mul(min_normal, below_one, Rounding::Rne),
        (min_normal, FpFlags::UF | NX)
    );
    assert_eq!(
        single::mul(min_normal, below_one, Rounding::Rtz),
        (0x007f_ffff, FpFlags::UF | NX)
    );
    // An exact subnormal result is not underflow.
    let half = 0x3f00_0000u32;
    assert_eq!(
        single::mul(min_normal, half, Rounding::Rne),
        (0x0040_0000, ZERO)
    );
    // Rounding a tiny result to zero is tiny.
    assert_eq!(
        single::mul(0x0000_0001, half, Rounding::Rne),
        (0x0000_0000, FpFlags::UF | NX)
    );
    assert_eq!(
        single::mul(0x0000_0001, half, Rounding::Rup),
        (0x0000_0001, FpFlags::UF | NX)
    );
    // Overflow raises OF|NX and saturates according to the rounding mode.
    assert_eq!(
        single::mul(0x7f7f_ffff, 0x4000_0000, Rounding::Rne),
        (0x7f80_0000, FpFlags::OF | NX)
    );
    assert_eq!(
        single::mul(0x7f7f_ffff, 0x4000_0000, Rounding::Rtz),
        (0x7f7f_ffff, FpFlags::OF | NX)
    );
}

#[test]
fn division_by_zero_and_sqrt_results() {
    assert_eq!(
        single::div(0x3f80_0000, 0x0000_0000, Rounding::Rne),
        (0x7f80_0000, FpFlags::DZ)
    );
    assert_eq!(
        single::div(0xbf80_0000, 0x0000_0000, Rounding::Rne),
        (0xff80_0000, FpFlags::DZ)
    );
    assert_eq!(
        single::sqrt(0x4080_0000, Rounding::Rne),
        (0x4000_0000, ZERO)
    );
    assert_eq!(single::sqrt(0x4000_0000, Rounding::Rne), (0x3fb5_04f3, NX));
    assert_eq!(single::sqrt(0x4000_0000, Rounding::Rup), (0x3fb5_04f4, NX));
    assert_eq!(
        double::sqrt(0x4000_0000_0000_0000, Rounding::Rne),
        (0x3ff6_a09e_667f_3bcd, NX)
    );
    assert_eq!(
        double::sqrt(0x7ff0_0000_0000_0000, Rounding::Rne),
        (0x7ff0_0000_0000_0000, ZERO)
    );
}

#[test]
fn fcvt_float_to_integer_saturation_table() {
    // The invalid-input column of the RISC-V conversion table:
    // +inf/NaN/positive overflow -> maximum, -inf/negative overflow ->
    // minimum (or zero for unsigned), independent of `rm`.
    let max32 = 0x0000_0000_7fff_ffffu64;
    let min32 = 0xffff_ffff_8000_0000u64;
    let max_unsigned32 = 0xffff_ffff_ffff_ffffu64;
    for rm in [
        Rounding::Rne,
        Rounding::Rtz,
        Rounding::Rdn,
        Rounding::Rup,
        Rounding::Rmm,
    ] {
        assert_eq!(single::to_i32(0x7f80_0000, rm), (max32, NV));
        assert_eq!(single::to_i32(0xff80_0000, rm), (min32, NV));
        assert_eq!(single::to_i32(QNAN32, rm), (max32, NV));
        assert_eq!(single::to_i32(0x7fc0_0001, rm), (max32, NV));
        assert_eq!(single::to_ui32(0xff80_0000, rm), (0, NV));
        assert_eq!(single::to_ui32(QNAN32, rm), (max_unsigned32, NV));
        assert_eq!(single::to_i64(0x7f80_0000, rm), (i64::MAX as u64, NV));
        assert_eq!(single::to_i64(0xff80_0000, rm), (i64::MIN as u64, NV));
        assert_eq!(single::to_ui64(0xff80_0000, rm), (0, NV));
        assert_eq!(single::to_ui64(SNAN32, rm), (u64::MAX, NV));
    }

    // In-range boundaries and RV64 sign extension of 32-bit results.
    assert_eq!(single::to_i32(0x4f00_0000, Rounding::Rne), (max32, NV)); // 2^31
    assert_eq!(single::to_i32(0xcf00_0000, Rounding::Rne), (min32, ZERO)); // -2^31
    assert_eq!(single::to_i32(0xcf00_0001, Rounding::Rne), (min32, NV));
    assert_eq!(
        single::to_i32(0x4eff_ffff, Rounding::Rne),
        (2_147_483_520, ZERO)
    );
    assert_eq!(
        single::to_ui32(0x4f80_0000, Rounding::Rne),
        (max_unsigned32, NV)
    ); // 2^32
    assert_eq!(single::to_ui32(0xbf80_0000, Rounding::Rtz), (0, NV)); // -1.0
    assert_eq!(
        single::to_i64(0x5f00_0000, Rounding::Rne),
        (i64::MAX as u64, NV)
    ); // 2^63
    assert_eq!(
        single::to_i64(0xdf00_0000, Rounding::Rne),
        (i64::MIN as u64, ZERO)
    ); // -2^63

    // Fractions raise only inexact; rounding follows `rm`.
    let one_point_seventy_five = 0x3fe0_0000;
    assert_eq!(
        single::to_i32(one_point_seventy_five, Rounding::Rne),
        (2, NX)
    );
    assert_eq!(
        single::to_i32(one_point_seventy_five, Rounding::Rtz),
        (1, NX)
    );
    assert_eq!(
        single::to_i32(one_point_seventy_five, Rounding::Rup),
        (2, NX)
    );

    // Ties round per mode; RMM is distinct from RNE.
    let two_point_five = 0x4020_0000; // 2.5
    let minus_two_point_five = 0xc020_0000;
    assert_eq!(single::to_i32(two_point_five, Rounding::Rne), (2, NX));
    assert_eq!(single::to_i32(two_point_five, Rounding::Rmm), (3, NX));
    assert_eq!(single::to_i32(two_point_five, Rounding::Rtz), (2, NX));
    assert_eq!(single::to_i32(two_point_five, Rounding::Rdn), (2, NX));
    assert_eq!(single::to_i32(two_point_five, Rounding::Rup), (3, NX));
    assert_eq!(
        single::to_i32(minus_two_point_five, Rounding::Rne),
        (0xffff_ffff_ffff_fffe, NX)
    );
    assert_eq!(
        single::to_i32(minus_two_point_five, Rounding::Rmm),
        (0xffff_ffff_ffff_fffd, NX)
    );
    // Signed zeros convert to zero exactly.
    assert_eq!(single::to_i32(0x0000_0000, Rounding::Rdn), (0, ZERO));
    assert_eq!(single::to_i32(0x8000_0000, Rounding::Rup), (0, ZERO));
    assert_eq!(
        double::to_i64(0x8000_0000_0000_0000, Rounding::Rne),
        (0, ZERO)
    );
    assert_eq!(single::to_i64(0x8000_0000, Rounding::Rmm), (0, ZERO));

    // Unsigned conversion of a negative value saturates to zero
    // (negative control from the contract).
    assert_eq!(single::to_ui32(0xbf80_0000, Rounding::Rne), (0, NV));
    assert_eq!(single::to_ui32(0xbf00_0000, Rounding::Rne), (0, NX)); // -0.5 -> 0
    assert_eq!(single::to_ui32(0xbf00_0000, Rounding::Rdn), (0, NV)); // -0.5 -> -1

    // Double precision follows the same table.
    assert_eq!(
        double::to_i32(0x7ff0_0000_0000_0000, Rounding::Rne),
        (max32, NV)
    );
    assert_eq!(
        double::to_i32(0xfff0_0000_0000_0000, Rounding::Rne),
        (min32, NV)
    );
    assert_eq!(double::to_i32(QNAN64, Rounding::Rne), (max32, NV));
    assert_eq!(
        double::to_i64(0x43e0_0000_0000_0000, Rounding::Rne),
        (i64::MAX as u64, NV)
    );
    assert_eq!(
        double::to_i64(0xc3e0_0000_0000_0000, Rounding::Rne),
        (i64::MIN as u64, ZERO)
    );
    assert_eq!(
        double::to_i32(0x41e0_0000_0000_0000, Rounding::Rne),
        (max32, NV)
    ); // 2^31
}

#[test]
fn fcvt_integer_to_float_rounds_per_mode() {
    assert_eq!(single::from_i32(-3, Rounding::Rne), (0xc040_0000, ZERO));
    assert_eq!(single::from_i32(0, Rounding::Rdn), (0x0000_0000, ZERO));
    assert_eq!(
        single::from_ui32(u32::MAX, Rounding::Rne),
        (0x4f80_0000, NX)
    );
    // i64::MAX rounds to 2^63 (RNE/RUP) or 2^63 - 2^40 (RTZ/RDN).
    assert_eq!(single::from_i64(i64::MAX, Rounding::Rne), (0x5f00_0000, NX));
    assert_eq!(single::from_i64(i64::MAX, Rounding::Rtz), (0x5eff_ffff, NX));
    assert_eq!(single::from_i64(i64::MAX, Rounding::Rup), (0x5f00_0000, NX));
    // u64::MAX rounds to 2^64 (RNE) or the largest float below it (RTZ).
    assert_eq!(
        single::from_ui64(u64::MAX, Rounding::Rne),
        (0x5f80_0000, NX)
    );
    assert_eq!(
        single::from_ui64(u64::MAX, Rounding::Rtz),
        (0x5f7f_ffff, NX)
    );
    assert_eq!(
        double::from_i64(i64::MAX, Rounding::Rne),
        (0x43e0_0000_0000_0000, NX)
    );
    assert_eq!(
        double::from_i64(i64::MAX, Rounding::Rtz),
        (0x43df_ffff_ffff_ffff, NX)
    );
    assert_eq!(
        double::from_ui64(u64::MAX, Rounding::Rne),
        (0x43f0_0000_0000_0000, NX)
    );
}

#[test]
fn fcvt_between_formats_canonicalizes_nan_and_flags_signaling() {
    assert_eq!(
        double::from_single(0x3f80_0000),
        (0x3ff0_0000_0000_0000, ZERO)
    );
    assert_eq!(
        double::from_single(0x0000_0001),
        (0x36a0_0000_0000_0000, ZERO)
    );
    assert_eq!(double::from_single(SNAN32), (QNAN64, NV));
    assert_eq!(double::from_single(0x7fc0_0001), (QNAN64, ZERO));

    assert_eq!(
        single::from_double(0x3ff0_0000_0000_0001, Rounding::Rne),
        (0x3f80_0000, NX)
    );
    assert_eq!(
        single::from_double(0x3ff0_0000_0000_0001, Rounding::Rup),
        (0x3f80_0001, NX)
    );
    assert_eq!(single::from_double(SNAN64, Rounding::Rne), (QNAN32, NV));
    assert_eq!(
        single::from_double(0x7ff8_0000_0000_0001, Rounding::Rne),
        (QNAN32, ZERO)
    );
    // f64 -> f32 underflow and overflow flags.
    assert_eq!(
        single::from_double(0x0000_0000_0000_0001, Rounding::Rne),
        (0x0000_0000, FpFlags::UF | NX)
    );
    assert_eq!(
        single::from_double(0x47f0_0000_2000_0000, Rounding::Rne),
        (0x7f80_0000, FpFlags::OF | NX)
    );
}

#[test]
fn comparisons_use_quiet_and_signaling_nan_rules() {
    let one = 0x3f80_0000u32;
    let qnan = 0x7fc0_0001u32;
    assert_eq!(single::eq(qnan, one), (false, ZERO));
    assert_eq!(single::eq(one, qnan), (false, ZERO));
    assert_eq!(single::eq(SNAN32, one), (false, NV));
    assert_eq!(single::lt(qnan, one), (false, NV));
    assert_eq!(single::lt(one, SNAN32), (false, NV));
    assert_eq!(single::le(qnan, qnan), (false, NV));

    assert_eq!(single::lt(0xbf80_0000, 0x0000_0000), (true, ZERO));
    assert_eq!(single::lt(0x0000_0000, 0xbf80_0000), (false, ZERO));
    assert_eq!(single::lt(0x8000_0000, 0x0000_0000), (false, ZERO));
    assert_eq!(single::le(0x8000_0000, 0x0000_0000), (true, ZERO));
    assert_eq!(single::eq(0x8000_0000, 0x0000_0000), (true, ZERO));
    assert_eq!(single::lt(0x3f80_0000, 0x4000_0000), (true, ZERO));
    assert_eq!(single::le(0x3f80_0000, 0x3f80_0000), (true, ZERO));
    assert_eq!(single::lt(0x3f80_0000, 0x3f80_0000), (false, ZERO));

    assert_eq!(double::eq(0x7ff8_0000_0000_0001, 0), (false, ZERO));
    assert_eq!(double::lt(0x7ff0_0000_0000_0001, 0), (false, NV));
    assert_eq!(double::le(0, 0x8000_0000_0000_0000), (true, ZERO));
}
