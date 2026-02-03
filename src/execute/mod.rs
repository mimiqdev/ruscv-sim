//!
//! Execute module
//!
//! Instruction execution dispatcher with re-exports from ISA modules.

use crate::core::CoreState;
use crate::csr::CsrError;
use crate::decode::Funct3;
use crate::decode::{DecodedInstruction, Opcode};
use crate::memory::{MemoryError, MemoryInterface};
use thiserror::Error;

/// Instruction executor function type (3 args - no Executor needed)
pub type ExecutorFn =
    fn(&DecodedInstruction, &mut CoreState, &mut dyn MemoryInterface) -> Result<(), ExecuteError>;

/// Execution error
#[derive(Error, Debug)]
pub enum ExecuteError {
    #[error("Misaligned memory access: addr 0x{0:016x}, alignment {1}")]
    MisalignedAccess(u64, u32),
    #[error("Invalid register access: x{0}")]
    InvalidRegister(u8),
    #[error("Invalid operation")]
    InvalidOperation,
    #[error("ECALL exception")]
    Ecall,
    #[error("EBREAK exception")]
    Ebreak,
    #[error("Memory error: {0}")]
    MemoryError(#[from] MemoryError),
    #[error("CSR error: {0}")]
    CsrError(#[from] CsrError),
}

// RV64A re-exports (from isa::rv64a)
pub use crate::isa::rv64a::{
    clear_reservation, exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin,
    exec_amominu, exec_amoor, exec_amoxor, exec_lr, exec_lr_w, exec_sc, exec_sc_w,
};

// RV64I re-exports (from isa::rv64i)
pub use crate::isa::rv64i::{
    exec_auipc, exec_branch, exec_jal, exec_jalr, exec_load, exec_lui, exec_op, exec_op_imm,
    exec_shift, exec_shift_imm, exec_store, exec_system,
};

// RV64D re-exports (from isa::rv64d)
pub use crate::isa::rv64d::{
    exec_fadd_d, exec_fclass_d, exec_fcvt_d_l, exec_fcvt_d_lu, exec_fcvt_d_s, exec_fcvt_d_w,
    exec_fcvt_d_wu, exec_fcvt_l_d, exec_fcvt_lu_d, exec_fcvt_s_d, exec_fcvt_w_d, exec_fcvt_wu_d,
    exec_fdiv_d, exec_feq_d, exec_fld, exec_fle_d, exec_flt_d, exec_fmadd_d, exec_fmsub_d,
    exec_fmul_d, exec_fnmadd_d, exec_fnmsub_d, exec_fsd as exec_fsd_d, exec_fsqrt_d, exec_fsub_d,
};

// RV64F re-exports (from isa::rv64f)
pub use crate::isa::rv64f::{
    exec_fadd_s, exec_fclass_s, exec_fcvt_l_s, exec_fcvt_lu_s, exec_fcvt_s_l, exec_fcvt_s_lu,
    exec_fcvt_s_w, exec_fcvt_s_wu, exec_fcvt_w_s, exec_fcvt_wu_s, exec_fdiv_s, exec_feq_s,
    exec_fle_s, exec_flt_s, exec_flw, exec_fmadd_s, exec_fmsub_s, exec_fmul_s, exec_fnmadd_s,
    exec_fnmsub_s, exec_fsqrt_s, exec_fsub_s, exec_fsw,
};

// RV64M re-exports (from isa::rv64m)
pub use crate::isa::rv64m::{
    exec_div, exec_divu, exec_mul, exec_mulh, exec_mulhsu, exec_mulhu, exec_rem, exec_remu,
};

/// Executor
#[derive(Debug)]
pub struct Executor {}

impl Executor {
    /// Create new executor
    pub fn new() -> Self {
        Self {}
    }

    /// Execute decoded instruction
    pub fn execute(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        match instr.opcode {
            Opcode::Lui => exec_lui(instr, state, mem),
            Opcode::Auipc => exec_auipc(instr, state, mem),
            Opcode::Jal => exec_jal(instr, state, mem),
            Opcode::Jalr => exec_jalr(instr, state, mem),
            Opcode::Branch => exec_branch(instr, state, mem),
            Opcode::Load => exec_load(instr, state, mem),
            Opcode::Store => exec_store(instr, state, mem),
            Opcode::LoadFp => self.execute_fpload(instr, state, mem),
            Opcode::StoreFp => self.execute_fpstore(instr, state, mem),
            Opcode::OpImm => {
                // Dispatch to shift or ALU based on funct3
                // SLLI, SRLI, SRAI use funct3 Sll or SrlSra
                if let Some(funct3) = instr.funct3 {
                    match funct3 {
                        Funct3::Sll | Funct3::SrlSra => exec_shift_imm(instr, state, mem),
                        _ => exec_op_imm(instr, state, mem),
                    }
                } else {
                    Err(ExecuteError::InvalidOperation)
                }
            }
            Opcode::Op => {
                // Dispatch to shift or ALU based on funct3
                if let Some(funct3) = instr.funct3 {
                    match funct3 {
                        Funct3::Sll | Funct3::SrlSra => exec_shift(instr, state, mem),
                        _ => exec_op(instr, state, mem),
                    }
                } else {
                    Err(ExecuteError::InvalidOperation)
                }
            }
            Opcode::OpFp => self.execute_fpu(instr, state, mem),
            Opcode::System => exec_system(instr, state, mem),
            Opcode::Amo => self.execute_amo(instr, state, mem),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// Execute FPU (Floating-Point Unit) instructions
    fn execute_fpu(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let funct7 = instr.funct7.unwrap_or(0);
        let funct3 = instr.funct3.map(|f| f as u8).unwrap_or(0);
        let is_d_extension = (funct7 & 0x20) != 0; // Bit 5 set for D extension

        match (funct7 & 0x1F, funct3, is_d_extension) {
            // FADD.S / FADD.D
            (0x00, 0, false) => exec_fadd_s(instr, state, mem),
            (0x00, 0, true) => exec_fadd_d(instr, state, mem),
            // FSUB.S / FSUB.D
            (0x04, 0, false) => exec_fsub_s(instr, state, mem),
            (0x04, 0, true) => exec_fsub_d(instr, state, mem),
            // FMUL.S / FMUL.D
            (0x08, 0, false) => exec_fmul_s(instr, state, mem),
            (0x08, 0, true) => exec_fmul_d(instr, state, mem),
            // FDIV.S / FDIV.D
            (0x0C, 0, false) => exec_fdiv_s(instr, state, mem),
            (0x0C, 0, true) => exec_fdiv_d(instr, state, mem),
            // FSQRT.S / FSQRT.D
            (0x2C, 0, false) => exec_fsqrt_s(instr, state, mem),
            (0x2C, 0, true) => exec_fsqrt_d(instr, state, mem),
            // FCLASS.S / FCLASS.D
            (0x70, 1, false) => exec_fclass_s(instr, state, mem),
            (0x70, 1, true) => exec_fclass_d(instr, state, mem),
            // FCVT.W.S / FCVT.L.S / FCVT.W.D / FCVT.L.D
            (0x60, 0, false) => exec_fcvt_w_s(instr, state, mem),
            (0x60, 0, true) => exec_fcvt_l_d(instr, state, mem),
            // FCVT.WU.S / FCVT.LU.S / FCVT.WU.D / FCVT.LU.D
            (0x61, 0, false) => exec_fcvt_wu_s(instr, state, mem),
            (0x61, 0, true) => exec_fcvt_lu_d(instr, state, mem),
            // FCVT.S.W / FCVT.S.L / FCVT.D.W / FCVT.D.L
            (0x68, 0, false) => exec_fcvt_s_w(instr, state, mem),
            (0x68, 0, true) => exec_fcvt_d_l(instr, state, mem),
            // FCVT.S.WU / FCVT.S.LU / FCVT.D.WU / FCVT.D.LU
            (0x69, 0, false) => exec_fcvt_s_wu(instr, state, mem),
            (0x69, 0, true) => exec_fcvt_d_lu(instr, state, mem),
            // FCVT.S.D (Single from Double) / FCVT.D.S (Double from Single)
            (0x40, 0, false) => exec_fcvt_s_d(instr, state, mem),
            (0x40, 0, true) => exec_fcvt_d_s(instr, state, mem),
            // FCVT.W.D / FCVT.L.D
            (0x41, 0, false) => exec_fcvt_w_s(instr, state, mem), // Not used
            (0x41, 0, true) => exec_fcvt_w_d(instr, state, mem),
            // FCVT.WU.D / FCVT.LU.D
            #[allow(unreachable_patterns)]
            (0x41, 0, false) => exec_fcvt_wu_s(instr, state, mem), // Not used
            #[allow(unreachable_patterns)]
            (0x41, 0, true) => exec_fcvt_wu_d(instr, state, mem),
            // FCVT.D.W / FCVT.D.WU
            (0x43, 0, false) => exec_fcvt_s_w(instr, state, mem), // Not used
            (0x43, 0, true) => exec_fcvt_d_w(instr, state, mem),
            // FCVT.D.W / FCVT.D.WU
            (0x42, 0, false) => exec_fcvt_s_wu(instr, state, mem), // Not used
            (0x42, 0, true) => exec_fcvt_d_wu(instr, state, mem),
            // FEQ.S / FLT.S / FLE.S / FEQ.D / FLT.D / FLE.D
            (0x50, 0, false) => exec_feq_s(instr, state, mem),
            (0x50, 0, true) => exec_feq_d(instr, state, mem),
            // FMSUB.S / FMSUB.D
            (0x01, 0, false) => exec_fmsub_s(instr, state, mem),
            (0x01, 0, true) => exec_fmsub_d(instr, state, mem),
            // FNMSUB.S / FNMSUB.D
            (0x02, 0, false) => exec_fnmsub_s(instr, state, mem),
            (0x02, 0, true) => exec_fnmsub_d(instr, state, mem),
            // FNMADD.S / FNMADD.D
            (0x03, 0, false) => exec_fnmadd_s(instr, state, mem),
            (0x03, 0, true) => exec_fnmadd_d(instr, state, mem),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// Execute Floating-Point Load instructions
    fn execute_fpload(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let funct3 = instr.funct3.map(|f| f as u8).unwrap_or(0);

        match funct3 {
            // FLW (Load 32-bit float)
            0x02 => exec_flw(instr, state, mem),
            // FLD (Load 64-bit double)
            0x03 => exec_fld(instr, state, mem),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// Execute Floating-Point Store instructions
    fn execute_fpstore(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let funct3 = instr.funct3.map(|f| f as u8).unwrap_or(0);

        match funct3 {
            // FSW (Store 32-bit float)
            0x02 => exec_fsw(instr, state, mem),
            // FSD (Store 64-bit double)
            0x03 => exec_fsd_d(instr, state, mem),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// Execute AMO (Atomic Memory Operation) instructions
    fn execute_amo(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let funct5 = (instr.raw >> 27) as u8;

        match funct5 {
            // LR/SC (Load-Reserved / Store-Conditional)
            0b00010 => {
                // LR or LR.W
                let _aq = ((instr.raw >> 26) & 1) as u8;
                let _rl = ((instr.raw >> 25) & 1) as u8;
                if instr.rs2 == Some(0) {
                    // LR or LR.W (rs2 = 0)
                    if instr.funct3 == Some(Funct3::Slt) {
                        exec_lr(instr, state, mem)
                    } else {
                        exec_lr_w(instr, state, mem)
                    }
                } else {
                    // SC or SC.W (rs2 != 0)
                    if instr.funct3 == Some(Funct3::Slt) {
                        exec_sc(instr, state, mem)
                    } else {
                        exec_sc_w(instr, state, mem)
                    }
                }
            }
            0b00011 => exec_sc(instr, state, mem), // SC (fallback)

            // AMO operations
            0b00001 => exec_amoadd(instr, state, mem), // AMOADD
            0b00111 => exec_amoand(instr, state, mem), // AMOAND
            0b00100 => exec_amoxor(instr, state, mem), // AMOXOR
            0b00110 => exec_amoor(instr, state, mem),  // AMOOR
            0b01000 => exec_amomin(instr, state, mem), // AMOMIN
            0b01001 => exec_amominu(instr, state, mem), // AMOMINU
            0b01010 => exec_amomax(instr, state, mem), // AMOMAX
            0b01011 => exec_amomaxu(instr, state, mem), // AMOMAXU
            _ => Err(ExecuteError::InvalidOperation),
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_executor_creation() {
        // Just verify the Executor struct can be created
        let _ = super::Executor::new();
    }
}
