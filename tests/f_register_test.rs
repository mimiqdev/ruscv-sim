//! FPU Register Tests
//!
//! Tests for FPU register file operations including NaN boxing

use ruscv_sim::fpu::{Fpr, FpuRegisterFile};
use ruscv_sim::CoreState;

#[test]
fn test_fpr_nan_boxing() {
    let value: f32 = 3.14159;
    let fpr = Fpr::new(value);

    assert!((fpr.get() - value).abs() < 1e-5);
    assert!(fpr.is_nan_boxed());
}

#[test]
fn test_fpr_bits() {
    let value: f32 = 2.71828;
    let fpr = Fpr::new(value);

    let bits = fpr.bits();
    // Lower 32 bits should be the f32 bits
    assert_eq!(bits as u32, value.to_bits());
    // Upper 32 bits should be all 1s (NaN boxing)
    assert_eq!((bits >> 32) as u32, 0xFFFF_FFFF);
}

#[test]
fn test_fpr_lower_bits() {
    let value: f32 = 1.5;
    let fpr = Fpr::new(value);

    assert_eq!(fpr.lower(), value.to_bits());
}

#[test]
fn test_fpr_from_bits() {
    let raw_bits: u64 = 0xFFFF_FFFF_4000_0000u64; // 2.0 NaN boxed
    let fpr = Fpr::from_bits(raw_bits);

    assert_eq!(fpr.get(), 2.0f32);
    assert!(fpr.is_nan_boxed());
}

#[test]
fn test_fpr_canonical_nan() {
    let nan = Fpr::canonical_nan();
    assert!(nan.get().is_nan());
    assert!(nan.is_nan_boxed());
}

#[test]
fn test_fpu_register_file_new() {
    let fpr = FpuRegisterFile::new();

    // All registers should default to 0 (NaN boxed)
    for i in 0..32 {
        let val = fpr.read(i);
        assert_eq!(val.get(), 0.0f32);
        assert!(val.is_nan_boxed());
    }
}

#[test]
fn test_fpu_register_file_write() {
    let mut fpr = FpuRegisterFile::new();

    fpr.write(1, Fpr::new(5.0));
    assert!((fpr.read(1).get() - 5.0).abs() < 1e-5);
}

#[test]
fn test_fpu_register_file_write_u32() {
    let mut fpr = FpuRegisterFile::new();

    fpr.write_u32(2, 0x41200000u32); // 10.0 in IEEE 754

    let val = fpr.read(2);
    assert!((val.get() - 10.0).abs() < 1e-5);
    assert!(val.is_nan_boxed());
}

#[test]
fn test_fpu_register_file_read_u32() {
    let mut fpr = FpuRegisterFile::new();

    fpr.write(3, Fpr::new(7.0));
    let bits = fpr.read_u32(3);

    assert_eq!(bits, 7.0f32.to_bits());
}

#[test]
fn test_fpu_register_file_f0_hardwired_zero() {
    let mut fpr = FpuRegisterFile::new();

    // f0 should always read as 0
    assert_eq!(fpr.read(0).get(), 0.0f32);

    // Writing to f0 should be ignored
    fpr.write(0, Fpr::new(999.0));
    assert_eq!(fpr.read(0).get(), 0.0f32);
}

#[test]
fn test_fpu_register_file_reset() {
    let mut fpr = FpuRegisterFile::new();

    fpr.write(1, Fpr::new(42.0));
    fpr.write(2, Fpr::new(100.0));

    fpr.reset();

    assert_eq!(fpr.read(1).get(), 0.0f32);
    assert_eq!(fpr.read(2).get(), 0.0f32);
}

#[test]
fn test_fpu_register_file_reg_masking() {
    let mut fpr = FpuRegisterFile::new();

    // Write to register 33 (should map to 1)
    fpr.write(33, Fpr::new(5.0));

    // Register 1 should have the value
    assert!((fpr.read(1).get() - 5.0).abs() < 1e-5);
    // Register 33 doesn't exist, but since we mask to 5 bits, it's same as 1
    assert!((fpr.read(33).get() - 5.0).abs() < 1e-5);
}

#[test]
fn test_fpu_register_file_is_valid_reg() {
    assert!(FpuRegisterFile::is_valid_reg(0));
    assert!(FpuRegisterFile::is_valid_reg(31));
    assert!(!FpuRegisterFile::is_valid_reg(32));
    assert!(!FpuRegisterFile::is_valid_reg(100));
}

#[test]
fn test_core_state_fpr_integration() {
    let mut state = CoreState::default();

    // FPR should be accessible
    state.fpr.write(1, Fpr::new(3.14));
    assert!((state.fpr.read(1).get() - 3.14).abs() < 1e-5);
}

#[test]
fn test_special_float_values() {
    let mut fpr = FpuRegisterFile::new();

    // Positive zero
    fpr.write(1, Fpr::new(0.0));
    assert_eq!(fpr.read(1).get(), 0.0);

    // Negative zero
    fpr.write(2, Fpr::new(-0.0));
    assert_eq!(fpr.read(2).get(), -0.0);

    // Infinity
    fpr.write(3, Fpr::new(f32::INFINITY));
    assert_eq!(fpr.read(3).get(), f32::INFINITY);

    // Negative infinity
    fpr.write(4, Fpr::new(f32::NEG_INFINITY));
    assert_eq!(fpr.read(4).get(), f32::NEG_INFINITY);

    // NaN
    fpr.write(5, Fpr::new(f32::NAN));
    assert!(fpr.read(5).get().is_nan());
}

#[test]
fn test_large_float_values() {
    let mut fpr = FpuRegisterFile::new();

    // Very small number
    fpr.write(1, Fpr::new(1e-38));
    assert!((fpr.read(1).get() - 1e-38).abs() < 1e-45);

    // Very large number
    fpr.write(2, Fpr::new(1e38));
    assert!((fpr.read(2).get() - 1e38).abs() < 1e31);
}

#[test]
fn test_negative_float_values() {
    let mut fpr = FpuRegisterFile::new();

    fpr.write(1, Fpr::new(-5.5));
    assert!((fpr.read(1).get() - (-5.5)).abs() < 1e-5);

    fpr.write(2, Fpr::new(-0.001));
    assert!((fpr.read(2).get() - (-0.001)).abs() < 1e-6);
}

#[test]
fn test_fpr_debug_format() {
    let fpr = Fpr::new(1.0);
    let debug_str = format!("{:?}", fpr);
    // Should show the raw bits
    assert!(debug_str.contains("0x") || debug_str.contains("NaN"));
}
