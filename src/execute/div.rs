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

/// DIV - Divide Signed (RV64M)
///
/// Divides rs1 by rs2 using 64-bit signed division.
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

    let dividend = state.regs[rs1] as i64;
    let divisor = state.regs[rs2] as i64;

    let result = if divisor == 0 {
        // Division by zero: return -1 (all ones)
        // RISC-V Spec: "Division by zero returns all ones (i.e. -1)"
        -1i64 as u64
    } else if dividend == i64::MIN && divisor == -1 {
        // Overflow case: -2^63 / -1 = 2^63, which doesn't fit in signed 64-bit
        // RISC-V Spec (RV64): "The quotient of MIN_VALUE / -1 is MIN_VALUE"
        // This matches x86 behavior and avoids undefined behavior from hardware
        i64::MIN as u64
    } else {
        (dividend / divisor) as u64
    };

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// DIVU - Divide Unsigned (RV64M)
///
/// Divides rs1 by rs2 using 64-bit unsigned division.
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

    let dividend = state.regs[rs1];
    let divisor = state.regs[rs2];

    let result = if divisor == 0 {
        // Division by zero: all ones
        u64::MAX
    } else {
        dividend / divisor
    };

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// REM - Remainder Signed (RV64M)
///
/// Computes the remainder of rs1 divided by rs2 using 64-bit signed division.
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

    let dividend = state.regs[rs1] as i64;
    let divisor = state.regs[rs2] as i64;

    let result = if divisor == 0 {
        // Division by zero: dividend
        dividend as u64
    } else if dividend == i64::MIN && divisor == -1 {
        // Overflow case: remainder is 0
        0
    } else {
        (dividend % divisor) as u64
    };

    if rd != 0 {
        state.regs[rd] = result;
    }
    Ok(())
}

/// REMU - Remainder Unsigned (RV64M)
///
/// Computes the remainder of rs1 divided by rs2 using 64-bit unsigned division.
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

    let dividend = state.regs[rs1];
    let divisor = state.regs[rs2];

    let result = if divisor == 0 {
        // Division by zero: dividend
        dividend
    } else {
        dividend % divisor
    };

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
    // DIV Tests
    // ========================================

    #[test]
    fn test_div_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 7);
    }

    #[test]
    fn test_div_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-42i64) as u64;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // In RV64, all ones is 64-bit: 0xFFFFFFFFFFFFFFFF
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_div_overflow() {
        let mut state = CoreState::default();
        // In RV64, MIN i64 is 0x8000_0000_0000_0000
        state.regs[1] = 0x8000_0000_0000_0000; // MIN i64
        state.regs[2] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_div(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // MIN i64 / -1 overflows, result is MIN i64
        assert_eq!(state.regs[3], 0x8000_0000_0000_0000);
    }

    #[test]
    fn test_div_remainder() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_divu(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        // In RV64, all ones is 64-bit: 0xFFFFFFFFFFFFFFFF
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
    }

    // ========================================
    // REM Tests
    // ========================================

    #[test]
    fn test_rem_basic() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3] as i32, 1); // 43 % 6 = 1
    }

    #[test]
    fn test_rem_negative() {
        let mut state = CoreState::default();
        state.regs[1] = (-43i64) as u64;
        state.regs[2] = 6;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[3], 42); // Dividend unchanged
    }

    #[test]
    fn test_rem_overflow() {
        let mut state = CoreState::default();
        // In RV64, MIN i64 is 0x8000_0000_0000_0000
        state.regs[1] = 0x8000_0000_0000_0000; // MIN i64
        state.regs[2] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
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

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 42);
    }

    #[test]
    fn test_div_minus_one() {
        let mut state = CoreState::default();
        state.regs[1] = 42;
        state.regs[2] = 0xFFFF_FFFF_FFFF_FFFF; // -1 in 64-bit

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3] as i64, -42);
    }

    #[test]
    fn test_div_exact() {
        let mut state = CoreState::default();
        state.regs[1] = 100;
        state.regs[2] = 10;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 10);
    }

    #[test]
    fn test_rem_negative_divisor() {
        let mut state = CoreState::default();
        state.regs[1] = 43;
        state.regs[2] = 0xFFFF_FFFF_FFFF_FFFA; // -6 in 64-bit

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_rem(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3] as i64, 1); // 43 % -6 = 1
    }

    #[test]
    fn test_div_unsigned_negative_interpretation() {
        let mut state = CoreState::default();
        state.regs[1] = 1;
        state.regs[2] = 0x8000_0000; // Interpreted as 2^31 unsigned

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_divu(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 0); // 1 / 2^31 = 0
    }

    #[test]
    fn test_rem_x0_dest() {
        let mut state = CoreState::default();
        state.regs[1] = 100;
        state.regs[2] = 30;

        let instr = create_div_instr(1, 2, 0, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_rem(&instr, &mut state, &mut mem);
        assert!(result.is_ok());
        assert_eq!(state.regs[0], 0); // x0 always reads as 0
    }

    #[test]
    fn test_div_min_dividend() {
        let mut state = CoreState::default();
        // In RV64, MIN i64 is 0x8000_0000_0000_0000
        state.regs[1] = 0x8000_0000_0000_0000; // MIN i64
        state.regs[2] = 2;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        exec_div(&instr, &mut state, &mut mem).unwrap();
        // MIN i64 / 2 = -4611686018427387904 = 0xC000_0000_0000_0000
        assert_eq!(state.regs[3], 0xC000_0000_0000_0000);
        assert_eq!(state.regs[3] as i64, -4611686018427387904);
    }
}
