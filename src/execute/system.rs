//! System instruction execution
//!
use crate::core::PrivilegeMode;
use crate::csr::machine;

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

/// MRET - Return from Machine mode trap
///
/// Reads the saved PC from MEPC, restores MIE from MPIE,
/// and restores the previous privilege mode from MPP.
///
/// # Operation
/// PC = MEPC
/// MIE = MPIE
/// MPP = [previous mode]
#[inline]
pub fn exec_mret(
    _instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u32, ExecuteError> {
    // Read current mstatus
    let mstatus = state
        .csr
        .read(machine::MSTATUS)
        .map_err(ExecuteError::CsrError)?;

    // Extract MPIE (bit 7) and MPP (bits 12:11)
    let mpie = (mstatus >> 7) & 1;
    let mpp = (mstatus >> 11) & 0b11;

    // Check if we're allowed to return from M-mode
    // In a real implementation, we would check privilege levels
    // For now, we assume mret is allowed from M-mode

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
        _ => PrivilegeMode::Machine, // Reserved, default to M-mode
    };

    // Return the new PC (MEPC value)
    Ok(mepc)
}

/// SRET - Return from Supervisor mode trap
///
/// Reads the saved PC from SEPC, restores SIE from SPIE,
/// and restores the previous privilege mode from SPP.
///
/// # Operation
/// PC = SEPC
/// SIE = SPIE
/// SPP = [previous mode]
#[inline]
pub fn exec_sret(
    _instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u32, ExecuteError> {
    use crate::core::PrivilegeMode;
    use crate::csr::supervisor;

    // Read current sstatus (or mstatus with SSTATUS bits)
    let sstatus = state
        .csr
        .read(supervisor::SSTATUS)
        .map_err(ExecuteError::CsrError)?;

    // Extract SPIE (bit 5) and SPP (bit 8)
    let spie = (sstatus >> 5) & 1;
    let spp = (sstatus >> 8) & 1;

    // Check if we can execute sret (requires S-mode or M-mode)
    if state.privilege == PrivilegeMode::User {
        return Err(ExecuteError::InvalidOperation);
    }

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

    // Return the new PC (SEPC value)
    Ok(sepc)
}

/// URET - Return from User mode trap (not implemented in RISC-V base)
///
/// This instruction is not part of the standard RISC-V privilege spec.
/// It would be used for returning to user mode from a higher privilege level.
#[inline]
pub fn exec_uret(
    _instr: &DecodedInstruction,
    _state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<u32, ExecuteError> {
    // URET is not defined in standard RISC-V
    // Some implementations may use it as a custom instruction
    Err(ExecuteError::InvalidOperation)
}

#[cfg(test)]
mod mret_sret_tests {
    use super::*;
    use crate::core::PrivilegeMode;
    use crate::csr::supervisor;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_mret_instr() -> DecodedInstruction {
        DecodedInstruction {
            raw: 0x30200073, // MRET instruction encoding
            format: InstructionFormat::RType,
            opcode: Opcode::System,
            funct3: None,
            funct7: Some(0b001_1000),
            rs1: None,
            rs2: Some(0b00010), // rs2 = 2 for MRET
            rd: None,
            imm: None,
            branch_taken: false,
        }
    }

    fn create_sret_instr() -> DecodedInstruction {
        DecodedInstruction {
            raw: 0x10200073, // SRET instruction encoding
            format: InstructionFormat::RType,
            opcode: Opcode::System,
            funct3: None,
            funct7: Some(0b000_1000),
            rs1: None,
            rs2: Some(0b00010), // rs2 = 2 for SRET
            rd: None,
            imm: None,
            branch_taken: false,
        }
    }

    #[test]
    fn test_mret_basic() {
        let mut state = CoreState::default();

        // Set up MEPC and mstatus
        state.csr.write(machine::MEPC, 0x1000).unwrap();
        state
            .csr
            .write(machine::MSTATUS, 0x0000_0080) // MPIE = 1
            .unwrap();

        let instr = create_mret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_mret(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0x1000);

        // Check that MIE was restored from MPIE
        let mstatus = state.csr.read(machine::MSTATUS).unwrap();
        assert_eq!((mstatus >> 3) & 1, 1); // MIE = 1
    }

    #[test]
    fn test_mret_restore_mpp() {
        let mut state = CoreState::default();

        // Set up MEPC and mstatus with MPP = Supervisor
        state.csr.write(machine::MEPC, 0x2000).unwrap();
        state
            .csr
            .write(machine::MSTATUS, 0x0000_0880) // MPIE = 1, MPP = 01 (S-mode)
            .unwrap();

        let instr = create_mret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        exec_mret(&instr, &mut state, &mut mem).unwrap();

        // Check that privilege mode was restored to Supervisor
        assert_eq!(state.privilege, PrivilegeMode::Supervisor);
    }

    #[test]
    fn test_mret_from_s_mode() {
        let mut state = CoreState {
            privilege: PrivilegeMode::Supervisor,
            ..Default::default()
        };

        // Set up MEPC and mstatus with MPP = Machine
        state.csr.write(machine::MEPC, 0x3000).unwrap();
        state
            .csr
            .write(machine::MSTATUS, 0x0000_1880) // MPIE = 1, MPP = 11 (M-mode)
            .unwrap();

        let instr = create_mret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        exec_mret(&instr, &mut state, &mut mem).unwrap();

        // Check that privilege mode was restored to Machine
        assert_eq!(state.privilege, PrivilegeMode::Machine);
    }

    #[test]
    fn test_sret_basic() {
        let mut state = CoreState {
            privilege: PrivilegeMode::Supervisor,
            ..Default::default()
        };

        // Set up SEPC and sstatus
        state.csr.write(supervisor::SEPC, 0x1000).unwrap();
        state
            .csr
            .write(supervisor::SSTATUS, 0x0000_0020) // SPIE = 1
            .unwrap();

        let instr = create_sret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_sret(&instr, &mut state, &mut mem);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0x1000);

        // Check that SIE was restored from SPIE
        let sstatus = state.csr.read(supervisor::SSTATUS).unwrap();
        assert_eq!((sstatus >> 1) & 1, 1); // SIE = 1
    }

    #[test]
    fn test_sret_restore_spp() {
        let mut state = CoreState {
            privilege: PrivilegeMode::Supervisor,
            ..Default::default()
        };

        // Set up SEPC and sstatus with SPP = User
        state.csr.write(supervisor::SEPC, 0x2000).unwrap();
        state
            .csr
            .write(supervisor::SSTATUS, 0x0000_0120) // SPIE = 1, SPP = 0 (U-mode)
            .unwrap();

        let instr = create_sret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        exec_sret(&instr, &mut state, &mut mem).unwrap();

        // Check that privilege mode was restored to User
        assert_eq!(state.privilege, PrivilegeMode::User);
    }

    #[test]
    fn test_sret_from_machine_mode() {
        let mut state = CoreState {
            privilege: PrivilegeMode::Machine,
            ..Default::default()
        };

        // Set up SEPC and sstatus with SPP = Supervisor
        state.csr.write(supervisor::SEPC, 0x3000).unwrap();
        state
            .csr
            .write(supervisor::SSTATUS, 0x0000_0120) // SPIE = 1, SPP = 1 (S-mode)
            .unwrap();

        let instr = create_sret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        exec_sret(&instr, &mut state, &mut mem).unwrap();

        // Check that privilege mode was restored to Supervisor
        assert_eq!(state.privilege, PrivilegeMode::Supervisor);
    }

    #[test]
    fn test_sret_from_user_mode_fails() {
        let mut state = CoreState {
            privilege: PrivilegeMode::User,
            ..Default::default()
        };

        let instr = create_sret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        let result = exec_sret(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::InvalidOperation)));
    }

    #[test]
    fn test_mret_clears_mie() {
        let mut state = CoreState::default();

        // Set up with MPIE = 0 (interrupts disabled when trap was taken)
        state.csr.write(machine::MEPC, 0x1000).unwrap();
        state
            .csr
            .write(machine::MSTATUS, 0x0000_0000) // MPIE = 0
            .unwrap();

        let instr = create_mret_instr();
        let mut mem = SimpleMemory::new(0x1000);

        exec_mret(&instr, &mut state, &mut mem).unwrap();

        // Check that MIE was restored from MPIE (which was 0)
        let mstatus = state.csr.read(machine::MSTATUS).unwrap();
        assert_eq!((mstatus >> 3) & 1, 0); // MIE = 0
    }
}
