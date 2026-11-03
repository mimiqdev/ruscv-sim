//! Bit-exact IEEE 754-2008 arithmetic for the RV64 F and D extensions.
//!
//! This is the F/D arithmetic core required by contract unit U2
//! (`docs/dev-plan.md`, decision D4). It is a Rust port of the subset of
//! [Berkeley SoftFloat Release 3e] that the F and D extensions need, plus the
//! RISC-V-mandated results that SoftFloat leaves to its platform layer:
//!
//! * every operation takes an explicit [`Rounding`] mode and returns the
//!   result bits together with the exact [`FpFlags`] set (`NV`, `DZ`, `OF`,
//!   `UF`, `NX`), with tininess detected after rounding;
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
//! [Berkeley SoftFloat Release 3e]: http://www.jhauser.us/arithmetic/SoftFloat.html

mod common;
pub mod double;
pub mod single;

#[cfg(test)]
mod tests;

pub use common::{FpFlags, Rounding};
