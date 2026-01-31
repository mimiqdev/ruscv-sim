//! System instruction execution
//!
//! System instructions handle system-level operations.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// System instructions (exec_system)
///
/// Executes system instructions including:
/// - ECALL: Environment call (system call)
/// - EBREAK: Environment break (debugger breakpoint)
#[inline]
pub fn exec_system(
    instr: &DecodedInstruction,
    _state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let Some(imm) = instr.imm else {
        return Err(ExecuteError::InvalidOperation);
    };

    match imm {
        0 => Err(ExecuteError::Ecall),
        1 => Err(ExecuteError::Ebreak),
        _ => Err(ExecuteError::InvalidOperation),
    }
}
