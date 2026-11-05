//! In-tree zero-delay Rust RAM target (dev-plan D2/D5).
//!
//! `run --transport tlm` and the public entry point that runs a file under a
//! [`crate::machine::MachineConfig`] use this target so the selectable RAM
//! route has one concrete implementation without a C++ or SystemC dependency.
//! It annotates zero delay, so a TLM run is time-equivalent to native until
//! unit U3 introduces a separate delaying target.
//!
//! Storage is a sparse map of 4 KiB chunks keyed by physical address.  The
//! Platform bounds every transaction to the RAM window, so chunk allocation is
//! proportional to the window the guest actually uses, not to the address
//! value.  Chunk bytes are atomics, because the platform may read and write
//! them through a direct-memory grant while another thread's transaction
//! reaches them through `b_transport` or debug transport (direct-memory
//! D10/D13).  A chunk is granted read-write, because a store the adapter may
//! serve through the pointer must land in the same bytes the blocking route
//! serves.  Alongside the bytes the target keeps exact per-byte committed-write
//! bookkeeping: a load-reserved returns its snapshot and a store-conditional
//! re-checks it inside the same lock hold, matching the native RAM target.
//!
//! Atomics arrive in one `b_transport` transaction carrying the ruscv-sim
//! atomic extension.  The target applies the encoded operation under this
//! file's operation table ([`hart_amods::apply`]) without seeing or calling the
//! Hart's transform function pointer; read-then-write emulation never occurs
//! here or in the adapter.

use super::{
    BlockingAtomicExtension, BlockingAtomicOperation, BlockingAtomicOutcome, BlockingCommand,
    BlockingDebug, BlockingDirectMemAccess, BlockingDirectMemGrant, BlockingDirectMemInvalidator,
    BlockingDirectMemQuery, BlockingDirectMemRegistry, BlockingDirectMemResponse,
    BlockingResponseStatus, BlockingTarget, BlockingTransaction, Femtoseconds,
};
use crate::hart_amods::{self, AmoOperation};
use crate::physical::{CommittedWriteSnapshot, ConditionalStatus};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// Chunk size used to index physical addresses.
const CHUNK_BITS: u32 = 12;
/// Bytes per storage chunk.
const CHUNK_SIZE: usize = 1 << CHUNK_BITS;
/// Offset mask within a chunk.
const CHUNK_MASK: u64 = (CHUNK_SIZE as u64) - 1;

type Chunks = BTreeMap<u64, Box<[AtomicU8]>>;

/// Committed-write version recorded for bytes no write has covered yet.
const NEVER_COMMITTED_VERSION: u64 = 0;

/// Exact per-byte committed-write bookkeeping keyed by physical address.
///
/// The version clock and interval rules match the native RAM target: every
/// committed write records a fresh version for each covered byte, a
/// load-reserved captures the exact snapshot, and a store-conditional fails
/// iff a committed write overlapped the reserved span.
#[derive(Debug, Default)]
struct CommitBookkeeping {
    /// Disjoint sorted byte intervals `start -> (end_exclusive, version)`.
    versions: BTreeMap<u64, (u64, u64)>,
    /// Monotonic committed-write clock; zero is reserved for never-written
    /// bytes, so the first commit issues version 1.
    next_version: u64,
}

impl CommitBookkeeping {
    fn byte_version(&self, address: u64) -> u64 {
        if let Some((_, &(end, version))) = self.versions.range(..=address).next_back() {
            if address < end {
                return version;
            }
        }
        NEVER_COMMITTED_VERSION
    }

    fn commit(&mut self, address: u64, len: usize) {
        if len == 0 {
            return;
        }
        let Some(end) = address.checked_add(len as u64) else {
            return;
        };
        self.next_version += 1;
        let version = self.next_version;

        // Split the interval containing the span start, keeping only the
        // prefix before it and the suffix after the exclusive end.
        if let Some((&start, &(existing_end, existing_version))) =
            self.versions.range(..=address).next_back()
        {
            if start < address && address < existing_end {
                self.versions.insert(start, (address, existing_version));
                if existing_end > end {
                    self.versions.insert(end, (existing_end, existing_version));
                }
            }
        }
        // Split the interval containing the exclusive span end so no record
        // crosses the new span's boundary.
        if let Some((&start, &(existing_end, existing_version))) =
            self.versions.range(..=end).next_back()
        {
            if start < end && end < existing_end {
                self.versions.insert(start, (end, existing_version));
                self.versions.insert(end, (existing_end, existing_version));
            }
        }
        self.versions
            .retain(|&start, &mut (existing_end, _)| !(start >= address && existing_end <= end));
        self.versions.insert(address, (end, version));
    }

    fn snapshot(&self, address: u64, len: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(len * 8);
        for i in 0..len {
            bytes.extend_from_slice(&self.byte_version(address + i as u64).to_le_bytes());
        }
        bytes
    }

    /// Re-checks a snapshot against the exact span.
    ///
    /// `Ok(true)` means a committed write overlapped the span. `Err(())` means
    /// the snapshot does not have this target's format; that is a simulator
    /// protocol failure, never an ordinary conditional failure.
    fn span_overwritten(&self, address: u64, len: usize, snapshot: &[u8]) -> Result<bool, ()> {
        if snapshot.is_empty() || snapshot.len() != len * 8 {
            return Err(());
        }
        for i in 0..len {
            let recorded = u64::from_le_bytes(
                snapshot[i * 8..i * 8 + 8]
                    .try_into()
                    .expect("snapshot is a multiple of eight bytes"),
            );
            if recorded != self.byte_version(address + i as u64) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Forgets every recorded version while keeping the clock monotonic.
    ///
    /// A transport reset discards the bytes, so their records go too, but a
    /// version must never repeat: a retained reservation's snapshot must not
    /// match a byte that a later run rewrote (direct-memory D10/D13).
    fn reset_preserving_clock(&mut self) {
        self.versions.clear();
    }
}

/// Bytes and committed-write bookkeeping under one lock.
#[derive(Default)]
struct Storage {
    chunks: Chunks,
    commits: CommitBookkeeping,
}

/// An in-tree blocking RAM target that annotates zero delay.
#[derive(Default)]
pub struct ZeroDelayRam {
    storage: Mutex<Storage>,
    /// Every adapter registration, kept without replacement so one reset
    /// reaches the composition that is still current as well
    /// (direct-memory D8).
    direct: BlockingDirectMemRegistry,
}

impl ZeroDelayRam {
    /// Creates an empty zero-delay RAM target.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads bytes from the target's storage without issuing a transport
    /// transaction.
    ///
    /// This is a test and diagnostic inspection helper, not a simulator access
    /// path: byte ranges no write has covered read as zero.
    pub fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        match self.storage.lock() {
            Ok(storage) => read(&storage.chunks, address, len),
            Err(_) => vec![0; len],
        }
    }

    /// Number of allocated 4 KiB chunks, for diagnostics.
    pub fn allocated_chunks(&self) -> usize {
        match self.storage.lock() {
            Ok(storage) => storage.chunks.len(),
            Err(_) => 0,
        }
    }
}

impl fmt::Debug for ZeroDelayRam {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ZeroDelayRam")
            .field("allocated_chunks", &self.allocated_chunks())
            .finish()
    }
}

impl BlockingTarget for ZeroDelayRam {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::ZERO
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        let mut storage = match self.storage.lock() {
            Ok(storage) => storage,
            Err(_) => {
                transaction.extension_mut().mark_outcome_unknown();
                return;
            }
        };
        if let Some(atomic) = transaction.atomic_extension() {
            apply_atomic(&mut storage, transaction, atomic);
            return;
        }
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = read(
                    &storage.chunks,
                    transaction.address(),
                    transaction.data().len(),
                );
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                write(&mut storage, transaction.address(), transaction.data());
            }
        }
        // The hint is the only licence for the adapter's query, so it goes on
        // a completed non-atomic transaction only (direct-memory D3/D13).
        transaction.set_dmi_allowed(true);
        transaction.set_status(BlockingResponseStatus::Ok);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        let mut storage = match self.storage.lock() {
            Ok(storage) => storage,
            Err(_) => return 0,
        };
        let len = transaction.data().len();
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = read(&storage.chunks, transaction.address(), len);
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                write(&mut storage, transaction.address(), transaction.data());
            }
        }
        len
    }

    fn transport_reset(&self) -> bool {
        match self.storage.lock() {
            Ok(mut storage) => {
                // Every registration is invalidated before any chunk is
                // discarded, and both happen under the storage lock, so no
                // grant is issued across the window and no cached pointer
                // outlives its storage (direct-memory D9/D13).
                self.direct.invalidate_all();
                storage.chunks.clear();
                storage.commits.reset_preserving_clock();
                true
            }
            Err(_) => false,
        }
    }

    fn get_direct_mem_ptr(&self, query: BlockingDirectMemQuery) -> BlockingDirectMemResponse {
        let storage = match self.storage.lock() {
            Ok(storage) => storage,
            Err(_) => return BlockingDirectMemResponse::Failed,
        };
        let Some((start, end, pointer)) = chunk_span(&storage.chunks, query.address()) else {
            return BlockingDirectMemResponse::Denied;
        };
        // SAFETY: the granted chunk is this target's only copy of those bytes,
        // `b_transport` and debug transport serve the same atomics, and the
        // chunk stays where it is until `transport_reset`, which invalidates
        // every registration before it discards it (direct-memory D10/D13).
        // A read-write grant can serve a store, which lands in the same
        // atomics a later `b_transport` or debug transport reads, so the
        // chunk's committed-write bookkeeping is untouched by a direct store
        // exactly as the adapter's write-suspension gate requires.
        let grant = unsafe {
            BlockingDirectMemGrant::new(
                start,
                end,
                pointer,
                BlockingDirectMemAccess::ReadWrite,
                Femtoseconds::ZERO,
                Femtoseconds::ZERO,
            )
        };
        BlockingDirectMemResponse::Granted(grant)
    }

    fn register_direct_mem_invalidator(&self, invalidator: BlockingDirectMemInvalidator) {
        self.direct.register(invalidator);
    }
}

/// Applies one encoded atomic operation inside the target's single lock hold.
fn apply_atomic(
    storage: &mut Storage,
    transaction: &mut BlockingTransaction<'_>,
    atomic: BlockingAtomicExtension,
) {
    let address = transaction.address();
    let width = atomic.width().bytes();
    match atomic.operation() {
        BlockingAtomicOperation::Rmw(kind) => {
            let old = read(&storage.chunks, address, width);
            let operation = AmoOperation::from_kind(kind);
            let new = match hart_amods::apply(operation, atomic.width(), &old, transaction.data()) {
                Ok(new) => new,
                Err(_) => {
                    transaction.set_status(BlockingResponseStatus::GenericError);
                    return;
                }
            };
            write(storage, address, &new[..width]);
            transaction.data_mut().copy_from_slice(&old);
            record_outcome(transaction, BlockingAtomicOutcome::Rmw);
        }
        BlockingAtomicOperation::LoadReserved => {
            let old = read(&storage.chunks, address, width);
            let snapshot =
                match CommittedWriteSnapshot::from_bytes(&storage.commits.snapshot(address, width))
                {
                    Ok(snapshot) => snapshot,
                    Err(_) => {
                        transaction.set_status(BlockingResponseStatus::GenericError);
                        return;
                    }
                };
            transaction.data_mut().copy_from_slice(&old);
            record_outcome(
                transaction,
                BlockingAtomicOutcome::LoadReserved { snapshot },
            );
        }
        BlockingAtomicOperation::StoreConditional => {
            let committed = match atomic.reservation() {
                Some(context) if covered(address, width, context) => {
                    match storage.commits.span_overwritten(
                        context.reserved.paddr,
                        context.reserved.width.bytes(),
                        context.snapshot.as_bytes(),
                    ) {
                        Ok(true) => false,
                        Ok(false) => {
                            write(storage, address, transaction.data());
                            true
                        }
                        Err(()) => {
                            transaction.set_status(BlockingResponseStatus::GenericError);
                            return;
                        }
                    }
                }
                _ => false,
            };
            record_outcome(
                transaction,
                BlockingAtomicOutcome::StoreConditional(if committed {
                    ConditionalStatus::Success
                } else {
                    ConditionalStatus::Failure
                }),
            );
        }
    }
    transaction.set_status(BlockingResponseStatus::Ok);
}

fn covered(address: u64, width: usize, context: crate::physical::AtomicReservationContext) -> bool {
    let Some(requested_end) = address.checked_add(width as u64) else {
        return false;
    };
    let Some(reserved_end) = context
        .reserved
        .paddr
        .checked_add(context.reserved.width.bytes() as u64)
    else {
        return false;
    };
    context.reserved.paddr <= address && requested_end <= reserved_end
}

fn record_outcome(transaction: &mut BlockingTransaction<'_>, outcome: BlockingAtomicOutcome) {
    if let Some(extension) = transaction.atomic_extension_mut() {
        extension.set_outcome(outcome);
    }
}

fn read(storage: &Chunks, address: u64, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    let mut done = 0;
    while done < len {
        let current = address.wrapping_add(done as u64);
        let chunk = current >> CHUNK_BITS;
        let offset = (current & CHUNK_MASK) as usize;
        let count = (CHUNK_SIZE - offset).min(len - done);
        if let Some(data) = storage.get(&chunk) {
            for index in 0..count {
                bytes[done + index] = data[offset + index].load(Ordering::Relaxed);
            }
        }
        done += count;
    }
    bytes
}

fn write(storage: &mut Storage, address: u64, bytes: &[u8]) {
    let mut done = 0;
    while done < bytes.len() {
        let current = address.wrapping_add(done as u64);
        let chunk = current >> CHUNK_BITS;
        let offset = (current & CHUNK_MASK) as usize;
        let count = (CHUNK_SIZE - offset).min(bytes.len() - done);
        let data = storage.chunks.entry(chunk).or_insert_with(alloc_chunk);
        for index in 0..count {
            data[offset + index].store(bytes[done + index], Ordering::Relaxed);
        }
        done += count;
    }
    storage.commits.commit(address, bytes.len());
}

/// One zeroed 4 KiB chunk of byte atomics.
fn alloc_chunk() -> Box<[AtomicU8]> {
    (0..CHUNK_SIZE)
        .map(|_| AtomicU8::new(0))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

/// The granted span of the chunk that contains `address`, when it exists.
fn chunk_span(storage: &Chunks, address: u64) -> Option<(u64, u64, *mut u8)> {
    let chunk = address >> CHUNK_BITS;
    let bytes = storage.get(&chunk)?;
    let start = chunk << CHUNK_BITS;
    Some((start, start + CHUNK_MASK, bytes.as_ptr() as *mut u8))
}

/// The most grants one `ZeroDelayRam` keeps registered (direct-memory D8).
#[cfg(test)]
impl ZeroDelayRam {
    /// Chunk pointer of `address`, for tests that script a grant.
    pub(crate) fn chunk_pointer(&self, address: u64) -> Option<(u64, u64, *mut u8)> {
        let storage = self.storage.lock().ok()?;
        chunk_span(&storage.chunks, address)
    }
}
