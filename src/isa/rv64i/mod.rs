//! RV64I Base Integer Instruction Set
//!
//! This module implements the RISC-V 64-bit Base Integer Instruction Set (RV64I).
//! RV64I is the base integer instruction set for 64-bit RISC-V processors.
//!
//! ## Instruction Categories
//!
//! - **ALU**: Arithmetic and logical operations (`alu`)
//! - **Shift**: Shift operations (`shift`)
//! - **Load**: Memory load operations (`load`)
//! - **Store**: Memory store operations (`store`)
//! - **Branch**: Conditional branch operations (`branch`)
//! - **Jump**: Unconditional jump operations (`jump`)
//! - **LUI/AUIPC**: Upper immediate operations (`lui_auipc`)
//! - **System**: System and CSR operations (`system`)
//! - **Fence**: Memory ordering operation (`fence`)
//!
//! ## Implemented Instructions
//!
//! ### Register-Register Operations
//! - `ADD`, `SUB`: Addition, Subtraction
//! - `SLL`: Shift Left Logical
//! - `SLT`, `SLTU`: Set Less Than (signed/unsigned)
//! - `XOR`: Exclusive OR
//! - `SRL`, `SRA`: Shift Right Logical/Arithmetic
//! - `OR`: Logical OR
//! - `AND`: Logical AND
//!
//! ### Immediate Operations
//! - `ADDI`: Add Immediate
//! - `SLTI`, `SLTIU`: Set Less Than Immediate (signed/unsigned)
//! - `XORI`: XOR Immediate
//! - `ORI`: OR Immediate
//! - `ANDI`: AND Immediate
//! - `SLLI`: Shift Left Logical Immediate
//! - `SRLI`, `SRAI`: Shift Right Logical/Arithmetic Immediate
//!
//! ### Load Operations
//! - `LB`, `LH`, `LW`, `LD`: Load byte/half/word/doubleword
//! - `LBU`, `LHU`, `LWU`: Load unsigned
//!
//! ### Store Operations
//! - `SB`, `SH`, `SW`, `SD`: Store byte/half/word/doubleword
//!
//! ### Branch Operations
//! - `BEQ`, `BNE`: Branch if equal/not equal
//! - `BLT`, `BGE`: Branch if less/greater or equal (signed)
//! - `BLTU`, `BGEU`: Branch if less/greater or equal (unsigned)
//!
//! ### Jump Operations
//! - `JAL`: Jump and Link
//! - `JALR`: Jump and Link Register
//!
//! ### Upper Immediate Operations
//! - `LUI`: Load Upper Immediate
//! - `AUIPC`: Add Upper Immediate to PC
//!
//! ### System Operations
//! - `ECALL`, `EBREAK`: Environment call/break
//! - `CSRRW`, `CSRRS`, `CSRRC`: CSR read-write/read-set/read-clear
//! - `CSRRWI`, `CSRRSI`, `CSRRCI`: CSR immediate variants
//! - `MRET`, `SRET`, `URET`: Return from trap
//!
//! ### Fence Operations
//! - `FENCE`: Memory ordering (all `pred`/`succ`/`fm` encodings, including
//!   `FENCE.TSO` and HINTs), a no-op on this synchronous single-Hart machine.
//!   `FENCE.I` is Zifencei and remains unimplemented.

pub mod alu;
pub mod branch;
pub mod fence;
pub mod jump;
pub mod load;
pub mod lui_auipc;
pub mod op32;
pub mod shift;
pub mod store;
pub mod system;

// Re-export ALU functions
pub use alu::{exec_op, exec_op_imm};

// Re-export shift functions
pub use shift::{exec_shift, exec_shift_imm};

// Re-export 32-bit operation functions
pub use op32::{exec_op_32, exec_op_imm_32};

// Re-export load/store functions
pub use load::exec_load;
pub use store::exec_store;

// Re-export branch functions
pub use branch::exec_branch;

// Re-export FENCE (MISC-MEM) functions
pub use fence::exec_fence;

// Re-export jump functions
pub use jump::{exec_jal, exec_jalr};

// Re-export LUI/AUIPC functions
pub use lui_auipc::{exec_auipc, exec_lui};

// Re-export system functions
pub use system::{exec_mret, exec_sret, exec_system, exec_uret};

/// Execute RV64I instruction based on opcode
///
/// This is a convenience function that dispatches to the appropriate
/// execution function based on the instruction opcode.
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state
/// * `mem` - Memory interface
///
/// # Returns
/// `Ok(())` on success, `Err(ExecuteError)` on failure
pub fn execute(
    instr: &crate::decode::DecodedInstruction,
    state: &mut crate::core::CoreState,
    mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), crate::execute::ExecuteError> {
    use crate::decode::Opcode;

    match instr.opcode {
        Opcode::Lui => exec_lui(instr, state, mem),
        Opcode::Auipc => exec_auipc(instr, state, mem),
        Opcode::Jal => exec_jal(instr, state, mem),
        Opcode::Jalr => exec_jalr(instr, state, mem),
        Opcode::Branch => exec_branch(instr, state, mem),
        Opcode::Load => exec_load(instr, state, mem),
        Opcode::Store => exec_store(instr, state, mem),
        Opcode::OpImm => {
            // Dispatch to shift or ALU based on funct3
            if let Some(funct3) = instr.funct3 {
                use crate::decode::Funct3;
                match funct3 {
                    Funct3::Sll | Funct3::SrlSra => exec_shift_imm(instr, state, mem),
                    _ => exec_op_imm(instr, state, mem),
                }
            } else {
                Err(crate::execute::ExecuteError::InvalidOperation)
            }
        }
        Opcode::Op => {
            // Dispatch to shift or ALU based on funct3
            if let Some(funct3) = instr.funct3 {
                use crate::decode::Funct3;
                match funct3 {
                    Funct3::Sll | Funct3::SrlSra => exec_shift(instr, state, mem),
                    _ => exec_op(instr, state, mem),
                }
            } else {
                Err(crate::execute::ExecuteError::InvalidOperation)
            }
        }
        Opcode::OpImm32 => {
            // Dispatch to shift (for SLLIW/SRLIW/SRAIW) or ALU (for ADDIW)
            if let Some(funct3) = instr.funct3 {
                use crate::decode::Funct3;
                match funct3 {
                    Funct3::Sll | Funct3::SrlSra | Funct3::AddSub => {
                        exec_op_imm_32(instr, state, mem)
                    }
                    _ => Err(crate::execute::ExecuteError::InvalidOperation),
                }
            } else {
                Err(crate::execute::ExecuteError::InvalidOperation)
            }
        }
        Opcode::Op32 => {
            // Dispatch to shift (for SLLW/SRLW/SRAW) or ALU (for ADDW/SUBW)
            if let Some(funct3) = instr.funct3 {
                use crate::decode::Funct3;
                match funct3 {
                    Funct3::Sll | Funct3::SrlSra | Funct3::AddSub => exec_op_32(instr, state, mem),
                    _ => Err(crate::execute::ExecuteError::InvalidOperation),
                }
            } else {
                Err(crate::execute::ExecuteError::InvalidOperation)
            }
        }
        Opcode::System => exec_system(instr, state, mem),
        Opcode::MiscMem => exec_fence(instr, state, mem),
        _ => Err(crate::execute::ExecuteError::InvalidOperation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CoreState;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    #[test]
    fn test_execute_lui() {
        let mut state = CoreState::default();
        let instr = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::UType,
            opcode: Opcode::Lui,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rs3: None,
            rd: Some(1),
            imm: Some(0x12345000),
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x12345000);
    }

    #[test]
    fn test_execute_addi() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        let instr = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::AddSub),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rs3: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_execute_add() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;
        let instr = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(Funct3::AddSub),
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };
        let mut mem = SimpleMemory::new(0x1000);

        execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    /// `isa::rv64i::execute` and the active `Executor::execute` dispatcher
    /// must agree on FENCE (`Opcode::MiscMem`). Before this fix, this
    /// module's dispatcher fell through to its `_` arm and returned
    /// `Err(InvalidOperation)` for a decoded FENCE while `Executor::execute`
    /// returned `Ok(())` -- the same decoded instruction, two different
    /// answers. This test pins agreement so the two dispatchers cannot
    /// silently drift apart again.
    #[test]
    fn test_execute_fence_agrees_with_executor() {
        use crate::decode::InstructionDecoder;
        use crate::execute::Executor;

        let word: u32 = 0x0ff0000f; // fence (iorw, iorw)
        let decoded = InstructionDecoder::new().decode(word).unwrap();
        assert_eq!(decoded.opcode, Opcode::MiscMem);

        let mut state_a = CoreState::default();
        state_a.regs[1] = 0x1111_1111_0000_0001;
        let before_regs = state_a.regs;
        let mut mem_a = SimpleMemory::new(0x1000);

        let mut state_b = state_a.clone();
        let mut mem_b = SimpleMemory::new(0x1000);

        let result_a = execute(&decoded, &mut state_a, &mut mem_a);
        let result_b = Executor::new().execute(&decoded, &mut state_b, &mut mem_b);

        assert!(result_a.is_ok(), "isa::rv64i::execute must accept FENCE");
        assert!(result_b.is_ok(), "Executor::execute must accept FENCE");
        assert_eq!(state_a.regs, before_regs, "FENCE must not write any GPR");
        assert_eq!(state_b.regs, before_regs, "FENCE must not write any GPR");
        assert!(!state_a.branch_taken);
        assert!(!state_b.branch_taken);
    }
}
