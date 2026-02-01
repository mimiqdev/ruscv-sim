//! RV64M Multiply instructions
//!
//! Implements RV64M multiplication instructions:
//! - MUL: Multiply (lower 64 bits)
//! - MULH: Multiply signed * signed, upper 64 bits
//! - MULHU: Multiply unsigned * unsigned, upper 64 bits
//! - MULHSU: Multiply signed * unsigned, upper 64 bits

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// MUL - Multiply (RV64M)
///
/// Multiplies two 64-bit signed values and returns the lower 64 bits.
///
/// # Operation
/// rd = rs1 * rs2 (64-bit signed multiplication)
#[inline]
pub fn exec_mul(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i64;
    let b = state.regs[rs2] as i64;
    let result = a.wrapping_mul(b) as u64;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULH - Multiply Signed * Signed (RV64M)
///
/// Multiplies two 64-bit signed values and returns the upper 64 bits
/// of the 128-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 64 (signed * signed)
#[inline]
pub fn exec_mulh(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i64 as i128;
    let b = state.regs[rs2] as i64 as i128;
    let result = ((a * b) >> 64) as u64;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULHU - Multiply Unsigned * Unsigned (RV64M)
///
/// Multiplies two 64-bit unsigned values and returns the upper 64 bits
/// of the 128-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 64 (unsigned * unsigned)
#[inline]
pub fn exec_mulhu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as u128;
    let b = state.regs[rs2] as u128;
    let result = ((a * b) >> 64) as u64;

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// MULHSU - Multiply Signed * Unsigned (RV64M)
///
/// Multiplies a 64-bit signed value with a 64-bit unsigned value and
/// returns the upper 64 bits of the 128-bit result.
///
/// # Operation
/// rd = (rs1 * rs2) >> 64 (signed * unsigned)
#[inline]
pub fn exec_mulhsu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let a = state.regs[rs1] as i64 as i128;
    let b = state.regs[rs2] as u128;
    let result = ((a * b as i128) >> 64) as u64;

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
            rs3: None,
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
        state.regs[1] = (-6i64) as u64; // -6
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
        // In RV64, -1 is 0xFFFFFFFFFFFFFFFF
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit
        state.regs[2] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit

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
        // In RV64, MULH returns upper 64 bits of 128-bit product
        // Use larger values to get non-zero upper bits
        state.regs[1] = 0x0001_0000_0000_0000; // 2^48
        state.regs[2] = 0x0001_0000_0000_0000; // 2^48

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (2^48 * 2^48) >> 64 = 2^96 >> 64 = 2^32 = 0x1_0000_0000
        assert_eq!(state.regs[3], 0x1_0000_0000);
    }

    #[test]
    fn test_mulh_negative_result() {
        let mut state = CoreState::default();
        // In RV64, use proper 64-bit signed values
        // -1 * 2^48 = -2^48, upper 64 bits of 128-bit result = -1
        state.regs[1] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit
        state.regs[2] = 0x0001_0000_0000_0000; // 2^48

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulh(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (-1) * 2^48 = -2^48, upper 64 bits = 0xFFFFFFFFFFFFFFFF (-1)
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
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
        // In RV64, use proper 64-bit unsigned values
        state.regs[1] = 0x8000_0000_0000_0000; // 2^63
        state.regs[2] = 0x8000_0000_0000_0000; // 2^63

        let instr = create_mul_instr(1, 2, 3, 0b000_0011);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (2^63 * 2^63) >> 64 = 2^126 >> 64 = 2^62 = 0x4000_0000_0000_0000
        assert_eq!(state.regs[3], 0x4000_0000_0000_0000);
    }

    #[test]
    fn test_mulhu_positive() {
        let mut state = CoreState::default();
        // In RV64, use proper 64-bit values for meaningful upper bits
        state.regs[1] = 0x0001_0000_0000_0000; // 2^48
        state.regs[2] = 0x0001_0000_0000_0000; // 2^48

        let instr = create_mul_instr(1, 2, 3, 0b000_0011);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (2^48 * 2^48) >> 64 = 2^96 >> 64 = 2^32 = 0x1_0000_0000
        assert_eq!(state.regs[3], 0x1_0000_0000);
    }

    // ========================================
    // MULHSU Tests
    // ========================================

    #[test]
    fn test_mulhsu_basic() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i64) as u64; // -1 signed (64-bit)
        state.regs[2] = 0x0001_0000_0000_0000; // 2^48 unsigned

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mulhsu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (-1) * 2^48 = -2^48, upper 64 bits = 0xFFFFFFFFFFFFFFFF
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_mulhsu_positive_unsigned() {
        let mut state = CoreState::default();
        // In RV64, use proper 64-bit values
        state.regs[1] = 0x0001_0000_0000_0000; // 2^48 (positive signed)
        state.regs[2] = 0x0001_0000_0000_0000; // 2^48 (unsigned)

        // Using MULH encoding but MULHSU uses same funct7
        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        // Note: This is actually testing MULH behavior, need to use correct funct7
        // MULHSU should have funct7 = 0b000_0010, same as MULH
        // The difference is in how rs1 is interpreted (signed vs unsigned)
        let result = exec_mulhsu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // (2^48 * 2^48) >> 64 = 2^32 = 0x1_0000_0000
        assert_eq!(state.regs[3], 0x1_0000_0000);
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
        // In RV64, MIN i64 is 0x8000_0000_0000_0000 (-9223372036854775808)
        state.regs[1] = 0x8000_0000_0000_0000; // MIN i64
        state.regs[2] = 0x8000_0000_0000_0000;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mul(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // MIN_i64 * MIN_i64 = 2^126, lower 64 bits = 0
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

        let expected = (0x1234_5678i64 * 0x9ABC_DEF0i64) as u64;
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

        // MULH: signed * signed, upper 64 bits of 128-bit product
        // In RV64, this is now 128-bit multiplication, upper 64 bits
        let a = 0x1234_5678i64;
        let b = 0x9ABC_DEF0u64 as i64;
        let expected = (((a as i128) * (b as i128)) >> 64) as u64;
        assert_eq!(state.regs[3], expected);
    }
}
