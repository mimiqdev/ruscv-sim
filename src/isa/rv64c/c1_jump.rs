//! RV64C C1 Quadrant Jump Instruction
//!
//! This module implements the execution of C1 quadrant compressed jump
//! instruction: C.J (unconditional jump).

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.J - Unconditional jump (compressed)
///
/// Expands to: `jal x0, offset`
///
/// Performs an unconditional jump to the target address. The target address
/// is calculated by adding the sign-extended 12-bit offset to the current PC.
/// The offset is shifted left by 1 (halfword aligned).
///
/// Unlike C.JAL, this instruction does not save the return address.
///
/// # Arguments
/// * `offset` - 12-bit signed offset (jump target, halfword aligned)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.J 200  (PC = PC + 200)
/// // Expands to: jal x0, 200
/// ```
pub fn exec_c_j(offset: i32, state: &mut CoreState) -> Result<(), ExecuteError> {
    // Calculate target address
    let target = state.pc.wrapping_add(offset as u64);

    // Check alignment (must be aligned to 2 bytes for compressed instructions)
    if !target.is_multiple_of(2) {
        return Err(ExecuteError::MisalignedAccess(target, 2));
    }

    // Update PC to jump target
    state.pc = target;
    // Mark that a jump was taken (used by step() to skip pc += 4)
    state.branch_taken = true;
    Ok(())
}

/// Execute C.JAL - Jump and link (compressed, RV32 only)
///
/// Expands to: `jal x1, offset`
///
/// Performs an unconditional jump to the target address and saves the return
/// address (PC + 2) in register x1 (ra). The target address is calculated by
/// adding the sign-extended 12-bit offset to the current PC.
///
/// Note: In RV64, C.JAL is replaced by C.ADDIW, so this instruction is only
/// available in RV32. This implementation is provided for completeness.
///
/// # Arguments
/// * `offset` - 12-bit signed offset (jump target, halfword aligned)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.JAL 200  (ra = PC + 2, PC = PC + 200)
/// // Expands to: jal x1, 200
/// ```
#[allow(dead_code)]
pub fn exec_c_jal(offset: i32, state: &mut CoreState) -> Result<(), ExecuteError> {
    // Calculate target address
    let target = state.pc.wrapping_add(offset as u64);

    // Check alignment (must be aligned to 2 bytes for compressed instructions)
    if !target.is_multiple_of(2) {
        return Err(ExecuteError::MisalignedAccess(target, 2));
    }

    // Save return address (PC + 2 for compressed instruction)
    let return_addr = state.pc.wrapping_add(2);
    state.regs[1] = return_addr;

    // Update PC to jump target
    state.pc = target;
    // Mark that a jump was taken (used by step() to skip pc += 4)
    state.branch_taken = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_c_j_forward() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.J 256 (jump forward 256 bytes)
        exec_c_j(256, &mut state).unwrap();

        // PC should be updated to 0x200 (0x100 + 256)
        assert_eq!(state.pc, 0x200);
    }

    #[test]
    fn test_c_j_backward() {
        let mut state = setup_test();

        state.pc = 0x1000;

        // C.J -512 (jump backward 512 bytes)
        exec_c_j(-512, &mut state).unwrap();

        // PC should be updated to 0xE00 (0x1000 - 512)
        assert_eq!(state.pc, 0xE00);
    }

    #[test]
    fn test_c_j_small_offset() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.J 4 (jump forward 4 bytes)
        exec_c_j(4, &mut state).unwrap();

        // PC should be updated to 0x104
        assert_eq!(state.pc, 0x104);
    }

    #[test]
    fn test_c_j_zero_offset() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.J 0 (infinite loop to self)
        exec_c_j(0, &mut state).unwrap();

        // PC should remain at 0x100
        assert_eq!(state.pc, 0x100);
    }

    #[test]
    fn test_c_j_misaligned() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.J 1 (misaligned target)
        let result = exec_c_j(1, &mut state);

        // Should fail due to misalignment
        assert!(result.is_err());
    }

    #[test]
    fn test_c_j_large_forward() {
        let mut state = setup_test();

        state.pc = 0x1000;

        // C.J 2046 (maximum positive offset for 11-bit signed immediate, times 2)
        exec_c_j(2046, &mut state).unwrap();

        // PC should be updated
        assert_eq!(state.pc, 0x17FE);
    }

    #[test]
    fn test_c_j_large_backward() {
        let mut state = setup_test();

        state.pc = 0x1000;

        // C.J -2048 (maximum negative offset)
        exec_c_j(-2048, &mut state).unwrap();

        // PC should be updated
        assert_eq!(state.pc, 0x800);
    }

    #[test]
    fn test_c_jal_forward() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.JAL 128 (jump forward and link)
        exec_c_jal(128, &mut state).unwrap();

        // PC should be updated to 0x180 (0x100 + 128)
        assert_eq!(state.pc, 0x180);
        // Return address should be saved (PC + 2)
        assert_eq!(state.regs[1], 0x102);
    }

    #[test]
    fn test_c_jal_backward() {
        let mut state = setup_test();

        state.pc = 0x1000;

        // C.JAL -256 (jump backward and link)
        exec_c_jal(-256, &mut state).unwrap();

        // PC should be updated to 0xF00 (0x1000 - 256)
        assert_eq!(state.pc, 0xF00);
        // Return address should be saved (PC + 2)
        assert_eq!(state.regs[1], 0x1002);
    }

    #[test]
    fn test_c_jal_misaligned() {
        let mut state = setup_test();

        state.pc = 0x100;

        // C.JAL 3 (misaligned target)
        let result = exec_c_jal(3, &mut state);

        // Should fail due to misalignment
        assert!(result.is_err());
    }

    #[test]
    fn test_c_j_function_call_simulation() {
        // Simulate a function call pattern using C.J and C.JAL
        let mut state = setup_test();

        // Main program at 0x100
        state.pc = 0x100;

        // Call function at 0x200 using C.JAL
        exec_c_jal(0x100, &mut state).unwrap(); // Jump to 0x200, ra = 0x102
        assert_eq!(state.pc, 0x200);
        assert_eq!(state.regs[1], 0x102);

        // Function returns using C.JR (implemented in c2_move.rs)
        // For this test, we'll simulate it manually
        state.pc = state.regs[1];
        assert_eq!(state.pc, 0x102);
    }

    #[test]
    fn test_c_j_loop_simulation() {
        // Simulate a simple infinite loop using C.J
        let mut state = setup_test();

        state.pc = 0x100; // Loop start

        // Loop body...
        // C.J -4 (jump back to start of loop)
        exec_c_j(-4, &mut state).unwrap();
        assert_eq!(state.pc, 0xFC);
    }
}
