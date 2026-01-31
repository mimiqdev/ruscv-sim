//! RV64M Divide instruction tests
//!
//! Tests for DIV, DIVU, REM, REMU instructions

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
use ruscv_sim::memory::SimpleMemory;

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
// DIV Tests (funct7 = 0b000_0001)
// ========================================

#[test]
fn test_div_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 10;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 10);
}

#[test]
fn test_div_negative_positive() {
    let mut state = CoreState::default();
    state.regs[1] = (-100i32) as u32;
    state.regs[2] = 10;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, -10);
}

#[test]
fn test_div_positive_negative() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = (-10i32) as u32;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, -10);
}

#[test]
fn test_div_negative_negative() {
    let mut state = CoreState::default();
    state.regs[1] = (-100i32) as u32;
    state.regs[2] = (-10i32) as u32;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, 10);
}

#[test]
fn test_div_by_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 0;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones
}

#[test]
fn test_div_overflow() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // MIN i32
    state.regs[2] = 0xFFFF_FFFF; // -1

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones (overflow)
}

#[test]
fn test_div_one() {
    let mut state = CoreState::default();
    state.regs[1] = 12345;
    state.regs[2] = 1;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 12345);
}

#[test]
fn test_div_self() {
    let mut state = CoreState::default();
    state.regs[1] = 42;

    let instr = create_div_instr(1, 1, 2, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 1);
}

#[test]
fn test_div_remainder_truncated() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 30;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 3); // 100 / 30 = 3.33... truncated to 3
}

// ========================================
// DIVU Tests (funct7 = 0b000_0001)
// ========================================

#[test]
fn test_divu_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 10;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 10);
}

#[test]
fn test_divu_large_unsigned() {
    let mut state = CoreState::default();
    state.regs[1] = 0xFFFF_FFFF;
    state.regs[2] = 2;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0x7FFF_FFFF);
}

#[test]
fn test_divu_by_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 0;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0xFFFF_FFFF); // All ones
}

#[test]
fn test_divu_high_bit_set() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // 2^31
    state.regs[2] = 2;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0x4000_0000); // 2^30
}

#[test]
fn test_divu_small_by_large() {
    let mut state = CoreState::default();
    state.regs[1] = 1;
    state.regs[2] = 0x8000_0000;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

// ========================================
// REM Tests (funct7 = 0b000_0001)
// ========================================

#[test]
fn test_rem_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 30;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 10); // 100 % 30 = 10
}

#[test]
fn test_rem_negative_positive() {
    let mut state = CoreState::default();
    state.regs[1] = (-100i32) as u32;
    state.regs[2] = 30;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, -10); // (-100) % 30 = -10
}

#[test]
fn test_rem_positive_negative() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = (-30i32) as u32;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, 10); // 100 % (-30) = 10
}

#[test]
fn test_rem_negative_negative() {
    let mut state = CoreState::default();
    state.regs[1] = (-100i32) as u32;
    state.regs[2] = (-30i32) as u32;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, -10); // (-100) % (-30) = -10
}

#[test]
fn test_rem_by_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 0;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 100); // Dividend unchanged
}

#[test]
fn test_rem_overflow() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // MIN i32
    state.regs[2] = 0xFFFF_FFFF; // -1

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0); // Remainder is 0
}

#[test]
fn test_rem_exact_division() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 10;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0); // 100 % 10 = 0
}

// ========================================
// REMU Tests (funct7 = 0b000_0001)
// ========================================

#[test]
fn test_remu_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 30;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_remu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 10);
}

#[test]
fn test_remu_large_unsigned() {
    let mut state = CoreState::default();
    state.regs[1] = 0xFFFF_FFFF;
    state.regs[2] = 1000;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_remu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0xFFFF_FFFF % 1000);
}

#[test]
fn test_remu_by_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 0;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_remu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 100); // Dividend unchanged
}

#[test]
fn test_remu_high_bit_set() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // 2^31
    state.regs[2] = 1000;

    let instr = create_div_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_remu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0x8000_0000 % 1000);
}

// ========================================
// Property Tests
// ========================================

#[test]
fn test_div_mul_property() {
    // Test that (a / b) * b + (a % b) = a
    let test_cases = [
        (100i32, 30i32),
        (100i32, -30i32),
        (-100i32, 30i32),
        (-100i32, -30i32),
        (100i32, 7i32),
        (0i32, 5i32),
        (1i32, 1i32),
        (1i32, -1i32),
        (7i32, 1i32),
    ];

    for (a, b) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = a as u32;
        state.regs[2] = b as u32;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
        let div_result = state.regs[3] as i32;

        let instr = create_div_instr(1, 2, 4, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
        let rem_result = state.regs[4] as i32;

        let computed = div_result.wrapping_mul(b).wrapping_add(rem_result);
        assert_eq!(
            computed, a,
            "Property failed: ({} / {}) * {} + ({} % {}) = {}",
            a, b, b, a, b, computed
        );
    }
}

#[test]
fn test_divu_remu_property() {
    // Test that (a / b) * b + (a % b) = a for unsigned
    let test_cases = [
        (100u32, 30u32),
        (0xFFFF_FFFFu32, 1000u32),
        (0x8000_0000u32, 2u32),
        (1u32, 1u32),
        (1u32, 2u32),
    ];

    for (a, b) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = a;
        state.regs[2] = b;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        ruscv_sim::execute::div::exec_divu(&instr, &mut state, &mut mem).unwrap();
        let div_result = state.regs[3];

        let instr = create_div_instr(1, 2, 4, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        ruscv_sim::execute::div::exec_remu(&instr, &mut state, &mut mem).unwrap();
        let rem_result = state.regs[4];

        let computed = div_result.wrapping_mul(b).wrapping_add(rem_result);
        assert_eq!(
            computed, a,
            "Property failed: ({} / {}) * {} + ({} % {}) = {}",
            a, b, b, a, b, computed
        );
    }
}

#[test]
fn test_div_edge_cases() {
    // Test various edge cases
    let test_cases: Vec<(i32, i32)> = vec![
        (i32::MAX, 1),
        (i32::MIN, -1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (-1, 1),
        (10, -1),
        (0, 1),
        (0, -1),
        (i32::MAX, i32::MAX),
        (i32::MIN, i32::MIN),
    ];

    for (a, b) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = a as u32;
        state.regs[2] = b as u32;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        let result = ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem);

        if b == 0 || (a == i32::MIN && b == -1) {
            // Division by zero or overflow
            assert!(result.is_ok());
            assert_eq!(state.regs[3], 0xFFFF_FFFF);
        } else {
            assert!(result.is_ok());
            let expected = a / b;
            assert_eq!(
                state.regs[3] as i32, expected,
                "Division failed: {} / {}",
                a, b
            );
        }
    }
}

#[test]
fn test_rem_edge_cases() {
    // Test edge cases for REM
    let test_cases: Vec<(i32, i32)> = vec![
        (i32::MAX, 1),
        (i32::MIN, -1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (-1, 1),
        (10, -1),
        (0, 1),
        (0, -1),
        (i32::MAX, i32::MAX),
        (i32::MIN, i32::MIN),
    ];

    for (a, b) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = a as u32;
        state.regs[2] = b as u32;

        let instr = create_div_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);
        let result = ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem);

        if b == 0 {
            // Division by zero - remainder is dividend
            assert!(result.is_ok());
            assert_eq!(state.regs[3], a as u32);
        } else if a == i32::MIN && b == -1 {
            // Overflow case
            assert!(result.is_ok());
            assert_eq!(state.regs[3], 0);
        } else {
            assert!(result.is_ok());
            let expected = a % b;
            assert_eq!(
                state.regs[3] as i32, expected,
                "Remainder failed: {} % {}",
                a, b
            );
        }
    }
}

#[test]
fn test_div_x0_dest() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 10;

    let instr = create_div_instr(1, 2, 0, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_div(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[0], 0);
}

#[test]
fn test_rem_x0_dest() {
    let mut state = CoreState::default();
    state.regs[1] = 100;
    state.regs[2] = 30;

    let instr = create_div_instr(1, 2, 0, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::execute::div::exec_rem(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[0], 0);
}
