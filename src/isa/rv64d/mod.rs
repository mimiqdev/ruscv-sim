//! RV64D Double-Precision Floating-Point Extension
//!
//! This module implements the RISC-V 64-bit Double-Precision Floating-Point
//! Extension (RV64D). These instructions operate on 64-bit IEEE 754 floating-point values.
//!
//! ## Implemented Instructions
//!
//! ### Arithmetic
//! - `FADD.D`: Floating-point Add
//! - `FSUB.D`: Floating-point Subtract
//! - `FMUL.D`: Floating-point Multiply
//!
//! ### Load/Store
//! - `FLD`: Load Doubleword (64-bit float)
//! - `FSD`: Store Doubleword (64-bit float)
//!
//! ### Comparison
//! - `FEQ.D`: Floating-point Equal
//! - `FLT.D`: Floating-point Less Than
//! - `FLE.D`: Floating-point Less or Equal
//!
//! ### Conversion
//! - `FCVT.W.D`: Convert double to 32-bit signed integer
//! - `FCVT.WU.D`: Convert double to 32-bit unsigned integer
//! - `FCVT.L.D`: Convert double to 64-bit signed integer
//! - `FCVT.LU.D`: Convert double to 64-bit unsigned integer
//! - `FCVT.D.W`: Convert 32-bit signed integer to double
//! - `FCVT.D.WU`: Convert 32-bit unsigned integer to double
//! - `FCVT.D.L`: Convert 64-bit signed integer to double
//! - `FCVT.D.LU`: Convert 64-bit unsigned integer to double
//! - `FCVT.D.S`: Convert single to double
//! - `FCVT.S.D`: Convert double to single
//!
//! ### Classification
//! - `FCLASS.D`: Floating-point Classify
//!
//! ### Division and Square Root
//! - `FDIV.D`: Floating-point Divide
//! - `FSQRT.D`: Floating-point Square Root
//!
//! ### Fused Multiply-Add
//! - `FMADD.D`: Fused Multiply-Add
//! - `FMSUB.D`: Fused Multiply-Subtract
//! - `FNMADD.D`: Negated Fused Multiply-Add
//! - `FNMSUB.D`: Negated Fused Multiply-Subtract

pub mod arith;
pub mod classify;
pub mod compare;
pub mod convert;
pub mod div_sqrt;
pub mod load_store;
pub mod madd;

// Re-export arithmetic functions
pub use arith::{exec_fadd_d, exec_fmul_d, exec_fsub_d};

// Re-export classify function
pub use classify::exec_fclass_d;

// Re-export comparison functions
pub use compare::{exec_feq_d, exec_fle_d, exec_flt_d};

// Re-export conversion functions
pub use convert::{
    exec_fcvt_d_l, exec_fcvt_d_lu, exec_fcvt_d_s, exec_fcvt_d_w, exec_fcvt_d_wu, exec_fcvt_l_d,
    exec_fcvt_lu_d, exec_fcvt_s_d, exec_fcvt_w_d, exec_fcvt_wu_d,
};

// Re-export division and square root functions
pub use div_sqrt::{exec_fdiv_d, exec_fsqrt_d};

// Re-export load/store functions
pub use load_store::{exec_fld, exec_fsd};

// Re-export fused multiply-add functions
pub use madd::{exec_fmadd_d, exec_fmsub_d, exec_fnmadd_d, exec_fnmsub_d};
