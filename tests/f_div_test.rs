//! FPU Division Tests
//!
//! Tests for FDIV.S and FSQRT.S floating-point operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_fdiv_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(10.0));
    state.fpr.write(2, Fpr::new(2.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x0C),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fdiv_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!((result - 5.0).abs() < 1e-5);
}

#[test]
fn test_fdiv_by_zero() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
    state.fpr.write(2, Fpr::new(0.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x0C),
        rs1: Some(1),
        rs2: Some(2),
        rs3: None,
        rd: Some(3),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fdiv_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(3).get();
    assert!(result.is_infinite());
}

#[test]
fn test_fsqrt_basic() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(16.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x2C),
        rs1: Some(1),
        rs2: Some(0),
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fsqrt_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!((result - 4.0).abs() < 1e-5);
}

#[test]
fn test_fsqrt_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-4.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0x2C),
        rs1: Some(1),
        rs2: Some(0),
        rd: Some(2),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::execute_fsqrt_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(2).get();
    assert!(result.is_nan());
}
