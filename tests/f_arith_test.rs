//! FPU Arithmetic Tests
//!
//! Tests for FADD.S, FSUB.S, and FMUL.S floating-point arithmetic operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_fadd_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(1.5));
    state.fpr.write(2, Fpr::new(2.5));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 4.0).abs() < 1e-6);
}

#[test]
fn test_fadd_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-3.0));
    state.fpr.write(2, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - (-2.0)).abs() < 1e-6);
}

#[test]
fn test_fadd_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
    state.fpr.write(2, Fpr::new(0.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 5.0).abs() < 1e-6);
}

#[test]
fn test_fsub_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(10.0));
    state.fpr.write(2, Fpr::new(3.5));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fsub_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 6.5).abs() < 1e-6);
}

#[test]
fn test_fsub_negative_result() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(5.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fsub_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - (-3.0)).abs() < 1e-6);
}

#[test]
fn test_fmul_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(4.0));
    state.fpr.write(2, Fpr::new(2.5));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 10.0).abs() < 1e-6);
}

#[test]
fn test_fmul_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(0.0));
    state.fpr.write(2, Fpr::new(999.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert_eq!(result, 0.0);
}

#[test]
fn test_fmul_by_one() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(7.0));
    state.fpr.write(2, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 7.0).abs() < 1e-6);
}

#[test]
fn test_fadd_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(f32::INFINITY));
    state.fpr.write(2, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert_eq!(result, f32::INFINITY);
}

#[test]
fn test_fsub_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(f32::INFINITY));
    state.fpr.write(2, Fpr::new(f32::INFINITY));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fsub_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!(result.is_nan());
}

#[test]
fn test_fmul_infinity() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(f32::INFINITY));
    state.fpr.write(2, Fpr::new(2.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert_eq!(result, f32::INFINITY);
}

#[test]
fn test_result_nan_boxing() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(3.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3);
    assert!(result.is_nan_boxed());
}

#[test]
fn test_chained_operations() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    // ((1 + 2) * 3) - 4 = 5
    state.fpr.write(1, Fpr::new(1.0));
    state.fpr.write(2, Fpr::new(2.0));
    state.fpr.write(3, Fpr::new(3.0));
    state.fpr.write(4, Fpr::new(4.0));

    // Add
    let add_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(5),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&add_dec, &mut state, &mut mem).unwrap();

    // Multiply
    let mul_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(5),
        rs2: Some(3),
        rs3: None,
        rd: Some(6),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&mul_dec, &mut state, &mut mem).unwrap();

    // Subtract
    let sub_dec = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(6),
        rs2: Some(4),
        rs3: None,
        rd: Some(7),
        imm: None,
        branch_taken: false,
    };
    ruscv_sim::isa::rv64f::arith::exec_fsub_s(&sub_dec, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(7).get();
    assert!((result - 5.0).abs() < 1e-5);
}

#[test]
fn test_small_decimal_values() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(0.1));
    state.fpr.write(2, Fpr::new(0.2));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 0.3).abs() < 1e-6);
}

#[test]
fn test_negative_multiply() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-2.0));
    state.fpr.write(2, Fpr::new(3.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::arith::exec_fmul_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - (-6.0)).abs() < 1e-6);
}
