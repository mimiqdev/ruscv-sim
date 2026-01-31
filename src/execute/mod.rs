//!
//! Execute module
//!
//! RV32I instruction execution - refactored by instruction type

// Re-export sub-modules
pub mod amo;
pub mod b_type; // Branch instructions
pub mod div; // RV64M divide instructions
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
            Opcode::OpImm => exec_op_imm(instr, state, mem),
            Opcode::Op => exec_op(instr, state, mem),
            Opcode::System => exec_system(instr, state, mem),
            Opcode::Amo => self.execute_amo(instr, state, mem),
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
                let aq = ((instr.raw >> 26) & 1) as u8;
                let rl = ((instr.raw >> 25) & 1) as u8;
                if instr.rs2 == Some(0) {
                    // LR or LR.W (rs2 = 0)
                    if instr.funct3 == Some(0b010) {
                        exec_lr(instr, state, mem)
                    } else {
                        exec_lr_w(instr, state, mem)
                    }
                } else {
                    // SC or SC.W (rs2 != 0)
                    if instr.funct3 == Some(0b010) {
                        exec_sc(instr, state, mem)
                    } else {
                        exec_sc_w(instr, state, mem)
                    }
                }
            }
            0b00011 => exec_sc(instr, state, mem), // SC (fallback)

            // AMO operations
            0b00001 => exec_amoadd(instr, state, mem), // AMOADD
            0b00011 => exec_amoand(instr, state, mem), // AMOAND
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
