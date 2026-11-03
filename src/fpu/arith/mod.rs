//! Bit-exact IEEE 754-2008 arithmetic for the RV64 F and D extensions.
//!
//! This is the F/D arithmetic core required by contract unit U2
//! (`docs/dev-plan.md`, decision D4). It is a Rust port of the subset of
//! [Berkeley SoftFloat Release 3e] that the F and D extensions need, plus the
//! RISC-V-mandated results that SoftFloat leaves to its platform layer:
//!
//! * every operation takes an explicit [`Rounding`] mode and returns the
//!   result bits together with the exact [`FpFlags`] set in architectural
//!   `fflags` bit order (`NX=1`, `UF=2`, `OF=4`, `DZ=8`, `NV=16`), with
//!   tininess detected after rounding;
//! * NaN results of arithmetic, square root, fused multiply-add, conversion
//!   and `FMIN`/`FMAX` are the canonical NaN (`0x7fc00000` / `0x7ff8000000000000`);
//! * sign injection, classification and the comparisons take and return raw
//!   format bits, so payloads are preserved or inspected bit for bit;
//! * 32-bit integer conversion results are sign-extended to 64 bits and
//!   out-of-range conversion results follow the RISC-V saturation table.
//!
//! The core performs no host `f32`/`f64` arithmetic: it operates on integer
//! bit patterns only, so results do not depend on the host floating-point
//! environment, compiler flags or CPU.
//!
//! The module is a Hart-internal component. It has no CSR, register-file or
//! decode dependency; `single`/`double` take and return format bit patterns
//! and the caller owns NaN-boxing, `mstatus.FS` and `fcsr` accrual.
//!
//! Portions of this module are a Rust port of Berkeley SoftFloat Release 3e
//! (Copyright 2011-2018 The Regents of the University of California). See
//! `tools/fpu/SOFTFLOAT-LICENSE.txt` for the copyright notice, conditions and
//! disclaimer.
//!
//! [Berkeley SoftFloat Release 3e]: http://www.jhauser.us/arithmetic/SoftFloat.html

mod common;
pub mod double;
pub mod single;

#[cfg(test)]
mod tests;

pub use common::{FpFlags, Rounding};
