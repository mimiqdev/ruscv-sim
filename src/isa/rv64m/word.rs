//! RV64M word instructions
//!
//! Implements the 32-bit RV64M operations, each of which sign-extends its
//! 32-bit result to 64 bits:
//! - MULW: Multiply word
//! - DIVW: Divide word signed
//! - DIVUW: Divide word unsigned
//! - REMW: Remainder word signed
//! - REMUW: Remainder word unsigned

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Read the low 32 bits of rs1 and rs2, apply `op`, and write the
/// sign-extended 32-bit result to rd.
fn exec_word(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    op: fn(u32, u32) -> u32,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let result = op(state.regs[rs1] as u32, state.regs[rs2] as u32);
    if rd != 0 {
        state.regs[rd] = result as i32 as i64 as u64;
    }
    Ok(())
}

/// MULW - Multiply word (RV64M)
#[inline]
pub fn exec_mulw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_word(instr, state, u32::wrapping_mul)
}

/// DIVW - Divide word signed (RV64M)
///
/// Division by zero returns -1; `i32::MIN / -1` returns `i32::MIN`.
#[inline]
pub fn exec_divw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_word(instr, state, |a, b| {
        let (a, b) = (a as i32, b as i32);
        if b == 0 {
            u32::MAX
        } else {
            a.wrapping_div(b) as u32
        }
    })
}

/// DIVUW - Divide word unsigned (RV64M)
///
/// Division by zero returns all ones, sign-extended.
#[inline]
pub fn exec_divuw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_word(instr, state, |a, b| a.checked_div(b).unwrap_or(u32::MAX))
}

/// REMW - Remainder word signed (RV64M)
///
/// Remainder by zero returns the dividend; `i32::MIN % -1` returns 0.
#[inline]
pub fn exec_remw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_word(instr, state, |a, b| {
        let (a, b) = (a as i32, b as i32);
        if b == 0 {
            a as u32
        } else {
            a.wrapping_rem(b) as u32
        }
    })
}

/// REMUW - Remainder word unsigned (RV64M)
///
/// Remainder by zero returns the dividend.
#[inline]
pub fn exec_remuw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_word(instr, state, |a, b| a.checked_rem(b).unwrap_or(a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    type WordOp = fn(
        &DecodedInstruction,
        &mut CoreState,
        &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError>;

    fn run(op: WordOp, a: u64, b: u64) -> u64 {
        let mut instr = DecodedInstruction::new(0x0220_81BB);
        instr.opcode = Opcode::Op32;
        instr.format = InstructionFormat::RType;
        instr.rd = Some(3);
        instr.rs1 = Some(1);
        instr.rs2 = Some(2);
        instr.funct7 = Some(1);
        let mut state = CoreState::default();
        state.regs[1] = a;
        state.regs[2] = b;
        let mut mem = SimpleMemory::new(16);
        op(&instr, &mut state, &mut mem).unwrap();
        state.regs[3]
    }

    #[test]
    fn mulw_uses_low_words_and_sign_extends() {
        assert_eq!(run(exec_mulw, 0xFFFF_FFFF_0000_0003, 7), 21);
        assert_eq!(run(exec_mulw, 0x8000_0000, 1), 0xFFFF_FFFF_8000_0000);
        assert_eq!(run(exec_mulw, 0x1_0000, 0x1_0000), 0);
    }

    #[test]
    fn divw_and_remw_follow_the_m_extension_corner_cases() {
        assert_eq!(run(exec_divw, (-7i64) as u64, 2), (-3i64) as u64);
        assert_eq!(run(exec_remw, (-7i64) as u64, 2), (-1i64) as u64);
        assert_eq!(run(exec_divw, 5, 0), u64::MAX);
        assert_eq!(run(exec_remw, 5, 0), 5);
        let min = 0x8000_0000u64;
        assert_eq!(run(exec_divw, min, u64::MAX), 0xFFFF_FFFF_8000_0000);
        assert_eq!(run(exec_remw, min, u64::MAX), 0);
    }

    #[test]
    fn divuw_and_remuw_are_unsigned_on_low_words() {
        assert_eq!(run(exec_divuw, 0xFFFF_FFFF, 2), 0x7FFF_FFFF);
        assert_eq!(run(exec_remuw, 0xFFFF_FFFF, 2), 1);
        assert_eq!(run(exec_divuw, 9, 0), u64::MAX);
        // REMUW by zero returns the dividend word, sign-extended.
        assert_eq!(run(exec_remuw, 0x8000_0001, 0), 0xFFFF_FFFF_8000_0001);
        assert_eq!(run(exec_divuw, 0x8000_0000, 1), 0xFFFF_FFFF_8000_0000);
    }
}
