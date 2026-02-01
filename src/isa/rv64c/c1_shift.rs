//! RV64C C1 Quadrant Shift Instructions
//!
//! This module implements the execution of C1 quadrant compressed shift
//! immediate instructions: C.SRLI and C.SRAI.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.SRLI - Shift right logical immediate (compressed)
///
/// Expands to: `srli rd', rd', shamt[5:0]`
///
/// Performs a logical right shift of the value in register `rd'` by the
/// immediate value `shamt`. The vacated bits are filled with zeros.
///
/// # Arguments
/// * `rd` - Destination and source register (compressed, 8-15)
/// * `shamt` - Shift amount (0-63 for RV64)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SRLI x8, 4  (x8 = x8 >> 4, logical)
/// // Expands to: srli x8, x8, 4
/// ```
pub fn exec_c_srli(rd: u8, shamt: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    // For RV64, shamt can be 0-63
    if shamt > 63 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize];
    let result = rs1_val >> shamt;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.SRAI - Shift right arithmetic immediate (compressed)
///
/// Expands to: `srai rd', rd', shamt[5:0]`
///
/// Performs an arithmetic right shift of the value in register `rd'` by the
/// immediate value `shamt`. The vacated bits are filled with the sign bit (bit 63).
///
/// # Arguments
/// * `rd` - Destination and source register (compressed, 8-15)
/// * `shamt` - Shift amount (0-63 for RV64)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SRAI x8, 4  (x8 = x8 >> 4, arithmetic)
/// // Expands to: srai x8, x8, 4
/// ```
pub fn exec_c_srai(rd: u8, shamt: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    // For RV64, shamt can be 0-63
    if shamt > 63 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize] as i64;
    let result = (rs1_val >> shamt) as u64;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.SLLI - Shift left logical immediate (compressed, C2 quadrant)
///
/// Expands to: `slli rd, rd, shamt[5:0]`
///
/// Performs a logical left shift of the value in register `rd` by the
/// immediate value `shamt`. The vacated bits are filled with zeros.
///
/// # Arguments
/// * `rd` - Destination and source register (0-31, but 0 is reserved)
/// * `shamt` - Shift amount (0-63 for RV64)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SLLI x5, 4  (x5 = x5 << 4)
/// // Expands to: slli x5, x5, 4
/// ```
pub fn exec_c_slli(rd: u8, shamt: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    // For RV64, shamt can be 0-63
    if shamt > 63 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize];
    let result = rs1_val << shamt;
    state.regs[rd as usize] = result;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;

    fn setup_test() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_c_srli() {
        let mut state = setup_test();

        state.regs[8] = 0xF0F0F0F0F0F0F0F0;

        // C.SRLI x8, 4
        exec_c_srli(8, 4, &mut state).unwrap();

        // Logical right shift fills with zeros
        assert_eq!(state.regs[8], 0x0F0F0F0F0F0F0F0F);
    }

    #[test]
    fn test_c_srli_zero_fill() {
        let mut state = setup_test();

        // Value with high bit set
        state.regs[8] = 0x8000000000000000;

        // C.SRLI x8, 1
        exec_c_srli(8, 1, &mut state).unwrap();

        // Logical shift fills with zero
        assert_eq!(state.regs[8], 0x4000000000000000);
    }

    #[test]
    fn test_c_srli_by_zero() {
        let mut state = setup_test();

        state.regs[8] = 0x123456789ABCDEF0;

        // C.SRLI x8, 0
        exec_c_srli(8, 0, &mut state).unwrap();

        // No change
        assert_eq!(state.regs[8], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_srli_by_63() {
        let mut state = setup_test();

        state.regs[8] = 0x8000000000000000;

        // C.SRLI x8, 63
        exec_c_srli(8, 63, &mut state).unwrap();

        // Only LSB should be 1
        assert_eq!(state.regs[8], 1);
    }

    #[test]
    fn test_c_srli_invalid_shamt() {
        let mut state = setup_test();

        state.regs[8] = 0x123456789ABCDEF0;

        // C.SRLI with shamt > 63 should fail
        let result = exec_c_srli(8, 64, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_srai() {
        let mut state = setup_test();

        // Positive value
        state.regs[8] = 0x0F0F0F0F0F0F0F0F;

        // C.SRAI x8, 4
        exec_c_srai(8, 4, &mut state).unwrap();

        // Arithmetic right shift of positive is same as logical
        assert_eq!(state.regs[8], 0x00F0F0F0F0F0F0F0);
    }

    #[test]
    fn test_c_srai_sign_extend() {
        let mut state = setup_test();

        // Negative value (sign bit set)
        state.regs[8] = 0xF0F0F0F0F0F0F0F0;

        // C.SRAI x8, 4
        exec_c_srai(8, 4, &mut state).unwrap();

        // Arithmetic shift fills with sign bit (1)
        assert_eq!(state.regs[8], 0xFF0F0F0F0F0F0F0F);
    }

    #[test]
    fn test_c_srai_negative_value() {
        let mut state = setup_test();

        // Negative value
        state.regs[8] = 0xFFFFFFFFFFFFFFFF; // -1

        // C.SRAI x8, 1
        exec_c_srai(8, 1, &mut state).unwrap();

        // Still -1 (sign-extended)
        assert_eq!(state.regs[8], 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_c_srai_by_zero() {
        let mut state = setup_test();

        state.regs[8] = 0x123456789ABCDEF0;

        // C.SRAI x8, 0
        exec_c_srai(8, 0, &mut state).unwrap();

        // No change
        assert_eq!(state.regs[8], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_srai_invalid_shamt() {
        let mut state = setup_test();

        state.regs[8] = 0x123456789ABCDEF0;

        // C.SRAI with shamt > 63 should fail
        let result = exec_c_srai(8, 64, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_slli() {
        let mut state = setup_test();

        state.regs[5] = 0x00000000000000FF;

        // C.SLLI x5, 8
        exec_c_slli(5, 8, &mut state).unwrap();

        assert_eq!(state.regs[5], 0x000000000000FF00);
    }

    #[test]
    fn test_c_slli_overflow() {
        let mut state = setup_test();

        // Value that will overflow
        state.regs[5] = 0xFF00000000000000;

        // C.SLLI x5, 8
        exec_c_slli(5, 8, &mut state).unwrap();

        // Bits shifted out are lost
        assert_eq!(state.regs[5], 0x0000000000000000);
    }

    #[test]
    fn test_c_slli_by_zero() {
        let mut state = setup_test();

        state.regs[5] = 0x123456789ABCDEF0;

        // C.SLLI x5, 0
        exec_c_slli(5, 0, &mut state).unwrap();

        // No change
        assert_eq!(state.regs[5], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_slli_by_63() {
        let mut state = setup_test();

        state.regs[5] = 0x0000000000000001;

        // C.SLLI x5, 63
        exec_c_slli(5, 63, &mut state).unwrap();

        // Only MSB should be set
        assert_eq!(state.regs[5], 0x8000000000000000);
    }

    #[test]
    fn test_c_slli_x0_fails() {
        let mut state = setup_test();

        // C.SLLI x0, 1 should fail
        let result = exec_c_slli(0, 1, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_slli_invalid_shamt() {
        let mut state = setup_test();

        state.regs[5] = 0x123456789ABCDEF0;

        // C.SLLI with shamt > 63 should fail
        let result = exec_c_slli(5, 64, &mut state);
        assert!(result.is_err());
    }
}
