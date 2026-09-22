//! The single Hart-owned AMO arithmetic module (dev-plan §5.1, §5.2 M1).
//!
//! Every AMO read-modify-write operation's arithmetic — swap, add, bitwise
//! and/or/xor, and the signed/unsigned min/max comparisons — exists exactly
//! once, here in the Hart layer, identically for every backend (ADR-0002 §6;
//! dev-plan §5.2 M1).  The module applies the spec-mandated W/D width rules
//! (dev-plan §2) and produces the pure byte-level transforms carried by the
//! atomic operation envelopes ([`crate::physical::AmoTransform`]).  The
//! physical domain and its backends apply those transforms opaquely and
//! implement no ISA semantics themselves.
//!
//! Byte order: all byte slices are in physical/guest memory order, which for
//! RV64 is little-endian.  W envelopes compute 32-bit results from the
//! operand's low 32 bits and write a zero-extended 32-bit value (the low four
//! bytes of the output buffer); D envelopes compute full-width results.
//!
//! This module performs no memory access, holds no reservation state, and is
//! deliberately not referenced by any execution path at T1; the Hart dispatch
//! integrates it in a later task (dev-plan §8 T3).

use crate::physical::{AmoTransform, AmoWidth, PureAmoTransform, MAX_PHYSICAL_ACCESS_BYTES};
use std::fmt;

/// The nine AMO read-modify-write operations of the approved `funct5` table
/// (dev-plan §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmoOperation {
    /// AMOSWAP: the operand replaces the old span bytes.
    Swap,
    /// AMOADD: wrapping addition.
    Add,
    /// AMOXOR: bitwise exclusive or.
    BitXor,
    /// AMOAND: bitwise and.
    BitAnd,
    /// AMOOR: bitwise or.
    BitOr,
    /// AMOMIN: signed minimum.
    Min,
    /// AMOMINU: unsigned minimum.
    Minu,
    /// AMOMAX: signed maximum.
    Max,
    /// AMOMAXU: unsigned maximum.
    Maxu,
}

/// Every AMO operation in `funct5` table order, for exhaustive callers.
pub const ALL_OPERATIONS: [AmoOperation; 9] = [
    AmoOperation::Swap,
    AmoOperation::Add,
    AmoOperation::BitXor,
    AmoOperation::BitAnd,
    AmoOperation::BitOr,
    AmoOperation::Min,
    AmoOperation::Minu,
    AmoOperation::Max,
    AmoOperation::Maxu,
];

impl fmt::Display for AmoOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Swap => "amoswap",
            Self::Add => "amoadd",
            Self::BitXor => "amoxor",
            Self::BitAnd => "amoand",
            Self::BitOr => "amoor",
            Self::Min => "amomin",
            Self::Minu => "amominu",
            Self::Max => "amomax",
            Self::Maxu => "amomaxu",
        };
        formatter.write_str(name)
    }
}

/// Typed input-length failure of a direct [`apply`] call.
///
/// The envelope-produced transforms are total and never return this error;
/// only the direct entry point validates its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmoArithmeticError {
    /// The old span bytes did not have exactly the selected width.
    OldSpanLength {
        /// Required byte count.
        expected: usize,
        /// Supplied byte count.
        actual: usize,
    },
    /// The operand bytes did not have exactly the selected width.
    OperandLength {
        /// Required byte count.
        expected: usize,
        /// Supplied byte count.
        actual: usize,
    },
}

/// Computes one AMO result from old span bytes and operand bytes.
///
/// This is the direct, length-checked entry point to the one Hart-owned
/// implementation.  The returned buffer's first `width.bytes()` bytes are the
/// new span content in guest memory order; trailing bytes are zero and are
/// ignored for narrower spans.
pub fn apply(
    operation: AmoOperation,
    width: AmoWidth,
    old_bytes: &[u8],
    operand_bytes: &[u8],
) -> Result<[u8; MAX_PHYSICAL_ACCESS_BYTES], AmoArithmeticError> {
    let expected = width.bytes();
    if old_bytes.len() != expected {
        return Err(AmoArithmeticError::OldSpanLength {
            expected,
            actual: old_bytes.len(),
        });
    }
    if operand_bytes.len() != expected {
        return Err(AmoArithmeticError::OperandLength {
            expected,
            actual: operand_bytes.len(),
        });
    }
    Ok(compute(operation, width, old_bytes, operand_bytes))
}

/// Produces the envelope-carried pure transform for one operation and width.
///
/// This is the only way to obtain an [`AmoTransform`]: the arithmetic exists
/// exactly once, in this module, identically for every backend (dev-plan
/// §5.2 M1).
pub const fn transform(operation: AmoOperation, width: AmoWidth) -> AmoTransform {
    let apply_fn: PureAmoTransform = match (operation, width) {
        (AmoOperation::Swap, AmoWidth::Word) => apply_swap_word,
        (AmoOperation::Swap, AmoWidth::Doubleword) => apply_swap_doubleword,
        (AmoOperation::Add, AmoWidth::Word) => apply_add_word,
        (AmoOperation::Add, AmoWidth::Doubleword) => apply_add_doubleword,
        (AmoOperation::BitXor, AmoWidth::Word) => apply_bitxor_word,
        (AmoOperation::BitXor, AmoWidth::Doubleword) => apply_bitxor_doubleword,
        (AmoOperation::BitAnd, AmoWidth::Word) => apply_bitand_word,
        (AmoOperation::BitAnd, AmoWidth::Doubleword) => apply_bitand_doubleword,
        (AmoOperation::BitOr, AmoWidth::Word) => apply_bitor_word,
        (AmoOperation::BitOr, AmoWidth::Doubleword) => apply_bitor_doubleword,
        (AmoOperation::Min, AmoWidth::Word) => apply_min_word,
        (AmoOperation::Min, AmoWidth::Doubleword) => apply_min_doubleword,
        (AmoOperation::Minu, AmoWidth::Word) => apply_minu_word,
        (AmoOperation::Minu, AmoWidth::Doubleword) => apply_minu_doubleword,
        (AmoOperation::Max, AmoWidth::Word) => apply_max_word,
        (AmoOperation::Max, AmoWidth::Doubleword) => apply_max_doubleword,
        (AmoOperation::Maxu, AmoWidth::Word) => apply_maxu_word,
        (AmoOperation::Maxu, AmoWidth::Doubleword) => apply_maxu_doubleword,
    };
    AmoTransform::new(width, apply_fn)
}

/// Defines one total envelope-transform entry function.
///
/// The envelope carries plain function pointers, so each (operation, width)
/// pair has one named entry that forwards to the shared [`compute`] core.
macro_rules! amo_transform_entry {
    ($name:ident, $operation:expr, $width:expr) => {
        fn $name(old_bytes: &[u8], operand_bytes: &[u8]) -> [u8; MAX_PHYSICAL_ACCESS_BYTES] {
            compute($operation, $width, old_bytes, operand_bytes)
        }
    };
}

amo_transform_entry!(apply_swap_word, AmoOperation::Swap, AmoWidth::Word);
amo_transform_entry!(
    apply_swap_doubleword,
    AmoOperation::Swap,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_add_word, AmoOperation::Add, AmoWidth::Word);
amo_transform_entry!(
    apply_add_doubleword,
    AmoOperation::Add,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_bitxor_word, AmoOperation::BitXor, AmoWidth::Word);
amo_transform_entry!(
    apply_bitxor_doubleword,
    AmoOperation::BitXor,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_bitand_word, AmoOperation::BitAnd, AmoWidth::Word);
amo_transform_entry!(
    apply_bitand_doubleword,
    AmoOperation::BitAnd,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_bitor_word, AmoOperation::BitOr, AmoWidth::Word);
amo_transform_entry!(
    apply_bitor_doubleword,
    AmoOperation::BitOr,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_min_word, AmoOperation::Min, AmoWidth::Word);
amo_transform_entry!(
    apply_min_doubleword,
    AmoOperation::Min,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_minu_word, AmoOperation::Minu, AmoWidth::Word);
amo_transform_entry!(
    apply_minu_doubleword,
    AmoOperation::Minu,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_max_word, AmoOperation::Max, AmoWidth::Word);
amo_transform_entry!(
    apply_max_doubleword,
    AmoOperation::Max,
    AmoWidth::Doubleword
);
amo_transform_entry!(apply_maxu_word, AmoOperation::Maxu, AmoWidth::Word);
amo_transform_entry!(
    apply_maxu_doubleword,
    AmoOperation::Maxu,
    AmoWidth::Doubleword
);

/// The shared arithmetic core: total over all inputs.
///
/// Validated envelopes always supply exactly `width.bytes()` input bytes, so
/// the defensive zero-filled return for shorter inputs is unreachable through
/// the envelope path; a backend critical section must never panic.
fn compute(
    operation: AmoOperation,
    width: AmoWidth,
    old_bytes: &[u8],
    operand_bytes: &[u8],
) -> [u8; MAX_PHYSICAL_ACCESS_BYTES] {
    let mut out = [0u8; MAX_PHYSICAL_ACCESS_BYTES];
    let expected = width.bytes();
    if old_bytes.len() < expected || operand_bytes.len() < expected {
        return out;
    }
    match width {
        AmoWidth::Word => {
            let old = u32::from_le_bytes(old_bytes[..4].try_into().expect("length checked above"));
            let operand =
                u32::from_le_bytes(operand_bytes[..4].try_into().expect("length checked above"));
            let result: u32 = match operation {
                AmoOperation::Swap => operand,
                AmoOperation::Add => old.wrapping_add(operand),
                AmoOperation::BitAnd => old & operand,
                AmoOperation::BitOr => old | operand,
                AmoOperation::BitXor => old ^ operand,
                AmoOperation::Min => (old as i32).min(operand as i32) as u32,
                AmoOperation::Minu => old.min(operand),
                AmoOperation::Max => (old as i32).max(operand as i32) as u32,
                AmoOperation::Maxu => old.max(operand),
            };
            // W writes a zero-extended 32-bit result: the low four bytes only.
            out[..4].copy_from_slice(&result.to_le_bytes());
        }
        AmoWidth::Doubleword => {
            let old = u64::from_le_bytes(old_bytes[..8].try_into().expect("length checked above"));
            let operand =
                u64::from_le_bytes(operand_bytes[..8].try_into().expect("length checked above"));
            let result: u64 = match operation {
                AmoOperation::Swap => operand,
                AmoOperation::Add => old.wrapping_add(operand),
                AmoOperation::BitAnd => old & operand,
                AmoOperation::BitOr => old | operand,
                AmoOperation::BitXor => old ^ operand,
                AmoOperation::Min => (old as i64).min(operand as i64) as u64,
                AmoOperation::Minu => old.min(operand),
                AmoOperation::Max => (old as i64).max(operand as i64) as u64,
                AmoOperation::Maxu => old.max(operand),
            };
            out.copy_from_slice(&result.to_le_bytes());
        }
    }
    out
}
