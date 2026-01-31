//! FPU Comparison Tests
//!
//! Tests for FEQ.S, FLT.S, and FLE.S operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_feq_equal() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
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

    ruscv_sim::execute::exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 1);
}

#[test]
fn test_feq_not_equal() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
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

    ruscv_sim::execute::exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_flt_less() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(3.0));
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

    ruscv_sim::execute::exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 1);
}

#[test]
fn test_flt_greater() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
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

    ruscv_sim::execute::exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_fle_less_or_equal() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(3.0));
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

    ruscv_sim::execute::exec_fle_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 1);
}

#[test]
fn test_fle_equal() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(5.0));
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

    ruscv_sim::execute::exec_fle_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 1);
}

#[test]
fn test_feq_nan() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(f32::NAN));
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

    ruscv_sim::execute::exec_feq_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 0);
}

#[test]
fn test_flt_negative() {
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(-5.0));
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

    ruscv_sim::execute::exec_flt_s(&decoded, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[3], 1);
}
