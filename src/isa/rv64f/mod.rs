//! RV64F Single-Precision Floating-Point Extension
//!
//! This module implements the RISC-V 64-bit Single-Precision Floating-Point
//! Extension (RV64F). These instructions operate on 32-bit IEEE 754 floating-point values.
//!
//! ## Implemented Instructions
//!
//! ### Arithmetic
//! - `FADD.S`: Floating-point Add
//! - `FSUB.S`: Floating-point Subtract
//! - `FMUL.S`: Floating-point Multiply
//!
//! ### Load/Store
//! - `FLW`: Load Word (32-bit float)
//! - `FSW`: Store Word (32-bit float)
//!
//! ### Comparison
//! - `FEQ.S`: Floating-point Equal
//! - `FLT.S`: Floating-point Less Than
//! - `FLE.S`: Floating-point Less or Equal
//!
//! ### Conversion
//! - `FCVT.W.S`: Convert float to 32-bit signed integer
//! - `FCVT.WU.S`: Convert float to 32-bit unsigned integer
//! - `FCVT.L.S`: Convert float to 64-bit signed integer
//! - `FCVT.LU.S`: Convert float to 64-bit unsigned integer
//! - `FCVT.S.W`: Convert 32-bit signed integer to float
//! - `FCVT.S.WU`: Convert 32-bit unsigned integer to float
//! - `FCVT.S.L`: Convert 64-bit signed integer to float
//! - `FCVT.S.LU`: Convert 64-bit unsigned integer to float
//!
//! ### Classification
//! - `FCLASS.S`: Floating-point Classify
//!
//! ### Division and Square Root
//! - `FDIV.S`: Floating-point Divide
//! - `FSQRT.S`: Floating-point Square Root
//!
//! ### Fused Multiply-Add
//! - `FMADD.S`: Fused Multiply-Add
//! - `FMSUB.S`: Fused Multiply-Subtract
//! - `FNMADD.S`: Negated Fused Multiply-Add
//! - `FNMSUB.S`: Negated Fused Multiply-Subtract

pub mod arith;
pub mod classify;
pub mod compare;
pub mod convert;
pub mod div_sqrt;
pub mod load_store;
pub mod madd;

// Re-export arithmetic functions
pub use arith::{exec_fadd_s, exec_fmul_s, exec_fsub_s};

// Re-export classify function
pub use classify::exec_fclass_s;

// Re-export comparison functions
pub use compare::{exec_feq_s, exec_fle_s, exec_flt_s};

// Re-export conversion functions
pub use convert::{
    exec_fcvt_l_s, exec_fcvt_lu_s, exec_fcvt_s_l, exec_fcvt_s_lu, exec_fcvt_s_w, exec_fcvt_s_wu,
    exec_fcvt_w_s, exec_fcvt_wu_s,
};

// Re-export division and square root functions
pub use div_sqrt::{exec_fdiv_s, exec_fsqrt_s};

// Re-export load/store functions
pub use load_store::{exec_flw, exec_fsw};

// Re-export fused multiply-add functions
pub use madd::{exec_fmadd_s, exec_fmsub_s, exec_fnmadd_s, exec_fnmsub_s};
