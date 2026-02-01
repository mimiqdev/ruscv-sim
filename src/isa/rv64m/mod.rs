//! RV64M Integer Multiplication and Division Extension
//!
//! This module implements the RISC-V 64-bit Integer Multiplication and Division
//! Extension (RV64M). These instructions are part of the M extension.
//!
//! ## Implemented Instructions
//!
//! ### Multiplication
//! - `MUL`: Multiply (lower 64 bits)
//! - `MULH`: Multiply Signed * Signed (upper 64 bits)
//! - `MULHU`: Multiply Unsigned * Unsigned (upper 64 bits)
//! - `MULHSU`: Multiply Signed * Unsigned (upper 64 bits)
//!
//! ### Division
//! - `DIV`: Divide Signed
//! - `DIVU`: Divide Unsigned
//! - `REM`: Remainder Signed
//! - `REMU`: Remainder Unsigned

pub mod div;
pub mod mul;

// Re-export all multiplication functions
pub use mul::{exec_mul, exec_mulh, exec_mulhsu, exec_mulhu};

// Re-export all division functions
pub use div::{exec_div, exec_divu, exec_rem, exec_remu};
