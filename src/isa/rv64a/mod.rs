//! RV64A Atomic Instruction Extension
//!
//! This module implements the RISC-V 64-bit Atomic Instruction Extension (RV64A).
//! Atomic instructions provide synchronization primitives for multi-processor systems.
//!
//! ## Implemented Instructions
//!
//! ### Load-Reserved / Store-Conditional
//! - `LR`: Load Reserved (64-bit)
//! - `LR.W`: Load Reserved Word (32-bit)
//! - `SC`: Store Conditional (64-bit)
//! - `SC.W`: Store Conditional Word (32-bit)
//!
//! ### Atomic Memory Operations (AMO)
//! - `AMOADD`: Atomic Add
//! - `AMOAND`: Atomic AND
//! - `AMOOR`: Atomic OR
//! - `AMOXOR`: Atomic XOR
//! - `AMOMAX`: Atomic Maximum (signed)
//! - `AMOMAXU`: Atomic Maximum (unsigned)
//! - `AMOMIN`: Atomic Minimum (signed)
//! - `AMOMINU`: Atomic Minimum (unsigned)

pub mod amo;
pub mod lr_sc;

// Re-export LR/SC functions
pub use lr_sc::{clear_reservation, exec_lr, exec_lr_w, exec_sc, exec_sc_w};

// Re-export AMO functions
pub use amo::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoxor,
};
