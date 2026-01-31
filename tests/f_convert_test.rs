//! FPU Conversion Tests
//!
//! Tests for FCVT floating-point conversion operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_fcvt_s_w_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 42;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_s_w(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!((result - 42.0).abs() < 1e-5);
}

#[test]
fn test_fcvt_s_w_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0xFFFFFFFEu32; // -2 as i32

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_s_w(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!((result - (-2.0)).abs() < 1e-5);
}

#[test]
fn test_fcvt_w_s_positive() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.5));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_w_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[2], 5);
}

#[test]
fn test_fcvt_w_s_nan() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(f32::NAN));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_w_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[2], 0x7FFFFFFF);
}

#[test]
fn test_fcvt_wu_s_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_wu_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[2], u32::MAX);
}

#[test]
fn test_fcvt_s_wu() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 100;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_s_wu(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!((result - 100.0).abs() < 1e-5);
}

#[test]
fn test_fcvt_s_w_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.regs[1] = 0;

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_s_w(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert_eq!(result, 0.0);
}

#[test]
fn test_fcvt_w_s_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(0.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(0),
        rs3: None,
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fcvt_w_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[2], 0);
}
