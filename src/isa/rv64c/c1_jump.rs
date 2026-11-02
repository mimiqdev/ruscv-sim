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
/// ```text
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
