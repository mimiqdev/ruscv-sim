//! RV64C C2 Quadrant Move and Jump Instructions
//!
//! This module implements the execution of C2 quadrant compressed instructions
//! for register moves, jumps, and system operations.

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// Execute C.JR - Jump register (compressed)
///
/// Expands to: `jalr x0, 0(rs1)`
///
/// Performs an unconditional jump to the address in register `rs1`.
/// Does not save the return address.
///
/// # Arguments
/// * `rs1` - Source register containing jump address (0-31, but 0 is reserved)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.JR x5  (Jump to address in x5)
/// // Expands to: jalr x0, 0(x5)
/// ```
pub fn exec_c_jr(rs1: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rs1 == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let target = state.regs[rs1 as usize];

    // Set PC to target address (must be aligned to 2 bytes)
    if target % 2 != 0 {
        return Err(ExecuteError::MisalignedAccess(target, 2));
    }

    state.pc = target;

    Ok(())
}

/// Execute C.JALR - Jump and link register (compressed)
///
/// Expands to: `jalr x1, 0(rs1)`
///
/// Performs an unconditional jump to the address in register `rs1`.
/// Saves the return address (PC + 2) in register x1 (ra).
///
/// # Arguments
/// * `rs1` - Source register containing jump address (0-31, but 0 is reserved)
/// * `state` - Core state
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.JALR x5  (Jump to address in x5, save return address in ra)
/// // Expands to: jalr x1, 0(x5)
/// ```
pub fn exec_c_jalr(rs1: u8, state: &mut CoreState) -> Result<(), ExecuteError> {
    if rs1 == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let target = state.regs[rs1 as usize];

    // Check alignment (must be aligned to 2 bytes)
    if target % 2 != 0 {
        return Err(ExecuteError::MisalignedAccess(target, 2));
    }

    // Save return address (PC + 2 for compressed instruction)
    let return_addr = state.pc.wrapping_add(2);
    state.regs[1] = return_addr;

    // Jump to target
    state.pc = target;

    Ok(())
}

/// Execute C.EBREAK - Environment breakpoint (compressed)
///
/// Expands to: `ebreak`
///
/// Generates a breakpoint exception, transferring control to the debugger.
///
/// # Arguments
/// * `state` - Core state
///
/// # Returns
/// Always returns ExecuteError::Ebreak
///
/// # Example
/// ```
/// // C.EBREAK
/// // Expands to: ebreak
/// ```
pub fn exec_c_ebreak(_state: &mut CoreState) -> Result<(), ExecuteError> {
    Err(ExecuteError::Ebreak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;

    fn setup_test() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_c_jr() {
        let mut state = setup_test();

        // Set up jump target in x5
        state.regs[5] = 0x1000;
        state.pc = 0x100; // Current PC

        // C.JR x5
        exec_c_jr(5, &mut state).unwrap();

        // PC should be updated to target
        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_c_jr_x0_fails() {
        let mut state = setup_test();

        // C.JR x0 should fail
        let result = exec_c_jr(0, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_jr_misaligned() {
        let mut state = setup_test();

        // Set up misaligned jump target in x5
        state.regs[5] = 0x1001;

        // C.JR x5 should fail due to misalignment
        let result = exec_c_jr(5, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_jalr() {
        let mut state = setup_test();

        // Set up jump target in x5
        state.regs[5] = 0x2000;
        state.pc = 0x100; // Current PC

        // C.JALR x5
        exec_c_jalr(5, &mut state).unwrap();

        // PC should be updated to target
        assert_eq!(state.pc, 0x2000);
        // Return address should be saved in x1 (ra)
        assert_eq!(state.regs[1], 0x102); // PC + 2
    }

    #[test]
    fn test_c_jalr_x0_fails() {
        let mut state = setup_test();

        // C.JALR x0 should fail
        let result = exec_c_jalr(0, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_jalr_saves_return_address() {
        let mut state = setup_test();

        // Set up jump target
        state.regs[10] = 0x3000;
        state.pc = 0x500;

        // C.JALR x10
        exec_c_jalr(10, &mut state).unwrap();

        // Check return address (PC + 2)
        assert_eq!(state.regs[1], 0x502);
    }

    #[test]
    fn test_c_ebreak() {
        let mut state = setup_test();

        // C.EBREAK should return Ebreak error
        let result = exec_c_ebreak(&mut state);
        assert!(matches!(result, Err(ExecuteError::Ebreak)));
    }
}
