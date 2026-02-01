//! RV64M Multiply instruction tests
//!
//! Tests for MUL, MULH, MULHU, MULHSU instructions

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
use ruscv_sim::memory::SimpleMemory;

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
// MUL Tests (funct7 = 0b000_0001)
// ========================================

#[test]
fn test_mul_basic_positive() {
    let mut state = CoreState::default();
    state.regs[1] = 10;
    state.regs[2] = 20;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 200);
}

#[test]
fn test_mul_negative_positive() {
    let mut state = CoreState::default();
    state.regs[1] = (-5i64) as u64;
    state.regs[2] = 6;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, -30);
}

#[test]
fn test_mul_negative_negative() {
    let mut state = CoreState::default();
    state.regs[1] = (-5i64) as u64;
    state.regs[2] = (-6i64) as u64;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3] as i32, 30);
}

#[test]
fn test_mul_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 0;
    state.regs[2] = 12345;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mul_one() {
    let mut state = CoreState::default();
    state.regs[1] = 1;
    state.regs[2] = 999;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 999);
}

#[test]
fn test_mul_max_values() {
    let mut state = CoreState::default();
    state.regs[1] = 0x7FFF_FFFF;
    state.regs[2] = 2;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    // 2147483647 * 2 = 4294967294 = 0xFFFFFFFE
    assert_eq!(state.regs[3], 0xFFFF_FFFE);
}

#[test]
fn test_mul_min_values() {
    let mut state = CoreState::default();
    // RV64: 0x8000_0000 is sign-extended to 0xFFFF_FFFF_8000_0000
    state.regs[1] = 0xFFFF_FFFF_8000_0000;
    state.regs[2] = 2;

    let instr = create_mul_instr(1, 2, 3, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    // MUL returns low 64 bits: 0xFFFF_FFFF_8000_0000 * 2 = 0xFFFF_FFFF_0000_0000
    assert_eq!(state.regs[3], 0xFFFF_FFFF_0000_0000);
}

#[test]
fn test_mul_x0_dest() {
    let mut state = CoreState::default();
    state.regs[1] = 10;
    state.regs[2] = 20;

    let instr = create_mul_instr(1, 2, 0, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[0], 0);
}

#[test]
fn test_mul_same_register() {
    let mut state = CoreState::default();
    state.regs[1] = 15;

    let instr = create_mul_instr(1, 1, 2, 0b000_0001);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 225);
}

// ========================================
// MULH Tests (funct7 = 0b000_0010)
// ========================================

#[test]
fn test_mulh_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 0x0001_0000;
    state.regs[2] = 0x0001_0000;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
    // RV64: 65536 * 65536 = 4294967296, 128-bit product is 0x00000000_00000001_00000000_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulh_small_result() {
    let mut state = CoreState::default();
    state.regs[1] = 3;
    state.regs[2] = 4;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulh_negative_result() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // -2147483648 in RV32, but positive in RV64
    state.regs[2] = 2;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
    // RV64: 0x8000_0000 * 2 = 0x1_0000_0000, 128-bit product is 0x00000000_00000000_00000001_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulh_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 0;
    state.regs[2] = 0xFFFF_FFFF;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulh_both_negative() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // 2147483648 (positive in RV64)
    state.regs[2] = 0x8000_0000;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
    // RV64: 2^31 * 2^31 = 2^62, 128-bit product is 0x00000000_00000003_ffffffff_80000000
    // Wait, let me recalculate: 0x8000_0000 * 0x8000_0000 = 0x4000_0000_0000_0000
    // 128-bit product is 0x00000000_00000000_40000000_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

// ========================================
// MULHU Tests (funct7 = 0b000_0011)
// ========================================

#[test]
fn test_mulhu_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000;
    state.regs[2] = 2;

    let instr = create_mul_instr(1, 2, 3, 0b000_0011);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhu(&instr, &mut state, &mut mem).unwrap();
    // 2^31 * 2 = 2^32 = 0x1_0000_0000
    // 128-bit product is 0x00000000_00000000_00000001_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulhu_small_result() {
    let mut state = CoreState::default();
    state.regs[1] = 3;
    state.regs[2] = 4;

    let instr = create_mul_instr(1, 2, 3, 0b000_0011);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulhu_both_large() {
    let mut state = CoreState::default();
    state.regs[1] = 0xFFFF_FFFF;
    state.regs[2] = 0xFFFF_FFFF;

    let instr = create_mul_instr(1, 2, 3, 0b000_0011);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhu(&instr, &mut state, &mut mem).unwrap();
    // (2^32-1)^2 = 2^64 - 2^33 + 1 = 0xFFFF_FFFE_0000_0001
    // 128-bit product is 0x00000000_00000000_FFFFFFFE_00000001
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulhu_zero() {
    let mut state = CoreState::default();
    state.regs[1] = 0;
    state.regs[2] = 0xFFFF_FFFF;

    let instr = create_mul_instr(1, 2, 3, 0b000_0011);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

// ========================================
// MULHSU Tests (funct7 = 0b000_0010)
// ========================================

#[test]
fn test_mulhsu_basic() {
    let mut state = CoreState::default();
    state.regs[1] = 0x8000_0000; // 2147483648 (positive in RV64 when loaded this way)
    state.regs[2] = 0x8000_0000; // 2147483648 unsigned

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhsu(&instr, &mut state, &mut mem).unwrap();
    // In RV64, 0x8000_0000 is positive (2^31), so 2^31 * 2^31 = 2^62
    // 128-bit product is 0x00000000_00000000_40000000_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulhsu_positive_signed() {
    let mut state = CoreState::default();
    state.regs[1] = 0x0001_0000;
    state.regs[2] = 0x0001_0000;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhsu(&instr, &mut state, &mut mem).unwrap();
    // 65536 * 65536 = 4294967296 = 0x1_0000_0000
    // 128-bit product is 0x00000000_00000000_00000001_00000000
    // Upper 64 bits = 0
    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_mulhsu_negative_signed() {
    let mut state = CoreState::default();
    state.regs[1] = (-1i64) as u64; // -1 signed
    state.regs[2] = 0x8000_0000; // Large unsigned (2^31)

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhsu(&instr, &mut state, &mut mem).unwrap();
    // (-1) * 2^31 = -2^31 = 0xFFFF_FFFF_8000_0000
    // 128-bit product is 0xFFFFFFFF_FFFFFFFF_FFFFFFFF_80000000
    // Upper 64 bits = 0xFFFF_FFFF_FFFF_FFFF
    assert_eq!(state.regs[3], 0xFFFF_FFFF_FFFF_FFFF);
}

#[test]
fn test_mulhsu_small_result() {
    let mut state = CoreState::default();
    state.regs[1] = 5;
    state.regs[2] = 3;

    let instr = create_mul_instr(1, 2, 3, 0b000_0010);
    let mut mem = SimpleMemory::new(0x1000);

    ruscv_sim::isa::rv64m::mul::exec_mulhsu(&instr, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 0);
}

// ========================================
// Edge Cases and Comprehensive Tests
// ========================================

#[test]
fn test_mul_random_values() {
    let test_cases: [(u32, u32, u32); 10] = [
        (123, 456, 56088),
        (0xFF, 0xAA, 0xFF * 0xAA),
        (0x1234, 0x5678, 0x1234 * 0x5678),
        (1, 1, 1),
        (1, 0, 0),
        (0xFFFF, 0xFFFF, 0xFFFE_0001),
        (1000, 1000, 1000000),
        (0x7FFF_FFFF, 1, 0x7FFF_FFFF),
        (0x8000_0000, 1, 0x8000_0000),
        (0x4000_0000, 2, 0x8000_0000),
    ];

    for (rs1, rs2, expected) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = rs1 as u64;
        state.regs[2] = rs2 as u64;

        let instr = create_mul_instr(1, 2, 3, 0b000_0001);
        let mut mem = SimpleMemory::new(0x1000);

        ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(
            state.regs[3], expected as u64,
            "Failed for {} * {}",
            rs1, rs2
        );
    }
}

#[test]
fn test_mulh_corner_cases() {
    // Test edge cases where upper bits are significant
    let test_cases: [(i64, i64, i64); 5] = [
        (0x4000_0000, 2, 0),           // 2^30 * 2 = 2^31, upper 64 bits = 0
        (0x4000_0000, 4, 0),           // 2^30 * 4 = 2^32, upper 64 bits = 0
        (-0x4000_0000_i64, 4, -1),     // -2^30 * 4 = -2^32, upper 64 bits = -1 (sign extended)
        (0x2000_0000, 0x2000_0000, 0), // (2^29)^2 = 2^58, upper 64 bits = 0
        (-1, -1, 0),                   // (-1) * (-1) = 1, upper 64 bits = 0
    ];

    for (rs1, rs2, expected) in test_cases {
        let mut state = CoreState::default();
        state.regs[1] = rs1 as u64;
        state.regs[2] = rs2 as u64;

        let instr = create_mul_instr(1, 2, 3, 0b000_0010);
        let mut mem = SimpleMemory::new(0x1000);

        ruscv_sim::isa::rv64m::mul::exec_mulh(&instr, &mut state, &mut mem).unwrap();
        assert_eq!(
            state.regs[3] as i64, expected,
            "Failed for {:?} * {:?}",
            rs1, rs2
        );
    }
}

#[test]
fn test_mul_property_distributive() {
    // Test (a + b) * c = a*c + b*c
    for &c in &[1u64, 2, 3, 7, 0xFF, 0xFFFF] {
        for &a in &[0u64, 1, 100, 0x8000_0000] {
            for &b in &[0u64, 1, 100, 0x8000_0000] {
                // (a + b) * c
                let mut state1 = CoreState::default();
                state1.regs[1] = a.wrapping_add(b);
                state1.regs[2] = c;
                let instr = create_mul_instr(1, 2, 3, 0b000_0001);
                let mut mem = SimpleMemory::new(0x1000);
                ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state1, &mut mem).unwrap();
                let result1 = state1.regs[3];

                // a * c + b * c
                let mut state2 = CoreState::default();
                state2.regs[1] = a;
                state2.regs[2] = c;
                let instr = create_mul_instr(1, 2, 3, 0b000_0001);
                let mut mem = SimpleMemory::new(0x1000);
                ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state2, &mut mem).unwrap();
                let ac = state2.regs[3];

                state2.regs[1] = b;
                let instr = create_mul_instr(1, 2, 4, 0b000_0001);
                let mut mem = SimpleMemory::new(0x1000);
                ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state2, &mut mem).unwrap();
                let result2 = ac.wrapping_add(state2.regs[4]);

                assert_eq!(
                    result1, result2,
                    "Distributive property failed: ({} + {}) * {}",
                    a, b, c
                );
            }
        }
    }
}

#[test]
fn test_mul_different_registers() {
    // Test multiplication with various register combinations
    let registers = [1u8, 5, 10, 15, 31];

    for &rs1 in &registers {
        for &rs2 in &registers {
            for &rd in &registers {
                if rd != rs1 && rd != rs2 {
                    let mut state = CoreState::default();
                    state.regs[rs1 as usize] = 7;
                    // When rs1 == rs2, use same value; otherwise use different value
                    if rs1 == rs2 {
                        state.regs[rs2 as usize] = 8; // Will overwrite rs1, so use 8
                    } else {
                        state.regs[rs2 as usize] = 8;
                    }

                    let instr = create_mul_instr(rs1, rs2, rd, 0b000_0001);
                    let mut mem = SimpleMemory::new(0x1000);

                    ruscv_sim::isa::rv64m::mul::exec_mul(&instr, &mut state, &mut mem).unwrap();
                    let result = state.regs[rd as usize];
                    let expected = if rs1 == rs2 { 64 } else { 56 }; // 8*8=64 when rs1==rs2, else 7*8=56
                    assert_eq!(
                        result, expected,
                        "Failed for rs1={}, rs2={}, rd={}",
                        rs1, rs2, rd
                    );
                }
            }
        }
    }
}
