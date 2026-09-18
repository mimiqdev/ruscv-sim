//! CSR access instruction tests
//!
//! Tests CSR instructions (CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI)

use ruscv_sim::core::{CoreState, PrivilegeMode};
use ruscv_sim::csr::machine;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::isa::rv64i::system::exec_system_with_csr_access;
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

#[test]
fn test_minstret_csr_write_classification_exposes_precedence_fact() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    state.csr.write(machine::MINSTRET, 0x100).unwrap();

    // CSRRS with a non-x0 source whose value is zero is still an explicit
    // write.  The value remains unchanged, but the fact must be preserved for
    // Task 3's same-instruction retirement precedence.
    state.regs[5] = 0;
    let set_with_zero = create_csr_instr(0b010, 10, 5, machine::MINSTRET);
    let access = exec_system_with_csr_access(&set_with_zero, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, 0x100);
    assert_eq!(access.new_value, 0x100);
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), 0x100);

    // CSRRS with rs1=x0 is read-only.
    let read_only = create_csr_instr(0b010, 11, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&read_only, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, 0x100);
    assert_eq!(access.new_value, 0x100);
    assert!(!access.wrote);

    // CSRRW and CSRRWI always write, including x0/zimm=0.
    let swap_zero = create_csr_instr(0b001, 12, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&swap_zero, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, 0x100);
    assert_eq!(access.new_value, 0);
    assert!(access.wrote);

    state.csr.write(machine::MINSTRET, 0x200).unwrap();
    let swap_immediate_zero = create_csr_instr(0b101, 13, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&swap_immediate_zero, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, 0x200);
    assert_eq!(access.new_value, 0);
    assert!(access.wrote);
}

#[test]
fn test_minstret_immediate_set_clear_classification() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    state.csr.write(machine::MINSTRET, 0xF0).unwrap();

    // zimm=0 is read-only for CSRRSI/CSRRCI.
    let read_set = create_csr_instr(0b110, 10, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&read_set, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(!access.wrote);
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), 0xF0);

    // A non-zero zimm writes even if the resulting value is unchanged.
    let set = create_csr_instr(0b110, 11, 0x0F, machine::MINSTRET);
    let access = exec_system_with_csr_access(&set, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(access.old_value, 0xF0);
    assert_eq!(access.new_value, 0xFF);

    let clear = create_csr_instr(0b111, 12, 0x0F, machine::MINSTRET);
    let access = exec_system_with_csr_access(&clear, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(access.old_value, 0xFF);
    assert_eq!(access.new_value, 0xF0);
}

#[test]
fn test_csrrc_write_classification_preserves_identity_fact() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    let initial = 0x100_u64;
    state.csr.write(machine::MINSTRET, initial).unwrap();

    // CSRRC with rs1=x0 is read-only.
    let read_only = create_csr_instr(0b011, 10, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&read_only, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, initial);
    assert_eq!(access.new_value, initial);
    assert!(!access.wrote);
    assert_eq!(state.regs[10], initial);
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), initial);

    // A non-x0 source whose value is zero still performs an explicit write.
    // The architectural value is unchanged, but Task 3 must see `wrote=true`.
    state.regs[5] = 0;
    let explicit = create_csr_instr(0b011, 11, 5, machine::MINSTRET);
    let access = exec_system_with_csr_access(&explicit, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, initial);
    assert_eq!(access.new_value, initial);
    assert!(access.wrote);
    assert_eq!(state.regs[11], initial);
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), initial);
}

#[test]
fn test_csrrci_zero_zimm_is_read_only() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    let initial = 0x200_u64;
    state.csr.write(machine::MINSTRET, initial).unwrap();
    state.regs[12] = 0xDEAD_BEEF;

    let instr = create_csr_instr(0b111, 12, 0, machine::MINSTRET);
    let access = exec_system_with_csr_access(&instr, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert_eq!(access.old_value, initial);
    assert_eq!(access.new_value, initial);
    assert!(!access.wrote);
    assert_eq!(state.regs[12], initial);
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), initial);
}

#[test]
fn test_csrrc_zero_source_rejects_read_only_csr_without_side_effects() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    state.regs[5] = 0;
    state.regs[13] = 0xDEAD_BEEF;
    let before = state.csr.read(machine::MHARTID).unwrap();

    // rs1!=x0 is a write attempt even though its value is zero.  MHARTID is
    // read-only, so CSRRC must fail before changing rd or the CSR.
    let instr = create_csr_instr(0b011, 13, 5, machine::MHARTID);
    assert!(exec_system_with_csr_access(&instr, &mut state, &mut mem).is_err());
    assert_eq!(state.regs[13], 0xDEAD_BEEF);
    assert_eq!(state.csr.read(machine::MHARTID).unwrap(), before);
}

#[test]
fn test_misa_and_mepc_warl_through_immediate_csr_paths() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);

    // CSRRWI writes the candidate value, but MISA.C remains zero.
    let misa_write = create_csr_instr(0b101, 10, 0b00100, machine::MISA);
    let access = exec_system_with_csr_access(&misa_write, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MISA).unwrap() & (1 << 2), 0);

    // CSRRSI/CSRRCI are still explicit writes when zimm is non-zero, even
    // when WARL leaves the architectural value unchanged.
    let misa_set = create_csr_instr(0b110, 11, 0b00100, machine::MISA);
    let access = exec_system_with_csr_access(&misa_set, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MISA).unwrap() & (1 << 2), 0);

    let mepc_write = create_csr_instr(0b101, 12, 0b00011, machine::MEPC);
    let access = exec_system_with_csr_access(&mepc_write, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MEPC).unwrap() & 0b11, 0);

    let mepc_set = create_csr_instr(0b110, 13, 0b00001, machine::MEPC);
    let access = exec_system_with_csr_access(&mepc_set, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MEPC).unwrap() & 0b11, 0);

    let mepc_clear = create_csr_instr(0b111, 14, 0b00001, machine::MEPC);
    let access = exec_system_with_csr_access(&mepc_clear, &mut state, &mut mem)
        .unwrap()
        .unwrap();
    assert!(access.wrote);
    assert_eq!(state.csr.read(machine::MEPC).unwrap() & 0b11, 0);
}

#[test]
fn test_failed_csr_access_preserves_old_value_and_destination() {
    let mut state = CoreState::default();
    let mut mem = SimpleMemory::new(0x1000);
    state.regs[10] = 0xDEAD_BEEF;
    state.regs[5] = 0;
    let before = state.csr.read(machine::MHARTID).unwrap();

    // rs1!=x0 means this is a write attempt, so a read-only CSR rejects it.
    let instr = create_csr_instr(0b010, 10, 5, machine::MHARTID);
    assert!(exec_system_with_csr_access(&instr, &mut state, &mut mem).is_err());
    assert_eq!(state.regs[10], 0xDEAD_BEEF);
    assert_eq!(state.csr.read(machine::MHARTID).unwrap(), before);
}
