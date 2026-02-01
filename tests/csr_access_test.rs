//! CSR access instruction tests
//!
//! Tests CSR instructions (CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI)

use ruscv_sim::core::{CoreState, PrivilegeMode};
use ruscv_sim::csr::machine;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::ExecuteError;

fn create_csr_instr(funct3: u8, rd: u8, rs1: u8, csr: u16) -> DecodedInstruction {
    let raw = ((csr as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0b111_0011;
    DecodedInstruction {
        raw,
        format: InstructionFormat::IType,
        opcode: Opcode::System,
        funct3: None,
        funct7: None,
        rs1: Some(rs1),
        rs2: None,
        rs3: None,
        rd: Some(rd),
        imm: Some(csr as u32),
        branch_taken: false,
    }
}

fn exec_system(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn ruscv_sim::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    ruscv_sim::isa::rv64i::system::exec_system(instr, state, mem)
}

// CSRRW Tests

#[test]
fn test_csrrw_basic() {
    let mut state = CoreState::default();
    state.regs[5] = 0xABCD_1234;
    state.csr.write(machine::MSCRATCH, 0x5678_9ABC).unwrap();

    let instr = create_csr_instr(0b001, 10, 5, machine::MSCRATCH);
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x5678_9ABC); // Old value in rd
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xABCD_1234); // New value in CSR
}

#[test]
fn test_csrrw_rd_x0() {
    let mut state = CoreState::default();
    state.regs[5] = 0x1111_2222;

    let instr = create_csr_instr(0b001, 0, 5, machine::MSCRATCH); // rd=x0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[0], 0); // x0 always 0
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x1111_2222);
}

#[test]
fn test_csrrw_rs1_x0() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xFFFF_FFFF).unwrap();

    let instr = create_csr_instr(0b001, 10, 0, machine::MSCRATCH); // rs1=x0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xFFFF_FFFF);
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0);
}

// CSRRS Tests

#[test]
fn test_csrrs_set_bits() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xF0F0_F0F0).unwrap();
    state.regs[5] = 0x0F0F_0F0F;

    let instr = create_csr_instr(0b010, 10, 5, machine::MSCRATCH);
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xF0F0_F0F0); // Old value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xFFFF_FFFF); // All bits set
}

#[test]
fn test_csrrs_no_write_when_rs1_x0() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x1234_5678).unwrap();

    let instr = create_csr_instr(0b010, 10, 0, machine::MSCRATCH); // rs1=x0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x1234_5678); // Read value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x1234_5678); // Unchanged
}

#[test]
fn test_csrrs_partial_set() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x0000_00FF).unwrap();
    state.regs[5] = 0x0000_FF00;

    let instr = create_csr_instr(0b010, 10, 5, machine::MSCRATCH);
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x0000_00FF);
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x0000_FFFF);
}

// CSRRC Tests

#[test]
fn test_csrrc_clear_bits() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xFFFF_FFFF).unwrap();
    state.regs[5] = 0x0F0F_0F0F;

    let instr = create_csr_instr(0b011, 10, 5, machine::MSCRATCH);
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xFFFF_FFFF); // Old value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xF0F0_F0F0); // Bits cleared
}

#[test]
fn test_csrrc_no_write_when_rs1_x0() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xABCD_EF01).unwrap();

    let instr = create_csr_instr(0b011, 10, 0, machine::MSCRATCH); // rs1=x0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xABCD_EF01); // Read value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xABCD_EF01); // Unchanged
}

#[test]
fn test_csrrc_partial_clear() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xFFFF_FFFF).unwrap();
    state.regs[5] = 0x0000_000F;

    let instr = create_csr_instr(0b011, 10, 5, machine::MSCRATCH);
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xFFFF_FFFF);
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xFFFF_FFF0);
}

// CSRRWI Tests

#[test]
fn test_csrrwi_basic() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xDEAD_BEEF).unwrap();

    let instr = create_csr_instr(0b101, 10, 21, machine::MSCRATCH); // zimm=21
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xDEAD_BEEF); // Old value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 21); // New value
}

#[test]
fn test_csrrwi_zimm_zero() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x1234_5678).unwrap();

    let instr = create_csr_instr(0b101, 10, 0, machine::MSCRATCH); // zimm=0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x1234_5678);
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0);
}

#[test]
fn test_csrrwi_max_zimm() {
    let mut state = CoreState::default();

    let instr = create_csr_instr(0b101, 10, 31, machine::MSCRATCH); // zimm=31 (max)
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 31);
}

// CSRRSI Tests

#[test]
fn test_csrrsi_set_bits() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x0000_0010).unwrap();

    let instr = create_csr_instr(0b110, 10, 5, machine::MSCRATCH); // zimm=5
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x0000_0010); // Old value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x0000_0015); // 0x10 | 0x05 = 0x15
}

#[test]
fn test_csrrsi_no_write_when_zimm_zero() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0xABCD_EF01).unwrap();

    let instr = create_csr_instr(0b110, 10, 0, machine::MSCRATCH); // zimm=0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0xABCD_EF01); // Read value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0xABCD_EF01); // Unchanged
}

// CSRRCI Tests

#[test]
fn test_csrrci_clear_bits() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x0000_001F).unwrap();

    let instr = create_csr_instr(0b111, 10, 7, machine::MSCRATCH); // zimm=7
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x0000_001F); // Old value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x0000_0018); // 0x1F & ~0x07 = 0x18
}

#[test]
fn test_csrrci_no_write_when_zimm_zero() {
    let mut state = CoreState::default();
    state.csr.write(machine::MSCRATCH, 0x5555_5555).unwrap();

    let instr = create_csr_instr(0b111, 10, 0, machine::MSCRATCH); // zimm=0
    let mut mem = SimpleMemory::new(0x1000);

    exec_system(&instr, &mut state, &mut mem).unwrap();

    assert_eq!(state.regs[10], 0x5555_5555); // Read value
    assert_eq!(state.csr.read(machine::MSCRATCH).unwrap(), 0x5555_5555); // Unchanged
}

// Privilege violation tests

#[test]
fn test_privilege_violation_user_access_machine() {
    let mut state = CoreState::default();
    state.csr.set_privilege(PrivilegeMode::User);

    let instr = create_csr_instr(0b001, 10, 5, machine::MSTATUS);
    let mut mem = SimpleMemory::new(0x1000);

    let result = exec_system(&instr, &mut state, &mut mem);
    assert!(result.is_err());
}

#[test]
fn test_privilege_violation_supervisor_access_machine() {
    let mut state = CoreState::default();
    state.csr.set_privilege(PrivilegeMode::Supervisor);

    let instr = create_csr_instr(0b001, 10, 5, machine::MEPC);
    let mut mem = SimpleMemory::new(0x1000);

    let result = exec_system(&instr, &mut state, &mut mem);
    assert!(result.is_err());
}

// Read-only CSR tests

#[test]
fn test_write_to_readonly_csr() {
    let mut state = CoreState::default();
    state.regs[5] = 999;

    let instr = create_csr_instr(0b001, 10, 5, machine::MHARTID);
    let mut mem = SimpleMemory::new(0x1000);

    let result = exec_system(&instr, &mut state, &mut mem);
    assert!(result.is_err());
}

// Complex sequence tests

#[test]
fn test_csr_sequence() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);

    // 1. Write value with CSRRW
    state.regs[1] = 0x1000_0000;
    let instr1 = create_csr_instr(0b001, 10, 1, machine::MEPC);
    exec_system(&instr1, &mut state, &mut mem).unwrap();
    assert_eq!(state.csr.read(machine::MEPC).unwrap(), 0x1000_0000);

    // 2. Set bits with CSRRS
    state.regs[2] = 0x0F00_0000;
    let instr2 = create_csr_instr(0b010, 11, 2, machine::MEPC);
    exec_system(&instr2, &mut state, &mut mem).unwrap();
    assert_eq!(state.csr.read(machine::MEPC).unwrap(), 0x1F00_0000);

    // 3. Clear bits with CSRRC
    state.regs[3] = 0x0F00_0000;
    let instr3 = create_csr_instr(0b011, 12, 3, machine::MEPC);
    exec_system(&instr3, &mut state, &mut mem).unwrap();
    assert_eq!(state.csr.read(machine::MEPC).unwrap(), 0x1000_0000);
}
