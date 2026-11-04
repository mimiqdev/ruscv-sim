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
//! value.  Alongside the bytes the target keeps exact per-byte committed-write
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
    BlockingDebug, BlockingResponseStatus, BlockingTarget, BlockingTransaction, Femtoseconds,
};
use crate::hart_amods::{self, AmoOperation};
use crate::physical::{CommittedWriteSnapshot, ConditionalStatus};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// Chunk size used to index physical addresses.
const CHUNK_BITS: u32 = 12;
/// Bytes per storage chunk.
const CHUNK_SIZE: usize = 1 << CHUNK_BITS;
/// Offset mask within a chunk.
const CHUNK_MASK: u64 = (CHUNK_SIZE as u64) - 1;

type Chunks = BTreeMap<u64, Box<[u8; CHUNK_SIZE]>>;

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

    /// Returns true when the snapshot no longer describes the exact span.
    /// A malformed snapshot is conservatively treated as overwritten; the
    /// adapter rejects one before it can reach the target.
    fn span_overwritten(&self, address: u64, len: usize, snapshot: &[u8]) -> bool {
        if snapshot.is_empty() || snapshot.len() != len * 8 {
            return true;
        }
        for i in 0..len {
            let recorded = u64::from_le_bytes(
                snapshot[i * 8..i * 8 + 8]
                    .try_into()
                    .expect("snapshot is a multiple of eight bytes"),
            );
            if recorded != self.byte_version(address + i as u64) {
                return true;
            }
        }
        false
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
                write(
                    &mut storage,
                    transaction.address(),
                    transaction.data().to_vec(),
                );
            }
        }
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
                write(
                    &mut storage,
                    transaction.address(),
                    transaction.data().to_vec(),
                );
            }
        }
        len
    }

    fn transport_reset(&self) -> bool {
        match self.storage.lock() {
            Ok(mut storage) => {
                storage.chunks.clear();
                storage.commits = CommitBookkeeping::default();
                true
            }
            Err(_) => false,
        }
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
            write(storage, address, new[..width].to_vec());
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
                Some(context)
                    if covered(address, width, context)
                        && !storage.commits.span_overwritten(
                            context.reserved.paddr,
                            context.reserved.width.bytes(),
                            context.snapshot.as_bytes(),
                        ) =>
                {
                    write(storage, address, transaction.data().to_vec());
                    true
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
            bytes[done..done + count].copy_from_slice(&data[offset..offset + count]);
        }
        done += count;
    }
    bytes
}

fn write(storage: &mut Storage, address: u64, bytes: Vec<u8>) {
    let mut done = 0;
    while done < bytes.len() {
        let current = address.wrapping_add(done as u64);
        let chunk = current >> CHUNK_BITS;
        let offset = (current & CHUNK_MASK) as usize;
        let count = (CHUNK_SIZE - offset).min(bytes.len() - done);
        let data = storage
            .chunks
            .entry(chunk)
            .or_insert_with(|| Box::new([0u8; CHUNK_SIZE]));
        data[offset..offset + count].copy_from_slice(&bytes[done..done + count]);
        done += count;
    }
    storage.commits.commit(address, bytes.len());
}
