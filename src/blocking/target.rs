//! The blocking RAM-target interface (dev-plan D3).
//!
//! Implementations model one IEEE 1666 blocking transport endpoint.  The
//! adapter in [`super::adapter`] owns every ruscv-sim rule: one physical
//! request becomes at most one [`BlockingTransaction`], the request is never
//! split or merged, every status is reclassified into the ADR-0002 §4
//! categories, and a granted host pointer may only replace a transaction the
//! target's own hint offers (direct-memory D3/D4).  A target therefore carries
//! no TLM, ISA or trap dependency and cannot affect a guest outcome by itself.

use super::{
    BlockingDebug, BlockingDirectMemInvalidator, BlockingDirectMemQuery, BlockingDirectMemResponse,
    BlockingTransaction, Femtoseconds,
};

/// A blocking guest-RAM target (dev-plan D3/D4/D6).
///
/// All methods take `&self`: a [`crate::machine::MachineConfig`] holds the
/// target as a shared `Arc`, and a target's own synchronization must not be
/// observable through this interface.  Calls are synchronous, return before
/// the call that made them returns, and are never re-entered by ruscv-sim.  A
/// target that declares [`Self::suspends_host_time`] may let host time pass
/// inside `b_transport` for a request whose `may_suspend` is true, provided the
/// callback resumes and returns on the thread that made the call (nb D2/D6);
/// every other callback still must not wait or advance host time.
pub trait BlockingTarget: Send + Sync {
    /// Whether this target lets host time pass inside `b_transport` (nb D2).
    ///
    /// The default is `false`, which keeps every existing implementor exactly
    /// as it is.  A `true` target may suspend the calling host thread inside a
    /// [`BlockingTransaction`] whose `may_suspend` is true and report that time
    /// through [`BlockingTransaction::set_host_elapsed`].  The adapter reads
    /// this once, at construction.  A suspending target cannot be combined with
    /// a non-zero quantum: host time passing inside a grant would stop the
    /// kernel's clock from being the quantum anchor for the whole grant, so the
    /// build refuses that combination (nb D2).
    fn suspends_host_time(&self) -> bool {
        false
    }

    /// The declared per-transaction maximum delay (dev-plan D7).
    ///
    /// Unit U3 uses it to form each turn's conservative completion bound and
    /// to reject an overshooting response.  It is an upper bound on the delay
    /// a target annotates for one transaction; the in-tree [`super::ZeroDelayRam`]
    /// declares zero, and a suspending target declares its host-time latency.
    /// delay D3: it also bounds a Hart device response's annotation, because a
    /// guest-RAM declaration is the route's only declared maximum and a device
    /// transaction is one of the turn's counted transactions.
    fn declared_max_delay(&self) -> Femtoseconds;

    /// Services one complete blocking transaction.
    ///
    /// The target must leave the payload's address, command and length
    /// unchanged, select exactly one [`super::BlockingResponseStatus`] (a
    /// target that forgets leaves `Incomplete`, which is a protocol failure),
    /// and write the response bytes for a read into
    /// [`BlockingTransaction::data_mut`].  It must apply a write as one
    /// indivisible semantic write.  An annotated delay is converted to
    /// Platform ticks under the declared tick period and consumed by the
    /// turn's outcome; a delay above the declared per-transaction maximum is
    /// an overshoot failure (ADR-0004 §5.2/§5.3).
    ///
    /// An atomic transaction carries [`super::BlockingAtomicExtension`].  A
    /// target that supports it applies the encoded operation indivisibly inside
    /// this one call and records the completion in the same extension.  A
    /// target that does not support atomics sets
    /// [`super::BlockingResponseStatus::CommandError`], which the adapter maps
    /// to the native load or store/AMO access fault; it must not emulate the
    /// operation with ordinary reads and writes.
    ///
    /// A target that declares [`Self::suspends_host_time`] reports the host
    /// time it let pass through [`BlockingTransaction::set_host_elapsed`].  A
    /// target that does not never sets it: a non-zero value on a request that
    /// may not suspend is a protocol failure of the access (nb D4).
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
    ///
    /// A target that grants direct memory invalidates every registration it
    /// keeps before it discards, moves or remaps granted storage, atomically
    /// with respect to new grants, and keeps its committed-write version clock
    /// across the reset (direct-memory D10/D13).
    fn transport_reset(&self) -> bool;

    /// Answers one direct-memory query (direct-memory D3).
    ///
    /// The adapter asks once per completed Hart-initiated non-atomic
    /// transaction that carried the `dmi_allowed` hint.  A target that offers
    /// direct memory returns a [`super::BlockingDirectMemGrant`] whose pointer
    /// and range satisfy the obligations of
    /// [`super::BlockingDirectMemGrant::new`] until it invalidates them, and
    /// whose read and write latencies are each at most
    /// [`Self::declared_max_delay`].  Returning
    /// [`BlockingDirectMemResponse::Denied`] caches nothing and is not an
    /// error; [`BlockingDirectMemResponse::Failed`] is a contained failure of
    /// the query itself and poisons the transport.
    ///
    /// The default offers nothing, so an existing target needs no change.
    fn get_direct_mem_ptr(&self, query: BlockingDirectMemQuery) -> BlockingDirectMemResponse {
        let _ = query;
        BlockingDirectMemResponse::Denied
    }

    /// Keeps one adapter's direct-memory invalidation handle (direct-memory
    /// D8).
    ///
    /// The adapter calls this once when it is constructed with direct memory
    /// selected, before its first transport reset.  A target must keep every
    /// registration and never replace an earlier one with a later one, because
    /// a reset must reach the composition that is still current as well as the
    /// one under construction.  [`super::BlockingDirectMemRegistry`] provides
    /// that bookkeeping.  The handle is inert once its adapter is gone.
    ///
    /// The default keeps nothing, which is enough because a target that grants
    /// no direct memory needs no invalidation.
    fn register_direct_mem_invalidator(&self, invalidator: BlockingDirectMemInvalidator) {
        let _ = invalidator;
    }
}
