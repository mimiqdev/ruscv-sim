//! RV64C C0 Quadrant Floating-Point Instructions
//!
//! This module implements the execution of C0 quadrant compressed floating-point
//! instructions for RV64C extension with F and D extensions.
//!
//! ## Instructions
//!
//! - **C.FLD**: Load double-precision floating-point (RV64D)
//! - **C.FLW**: Load single-precision floating-point (RV64F)
//! - **C.FSD**: Store double-precision floating-point (RV64D)
//! - **C.FSW**: Store single-precision floating-point (RV64F)

use crate::core::CoreState;
use crate::execute::ExecuteError;
use crate::fpu::Fpr;
use crate::memory::MemoryInterface;

/// Execute C.FLD - Load double-precision floating-point (compressed)
///
/// Expands to: `fld rd', offset[7:3](rs1')`
///
/// Loads a 64-bit double-precision floating-point value from memory into
/// floating-point register `rd'`. The effective address is formed by adding
/// the zero-extended offset (scaled by 8) to the base address in register `rs1'`.
///
/// # Arguments
/// * `rd` - Destination floating-point register (compressed, 8-15)
/// * `rs1` - Base address register (compressed, 8-15)
/// * `offset` - Offset value (0-248, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Note
/// Requires RV64D extension
pub fn exec_c_fld(
    rd: u8,
    rs1: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    // Convert compressed register indices (0-7) to full indices (8-15)
    let rs1_full = (rs1 & 0x7) + 8;
    let rd_full = (rd & 0x7) + 8;

    let base_addr = state.regs[rs1_full as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment (8-byte aligned for double precision)
    if !effective_addr.is_multiple_of(8) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 8));
    }

    // Load 64-bit double from memory
    let value = mem.read_dword(effective_addr)?;

    // Write to FPR (stored as raw bits, no NaN boxing needed for double)
    state.fpr.write(rd_full as usize, Fpr::from_bits(value));

    Ok(())
}

/// Execute C.FLW - Load single-precision floating-point (compressed)
///
/// Expands to: `flw rd', offset[6:2](rs1')`
///
/// Loads a 32-bit single-precision floating-point value from memory into
/// floating-point register `rd'`. The effective address is formed by adding
/// the zero-extended offset (scaled by 4) to the base address in register `rs1'`.
/// The loaded value is NaN-boxed in the 64-bit register.
///
/// # Arguments
/// * `rd` - Destination floating-point register (compressed, 8-15)
/// * `rs1` - Base address register (compressed, 8-15)
/// * `offset` - Offset value (0-124, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Note
/// Requires RV64F extension
pub fn exec_c_flw(
    rd: u8,
    rs1: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    // Convert compressed register indices (0-7) to full indices (8-15)
    let rs1_full = (rs1 & 0x7) + 8;
    let rd_full = (rd & 0x7) + 8;

    let base_addr = state.regs[rs1_full as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment (4-byte aligned for single precision)
    if !effective_addr.is_multiple_of(4) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Load 32-bit float from memory
    let value = mem.read_word(effective_addr)?;

    // NaN-box the value and write to FPR
    let fpr = Fpr::new(f32::from_bits(value));
    state.fpr.write(rd_full as usize, fpr);

    Ok(())
}

/// Execute C.FSD - Store double-precision floating-point (compressed)
///
/// Expands to: `fsd rs2', offset[7:3](rs1')`
///
/// Stores the 64-bit double-precision floating-point value from register `rs2'`
/// to memory. The effective address is formed by adding the zero-extended
/// offset (scaled by 8) to the base address in register `rs1'`.
///
/// # Arguments
/// * `rs1` - Base address register (compressed, 8-15)
/// * `rs2` - Source floating-point register (compressed, 8-15)
/// * `offset` - Offset value (0-248, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Note
/// Requires RV64D extension
pub fn exec_c_fsd(
    rs1: u8,
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    // Convert compressed register indices (0-7) to full indices (8-15)
    let rs1_full = (rs1 & 0x7) + 8;
    let rs2_full = (rs2 & 0x7) + 8;

    let base_addr = state.regs[rs1_full as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment (8-byte aligned for double precision)
    if !effective_addr.is_multiple_of(8) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 8));
    }

    // Read 64-bit value from FPR
    let bits = state.fpr.read(rs2_full as usize).bits();

    // Store to memory as 64-bit value
    mem.write_dword(effective_addr, bits)?;

    Ok(())
}

/// Execute C.FSW - Store single-precision floating-point (compressed)
///
/// Expands to: `fsw rs2', offset[6:2](rs1')`
///
/// Stores the lower 32 bits of register `rs2'` (single-precision floating-point)
/// to memory. The effective address is formed by adding the zero-extended
/// offset (scaled by 4) to the base address in register `rs1'`.
///
/// # Arguments
/// * `rs1` - Base address register (compressed, 8-15)
/// * `rs2` - Source floating-point register (compressed, 8-15)
/// * `offset` - Offset value (0-124, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Note
/// Requires RV64F extension
pub fn exec_c_fsw(
    rs1: u8,
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    // Convert compressed register indices (0-7) to full indices (8-15)
    let rs1_full = (rs1 & 0x7) + 8;
    let rs2_full = (rs2 & 0x7) + 8;

    let base_addr = state.regs[rs1_full as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment (4-byte aligned for single precision)
    if !effective_addr.is_multiple_of(4) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Read lower 32 bits from FPR and store to memory
    let value = state.fpr.read_u32(rs2_full as usize);
    mem.write_word(effective_addr, value)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::memory::SimpleMemory;

    fn setup_test() -> (CoreState, SimpleMemory) {
        (CoreState::default(), SimpleMemory::new(0x20000)) // 128KB
    }

    // C.FLD Tests
    #[test]
    fn test_c_fld_basic() {
        let (mut state, mut mem) = setup_test();

        // Set up base address (using compressed register x8)
        state.regs[8] = 0x100;

        // Write a double value to memory
        let test_value: f64 = std::f64::consts::PI;
        let bits = test_value.to_bits();
        mem.write_word(0x100, bits as u32).unwrap();
        mem.write_word(0x104, (bits >> 32) as u32).unwrap();

        // Execute C.FLD f8, 0(x8)
        // rd'=0 (f8), rs1'=0 (x8), offset=0
        exec_c_fld(0, 0, 0, &mut state, &mut mem).unwrap();

        // Verify the value was loaded
        let loaded_bits = state.fpr.read(8).bits();
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_c_fld_with_offset() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[9] = 0x100;

        // Write a double value to memory at offset 16
        let test_value: f64 = std::f64::consts::E;
        let bits = test_value.to_bits();
        mem.write_word(0x110, bits as u32).unwrap();
        mem.write_word(0x114, (bits >> 32) as u32).unwrap();

        // Execute C.FLD f9, 16(x9)
        // rd'=1 (f9), rs1'=1 (x9), offset=16
        exec_c_fld(1, 1, 16, &mut state, &mut mem).unwrap();

        // Verify the value was loaded
        let loaded_bits = state.fpr.read(9).bits();
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_c_fld_misaligned() {
        let (mut state, mut mem) = setup_test();

        // Set up misaligned base address
        state.regs[8] = 0x101;

        // Should fail due to misalignment
        let result = exec_c_fld(0, 0, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_fld_special_values() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x100;

        // Test infinity
        let inf_bits = f64::INFINITY.to_bits();
        mem.write_word(0x100, inf_bits as u32).unwrap();
        mem.write_word(0x104, (inf_bits >> 32) as u32).unwrap();

        exec_c_fld(0, 0, 0, &mut state, &mut mem).unwrap();
        assert_eq!(state.fpr.read(8).bits(), inf_bits);

        // Test negative zero
        let neg_zero_bits = (-0.0f64).to_bits();
        mem.write_word(0x108, neg_zero_bits as u32).unwrap();
        mem.write_word(0x10c, (neg_zero_bits >> 32) as u32).unwrap();

        exec_c_fld(1, 0, 8, &mut state, &mut mem).unwrap();
        assert_eq!(state.fpr.read(9).bits(), neg_zero_bits);
    }

    // C.FLW Tests
    #[test]
    fn test_c_flw_basic() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[8] = 0x100;

        // Write a float value to memory
        let test_value: f32 = std::f32::consts::PI;
        mem.write_word(0x100, test_value.to_bits()).unwrap();

        // Execute C.FLW f8, 0(x8)
        // rd'=0 (f8), rs1'=0 (x8), offset=0
        exec_c_flw(0, 0, 0, &mut state, &mut mem).unwrap();

        // Verify the value was loaded and NaN-boxed
        let loaded = state.fpr.read(8).get();
        assert!((loaded - test_value).abs() < 1e-5);
        assert!(state.fpr.read(8).is_nan_boxed());
    }

    #[test]
    fn test_c_flw_with_offset() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[9] = 0x100;

        // Write a float value to memory at offset 16
        let test_value: f32 = std::f32::consts::E;
        mem.write_word(0x110, test_value.to_bits()).unwrap();

        // Execute C.FLW f9, 16(x9)
        exec_c_flw(1, 1, 16, &mut state, &mut mem).unwrap();

        let loaded = state.fpr.read(9).get();
        assert!((loaded - test_value).abs() < 1e-5);
    }

    #[test]
    fn test_c_flw_misaligned() {
        let (mut state, mut mem) = setup_test();

        // Set up misaligned base address
        state.regs[8] = 0x101;

        // Should fail due to misalignment
        let result = exec_c_flw(0, 0, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_flw_negative_value() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x100;

        // Write a negative float value to memory
        let test_value: f32 = -123.456;
        mem.write_word(0x100, test_value.to_bits()).unwrap();

        exec_c_flw(0, 0, 0, &mut state, &mut mem).unwrap();

        let loaded = state.fpr.read(8).get();
        assert!((loaded - test_value).abs() < 1e-3);
    }

    // C.FSD Tests
    #[test]
    fn test_c_fsd_basic() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[8] = 0x100; // base
        let test_value: f64 = std::f64::consts::PI;
        state.fpr.write(9, Fpr::from_bits(test_value.to_bits())); // value to store

        // Execute C.FSD f9, 0(x8)
        // rs2'=1 (f9), rs1'=0 (x8), offset=0
        exec_c_fsd(0, 1, 0, &mut state, &mut mem).unwrap();

        // Verify memory
        let low = mem.read_word(0x100).unwrap();
        let high = mem.read_word(0x104).unwrap();
        let stored_bits = ((high as u64) << 32) | (low as u64);
        assert_eq!(stored_bits, test_value.to_bits());
    }

    #[test]
    fn test_c_fsd_with_offset() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[9] = 0x100; // base
        let test_value: f64 = std::f64::consts::E;
        state.fpr.write(10, Fpr::from_bits(test_value.to_bits())); // value to store

        // Execute C.FSD f10, 24(x9)
        exec_c_fsd(1, 2, 24, &mut state, &mut mem).unwrap();

        // Verify memory at 0x100 + 24 = 0x118
        let low = mem.read_word(0x118).unwrap();
        let high = mem.read_word(0x11c).unwrap();
        let stored_bits = ((high as u64) << 32) | (low as u64);
        assert_eq!(stored_bits, test_value.to_bits());
    }

    #[test]
    fn test_c_fsd_misaligned() {
        let (mut state, mut mem) = setup_test();

        // Set up misaligned base address
        state.regs[8] = 0x101;

        // Should fail due to misalignment
        let result = exec_c_fsd(0, 1, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    // C.FSW Tests
    #[test]
    fn test_c_fsw_basic() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[8] = 0x100; // base
        let test_value: f32 = std::f32::consts::PI;
        state.fpr.write(9, Fpr::new(test_value)); // value to store (NaN-boxed)

        // Execute C.FSW f9, 0(x8)
        exec_c_fsw(0, 1, 0, &mut state, &mut mem).unwrap();

        // Verify memory
        let stored = mem.read_word(0x100).unwrap();
        assert_eq!(stored, test_value.to_bits());
    }

    #[test]
    fn test_c_fsw_with_offset() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[9] = 0x100; // base
        let test_value: f32 = std::f32::consts::E;
        state.fpr.write(10, Fpr::new(test_value));

        // Execute C.FSW f10, 20(x9)
        exec_c_fsw(1, 2, 20, &mut state, &mut mem).unwrap();

        // Verify memory at 0x100 + 20 = 0x114
        let stored = mem.read_word(0x114).unwrap();
        assert_eq!(stored, test_value.to_bits());
    }

    #[test]
    fn test_c_fsw_misaligned() {
        let (mut state, mut mem) = setup_test();

        // Set up misaligned base address
        state.regs[8] = 0x101;

        // Should fail due to misalignment
        let result = exec_c_fsw(0, 1, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    // Roundtrip Tests
    #[test]
    fn test_c_fld_fsd_roundtrip() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[8] = 0x100;

        // Write a double value to FPR
        let test_value: f64 = std::f64::consts::PI;
        state.fpr.write(9, Fpr::from_bits(test_value.to_bits()));

        // Store to memory using C.FSD
        exec_c_fsd(0, 1, 0, &mut state, &mut mem).unwrap();

        // Load from memory using C.FLD into different register
        exec_c_fld(2, 0, 0, &mut state, &mut mem).unwrap();

        // Verify the roundtrip preserved the value
        let loaded_bits = state.fpr.read(10).bits(); // rd'=2 -> f10
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_c_flw_fsw_roundtrip() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[8] = 0x100;

        // Write a float value to FPR
        let test_value: f32 = std::f32::consts::PI;
        state.fpr.write(9, Fpr::new(test_value));

        // Store to memory using C.FSW
        exec_c_fsw(0, 1, 0, &mut state, &mut mem).unwrap();

        // Load from memory using C.FLW into different register
        exec_c_flw(2, 0, 0, &mut state, &mut mem).unwrap();

        // Verify the roundtrip preserved the value
        let loaded = state.fpr.read(10).get(); // rd'=2 -> f10
        assert!((loaded - test_value).abs() < 1e-5);
    }

    // Floating-point Value Preservation Tests
    #[test]
    fn test_floating_point_values_preserved() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x100;

        // Test various special floating-point values
        let test_values: Vec<f64> = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            std::f64::consts::PI,
            std::f64::consts::E,
        ];

        for (i, value) in test_values.iter().enumerate() {
            let offset = i * 8;
            state.fpr.write(9, Fpr::from_bits(value.to_bits()));
            exec_c_fsd(0, 1, offset as u32, &mut state, &mut mem).unwrap();

            // Clear the register
            state.fpr.write(10, Fpr::default());

            // Load back
            exec_c_fld(2, 0, offset as u32, &mut state, &mut mem).unwrap();

            let loaded_bits = state.fpr.read(10).bits();
            assert_eq!(
                loaded_bits,
                value.to_bits(),
                "Value {} (index {}) was not preserved correctly",
                value,
                i
            );
        }
    }

    // Register Index Tests
    #[test]
    fn test_register_index_mapping() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x100; // x8 - base

        // Test all compressed register indices (0-7 -> f8-f15)
        for i in 0..8u8 {
            let test_value: f32 = 1.0 + i as f32;
            state.fpr.write((i + 8) as usize, Fpr::new(test_value));

            // Store using C.FSW
            exec_c_fsw(0, i, (i * 4) as u32, &mut state, &mut mem).unwrap();

            // Verify stored value
            let stored = mem.read_word(0x100 + (i * 4) as u64).unwrap();
            assert_eq!(stored, test_value.to_bits(), "Register index {} failed", i);
        }
    }

    // Maximum Offset Tests
    #[test]
    fn test_maximum_offsets() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x100;

        // C.FLD/C.FSD maximum offset: 248 (31 * 8)
        let max_dbl_offset: u32 = 248;
        let test_dbl: f64 = 1.23456789;
        state.fpr.write(9, Fpr::from_bits(test_dbl.to_bits()));
        exec_c_fsd(0, 1, max_dbl_offset, &mut state, &mut mem).unwrap();
        exec_c_fld(2, 0, max_dbl_offset, &mut state, &mut mem).unwrap();
        assert_eq!(state.fpr.read(10).bits(), test_dbl.to_bits());

        // C.FLW/C.FSW maximum offset: 124 (31 * 4)
        let max_flt_offset: u32 = 124;
        let test_flt: f32 = 9.876543;
        state.fpr.write(9, Fpr::new(test_flt));
        exec_c_fsw(0, 1, max_flt_offset, &mut state, &mut mem).unwrap();
        exec_c_flw(2, 0, max_flt_offset, &mut state, &mut mem).unwrap();
        assert!((state.fpr.read(10).get() - test_flt).abs() < 1e-5);
    }
}
