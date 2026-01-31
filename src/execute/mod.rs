//!
//! Execute module
//!
//! RV32I instruction execution - refactored by instruction type

// Re-export sub-modules
pub mod b_type; // Branch instructions
pub mod i_type; // I-type instructions
pub mod j_type; // Jump instructions
pub mod r_type; // R-type instructions
pub mod s_type; // S-type instructions
pub mod system; // System instructions
pub mod u_type; // U-type instructions

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Opcode};
use crate::memory::{MemoryError, MemoryInterface};
use thiserror::Error;

/// Instruction executor function type
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
}

// Re-export executor functions from sub-modules
pub use self::b_type::exec_branch;
pub use self::i_type::{exec_load, exec_op_imm};
pub use self::j_type::{exec_jal, exec_jalr};
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
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = Executor::new();
        // Can't test execution without valid instruction
        assert!(true);
    }
}
