//! The blocking RAM-target interface (dev-plan D3).
//!
//! Implementations model one IEEE 1666 blocking transport endpoint.  The
//! adapter in [`super::adapter`] owns every ruscv-sim rule: one physical
//! request becomes exactly one [`BlockingTransaction`], the request is never
//! split, merged, cached or retried, and every status is reclassified into the
//! ADR-0002 §4 categories.  A target therefore carries no TLM, ISA or trap
//! dependency and cannot affect a guest outcome by itself.

use super::{BlockingDebug, BlockingTransaction, Femtoseconds};

/// A blocking guest-RAM target (dev-plan D3/D4/D6).
///
/// All methods take `&self`: a [`crate::machine::MachineConfig`] holds the
/// target as a shared `Arc`, and a target's own synchronization must not be
/// observable through this interface.  Calls are synchronous and are never
/// re-entered by ruscv-sim.
pub trait BlockingTarget: Send + Sync {
    /// The declared per-transaction maximum delay (dev-plan D7).
    ///
    /// Unit U3 uses it to form each turn's conservative completion bound.  It
    /// is an upper bound on the delay a target annotates for one transaction;
    /// the in-tree and SystemC RAM targets declare zero.
    fn declared_max_delay(&self) -> Femtoseconds;

    /// Services one complete blocking transaction.
    ///
    /// The target must leave the payload's address, command and length
    /// unchanged, select exactly one [`super::BlockingResponseStatus`] (a
    /// target that forgets leaves `Incomplete`, which is a protocol failure),
    /// and write the response bytes for a read into
    /// [`BlockingTransaction::data_mut`].  It must apply a write as one
    /// indivisible semantic write.  Annotating a non-zero delay is an adapter
    /// failure until unit U3.
    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>);

    /// Services one debug-transport request and returns the number of bytes
    /// transferred.
    ///
    /// Debug transport has no modeled device side effects beyond the addressed
    /// bytes and is not counted as a transaction (dev-plan D6).  Returning
    /// fewer bytes than requested is a failed or short debug transport.
    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize;

    /// Acknowledges a transport reset.
    ///
    /// The Platform calls this before zero-filling and reinstalling the RAM
    /// window after an unknown completion or a fresh reset.  `true` means the
    /// target has quiesced and accepts new accesses; `false` leaves the
    /// transport unusable (dev-plan D6).
    fn transport_reset(&self) -> bool;
}
