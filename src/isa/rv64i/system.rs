//! RV64I System Operations
//!
//! This module implements the system instructions for RV64I:
//! - ECALL: Environment Call
//! - EBREAK: Environment Break
//! - CSR operations: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI
//! - Trap returns: MRET, SRET, URET

use crate::core::{CoreState, PrivilegeMode};
use crate::csr::{machine, supervisor};
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// Execute system instructions (RV64I)
///
/// # Operations
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
                0x302 => {
                    // MRET
                    let target = exec_mret(instr, state, _mem)?;
                    state.pc = target;
                    state.branch_taken = true;
                    Ok(())
                }
                0x102 => {
                    // SRET
                    let target = exec_sret(instr, state, _mem)?;
                    state.pc = target;
                    state.branch_taken = true;
                    Ok(())
                }
                _ => Err(ExecuteError::InvalidOperation),
            }
        }
        0b001 => exec_csrrw(instr, state, imm),
        0b010 => exec_csrrs(instr, state, imm),
        0b011 => exec_csrrc(instr, state, imm),
        0b101 => exec_csrrwi(instr, state, imm),
        0b110 => exec_csrrsi(instr, state, imm),
        0b111 => exec_csrrci(instr, state, imm),
        _ => Err(ExecuteError::InvalidOperation),
    }
}

/// CSRRW - CSR Read-Write
fn exec_csrrw(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
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

/// CSRRS - CSR Read-Set
fn exec_csrrs(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
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

/// CSRRC - CSR Read-Clear
fn exec_csrrc(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
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

/// CSRRWI - CSR Read-Write Immediate
fn exec_csrrwi(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
    let csr_addr = (imm & 0xFFF) as u16;
    let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u64;
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

/// CSRRSI - CSR Read-Set Immediate
fn exec_csrrsi(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
    let csr_addr = (imm & 0xFFF) as u16;
    let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u64;
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

/// CSRRCI - CSR Read-Clear Immediate
fn exec_csrrci(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    imm: u32,
) -> Result<(), ExecuteError> {
    let csr_addr = (imm & 0xFFF) as u16;
    let zimm = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as u64;
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

/// MRET - Return from Machine mode trap
///
/// Reads the saved PC from MEPC, restores MIE from MPIE,
/// and restores the previous privilege mode from MPP.
///
/// # Returns
/// The new PC value (MEPC)
#[inline]
pub fn exec_mret(
    _instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u64, ExecuteError> {
    // Read current mstatus
    let mstatus = state
        .csr
        .read(machine::MSTATUS)
        .map_err(ExecuteError::CsrError)?;

    // Extract MPIE (bit 7) and MPP (bits 12:11)
    let mpie = (mstatus >> 7) & 1;
    let mpp = (mstatus >> 11) & 0b11;

    // Read MEPC for the return address
    let mepc = state
        .csr
        .read(machine::MEPC)
        .map_err(ExecuteError::CsrError)?;

    // Restore MIE from MPIE
    let new_mstatus = (mstatus & !0x8) | (mpie << 3);

    // Clear MPP bits
    let new_mstatus = new_mstatus & !(0b11 << 11);

    // Write back the new mstatus
    state
        .csr
        .write(machine::MSTATUS, new_mstatus)
        .map_err(ExecuteError::CsrError)?;

    // Set privilege mode based on MPP
    state.privilege = match mpp {
        0b00 => PrivilegeMode::User,
        0b01 => PrivilegeMode::Supervisor,
        0b11 => PrivilegeMode::Machine,
        _ => PrivilegeMode::Machine,
    };

    Ok(mepc)
}

/// SRET - Return from Supervisor mode trap
///
/// Reads the saved PC from SEPC, restores SIE from SPIE,
/// and restores the previous privilege mode from SPP.
///
/// # Returns
/// The new PC value (SEPC)
#[inline]
pub fn exec_sret(
    _instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u64, ExecuteError> {
    // Check if we can execute sret (requires S-mode or M-mode)
    if state.privilege == PrivilegeMode::User {
        return Err(ExecuteError::InvalidOperation);
    }

    // Read current sstatus
    let sstatus = state
        .csr
        .read(supervisor::SSTATUS)
        .map_err(ExecuteError::CsrError)?;

    // Extract SPIE (bit 5) and SPP (bit 8)
    let spie = (sstatus >> 5) & 1;
    let spp = (sstatus >> 8) & 1;

    // Read SEPC for the return address
    let sepc = state
        .csr
        .read(supervisor::SEPC)
        .map_err(ExecuteError::CsrError)?;

    // Restore SIE from SPIE
    let new_sstatus = (sstatus & !0x2) | (spie << 1);

    // Clear SPP bit
    let new_sstatus = new_sstatus & !(1 << 8);

    // Write back the new sstatus
    state
        .csr
        .write(supervisor::SSTATUS, new_sstatus)
        .map_err(ExecuteError::CsrError)?;

    // Set privilege mode based on SPP
    state.privilege = if spp == 1 {
        PrivilegeMode::Supervisor
    } else {
        PrivilegeMode::User
    };

    Ok(sepc)
}

/// URET - Return from User mode trap
///
/// This instruction is not part of the standard RISC-V privilege spec.
#[inline]
pub fn exec_uret(
    _instr: &DecodedInstruction,
    _state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u64, ExecuteError> {
    // URET is not defined in standard RISC-V
    Err(ExecuteError::InvalidOperation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_test_instr(imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::System,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rs3: None,
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
            rs3: None,
            rd: Some(rd),
            imm: Some(csr as u32),
            branch_taken: false,
        }
    }

    #[test]
    fn test_ecall() {
        let mut state = CoreState::default();
        let instr = create_test_instr(0);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::Ecall)));
    }

    #[test]
    fn test_ebreak() {
        let mut state = CoreState::default();
        let instr = create_test_instr(1);
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
        state.csr.write(machine::MSTATUS, 0x0080).unwrap();
        state.regs[5] = 0x0008;

        let instr = create_csr_instr(0b010, 10, 5, machine::MSTATUS);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x0088);
        assert_eq!(state.regs[10], 0x0080);
    }

    #[test]
    fn test_csrrc() {
        let mut state = CoreState::default();
        state.csr.write(machine::MSTATUS, 0x0088).unwrap();
        state.regs[5] = 0x0008;

        let instr = create_csr_instr(0b011, 10, 5, machine::MSTATUS);
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_system(&instr, &mut state, &mut mem);
        assert!(result.is_ok());

        assert_eq!(state.csr.read(machine::MSTATUS).unwrap(), 0x0080);
        assert_eq!(state.regs[10], 0x0088);
    }

    #[test]
    fn test_mret() {
        let mut state = CoreState::default();
        state.csr.write(machine::MEPC, 0x1000).unwrap();
        state.csr.write(machine::MSTATUS, 0x0000_0080).unwrap();

        let instr = DecodedInstruction {
            raw: 0x30200073,
            format: InstructionFormat::RType,
            opcode: Opcode::System,
            funct3: None,
            funct7: Some(0b001_1000),
            rs1: None,
            rs2: Some(0b00010),
            rs3: None,
            rd: None,
            imm: None,
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mret(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0x1000);

        let mstatus = state.csr.read(machine::MSTATUS).unwrap();
        assert_eq!((mstatus >> 3) & 1, 1);
    }

    #[test]
    fn test_sret() {
        let mut state = CoreState {
            privilege: PrivilegeMode::Supervisor,
            ..Default::default()
        };
        state.csr.write(supervisor::SEPC, 0x1000).unwrap();
        state.csr.write(supervisor::SSTATUS, 0x0000_0020).unwrap();

        let instr = DecodedInstruction {
            raw: 0x10200073,
            format: InstructionFormat::RType,
            opcode: Opcode::System,
            funct3: None,
            funct7: Some(0b000_1000),
            rs1: None,
            rs2: Some(0b00010),
            rs3: None,
            rd: None,
            imm: None,
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_sret(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0x1000);

        let sstatus = state.csr.read(supervisor::SSTATUS).unwrap();
        assert_eq!((sstatus >> 1) & 1, 1);
    }

    #[test]
    fn test_sret_from_user_fails() {
        let mut state = CoreState {
            privilege: PrivilegeMode::User,
            ..Default::default()
        };

        let instr = DecodedInstruction {
            raw: 0x10200073,
            format: InstructionFormat::RType,
            opcode: Opcode::System,
            funct3: None,
            funct7: Some(0b000_1000),
            rs1: None,
            rs2: Some(0b00010),
            rs3: None,
            rd: None,
            imm: None,
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_sret(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::InvalidOperation)));
    }
}
