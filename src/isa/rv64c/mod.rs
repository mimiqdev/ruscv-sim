//! RV64C Compressed Instruction Extension
//!
//! This module implements the RISC-V Compressed (C) extension for RV64.
//! Compressed instructions are 16-bit versions of common 32-bit RV64I operations.
//!
//! ## Overview
//!
//! The C extension reduces code size by providing 16-bit encodings for frequently
//! used instructions. It is divided into three quadrants:
//!
//! - **C0 (00)**: Load/Store operations, stack pointer relative operations
//! - **C1 (01)**: Arithmetic, logic, branches, and jumps
//! - **C2 (10)**: Register-based operations and stack pointer memory access
//!
//! ## Implemented Instructions
//!
//! ### C0 Quadrant
//! - `C.ADDI4SPN` - Add immediate to stack pointer (x2), scaled by 4
//! - `C.LW` - Load word
//! - `C.LD` - Load doubleword (RV64)
//! - `C.SW` - Store word
//! - `C.SD` - Store doubleword (RV64)
//!
//! ### C1 Quadrant
//! - `C.ADDI` - Add immediate
//! - `C.ADDIW` - Add immediate word (RV64)
//! - `C.LI` - Load immediate
//! - `C.LUI` - Load upper immediate
//! - `C.ADDI16SP` - Add immediate to stack pointer (16-bit scaled)
//! - `C.NOP` - No operation
//! - `C.SRLI` - Shift right logical immediate
//! - `C.SRAI` - Shift right arithmetic immediate
//! - `C.ANDI` - AND immediate
//! - `C.SUB` - Subtract
//! - `C.XOR` - XOR
//! - `C.OR` - OR
//! - `C.AND` - AND
//! - `C.SUBW` - Subtract word (RV64)
//! - `C.ADDW` - Add word (RV64)
//! - `C.J` - Unconditional jump
//! - `C.JR` - Jump register
//! - `C.JALR` - Jump and link register
//! - `C.BEQZ` - Branch if equal to zero
//! - `C.BNEZ` - Branch if not equal to zero
//!
//! ### C2 Quadrant
//! - `C.SLLI` - Shift left logical immediate
//! - `C.LWSP` - Load word from stack pointer
//! - `C.LDSP` - Load doubleword from stack pointer (RV64)
//! - `C.SWSP` - Store word to stack pointer
//! - `C.SDSP` - Store doubleword to stack pointer (RV64)
//! - `C.MV` - Move register
//! - `C.ADD` - Add registers
//! - `C.EBREAK` - Environment breakpoint
//!
//! ## Usage
//!
//! ```rust
//! use ruscv_sim::isa::rv64c::CompressedDecoder;
//!
//! let decoder = CompressedDecoder::new();
//! let compressed_inst: u16 = 0x6105; // Example: c.addi x2, -32
//! match decoder.decode_16bit(compressed_inst) {
//!     Ok(decoded) => println!("Decoded: {:?}", decoded),
//!     Err(e) => println!("Decode error: {:?}", e),
//! }
//! ```

pub mod c0_quadw;
pub mod c1_addiw;
pub mod c1_arith;
pub mod c1_branch;
pub mod c1_jump;
pub mod c1_shift;
pub mod c1_zero;
pub mod c2_move;
pub mod c2_stack;
pub mod decoder_16bit;

pub use decoder_16bit::{COpcode, CQuadrant, CompressedDecoder};

/// Check if an instruction is compressed (16-bit)
pub fn is_compressed(instruction: u32) -> bool {
    decoder_16bit::CompressedDecoder::is_compressed(instruction)
}

/// Get the instruction length in bits
pub fn instruction_length(instruction: u32) -> u8 {
    decoder_16bit::CompressedDecoder::instruction_length(instruction)
}

/// Re-export execution functions for compressed instructions
pub use c0_quadw::{exec_c_addi4spn, exec_c_ld, exec_c_lw, exec_c_sd, exec_c_sw};
pub use c1_addiw::{exec_c_addiw, exec_c_addw, exec_c_subw};
pub use c1_arith::{
    exec_c_add, exec_c_addi, exec_c_addi16sp, exec_c_and, exec_c_andi, exec_c_li, exec_c_lui,
    exec_c_mv, exec_c_or, exec_c_sub, exec_c_xor,
};
pub use c1_branch::{exec_c_beqz, exec_c_bnez};
pub use c1_jump::{exec_c_j, exec_c_jal};
pub use c1_shift::{exec_c_srai, exec_c_srli};
pub use c1_zero::exec_c_nop;
pub use c2_move::{exec_c_ebreak, exec_c_jalr, exec_c_jr};
pub use c2_stack::{exec_c_ldsp, exec_c_lwsp, exec_c_sdsp, exec_c_slli, exec_c_swsp};

/// Execute a decoded compressed instruction
///
/// This is a convenience function that decodes and executes a 16-bit
/// compressed instruction in one step.
pub fn execute_compressed(
    inst: u16,
    state: &mut crate::core::CoreState,
    mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), crate::execute::ExecuteError> {
    let decoder = CompressedDecoder::new();
    let decoded = decoder
        .decode_16bit(inst)
        .map_err(|_| crate::execute::ExecuteError::InvalidOperation)?;

    let mut executor = crate::execute::Executor::new();
    executor.execute(&decoded, state, mem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compressed_detection() {
        // Instructions with lower 2 bits != 11 are compressed
        assert!(is_compressed(0x0000));
        assert!(is_compressed(0x0001));
        assert!(is_compressed(0x0002));
        assert!(!is_compressed(0x0003));
        assert!(!is_compressed(0xFFFFFFFF));
    }

    #[test]
    fn test_instruction_lengths() {
        assert_eq!(instruction_length(0x0000), 16);
        assert_eq!(instruction_length(0x0003), 32);
    }
}
