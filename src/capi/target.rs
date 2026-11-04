//! The C callback table as one blocking RAM target (dev-plan D8).
//!
//! The table is copied once at create time; `user` must stay valid until
//! destroy.  Every callback runs synchronously inside a ruscv-sim call on the
//! creating thread, so a callback that calls back into the C ABI is rejected as
//! re-entry before it can observe or mutate any handle state.
//!
//! The mapping is deliberately narrow and total.  A C target can only report a
//! [`BlockingResponseStatus`], an atomic completion, an outcome-unknown flag and
//! a delay; anything else it does to the payload struct reveals a contract
//! violation (an unrecognized status, a consumed-but-invalid snapshot, a
//! mutated address/command/length) and is mapped to a non-completion, so the
//! blocking adapter keeps ownership of every D4 classification.  No C value can make ruscv-sim report a guest fault or a
//! completion that the target did not produce.

use crate::blocking::{
    BlockingAtomicOutcome, BlockingCommand, BlockingDebug, BlockingResponseStatus, BlockingTarget,
    BlockingTransaction, Femtoseconds,
};
use crate::physical::{
    CommittedWriteSnapshot, ConditionalStatus, PhysicalAccessKind,
    MAX_COMMITTED_WRITE_SNAPSHOT_BYTES,
};
use std::ffi::c_void;
use std::sync::Arc;

/// Capacity of one C snapshot buffer, in bytes.
///
/// It matches [`MAX_COMMITTED_WRITE_SNAPSHOT_BYTES`]: a committed-write snapshot
/// is opaque to the transport but never longer than the native reservation
/// window, so one fixed-size field serves both directions.
pub(crate) const SNAPSHOT_CAPACITY: usize = MAX_COMMITTED_WRITE_SNAPSHOT_BYTES;

/// C mirror of [`BlockingResponseStatus`] (`ruscv_tlm_status`).
pub(crate) mod status {
    pub(crate) const INCOMPLETE: i32 = 0;
    pub(crate) const OK: i32 = 1;
    pub(crate) const ADDRESS_ERROR: i32 = 2;
    pub(crate) const COMMAND_ERROR: i32 = 3;
    pub(crate) const BURST_ERROR: i32 = 4;
    pub(crate) const BYTE_ENABLE_ERROR: i32 = 5;
    pub(crate) const GENERIC_ERROR: i32 = 6;
}

/// C command (`ruscv_command`).
pub(crate) mod command {
    pub(crate) const READ: i32 = 0;
    pub(crate) const WRITE: i32 = 1;
}

/// C access kind (`ruscv_access_kind`).
pub(crate) mod access_kind {
    pub(crate) const FETCH: i32 = 0;
    pub(crate) const DATA_READ: i32 = 1;
    pub(crate) const DATA_WRITE: i32 = 2;
}

/// C initiator (`ruscv_initiator`).
pub(crate) mod initiator {
    pub(crate) const HART: i32 = 0;
    pub(crate) const DEBUG_INBOUND: i32 = 1;
}

/// C atomic kind (`ruscv_atomic_kind`).
pub(crate) mod atomic_kind {
    pub(crate) const RMW: i32 = 0;
    pub(crate) const LOAD_RESERVED: i32 = 1;
    pub(crate) const STORE_CONDITIONAL: i32 = 2;
}

/// C atomic outcome (`ruscv_atomic_outcome`).
pub(crate) mod atomic_outcome {
    pub(crate) const NONE: i32 = 0;
    pub(crate) const RMW: i32 = 1;
    pub(crate) const LOAD_RESERVED: i32 = 2;
    pub(crate) const STORE_CONDITIONAL_SUCCESS: i32 = 3;
    pub(crate) const STORE_CONDITIONAL_FAILURE: i32 = 4;
}

/// One atomic payload extension as the C target sees it (`ruscv_atomic`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CAtomic {
    /// Nonzero when this transaction carries the atomic extension.
    pub(crate) present: i32,
    /// Request kind: [`atomic_kind`].
    pub(crate) kind: i32,
    /// AMO operation code for `RMW` ([`AmoKind::code`]).
    pub(crate) operation: i32,
    /// Transfer width in bytes: 4 or 8.
    pub(crate) width: i32,
    /// Acquire ordering bit.
    pub(crate) acquire: i32,
    /// Release ordering bit.
    pub(crate) release: i32,
    /// Nonzero when a store-conditional carries its reservation context.
    pub(crate) reserved: i32,
    /// Start address of the reserved span.
    pub(crate) reserved_address: u64,
    /// Length of the reserved span in bytes.
    pub(crate) reserved_length: u32,
    /// Length of the echoed committed-write snapshot.
    pub(crate) reserved_snapshot_length: u32,
    /// Echoed committed-write snapshot bytes.
    pub(crate) reserved_snapshot: [u8; SNAPSHOT_CAPACITY],
    /// Response kind: [`atomic_outcome`].
    pub(crate) outcome: i32,
    /// Length of the load-reserved committed-write snapshot.
    pub(crate) snapshot_length: u32,
    /// Load-reserved committed-write snapshot bytes.
    pub(crate) snapshot: [u8; SNAPSHOT_CAPACITY],
}

impl Default for CAtomic {
    fn default() -> Self {
        Self {
            present: 0,
            kind: atomic_kind::RMW,
            operation: 0,
            width: 0,
            acquire: 0,
            release: 0,
            reserved: 0,
            reserved_address: 0,
            reserved_length: 0,
            reserved_snapshot_length: 0,
            reserved_snapshot: [0; SNAPSHOT_CAPACITY],
            outcome: atomic_outcome::NONE,
            snapshot_length: 0,
            snapshot: [0; SNAPSHOT_CAPACITY],
        }
    }
}

/// One blocking transaction as the C target sees it (`ruscv_payload`).
///
/// `data` points at a Rust-owned buffer whose length is exactly `length`; it is
/// valid only for the duration of the `b_transport` call.  A read target writes
/// the response bytes there, a write target reads the request bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CPayload {
    /// Explicit physical start address.
    pub(crate) address: u64,
    /// Request direction: [`command`].
    pub(crate) command: i32,
    /// Existing physical access category: [`access_kind`].
    pub(crate) access_kind: i32,
    /// Request initiator: [`initiator`].
    pub(crate) initiator: i32,
    /// Rust-owned transfer bytes.
    pub(crate) data: *mut u8,
    /// Transfer length in bytes, equal to the access width.
    pub(crate) length: usize,
    /// Response status: starts [`status::INCOMPLETE`], the target sets it.
    pub(crate) status: i32,
    /// Nonzero when the target cannot vouch for the outcome.
    pub(crate) outcome_unknown: u32,
    /// Annotated response delay in exact femtoseconds.
    pub(crate) delay_fs: u64,
    /// Atomic payload extension.
    pub(crate) atomic: CAtomic,
}

/// One debug-transport request as the C target sees it (`ruscv_debug`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CDebug {
    /// Explicit physical start address.
    pub(crate) address: u64,
    /// Request direction: [`command`].
    pub(crate) command: i32,
    /// Rust-owned transfer bytes.
    pub(crate) data: *mut u8,
    /// Transfer length in bytes.
    pub(crate) length: usize,
}

/// One C callback table (`ruscv_target`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CTarget {
    /// Declared per-transaction maximum delay in exact femtoseconds.
    pub(crate) max_delay_fs: u64,
    /// Opaque host pointer passed to every callback.
    pub(crate) user: *mut c_void,
    /// Blocking transport service.
    pub(crate) b_transport: Option<unsafe extern "C" fn(*mut c_void, *mut CPayload)>,
    /// Debug transport service; returns the bytes transferred.
    pub(crate) transport_dbg: Option<unsafe extern "C" fn(*mut c_void, *mut CDebug) -> usize>,
    /// Transport reset; returns nonzero when acknowledged.
    pub(crate) transport_reset: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

/// A validated C callback table used as one blocking RAM target.
pub(crate) struct CBlockingTarget {
    table: CTarget,
    b_transport: unsafe extern "C" fn(*mut c_void, *mut CPayload),
    transport_dbg: unsafe extern "C" fn(*mut c_void, *mut CDebug) -> usize,
    transport_reset: unsafe extern "C" fn(*mut c_void) -> i32,
}

// SAFETY: the C ABI accepts calls only on the creating thread, and the Machine
// runs callbacks synchronously and never re-enters them.  The target therefore
// never observes concurrent or interleaved use, and the host keeps the table,
// its `user` pointer and the pointed-to state alive until destroy (dev-plan D8).
unsafe impl Send for CBlockingTarget {}
// SAFETY: see the `Send` impl; the shared `Arc` is only ever driven from the
// creating thread inside one ruscv-sim call.
unsafe impl Sync for CBlockingTarget {}

impl CBlockingTarget {
    /// Validates and copies one C callback table.
    ///
    /// All three callbacks are required: image installation and every host view
    /// reach RAM through the debug transport, and a transport reset
    /// acknowledgement is the only evidence that resolves an unknown
    /// completion (dev-plan D6).  A table without them cannot serve a Machine.
    pub(crate) fn new(table: CTarget) -> Result<Arc<Self>, &'static str> {
        let (Some(b_transport), Some(transport_dbg), Some(transport_reset)) = (
            table.b_transport,
            table.transport_dbg,
            table.transport_reset,
        ) else {
            return Err("the RAM target must provide b_transport, transport_dbg and reset");
        };
        Ok(Arc::new(Self {
            table,
            b_transport,
            transport_dbg,
            transport_reset,
        }))
    }

    /// The declared per-transaction maximum delay.
    pub(crate) fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::new(self.table.max_delay_fs)
    }
}

impl BlockingTarget for CBlockingTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        CBlockingTarget::declared_max_delay(self)
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        let mut payload = CPayload {
            address: transaction.address(),
            command: match transaction.command() {
                BlockingCommand::Read => command::READ,
                BlockingCommand::Write => command::WRITE,
            },
            access_kind: access_kind_code(transaction.extension().access_kind()),
            initiator: match transaction.extension().initiator() {
                crate::blocking::BlockingInitiator::Hart => initiator::HART,
                crate::blocking::BlockingInitiator::DebugInbound => initiator::DEBUG_INBOUND,
            },
            data: transaction.data_mut().as_mut_ptr(),
            length: transaction.data_length(),
            status: status::INCOMPLETE,
            outcome_unknown: 0,
            delay_fs: 0,
            atomic: atomic_request(transaction),
        };
        let _in_callback = super::CallbackScope::enter();
        // SAFETY: the table was validated at create time, the payload and its
        // data buffer stay alive and exclusively borrowed for this call, and
        // the host keeps `user` valid until destroy.
        unsafe { (self.b_transport)(self.table.user, &mut payload) };

        // A target has no way to change the Rust payload through the C struct,
        // so a changed address, command or length in the copy is a contract
        // violation.  Mapping it to the untouched `Incomplete` status keeps the
        // adapter's D4 classification ("a payload the target changed is not a
        // completion") in charge instead of inventing a status here.
        let unchanged = payload.address == transaction.address()
            && payload.command
                == match transaction.command() {
                    BlockingCommand::Read => command::READ,
                    BlockingCommand::Write => command::WRITE,
                }
            && payload.length == transaction.data_length();
        transaction.set_status(if unchanged {
            response_status(payload.status)
        } else {
            BlockingResponseStatus::Incomplete
        });
        if payload.outcome_unknown != 0 {
            transaction.extension_mut().mark_outcome_unknown();
        }
        transaction.set_response_delay(Femtoseconds::new(payload.delay_fs));
        record_atomic_outcome(transaction, &payload.atomic);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        let mut debug = CDebug {
            address: transaction.address(),
            command: match transaction.command() {
                BlockingCommand::Read => command::READ,
                BlockingCommand::Write => command::WRITE,
            },
            data: transaction.data_mut().as_mut_ptr(),
            length: transaction.len(),
        };
        let _in_callback = super::CallbackScope::enter();
        // SAFETY: as in `b_transport`: validated callback, live buffer, and a
        // `user` pointer the host keeps valid until destroy.
        unsafe { (self.transport_dbg)(self.table.user, &mut debug) }
    }

    fn transport_reset(&self) -> bool {
        let _in_callback = super::CallbackScope::enter();
        // SAFETY: as in `b_transport`; the callback takes no Rust reference.
        unsafe { (self.transport_reset)(self.table.user) != 0 }
    }
}

/// C access-kind code for one physical access category.
fn access_kind_code(kind: PhysicalAccessKind) -> i32 {
    match kind {
        PhysicalAccessKind::Fetch => access_kind::FETCH,
        PhysicalAccessKind::DataRead => access_kind::DATA_READ,
        PhysicalAccessKind::DataWrite => access_kind::DATA_WRITE,
    }
}

/// The C status for one reported status code.
///
/// An unrecognized code is not a completion: the transaction keeps the status
/// it had, so the adapter reports the same protocol failure as a target that
/// never set one.
fn response_status(code: i32) -> BlockingResponseStatus {
    match code {
        status::INCOMPLETE => BlockingResponseStatus::Incomplete,
        status::OK => BlockingResponseStatus::Ok,
        status::ADDRESS_ERROR => BlockingResponseStatus::AddressError,
        status::COMMAND_ERROR => BlockingResponseStatus::CommandError,
        status::BURST_ERROR => BlockingResponseStatus::BurstError,
        status::BYTE_ENABLE_ERROR => BlockingResponseStatus::ByteEnableError,
        status::GENERIC_ERROR => BlockingResponseStatus::GenericError,
        _ => BlockingResponseStatus::Incomplete,
    }
}

/// The request half of the atomic extension, or an absent extension.
fn atomic_request(transaction: &BlockingTransaction<'_>) -> CAtomic {
    let Some(atomic) = transaction.atomic_extension() else {
        return CAtomic::default();
    };
    let mut out = CAtomic {
        present: 1,
        kind: match atomic.operation() {
            crate::blocking::BlockingAtomicOperation::Rmw(_) => atomic_kind::RMW,
            crate::blocking::BlockingAtomicOperation::LoadReserved => atomic_kind::LOAD_RESERVED,
            crate::blocking::BlockingAtomicOperation::StoreConditional => {
                atomic_kind::STORE_CONDITIONAL
            }
        },
        operation: match atomic.operation().rmw_operation() {
            Some(operation) => i32::from(operation.code()),
            None => -1,
        },
        width: atomic.width().bytes() as i32,
        acquire: i32::from(atomic.ordering().aq),
        release: i32::from(atomic.ordering().rl),
        ..CAtomic::default()
    };
    if let Some(reservation) = atomic.reservation() {
        let snapshot = reservation.snapshot.as_bytes();
        out.reserved = 1;
        out.reserved_address = reservation.reserved.paddr;
        out.reserved_length = reservation.reserved.width.bytes() as u32;
        out.reserved_snapshot_length = snapshot.len() as u32;
        out.reserved_snapshot[..snapshot.len()].copy_from_slice(snapshot);
    }
    out
}

/// Records the target's atomic completion, if it produced a usable one.
///
/// An unknown outcome code, a snapshot longer than the C buffer or a malformed
/// snapshot leaves the extension without an outcome.  The adapter then rejects
/// an `Ok` response that carries no atomic completion, which is the protocol
/// failure dev-plan D5 requires; no error is deduced from the C value.
fn record_atomic_outcome(transaction: &mut BlockingTransaction<'_>, atomic: &CAtomic) {
    let outcome = match atomic.outcome {
        atomic_outcome::RMW => Some(BlockingAtomicOutcome::Rmw),
        atomic_outcome::LOAD_RESERVED => {
            // A reported length the fixed buffer cannot hold is a malformed
            // completion: leave the outcome absent so the adapter classifies it
            // as a protocol failure instead of truncating the snapshot.
            if atomic.snapshot_length as usize > SNAPSHOT_CAPACITY {
                None
            } else {
                let snapshot = &atomic.snapshot[..atomic.snapshot_length as usize];
                CommittedWriteSnapshot::from_bytes(snapshot)
                    .ok()
                    .map(|snapshot| BlockingAtomicOutcome::LoadReserved { snapshot })
            }
        }
        atomic_outcome::STORE_CONDITIONAL_SUCCESS => Some(BlockingAtomicOutcome::StoreConditional(
            ConditionalStatus::Success,
        )),
        atomic_outcome::STORE_CONDITIONAL_FAILURE => Some(BlockingAtomicOutcome::StoreConditional(
            ConditionalStatus::Failure,
        )),
        _ => None,
    };
    let (Some(outcome), Some(extension)) = (outcome, transaction.atomic_extension_mut()) else {
        return;
    };
    extension.set_outcome(outcome);
}
