//! RV64A Atomic Memory Operation (AMO) instructions — typed route helpers.
//!
//! These functions implement the `MemoryInterface` read-modify-write pair for
//! the old `RiscvCore::new` typed constructor: a labeled, non-conforming
//! compatibility adapter (dev-plan §7.1, C24).  The typed pair is not
//! indivisible; standard facades issue the atomic envelope instead.  Both
//! routes share the Hart-owned arithmetic: every operation here computes its
//! result through [`crate::hart_amods`], the single Hart-owned AMO
//! implementation (dev-plan §5.1/§5.2 M1), so no ISA arithmetic lives in a
//! backend or is duplicated here.
//!
//! The generic `exec_amo_rmw` performs the W/D-width typed pair
//! (sign-extended old value for W, full-width for D); the named `exec_*`
//! helpers retain their historical signatures as the public adapter surface.
//!
//! # References
//!
//! - RISC-V ISA Volume I: Unprivileged Spec ("A" extension, Zamo)

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::hart_amods::{self, AmoOperation};
use crate::memory::MemoryInterface;
use crate::physical::AmoWidth;

/// The typed-route AMO read-modify-write for one operation and width.
///
/// Reads the old span bytes, applies the one Hart-owned transform from
/// [`crate::hart_amods`], writes the result, and retires the sign-extended
/// (W) or full-width (D) old value into `rd`.  `rd = x0` suppresses only the
/// register write; the memory operation still occurs.
pub(crate) fn exec_amo_rmw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
    operation: AmoOperation,
    width: AmoWidth,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let operand = state.regs[rs2].to_le_bytes();

    let (old, new) = match width {
        AmoWidth::Word => {
            let old = mem.read_word(addr).map_err(ExecuteError::MemoryError)?;
            let new = hart_amods::apply(operation, width, &old.to_le_bytes(), &operand[..4])
                .map_err(|_| ExecuteError::InvalidOperation)?;
            let new = u32::from_le_bytes(
                new[..4]
                    .try_into()
                    .expect("a W transform returns four bytes"),
            );
            (old as u64, new as u64)
        }
        AmoWidth::Doubleword => {
            let old = mem.read_dword(addr).map_err(ExecuteError::MemoryError)?;
            let new = hart_amods::apply(operation, width, &old.to_le_bytes(), &operand)
                .map_err(|_| ExecuteError::InvalidOperation)?;
            (old, u64::from_le_bytes(new))
        }
    };

    match width {
        AmoWidth::Word => mem
            .write_word(addr, new as u32)
            .map_err(ExecuteError::MemoryError)?,
        AmoWidth::Doubleword => mem
            .write_dword(addr, new)
            .map_err(ExecuteError::MemoryError)?,
    }

    if rd != 0 {
        state.regs[rd] = match width {
            AmoWidth::Word => (old as u32 as i32) as i64 as u64,
            AmoWidth::Doubleword => old,
        };
    }
    Ok(())
}

/// AMOSWAP.W - Atomic Swap (32-bit, typed compatibility route)
///
/// Atomically swaps `rs2`'s low 32 bits with the word at `rs1`, returning the
/// old word sign-extended into `rd`.
#[inline]
pub fn exec_amoswap(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Swap, AmoWidth::Word)
}

/// AMOADD.W - Atomic Add (32-bit, typed compatibility route)
///
/// Atomically adds `rs2`'s low 32 bits to the word at `rs1`, returning the
/// old word sign-extended into `rd`.
#[inline]
pub fn exec_amoadd(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Add, AmoWidth::Word)
}

/// AMOAND.W - Atomic And (32-bit, typed compatibility route)
#[inline]
pub fn exec_amoand(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::BitAnd, AmoWidth::Word)
}

/// AMOOR.W - Atomic Or (32-bit, typed compatibility route)
#[inline]
pub fn exec_amoor(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::BitOr, AmoWidth::Word)
}

/// AMOXOR.W - Atomic Xor (32-bit, typed compatibility route)
#[inline]
pub fn exec_amoxor(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::BitXor, AmoWidth::Word)
}

/// AMOMAX.W - Atomic Max Signed (32-bit, typed compatibility route)
#[inline]
pub fn exec_amomax(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Max, AmoWidth::Word)
}

/// AMOMIN.W - Atomic Min Signed (32-bit, typed compatibility route)
#[inline]
pub fn exec_amomin(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Min, AmoWidth::Word)
}

/// AMOMAXU.W - Atomic Max Unsigned (32-bit, typed compatibility route)
#[inline]
pub fn exec_amomaxu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Maxu, AmoWidth::Word)
}

/// AMOMINU.W - Atomic Min Unsigned (32-bit, typed compatibility route)
#[inline]
pub fn exec_amominu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_amo_rmw(instr, state, mem, AmoOperation::Minu, AmoWidth::Word)
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
        _aq: u8,
        _rl: u8,
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
            funct3: Some(Funct3::Slt), // W width encoding
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
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

        let instr = create_amo_instr(1, 2, 3, 0b00000, 0, 0);
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

        let instr = create_amo_instr(1, 2, 3, 0b00000, 0, 0);
        let result = exec_amoadd(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        // AMO.W returns 32-bit value sign-extended to 64 bits
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 1);
    }

    #[test]
    fn test_amoadd_zero() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 100).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0;

        let instr = create_amo_instr(1, 2, 3, 0b00000, 0, 0);
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

        let instr = create_amo_instr(1, 2, 3, 0b01100, 0, 0);
        let result = exec_amoand(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFF);
        assert_eq!(mem.read_word(0x100).unwrap(), 0x0F);
    }

    #[test]
    fn test_amoand_all_ones() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xABCD_EF01).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xFFFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b01100, 0, 0);
        let result = exec_amoand(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF_ABCD_EF01);
        assert_eq!(mem.read_word(0x100).unwrap(), 0xABCD_EF01);
    }

    // ========================================
    // AMOOR Tests
    // ========================================

    #[test]
    fn test_amoor_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xF0).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x0F;

        let instr = create_amo_instr(1, 2, 3, 0b01000, 0, 0);
        let result = exec_amoor(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xF0);
        assert_eq!(mem.read_word(0x100).unwrap(), 0xFF);
    }

    #[test]
    fn test_amoor_sign_bit() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0x8000_0000).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0x0000_0001;

        let instr = create_amo_instr(1, 2, 3, 0b01000, 0, 0);
        let result = exec_amoor(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF_8000_0000);
        assert_eq!(mem.read_word(0x100).unwrap(), 0x8000_0001);
    }

    // ========================================
    // AMOXOR Tests
    // ========================================

    #[test]
    fn test_amoxor_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xAA).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xFF;

        let instr = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
        let result = exec_amoxor(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xAA);
        assert_eq!(mem.read_word(0x100).unwrap(), 0x55);
    }

    #[test]
    fn test_amoxor_toggle() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0x55).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 0xFF;

        let instr = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
        exec_amoxor(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(mem.read_word(0x100).unwrap(), 0xAA);

        let instr2 = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
        exec_amoxor(&instr2, &mut state, &mut mem).unwrap();
        assert_eq!(mem.read_word(0x100).unwrap(), 0x55);
    }

    // ========================================
    // AMOMAX Tests (signed)
    // ========================================

    #[test]
    fn test_amomax_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 5).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b10100, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 5);
        assert_eq!(mem.read_word(0x100).unwrap(), 10);
    }

    #[test]
    fn test_amomax_negative() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFF6).unwrap(); // -10
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 3, 0b10100, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFF6); // -10 sign-extended
        assert_eq!(mem.read_word(0x100).unwrap(), 5); // max(-10, 5) = 5
    }

    #[test]
    fn test_amomax_unsigned_greater_but_signed_smaller() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0x8000_0000).unwrap(); // INT32_MIN
        state.regs[1] = 0x100;
        state.regs[2] = 0x7FFF_FFFF;

        let instr = create_amo_instr(1, 2, 3, 0b10100, 0, 0);
        let result = exec_amomax(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(mem.read_word(0x100).unwrap(), 0x7FFF_FFFF);
    }

    // ========================================
    // AMOMIN Tests (signed)
    // ========================================

    #[test]
    fn test_amomin_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 5).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b10000, 0, 0);
        let result = exec_amomin(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 5);
        assert_eq!(mem.read_word(0x100).unwrap(), 5);
    }

    #[test]
    fn test_amomin_negative() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFF6).unwrap(); // -10
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 3, 0b10000, 0, 0);
        let result = exec_amomin(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(mem.read_word(0x100).unwrap(), 0xFFFF_FFF6); // min(-10, 5) = -10
    }

    // ========================================
    // AMOMAXU Tests (unsigned)
    // ========================================

    #[test]
    fn test_amomaxu_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 5).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b11100, 0, 0);
        let result = exec_amomaxu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 5);
        assert_eq!(mem.read_word(0x100).unwrap(), 10);
    }

    #[test]
    fn test_amomaxu_unsigned_comparison() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFFF).unwrap(); // large unsigned
        state.regs[1] = 0x100;
        state.regs[2] = 1;

        let instr = create_amo_instr(1, 2, 3, 0b11100, 0, 0);
        let result = exec_amomaxu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(mem.read_word(0x100).unwrap(), 0xFFFF_FFFF);
    }

    // ========================================
    // AMOMINU Tests (unsigned)
    // ========================================

    #[test]
    fn test_amominu_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 5).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 10;

        let instr = create_amo_instr(1, 2, 3, 0b11000, 0, 0);
        let result = exec_amominu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 5);
        assert_eq!(mem.read_word(0x100).unwrap(), 5);
    }

    #[test]
    fn test_amominu_unsigned_comparison() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 0xFFFF_FFFF).unwrap(); // large unsigned
        state.regs[1] = 0x100;
        state.regs[2] = 1;

        let instr = create_amo_instr(1, 2, 3, 0b11000, 0, 0);
        let result = exec_amominu(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(mem.read_word(0x100).unwrap(), 1); // min(0xFFFFFFFF, 1) = 1
    }

    // ========================================
    // AMOSWAP Tests
    // ========================================

    #[test]
    fn test_amoswap_basic() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 6;

        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        let result = exec_amoswap(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[3], 10, "rd receives the old value");
        assert_eq!(mem.read_word(0x100).unwrap(), 6, "memory receives rs2");
    }

    // ========================================
    // Cross-cutting behavior
    // ========================================

    #[test]
    fn test_amo_x0_dest() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 10).unwrap();
        state.regs[1] = 0x100;
        state.regs[2] = 5;

        let instr = create_amo_instr(1, 2, 0, 0b00000, 0, 0);
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
        state.regs[2] = 8;

        let instr = create_amo_instr(1, 2, 3, 0b00000, 0, 0);
        exec_amoadd(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 42);
    }

    #[test]
    fn test_amo_sequence() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x100, 100).unwrap();
        state.regs[1] = 0x100;

        state.regs[2] = 10;
        exec_amoadd(
            &create_amo_instr(1, 2, 3, 0b00000, 0, 0),
            &mut state,
            &mut mem,
        )
        .unwrap();
        state.regs[2] = 5;
        exec_amoand(
            &create_amo_instr(1, 2, 4, 0b01100, 0, 0),
            &mut state,
            &mut mem,
        )
        .unwrap();
        state.regs[2] = 3;
        exec_amoor(
            &create_amo_instr(1, 2, 5, 0b01000, 0, 0),
            &mut state,
            &mut mem,
        )
        .unwrap();
        state.regs[2] = 4;
        exec_amoxor(
            &create_amo_instr(1, 2, 6, 0b00100, 0, 0),
            &mut state,
            &mut mem,
        )
        .unwrap();

        // rd receives each operation's old value; the chain is
        // 100 +10 = 110, 110 & 5 = 4, 4 | 3 = 7, 7 ^ 4 = 3.
        assert_eq!(state.regs[3], 100);
        assert_eq!(state.regs[4], 110);
        assert_eq!(state.regs[5], 4);
        assert_eq!(state.regs[6], 7);
        assert_eq!(mem.read_word(0x100).unwrap(), 3);
    }

    #[test]
    fn test_amo_d_variants_operate_full_width() {
        // The generic D-width path: a D read-modify-write touches all eight
        // bytes and returns the full old value (dev-plan §5.3).
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_dword(0x108, 0x0000_0001_8000_0002).unwrap();
        state.regs[1] = 0x108;
        state.regs[2] = 1;

        let instr = create_amo_instr(1, 2, 3, 0b00000, 0, 0);
        exec_amo_rmw(
            &instr,
            &mut state,
            &mut mem,
            AmoOperation::Add,
            AmoWidth::Doubleword,
        )
        .unwrap();

        assert_eq!(state.regs[3], 0x0000_0001_8000_0002, "full-width old value");
        assert_eq!(mem.read_dword(0x108).unwrap(), 0x0000_0001_8000_0003);
    }
}
