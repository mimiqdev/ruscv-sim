//! System instruction execution
//!
//! System instructions handle system-level operations including CSR access.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// System instructions (exec_system)
///
/// Executes system instructions including:
/// - ECALL: Environment call (system call)
/// - EBREAK: Environment break (debugger breakpoint)
/// - CSRRW: CSR Read-Write
/// - CSRRS: CSR Read-Set
/// - CSRRC: CSR Read-Clear
/// - CSRRWI: CSR Read-Write Immediate
/// - CSRRSI: CSR Read-Set Immediate
/// - CSRRCI: CSR Read-Clear Immediate
#[inline]
pub fn exec_system(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    // For CSR instructions, imm contains CSR address (bits[31:20])
    // For ECALL/EBREAK, imm contains the function code (0 or 1)
    let Some(imm) = instr.imm else {
        return Err(ExecuteError::InvalidOperation);
    };

    // Check funct3 to determine instruction type
    let funct3 = ((instr.raw >> 12) & 0b111) as u8;

    match funct3 {
        0b000 => {
            // ECALL/EBREAK (I-type with funct3=0)
            match imm {
                0 => Err(ExecuteError::Ecall),
                1 => Err(ExecuteError::Ebreak),
                _ => Err(ExecuteError::InvalidOperation),
            }
        }
        0b001 => {
            // CSRRW - CSR Read-Write
            let csr_addr = (imm & 0xFFF) as u16;
            let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            let rs1_value = state.regs[rs1];

            // Read old value and write new value
            let old_value = state
                .csr
                .read_write(csr_addr, rs1_value)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        0b010 => {
            // CSRRS - CSR Read-Set
            let csr_addr = (imm & 0xFFF) as u16;
            let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            let rs1_value = state.regs[rs1];

            // Read old value and set bits
            let old_value = state
                .csr
                .read_set(csr_addr, rs1_value)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        0b011 => {
            // CSRRC - CSR Read-Clear
            let csr_addr = (imm & 0xFFF) as u16;
            let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            let rs1_value = state.regs[rs1];

            // Read old value and clear bits
            let old_value = state
                .csr
                .read_clear(csr_addr, rs1_value)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        0b101 => {
            // CSRRWI - CSR Read-Write Immediate
            let csr_addr = (imm & 0xFFF) as u16;
            let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u32; // rs1 field holds zimm
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            // Read old value and write immediate
            let old_value = state
                .csr
                .read_write(csr_addr, zimm)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        0b110 => {
            // CSRRSI - CSR Read-Set Immediate
            let csr_addr = (imm & 0xFFF) as u16;
            let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u32;
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            // Read old value and set bits with immediate
            let old_value = state
                .csr
                .read_set(csr_addr, zimm)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        0b111 => {
            // CSRRCI - CSR Read-Clear Immediate
            let csr_addr = (imm & 0xFFF) as u16;
            let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u32;
            let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

            // Read old value and clear bits with immediate
            let old_value = state
                .csr
                .read_clear(csr_addr, zimm)
                .map_err(ExecuteError::CsrError)?;

            // Write old value to rd (unless rd=x0)
            if rd != 0 {
                state.regs[rd] = old_value;
            }
            Ok(())
        }
        _ => Err(ExecuteError::InvalidOperation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csr::machine;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr_system(imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::System,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: None,
            imm: Some(imm),
            branch_taken: false,
        }
    }

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
            rd: Some(rd),
            imm: Some(csr as u32),
            branch_taken: false,
        }
    }

    #[test]
    fn test_ecall() {
        let mut state = CoreState::default();
        let instr = create_test_instr_system(0);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::Ecall)));
    }

    #[test]
    fn test_ebreak() {
        let mut state = CoreState::default();
        let instr = create_test_instr_system(1);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::Ebreak)));
    }

    #[test]
    fn test_csrrw() {
        let mut state = CoreState::default();
        state.regs[5] = 0x1234_5678;

        let instr = create_csr_instr(0b001, 10, 5, machine::MEPC);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that MEPC was written with value from x5
        assert_eq!(state.csr.read(machine::MEPC).unwrap(), 0x1234_5678);

        // Check that x10 received the old value (0)
        assert_eq!(state.regs[10], 0);
    }

    #[test]
    fn test_csrrs() {
        let mut state = CoreState::default();
        state.csr.write(machine::MSTATUS, 0x1000).unwrap();
        state.regs[5] = 0x0100;

        let instr = create_csr_instr(0b010, 10, 5, machine::MSTATUS);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that bits were set
        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x1100);

        // Check that x10 received the old value
        assert_eq!(state.regs[10], 0x1000);
    }

    #[test]
    fn test_csrrc() {
        let mut state = CoreState::default();
        state.csr.write(machine::MSTATUS, 0x1111).unwrap();
        state.regs[5] = 0x0101;

        let instr = create_csr_instr(0b011, 10, 5, machine::MSTATUS);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that bits were cleared
        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x1010);

        // Check that x10 received the old value
        assert_eq!(state.regs[10], 0x1111);
    }

    #[test]
    fn test_csrrwi() {
        let mut state = CoreState::default();

        let instr = create_csr_instr(0b101, 10, 15, machine::MEPC); // zimm=15
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that MEPC was written with immediate value
        assert_eq!(state.csr.read(machine::MEPC).unwrap(), 15);

        // Check that x10 received the old value (0)
        assert_eq!(state.regs[10], 0);
    }

    #[test]
    fn test_csrrsi() {
        let mut state = CoreState::default();
        state.csr.write(machine::MSTATUS, 0x1000).unwrap();

        let instr = create_csr_instr(0b110, 10, 7, machine::MSTATUS); // zimm=7
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that bits were set with immediate
        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x1007);

        // Check that x10 received the old value
        assert_eq!(state.regs[10], 0x1000);
    }

    #[test]
    fn test_csrrci() {
        let mut state = CoreState::default();
        state.csr.write(machine::MSTATUS, 0x00FF).unwrap();

        let instr = create_csr_instr(0b111, 10, 15, machine::MSTATUS); // zimm=15
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        // Check that bits were cleared with immediate (0xFF & ~0x0F = 0xF0)
        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x00F0);

        // Check that x10 received the old value
        assert_eq!(state.regs[10], 0x00FF);
    }
}
