//! D Extension Load/Store Instructions (RV64D)
//!
//! Implements FLD and FSD instructions for 64-bit double precision
//! floating-point memory operations.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Execute FLD (Load 64-bit Double from Memory)
/// Format: I-type (Load)
/// Encoding: | imm[11:0] | rs1 | funct3=011 | rd | opcode=LoadFp(0000111) |
pub fn exec_fld(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FLD requires rs1");
    let rd = instr.rd.expect("FLD requires rd");
    let imm = instr.imm.expect("FLD requires imm") as i32;

    // Calculate effective address (64-bit)
    let base = state.regs[rs1 as usize];
    let addr = (base as i64).wrapping_add(imm as i64) as u64;

    // Load 64-bit double from memory
    let value = mem.read_dword(addr)?;

    // Write to FPR (stored as raw bits, no NaN boxing needed for double)
    state
        .fpr
        .write(rd as usize, crate::fpu::Fpr::from_bits(value));

    Ok(())
}

/// Execute FSD (Store 64-bit Double to Memory)
/// Format: S-type (Store)
/// Encoding: | imm[11:5] | rs2 | rs1 | funct3=011 | imm[4:0] | opcode=StoreFp(0100111) |
pub fn exec_fsd(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FSD requires rs1");
    let rs2 = instr.rs2.expect("FSD requires rs2");
    let imm = instr.imm.expect("FSD requires imm") as i32;

    // Calculate effective address (64-bit)
    let base = state.regs[rs1 as usize];
    let addr = (base as i64).wrapping_add(imm as i64) as u64;

    // Read 64-bit value from FPR
    let bits = state.fpr.read(rs2 as usize).bits();

    // Store to memory as 64-bit value
    mem.write_dword(addr, bits)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fld_basic() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = create_test_state();
        state.regs[1] = 0x100; // Base address

        // Write a double value to memory
        let test_value: f64 = std::f64::consts::PI;
        let bits = test_value.to_bits();
        mem.write_word(0x100, bits as u32).unwrap();
        mem.write_word(0x104, (bits >> 32) as u32).unwrap();

        // Create FLD instruction: FLD f1, 0(x1)
        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rs3: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };

        exec_fld(&decoded, &mut state, &mut mem).unwrap();

        // Verify the value was loaded
        let loaded_bits = state.fpr.read(2).bits();
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_fsd_basic() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = create_test_state();
        state.regs[1] = 0x100; // Base address

        // Write a double value to FPR
        let test_value: f64 = std::f64::consts::E;
        state
            .fpr
            .write(3, crate::fpu::Fpr::from_bits(test_value.to_bits()));

        // Create FSD instruction: FSD f3, 0(x1)
        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::SType,
            opcode: Opcode::StoreFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: None,
            imm: Some(0),
            branch_taken: false,
        };

        exec_fsd(&decoded, &mut state, &mut mem).unwrap();

        // Verify the value was stored correctly
        let low = mem.read_word(0x100).unwrap();
        let high = mem.read_word(0x104).unwrap();
        let stored_bits = ((high as u64) << 32) | (low as u64);
        assert_eq!(stored_bits, test_value.to_bits());
    }

    #[test]
    fn test_fld_with_offset() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = create_test_state();
        state.regs[1] = 0x100; // Base address

        // Write a double value to memory at offset 8
        let test_value: f64 = -1.5;
        let bits = test_value.to_bits();
        mem.write_word(0x108, bits as u32).unwrap();
        mem.write_word(0x10c, (bits >> 32) as u32).unwrap();

        // Create FLD instruction with offset 8
        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rs3: None,
            rd: Some(2),
            imm: Some(8),
            branch_taken: false,
        };

        exec_fld(&decoded, &mut state, &mut mem).unwrap();

        let loaded_bits = state.fpr.read(2).bits();
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_fld_fsd_roundtrip() {
        let mut mem = SimpleMemory::new(0x2000);
        let mut state = create_test_state();
        state.regs[1] = 0x100; // Base address

        // Store a double value
        let test_value: f64 = std::f64::consts::PI;
        state
            .fpr
            .write(5, crate::fpu::Fpr::from_bits(test_value.to_bits()));

        // Store to memory
        let store_dec = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::SType,
            opcode: Opcode::StoreFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: Some(5),
            rs3: None,
            rd: None,
            imm: Some(0),
            branch_taken: false,
        };
        exec_fsd(&store_dec, &mut state, &mut mem).unwrap();

        // Load from memory
        let load_dec = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rs3: None,
            rd: Some(6),
            imm: Some(0),
            branch_taken: false,
        };
        exec_fld(&load_dec, &mut state, &mut mem).unwrap();

        // Verify the roundtrip preserved the value
        let loaded_bits = state.fpr.read(6).bits();
        assert_eq!(loaded_bits, test_value.to_bits());
    }

    #[test]
    fn test_fld_special_values() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = create_test_state();
        state.regs[1] = 0x100;

        // Test infinity
        let inf_bits = f64::INFINITY.to_bits();
        mem.write_word(0x100, inf_bits as u32).unwrap();
        mem.write_word(0x104, (inf_bits >> 32) as u32).unwrap();

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rs3: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };
        exec_fld(&decoded, &mut state, &mut mem).unwrap();

        let loaded = state.fpr.read(2).bits();
        assert_eq!(loaded, inf_bits);
    }
}
