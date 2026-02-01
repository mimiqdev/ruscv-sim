//! RV64A Load-Reserved / Store-Conditional instructions
//!
//! This module re-exports the LR/SC implementation from `isa::rv64a::lr_sc`.
//!
//! Tests are located in `src/isa/rv64a/lr_sc.rs`.
//!
//! # Reservation Mechanism
//!
//! LR/SC provides atomic read-modify-write operations:
//! - LR loads a value and creates a reservation on the memory location
//! - SC attempts to store only if the reservation is still valid
//! - If successful, returns 0; otherwise returns non-zero
//!
//! # References
//!
//! - RISC-V ISA Volume I: Unprivileged Spec, Section 8.3 (Load-Reserved/Store-Conditional)
//! - RISC-V ISA Volume II: Privileged Spec, Section 3.5.1 (Reservation Granularity)

// Re-export all public items from the canonical implementation in isa::rv64a
pub use crate::isa::rv64a::{
    clear_reservation, exec_lr, exec_lr_w, exec_sc, exec_sc_w, ReservationSet,
};
