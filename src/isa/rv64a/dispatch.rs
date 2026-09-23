//! Hart-side AMO/LR/SC dispatch (dev-plan §5.1, §5.3, §5.4).
//!
//! [`decode_amo`] is the single Hart-owned decode authority for the AMO
//! major opcode: `funct5` selects the operation against the approved §2
//! table, `funct3` selects W or D, `aq`/`rl` are informational ordering bits
//! accepted in all four combinations, and reserved encodings — every
//! unassigned `funct5` value and LR with `rs2 != 0` — are
//! illegal-instruction traps, not simulator failures.
//!
//! [`execute_amo_typed`] is the typed `MemoryInterface` route retained for
//! the old `RiscvCore::new` constructor: a labeled, non-conforming
//! compatibility adapter whose read/write pairs are not indivisible and
//! whose store-conditional cannot observe committed competing writes (no
//! bookkeeping reaches it).  [`execute_amo_port`] is the standard route:
//! each admitted instruction issues exactly one atomic envelope through the
//! validated data port, and Hart-side legality, misalignment (checked
//! earlier), and store-conditional preconditions issue zero physical
//! requests.
//!
//! Both routes share the one per-Hart reservation record in
//! [`CoreState::reservation`] (dev-plan §5.4): a successful LR replaces it,
//! an executed SC consumes it on success and on conditional failure, a
//! faulting LR establishes nothing and preserves it, and a faulting SC
//! retains it (approved retain, §11 item 2) because the Hart discards the
//! staged state on the trap.

use crate::core::{CoreState, PhysicalAtomicAdapter};
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::hart_amods::{self, AmoOperation};
use crate::memory::{MemoryError, MemoryInterface};
use crate::physical::{
    AmoWidth, AtomicOrdering, AtomicRequest, AtomicReservationContext, ConditionalStatus,
};

use super::amo::exec_amo_rmw;
use super::lr_sc::{exec_lr, exec_lr_w, exec_sc, exec_sc_w, ReservationSet};

/// The instruction family selected by `funct5` in the approved §2 table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AmoKind {
    /// One read-modify-write operation.
    Rmw(AmoOperation),
    /// Load-reserved (`funct5 = 00010`, `rs2` must be zero).
    LoadReserved,
    /// Store-conditional (`funct5 = 00011`).
    StoreConditional,
}

/// One fully decoded AMO-family instruction under the §2/§5.3 contract.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DecodedAmo {
    /// The selected instruction family and operation.
    pub kind: AmoKind,
    /// The W/D result and span width.
    pub width: AmoWidth,
    /// Informational `aq`/`rl` ordering bits (no extra ordering effect in
    /// this single-Hart in-order profile, dev-plan §5.3).
    pub ordering: AtomicOrdering,
    /// `rd` register index (`x0` suppresses only the register write).
    pub rd: usize,
    /// `rs1` register index (the effective address source).
    pub rs1: usize,
    /// `rs2` register index (operand/store source; `x0` for LR).
    pub rs2: usize,
}

/// Decodes one AMO-major-opcode instruction against the §2 `funct5` table.
///
/// Reserved `funct5` values, a `funct3` outside {010, 011}, LR with
/// `rs2 != 0`, and missing register fields all return
/// [`ExecuteError::IllegalInstruction`] — the Hart enters an
/// illegal-instruction trap before any access.
pub(crate) fn decode_amo(instr: &DecodedInstruction) -> Result<DecodedAmo, ExecuteError> {
    let funct5 = (instr.raw >> 27) & 0x1f;
    let ordering = AtomicOrdering {
        aq: (instr.raw >> 26) & 1 != 0,
        rl: (instr.raw >> 25) & 1 != 0,
    };
    let width = match (instr.raw >> 12) & 0x7 {
        0b010 => AmoWidth::Word,
        0b011 => AmoWidth::Doubleword,
        _ => return Err(ExecuteError::IllegalInstruction),
    };
    let kind = match funct5 {
        0b00000 => AmoKind::Rmw(AmoOperation::Add),
        0b00001 => AmoKind::Rmw(AmoOperation::Swap),
        0b00010 => {
            // LR is only defined with rs2 = x0; any other rs2 is a reserved
            // encoding (dev-plan §2), never an SC fallback.
            if instr.rs2 != Some(0) {
                return Err(ExecuteError::IllegalInstruction);
            }
            AmoKind::LoadReserved
        }
        0b00011 => AmoKind::StoreConditional,
        0b00100 => AmoKind::Rmw(AmoOperation::BitXor),
        0b01000 => AmoKind::Rmw(AmoOperation::BitOr),
        0b01100 => AmoKind::Rmw(AmoOperation::BitAnd),
        0b10000 => AmoKind::Rmw(AmoOperation::Min),
        0b10100 => AmoKind::Rmw(AmoOperation::Max),
        0b11000 => AmoKind::Rmw(AmoOperation::Minu),
        0b11100 => AmoKind::Rmw(AmoOperation::Maxu),
        // Every other funct5 — including the draft-mis-listed 10001/10101 —
        // is a reserved encoding: an illegal-instruction trap, not a
        // simulator failure (dev-plan §2, C6).
        _ => return Err(ExecuteError::IllegalInstruction),
    };
    Ok(DecodedAmo {
        kind,
        width,
        ordering,
        rd: instr.rd.ok_or(ExecuteError::IllegalInstruction)? as usize,
        rs1: instr.rs1.ok_or(ExecuteError::IllegalInstruction)? as usize,
        rs2: instr.rs2.ok_or(ExecuteError::IllegalInstruction)? as usize,
    })
}

/// Returns whether one AMO-major-opcode encoding is architecturally legal.
///
/// The Hart's legality pass uses this so a reserved encoding traps before
/// any alignment precheck or access; [`decode_amo`] applies the identical
/// rule at dispatch.
pub(crate) fn amo_encoding_is_legal(instr: &DecodedInstruction) -> bool {
    decode_amo(instr).is_ok()
}

/// The typed-route AMO/LR/SC dispatcher (labeled non-conforming adapter).
///
/// `mem` performs the conversion the route uses; the reservation record keys
/// the effective address these helpers issue (the typed adapter applies its
/// own base conversion, dev-plan §7.1/C24).
pub(crate) fn execute_amo_typed(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    match decode_amo(instr)? {
        DecodedAmo {
            kind: AmoKind::Rmw(operation),
            width,
            ..
        } => exec_amo_rmw(instr, state, mem, operation, width),
        DecodedAmo {
            kind: AmoKind::LoadReserved,
            width: AmoWidth::Word,
            ..
        } => exec_lr_w(instr, state, mem),
        DecodedAmo {
            kind: AmoKind::LoadReserved,
            width: AmoWidth::Doubleword,
            ..
        } => exec_lr(instr, state, mem),
        DecodedAmo {
            kind: AmoKind::StoreConditional,
            width: AmoWidth::Word,
            ..
        } => exec_sc_w(instr, state, mem),
        DecodedAmo {
            kind: AmoKind::StoreConditional,
            width: AmoWidth::Doubleword,
            ..
        } => exec_sc(instr, state, mem),
    }
}

/// Sign-extends (W) or keeps (D) the old span bytes of an envelope for `rd`.
fn old_value(width: AmoWidth, old: &[u8]) -> u64 {
    match width {
        AmoWidth::Word => {
            let value = u32::from_le_bytes(
                old[..4]
                    .try_into()
                    .expect("a W envelope returns four bytes"),
            );
            (value as i32) as i64 as u64
        }
        AmoWidth::Doubleword => u64::from_le_bytes(
            old[..8]
                .try_into()
                .expect("a D envelope returns eight bytes"),
        ),
    }
}

/// The envelope-route AMO/LR/SC dispatcher for port-configured cores.
///
/// Every admitted instruction issues exactly one atomic envelope through the
/// validated data port (dev-plan §5.2 M1, §7.1): an RMW carries the operand
/// bytes and the Hart-supplied transform from [`hart_amods`], an LR returns
/// the old bytes plus the committed-write snapshot, and an SC carries the
/// Hart's reservation context for the backend's critical-section re-check.
///
/// Hart-side rejections issue **no** physical request: reserved encodings
/// trap via [`decode_amo`], and a store-conditional whose span the recorded
/// reservation does not cover retires `rd = 1` without touching the port
/// (dev-plan C13).  Target rejections surface as memory errors the Hart maps
/// to load/store-AMO access faults (causes 5/7) with the original guest
/// address.
pub(crate) fn execute_amo_port(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    adapter: &mut PhysicalAtomicAdapter<'_>,
) -> Result<(), ExecuteError> {
    let decoded = decode_amo(instr)?;
    let ea = state.regs[decoded.rs1];
    let width = decoded.width;
    let span_width = width.physical_width();

    match decoded.kind {
        AmoKind::LoadReserved => {
            let paddr = adapter
                .issued_addr(ea, span_width)
                .map_err(ExecuteError::MemoryError)?;
            let request = AtomicRequest::load_reserved(paddr, span_width, decoded.ordering)
                .map_err(|error| {
                    ExecuteError::MemoryError(MemoryError::Protocol(error.to_string()))
                })?;
            let response = adapter
                .access_atomic(request)
                .map_err(ExecuteError::MemoryError)?;
            let old = response.old_bytes().ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "load-reserved completion carried no old bytes".into(),
                ))
            })?;
            let snapshot = response.snapshot().ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "load-reserved completion carried no committed-write snapshot".into(),
                ))
            })?;
            // A successful LR replaces any prior reservation; the record keys
            // the port-issued physical address, the exact byte span, and the
            // LR-time committed-write snapshot (dev-plan §5.4).
            state.reservation = Some(ReservationSet::new(paddr, span_width, Some(snapshot)));
            if decoded.rd != 0 {
                state.regs[decoded.rd] = old_value(width, old);
            }
            Ok(())
        }
        AmoKind::StoreConditional => {
            // C13's no-reservation case is resolved entirely in the Hart:
            // retire rd = 1 without address conversion or any physical
            // request.  If a reservation exists, preserve the established
            // conversion-before-coverage behavior: a guest address below the
            // flat image base or a rejected storage offset faults as cause 7
            // with the original guest mtval, and the staged discard retains
            // the live reservation (approved faulting-SC retain, C15).
            if state.reservation.is_none() {
                if decoded.rd != 0 {
                    state.regs[decoded.rd] = 1;
                }
                return Ok(());
            }
            let paddr = adapter
                .issued_addr(ea, span_width)
                .map_err(ExecuteError::MemoryError)?;
            let reservation = state.reservation.take().ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "a store-conditional lost its reservation before address validation".into(),
                ))
            })?;
            if !reservation.covers(paddr, span_width) {
                if decoded.rd != 0 {
                    state.regs[decoded.rd] = 1;
                }
                return Ok(());
            }
            let snapshot = reservation.snapshot.ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "the per-Hart reservation has no committed-write snapshot".into(),
                ))
            })?;
            let payload = state.regs[decoded.rs2].to_le_bytes();
            let request = AtomicRequest::store_conditional(
                paddr,
                span_width,
                decoded.ordering,
                &payload[..span_width.bytes()],
                AtomicReservationContext {
                    reserved: reservation.reserved,
                    snapshot,
                },
            )
            .map_err(|error| ExecuteError::MemoryError(MemoryError::Protocol(error.to_string())))?;
            let response = adapter
                .access_atomic(request)
                .map_err(ExecuteError::MemoryError)?;
            let status = response.conditional_status().ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "store-conditional completion carried no conditional status".into(),
                ))
            })?;
            if decoded.rd != 0 {
                state.regs[decoded.rd] = match status {
                    ConditionalStatus::Success => 0,
                    ConditionalStatus::Failure => 1,
                };
            }
            Ok(())
        }
        AmoKind::Rmw(operation) => {
            let paddr = adapter
                .issued_addr(ea, span_width)
                .map_err(ExecuteError::MemoryError)?;
            let operand = state.regs[decoded.rs2].to_le_bytes();
            let request = AtomicRequest::rmw(
                paddr,
                span_width,
                decoded.ordering,
                &operand[..span_width.bytes()],
                hart_amods::transform(operation, width),
            )
            .map_err(|error| ExecuteError::MemoryError(MemoryError::Protocol(error.to_string())))?;
            let response = adapter
                .access_atomic(request)
                .map_err(ExecuteError::MemoryError)?;
            let old = response.old_bytes().ok_or_else(|| {
                ExecuteError::MemoryError(MemoryError::Protocol(
                    "read-modify-write completion carried no old bytes".into(),
                ))
            })?;
            if decoded.rd != 0 {
                state.regs[decoded.rd] = old_value(width, old);
            }
            Ok(())
        }
    }
}
