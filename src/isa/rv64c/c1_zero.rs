//! RV64C C1 Quadrant Zero/Hint Instructions
//!
//! This module implements the execution of C1 quadrant compressed instructions
//! that operate on x0 or serve as hints: C.NOP.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.NOP - No operation (compressed)
///
/// Expands to: `addi x0, x0, 0`
///
/// Performs no operation. This is a pseudo-instruction that is encoded as
/// C.ADDI x0, 0, which is a hint instruction that does not change any
/// architectural state.
///
/// C.NOP is useful for padding code or alignment purposes.
///
/// # Arguments
/// * `state` - Core state (unused, but kept for API consistency)
///
/// # Returns
/// Always returns Ok(())
///
/// # Example
/// ```
/// // C.NOP (no operation)
/// // Expands to: addi x0, x0, 0
/// ```
pub fn exec_c_nop(_state: &mut CoreState) -> Result<(), ExecuteError> {
    // No operation - x0 is hardwired to zero and cannot be modified
    Ok(())
}

/// Execute C.ADDI with rd=x0 (hint instruction)
///
/// When C.ADDI has rd=x0, it is defined as a hint instruction that does
/// not modify any architectural state. Different immediate values may
/// encode different hints in future extensions.
///
/// # Arguments
/// * `imm` - 6-bit immediate (may encode hint type)
/// * `state` - Core state (unused, but kept for API consistency)
///
/// # Returns
/// Always returns Ok(())
pub fn exec_c_addi_hint(imm: u32, _state: &mut CoreState) -> Result<(), ExecuteError> {
    // For now, all hints with rd=x0 are treated as NOPs
    // The immediate value is ignored but could be used for future hint extensions
    let _ = imm; // Explicitly ignore imm for now
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_c_nop() {
        let mut state = setup_test();

        // Set up some initial state
        state.pc = 0x100;
        state.regs[1] = 0x123456789ABCDEF0;

        // C.NOP should not change any state
        exec_c_nop(&mut state).unwrap();

        // Verify state is unchanged
        assert_eq!(state.pc, 0x100);
        assert_eq!(state.regs[1], 0x123456789ABCDEF0);
        assert_eq!(state.regs[0], 0); // x0 always zero
    }

    #[test]
    fn test_c_addi_hint_zero() {
        let mut state = setup_test();

        // C.ADDI x0, 0 is C.NOP (hint)
        exec_c_addi_hint(0, &mut state).unwrap();

        // State should be unchanged
        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_c_addi_hint_nonzero() {
        let mut state = setup_test();

        // C.ADDI x0, n (where n != 0) is a hint
        exec_c_addi_hint(0x1F, &mut state).unwrap();

        // State should be unchanged
        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_c_nop_sequence() {
        let mut state = setup_test();

        // Multiple NOPs should have no effect
        state.pc = 0x100;

        exec_c_nop(&mut state).unwrap();
        exec_c_nop(&mut state).unwrap();
        exec_c_nop(&mut state).unwrap();

        assert_eq!(state.pc, 0x100);
    }

    #[test]
    fn test_c_nop_with_register_operations() {
        let mut state = setup_test();

        // Set up initial values
        state.regs[5] = 100;
        state.regs[6] = 200;

        // Perform some operations
        state.regs[7] = state.regs[5] + state.regs[6]; // 300

        // NOP should not affect anything
        exec_c_nop(&mut state).unwrap();

        // Verify all values are preserved
        assert_eq!(state.regs[5], 100);
        assert_eq!(state.regs[6], 200);
        assert_eq!(state.regs[7], 300);
    }
}
