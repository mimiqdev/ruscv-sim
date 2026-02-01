//! RV64C C0 Quadrant Instructions - Load/Store Operations
//!
//! This module implements the execution of C0 quadrant compressed instructions
//! which are primarily load/store operations and stack pointer relative operations.

use crate::core::CoreState;

use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Execute C.ADDI4SPN - Add immediate to stack pointer (scaled by 4)
///
/// Expands to: `addi rd', x2, nzuimm[9:2]`
///
/// Adds a zero-extended non-zero immediate, scaled by 4, to the stack pointer (x2)
/// and writes the result to `rd'` (registers 8-15).
///
/// # Arguments
/// * `rd` - Destination register (compressed, 8-15)
/// * `imm` - Immediate value (0-1020, must be non-zero and multiple of 4)
/// * `state` - Core state containing registers
///
/// # Returns
/// Result indicating success or execution error
///
/// # Example
/// ```
/// // C.ADDI4SPN x8, 64  (Add 64 to SP, store in x8)
/// // Expands to: addi x8, x2, 64
/// ```
pub fn exec_c_addi4spn(rd: u8, imm: u32, state: &mut CoreState) -> Result<(), ExecuteError> {
    if imm == 0 {
        return Err(ExecuteError::InvalidOperation);
    }

    let sp_val = state.regs[2];
    let result = sp_val.wrapping_add(imm as u64);
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute C.LW - Load word (compressed)
///
/// Expands to: `lw rd', offset[6:2](rs1')`
///
/// Loads a 32-bit value from memory into register `rd'`. The effective address
/// is formed by adding the zero-extended offset (scaled by 4) to the base
/// address in register `rs1'`.
///
/// # Arguments
/// * `rd` - Destination register (compressed, 8-15)
/// * `rs1` - Base address register (compressed, 8-15)
/// * `offset` - Offset value (0-124, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
pub fn exec_c_lw(
    rd: u8,
    rs1: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let base_addr = state.regs[rs1 as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment
    if !effective_addr.is_multiple_of(4) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Load 32-bit word and sign-extend to 64-bit
    let value = mem.read_word(effective_addr)?;
    let sign_extended = value as i32 as i64 as u64;
    state.regs[rd as usize] = sign_extended;

    Ok(())
}

/// Execute C.LD - Load doubleword (RV64)
///
/// Expands to: `ld rd', offset[7:3](rs1')`
///
/// Loads a 64-bit value from memory into register `rd'`. The effective address
/// is formed by adding the zero-extended offset (scaled by 8) to the base
/// address in register `rs1'`.
///
/// # Arguments
/// * `rd` - Destination register (compressed, 8-15)
/// * `rs1` - Base address register (compressed, 8-15)
/// * `offset` - Offset value (0-248, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
pub fn exec_c_ld(
    rd: u8,
    rs1: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let base_addr = state.regs[rs1 as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment
    if !effective_addr.is_multiple_of(8) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 8));
    }

    // Load 64-bit doubleword
    let value = mem.read_dword(effective_addr)?;
    state.regs[rd as usize] = value;

    Ok(())
}

/// Execute C.SW - Store word (compressed)
///
/// Expands to: `sw rs2', offset[6:2](rs1')`
///
/// Stores the lower 32 bits of register `rs2'` to memory. The effective address
/// is formed by adding the zero-extended offset (scaled by 4) to the base
/// address in register `rs1'`.
///
/// # Arguments
/// * `rs1` - Base address register (compressed, 8-15)
/// * `rs2` - Source register (compressed, 8-15)
/// * `offset` - Offset value (0-124, multiple of 4)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
pub fn exec_c_sw(
    rs1: u8,
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let base_addr = state.regs[rs1 as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment
    if !effective_addr.is_multiple_of(4) {
        return Err(ExecuteError::MisalignedAccess(effective_addr, 4));
    }

    // Store 32-bit word (lower 32 bits of rs2)
    let value = state.regs[rs2 as usize] as u32;
    mem.write_word(effective_addr, value)?;

    Ok(())
}

/// Execute C.SD - Store doubleword (RV64)
///
/// Expands to: `sd rs2', offset[7:3](rs1')`
///
/// Stores the 64-bit value of register `rs2'` to memory. The effective address
/// is formed by adding the zero-extended offset (scaled by 8) to the base
/// address in register `rs1'`.
///
/// # Arguments
/// * `rs1` - Base address register (compressed, 8-15)
/// * `rs2` - Source register (compressed, 8-15)
/// * `offset` - Offset value (0-248, multiple of 8)
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// Result indicating success or execution error
pub fn exec_c_sd(
    rs1: u8,
    rs2: u8,
    offset: u32,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let base_addr = state.regs[rs1 as usize];
    let effective_addr = base_addr.wrapping_add(offset as u64);

    // Check alignment
    if !effective_addr.is_multiple_of(8) {
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
        (CoreState::default(), SimpleMemory::new(0x20000)) // 128KB
    }

    #[test]
    fn test_c_addi4spn() {
        let (mut state, _mem) = setup_test();

        // Set SP to some value
        state.regs[2] = 0x1000;

        // Execute C.ADDI4SPN x8, 64
        exec_c_addi4spn(8, 64, &mut state).unwrap();

        assert_eq!(state.regs[8], 0x1040);
    }

    #[test]
    fn test_c_addi4spn_zero_imm_fails() {
        let (mut state, _mem) = setup_test();
        state.regs[2] = 0x1000;

        // C.ADDI4SPN with imm=0 should fail
        let result = exec_c_addi4spn(8, 0, &mut state);
        assert!(result.is_err());
    }

    #[test]
    fn test_c_lw() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[8] = 0x100;

        // Write test value to memory
        mem.write_word(0x104, 0xDEADBEEF).unwrap();

        // Execute C.LW x9, 4(x8) - offset is 4 bytes
        exec_c_lw(9, 8, 4, &mut state, &mut mem).unwrap();

        // Should be sign-extended
        assert_eq!(state.regs[9], 0xFFFFFFFF_DEADBEEF);
    }

    #[test]
    fn test_c_ld() {
        let (mut state, mut mem) = setup_test();

        // Set up base address
        state.regs[8] = 0x100;

        // Write test value to memory
        mem.write_dword(0x108, 0x123456789ABCDEF0).unwrap();

        // Execute C.LD x9, 8(x8) - offset is 8 bytes
        exec_c_ld(9, 8, 8, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[9], 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_sw() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[8] = 0x100; // base
        state.regs[9] = 0xDEADBEEF; // value to store (lower 32 bits)

        // Execute C.SW x9, 4(x8)
        exec_c_sw(8, 9, 4, &mut state, &mut mem).unwrap();

        // Verify memory
        assert_eq!(mem.read_word(0x104).unwrap(), 0xDEADBEEF);
    }

    #[test]
    fn test_c_sd() {
        let (mut state, mut mem) = setup_test();

        // Set up registers
        state.regs[8] = 0x100; // base
        state.regs[9] = 0x123456789ABCDEF0; // value to store

        // Execute C.SD x9, 8(x8)
        exec_c_sd(8, 9, 8, &mut state, &mut mem).unwrap();

        // Verify memory
        assert_eq!(mem.read_dword(0x108).unwrap(), 0x123456789ABCDEF0);
    }

    #[test]
    fn test_c_lw_misaligned() {
        let (mut state, mut mem) = setup_test();

        state.regs[8] = 0x101; // misaligned base

        // Should fail due to misalignment
        let result = exec_c_lw(9, 8, 0, &mut state, &mut mem);
        assert!(result.is_err());
    }
}
