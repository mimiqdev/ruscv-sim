//! RV64A Atomic Memory Operation (AMO) instructions
//!
//! Implements AMO instructions for atomic read-modify-write operations:
//! - AMOADD: Atomic add
//! - AMOAND: Atomic and
//! - AMOOR: Atomic or
//! - AMOXOR: Atomic xor
//! - AMOMAX: Atomic max (signed)
//! - AMOMIN: Atomic min (signed)
//! - AMOMAXU: Atomic max (unsigned)
//! - AMOMINU: Atomic min (unsigned)

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// AMOADD - Atomic Add
///
/// Atomically adds rs2 to the value in memory at rs1.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = temp + rs2
/// rd = temp
#[inline]
pub fn exec_amoadd(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    // Read the current value
    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    // Compute new value
    let new_value = old_value.wrapping_add(value);

    // Write back
    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    // Return old value to rd
    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOAND - Atomic And
///
/// Atomically performs bitwise AND of rs2 with the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = temp & rs2
/// rd = temp
#[inline]
pub fn exec_amoand(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = old_value & value;

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOOR - Atomic Or
///
/// Atomically performs bitwise OR of rs2 with the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = temp | rs2
/// rd = temp
#[inline]
pub fn exec_amoor(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = old_value | value;

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOXOR - Atomic Xor
///
/// Atomically performs bitwise XOR of rs2 with the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = temp ^ rs2
/// rd = temp
#[inline]
pub fn exec_amoxor(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = old_value ^ value;

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOMAX - Atomic Max (Signed)
///
/// Atomically stores the maximum (signed) of rs2 and the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = max(temp, rs2) [signed]
/// rd = temp
#[inline]
pub fn exec_amomax(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = if (old_value as i32) > (value as i32) {
        old_value
    } else {
        value
    };

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOMIN - Atomic Min (Signed)
///
/// Atomically stores the minimum (signed) of rs2 and the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = min(temp, rs2) [signed]
/// rd = temp
#[inline]
pub fn exec_amomin(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = if (old_value as i32) < (value as i32) {
        old_value
    } else {
        value
    };

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOMAXU - Atomic Max (Unsigned)
///
/// Atomically stores the maximum (unsigned) of rs2 and the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = max(temp, rs2) [unsigned]
/// rd = temp
#[inline]
pub fn exec_amomaxu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = if old_value > value { old_value } else { value };

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

/// AMOMINU - Atomic Min (Unsigned)
///
/// Atomically stores the minimum (unsigned) of rs2 and the value in memory.
/// Returns the original value in memory.
///
/// # Operation
/// temp = MEM[rs1]
/// MEM[rs1] = min(temp, rs2) [unsigned]
/// rd = temp
#[inline]
pub fn exec_amominu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    let old_value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;
    let new_value = if old_value < value { old_value } else { value };

    mem.write_word(addr, new_value)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    if rd != 0 {
        state.regs[rd] = old_value;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_amo_instr(
        rs1: u8,
        rs2: u8,
        rd: u8,
        funct5: u8,
        aq: u8,
        rl: u8,
    ) -> DecodedInstruction {
        let raw = ((funct5 as u32) << 27)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b010_1111;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Amo,
            funct3: Some(Funct3::Slt), // Using Slt (0b010) for AMO width encoding
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    // ========================================
    // AMOADD Tests
    // ========================================

    #[test]
    fn test_amoadd_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        let result = exec_amoadd(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 10); // Returns old value
        assert_eq!(mem.read_word(0x100).unwrap(), 15); // New value
    }

    #[test]
    fn test_amoadd_wrapping() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFFF).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 2;

        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        let result = exec_amoadd(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 1);
    }

    #[test]
    fn test_amoadd_zero() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 100).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0;

        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        let result = exec_amoadd(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 100);
        assert_eq!(mem.read_word(0x100).unwrap(), 100);
    }

    // ========================================
    // AMOAND Tests
    // ========================================

    #[test]
    fn test_amoand_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFF).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x0F;

        let instr = create_amo_instr(1, 2, 3, 0b00011, 0, 0);
        let result = exec_amoand(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 0x0F);
    }

    #[test]
    fn test_amoand_all_ones() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFFF).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xAAAA_AAAA;

        let instr = create_amo_instr(1, 2, 3, 0b00011, 0, 0);
        let result = exec_amoand(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 0xAAAA_AAAA);
    }

    // ========================================
    // AMOOR Tests
    // ========================================

    #[test]
    fn test_amoor_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0x0F).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xF0;

        let instr = create_amo_instr(1, 2, 3, 0b00110, 0, 0);
        let result = exec_amoor(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0x0F);
        assert_eq!(mem.read_word(0x100).unwrap(), 0xFF);
    }

    // ========================================
    // AMOXOR Tests
    // ========================================

    #[test]
    fn test_amoxor_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFF).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x0F;

        let instr = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
        let result = exec_amoxor(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 0xF0);
    }

    #[test]
    fn test_amoxor_toggle() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xFFFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
        exec_amoxor(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_word(0x100).unwrap(), 0xFFFF_FFFF);

        // Toggle again
        state.regs[2] = 0xFFFF_FFFF;
        let instr2 = create_amo_instr(1, 2, 4, 0b00100, 0, 0);
        exec_amoxor(&instr2, &mut state, &mut mem).unwrap();

        assert_eq!(mem.read_word(0x100).unwrap(), 0);
    }

    // ========================================
    // AMOMAX Tests (Signed)
    // ========================================

    #[test]
    fn test_amomax_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 20;

        let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 10);
        assert_eq!(mem.read_word(0x100).unwrap(), 20);
    }

    #[test]
    fn test_amomax_negative() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, (-10i32) as u32).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 20;

        let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, -10);
        assert_eq!(state.regs[3] as i32, -10); // -10 > 20 is false
        assert_eq!(mem.read_word(0x100).unwrap(), 20);
    }

    #[test]
    fn test_amomax_unsigned_greater_but_signed_smaller() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // 0x8000_0000 as signed is -2147483648, as unsigned is 2147483648
        // 0x7FFF_FFFF as signed is 2147483647, as unsigned is 2147483647
        mem.write_word(0x100, 0x8000_0000).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x7FFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        // Signed: -2147483648 < 2147483647, so 0x7FFF_FFFF wins
        assert_eq!(state.regs[3], 0x8000_0000);
        assert_eq!(mem.read_word(0x100).unwrap(), 0x7FFF_FFFF);
    }

    // ========================================
    // AMOMIN Tests (Signed)
    // ========================================

    #[test]
    fn test_amomin_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 20).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b01000, 0, 0);
        let result = exec_amomin(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 20);
        assert_eq!(mem.read_word(0x100).unwrap(), 10);
    }

    #[test]
    fn test_amomin_negative() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, (-10i32) as u32).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 3, 0b01000, 0, 0);
        let result = exec_amomin(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, -10);
        assert_eq!(state.regs[3] as i32, -10); // -10 < 5, so -10 wins
        assert_eq!(mem.read_word(0x100).unwrap(), (-10i32) as u32);
    }

    // ========================================
    // AMOMAXU Tests (Unsigned)
    // ========================================

    #[test]
    fn test_amomaxu_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 20;

        let instr = create_amo_instr(1, 2, 3, 0b01011, 0, 0);
        let result = exec_amomaxu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 10);
        assert_eq!(mem.read_word(0x100).unwrap(), 20);
    }

    #[test]
    fn test_amomaxu_unsigned_comparison() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // 0x8000_0000 as unsigned is 2147483648
        // 0x7FFF_FFFF as unsigned is 2147483647
        mem.write_word(0x100, 0x8000_0000).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x7FFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b01011, 0, 0);
        let result = exec_amomaxu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0x8000_0000);
        // Unsigned: 2147483648 > 2147483647, so 0x8000_0000 wins
        assert_eq!(mem.read_word(0x100).unwrap(), 0x8000_0000);
    }

    // ========================================
    // AMOMINU Tests (Unsigned)
    // ========================================

    #[test]
    fn test_amominu_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 20).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b01001, 0, 0);
        let result = exec_amominu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 20);
        assert_eq!(mem.read_word(0x100).unwrap(), 10);
    }

    #[test]
    fn test_amominu_unsigned_comparison() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // 0x8000_0000 as unsigned is 2147483648
        // 0x7FFF_FFFF as unsigned is 2147483647
        mem.write_word(0x100, 0x8000_0000).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x7FFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b01001, 0, 0);
        let result = exec_amominu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0x8000_0000);
        // Unsigned: 2147483648 > 2147483647, so 0x7FFF_FFFF wins (smaller)
        assert_eq!(mem.read_word(0x100).unwrap(), 0x7FFF_FFFF);
    }

    // ========================================
    // Edge Cases
    // ========================================

    #[test]
    fn test_amo_x0_dest() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 0, 0b00001, 0, 0);
        let result = exec_amoadd(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[0], 0); // x0 always 0
        assert_eq!(mem.read_word(0x100).unwrap(), 15);
    }

    #[test]
    fn test_amo_returns_original() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 42).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 100;

        // AMOADD
        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        exec_amoadd(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 42);

        // AMOAND
        mem.write_word(0x100, 0xFF).unwrap();
        state.regs[2] = 0x0F;
        let instr2 = create_amo_instr(1, 2, 4, 0b00011, 0, 0);
        exec_amoand(&instr2, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[4], 0xFF);

        // AMOOR
        mem.write_word(0x100, 0x0F).unwrap();
        state.regs[2] = 0xF0;
        let instr3 = create_amo_instr(1, 2, 5, 0b00110, 0, 0);
        exec_amoor(&instr3, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[5], 0x0F);
    }

    #[test]
    fn test_amo_sequence() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0).unwrap();
        state.regs[1] = 0x100;

        // Increment by 1, ten times
        for i in 0..10 {
            state.regs[2] = 1;
            let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
            exec_amoadd(&instr, &mut state, &mut mem).unwrap();
            assert_eq!(state.regs[3], i as u32); // Returns previous value
        }

        assert_eq!(mem.read_word(0x100).unwrap(), 10);
    }
}
