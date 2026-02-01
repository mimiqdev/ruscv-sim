//! RV64C C2 Quadrant Stack and Shift Instructions
//!
//! This module implements the execution of C2 quadrant compressed instructions
//! for stack pointer relative memory access and shift operations.

use crate::core::CoreState;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

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

/// Execute C.LWSP - Load word from stack pointer (compressed)
///
/// Expands to: `lw rd, offset[7:2](x2)`
///
/// Loads a 32-bit value from memory at address (SP + offset) into register `rd`.
/// The value is sign-extended to 64 bits.
///
/// # Arguments
/// * `rd` - Destination register (0-31, but 0 is reserved)
/// * `offset` - Offset value (0-252, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.LWSP x5, 16  (x5 = sign_extend(mem[SP + 16]))
/// // Expands to: lw x5, 16(x2)
/// ```
pub fn exec_c_lwsp(
    rd: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let sp_val = state.regs[2]; // x2 is SP
    let effective_addr = sp_val.wrapping_add(offset as u64);

    // Check alignment
    if effective_addr % 4 != 0 {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Load 32-bit word and sign-extend to 64-bit
    let value = mem.read_word(effective_addr)?;
    let sign_extended = (value as i32) as i64 as u64;
    state.regs[rd as usize] = sign_extended;

    Ok(())
}

/// Execute C.LDSP - Load doubleword from stack pointer (RV64)
///
/// Expands to: `ld rd, offset[8:3](x2)`
///
/// Loads a 64-bit value from memory at address (SP + offset) into register `rd`.
///
/// # Arguments
/// * `rd` - Destination register (0-31, but 0 is reserved)
/// * `offset` - Offset value (0-504, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.LDSP x5, 16  (x5 = mem[SP + 16])
/// // Expands to: ld x5, 16(x2)
/// ```
pub fn exec_c_ldsp(
    rd: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    if rd == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let sp_val = state.regs[2]; // x2 is SP
    let effective_addr = sp_val.wrapping_add(offset as u64);

    // Check alignment
    if effective_addr % 8 != 0 {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 8));
    }

    // Load 64-bit doubleword
    let value = mem.read_dword(effective_addr)?;
    state.regs[rd as usize] = value;

    Ok(())
}

/// Execute C.SWSP - Store word to stack pointer (compressed)
///
/// Expands to: `sw rs2, offset[7:2](x2)`
///
/// Stores the lower 32 bits of register `rs2` to memory at address (SP + offset).
///
/// # Arguments
/// * `rs2` - Source register (0-31)
/// * `offset` - Offset value (0-252, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SWSP x5, 16  (mem[SP + 16] = x5[31:0])
/// // Expands to: sw x5, 16(x2)
/// ```
pub fn exec_c_swsp(
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let sp_val = state.regs[2]; // x2 is SP
    let effective_addr = sp_val.wrapping_add(offset as u64);

    // Check alignment
    if effective_addr % 4 != 0 {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Store 32-bit word (lower 32 bits of rs2)
    let value = state.regs[rs2 as usize] as u32;
    mem.write_word(effective_addr, value)?;

    Ok(())
}

/// Execute C.SDSP - Store doubleword to stack pointer (RV64)
///
/// Expands to: `sd rs2, offset[8:3](x2)`
///
/// Stores the 64-bit value of register `rs2` to memory at address (SP + offset).
///
/// # Arguments
/// * `rs2` - Source register (0-31)
/// * `offset` - Offset value (0-504, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.SDSP x5, 16  (mem[SP + 16] = x5)
/// // Expands to: sd x5, 16(x2)
/// ```
pub fn exec_c_sdsp(
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let sp_val = state.regs[2]; // x2 is SP
    let effective_addr = sp_val.wrapping_add(offset as u64);

    // Check alignment
    if effective_addr % 8 != 0 {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 8));
    }

    // Store 64-bit doubleword
    let value = state.regs[rs2 as usize];
    mem.write_dword(effective_addr, value)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::memory::SimpleMemory;

    fn setup_test() -> (CoreState, SimpleMemory) {
        (CoreState::default(), SimpleMemory::new(0x20000)) // 128KB for stack tests
    }

    #[test]
    fn test_c_slli() {
        let (mut state, _mem) = setup_test();

        state.regs[5] = 0x00000000000000FF;

        // C.SLLI x5, 8
        exec_c_slli(5, 8, &mut state).unwrap();

        assert_eq!(state.regs[5], 0x000000000000FF00);
    }

    #[test]
    fn test_c_slli_overflow() {
        let (mut state, _mem) = setup_test();

        // Value that will overflow
        state.regs[5] = 0xFF00000000000000;

        // C.SLLI x5, 8
        exec_c_slli(5, 8, &mut state).unwrap();

        // Bits shifted out are lost
        assert_eq!(state.regs[5], 0x0000000000000000);
    }

    #[test]
    fn test_c_slli_by_zero() {
        let (mut state, _mem) = setup_test();

        state.regs[5] = 0x123456789ABCDEF0;

        // C.SLLI x5, 0
        exec_c_slli(5, 0, &mut state).unwrap();

        // No change
        assert_eq!(state.regs[5], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_slli_by_63() {
        let (mut state, _mem) = setup_test();

        state.regs[5] = 0x0000000000000001;

        // C.SLLI x5, 63
        exec_c_slli(5, 63, &mut state).unwrap();

        // Only MSB should be set
        assert_eq!(state.regs[5], 0x8000000000000000);
    }

    #[test]
    fn test_c_slli_x0_fails() {
        let (mut state, _mem) = setup_test();

        // C.SLLI x0, 1 should fail
        let result = exec_c_slli(0, 1, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_slli_invalid_shamt() {
        let (mut state, _mem) = setup_test();

        state.regs[5] = 0x123456789ABCDEF0;

        // C.SLLI with shamt > 63 should fail
        let result = exec_c_slli(5, 64, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_lwsp() {
        let (mut state, mut mem) = setup_test();

        // Set up stack pointer
        state.regs[2] = 0x1000;

        // Write test value to memory
        mem.write_word(0x1004, 0xDEADBEEF).unwrap();

        // Execute C.LWSP x5, 4
        exec_c_lwsp(5, 4, &mut state, &mut mem).unwrap();

        // Should be sign-extended
        assert_eq!(state.regs[5], 0xFFFFFFFF_DEADBEEF);
    }

    #[test]
    fn test_c_lwsp_x0_fails() {
        let (mut state, mut mem) = setup_test();

        state.regs[2] = 0x1000;

        // C.LWSP x0, 4 should fail
        let result = exec_c_lwsp(0, 4, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_lwsp_misaligned() {
        let (mut state, mut mem) = setup_test();

        state.regs[2] = 0x1001; // Misaligned SP

        // C.LWSP x5, 0 should fail due to misalignment
        let result = exec_c_lwsp(5, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_ldsp() {
        let (mut state, mut mem) = setup_test();

        // Set up stack pointer
        state.regs[2] = 0x1000;

        // Write test value to memory
        mem.write_dword(0x1008, 0x123456789ABCDEF0).unwrap();

        // Execute C.LDSP x5, 8
        exec_c_ldsp(5, 8, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[5], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_ldsp_x0_fails() {
        let (mut state, mut mem) = setup_test();

        state.regs[2] = 0x1000;

        // C.LDSP x0, 8 should fail
        let result = exec_c_ldsp(0, 8, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_swsp() {
        let (mut state, mut mem) = setup_test();

        // Set up stack pointer and source register
        state.regs[2] = 0x1000;
        state.regs[5] = 0xDEADBEEF;

        // Execute C.SWSP x5, 4
        exec_c_swsp(5, 4, &mut state, &mut mem).unwrap();

        // Verify memory
        assert_eq!(mem.read_word(0x1004).unwrap(), 0xDEADBEEF);
    }

    #[test]
    fn test_c_swsp_misaligned() {
        let (mut state, mut mem) = setup_test();

        state.regs[2] = 0x1001; // Misaligned SP
        state.regs[5] = 0xDEADBEEF;

        // C.SWSP x5, 0 should fail due to misalignment
        let result = exec_c_swsp(5, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_sdsp() {
        let (mut state, mut mem) = setup_test();

        // Set up stack pointer and source register
        state.regs[2] = 0x1000;
        state.regs[5] = 0x123456789ABCDEF0;

        // Execute C.SDSP x5, 8
        exec_c_sdsp(5, 8, &mut state, &mut mem).unwrap();

        // Verify memory
        assert_eq!(mem.read_dword(0x1008).unwrap(), 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_sdsp_misaligned() {
        let (mut state, mut mem) = setup_test();

        state.regs[2] = 0x1001; // Misaligned SP
        state.regs[5] = 0x123456789ABCDEF0;

        // C.SDSP x5, 0 should fail due to misalignment
        let result = exec_c_sdsp(5, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }
}
