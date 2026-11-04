//! Scriptable blocking targets for unit tests.
//!
//! These fixtures are test-only.  They exercise the adapter's mapping and
//! poisoning rules, and the machine-level fixtures count transactions and
//! debug transport calls so the "one transaction per access" and
//! "host views use debug transport" controls can be asserted.

use super::{
    BlockingCommand, BlockingDebug, BlockingResponseStatus, BlockingTarget, BlockingTransaction,
    Femtoseconds,
};
use crate::memory::SimpleMemory;
use crate::physical::PhysicalAccessKind;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/// A request-identity mutation the target applies before returning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    /// Leave the request identity unchanged.
    None,
    /// Change the payload address.
    Address,
    /// Change the payload command.
    Command,
    /// Change the payload length.
    Length,
}

/// One logged blocking transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Call {
    pub address: u64,
    pub command: BlockingCommand,
    pub length: usize,
    pub access_kind: PhysicalAccessKind,
}

/// A scriptable blocking target over one `SimpleMemory` backing store.
pub(crate) struct ScriptTarget {
    base: u64,
    memory: Mutex<SimpleMemory>,
    /// Blocking transactions issued.
    pub transactions: AtomicUsize,
    /// Debug-transport requests issued.
    pub debug_requests: AtomicUsize,
    /// Transport resets issued.
    pub resets: AtomicUsize,
    status: Mutex<BlockingResponseStatus>,
    delay: AtomicU64,
    unknown: AtomicBool,
    panic: AtomicBool,
    mutation: Mutex<Mutation>,
    debug_short: AtomicBool,
    reset_ack: AtomicBool,
    held_write: Mutex<Option<(u64, Vec<u8>)>>,
    log: Mutex<Vec<Call>>,
}

impl ScriptTarget {
    /// Creates a scripted target for the physical window `base..base+size`.
    pub(crate) fn new(base: u64, size: usize) -> Self {
        Self {
            base,
            memory: Mutex::new(SimpleMemory::new(size)),
            transactions: AtomicUsize::new(0),
            debug_requests: AtomicUsize::new(0),
            resets: AtomicUsize::new(0),
            status: Mutex::new(BlockingResponseStatus::Ok),
            delay: AtomicU64::new(0),
            unknown: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            mutation: Mutex::new(Mutation::None),
            debug_short: AtomicBool::new(false),
            reset_ack: AtomicBool::new(true),
            held_write: Mutex::new(None),
            log: Mutex::new(Vec::new()),
        }
    }

    /// Sets the status the next transactions report.
    pub(crate) fn set_status(&self, status: BlockingResponseStatus) {
        *self.status.lock().unwrap() = status;
    }

    /// Sets the annotated response delay.
    pub(crate) fn set_delay(&self, delay: u64) {
        self.delay.store(delay, Ordering::SeqCst);
    }

    /// Marks the next transactions' outcome unknown.
    pub(crate) fn set_unknown(&self, unknown: bool) {
        self.unknown.store(unknown, Ordering::SeqCst);
    }

    /// Makes the next transaction panic.
    pub(crate) fn set_panic(&self, panic: bool) {
        self.panic.store(panic, Ordering::SeqCst);
    }

    /// Sets the request-identity mutation applied before returning.
    pub(crate) fn set_mutation(&self, mutation: Mutation) {
        *self.mutation.lock().unwrap() = mutation;
    }

    /// Makes debug transport transfer only half of each request.
    pub(crate) fn set_debug_short(&self, short: bool) {
        self.debug_short.store(short, Ordering::SeqCst);
    }

    /// Sets whether transport reset is acknowledged.
    pub(crate) fn set_reset_ack(&self, ack: bool) {
        self.reset_ack.store(ack, Ordering::SeqCst);
    }

    /// Holds one write that is applied during the next transport reset.
    pub(crate) fn hold_write(&self, address: u64, bytes: Vec<u8>) {
        *self.held_write.lock().unwrap() = Some((address, bytes));
    }

    /// The logged blocking transactions.
    pub(crate) fn log(&self) -> Vec<Call> {
        self.log.lock().unwrap().clone()
    }

    /// Direct inspection read of the backing store.
    pub(crate) fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        self.memory
            .lock()
            .unwrap()
            .read_bytes(address - self.base, len)
            .unwrap()
    }

    fn read(&self, address: u64, len: usize) -> Vec<u8> {
        self.memory
            .lock()
            .unwrap()
            .read_bytes(address - self.base, len)
            .unwrap()
    }

    fn write(&self, address: u64, bytes: &[u8]) {
        self.memory
            .lock()
            .unwrap()
            .write_bytes(address - self.base, bytes)
            .unwrap();
    }
}

impl BlockingTarget for ScriptTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::ZERO
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            access_kind: transaction.extension().access_kind(),
        });
        if self.panic.load(Ordering::SeqCst) {
            panic!("scripted blocking target panic");
        }
        match *self.mutation.lock().unwrap() {
            Mutation::None => {}
            Mutation::Address => {
                let address = transaction.address();
                transaction.set_address(address.wrapping_add(4));
            }
            Mutation::Command => {
                let command = match transaction.command() {
                    BlockingCommand::Read => BlockingCommand::Write,
                    BlockingCommand::Write => BlockingCommand::Read,
                };
                transaction.set_command(command);
            }
            Mutation::Length => {
                let length = transaction.data_length();
                transaction.set_data_length(length + 1);
            }
        }
        if self.unknown.load(Ordering::SeqCst) {
            transaction.extension_mut().mark_outcome_unknown();
            return;
        }
        let delay = self.delay.load(Ordering::SeqCst);
        if delay != 0 {
            transaction.set_response_delay(Femtoseconds::new(delay));
        }
        let status = *self.status.lock().unwrap();
        if status != BlockingResponseStatus::Ok {
            transaction.set_status(status);
            return;
        }
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = self.read(transaction.address(), transaction.data().len());
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                self.write(transaction.address(), transaction.data());
            }
        }
        transaction.set_status(BlockingResponseStatus::Ok);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.debug_requests.fetch_add(1, Ordering::SeqCst);
        let len = transaction.data().len();
        if self.debug_short.load(Ordering::SeqCst) {
            return len / 2;
        }
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = self.read(transaction.address(), len);
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                self.write(transaction.address(), transaction.data());
            }
        }
        len
    }

    fn transport_reset(&self) -> bool {
        self.resets.fetch_add(1, Ordering::SeqCst);
        if let Some((address, bytes)) = self.held_write.lock().unwrap().take() {
            self.write(address, &bytes);
        }
        self.reset_ack.load(Ordering::SeqCst)
    }
}
