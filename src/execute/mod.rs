//!
//! Execute module
//!
//! Instruction execution dispatcher with re-exports from ISA modules.

use crate::core::CoreState;
use crate::csr::{CsrAccess, CsrError};
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
    #[error("Illegal instruction")]
    IllegalInstruction,
    #[error("ECALL exception")]
    Ecall,
    #[error("EBREAK exception")]
    Ebreak,
    #[error("Memory error: {0}")]
    MemoryError(#[from] MemoryError),
    #[error("CSR error: {0}")]
    CsrError(#[from] CsrError),
}

// RV64A re-exports (from isa::rv64a).  The process-global reservation
// singleton and `clear_reservation` were removed under A8 (dev-plan C23);
// the per-Hart reservation record lives in `CoreState`.
pub use crate::isa::rv64a::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoswap, exec_amoxor, exec_lr, exec_lr_w, exec_sc, exec_sc_w,
};

// RV64I re-exports (from isa::rv64i)
pub use crate::isa::rv64i::{
    exec_auipc, exec_branch, exec_fence, exec_fence_i, exec_jal, exec_jalr, exec_load, exec_lui,
    exec_op, exec_op_32, exec_op_imm, exec_op_imm_32, exec_shift, exec_shift_imm, exec_store,
    exec_system, exec_system_with_csr_access,
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

    /// Execute decoded instruction while retaining the CSR write
    /// classification for the retirement boundary.
    ///
    /// The existing [`Self::execute`] API intentionally remains the public
    /// compatibility entry point.  The core uses this companion method so a
    /// CSRRS/CSRRC read-only access cannot be confused with an explicit
    /// `minstret` write that has the same resulting value.
    pub fn execute_with_csr_access(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<Option<CsrAccess>, ExecuteError> {
        self.execute_transition(instr, state, mem)
            .map(|(csr, _)| csr)
    }

    /// The executed dispatch arm supplies destination intent, including equal
    /// writes. This metadata accompanies execution; observers do not decode.
    pub(crate) fn execute_transition(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(Option<CsrAccess>, Option<u8>), ExecuteError> {
        if instr.opcode == Opcode::System {
            return exec_system_with_csr_access(instr, state, mem)
                .map(|access| (access, access.and(instr.rd)));
        }

        let result = match instr.opcode {
            Opcode::Lui => exec_lui(instr, state, mem),
            Opcode::Auipc => exec_auipc(instr, state, mem),
            Opcode::Jal => exec_jal(instr, state, mem),
            Opcode::Jalr => exec_jalr(instr, state, mem),
            Opcode::Branch => exec_branch(instr, state, mem),
            Opcode::Load => exec_load(instr, state, mem),
            Opcode::Store => exec_store(instr, state, mem),
            Opcode::OpImm => {
                // Dispatch to shift or ALU based on funct3.
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
                // Check funct7 first to distinguish RV64M (mul/div) from
                // RV64I (ALU).
                let funct7 = instr.funct7.unwrap_or(0);
                if funct7 == 0b000_0001 {
                    self.execute_rv64m(instr, state, mem)
                } else if let Some(funct3) = instr.funct3 {
                    match funct3 {
                        Funct3::Sll | Funct3::SrlSra => exec_shift(instr, state, mem),
                        _ => exec_op(instr, state, mem),
                    }
                } else {
                    Err(ExecuteError::InvalidOperation)
                }
            }
            Opcode::OpImm32 => {
                if let Some(funct3) = instr.funct3 {
                    match funct3 {
                        Funct3::Sll | Funct3::SrlSra | Funct3::AddSub => {
                            exec_op_imm_32(instr, state, mem)
                        }
                        _ => Err(ExecuteError::InvalidOperation),
                    }
                } else {
                    Err(ExecuteError::InvalidOperation)
                }
            }
            Opcode::Op32 if instr.funct7 == Some(0b000_0001) => {
                match instr.funct3.map(|f| f as u8) {
                    Some(0b000) => crate::isa::rv64m::exec_mulw(instr, state, mem),
                    Some(0b100) => crate::isa::rv64m::exec_divw(instr, state, mem),
                    Some(0b101) => crate::isa::rv64m::exec_divuw(instr, state, mem),
                    Some(0b110) => crate::isa::rv64m::exec_remw(instr, state, mem),
                    Some(0b111) => crate::isa::rv64m::exec_remuw(instr, state, mem),
                    _ => Err(ExecuteError::InvalidOperation),
                }
            }
            Opcode::Op32 => {
                if let Some(funct3) = instr.funct3 {
                    match funct3 {
                        Funct3::Sll | Funct3::SrlSra | Funct3::AddSub => {
                            exec_op_32(instr, state, mem)
                        }
                        _ => Err(ExecuteError::InvalidOperation),
                    }
                } else {
                    Err(ExecuteError::InvalidOperation)
                }
            }
            Opcode::LoadFp
            | Opcode::StoreFp
            | Opcode::OpFp
            | Opcode::Madd
            | Opcode::Msub
            | Opcode::Nmsub
            | Opcode::Nmadd => {
                return crate::isa::fd::execute(instr, state, mem).map(|rd| (None, rd))
            }
            Opcode::Amo => self.execute_amo(instr, state, mem),
            Opcode::MiscMem => match instr.funct3.map(|value| value as u8) {
                Some(0) => exec_fence(instr, state, mem),
                Some(1) => exec_fence_i(instr, state, mem),
                _ => Err(ExecuteError::IllegalInstruction),
            },
            // SYSTEM is handled above so this arm is unreachable, but keeping
            // it explicit makes the dispatcher exhaustive if Opcode grows.
            Opcode::System => unreachable!("SYSTEM was handled before dispatch"),
        };

        result.map(|_| {
            (
                None,
                match instr.opcode {
                    Opcode::Store | Opcode::Branch | Opcode::MiscMem => None,
                    _ => instr.rd,
                },
            )
        })
    }

    /// Execute decoded instruction.
    pub fn execute(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        self.execute_with_csr_access(instr, state, mem).map(|_| ())
    }

    /// Execute RV64M (Multiplication and Division) instructions
    fn execute_rv64m(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let funct3 = instr.funct3.map(|f| f as u8).unwrap_or(0);

        match funct3 {
            // MUL: Multiply (lower 64 bits)
            0b000 => exec_mul(instr, state, mem),
            // MULH: Multiply Signed * Signed (upper 64 bits)
            0b001 => exec_mulh(instr, state, mem),
            // MULHSU: Multiply Signed * Unsigned (upper 64 bits)
            0b010 => exec_mulhsu(instr, state, mem),
            // MULHU: Multiply Unsigned * Unsigned (upper 64 bits)
            0b011 => exec_mulhu(instr, state, mem),
            // DIV: Divide Signed
            0b100 => exec_div(instr, state, mem),
            // DIVU: Divide Unsigned
            0b101 => exec_divu(instr, state, mem),
            // REM: Remainder Signed
            0b110 => exec_rem(instr, state, mem),
            // REMU: Remainder Unsigned
            0b111 => exec_remu(instr, state, mem),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }

    /// Execute AMO/LR/SC instructions on the typed compatibility route
    /// (dev-plan §5.3, C24).
    ///
    /// The dispatcher follows the approved §2 table through
    /// [`crate::isa::rv64a`]'s single Hart-owned decode: `funct5` selects
    /// the operation (AMOSWAP included), `funct3` selects W or D, reserved
    /// `funct5` values and LR with `rs2 != 0` are illegal instructions, and
    /// `aq`/`rl` are legal in all four combinations with no added ordering
    /// effect.  Port-configured cores never reach this method: the Hart
    /// issues one atomic envelope per instruction instead.
    fn execute_amo(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        crate::isa::rv64a::execute_amo_typed(instr, state, mem)
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
