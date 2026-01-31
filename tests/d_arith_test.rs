//! D Extension Arithmetic Tests
//!
//! Tests for FADD.D, FSUB.D, and FMUL.D double-precision floating-point
//! arithmetic operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_fadd_d_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state
        .fpr
        .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));
    state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - (std::f64::consts::PI + 1.0)).abs() < 1e-10);
}

#[test]
fn test_fadd_d_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-3.0f64).to_bits()));
    state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - (-2.0)).abs() < 1e-10);
}

#[test]
fn test_fadd_d_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(5.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(0.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 5.0).abs() < 1e-10);
}

#[test]
fn test_fsub_d_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(10.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(3.5f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x05),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fsub_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 6.5).abs() < 1e-10);
}

#[test]
fn test_fsub_d_negative_result() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(5.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x05),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fsub_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - (-3.0)).abs() < 1e-10);
}

#[test]
fn test_fmul_d_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(4.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(2.5f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 10.0).abs() < 1e-10);
}

#[test]
fn test_fmul_d_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(0.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(999.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert_eq!(result, 0.0);
}

#[test]
fn test_fmul_d_by_one() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(7.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 7.0).abs() < 1e-10);
}

#[test]
fn test_fadd_d_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
    state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert_eq!(result, f64::INFINITY);
}

#[test]
fn test_fsub_d_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
    state.fpr.write(2, Fpr::from_bits(f64::INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x05),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fsub_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!(result.is_nan());
}

#[test]
fn test_fmul_d_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
    state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert_eq!(result, f64::INFINITY);
}

#[test]
fn test_fmul_d_negative_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state
        .fpr
        .write(1, Fpr::from_bits(f64::NEG_INFINITY.to_bits()));
    state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert_eq!(result, f64::NEG_INFINITY);
}

#[test]
fn test_fadd_d_negative_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-0.0f64).to_bits()));
    state.fpr.write(2, Fpr::from_bits(1.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn test_fsub_d_negative_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
    state.fpr.write(2, Fpr::from_bits(f64::INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x05),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fsub_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!(result.is_nan());
}

#[test]
fn test_fadd_d_opposite_infinities() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));
    state
        .fpr
        .write(2, Fpr::from_bits(f64::NEG_INFINITY.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!(result.is_nan());
}

#[test]
fn test_result_stored_as_double() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3);
    // Result should be a valid double (no NaN boxing needed)
    let result_f64 = f64::from_bits(result.bits());
    assert!((result_f64 - 5.0).abs() < 1e-10);
}

#[test]
fn test_chained_operations() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // ((1.0 + 2.0) * 3.0) - 4.0 = 5.0
    state.fpr.write(1, Fpr::from_bits(1.0f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(2.0f64.to_bits()));
    state.fpr.write(3, Fpr::from_bits(3.0f64.to_bits()));
    state.fpr.write(4, Fpr::from_bits(4.0f64.to_bits()));

    // Add
    let add_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(5),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::execute::exec_fadd_d(&add_dec, &mut state, &mut mem).unwrap();

    // Multiply
    let mul_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(5),
        rs2: Some(3),
        rs3: None,
        rd: Some(6),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::execute::exec_fmul_d(&mul_dec, &mut state, &mut mem).unwrap();

    // Subtract
    let sub_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x05),
        rs1: Some(6),
        rs2: Some(4),
        rs3: None,
        rd: Some(7),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::execute::exec_fsub_d(&sub_dec, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(7).bits());
    assert!((result - 5.0).abs() < 1e-10);
}

#[test]
fn test_small_decimal_values() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits(0.1f64.to_bits()));
    state.fpr.write(2, Fpr::from_bits(0.2f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x01),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fadd_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - 0.3).abs() < 1e-15);
}

#[test]
fn test_negative_multiply() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((-2.0f64).to_bits()));
    state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert!((result - (-6.0)).abs() < 1e-10);
}

#[test]
fn test_very_large_numbers() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((1e200f64).to_bits()));
    state.fpr.write(2, Fpr::from_bits((1e200f64).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    assert_eq!(result, f64::INFINITY);
}

#[test]
fn test_very_small_numbers() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::from_bits((1e-200f64).to_bits()));
    state.fpr.write(2, Fpr::from_bits((1e-200f64).to_bits()));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x09),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute::exec_fmul_d(&decoded, &mut state, &mut mem).unwrap();

    let result = f64::from_bits(state.fpr.read(3).bits());
    // 1e-200 * 1e-200 = 1e-400, which underflows to 0 (smallest f64 is ~1e-324)
    assert!(result == 0.0 || result.is_subnormal());
}
