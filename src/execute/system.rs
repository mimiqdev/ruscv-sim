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

#[cfg(test)]
mod tests {
    use super::*;
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
}
