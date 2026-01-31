//!
//! Execute module
//!
//! RV32I instruction execution - refactored by instruction type

// Re-export sub-modules
pub mod amo;
pub mod b_type; // Branch instructions
pub mod div; // RV64M divide instructions
pub mod f_arith; // RV64F arithmetic instructions
pub mod f_classify; // RV64F classification instruction
pub mod f_compare; // RV64F comparison instructions
pub mod f_convert; // RV64F conversion instructions
pub mod f_div_sqrt; // RV64F division and square root
pub mod f_load_store; // RV64F load/store instructions
pub mod f_madd; // RV64F fused multiply-add instructions
pub mod i_type; // I-type instructions
pub mod j_type; // Jump instructions
pub mod lr_sc; // RV64A load-reserved/store-conditional instructions
pub mod mul; // RV64M multiply instructions
pub mod r_type; // R-type instructions
pub mod s_type; // S-type instructions
pub mod system; // System instructions
pub mod u_type; // U-type instructions // RV64A atomic memory operation instructions

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
    #[error("Misaligned memory access: addr 0x{0:08x}, alignment {1}")]
    MisalignedAccess(u32, u32),
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

pub use self::amo::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoxor,
};
pub use self::b_type::exec_branch;
pub use self::div::{exec_div, exec_divu, exec_rem, exec_remu};
pub use self::f_arith::{exec_fadd_s, exec_fmul_s, exec_fsub_s};
pub use self::f_classify::exec_fclass_s;
pub use self::f_compare::{exec_feq_s, exec_fle_s, exec_flt_s};
pub use self::f_convert::{
    exec_fcvt_l_s, exec_fcvt_lu_s, exec_fcvt_s_l, exec_fcvt_s_lu, exec_fcvt_s_w, exec_fcvt_s_wu,
    exec_fcvt_w_s, exec_fcvt_wu_s,
};
pub use self::f_div_sqrt::{exec_fdiv_s, exec_fsqrt_s};
pub use self::f_load_store::{exec_flw, exec_fsd};
pub use self::f_madd::{exec_fmadd_s, exec_fmsub_s, exec_fnmadd_s, exec_fnmsub_s};
pub use self::i_type::{exec_load, exec_op_imm};
pub use self::j_type::{exec_jal, exec_jalr};
pub use self::lr_sc::{clear_reservation, exec_lr, exec_lr_w, exec_sc, exec_sc_w};
pub use self::mul::{exec_mul, exec_mulh, exec_mulhsu, exec_mulhu};
pub use self::r_type::exec_op;
pub use self::s_type::exec_store;
pub use self::system::exec_system;
pub use self::u_type::{exec_auipc, exec_lui};

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
            Opcode::LoadFp => exec_flw(instr, state, mem),
            Opcode::StoreFp => exec_fsd(instr, state, mem),
            Opcode::OpImm => exec_op_imm(instr, state, mem),
            Opcode::Op => exec_op(instr, state, mem),
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

        match (funct7, funct3) {
            // FADD.S
            (0x00, 0) => exec_fadd_s(instr, state, mem),
            // FSUB.S
            (0x04, 0) => exec_fsub_s(instr, state, mem),
            // FMUL.S
            (0x08, 0) => exec_fmul_s(instr, state, mem),
            // FDIV.S
            (0x0C, 0) => exec_fdiv_s(instr, state, mem),
            // FSQRT.S
            (0x2C, 0) => exec_fsqrt_s(instr, state, mem),
            // FSGNJ.S, FSGNJN.S, FSGNJX.S
            (0x10, 0) => exec_fadd_s(instr, state, mem), // Placeholder
            // FMIN.S, FMAX.S
            (0x14, 0) => exec_fadd_s(instr, state, mem), // Placeholder
            // FCVT.W.S, FCVT.L.S
            (0x60, 0) => exec_fcvt_w_s(instr, state, mem),
            // FCVT.WU.S, FCVT.LU.S
            (0x61, 0) => exec_fcvt_wu_s(instr, state, mem),
            // FMV.X.W
            (0x70, 0) => exec_fcvt_w_s(instr, state, mem), // Placeholder
            // FCLASS.S
            (0x70, 1) => exec_fclass_s(instr, state, mem),
            // FCVT.S.W, FCVT.S.L
            (0x68, 0) => exec_fcvt_s_w(instr, state, mem),
            // FCVT.S.WU, FCVT.S.LU
            (0x69, 0) => exec_fcvt_s_wu(instr, state, mem),
            // FMV.W.X
            (0x78, 0) => exec_fcvt_s_w(instr, state, mem), // Placeholder
            // FEQ.S, FLT.S, FLE.S
            (0x50, 0) => exec_feq_s(instr, state, mem),
            // FMSUB.S
            (0x01, 0) => exec_fmsub_s(instr, state, mem),
            // FNMSUB.S
            (0x02, 0) => exec_fnmsub_s(instr, state, mem),
            // FNMADD.S
            (0x03, 0) => exec_fnmadd_s(instr, state, mem),
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
