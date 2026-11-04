//! In-tree zero-delay Rust RAM target (dev-plan D2).
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
//! value.

use super::{BlockingCommand, BlockingDebug, BlockingResponseStatus, BlockingTarget};
use super::{BlockingTransaction, Femtoseconds};
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

/// An in-tree blocking RAM target that annotates zero delay.
#[derive(Default)]
pub struct ZeroDelayRam {
    storage: Mutex<Chunks>,
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
            Ok(storage) => read(&storage, address, len),
            Err(_) => vec![0; len],
        }
    }

    /// Number of allocated 4 KiB chunks, for diagnostics.
    pub fn allocated_chunks(&self) -> usize {
        match self.storage.lock() {
            Ok(storage) => storage.len(),
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
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = read(&storage, transaction.address(), transaction.data().len());
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                write(&mut storage, transaction.address(), transaction.data());
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
                let bytes = read(&storage, transaction.address(), len);
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
                storage.clear();
                true
            }
            Err(_) => false,
        }
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

fn write(storage: &mut Chunks, address: u64, bytes: &[u8]) {
    let mut done = 0;
    while done < bytes.len() {
        let current = address.wrapping_add(done as u64);
        let chunk = current >> CHUNK_BITS;
        let offset = (current & CHUNK_MASK) as usize;
        let count = (CHUNK_SIZE - offset).min(bytes.len() - done);
        let data = storage
            .entry(chunk)
            .or_insert_with(|| Box::new([0u8; CHUNK_SIZE]));
        data[offset..offset + count].copy_from_slice(&bytes[done..done + count]);
        done += count;
    }
}
