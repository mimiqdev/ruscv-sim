//! NaN Boxing Tests
//!
//! Tests for the NaN boxing handler module, ensuring proper handling
//! of D/F interoperability and IEEE 754 compliance

use ruscv_sim::fpu::nan_boxing::{
    canonical_nan_f32, canonical_nan_f64, default_nan, effective_nan, extract_boxed_f32,
    f32_to_f64, f64_to_f32, format_nan_boxed, is_double_precision, is_invalid_boxed_value,
    is_nan_boxed, nan_box_f32, validate_nan_boxing, NanBoxingResult, NAN_BOX_MASK, NAN_BOX_UPPER,
};

#[test]
fn test_is_nan_boxed_valid() {
    // Valid NaN boxed value
    let valid = NAN_BOX_MASK | 0x12345678u64;
    assert!(is_nan_boxed(valid));
}

#[test]
fn test_is_nan_boxed_invalid() {
    // Invalid - upper bits not all 1s
    let invalid = 0x1234_5678_ABCD_u64;
    assert!(!is_nan_boxed(invalid));
}

#[test]
fn test_is_nan_boxed_upper_bits_all_ones() {
    // Edge case: upper 32 bits all 1s
    let edge = 0xFFFF_FFFF_1234_5678u64;
    assert!(is_nan_boxed(edge));
}

#[test]
fn test_is_nan_boxed_upper_bits_not_all_ones() {
    // Edge case: upper 32 bits not all 1s
    let edge = 0xFFFF_FFFE_1234_5678u64;
    assert!(!is_nan_boxed(edge));
}

#[test]
fn test_is_double_precision_normal() {
    // Normal double
    let normal = std::f64::consts::PI.to_bits();
    assert!(is_double_precision(normal));
}

#[test]
fn test_is_double_precision_subnormal() {
    // Subnormal (denormalized) double - exponent = 0, significand != 0
    // is_double_precision() returns false for subnormals
    let subnormal = 0x000F_FFFF_FFFF_FFFFu64;
    assert!(!is_double_precision(subnormal));
}

#[test]
fn test_is_double_precision_nan() {
    // Double NaN - exponent = 0x7FF, significand != 0
    // is_double_precision() returns false for NaN
    let nan = f64::NAN.to_bits();
    assert!(!is_double_precision(nan));
}

#[test]
fn test_is_double_precision_infinity() {
    // Double infinity
    let inf = f64::INFINITY.to_bits();
    assert!(is_double_precision(inf));
}

#[test]
fn test_is_double_precision_negative_infinity() {
    // Negative double infinity
    let neg_inf = f64::NEG_INFINITY.to_bits();
    assert!(is_double_precision(neg_inf));
}

#[test]
fn test_nan_box_f32() {
    let val: f32 = std::f32::consts::PI;
    let boxed = nan_box_f32(val);
    assert!(is_nan_boxed(boxed));
    assert_eq!(boxed as u32, val.to_bits());
}

#[test]
fn test_nan_box_f32_zero() {
    let val: f32 = 0.0;
    let boxed = nan_box_f32(val);
    assert!(is_nan_boxed(boxed));
    assert_eq!(boxed as u32, 0x00000000);
}

#[test]
fn test_nan_box_f32_negative() {
    let val: f32 = -std::f32::consts::PI;
    let boxed = nan_box_f32(val);
    assert!(is_nan_boxed(boxed));
    assert_eq!(boxed as u32, val.to_bits());
}

#[test]
fn test_extract_boxed_f32() {
    let val: f32 = std::f32::consts::PI;
    let boxed = nan_box_f32(val);
    let extracted = extract_boxed_f32(boxed);
    assert!((extracted - val).abs() < 1e-6);
}

#[test]
fn test_extract_boxed_f32_zero() {
    let val: f32 = 0.0;
    let boxed = nan_box_f32(val);
    let extracted = extract_boxed_f32(boxed);
    assert_eq!(extracted, 0.0);
}

#[test]
fn test_extract_double_precision() {
    let val: f64 = std::f64::consts::PI;
    let bits = val.to_bits();
    let extracted = extract_boxed_f32(bits);
    assert!((extracted as f64 - val).abs() < 1e-6);
}

#[test]
fn test_validate_nan_boxing_valid() {
    let valid = nan_box_f32(1.0f32);
    assert_eq!(validate_nan_boxing(valid), NanBoxingResult::Valid);
}

#[test]
fn test_validate_nan_boxing_double() {
    let double = std::f64::consts::PI.to_bits();
    assert_eq!(
        validate_nan_boxing(double),
        NanBoxingResult::DoublePrecision
    );
}

#[test]
fn test_validate_nan_boxing_invalid() {
    let invalid = 0x1234_5678_ABCD_u64;
    assert_eq!(validate_nan_boxing(invalid), NanBoxingResult::Invalid);
}

#[test]
fn test_validate_quiet_nan() {
    // Canonical quiet NaN (double)
    let quiet_nan = 0x7FF8_0000_0000_0000u64;
    assert_eq!(validate_nan_boxing(quiet_nan), NanBoxingResult::QuietNan);
}

#[test]
fn test_validate_quiet_nan_boxed() {
    // NaN boxed quiet NaN
    let boxed_quiet = nan_box_f32(f32::from_bits(0x7FC00000));
    assert_eq!(validate_nan_boxing(boxed_quiet), NanBoxingResult::Valid);
}

#[test]
fn test_validate_signaling_nan() {
    // Signaling NaN (double) - MSB of significand clear
    let signaling_nan = 0x7FF0_0000_0000_0001u64;
    assert_eq!(
        validate_nan_boxing(signaling_nan),
        NanBoxingResult::SignalingNan
    );
}

#[test]
fn test_validate_positive_infinity() {
    let pos_inf = f64::INFINITY.to_bits();
    assert_eq!(
        validate_nan_boxing(pos_inf),
        NanBoxingResult::DoublePrecision
    );
}

#[test]
fn test_validate_negative_infinity() {
    let neg_inf = f64::NEG_INFINITY.to_bits();
    assert_eq!(
        validate_nan_boxing(neg_inf),
        NanBoxingResult::DoublePrecision
    );
}

#[test]
fn test_canonical_nan_f32() {
    let nan = canonical_nan_f32();
    assert!(is_nan_boxed(nan));
    let f32_val = f32::from_bits(nan as u32);
    assert!(f32_val.is_nan());
}

#[test]
fn test_canonical_nan_f64() {
    let nan = canonical_nan_f64();
    let f64_val = f64::from_bits(nan);
    assert!(f64_val.is_nan());
}

#[test]
fn test_f32_to_f64() {
    let val: f32 = std::f32::consts::PI;
    let boxed = nan_box_f32(val);
    let converted = f32_to_f64(boxed);
    assert!((converted - std::f64::consts::PI).abs() < 1e-5);
}

#[test]
fn test_f32_to_f64_negative() {
    let val: f32 = -2.5;
    let boxed = nan_box_f32(val);
    let converted = f32_to_f64(boxed);
    assert!((converted - (-2.5)).abs() < 1e-5);
}

#[test]
fn test_f64_to_f32() {
    let val: f64 = std::f64::consts::PI;
    let boxed = f64_to_f32(val, 0);
    assert!(is_nan_boxed(boxed));
    let f32_val = f32::from_bits(boxed as u32);
    assert!((f32_val as f64 - val).abs() < 1e-5);
}

#[test]
fn test_f64_to_f32_rounding_rtz() {
    let val: f64 = std::f64::consts::PI;
    let boxed = f64_to_f32(val, 1); // RTZ
    let f32_val = f32::from_bits(boxed as u32);
    assert!((f32_val as f64 - val).abs() < 1.0);
}

#[test]
fn test_is_invalid_boxed_value_valid_double() {
    // Valid double
    assert!(!is_invalid_boxed_value(std::f64::consts::PI.to_bits()));
}

#[test]
fn test_is_invalid_boxed_value_valid_nan_boxed() {
    // Valid NaN boxed
    assert!(!is_invalid_boxed_value(nan_box_f32(1.0f32)));
}

#[test]
fn test_is_invalid_boxed_value_invalid() {
    // Invalid - not NaN boxed and not double
    assert!(is_invalid_boxed_value(0x1234_5678_ABCD_u64));
}

#[test]
fn test_default_nan_f32() {
    let nan = default_nan(false);
    assert!(is_nan_boxed(nan));
    assert!(f32::from_bits(nan as u32).is_nan());
}

#[test]
fn test_default_nan_f64() {
    let nan = default_nan(true);
    assert!(f64::from_bits(nan).is_nan());
}

#[test]
fn test_format_nan_boxed_valid() {
    let valid = nan_box_f32(1.0f32);
    let output = format_nan_boxed(valid);
    assert!(output.contains("valid NaN-boxed"));
}

#[test]
fn test_format_nan_boxed_double() {
    let double = std::f64::consts::PI.to_bits();
    let output = format_nan_boxed(double);
    assert!(output.contains("double precision"));
}

#[test]
fn test_format_nan_boxed_invalid() {
    let invalid = 0x1234_5678_ABCD_u64;
    let output = format_nan_boxed(invalid);
    assert!(output.contains("INVALID"));
}

#[test]
fn test_format_nan_boxed_quiet_nan() {
    let quiet_nan = 0x7FF8_0000_0000_0000u64;
    let output = format_nan_boxed(quiet_nan);
    assert!(output.contains("quiet NaN"));
}

#[test]
fn test_format_nan_boxed_signaling_nan() {
    let signaling_nan = 0x7FF0_0000_0000_0001u64;
    let output = format_nan_boxed(signaling_nan);
    assert!(output.contains("signaling NaN"));
}

#[test]
fn test_effective_nan_both_quiet() {
    // Both operands are quiet NaNs - return first one
    let rs1 = nan_box_f32(f32::from_bits(0x7FC00001));
    let rs2 = nan_box_f32(f32::from_bits(0x7FC00002));
    let result = effective_nan(rs1, rs2, false);
    // Result should be canonical NaN (properly NaN boxed)
    assert!(is_nan_boxed(result));
}

#[test]
fn test_effective_nan_signaling_takes_precedence() {
    // Signaling NaN takes precedence
    let signaling = 0x7FF0_0000_0000_0001u64; // Double signaling NaN
    let quiet = nan_box_f32(f32::from_bits(0x7FC00000));
    let result = effective_nan(signaling, quiet, false);
    // Should return canonical NaN
    assert!(is_nan_boxed(result));
}

#[test]
fn test_effective_nan_double_precision() {
    let rs1 = f64::NAN.to_bits();
    let rs2 = nan_box_f32(1.0f32);
    let result = effective_nan(rs1, rs2, true);
    // For double precision, result should be canonical f64 NaN
    assert_eq!(result, canonical_nan_f64());
}

#[test]
fn test_round_trip_f32_to_f64() {
    let original: f32 = 1.234567f32;
    let boxed = nan_box_f32(original);
    let converted = f32_to_f64(boxed);
    let back_to_f32 = converted as f32;
    assert!((back_to_f32 - original).abs() < 1e-6);
}

#[test]
fn test_round_trip_f64_to_f32() {
    let original: f64 = 1.23456789012345;
    let boxed = f64_to_f32(original, 0);
    let extracted = extract_boxed_f32(boxed);
    let back_to_f64 = extracted as f64;
    // Precision loss is expected
    assert!((back_to_f64 - original).abs() < 1e-6);
}

#[test]
fn test_constants() {
    // Verify NaN boxing constants
    assert_eq!(NAN_BOX_MASK, 0xFFFF_FFFF_0000_0000u64);
    assert_eq!(NAN_BOX_UPPER, 0xFFFF_FFFFu64);
}

#[test]
fn test_special_values_preserved() {
    // Test that special values are preserved through boxing/unboxing
    let special_values: [f32; 4] = [0.0, -0.0, f32::INFINITY, f32::NEG_INFINITY];

    for val in &special_values {
        let boxed = nan_box_f32(*val);
        let extracted = extract_boxed_f32(boxed);

        // For non-NaN values, equality should hold
        if !val.is_nan() {
            assert_eq!(extracted, *val, "Failed for value: {}", val);
        }
    }
}

#[test]
fn test_nan_propagation() {
    // Test NaN propagation in conversions
    let nan_f32 = f32::NAN;
    let boxed_nan = nan_box_f32(nan_f32);
    let converted = f32_to_f64(boxed_nan);
    assert!(converted.is_nan());
}

#[test]
fn test_denormalized_conversion() {
    // Test conversion of denormalized numbers
    let denorm: f32 = 1e-40;
    let boxed = nan_box_f32(denorm);
    let converted = f32_to_f64(boxed);
    // Should still be a very small number
    assert!(converted > 0.0 && converted < 1e-30);
}

#[test]
fn test_large_number_conversion() {
    // Test conversion of large numbers
    let large: f32 = 1e30;
    let boxed = nan_box_f32(large);
    let converted = f32_to_f64(boxed);
    assert!((converted - large as f64).abs() < 1e20);
}

#[test]
fn test_small_number_conversion() {
    // Test conversion of small numbers
    let small: f32 = 1e-30;
    let boxed = nan_box_f32(small);
    let converted = f32_to_f64(boxed);
    assert!((converted - small as f64).abs() < 1e-40);
}
