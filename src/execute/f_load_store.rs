//! Floating-point Load/Store Instructions (RV64F)
//!
//! Implements FLW and FSD instructions for 32-bit floating-point memory operations.
//! FLW loads a 32-bit float and NaN-boxes it in a 64-bit register.
//! FSD stores the lower 32 bits of a floating-point register.

use crate::core::CoreState;
use crate::decode::InstructionFormat;
use crate::decode::{DecodedInstruction, Opcode};
use crate::fpu::Fpr;
use crate::memory::{MemoryError, MemoryInterface};

/// Execute FLW (Load 32-bit Float from Memory)
/// Format: I-type (Load)
/// Encoding: | imm[11:0] | rs1 | funct3=010 | rd | opcode=LoadFp(0000111) |
pub fn exec_flw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), MemoryError> {
    let rs1 = instr.rs1.expect("FLW requires rs1");
    let rd = instr.rd.expect("FLW requires rd");
    let imm = instr.imm.expect("FLW requires imm") as i32;

    // Calculate effective address
    let base = state.regs[rs1 as usize] as u64;
    let addr = (base as i64 + imm as i64) as u32;

    // Load 32-bit float from memory
    let value = mem.read_word(addr)?;

    // NaN-box the value and write to FPR
    let fpr = Fpr::new(f32::from_bits(value));
    state.fpr.write(rd as usize, fpr);

    Ok(())
}

/// Execute FSD (Store 32-bit Float to Memory)
/// Format: S-type (Store)
/// Encoding: | imm[11:5] | rs2 | rs1 | funct3=010 | imm[4:0] | opcode=StoreFp(0100111) |
pub fn exec_fsd(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), MemoryError> {
    let rs1 = instr.rs1.expect("FSD requires rs1");
    let rs2 = instr.rs2.expect("FSD requires rs2");
    let imm = instr.imm.expect("FSD requires imm") as i32;

    // Calculate effective address
    let base = state.regs[rs1 as usize] as u64;
    let addr = (base as i64 + imm as i64) as u32;

    // Read lower 32 bits from FPR and store to memory
    let value = state.fpr.read_u32(rs2 as usize);
    mem.write_word(addr, value)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::FpuRegisterFile;
    use crate::memory::SimpleMemory;

    #[test]
    fn test_flw_basic() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = CoreState::default();
        state.regs[1] = 0x100; // Base address

        // Write a float value to memory
        let test_value: f32 = 3.14159;
        mem.write_word(0x100, test_value.to_bits()).unwrap();

        // Create FLW instruction: FLW f1, 0(x1)
        // imm=0, rs1=1, rd=2 (f1), funct3=010, opcode=LoadFp
        let flw_instr = (0u32 << 20) | (1u32 << 15) | (0b010u32 << 12) | (2u32 << 7) | 0b000_0111;

        let decoded = DecodedInstruction {
            raw: flw_instr,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };

        exec_flw(&decoded, &mut state, &mut mem).unwrap();

        // Verify the value was loaded and NaN-boxed
        let loaded = state.fpr.read(2).get();
        assert!((loaded - test_value).abs() < 1e-5);
        assert!(state.fpr.read(2).is_nan_boxed());
    }

    #[test]
    fn test_fsd_basic() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = CoreState::default();
        state.regs[1] = 0x100; // Base address

        // Write a float value to FPR (NaN-boxed)
        let test_value: f32 = 2.71828;
        state.fpr.write(3, Fpr::new(test_value));

        // Create FSD instruction: FSD f3, 0(x1)
        // imm=0, rs2=3, rs1=1, funct3=010, opcode=StoreFp
        let fsd_instr = (0u32 << 25) | (3u32 << 20) | (1u32 << 15) | (0b010u32 << 12) | 0b010_0111;

        let decoded = DecodedInstruction {
            raw: fsd_instr,
            format: InstructionFormat::SType,
            opcode: Opcode::StoreFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: Some(3),
            rd: None,
            imm: Some(0),
            branch_taken: false,
        };

        exec_fsd(&decoded, &mut state, &mut mem).unwrap();

        // Verify the value was stored correctly
        let stored = mem.read_word(0x100).unwrap();
        assert_eq!(stored, test_value.to_bits());
    }

    #[test]
    fn test_flw_with_offset() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = CoreState::default();
        state.regs[1] = 0x100; // Base address

        // Write a float value to memory at offset 16
        let test_value: f32 = -1.5;
        mem.write_word(0x110, test_value.to_bits()).unwrap();

        // Create FLW instruction with offset 16
        let flw_instr = (16u32 << 20) | (1u32 << 15) | (0b010u32 << 12) | (2u32 << 7) | 0b000_0111;

        let decoded = DecodedInstruction {
            raw: flw_instr,
            format: InstructionFormat::IType,
            opcode: Opcode::LoadFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(16),
            branch_taken: false,
        };

        exec_flw(&decoded, &mut state, &mut mem).unwrap();

        let loaded = state.fpr.read(2).get();
        assert!((loaded - test_value).abs() < 1e-5);
    }

    #[test]
    fn test_fsd_zero_register() {
        let mut mem = SimpleMemory::new(0x1000);
        let mut state = CoreState::default();
        state.regs[1] = 0x100;

        // Writing to f0 should be ignored (it's hardwired to 0)
        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::SType,
            opcode: Opcode::StoreFp,
            funct3: None,
            funct7: None,
            rs1: Some(1),
            rs2: Some(0),
            rd: None,
            imm: Some(0),
            branch_taken: false,
        };

        exec_fsd(&decoded, &mut state, &mut mem).unwrap();

        // Memory should still be 0 (not written)
        assert_eq!(mem.read_word(0x100).unwrap(), 0);
    }
}
