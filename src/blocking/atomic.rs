//! The ruscv-sim atomic payload extension (dev-plan D5).
//!
//! LR, SC and every AMO cross the blocking transport as one transaction with
//! this extension attached.  The extension carries the operation, width and
//! ordering bits as data, plus the Hart reservation context for a
//! store-conditional.  The Hart-supplied [`crate::physical::AmoTransform`]
//! function pointer never crosses the transport: the target applies the
//! encoded operation under its own table.
//!
//! For an RMW the transaction data buffer carries the operand on the way in
//! and the exact old span bytes on the way out.  A load-reserved returns its
//! old bytes in that buffer and its committed-write snapshot in the response
//! outcome.  A store-conditional carries its write payload in the buffer and
//! returns [`ConditionalStatus`] in the outcome.

use crate::physical::{
    AmoKind, AmoWidth, AtomicAccessKind, AtomicOrdering, AtomicReservationContext,
    CommittedWriteSnapshot, ConditionalStatus,
};

/// The atomic operation encoded in one blocking transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingAtomicOperation {
    /// One indivisible read-modify-write AMO.
    Rmw(AmoKind),
    /// One load-reserved read.
    LoadReserved,
    /// One conditional store.
    StoreConditional,
}

impl BlockingAtomicOperation {
    /// Returns the corresponding physical atomic envelope kind.
    pub const fn kind(self) -> AtomicAccessKind {
        match self {
            Self::Rmw(_) => AtomicAccessKind::Rmw,
            Self::LoadReserved => AtomicAccessKind::LoadReserved,
            Self::StoreConditional => AtomicAccessKind::StoreConditional,
        }
    }

    /// Returns the AMO operation for an RMW extension, or `None` otherwise.
    pub const fn rmw_operation(self) -> Option<AmoKind> {
        match self {
            Self::Rmw(operation) => Some(operation),
            Self::LoadReserved | Self::StoreConditional => None,
        }
    }
}

/// The target's atomic completion, returned in the same transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingAtomicOutcome {
    /// The RMW wrote its transformed bytes; the data buffer holds the old
    /// bytes.
    Rmw,
    /// The load-reserved read completed; the data buffer holds the old bytes
    /// and this outcome carries the committed-write snapshot.
    LoadReserved {
        /// The target's committed-write snapshot of the reserved span.
        snapshot: CommittedWriteSnapshot,
    },
    /// The conditional store completed with this status.
    StoreConditional(ConditionalStatus),
}

impl BlockingAtomicOutcome {
    /// Returns the corresponding physical atomic envelope kind.
    pub const fn kind(self) -> AtomicAccessKind {
        match self {
            Self::Rmw => AtomicAccessKind::Rmw,
            Self::LoadReserved { .. } => AtomicAccessKind::LoadReserved,
            Self::StoreConditional(_) => AtomicAccessKind::StoreConditional,
        }
    }
}

/// One ruscv-sim atomic payload extension.
///
/// The request fields are immutable after construction.  A target records its
/// completion with [`Self::set_outcome`]; the adapter rejects a completion
/// whose kind does not match the request or that carries no outcome at all
/// (the target ignored the extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockingAtomicExtension {
    operation: BlockingAtomicOperation,
    width: AmoWidth,
    ordering: AtomicOrdering,
    reservation: Option<AtomicReservationContext>,
    outcome: Option<BlockingAtomicOutcome>,
}

impl BlockingAtomicExtension {
    /// Creates an RMW extension carrying the Hart-owned operation identity.
    pub const fn rmw(operation: AmoKind, width: AmoWidth, ordering: AtomicOrdering) -> Self {
        Self {
            operation: BlockingAtomicOperation::Rmw(operation),
            width,
            ordering,
            reservation: None,
            outcome: None,
        }
    }

    /// Creates a load-reserved extension.
    pub const fn load_reserved(width: AmoWidth, ordering: AtomicOrdering) -> Self {
        Self {
            operation: BlockingAtomicOperation::LoadReserved,
            width,
            ordering,
            reservation: None,
            outcome: None,
        }
    }

    /// Creates a store-conditional extension carrying the Hart reservation
    /// context as data (`None` means no reservation).
    pub const fn store_conditional(
        width: AmoWidth,
        ordering: AtomicOrdering,
        reservation: Option<AtomicReservationContext>,
    ) -> Self {
        Self {
            operation: BlockingAtomicOperation::StoreConditional,
            width,
            ordering,
            reservation,
            outcome: None,
        }
    }

    /// The encoded atomic operation.
    pub const fn operation(self) -> BlockingAtomicOperation {
        self.operation
    }

    /// The encoded transfer width (4 or 8 bytes).
    pub const fn width(self) -> AmoWidth {
        self.width
    }

    /// The informational `aq`/`rl` ordering bits.
    pub const fn ordering(self) -> AtomicOrdering {
        self.ordering
    }

    /// The Hart reservation context for a store-conditional, if any.
    pub const fn reservation(self) -> Option<AtomicReservationContext> {
        self.reservation
    }

    /// The target's completion, once it recorded one.
    pub const fn outcome(self) -> Option<BlockingAtomicOutcome> {
        self.outcome
    }

    /// Records the target's atomic completion.
    pub fn set_outcome(&mut self, outcome: BlockingAtomicOutcome) {
        self.outcome = Some(outcome);
    }
}
