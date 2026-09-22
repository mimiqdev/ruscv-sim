//! RV64A Atomic Instruction Extension
//!
//! This module implements the RISC-V 64-bit Atomic Instruction Extension
//! (RV64A: the Zalrsc LR/SC and Zamo AMO profiles of dev-plan §2/§5.3).
//! Atomic instructions provide synchronization primitives for
//! multi-processor systems.
//!
//! ## Dispatch
//!
//! `dispatch` is the Hart-owned decode/dispatch authority: `funct5` selects
//! the operation, `funct3` selects W or D, reserved encodings and LR with
//! `rs2 != 0` are illegal-instruction traps, and `aq`/`rl` are accepted in
//! all four combinations with no additional ordering effect.
//!
//! ## Routes
//!
//! - Port-configured cores issue one atomic envelope per instruction through
//!   the validated data port ([`crate::physical::AtomicRequest`]).
//! - The old `RiscvCore::new` typed constructor keeps the
//!   `MemoryInterface` helper route as a labeled non-conforming
//!   compatibility adapter; it shares the same per-Hart reservation record.
//!
//! ## Reservation
//!
//! [`ReservationSet`] is the per-Hart reservation record stored in
//! [`crate::core::CoreState::reservation`]; the process-global
//! `GLOBAL_RESERVATION` singleton and `clear_reservation` were removed under
//! the approved A8 profile (dev-plan C23, §11 item 5).

pub mod amo;
pub(crate) mod dispatch;
pub mod lr_sc;

// Re-export LR/SC functions and the per-Hart reservation record.
pub use lr_sc::{exec_lr, exec_lr_w, exec_sc, exec_sc_w, ReservationSet};

// Re-export the typed-route AMO helpers (compatibility adapter surface).
pub use amo::{
    exec_amoadd, exec_amoand, exec_amomax, exec_amomaxu, exec_amomin, exec_amominu, exec_amoor,
    exec_amoswap, exec_amoxor,
};

// Hart dispatch entry points used by `core`/`execute`.
pub(crate) use dispatch::{amo_encoding_is_legal, execute_amo_port, execute_amo_typed};
