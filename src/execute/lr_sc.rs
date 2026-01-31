//! RV64A Load-Reserved / Store-Conditional instructions
//!
//! Implements LR (Load-Reserved) and SC (Store-Conditional) instructions
//! for atomic memory operations.
//!
//! # Reservation Mechanism
//!
//! LR/SC provides atomic read-modify-write operations:
//! - LR loads a value and creates a reservation on the memory location
//! - SC attempts to store only if the reservation is still valid
//! - If successful, returns 0; otherwise returns non-zero

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Reservation set for LR/SC operations
///
/// Tracks the address of the reservation for each hart.
/// In a multi-core system, this would need to be per-hart.
#[derive(Debug, Clone)]
pub struct ReservationSet {
    /// Reserved address, or None if no reservation
    reserved_addr: Option<u32>,
}

impl ReservationSet {
    /// Create a new reservation set
    pub fn new() -> Self {
        Self {
            reserved_addr: None,
        }
    }

    /// Check if we have a reservation for the given address
    pub fn has_reservation(&self, addr: u32) -> bool {
        self.reserved_addr == Some(addr)
    }

    /// Create a reservation for the given address
    pub fn reserve(&mut self, addr: u32) {
        self.reserved_addr = Some(addr);
    }

    /// Clear the reservation
    pub fn clear(&mut self) {
        self.reserved_addr = None;
    }

    /// Clear reservation for a specific address (only if matching)
    pub fn clear_if_matching(&mut self, addr: u32) {
        if self.reserved_addr == Some(addr) {
            self.reserved_addr = None;
        }
    }

    /// Get the reserved address if any
    pub fn reserved_address(&self) -> Option<u32> {
        self.reserved_addr
    }
}

impl Default for ReservationSet {
    fn default() -> Self {
        Self::new()
    }
}

use once_cell::sync::Lazy;
/// Global reservation set (singleton for single-core simulation)
///
/// In a real multi-core system, this would be per-hart.
use std::sync::Mutex;

static GLOBAL_RESERVATION: Lazy<Mutex<ReservationSet>> =
    Lazy::new(|| Mutex::new(ReservationSet::new()));

/// LR - Load-Reserved
///
/// Loads a 32-bit value from memory and creates a reservation on that address.
///
/// # Encoding
/// - funct5 = 00010 for LR
/// - rs2 = 00000 (no second source register)
///
/// # Operation
/// rd = MEM[rs1]
/// Create reservation on rs1
#[inline]
pub fn exec_lr(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];

    // Read the value from memory
    let value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    // Create reservation
    let mut reservation = GLOBAL_RESERVATION.lock().unwrap();
    reservation.reserve(addr);

    // Write result to rd (unless rd = x0)
    if rd != 0 {
        state.regs[rd] = value;
    }

    Ok(())
}

/// LR.W - Load-Reserved 32-bit (RV64 specific)
///
/// Loads a 32-bit value from memory, sign-extending to 64 bits.
/// Creates a reservation on the address.
///
/// # Operation
/// rd = sext(MEM[rs1][31:0])
/// Create reservation on rs1
#[inline]
pub fn exec_lr_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];

    // Read 32-bit value from memory
    let value = mem
        .read_word(addr)
        .map_err(|e| ExecuteError::MemoryError(e))?;

    // Sign-extend to 64 bits (but we store in u32 for now)
    let value = value as i32 as u64 as u32;

    // Create reservation
    let mut reservation = GLOBAL_RESERVATION.lock().unwrap();
    reservation.reserve(addr);

    // Write result to rd
    if rd != 0 {
        state.regs[rd] = value;
    }

    Ok(())
}

/// SC - Store-Conditional
///
/// Conditionally stores a 32-bit value to memory only if the reservation
/// is still valid.
///
/// # Encoding
/// - funct5 = 00011 for SC
/// - rs2 contains the value to store
///
/// # Operation
/// if reservation valid:
///   MEM[rs1] = rs2
///   rd = 0
/// else:
///   rd = non-zero
/// Clear reservation regardless of success
#[inline]
pub fn exec_sc(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    // Check reservation
    let mut reservation = GLOBAL_RESERVATION.lock().unwrap();
    let success = reservation.has_reservation(addr);

    if success {
        // Store the value
        mem.write_word(addr, value)
            .map_err(|e| ExecuteError::MemoryError(e))?;
        state.regs[rd] = 0; // Success
    } else {
        state.regs[rd] = 1; // Failure (non-zero)
    }

    // Clear reservation regardless
    reservation.clear();

    Ok(())
}

/// SC.W - Store-Conditional 32-bit
///
/// Conditionally stores a 32-bit value to memory.
#[inline]
pub fn exec_sc_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    // Check reservation
    let mut reservation = GLOBAL_RESERVATION.lock().unwrap();
    let success = reservation.has_reservation(addr);

    if success {
        // Store the lower 32 bits
        mem.write_word(addr, value)
            .map_err(|e| ExecuteError::MemoryError(e))?;
        state.regs[rd] = 0; // Success
    } else {
        state.regs[rd] = 1; // Failure
    }

    // Clear reservation regardless
    reservation.clear();

    Ok(())
}

/// Clear global reservation (for testing)
pub fn clear_reservation() {
    let mut reservation = GLOBAL_RESERVATION.lock().unwrap();
    reservation.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};

    fn create_lr_instr(rs1: u8, rd: u8, funct5: u8, aq: u8, rl: u8) -> DecodedInstruction {
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
            funct3: Some(0b010), // width = 32-bit
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(0), // 0 for LR
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    fn create_sc_instr(rs1: u8, rs2: u8, rd: u8, funct5: u8, aq: u8, rl: u8) -> DecodedInstruction {
        let raw = ((funct5 as u32) << 27)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((rd as u32) << 7)
            | 0b010_1111;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Amo,
            funct3: Some(0b010),
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
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

        // Write a value to memory
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

        let reservation = GLOBAL_RESERVATION.lock().unwrap();
        assert!(reservation.has_reservation(0x200));
        assert_eq!(reservation.reserved_address(), Some(0x200));
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
        state.regs[3] = 0xABCD_EFGI;
        let sc_instr = create_sc_instr(1, 3, 4, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(state.regs[4], 0); // Success

        // Check memory was updated
        assert_eq!(mem.read_word(0x300).unwrap(), 0xABCD_EFGI);
    }

    #[test]
    fn test_sc_fail_no_reservation() {
        clear_reservation();
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);

        // Try SC without LR first - should fail
        state.regs[1] = 0x400;
        state.regs[2] = 0x1234_5678;
        let sc_instr = create_sc_instr(1, 2, 3, 0b00011, 0, 0);
        let result = exec_sc(&sc_instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_ne!(state.regs[3], 0); // Failure

        // Memory should be unchanged
        assert!(mem.read_word(0x400).is_err());
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
        state.regs[3] = 0xABCD_EFGI;
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
