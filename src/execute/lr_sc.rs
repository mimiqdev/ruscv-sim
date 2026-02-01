//! RV64A Load-Reserved / Store-Conditional instructions
//!
//! This module re-exports the LR/SC implementation from `isa::rv64a::lr_sc`
//! and provides integration tests for the execute module.
//!
//! # Reservation Mechanism
//!
//! LR/SC provides atomic read-modify-write operations:
//! - LR loads a value and creates a reservation on the memory location
//! - SC attempts to store only if the reservation is still valid
//! - If successful, returns 0; otherwise returns non-zero
//!
//! # References
//!
//! - RISC-V ISA Volume I: Unprivileged Spec, Section 8.3 (Load-Reserved/Store-Conditional)
//! - RISC-V ISA Volume II: Privileged Spec, Section 3.5.1 (Reservation Granularity)

// Re-export all public items from the canonical implementation in isa::rv64a
pub use crate::isa::rv64a::{
    clear_reservation, exec_lr, exec_lr_w, exec_sc, exec_sc_w, ReservationSet,
};

// Re-export for tests that need the MemoryInterface trait
#[cfg(test)]
use crate::memory::MemoryInterface;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_lr_instr(rs1: u8, rd: u8, funct5: u8, _aq: u8, rl: u8) -> DecodedInstruction {
        // LR: funct5 = 00010, rs2 = 00000
        // SC: funct5 = 00011, rs2 = source register
        let raw = ((funct5 as u32) << 27)
            | ((rl as u32) << 25)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b010_1111;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Amo,
            funct3: Some(Funct3::Slt), // width = 32-bit
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(0), // 0 for LR
            rs3: None,
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    fn create_sc_instr(
        rs1: u8,
        rs2: u8,
        rd: u8,
        funct5: u8,
        _aq: u8,
        _rl: u8,
    ) -> DecodedInstruction {
        let raw = ((funct5 as u32) << 27)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b010_1111;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Amo,
            funct3: Some(Funct3::Slt),
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    #[test]
    fn test_lr_basic() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // Write a value to memory (positive 32-bit value for consistent behavior)
        mem.write_word(0x100, 0x1234_5678).unwrap();

        state.regs[1] = 0x100;

        let instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        let result = exec_lr(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[2], 0x1234_5678);
    }

    #[test]
    fn test_lr_creates_reservation() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x200, 0xDEAD_BEEF).unwrap();
        state.regs[1] = 0x200;

        let instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        exec_lr(&instr, &mut state, &mut mem).unwrap();

        // Verify reservation was created by checking SC succeeds
        state.regs[3] = 0xCAFE_BABE;
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        exec_sc(&sc_instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[4], 0); // Success
        assert_eq!(mem.read_word(0x200).unwrap(), 0xCAFE_BABE);
    }

    #[test]
    fn test_sc_success() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // First, create a reservation with LR
        mem.write_word(0x300, 0x0000_0000).unwrap();
        state.regs[1] = 0x300;
        let lr_instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        exec_lr(&lr_instr, &mut state, &mut mem).unwrap();

        // Now SC should succeed
        state.regs[3] = 0xABCDEFFF;
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[4], 0); // Success

        // Check memory was updated
        assert_eq!(mem.read_word(0x300).unwrap(), 0xABCDEFFF);
    }

    #[test]
    fn test_sc_fail_no_reservation() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // Write initial value to memory first so read succeeds
        mem.write_word(0x400, 0x0000_0000).unwrap();

        // Try SC without LR first - should fail
        state.regs[1] = 0x400;
        state.regs[2] = 0x1234_5678;
        let sc_instr = create_sc_instr(1, 2, 3, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_ne!(state.regs[3], 0); // Failure

        // Memory should be unchanged
        assert_eq!(mem.read_word(0x400).unwrap(), 0x0000_0000);
    }

    #[test]
    fn test_sc_fail_after_conflict() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // Create reservation with LR
        mem.write_word(0x500, 0x0000_0000).unwrap();
        state.regs[1] = 0x500;
        let lr_instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        exec_lr(&lr_instr, &mut state, &mut mem).unwrap();

        // Clear reservation manually (simulating another hart)
        clear_reservation();

        // Now SC should fail
        state.regs[3] = 0xABCDEFFF;
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_ne!(state.regs[4], 0); // Failure
    }

    #[test]
    fn test_lr_sc_atomic_sequence() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        let addr = 0x600;
        mem.write_word(addr, 0x1000).unwrap();

        // LR
        state.regs[1] = addr;
        let lr_instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        exec_lr(&lr_instr, &mut state, &mut mem).unwrap();
        let old_value = state.regs[2];

        // Modify (increment)
        state.regs[3] = old_value.wrapping_add(1);

        // SC
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        exec_sc(&sc_instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[4], 0); // Success
        assert_eq!(mem.read_word(addr).unwrap(), 0x1001);
    }

    #[test]
    fn test_sc_clears_reservation() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // Create reservation
        mem.write_word(0x700, 0).unwrap();
        state.regs[1] = 0x700;
        let lr_instr = create_lr_instr(1, 2, 0b00010, 0, 0);
        exec_lr(&lr_instr, &mut state, &mut mem).unwrap();

        // First SC
        state.regs[3] = 0x1111_1111;
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        exec_sc(&sc_instr, &mut state, &mut mem).unwrap();

        // Second SC should fail (reservation cleared)
        state.regs[3] = 0x2222_2222;
        let sc_instr2 = create_sc_instr(1, 3, 5, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr2, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_ne!(state.regs[5], 0); // Failure
    }

    #[test]
    fn test_lr_x0_dest() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        mem.write_word(0x800, 0x1234_5678).unwrap();
        state.regs[1] = 0x800;

        let instr = create_lr_instr(1, 0, 0b00010, 0, 0);
        let result = exec_lr(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[0], 0); // x0 always 0
    }

    #[test]
    fn test_reservation_set_operations() {
        let mut rs = ReservationSet::new();

        assert!(!rs.has_reservation(0x100));
        assert!(rs.reserved_address().is_none());

        rs.reserve(0x100);
        assert!(rs.has_reservation(0x100));
        assert_eq!(rs.reserved_address(), Some(0x100));

        rs.clear();
        assert!(!rs.has_reservation(0x100));
        assert!(rs.reserved_address().is_none());

        rs.reserve(0x200);
        rs.clear_if_matching(0x100); // Wrong address
        assert!(rs.has_reservation(0x200));

        rs.clear_if_matching(0x200); // Correct address
        assert!(!rs.has_reservation(0x200));
    }
}
