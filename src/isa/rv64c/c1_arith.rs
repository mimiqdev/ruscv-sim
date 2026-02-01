//! RV64C C1 Quadrant Arithmetic Instructions
//!
//! This module implements the execution of C1 quadrant compressed arithmetic
//! and logical instructions including register-immediate and register-register operations.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.ADDI - Add immediate (compressed)
///
/// Expands to: `addi rd, rd, nzimm[5:0]`
///
/// Adds the sign-extended 6-bit immediate to the value in register `rd`
/// and writes the result to `rd`. The immediate can be zero (used for hints).
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
/// // C.ADDI x5, 4  (x5 = x5 + 4)
/// // Expands to: addi x5, x5, 4
/// ```
pub fn exec_c_addi(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize];
    // Sign-extend 6-bit immediate to 64-bit
    let imm_masked = imm & 0x3F; // Keep only lower 6 bits
    let imm_sext = ((imm_masked as i64) << 58) >> 58; // Sign extend from bit 5
    let result = rs1_val.wrapping_add(imm_sext as u64);
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.LI - Load immediate
///
/// Expands to: `addi rd, x0, imm[5:0]`
///
/// Loads the sign-extended 6-bit immediate into register `rd`.
///
/// # Arguments
/// * `rd` - Destination register (0-31, but 0 is reserved)
/// * `imm` - 6-bit signed immediate
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.LI x5, -1  (x5 = -1)
/// // Expands to: addi x5, x0, -1
/// ```
pub fn exec_c_li(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    // Sign-extend 6-bit immediate to 64-bit
    let imm_masked = imm & 0x3F; // Keep only lower 6 bits
    let imm_sext = ((imm_masked as i64) << 58) >> 58; // Sign extend from bit 5
    state.regs[rd as usize] = imm_sext as u64;

    Ok(())
}

/// Execute C.LUI - Load upper immediate
///
/// Expands to: `lui rd, nzimm[17:12]`
///
/// Loads the non-zero immediate into bits 17-12 of register `rd`,
/// clearing the lower 12 bits. The immediate is shifted left by 12 bits.
///
/// # Arguments
/// * `rd` - Destination register (0-31, but 0 and 2 are reserved)
/// * `imm` - 18-bit signed immediate (upper 17 bits)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.LUI x5, 0x12345  (x5 = 0x12345000)
/// // Expands to: lui x5, 0x12345
/// ```
pub fn exec_c_lui(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 || rd == 2 {
        return Err(ExecuteError::InvalidOperation);
    }

    let imm_sext = ((imm << 12) as i32) as i64 as u64;
    state.regs[rd as usize] = imm_sext;

    Ok(())
}

/// Execute C.ADDI16SP - Add immediate to stack pointer (scaled by 16)
///
/// Expands to: `addi x2, x2, nzimm[9:4]`
///
/// Adds the sign-extended 6-bit immediate, scaled by 16, to the stack
/// pointer (x2). This is used for adjusting the stack pointer in function
/// prologues and epilogues.
///
/// # Arguments
/// * `imm` - 6-bit signed immediate (will be multiplied by 16)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ADDI16SP -32  (SP = SP - 32)
/// // Expands to: addi x2, x2, -32
/// ```
pub fn exec_c_addi16sp(imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    // Scale the immediate by 16
    // Sign-extend 6-bit immediate to 64-bit, then scale by 16
    let imm_masked = imm & 0x3F; // Keep only lower 6 bits
    let imm_sext = ((imm_masked as i64) << 58) >> 58; // Sign extend from bit 5
    let scaled_imm = (imm_sext << 4) as u64;

    let sp_val = state.regs[2];
    let result = sp_val.wrapping_add(scaled_imm);
    state.regs[2] = result;

    Ok(())
}

/// Execute C.ANDI - AND immediate
///
/// Expands to: `andi rd', rd', imm[5:0]`
///
/// Performs bitwise AND between the value in register `rd'` and the
/// sign-extended 6-bit immediate.
///
/// # Arguments
/// * `rd` - Destination and source register (compressed, 8-15)
/// * `imm` - 6-bit signed immediate
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ANDI x8, 0xF  (x8 = x8 & 0xF)
/// // Expands to: andi x8, x8, 0xF
/// ```
pub fn exec_c_andi(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize];
    let imm_sext = (imm as i32) as i64 as u64;
    let result = rs1_val & imm_sext;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.MV - Move register
///
/// Expands to: `add rd, x0, rs2`
///
/// Copies the value from register `rs2` to register `rd`.
///
/// # Arguments
/// * `rd` - Destination register (0-31, but 0 is reserved)
/// * `rs2` - Source register (0-31)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.MV x5, x6  (x5 = x6)
/// // Expands to: add x5, x0, x6
/// ```
pub fn exec_c_mv(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs2_val = state.regs[rs2 as usize];
    state.regs[rd as usize] = rs2_val;

    Ok(())
}

/// Execute C.ADD - Add registers
///
/// Expands to: `add rd, rd, rs2`
///
/// Adds the value in register `rs2` to the value in register `rd`
/// and writes the result to `rd`.
///
/// # Arguments
/// * `rd` - Destination and first source register (0-31, but 0 is reserved)
/// * `rs2` - Second source register (0-31)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ADD x5, x6  (x5 = x5 + x6)
/// // Expands to: add x5, x5, x6
/// ```
pub fn exec_c_add(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let rs1_val = state.regs[rd as usize];
    let rs2_val = state.regs[rs2 as usize];
    let result = rs1_val.wrapping_add(rs2_val);
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.SUB - Subtract registers (compressed)
///
/// Expands to: `sub rd', rd', rs2'`
///
/// Subtracts the value in register `rs2'` from the value in register `rd'`
/// and writes the result to `rd'`.
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
/// // C.SUB x8, x9  (x8 = x8 - x9)
/// // Expands to: sub x8, x8, x9
/// ```
pub fn exec_c_sub(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize];
    let rs2_val = state.regs[rs2 as usize];
    let result = rs1_val.wrapping_sub(rs2_val);
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.XOR - XOR registers (compressed)
///
/// Expands to: `xor rd', rd', rs2'`
///
/// Performs bitwise XOR between the values in registers `rd'` and `rs2'`
/// and writes the result to `rd'`.
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
/// // C.XOR x8, x9  (x8 = x8 ^ x9)
/// // Expands to: xor x8, x8, x9
/// ```
pub fn exec_c_xor(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize];
    let rs2_val = state.regs[rs2 as usize];
    let result = rs1_val ^ rs2_val;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.OR - OR registers (compressed)
///
/// Expands to: `or rd', rd', rs2'`
///
/// Performs bitwise OR between the values in registers `rd'` and `rs2'`
/// and writes the result to `rd'`.
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
/// // C.OR x8, x9  (x8 = x8 | x9)
/// // Expands to: or x8, x8, x9
/// ```
pub fn exec_c_or(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize];
    let rs2_val = state.regs[rs2 as usize];
    let result = rs1_val | rs2_val;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.AND - AND registers (compressed)
///
/// Expands to: `and rd', rd', rs2'`
///
/// Performs bitwise AND between the values in registers `rd'` and `rs2'`
/// and writes the result to `rd'`.
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
/// // C.AND x8, x9  (x8 = x8 & x9)
/// // Expands to: and x8, x8, x9
/// ```
pub fn exec_c_and(rd: u8, rs2: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rd as usize];
    let rs2_val = state.regs[rs2 as usize];
    let result = rs1_val & rs2_val;
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
    fn test_c_addi() {
        let mut state = setup_test();

        // Set up initial value
        state.regs[5] = 10;

        // C.ADDI x5, 5
        exec_c_addi(5, 5, &mut state).unwrap();

        assert_eq!(state.regs[5], 15);
    }

    #[test]
    fn test_c_addi_negative() {
        let mut state = setup_test();

        // Set up initial value
        state.regs[5] = 10;

        // C.ADDI x5, -1 (encoded as 0x3F = 63 unsigned, -1 signed 6-bit)
        exec_c_addi(5, 0x3F, &mut state).unwrap();

        assert_eq!(state.regs[5], 9);
    }

    #[test]
    fn test_c_addi_x0_fails() {
        let mut state = setup_test();

        // C.ADDI x0, 1 should fail
        let result = exec_c_addi(0, 1, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_li() {
        let mut state = setup_test();

        // C.LI x5, 31 (max positive value for 6-bit signed immediate)
        exec_c_li(5, 31, &mut state).unwrap();

        assert_eq!(state.regs[5], 31);
    }

    #[test]
    fn test_c_li_negative() {
        let mut state = setup_test();

        // C.LI x5, -1
        exec_c_li(5, 0x3F, &mut state).unwrap();

        assert_eq!(state.regs[5], 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_c_lui() {
        let mut state = setup_test();

        // C.LUI x5, 0x12345
        exec_c_lui(5, 0x12345, &mut state).unwrap();

        assert_eq!(state.regs[5], 0x0000000012345000);
    }

    #[test]
    fn test_c_addi16sp() {
        let mut state = setup_test();

        // Set SP
        state.regs[2] = 0x1000;

        // C.ADDI16SP -32 (encoded with appropriate immediate)
        // -32 / 16 = -2, encoded as 0x3E (62 unsigned for -2 in 6-bit signed)
        exec_c_addi16sp(0x3E, &mut state).unwrap();

        assert_eq!(state.regs[2], 0xFE0); // 0x1000 - 32 = 0xFE0 = 4064
    }

    #[test]
    fn test_c_andi() {
        let mut state = setup_test();

        state.regs[8] = 0xFF;

        // C.ANDI x8, 0x0F
        exec_c_andi(8, 0x0F, &mut state).unwrap();

        assert_eq!(state.regs[8], 0x0F);
    }

    #[test]
    fn test_c_mv() {
        let mut state = setup_test();

        state.regs[6] = 0x12345678;

        // C.MV x5, x6
        exec_c_mv(5, 6, &mut state).unwrap();

        assert_eq!(state.regs[5], 0x12345678);
    }

    #[test]
    fn test_c_add() {
        let mut state = setup_test();

        state.regs[5] = 10;
        state.regs[6] = 20;

        // C.ADD x5, x6
        exec_c_add(5, 6, &mut state).unwrap();

        assert_eq!(state.regs[5], 30);
    }

    #[test]
    fn test_c_sub() {
        let mut state = setup_test();

        state.regs[8] = 100;
        state.regs[9] = 30;

        // C.SUB x8, x9
        exec_c_sub(8, 9, &mut state).unwrap();

        assert_eq!(state.regs[8], 70);
    }

    #[test]
    fn test_c_xor() {
        let mut state = setup_test();

        state.regs[8] = 0xFF00;
        state.regs[9] = 0x0F0F;

        // C.XOR x8, x9
        exec_c_xor(8, 9, &mut state).unwrap();

        assert_eq!(state.regs[8], 0xF00F);
    }

    #[test]
    fn test_c_or() {
        let mut state = setup_test();

        state.regs[8] = 0xFF00;
        state.regs[9] = 0x00FF;

        // C.OR x8, x9
        exec_c_or(8, 9, &mut state).unwrap();

        assert_eq!(state.regs[8], 0xFFFF);
    }

    #[test]
    fn test_c_and() {
        let mut state = setup_test();

        state.regs[8] = 0xFF00;
        state.regs[9] = 0x0F0F;

        // C.AND x8, x9
        exec_c_and(8, 9, &mut state).unwrap();

        assert_eq!(state.regs[8], 0x0F00);
    }
}
