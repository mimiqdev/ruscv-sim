//! FPU Multiply-Add Tests
//!
//! Tests for FMADD.S, FMSUB.S, FNMSUB.S, and FNMADD.S operations

use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

fn create_test_state() -> CoreState {
    CoreState::default()
}

#[test]
fn test_fmadd_basic() {
    // (2.0 * 3.0) + 1.0 = 7.0
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(3.0));
    state.fpr.write(3, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: Some(3),
        rd: Some(4),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::madd::exec_fmadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(4).get();
    assert!((result - 7.0).abs() < 1e-5);
}

#[test]
fn test_fmsub_basic() {
    // (2.0 * 3.0) - 1.0 = 5.0
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(3.0));
    state.fpr.write(3, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: Some(3),
        rd: Some(4),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::madd::exec_fmsub_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(4).get();
    assert!((result - 5.0).abs() < 1e-5);
}

#[test]
fn test_fnmsub_basic() {
    // -(2.0 * 3.0) + 1.0 = -5.0
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(3.0));
    state.fpr.write(3, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: Some(3),
        rd: Some(4),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::madd::exec_fnmsub_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(4).get();
    assert!((result - (-5.0)).abs() < 1e-5);
}

#[test]
fn test_fnmadd_basic() {
    // -(2.0 * 3.0) - 1.0 = -7.0
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(2.0));
    state.fpr.write(2, Fpr::new(3.0));
    state.fpr.write(3, Fpr::new(1.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: Some(3),
        rd: Some(4),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::madd::exec_fnmadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(4).get();
    assert!((result - (-7.0)).abs() < 1e-5);
}

#[test]
fn test_fmadd_zero() {
    // (0.0 * 5.0) + 3.0 = 3.0
    let mut state = create_test_state();
    let mut mem = SimpleMemory::new(0x1000);

    state.fpr.write(1, Fpr::new(0.0));
    state.fpr.write(2, Fpr::new(5.0));
    state.fpr.write(3, Fpr::new(3.0));

    let decoded = DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(0),
        rs1: Some(1),
        rs2: Some(2),
        rs3: Some(3),
        rd: Some(4),
        imm: None,
        branch_taken: false,
    };

    ruscv_sim::isa::rv64f::madd::exec_fmadd_s(&decoded, &mut state, &mut mem).unwrap();

    let result = state.fpr.read(4).get();
    assert!((result - 3.0).abs() < 1e-5);
}
