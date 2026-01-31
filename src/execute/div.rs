//! RV64M Divide instructions
//!
//! Implements RV64M division and remainder instructions:
//! - DIV: Divide signed
//! - DIVU: Divide unsigned
//! - REM: Remainder signed
//! - REMU: Remainder unsigned

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// DIV - Divide Signed
///
/// Divides rs1 by rs2 using 32-bit signed division.
/// The quotient is written to rd.
///
/// # Operation
/// rd = rs1 / rs2 (signed division)
#[inline]
pub fn exec_div(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let dividend = state.regs[rs1] as i32;
    let divisor = state.regs[rs2] as i32;

    let result = if divisor == 0 {
        // Division by zero: all ones (-1)
        -1i32 as u32
    } else if dividend == i32::MIN && divisor == -1 {
        // Overflow: -2^31 / -1 = 2^31, doesn't fit in 32-bit signed
        // In RISC-V, this also results in all ones
        -1i32 as u32
    } else {
        (dividend / divisor) as u32
    };

    state.regs[rd] = result;
    Ok(())
}

/// DIVU - Divide Unsigned
///
/// Divides rs1 by rs2 using 32-bit unsigned division.
/// The quotient is written to rd.
///
/// # Operation
/// rd = rs1 / rs2 (unsigned division)
#[inline]
pub fn exec_divu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let dividend = state.regs[rs1] as u32;
    let divisor = state.regs[rs2] as u32;

    let result = if divisor == 0 {
        // Division by zero: all ones
        u32::MAX
    } else {
        dividend / divisor
    };

    state.regs[rd] = result;
    Ok(())
}

/// REM - Remainder Signed
///
/// Computes the remainder of rs1 divided by rs2 using 32-bit signed division.
/// The remainder is written to rd.
///
/// # Operation
/// rd = rs1 % rs2 (signed remainder)
#[inline]
pub fn exec_rem(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let dividend = state.regs[rs1] as i32;
    let divisor = state.regs[rs2] as i32;

    let result = if divisor == 0 {
        // Division by zero: dividend
        dividend as u32
    } else if dividend == i32::MIN && divisor == -1 {
        // Overflow case: remainder is 0
        0
    } else {
        (dividend % divisor) as u32
    };

    state.regs[rd] = result;
    Ok(())
}

/// REMU - Remainder Unsigned
///
/// Computes the remainder of rs1 divided by rs2 using 32-bit unsigned division.
/// The remainder is written to rd.
///
/// # Operation
/// rd = rs1 % rs2 (unsigned remainder)
#[inline]
pub fn exec_remu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let dividend = state.regs[rs1] as u32;
    let divisor = state.regs[rs2] as u32;

    let result = if divisor == 0 {
        // Division by zero: dividend
        dividend
    } else {
        dividend % divisor
    };

    state.regs[rd] = result;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};

    fn create_div_instr(rs1: u8, rs2: u8, rd: u8, funct7: u8) -> DecodedInstruction {
        let raw = ((funct7 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b011_0011;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(0),
            funct7: Some(funct7),
            rs1: Some(rs1),
            rs2: Some(rs2),
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    // ========================================
    // DIV Tests
    // ========================================

    #[test]
    fn test_div_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 7);
    }

    #[test]
    fn test_div_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-42i32) as u32;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, -7);
    }

    #[test]
    fn test_div_by_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones
    }

    #[test]
    fn test_div_overflow() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // -2147483648
        state.regs[2] = 0xFFFF_FFFF; // -1

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones (overflow)
    }

    #[test]
    fn test_div_remainder() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 7); // 43 / 6 = 7
    }

    // ========================================
    // DIVU Tests
    // ========================================

    #[test]
    fn test_divu_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_divu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 7);
    }

    #[test]
    fn test_divu_large_unsigned() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF;
        state.regs[2] = 2;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_divu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0x7FFF_FFFF);
    }

    #[test]
    fn test_divu_by_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_divu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones
    }

    // ========================================
    // REM Tests
    // ========================================

    #[test]
    fn test_rem_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, 1); // 43 % 6 = 1
    }

    #[test]
    fn test_rem_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-43i32) as u32;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, -1); // (-43) % 6 = -1
    }

    #[test]
    fn test_rem_by_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 42); // Dividend unchanged
    }

    #[test]
    fn test_rem_overflow() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // -2147483648
        state.regs[2] = 0xFFFF_FFFF; // -1

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0); // Remainder is 0
    }

    // ========================================
    // REMU Tests
    // ========================================

    #[test]
    fn test_remu_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_remu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 1);
    }

    #[test]
    fn test_remu_large_unsigned() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFF_FFFF;
        state.regs[2] = 1000;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_remu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF % 1000);
    }

    #[test]
    fn test_remu_by_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_remu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 42); // Dividend unchanged
    }

    // ========================================
    // Edge Cases
    // ========================================

    #[test]
    fn test_div_one() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 1;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 42);
    }

    #[test]
    fn test_div_minus_one() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0xFFFF_FFFFu32 as u32; // -1

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3] as i32, -42);
    }

    #[test]
    fn test_div_exact() {
        let mut state = CoreState::default();
        state.regs[1] = 100;
        state.regs[2] = 10;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 10);
    }

    #[test]
    fn test_rem_negative_divisor() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 0xFFFF_FFFAu32 as u32; // -6

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_rem(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3] as i32, 1); // 43 % -6 = 1
    }

    #[test]
    fn test_div_unsigned_negative_interpretation() {
        let mut state = CoreState::default();
        state.regs[1] = 1;
        state.regs[2] = 0x8000_0000; // Interpreted as 2^31 unsigned

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_divu(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 0); // 1 / 2^31 = 0
    }

    #[test]
    fn test_rem_x0_dest() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 0, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[0], 0);
    }

    #[test]
    fn test_div_min_dividend() {
        let mut state = CoreState::default();
        state.regs[1] = 0x8000_0000; // -2147483648
        state.regs[2] = 2;

        let instr = create_div_instr(1, 2, 3, 0b0000_001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3] as i32, -1073741824); // -2^30
    }
}
