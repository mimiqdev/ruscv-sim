//! RV64M Multiply instructions
//!
//! Implements RV64M multiplication instructions:
//! - MUL: Multiply (lower 32 bits)
//! - MULH: Multiply signed * signed, upper 32 bits
//! - MULHU: Multiply unsigned * unsigned, upper 32 bits
//! - MULHSU: Multiply signed * unsigned, upper 32 bits

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// MUL - Multiply
///
/// Multiplies two 32-bit signed values and returns the lower 32 bits.
///
/// # Operation
/// rd = rs1 * rs2 (32-bit signed multiplication)
#[inline]
pub fn exec_mul(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i32;
    let b = state.regs[rs2] as i32;
    let result = (a as i64 * b as i64) as u32;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULH - Multiply Signed * Signed
///
/// Multiplies two 32-bit signed values and returns the upper 32 bits
/// of the 64-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 32 (signed * signed)
#[inline]
pub fn exec_mulh(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i32;
    let b = state.regs[rs2] as i32;
    let result = ((a as i64 * b as i64) >> 32) as u32;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULHU - Multiply Unsigned * Unsigned
///
/// Multiplies two 32-bit unsigned values and returns the upper 32 bits
/// of the 64-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 32 (unsigned * unsigned)
#[inline]
pub fn exec_mulhu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as u64;
    let b = state.regs[rs2] as u64;
    let result = ((a * b) >> 32) as u32;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULHSU - Multiply Signed * Unsigned
///
/// Multiplies a 32-bit signed value with a 32-bit unsigned value and
/// returns the upper 32 bits of the 64-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 32 (signed * unsigned)
#[inline]
pub fn exec_mulhsu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i32;
    let b = state.regs[rs2] as u64;
    let result = ((a as i64 * b as i64) >> 32) as u32;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_mul_instr(rs1: u8, rs2: u8, rd: u8, funct7: u8) -> DecodedInstruction {
        let raw = ((funct7 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b011_0011;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(Funct3::AddSub),
            funct7: Some(funct7),
            rs1: Some(rs1),
            rs2: Some(rs2),
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    // ========================================
    // MUL Tests
    // ========================================

    #[test]
    fn test_mul_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 6;
        state.regs[2] = 7;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 42);
    }

    #[test]
    fn test_mul_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-6i32) as u32; // -6
        state.regs[2] = 7;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, -42);
    }

    #[test]
    fn test_mul_overflow() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF; // -1
        state.regs[2] = 0xFFFF_FFFF; // -1

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 1); // (-1) * (-1) = 1
    }

    #[test]
    fn test_mul_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 100;
        state.regs[2] = 0;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_mul_x0_dest() {
        let mut state = CoreState::default();
        state.regs[1] = 6;
        state.regs[2] = 7;

        let instr = create_mul_instr(1, 2, 0, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[0], 0); // x0 always reads as 0
    }

    // ========================================
    // MULH Tests
    // ========================================

    #[test]
    fn test_mulh_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0001_0000; // 65536
        state.regs[2] = 0x0001_0000; // 65536

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 1); // (65536 * 65536) >> 32 = 1
    }

    #[test]
    fn test_mulh_negative_result() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // -2147483648
        state.regs[2] = 2;

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF); // -1 (upper bits of -4294967296)
    }

    #[test]
    fn test_mulh_small_numbers() {
        let mut state = CoreState::default();
        state.regs[1] = 3;
        state.regs[2] = 4;

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0); // (3 * 4) = 12, no upper bits
    }

    // ========================================
    // MULHU Tests
    // ========================================

    #[test]
    fn test_mulhu_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // Large unsigned
        state.regs[2] = 0x8000_0000;

        let instr = create_mul_instr(1, 2, 3, 0b000_0011);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (2^31 * 2^31) >> 32 = 2^62 >> 32 = 2^30 = 0x4000_0000
        assert_eq!(state.regs[3], 0x4000_0000);
    }

    #[test]
    fn test_mulhu_positive() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0001_0000;
        state.regs[2] = 0x0001_0000;

        let instr = create_mul_instr(1, 2, 3, 0b000_0011);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 1);
    }

    // ========================================
    // MULHSU Tests
    // ========================================

    #[test]
    fn test_mulhsu_basic() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i32) as u32; // -1 signed
        state.regs[2] = 0x8000_0000; // Large unsigned

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhsu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (-1) * 2^32 = -2^32, upper 32 bits should be 0xFFFFFFFF
        assert_eq!(state.regs[3], 0xFFFF_FFFF);
    }

    #[test]
    fn test_mulhsu_positive_unsigned() {
        let mut state = CoreState::default();
        state.regs[1] = 0x0001_0000; // Positive signed
        state.regs[2] = 0x0001_0000; // Unsigned

        // Using MULH encoding but MULHSU uses same funct7
        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        // Note: This is actually testing MULH behavior, need to use correct funct7
        // MULHSU should have funct7 = 0b000_0010, same as MULH
        // The difference is in how rs1 is interpreted (signed vs unsigned)
        let result = exec_mulhsu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 1);
    }

    // ========================================
    // Edge Cases
    // ========================================

    #[test]
    fn test_mul_max_values() {
        let mut state = CoreState::default();
        state.regs[1] = 0x7FFF_FFFF; // MAX i32
        state.regs[2] = 2;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // 2147483647 * 2 = 4294967294 = 0xFFFFFFFE
        assert_eq!(state.regs[3], 0xFFFF_FFFE);
    }

    #[test]
    fn test_mul_min_values() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // MIN i32 (-2147483648)
        state.regs[2] = 0x8000_0000;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (-2147483648) * (-2147483648) = 4611686018427387904
        // Lower 32 bits = 0
        assert_eq!(state.regs[3], 0);
    }

    #[test]
    fn test_mul_large_numbers() {
        let mut state = CoreState::default();
        state.regs[1] = 0x1234_5678;
        state.regs[2] = 0x9ABC_DEF0;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        let expected = (0x1234_5678i64 * 0x9ABC_DEF0i64) as u32;
        assert_eq!(state.regs[3], expected);
    }

    #[test]
    fn test_mulh_large_numbers() {
        let mut state = CoreState::default();
        state.regs[1] = 0x1234_5678;
        state.regs[2] = 0x9ABC_DEF0;

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // MULH: signed * signed, upper 32 bits
        // Need to interpret as i32 first to get correct signed values
        let a = 0x1234_5678u32 as i32 as i64;
        let b = 0x9ABC_DEF0u32 as i32 as i64;
        let expected = ((a * b) >> 32) as u32;
        assert_eq!(state.regs[3], expected);
    }
}
