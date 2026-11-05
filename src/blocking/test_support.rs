//! Scriptable blocking targets for unit tests.
//!
//! These fixtures are test-only.  They exercise the adapter's mapping and
//! poisoning rules, and the machine-level fixtures count transactions and
//! debug transport calls so the "one transaction per access" and
//! "host views use debug transport" controls can be asserted.

use super::{
    BlockingAtomicOperation, BlockingCommand, BlockingDebug, BlockingDirectMemAccess,
    BlockingInitiator, BlockingResponseStatus, BlockingTarget, BlockingTransaction, Femtoseconds,
};
use crate::memory::SimpleMemory;
use crate::physical::PhysicalAccessKind;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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
    pub initiator: BlockingInitiator,
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
    /// Declared per-transaction maximum delay (dev-plan D7).
    declared_max: AtomicU64,
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
            declared_max: AtomicU64::new(0),
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

    /// Sets the declared per-transaction maximum delay.
    pub(crate) fn set_declared_max_delay(&self, delay: u64) {
        self.declared_max.store(delay, Ordering::SeqCst);
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
        Femtoseconds::new(self.declared_max.load(Ordering::SeqCst))
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            access_kind: transaction.extension().access_kind(),
            initiator: transaction.extension().initiator(),
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

/// A test-only delaying target over [`super::ZeroDelayRam`].
///
/// It supports the U2 atomic extension (delegated to the in-tree zero-delay
/// target) and annotates a fixed delay after the inner target completes, so
/// U3 can exercise the transport-delay path end to end without changing the
/// in-tree `ZeroDelayRam` declaration.
pub(crate) struct DelayingRam {
    inner: super::ZeroDelayRam,
    declared_max: AtomicU64,
    delay: AtomicU64,
    /// Blocking transactions issued.
    pub transactions: AtomicUsize,
}

impl DelayingRam {
    pub(crate) fn new(declared_max: u64, delay: u64) -> Arc<Self> {
        Arc::new(Self {
            inner: super::ZeroDelayRam::new(),
            declared_max: AtomicU64::new(declared_max),
            delay: AtomicU64::new(delay),
            transactions: AtomicUsize::new(0),
        })
    }

    pub(crate) fn set_delay(&self, delay: u64) {
        self.delay.store(delay, Ordering::SeqCst);
    }

    pub(crate) fn set_declared_max_delay(&self, delay: u64) {
        self.declared_max.store(delay, Ordering::SeqCst);
    }

    pub(crate) fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        self.inner.bytes_at(address, len)
    }
}

impl BlockingTarget for DelayingRam {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::new(self.declared_max.load(Ordering::SeqCst))
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        self.inner.b_transport(transaction);
        let delay = self.delay.load(Ordering::SeqCst);
        if delay != 0 {
            transaction.set_response_delay(Femtoseconds::new(delay));
        }
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.inner.transport_reset()
    }
}

/// How a [`DirectMemTarget`] answers one direct-memory query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectMemReply {
    /// No grant for this access.
    Denied,
    /// A contained failure of the query itself.
    Failed,
    /// A grant of the queried 4 KiB chunk with this access and the target's
    /// configured grant latencies.
    Chunk(BlockingDirectMemAccess),
    /// A literal grant shape, for malformed-grant controls.
    Custom {
        access: BlockingDirectMemAccess,
        start: u64,
        end: u64,
        read_latency: u64,
        write_latency: u64,
        null_pointer: bool,
    },
}

/// A counting direct-memory target over [`super::ZeroDelayRam`].
///
/// It delegates the bytes to the in-tree zero-delay target, so `b_transport`,
/// debug transport, atomics and committed-write bookkeeping behave exactly as
/// there.  On top of that it counts transactions and queries, controls the
/// `dmi_allowed` hint and the query answer, can annotate a transaction delay
/// together with a matching grant latency, and can free its storage on reset.
/// A target that frees storage makes a stale grant observable as a wrong byte
/// or a wrong transaction count.
pub(crate) struct DirectMemTarget {
    inner: super::ZeroDelayRam,
    registry: super::BlockingDirectMemRegistry,
    base: u64,
    size: usize,
    /// Blocking transactions issued.
    pub transactions: AtomicUsize,
    /// Direct-memory queries issued.
    pub queries: AtomicUsize,
    /// Atomic envelopes issued.
    pub atomics: AtomicUsize,
    /// True while the next store-conditional envelope reports a target fault.
    fault_sc: AtomicBool,
    /// Debug-transport requests issued.
    pub debug_requests: AtomicUsize,
    /// Transport resets issued.
    pub resets: AtomicUsize,
    /// Whether a completed non-atomic transaction carries the hint.
    hint: AtomicBool,
    reply: Mutex<DirectMemReply>,
    /// The last direct-memory query, for access-direction assertions.
    last_query: Mutex<Option<super::BlockingDirectMemQuery>>,
    declared_max: AtomicU64,
    delay: AtomicU64,
    grant_read_latency: AtomicU64,
    grant_write_latency: AtomicU64,
    status: Mutex<BlockingResponseStatus>,
    unknown: AtomicBool,
    debug_short: AtomicBool,
    /// Whether a transport reset invalidates every registration, as a
    /// conforming target must.
    reset_invalidates: AtomicBool,
    /// Runs inside `b_transport`, before the transaction completes.
    in_transport: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// Runs inside `get_direct_mem_ptr`, after the adapter's query began.
    in_query: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    log: Mutex<Vec<Call>>,
}

impl DirectMemTarget {
    pub(crate) fn new(base: u64, size: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: super::ZeroDelayRam::new(),
            registry: super::BlockingDirectMemRegistry::new(),
            base,
            size,
            transactions: AtomicUsize::new(0),
            queries: AtomicUsize::new(0),
            atomics: AtomicUsize::new(0),
            fault_sc: AtomicBool::new(false),
            debug_requests: AtomicUsize::new(0),
            resets: AtomicUsize::new(0),
            hint: AtomicBool::new(true),
            reply: Mutex::new(DirectMemReply::Chunk(BlockingDirectMemAccess::Read)),
            last_query: Mutex::new(None),
            declared_max: AtomicU64::new(0),
            delay: AtomicU64::new(0),
            grant_read_latency: AtomicU64::new(0),
            grant_write_latency: AtomicU64::new(0),
            status: Mutex::new(BlockingResponseStatus::Ok),
            unknown: AtomicBool::new(false),
            debug_short: AtomicBool::new(false),
            reset_invalidates: AtomicBool::new(true),
            in_transport: Mutex::new(None),
            in_query: Mutex::new(None),
            log: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn set_hint(&self, hint: bool) {
        self.hint.store(hint, Ordering::SeqCst);
    }

    /// Makes exactly the next store-conditional envelope report a target fault.
    pub(crate) fn fault_next_store_conditional(&self) {
        self.fault_sc.store(true, Ordering::SeqCst);
    }

    /// The last direct-memory query this target answered.
    pub(crate) fn last_query(&self) -> Option<super::BlockingDirectMemQuery> {
        *self.last_query.lock().unwrap()
    }

    pub(crate) fn set_reply(&self, reply: DirectMemReply) {
        *self.reply.lock().unwrap() = reply;
    }

    pub(crate) fn set_declared_max_delay(&self, delay: u64) {
        self.declared_max.store(delay, Ordering::SeqCst);
    }

    pub(crate) fn set_delay(&self, delay: u64) {
        self.delay.store(delay, Ordering::SeqCst);
    }

    pub(crate) fn set_grant_latency(&self, read: u64, write: u64) {
        self.grant_read_latency.store(read, Ordering::SeqCst);
        self.grant_write_latency.store(write, Ordering::SeqCst);
    }

    pub(crate) fn set_unknown(&self, unknown: bool) {
        self.unknown.store(unknown, Ordering::SeqCst);
    }

    pub(crate) fn set_debug_short(&self, short: bool) {
        self.debug_short.store(short, Ordering::SeqCst);
    }

    pub(crate) fn set_reset_invalidates(&self, invalidates: bool) {
        self.reset_invalidates.store(invalidates, Ordering::SeqCst);
    }

    pub(crate) fn on_transport(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        *self.in_transport.lock().unwrap() = Some(hook);
    }

    pub(crate) fn clear_transport_hook(&self) {
        *self.in_transport.lock().unwrap() = None;
    }

    pub(crate) fn on_query(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        *self.in_query.lock().unwrap() = Some(hook);
    }

    pub(crate) fn clear_query_hook(&self) {
        *self.in_query.lock().unwrap() = None;
    }

    pub(crate) fn transactions(&self) -> usize {
        self.transactions.load(Ordering::SeqCst)
    }

    pub(crate) fn queries(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }

    /// Atomic envelopes this target served.
    pub(crate) fn atomics(&self) -> usize {
        self.atomics.load(Ordering::SeqCst)
    }

    /// Direct inspection read of the bytes behind a grant.
    pub(crate) fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        self.inner.bytes_at(address, len)
    }

    /// Registrations this target keeps, including adapters that are gone.
    pub(crate) fn registrations(&self) -> usize {
        self.registry.registrations()
    }

    /// Invalidates the whole space in every registration, as a target does
    /// before it discards granted storage (direct-memory D8/D10).
    pub(crate) fn invalidate_everything(&self) {
        self.registry.invalidate_all();
    }

    pub(crate) fn log(&self) -> Vec<Call> {
        self.log.lock().unwrap().clone()
    }

    fn chunk_of(&self, address: u64) -> u64 {
        address >> 12
    }
}

impl BlockingTarget for DirectMemTarget {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::new(self.declared_max.load(Ordering::SeqCst))
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            access_kind: transaction.extension().access_kind(),
            initiator: transaction.extension().initiator(),
        });
        if transaction.atomic_extension().is_some() {
            self.atomics.fetch_add(1, Ordering::SeqCst);
            if transaction.atomic_extension().is_some_and(|atomic| {
                matches!(
                    atomic.operation(),
                    BlockingAtomicOperation::StoreConditional
                )
            }) && self.fault_sc.swap(false, Ordering::SeqCst)
            {
                transaction.set_status(BlockingResponseStatus::AddressError);
                return;
            }
        }
        if let Some(hook) = self.in_transport.lock().unwrap().clone() {
            hook();
        }
        if self.unknown.load(Ordering::SeqCst) {
            transaction.extension_mut().mark_outcome_unknown();
            return;
        }
        let status = *self.status.lock().unwrap();
        if status != BlockingResponseStatus::Ok {
            transaction.set_status(status);
            return;
        }
        self.inner.b_transport(transaction);
        if transaction.status() != BlockingResponseStatus::Ok {
            return;
        }
        let delay = self.delay.load(Ordering::SeqCst);
        if delay != 0 {
            transaction.set_response_delay(Femtoseconds::new(delay));
        }
        // The hint is the test's choice, not the inner target's.
        let hint = self.hint.load(Ordering::SeqCst);
        transaction.set_dmi_allowed(hint);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.debug_requests.fetch_add(1, Ordering::SeqCst);
        let len = transaction.data().len();
        if self.debug_short.load(Ordering::SeqCst) {
            return len / 2;
        }
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.resets.fetch_add(1, Ordering::SeqCst);
        if self.reset_invalidates.load(Ordering::SeqCst) {
            self.registry.invalidate_all();
        }
        // The inner target discards every chunk, so the storage a grant points
        // at is gone after this call.
        self.inner.transport_reset()
    }

    fn get_direct_mem_ptr(
        &self,
        query: super::BlockingDirectMemQuery,
    ) -> super::BlockingDirectMemResponse {
        self.queries.fetch_add(1, Ordering::SeqCst);
        *self.last_query.lock().unwrap() = Some(query);
        if let Some(hook) = self.in_query.lock().unwrap().clone() {
            hook();
        }
        let reply = *self.reply.lock().unwrap();
        let (access, start, end, read, write, null_pointer) = match reply {
            DirectMemReply::Denied => return super::BlockingDirectMemResponse::Denied,
            DirectMemReply::Failed => return super::BlockingDirectMemResponse::Failed,
            DirectMemReply::Chunk(access) => {
                let chunk = self.chunk_of(query.address());
                let start = chunk << 12;
                (
                    access,
                    start,
                    start + 0xfff,
                    self.grant_read_latency.load(Ordering::SeqCst),
                    self.grant_write_latency.load(Ordering::SeqCst),
                    false,
                )
            }
            DirectMemReply::Custom {
                access,
                start,
                end,
                read_latency,
                write_latency,
                null_pointer,
            } => (
                access,
                start,
                end,
                read_latency,
                write_latency,
                null_pointer,
            ),
        };
        if start < self.base || end >= self.base + self.size as u64 || start > end {
            // A malformed shape whose pointer cannot be derived: report the
            // null pointer the adapter must reject.
            let grant = unsafe {
                super::BlockingDirectMemGrant::new(
                    start,
                    end,
                    std::ptr::null_mut(),
                    access,
                    Femtoseconds::new(read),
                    Femtoseconds::new(write),
                )
            };
            return super::BlockingDirectMemResponse::Granted(grant);
        }
        let pointer = match self.inner.chunk_pointer(start) {
            Some((chunk_start, _, chunk_pointer)) => {
                chunk_pointer.wrapping_add((start - chunk_start) as usize)
            }
            None => std::ptr::null_mut(),
        };
        let pointer = if null_pointer {
            std::ptr::null_mut()
        } else {
            pointer
        };
        // SAFETY: the pointer addresses the granted start byte inside the
        // inner target's atomics, and a reset invalidates every registration
        // before it discards those chunks.
        let grant = unsafe {
            super::BlockingDirectMemGrant::new(
                start,
                end,
                pointer,
                access,
                Femtoseconds::new(read),
                Femtoseconds::new(write),
            )
        };
        super::BlockingDirectMemResponse::Granted(grant)
    }

    fn register_direct_mem_invalidator(&self, invalidator: super::BlockingDirectMemInvalidator) {
        self.registry.register(invalidator);
    }
}

impl fmt::Debug for DirectMemTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DirectMemTarget")
            .field("transactions", &self.transactions())
            .field("queries", &self.queries())
            .field("registrations", &self.registrations())
            .finish()
    }
}
