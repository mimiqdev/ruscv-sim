//! D Extension Conversion Tests
//!
//! Tests for FCVT floating-point conversion operations between
//! double-precision, single-precision, and integer types

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

// ===== FCVT.D.S Tests (Single to Double) =====

#[test]
fn test_fcvt_d_s_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(std::f32::consts::PI));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_s(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - std::f64::consts::PI).abs() < 1e-5);
}

#[test]
fn test_fcvt_d_s_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(0.0f32));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_s(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert_eq!(result, 0.0);
}

#[test]
fn test_fcvt_d_s_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-5.0f32));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_s(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - (-5.0)).abs() < 1e-5);
}

// ===== FCVT.S.D Tests (Double to Single) =====

#[test]
fn test_fcvt_s_d_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state
        .fpr
        .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x40),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!((result - std::f32::consts::PI).abs() < 1e-5);
}

#[test]
fn test_fcvt_s_d_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(0.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x40),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert_eq!(result, 0.0f32);
}

#[test]
fn test_fcvt_s_d_precision_loss() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // PI to more precision than f32 can hold
    state
        .fpr
        .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x40),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    // Converting back to f64 should show precision loss
    let back_to_f64 = result as f64;
    let original = std::f64::consts::PI;
    assert!((back_to_f64 - original).abs() > 1e-15);
}

#[test]
fn test_fcvt_s_d_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x40),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert_eq!(result, f32::INFINITY);
}

// ===== FCVT.D.W Tests (Signed Word to Double) =====

#[test]
fn test_fcvt_d_w_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 42;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_w(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - 42.0).abs() < 1e-10);
}

#[test]
fn test_fcvt_d_w_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0xFFFFFFFEu32; // -2 as i32

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_w(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - (-2.0)).abs() < 1e-10);
}

#[test]
fn test_fcvt_d_w_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_w(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert_eq!(result, 0.0);
}

// ===== FCVT.W.D Tests (Double to Signed Word) =====

#[test]
fn test_fcvt_w_d_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(42.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 42);
}

#[test]
fn test_fcvt_w_d_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-10.0f64).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2] as i32, -10);
}

#[test]
fn test_fcvt_w_d_nan() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x7FFFFFFF);
}

#[test]
fn test_fcvt_w_d_positive_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x7FFFFFFF);
}

#[test]
fn test_fcvt_w_d_negative_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state
        .fpr
        .write(1, Fpr::from_bits(f64::NEG_INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x80000000);
}

#[test]
fn test_fcvt_w_d_overflow() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // Value larger than i32::MAX
    state
        .fpr
        .write(1, Fpr::from_bits((i32::MAX as f64 + 1e10).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x7FFFFFFF);
}

#[test]
fn test_fcvt_w_d_underflow() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // Value smaller than i32::MIN
    state
        .fpr
        .write(1, Fpr::from_bits((i32::MIN as f64 - 1e10).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(1),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x80000000);
}

// ===== FCVT.D.WU Tests (Unsigned Word to Double) =====

#[test]
fn test_fcvt_d_wu_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 100;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(4),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_wu(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - 100.0).abs() < 1e-10);
}

#[test]
fn test_fcvt_d_wu_large() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0xFFFFFFFFu32; // Max u32

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(4),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_wu(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(2).bits());
    assert!((result - 4294967295.0).abs() < 1.0);
}

// ===== FCVT.WU.D Tests (Double to Unsigned Word) =====

#[test]
fn test_fcvt_wu_d_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(100.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], 100);
}

#[test]
fn test_fcvt_wu_d_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-1.0f64).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], u32::MAX);
}

#[test]
fn test_fcvt_wu_d_nan() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], u32::MAX);
}

#[test]
fn test_fcvt_wu_d_overflow() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // Value larger than u32::MAX
    state
        .fpr
        .write(1, Fpr::from_bits((u32::MAX as f64 + 1e10).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[3], u32::MAX);
}

// ===== FCVT.D.L Tests (Signed Long to Double) =====

#[test]
fn test_fcvt_d_l_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0x00000000;
    state.regs[2] = 0x00000001; // 2^32 as i64

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(5),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_l(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 4294967296.0).abs() < 1.0);
}

#[test]
fn test_fcvt_d_l_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0xFFFFFFFEu32;
    state.regs[2] = 0xFFFFFFFFu32; // -2 as i64

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(5),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_l(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - (-2.0)).abs() < 1.0);
}

// ===== FCVT.L.D Tests (Double to Signed Long) =====

#[test]
fn test_fcvt_l_d_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // Store 2^32 = 4294967296.0 as proper f64 bits
    state
        .fpr
        .write(1, Fpr::from_bits(4294967296.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(5),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_l_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x00000000); // Low 32 bits of 2^32
    assert_eq!(state.regs[3], 0x00000001); // High 32 bits of 2^32
}

#[test]
fn test_fcvt_l_d_nan() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(5),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_l_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x7FFFFFFF);
    assert_eq!(state.regs[3], 0x7FFFFFFF);
}

// ===== FCVT.D.LU Tests (Unsigned Long to Double) =====

#[test]
fn test_fcvt_d_lu_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0x00000000;
    state.regs[2] = 0x00000001; // 2^32 as u64

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(6),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_d_lu(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 4294967296.0).abs() < 1.0);
}

// ===== FCVT.LU.D Tests (Double to Unsigned Long) =====

#[test]
fn test_fcvt_lu_d_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // Store 2^32 = 4294967296.0 as proper f64 bits
    state
        .fpr
        .write(1, Fpr::from_bits(4294967296.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(6),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_lu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], 0x00000000); // Low 32 bits of 2^32
    assert_eq!(state.regs[3], 0x00000001); // High 32 bits of 2^32
}

#[test]
fn test_fcvt_lu_d_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-1.0f64).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x41),
        rs1: Some(1),
        rs2: Some(6),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fcvt_lu_d(&decoded, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[2], u32::MAX);
    assert_eq!(state.regs[3], u32::MAX);
}
