//! RV64C C1 Quadrant Word Instructions (RV64 only)
//!
//! This module implements the execution of C1 quadrant compressed word
//! arithmetic instructions that are only available in RV64.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.ADDIW - Add immediate word (RV64)
///
/// Expands to: `addiw rd, rd, imm[5:0]`
///
/// Adds the sign-extended 6-bit immediate to the lower 32 bits of register `rd`,
/// produces a 32-bit result, then sign-extends it to 64 bits.
///
/// # Arguments
/// * `rd` - Destination and source register (0-31, but 0 is reserved)
/// * `imm` - 6-bit signed immediate
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ADDIW x5, 1  (x5[31:0] = x5[31:0] + 1, sign-extended)
/// // Expands to: addiw x5, x5, 1
/// ```
pub fn exec_c_addiw(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize] as u32;
    // Sign-extend 6-bit immediate to 32-bit
    let imm_masked = imm & 0x3F; // Keep only lower 6 bits
    let imm_sext = ((imm_masked as i32) << 26) >> 26; // Sign extend from bit 5
    let result_32 = rs1_val.wrapping_add(imm_sext as u32);

    // Sign-extend to 64 bits
    let result_64 = (result_32 as i32) as i64 as u64;
    state.regs[rd as usize] = result_64;

    Ok(())
}

/// Execute C.SUBW - Subtract word (RV64)
///
/// Expands to: `subw rd', rd', rs2'`
///
/// Subtracts the lower 32 bits of register `rs2'` from the lower 32 bits
/// of register `rd'`, produces a 32-bit result, then sign-extends it to 64 bits.
///
/// # Arguments
/// * `rd` - Destination and first source register (compressed, 8-15)
/// * `rs2` - Second source register (compressed, 8-15)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SUBW x8, x9  (x8[31:0] = x8[31:0] - x9[31:0], sign-extended)
/// // Expands to: subw x8, x8, x9
/// ```
pub fn exec_c_subw(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize] as u32;
    let rs2_val = state.regs[rs2 as usize] as u32;
    let result_32 = rs1_val.wrapping_sub(rs2_val);

    // Sign-extend to 64 bits
    let result_64 = (result_32 as i32) as i64 as u64;
    state.regs[rd as usize] = result_64;

    Ok(())
}

/// Execute C.ADDW - Add word (RV64)
///
/// Expands to: `addw rd', rd', rs2'`
///
/// Adds the lower 32 bits of register `rs2'` to the lower 32 bits of
/// register `rd'`, produces a 32-bit result, then sign-extends it to 64 bits.
///
/// # Arguments
/// * `rd` - Destination and first source register (compressed, 8-15)
/// * `rs2` - Second source register (compressed, 8-15)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ADDW x8, x9  (x8[31:0] = x8[31:0] + x9[31:0], sign-extended)
/// // Expands to: addw x8, x8, x9
/// ```
pub fn exec_c_addw(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize] as u32;
    let rs2_val = state.regs[rs2 as usize] as u32;
    let result_32 = rs1_val.wrapping_add(rs2_val);

    // Sign-extend to 64 bits
    let result_64 = (result_32 as i32) as i64 as u64;
    state.regs[rd as usize] = result_64;

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
    fn test_c_addiw() {
        let mut state = setup_test();

        // Set up 32-bit value
        state.regs[5] = 0x00000000_FFFFFFFF;

        // C.ADDIW x5, 1
        exec_c_addiw(5, 1, &mut state).unwrap();

        // Result should be 0 (0xFFFFFFFF + 1 = 0x100000000, truncated to 32 bits = 0)
        // Then sign-extended to 64 bits = 0
        assert_eq!(state.regs[5], 0);
    }

    #[test]
    fn test_c_addiw_negative() {
        let mut state = setup_test();

        // Set up 32-bit value
        state.regs[5] = 0;

        // C.ADDIW x5, -1 (0x3F = 63 unsigned = -1 in 6-bit signed)
        exec_c_addiw(5, 0x3F, &mut state).unwrap();

        // Result should be 0xFFFFFFFFFFFFFFFF (sign-extended -1)
        assert_eq!(state.regs[5], 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_c_addiw_sign_extend_positive() {
        let mut state = setup_test();

        // Set up value
        state.regs[5] = 0x00000000_7FFFFFFF;

        // C.ADDIW x5, 0 (should sign-extend)
        exec_c_addiw(5, 0, &mut state).unwrap();

        // Result should be 0x00000000_7FFFFFFF (positive, so upper bits are 0)
        assert_eq!(state.regs[5], 0x00000000_7FFFFFFF);
    }

    #[test]
    fn test_c_addiw_sign_extend_negative() {
        let mut state = setup_test();

        // Set up value with high bit set
        state.regs[5] = 0x00000000_80000000;

        // C.ADDIW x5, 0 (should sign-extend)
        exec_c_addiw(5, 0, &mut state).unwrap();

        // Result should be 0xFFFFFFFF_80000000 (negative, so upper bits are 1s)
        assert_eq!(state.regs[5], 0xFFFFFFFF80000000);
    }

    #[test]
    fn test_c_subw() {
        let mut state = setup_test();

        // Set up values
        state.regs[8] = 0x00000000_00000064; // 100
        state.regs[9] = 0x00000000_0000001E; // 30

        // C.SUBW x8, x9
        exec_c_subw(8, 9, &mut state).unwrap();

        // Result should be 70 (100 - 30), sign-extended
        assert_eq!(state.regs[8], 70);
    }

    #[test]
    fn test_c_subw_overflow() {
        let mut state = setup_test();

        // Set up values for overflow
        state.regs[8] = 0; // 0
        state.regs[9] = 1; // 1

        // C.SUBW x8, x9 (0 - 1 = -1, sign-extended)
        exec_c_subw(8, 9, &mut state).unwrap();

        // Result should be 0xFFFFFFFFFFFFFFFF (sign-extended -1)
        assert_eq!(state.regs[8], 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_c_addw() {
        let mut state = setup_test();

        // Set up values
        state.regs[8] = 0x00000000_0000000A; // 10
        state.regs[9] = 0x00000000_00000014; // 20

        // C.ADDW x8, x9
        exec_c_addw(8, 9, &mut state).unwrap();

        // Result should be 30, sign-extended
        assert_eq!(state.regs[8], 30);
    }

    #[test]
    fn test_c_addw_overflow() {
        let mut state = setup_test();

        // Set up values for overflow
        state.regs[8] = 0xFFFFFFFF; // 0xFFFFFFFF = -1 in 32-bit
        state.regs[9] = 0x00000001; // 1

        // C.ADDW x8, x9 (-1 + 1 = 0)
        exec_c_addw(8, 9, &mut state).unwrap();

        // Result should be 0
        assert_eq!(state.regs[8], 0);
    }

    #[test]
    fn test_c_addiw_x0_fails() {
        let mut state = setup_test();

        // C.ADDIW x0, 1 should fail
        let result = exec_c_addiw(0, 1, &mut state);
        assert!(result.is_err());
    }
}
