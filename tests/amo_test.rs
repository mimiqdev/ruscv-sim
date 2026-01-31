//! Tests for RV64A Atomic Memory Operation (AMO) instructions
//!
//! Tests AMOADD, AMOAND, AMOOR, AMOXOR, AMOMAX, AMOMIN, AMOMAXU, AMOMINU

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
use ruscv_sim::execute::amo::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoxor,
};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};

fn create_amo_instr(rs1: u8, rs2: u8, rd: u8, funct5: u8, _aq: u8, _rl: u8) -> DecodedInstruction {
    let raw = ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((rd as u32) << 7)
        | 0b010_1111;
    DecodedInstruction {
        raw,
        format: InstructionFormat::RType,
        opcode: Opcode::Amo,
        funct3: Some(Funct3::Slt),
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
// AMOADD Tests (10 tests)
// ========================================

#[test]
fn test_amoadd_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0xFFFF_FFFE).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 5;

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0xFFFF_FFFE);
    assert_eq!(mem.read_word(0x100).unwrap(), 3); // Wrapped
}

#[test]
fn test_amoadd_zero() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 100).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0;

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 100);
    assert_eq!(mem.read_word(0x100).unwrap(), 100);
}

#[test]
fn test_amoadd_negative_value() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 50).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = (-10i32) as u32;

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 50);
    assert_eq!(mem.read_word(0x100).unwrap(), 40);
}

#[test]
fn test_amoadd_large_numbers() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x8000_0000).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0x8000_0000;

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x8000_0000);
    assert_eq!(mem.read_word(0x100).unwrap(), 0); // 0x8000_0000 + 0x8000_0000 = 0 (mod 2^32)
}

#[test]
fn test_amoadd_x0_dest() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
fn test_amoadd_sequence() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0).unwrap();
    state.regs[1] = 0x100;

    for i in 0..10 {
        state.regs[2] = 1;
        let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
        exec_amoadd(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], i as u32); // Returns previous value
    }

    assert_eq!(mem.read_word(0x100).unwrap(), 10);
}

#[test]
fn test_amoadd_max_values() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x7FFF_FFFF).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 1;

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x7FFF_FFFF);
    assert_eq!(mem.read_word(0x100).unwrap(), 0x8000_0000);
}

#[test]
fn test_amoadd_min_values() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 1).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0xFFFF_FFFF; // -1

    let instr = create_amo_instr(1, 2, 3, 0b00001, 0, 0);
    let result = exec_amoadd(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 1);
    assert_eq!(mem.read_word(0x100).unwrap(), 0);
}

// ========================================
// AMOAND Tests (8 tests)
// ========================================

#[test]
fn test_amoand_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0xFFFF_FFFF).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0xAAAA_AAAA;

    let instr = create_amo_instr(1, 2, 3, 0b00011, 0, 0);
    let result = exec_amoand(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0xFFFF_FFFF);
    assert_eq!(mem.read_word(0x100).unwrap(), 0xAAAA_AAAA);
}

#[test]
fn test_amoand_zero_mask() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x1234_5678).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0;

    let instr = create_amo_instr(1, 2, 3, 0b00011, 0, 0);
    let result = exec_amoand(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x1234_5678);
    assert_eq!(mem.read_word(0x100).unwrap(), 0);
}

#[test]
fn test_amoand_preserves_bits() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0xFFFF_0000).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0x00FF_00FF;

    let instr = create_amo_instr(1, 2, 3, 0b00011, 0, 0);
    let result = exec_amoand(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0xFFFF_0000);
    assert_eq!(mem.read_word(0x100).unwrap(), 0x00FF_0000);
}

// ========================================
// AMOOR Tests (6 tests)
// ========================================

#[test]
fn test_amoor_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x0F).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0xF0;

    let instr = create_amo_instr(1, 2, 3, 0b00110, 0, 0);
    let result = exec_amoor(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x0F);
    assert_eq!(mem.read_word(0x100).unwrap(), 0xFF);
}

#[test]
fn test_amoor_zero_operand() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x1234_5678).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0;

    let instr = create_amo_instr(1, 2, 3, 0b00110, 0, 0);
    let result = exec_amoor(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x1234_5678);
    assert_eq!(mem.read_word(0x100).unwrap(), 0x1234_5678);
}

#[test]
fn test_amoor_all_ones() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x0000_0000).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0xFFFF_FFFF;

    let instr = create_amo_instr(1, 2, 3, 0b00110, 0, 0);
    let result = exec_amoor(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0);
    assert_eq!(mem.read_word(0x100).unwrap(), 0xFFFF_FFFF);
}

// ========================================
// AMOXOR Tests (6 tests)
// ========================================

#[test]
fn test_amoxor_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
    let mut mem = SimpleMemory::new(0x10000);

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

#[test]
fn test_amoxor_zero() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x1234_5678).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0;

    let instr = create_amo_instr(1, 2, 3, 0b00100, 0, 0);
    let result = exec_amoxor(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 0x1234_5678);
    assert_eq!(mem.read_word(0x100).unwrap(), 0x1234_5678);
}

// ========================================
// AMOMAX Tests (Signed) (6 tests)
// ========================================

#[test]
fn test_amomax_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
fn test_amomax_keeps_larger() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 100).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 50;

    let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
    let result = exec_amomax(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 100);
    assert_eq!(mem.read_word(0x100).unwrap(), 100); // Unchanged
}

#[test]
fn test_amomax_negative() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, (-10i32) as u32).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 5;

    let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
    let result = exec_amomax(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3] as i32, -10);
    assert_eq!(state.regs[3] as i32, -10); // -10 < 5, so 5 wins
    assert_eq!(mem.read_word(0x100).unwrap(), 5);
}

// ========================================
// AMOMIN Tests (Signed) (6 tests)
// ========================================

#[test]
fn test_amomin_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
fn test_amomin_keeps_smaller() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 50).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 100;

    let instr = create_amo_instr(1, 2, 3, 0b01000, 0, 0);
    let result = exec_amomin(&instr, &mut state, &mut mem);

    assert!(result.is_ok());
    assert_eq!(state.regs[3], 50);
    assert_eq!(mem.read_word(0x100).unwrap(), 50); // Unchanged
}

#[test]
fn test_amomin_negative() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
// AMOMAXU Tests (Unsigned) (4 tests)
// ========================================

#[test]
fn test_amomaxu_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
    let mut mem = SimpleMemory::new(0x10000);

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
// AMOMINU Tests (Unsigned) (4 tests)
// ========================================

#[test]
fn test_amominu_basic() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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
    let mut mem = SimpleMemory::new(0x10000);

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
// Combined Tests (2 tests)
// ========================================

#[test]
fn test_amo_comparison_signed_vs_unsigned() {
    // Signed: 0x8000_0000 is negative (-2147483648)
    // Unsigned: 0x8000_0000 is positive (2147483648)

    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

    mem.write_word(0x100, 0x8000_0000).unwrap();
    state.regs[1] = 0x100;
    state.regs[2] = 0x7FFF_FFFF;

    // Signed max: -2147483648 < 2147483647, so 0x7FFF_FFFF wins
    let instr = create_amo_instr(1, 2, 3, 0b01010, 0, 0);
    exec_amomax(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(mem.read_word(0x100).unwrap(), 0x7FFF_FFFF);

    // Reset
    mem.write_word(0x100, 0x8000_0000).unwrap();

    // Unsigned max: 2147483648 > 2147483647, so 0x8000_0000 wins
    let instr2 = create_amo_instr(1, 2, 3, 0b01011, 0, 0);
    exec_amomaxu(&instr2, &mut state, &mut mem).unwrap();
    assert_eq!(mem.read_word(0x100).unwrap(), 0x8000_0000);
}

#[test]
fn test_amo_all_return_original() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x10000);

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

    // AMOXOR
    mem.write_word(0x100, 0xFF).unwrap();
    state.regs[2] = 0x0F;
    let instr4 = create_amo_instr(1, 2, 6, 0b00100, 0, 0);
    exec_amoxor(&instr4, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[6], 0xFF);
}
