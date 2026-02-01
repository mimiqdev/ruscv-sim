//! RV64C Compressed Instruction Extension Integration Tests
//!
//! This module tests the complete RV64C compressed instruction functionality
//! including decoding and execution.

use ruscv_sim::core::CoreState;
use ruscv_sim::fpu::Fpr;
use ruscv_sim::isa::rv64c::{
    exec_c_add, exec_c_addi, exec_c_addi16sp, exec_c_addi4spn, exec_c_addiw, exec_c_addw,
    exec_c_and, exec_c_andi, exec_c_beqz, exec_c_bnez, exec_c_ebreak, exec_c_fld, exec_c_flw,
    exec_c_fsd, exec_c_fsw, exec_c_j, exec_c_jalr, exec_c_jr, exec_c_ld, exec_c_ldsp, exec_c_li,
    exec_c_lui, exec_c_lw, exec_c_mv, exec_c_nop, exec_c_or, exec_c_sd, exec_c_sdsp, exec_c_slli,
    exec_c_srai, exec_c_srli, exec_c_sub, exec_c_subw, exec_c_sw, exec_c_xor, CompressedDecoder,
};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};

fn setup_test() -> (CoreState, SimpleMemory) {
    (CoreState::default(), SimpleMemory::new(0x20000))
}

#[test]
fn test_compressed_decoder_creation() {
    let _decoder = CompressedDecoder::new();
    // Test basic compressed instruction detection
    assert!(CompressedDecoder::is_compressed(0x0000));
    assert!(CompressedDecoder::is_compressed(0x0001));
    assert!(CompressedDecoder::is_compressed(0x0002));
    assert!(!CompressedDecoder::is_compressed(0x0003));
}

#[test]
fn test_instruction_length() {
    assert_eq!(CompressedDecoder::instruction_length(0x0000), 16);
    assert_eq!(CompressedDecoder::instruction_length(0x0001), 16);
    assert_eq!(CompressedDecoder::instruction_length(0x0002), 16);
    assert_eq!(CompressedDecoder::instruction_length(0x0003), 32);
}

#[test]
fn test_decode_c_addi() {
    let decoder = CompressedDecoder::new();
    // C.ADDI x5, 10: funct3=000, rd=00101, nzimm=01010, quadrant=01
    // Binary: 000_0_00101_01010_01 = 0x02A5
    let inst: u16 = 0b0000_0010_1010_1001;
    let decoded = decoder.decode_16bit(inst).unwrap();

    assert_eq!(decoded.rd, Some(5));
    assert_eq!(decoded.rs1, Some(5));
}

#[test]
fn test_decode_c_lw() {
    let decoder = CompressedDecoder::new();
    // C.LW x8, 4(x9): funct3=010, rs1'=01001, rd'=000, offset
    let inst: u16 = 0b0100_0100_1000_0000;
    let result = decoder.decode_16bit(inst);
    assert!(result.is_ok());
}

#[test]
fn test_c_integration_addi_li() {
    let (mut state, _mem) = setup_test();

    // Set up initial value
    state.regs[5] = 100;

    // C.ADDI x5, 20 -> x5 = 120
    exec_c_addi(5, 20, &mut state).unwrap();
    assert_eq!(state.regs[5], 120);

    // C.LI x6, 30 -> x6 = 30 (valid 6-bit signed immediate range: -32 to 31)
    exec_c_li(6, 30, &mut state).unwrap();
    assert_eq!(state.regs[6], 30);

    // C.ADD x5, x6 -> x5 = 150
    exec_c_add(5, 6, &mut state).unwrap();
    assert_eq!(state.regs[5], 150);
}

#[test]
fn test_c_integration_arithmetic_sequence() {
    let (mut state, _mem) = setup_test();

    // Initialize registers
    state.regs[8] = 100;
    state.regs[9] = 30;

    // C.SUB x8, x9 -> x8 = 70
    exec_c_sub(8, 9, &mut state).unwrap();
    assert_eq!(state.regs[8], 70);

    // C.ADDI x8, 10 -> x8 = 80
    exec_c_addi(8, 10, &mut state).unwrap();
    assert_eq!(state.regs[8], 80);

    // C.ANDI x8, 0x0F -> x8 = 0
    exec_c_andi(8, 0x0F, &mut state).unwrap();
    assert_eq!(state.regs[8], 0);
}

#[test]
fn test_c_integration_logic_operations() {
    let (mut state, _mem) = setup_test();

    state.regs[10] = 0xFF00;
    state.regs[11] = 0x0F0F;

    // C.XOR x10, x11 -> x10 = 0xF00F
    exec_c_xor(10, 11, &mut state).unwrap();
    assert_eq!(state.regs[10], 0xF00F);

    // C.OR x10, x11 -> x10 = 0xFF0F
    exec_c_or(10, 11, &mut state).unwrap();
    assert_eq!(state.regs[10], 0xFF0F);

    // C.AND x10, x11 -> x10 = 0x0F0F
    exec_c_and(10, 11, &mut state).unwrap();
    assert_eq!(state.regs[10], 0x0F0F);
}

#[test]
fn test_c_integration_shift_operations() {
    let (mut state, _mem) = setup_test();

    state.regs[5] = 0x00000000_000000FF;

    // C.SLLI x5, 8 -> x5 = 0xFF00
    exec_c_slli(5, 8, &mut state).unwrap();
    assert_eq!(state.regs[5], 0xFF00);

    // C.SRLI x5, 4 -> x5 = 0x0FF0
    exec_c_srli(5, 4, &mut state).unwrap();
    assert_eq!(state.regs[5], 0x0FF0);

    // C.SRAI x5, 4 -> x5 = 0x00FF
    exec_c_srai(5, 4, &mut state).unwrap();
    assert_eq!(state.regs[5], 0x00FF);
}

#[test]
fn test_c_integration_load_store() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;
    state.regs[9] = 0xDEADBEEF;

    // C.SW x9, 4(x8) -> mem[0x1004] = 0xDEADBEEF
    exec_c_sw(8, 9, 4, &mut state, &mut mem).unwrap();
    assert_eq!(mem.read_word(0x1004).unwrap(), 0xDEADBEEF);

    // C.LW x10, 4(x8) -> x10 = sign_extend(0xDEADBEEF)
    exec_c_lw(10, 8, 4, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[10], 0xFFFFFFFF_DEADBEEF);
}

#[test]
fn test_c_integration_load_store_double() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;
    state.regs[9] = 0x123456789ABCDEF0;

    // C.SD x9, 8(x8) -> mem[0x1008] = 0x123456789ABCDEF0
    exec_c_sd(8, 9, 8, &mut state, &mut mem).unwrap();
    assert_eq!(mem.read_dword(0x1008).unwrap(), 0x123456789ABCDEF0);

    // C.LD x10, 8(x8) -> x10 = 0x123456789ABCDEF0
    exec_c_ld(10, 8, 8, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[10], 0x123456789ABCDEF0);
}

#[test]
fn test_c_integration_stack_operations() {
    let (mut state, mut mem) = setup_test();

    // Set up stack pointer
    state.regs[2] = 0x2000;
    state.regs[5] = 0x123456789ABCDEF0;

    // C.SDSP x5, 8 -> mem[SP + 8] = x5
    exec_c_sdsp(5, 8, &mut state, &mut mem).unwrap();
    assert_eq!(mem.read_dword(0x2008).unwrap(), 0x123456789ABCDEF0);

    // C.LDSP x6, 8 -> x6 = mem[SP + 8]
    exec_c_ldsp(6, 8, &mut state, &mut mem).unwrap();
    assert_eq!(state.regs[6], 0x123456789ABCDEF0);
}

#[test]
fn test_c_integration_addi4spn() {
    let (mut state, _mem) = setup_test();

    // Set up stack pointer
    state.regs[2] = 0x1000;

    // C.ADDI4SPN x8, 64 -> x8 = SP + 64
    exec_c_addi4spn(8, 64, &mut state).unwrap();
    assert_eq!(state.regs[8], 0x1040);
}

#[test]
fn test_c_integration_addi16sp() {
    let (mut state, _mem) = setup_test();

    // Set up stack pointer
    state.regs[2] = 0x1000;

    // C.ADDI16SP -32 -> SP = SP - 32
    // -32 encoded as 6-bit signed: 0x3E (62 in unsigned)
    exec_c_addi16sp(0x3E, &mut state).unwrap();
    assert_eq!(state.regs[2], 0xFE0);
}

#[test]
fn test_c_integration_jump() {
    let mut state = CoreState {
        pc: 0x100,
        ..Default::default()
    };

    // Set up jump target
    state.regs[5] = 0x1000;

    // C.JR x5
    exec_c_jr(5, &mut state).unwrap();
    assert_eq!(state.pc, 0x1000);
}

#[test]
fn test_c_integration_jump_link() {
    let mut state = CoreState {
        pc: 0x100,
        ..Default::default()
    };

    // Set up jump target
    state.regs[5] = 0x2000;

    // C.JALR x5
    exec_c_jalr(5, &mut state).unwrap();
    assert_eq!(state.pc, 0x2000);
    assert_eq!(state.regs[1], 0x102); // Return address
}

#[test]
fn test_c_integration_ebreak() {
    let mut state = CoreState::default();

    // C.EBREAK should return Ebreak error
    let result = exec_c_ebreak(&mut state);
    assert!(matches!(result, Err(ruscv_sim::ExecuteError::Ebreak)));
}

#[test]
fn test_c_integration_word_operations() {
    let (mut state, _mem) = setup_test();

    // Set up 32-bit values
    state.regs[8] = 0x00000000_00000010; // 16
    state.regs[9] = 0x00000000_00000005; // 5

    // C.ADDW x8, x9 -> x8 = 21 (32-bit add, sign-extended)
    exec_c_addw(8, 9, &mut state).unwrap();
    assert_eq!(state.regs[8], 21);

    // C.SUBW x8, x9 -> x8 = 16 (32-bit sub, sign-extended)
    exec_c_subw(8, 9, &mut state).unwrap();
    assert_eq!(state.regs[8], 16);
}

#[test]
fn test_c_integration_addiw() {
    let (mut state, _mem) = setup_test();

    // Set up 32-bit value
    state.regs[5] = 0x00000000_00000010; // 16

    // C.ADDIW x5, 10 -> x5 = 26 (32-bit add, sign-extended)
    exec_c_addiw(5, 10, &mut state).unwrap();
    assert_eq!(state.regs[5], 26);
}

#[test]
fn test_c_integration_move() {
    let (mut state, _mem) = setup_test();

    state.regs[6] = 0x123456789ABCDEF0;

    // C.MV x5, x6 -> x5 = x6
    exec_c_mv(5, 6, &mut state).unwrap();
    assert_eq!(state.regs[5], 0x123456789ABCDEF0);
}

#[test]
fn test_c_integration_lui() {
    let (mut state, _mem) = setup_test();

    // C.LUI x5, 0x12345 -> x5 = 0x12345000
    exec_c_lui(5, 0x12345, &mut state).unwrap();
    assert_eq!(state.regs[5], 0x0000000012345000);
}

#[test]
fn test_c_integration_nop() {
    let (mut state, _mem) = setup_test();

    // Set up initial state
    state.pc = 0x100;
    state.regs[5] = 0x123456789ABCDEF0;

    // C.NOP should not change any state
    exec_c_nop(&mut state).unwrap();

    // Verify state is unchanged
    assert_eq!(state.pc, 0x100);
    assert_eq!(state.regs[5], 0x123456789ABCDEF0);
}

#[test]
fn test_c_integration_jump_unconditional() {
    let mut state = CoreState {
        pc: 0x100,
        ..Default::default()
    };

    // C.J 128 (unconditional jump forward 128 bytes)
    exec_c_j(128, &mut state).unwrap();

    // PC should be updated to 0x180 (0x100 + 128)
    assert_eq!(state.pc, 0x180);
}

#[test]
fn test_c_integration_branch_equal_zero() {
    let mut state = CoreState {
        regs: {
            let mut r = [0; 32];
            r[8] = 0; // Zero value
            r
        },
        pc: 0x100,
        ..Default::default()
    };

    // C.BEQZ x8, 32 (branch if x8 == 0)
    exec_c_beqz(8, 32, &mut state).unwrap();

    // PC should be updated (branch taken)
    assert_eq!(state.pc, 0x120);
}

#[test]
fn test_c_integration_branch_not_equal_zero() {
    let mut state = CoreState {
        regs: {
            let mut r = [0; 32];
            r[8] = 42; // Non-zero value
            r
        },
        pc: 0x100,
        ..Default::default()
    };

    // C.BNEZ x8, 32 (branch if x8 != 0)
    exec_c_bnez(8, 32, &mut state).unwrap();

    // PC should be updated (branch taken)
    assert_eq!(state.pc, 0x120);
}

#[test]
fn test_c_integration_branch_not_taken() {
    // Test C.BEQZ not taken
    let mut state = CoreState {
        regs: {
            let mut r = [0; 32];
            r[8] = 42; // Non-zero value
            r
        },
        pc: 0x100,
        ..Default::default()
    };

    exec_c_beqz(8, 32, &mut state).unwrap();
    assert_eq!(state.pc, 0x100); // PC unchanged (branch not taken)

    // Test C.BNEZ not taken
    let mut state = CoreState {
        regs: {
            let mut r = [0; 32];
            r[8] = 0; // Zero value
            r
        },
        pc: 0x100,
        ..Default::default()
    };

    exec_c_bnez(8, 32, &mut state).unwrap();
    assert_eq!(state.pc, 0x100); // PC unchanged (branch not taken)
}

#[test]
fn test_c_integration_conditional_loop() {
    // Simulate: for (int i = 5; i != 0; i--)
    let mut state = CoreState {
        regs: {
            let mut r = [0; 32];
            r[8] = 5; // Loop counter
            r
        },
        pc: 0x100, // Loop start
        ..Default::default()
    };

    // Iteration 1: C.BNEZ taken (5 != 0)
    exec_c_bnez(8, 16, &mut state).unwrap();
    assert_eq!(state.pc, 0x110); // Branch to loop body
    state.regs[8] = 4; // Decrement

    // Jump back to loop start
    state.pc = 0x100;

    // Iteration 2: C.BNEZ taken (4 != 0)
    exec_c_bnez(8, 16, &mut state).unwrap();
    assert_eq!(state.pc, 0x110);
    state.regs[8] = 3;

    // And so on... until counter reaches 0
    state.pc = 0x100;
    state.regs[8] = 1;

    exec_c_bnez(8, 16, &mut state).unwrap();
    assert_eq!(state.pc, 0x110);
    state.regs[8] = 0;

    // Final iteration: C.BNEZ not taken (0 == 0)
    state.pc = 0x100;
    exec_c_bnez(8, 16, &mut state).unwrap();
    assert_eq!(state.pc, 0x100); // Branch not taken, exit loop
}

// ============== Floating-Point Compressed Instruction Tests ==============

#[test]
fn test_decode_c_fld() {
    let decoder = CompressedDecoder::new();
    // C.FLD f8, 8(x9): funct3=001, rs1'=001 (x9), rd'=000 (f8), offset=8
    // Layout: [15:13]=funct3, [12:10]=offset[5:3], [9:7]=rs1', [6:5]=offset[7:6], [4:2]=rd', [1:0]=00
    // Binary: 001_001_001_00_000_00 = 0x2480
    let inst: u16 = 0x2480;
    let result = decoder.decode_16bit(inst);
    assert!(result.is_ok(), "Failed to decode C.FLD: {:?}", result);
    let decoded = result.unwrap();
    assert_eq!(decoded.opcode, ruscv_sim::decode::Opcode::LoadFp);
    assert_eq!(decoded.rd, Some(8)); // f8
    assert_eq!(decoded.rs1, Some(9)); // x9
}

#[test]
fn test_decode_c_flw() {
    let decoder = CompressedDecoder::new();
    // C.FLW f9, 4(x8): funct3=011, rs1'=000 (x8), rd'=001 (f9), offset=4
    // Layout: [15:13]=funct3, [12:10]=offset[4:2], [9:7]=rs1', [6:5]=?, [4:2]=rd', [1:0]=00
    // Binary: 011_001_000_00_001_00 = 0x6404
    let inst: u16 = 0x6404;
    let result = decoder.decode_16bit(inst);
    assert!(result.is_ok(), "Failed to decode C.FLW: {:?}", result);
    let decoded = result.unwrap();
    assert_eq!(decoded.opcode, ruscv_sim::decode::Opcode::LoadFp);
    assert_eq!(decoded.rd, Some(9)); // f9
    assert_eq!(decoded.rs1, Some(8)); // x8
}

#[test]
fn test_decode_c_fsd() {
    let decoder = CompressedDecoder::new();
    // C.FSD f9, 16(x10): funct3=101, rs1'=010 (x10), rs2'=001 (f9), offset=16
    // Layout: [15:13]=funct3, [12:10]=offset[5:3], [9:7]=rs1', [6:5]=offset[7:6], [4:2]=rs2', [1:0]=00
    // offset=16=0b10000, offset[7:6]=00, offset[5:3]=010
    // Binary: 101_010_010_00_001_00 = 0xA904
    let inst: u16 = 0xA904;
    let result = decoder.decode_16bit(inst);
    assert!(result.is_ok(), "Failed to decode C.FSD: {:?}", result);
    let decoded = result.unwrap();
    assert_eq!(decoded.opcode, ruscv_sim::decode::Opcode::StoreFp);
    assert_eq!(decoded.rs1, Some(10)); // x10
    assert_eq!(decoded.rs2, Some(9)); // f9
}

#[test]
fn test_decode_c_fsw() {
    let decoder = CompressedDecoder::new();
    // C.FSW f11, 8(x8): funct3=111, rs1'=000 (x8), rs2'=011 (f11), offset=8
    // Layout: [15:13]=funct3, [12:10]=offset[5:3], [9:7]=rs1', [6:5]=offset[7:6], [4:2]=rs2', [1:0]=00
    // offset=8=0b1000, offset[7:6]=00, offset[5:3]=001
    // Binary: 111_001_000_00_011_00 = 0xE40C
    let inst: u16 = 0xE40C;
    let result = decoder.decode_16bit(inst);
    assert!(result.is_ok(), "Failed to decode C.FSW: {:?}", result);
    let decoded = result.unwrap();
    assert_eq!(decoded.opcode, ruscv_sim::decode::Opcode::StoreFp);
    assert_eq!(decoded.rs1, Some(8)); // x8
    assert_eq!(decoded.rs2, Some(11)); // f11
}

#[test]
fn test_c_fld_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;

    // Write a double value to memory
    let test_value: f64 = std::f64::consts::PI;
    let bits = test_value.to_bits();
    mem.write_word(0x1000, bits as u32).unwrap();
    mem.write_word(0x1004, (bits >> 32) as u32).unwrap();

    // Execute C.FLD f8, 0(x8)
    exec_c_fld(0, 0, 0, &mut state, &mut mem).unwrap();

    // Verify the value was loaded
    let loaded_bits = state.fpr.read(8).bits();
    assert_eq!(loaded_bits, test_value.to_bits());
}

#[test]
fn test_c_flw_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[9] = 0x1000;

    // Write a float value to memory
    let test_value: f32 = std::f32::consts::PI;
    mem.write_word(0x1000, test_value.to_bits()).unwrap();

    // Execute C.FLW f9, 0(x9)
    exec_c_flw(1, 1, 0, &mut state, &mut mem).unwrap();

    // Verify the value was loaded and NaN-boxed
    let loaded = state.fpr.read(9).get();
    assert!((loaded - test_value).abs() < 1e-5);
    assert!(state.fpr.read(9).is_nan_boxed());
}

#[test]
fn test_c_fsd_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up registers
    state.regs[8] = 0x1000; // base
    let test_value: f64 = std::f64::consts::E;
    state.fpr.write(9, Fpr::from_bits(test_value.to_bits())); // value to store

    // Execute C.FSD f9, 0(x8)
    exec_c_fsd(0, 1, 0, &mut state, &mut mem).unwrap();

    // Verify memory
    let low = mem.read_word(0x1000).unwrap();
    let high = mem.read_word(0x1004).unwrap();
    let stored_bits = ((high as u64) << 32) | (low as u64);
    assert_eq!(stored_bits, test_value.to_bits());
}

#[test]
fn test_c_fsw_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up registers
    state.regs[9] = 0x1000; // base
    let test_value: f32 = std::f32::consts::E;
    state.fpr.write(10, Fpr::new(test_value)); // value to store (NaN-boxed)

    // Execute C.FSW f10, 0(x9)
    exec_c_fsw(1, 2, 0, &mut state, &mut mem).unwrap();

    // Verify memory
    let stored = mem.read_word(0x1000).unwrap();
    assert_eq!(stored, test_value.to_bits());
}

#[test]
fn test_c_fld_fsd_roundtrip_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;

    // Write a double value to FPR
    let test_value: f64 = std::f64::consts::PI;
    state.fpr.write(9, Fpr::from_bits(test_value.to_bits()));

    // Store to memory using C.FSD
    exec_c_fsd(0, 1, 0, &mut state, &mut mem).unwrap();

    // Load from memory using C.FLD into different register
    exec_c_fld(2, 0, 0, &mut state, &mut mem).unwrap();

    // Verify the roundtrip preserved the value
    let loaded_bits = state.fpr.read(10).bits(); // rd'=2 -> f10
    assert_eq!(loaded_bits, test_value.to_bits());
}

#[test]
fn test_c_flw_fsw_roundtrip_integration() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;

    // Write a float value to FPR
    let test_value: f32 = std::f32::consts::PI;
    state.fpr.write(9, Fpr::new(test_value));

    // Store to memory using C.FSW
    exec_c_fsw(0, 1, 0, &mut state, &mut mem).unwrap();

    // Load from memory using C.FLW into different register
    exec_c_flw(2, 0, 0, &mut state, &mut mem).unwrap();

    // Verify the roundtrip preserved the value
    let loaded = state.fpr.read(10).get(); // rd'=2 -> f10
    assert!((loaded - test_value).abs() < 1e-5);
}

#[test]
fn test_c_float_with_offset() {
    let (mut state, mut mem) = setup_test();

    // Set up base address
    state.regs[8] = 0x1000;

    // Test C.FLD with offset
    let test_value: f64 = 1.23456789;
    let bits = test_value.to_bits();
    mem.write_word(0x1008, bits as u32).unwrap();
    mem.write_word(0x100C, (bits >> 32) as u32).unwrap();

    exec_c_fld(0, 0, 8, &mut state, &mut mem).unwrap();
    assert_eq!(state.fpr.read(8).bits(), test_value.to_bits());

    // Test C.FLW with offset
    let test_value_f32: f32 = 9.876543;
    mem.write_word(0x1010, test_value_f32.to_bits()).unwrap();

    exec_c_flw(1, 0, 16, &mut state, &mut mem).unwrap();
    assert!((state.fpr.read(9).get() - test_value_f32).abs() < 1e-5);
}

#[test]
fn test_c_float_misaligned_access() {
    let (mut state, mut mem) = setup_test();

    // Test C.FLD misaligned (8-byte alignment required)
    state.regs[8] = 0x1001;
    let result = exec_c_fld(0, 0, 0, &mut state, &mut mem);
    assert!(result.is_err());

    // Reset and test C.FLW misaligned (4-byte alignment required)
    state.regs[8] = 0x1001;
    let result = exec_c_flw(0, 0, 0, &mut state, &mut mem);
    assert!(result.is_err());
}
