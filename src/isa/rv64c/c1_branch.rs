//! RV64C C1 Quadrant Branch Instructions
//!
//! This module implements the execution of C1 quadrant compressed branch
//! instructions: C.BEQZ and C.BNEZ.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.BEQZ - Branch if equal to zero (compressed)
///
/// Expands to: `beq rs1', x0, offset`
///
/// Branches to the target address if the value in register `rs1'` is equal to zero.
/// The target address is calculated by adding the sign-extended 9-bit offset
/// to the current PC. The offset is shifted left by 1 (halfword aligned).
///
/// # Arguments
/// * `rs1` - Source register to test (compressed, 8-15)
/// * `offset` - 9-bit signed offset (branch target, halfword aligned)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.BEQZ x8, 100  (if x8 == 0, PC = PC + 100)
/// // Expands to: beq x8, x0, 100
/// ```
pub fn exec_c_beqz(rs1: u8, offset: i32, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rs1 as usize];

    // Check if equal to zero
    if rs1_val == 0 {
        // Calculate target address
        let target = state.pc.wrapping_add(offset as u64);

        // Check alignment (must be aligned to 2 bytes for compressed instructions)
        if !target.is_multiple_of(2) {
            return Err(ExecuteError::MisalignedAccess(target, 2));
        }

        // Update PC to branch target
        state.pc = target;
    }

    Ok(())
}

/// Execute C.BNEZ - Branch if not equal to zero (compressed)
///
/// Expands to: `bne rs1', x0, offset`
///
/// Branches to the target address if the value in register `rs1'` is not equal
/// to zero. The target address is calculated by adding the sign-extended 9-bit
/// offset to the current PC. The offset is shifted left by 1 (halfword aligned).
///
/// # Arguments
/// * `rs1` - Source register to test (compressed, 8-15)
/// * `offset` - 9-bit signed offset (branch target, halfword aligned)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.BNEZ x8, 100  (if x8 != 0, PC = PC + 100)
/// // Expands to: bne x8, x0, 100
/// ```
pub fn exec_c_bnez(rs1: u8, offset: i32, state: &mut CoreState) -> Result<(), ExecuteError> {
    let rs1_val = state.regs[rs1 as usize];

    // Check if not equal to zero
    if rs1_val != 0 {
        // Calculate target address
        let target = state.pc.wrapping_add(offset as u64);

        // Check alignment (must be aligned to 2 bytes for compressed instructions)
        if !target.is_multiple_of(2) {
            return Err(ExecuteError::MisalignedAccess(target, 2));
        }

        // Update PC to branch target
        state.pc = target;
    }

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
    fn test_c_beqz_taken() {
        let mut state = setup_test();

        // Set up register with zero value
        state.regs[8] = 0;
        state.pc = 0x100;

        // C.BEQZ x8, 32 (branch forward 32 bytes)
        exec_c_beqz(8, 32, &mut state).unwrap();

        // PC should be updated to 0x120 (0x100 + 32)
        assert_eq!(state.pc, 0x120);
    }

    #[test]
    fn test_c_beqz_not_taken() {
        let mut state = setup_test();

        // Set up register with non-zero value
        state.regs[8] = 42;
        state.pc = 0x100;

        // C.BEQZ x8, 32 (branch should not be taken)
        exec_c_beqz(8, 32, &mut state).unwrap();

        // PC should remain unchanged
        assert_eq!(state.pc, 0x100);
    }

    #[test]
    fn test_c_beqz_negative_offset() {
        let mut state = setup_test();

        // Set up register with zero value
        state.regs[8] = 0;
        state.pc = 0x100;

        // C.BEQZ x8, -16 (branch backward 16 bytes)
        exec_c_beqz(8, -16, &mut state).unwrap();

        // PC should be updated to 0xF0 (0x100 - 16)
        assert_eq!(state.pc, 0xF0);
    }

    #[test]
    fn test_c_bnez_taken() {
        let mut state = setup_test();

        // Set up register with non-zero value
        state.regs[8] = 42;
        state.pc = 0x100;

        // C.BNEZ x8, 32 (branch forward 32 bytes)
        exec_c_bnez(8, 32, &mut state).unwrap();

        // PC should be updated to 0x120 (0x100 + 32)
        assert_eq!(state.pc, 0x120);
    }

    #[test]
    fn test_c_bnez_not_taken() {
        let mut state = setup_test();

        // Set up register with zero value
        state.regs[8] = 0;
        state.pc = 0x100;

        // C.BNEZ x8, 32 (branch should not be taken)
        exec_c_bnez(8, 32, &mut state).unwrap();

        // PC should remain unchanged
        assert_eq!(state.pc, 0x100);
    }

    #[test]
    fn test_c_bnez_negative_offset() {
        let mut state = setup_test();

        // Set up register with non-zero value
        state.regs[8] = 42;
        state.pc = 0x100;

        // C.BNEZ x8, -16 (branch backward 16 bytes)
        exec_c_bnez(8, -16, &mut state).unwrap();

        // PC should be updated to 0xF0 (0x100 - 16)
        assert_eq!(state.pc, 0xF0);
    }

    #[test]
    fn test_c_beqz_misaligned() {
        let mut state = setup_test();

        // Set up register with zero value
        state.regs[8] = 0;
        state.pc = 0x100;

        // C.BEQZ x8, 1 (misaligned target)
        let result = exec_c_beqz(8, 1, &mut state);

        // Should fail due to misalignment
        assert!(result.is_err());
    }

    #[test]
    fn test_c_bnez_misaligned() {
        let mut state = setup_test();

        // Set up register with non-zero value
        state.regs[8] = 42;
        state.pc = 0x100;

        // C.BNEZ x8, 1 (misaligned target)
        let result = exec_c_bnez(8, 1, &mut state);

        // Should fail due to misalignment
        assert!(result.is_err());
    }

    #[test]
    fn test_c_beqz_loop_simulation() {
        let mut state = setup_test();

        // Simulate a simple loop: while (x8 != 0) { x8--; }
        state.regs[8] = 3; // Loop counter
        state.pc = 0x100; // Loop start

        // First iteration - C.BEQZ not taken (x8 = 3)
        exec_c_beqz(8, 16, &mut state).unwrap(); // Skip 16 bytes if zero
        assert_eq!(state.pc, 0x100); // PC unchanged
        state.regs[8] = 2; // Decrement counter

        // Second iteration - C.BEQZ not taken (x8 = 2)
        exec_c_beqz(8, 16, &mut state).unwrap();
        assert_eq!(state.pc, 0x100);
        state.regs[8] = 1; // Decrement counter

        // Third iteration - C.BEQZ not taken (x8 = 1)
        exec_c_beqz(8, 16, &mut state).unwrap();
        assert_eq!(state.pc, 0x100);
        state.regs[8] = 0; // Decrement counter

        // Fourth iteration - C.BEQZ taken (x8 = 0)
        exec_c_beqz(8, 16, &mut state).unwrap();
        assert_eq!(state.pc, 0x110); // Branch taken, PC updated
    }

    #[test]
    fn test_c_bnez_forward_branch_simulation() {
        let mut state = setup_test();

        // Simulate: if (x8 != 0) goto label; else continue;
        state.regs[8] = 1; // Non-zero value
        state.pc = 0x100; // Current PC

        // C.BNEZ x8, 8 (branch forward if non-zero)
        exec_c_bnez(8, 8, &mut state).unwrap();

        // Branch should be taken
        assert_eq!(state.pc, 0x108);
    }
}
