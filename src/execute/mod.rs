//!
//! Execute module
//!
//! RV32I instruction execution - refactored by instruction type

// Re-export sub-modules
pub mod amo;
pub mod b_type; // Branch instructions
pub mod d_arith; // RV64D arithmetic instructions
pub mod d_classify; // RV64D classification instruction
pub mod d_compare; // RV64D comparison instructions
pub mod d_convert; // RV64D conversion instructions
pub mod d_div_sqrt; // RV64D division and square root
pub mod d_load_store; // RV64D load/store instructions
pub mod d_madd; // RV64D fused multiply-add instructions
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

pub use self::amo::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoxor,
};
pub use self::b_type::exec_branch;
pub use self::d_arith::{exec_fadd_d, exec_fmul_d, exec_fsub_d};
pub use self::d_classify::exec_fclass_d;
pub use self::d_compare::{exec_feq_d, exec_fle_d, exec_flt_d};
pub use self::d_convert::{
    exec_fcvt_d_l, exec_fcvt_d_lu, exec_fcvt_d_s, exec_fcvt_d_w, exec_fcvt_d_wu, exec_fcvt_l_d,
    exec_fcvt_lu_d, exec_fcvt_s_d, exec_fcvt_w_d, exec_fcvt_wu_d,
};
pub use self::d_div_sqrt::{exec_fdiv_d, exec_fsqrt_d};
pub use self::d_load_store::{exec_fld, exec_fsd as exec_fsd_d};
pub use self::d_madd::{exec_fmadd_d, exec_fmsub_d, exec_fnmadd_d, exec_fnmsub_d};
pub use self::div::{exec_div, exec_divu, exec_rem, exec_remu};
pub use self::f_arith::{exec_fadd_s, exec_fmul_s, exec_fsub_s};
pub use self::f_classify::exec_fclass_s;
pub use self::f_compare::{exec_feq_s, exec_fle_s, exec_flt_s};
pub use self::f_convert::{
    exec_fcvt_l_s, exec_fcvt_lu_s, exec_fcvt_s_l, exec_fcvt_s_lu, exec_fcvt_s_w, exec_fcvt_s_wu,
    exec_fcvt_w_s, exec_fcvt_wu_s,
};
pub use self::f_div_sqrt::{exec_fdiv_s, exec_fsqrt_s};
pub use self::f_load_store::{exec_flw, exec_fsw};
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
            Opcode::LoadFp => self.execute_fpload(instr, state, mem),
            Opcode::StoreFp => self.execute_fpstore(instr, state, mem),
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
