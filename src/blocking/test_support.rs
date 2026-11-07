//! Scriptable blocking targets for unit tests.
//!
//! These fixtures are test-only.  They exercise the adapter's mapping and
//! poisoning rules, and the machine-level fixtures count transactions and
//! debug transport calls so the "one transaction per access" and
//! "host views use debug transport" controls can be asserted.

use super::{
    BlockingAtomicOperation, BlockingAtomicOutcome, BlockingCommand, BlockingDebug,
    BlockingDeviceEvent, BlockingDeviceTarget, BlockingDirectMemAccess, BlockingInitiator,
    BlockingResponseStatus, BlockingTarget, BlockingTransaction, Femtoseconds,
};
use crate::memory::SimpleMemory;
use crate::peripherals::uart16550::{lsr_bits, reg_offset, UART_SIZE};
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
    /// The request's D11 local-time offset in exact femtoseconds.
    pub local_offset_fs: u64,
    /// The request's nb D3 `may_suspend` flag.
    pub may_suspend: bool,
    /// True when the request carried the atomic extension.
    pub atomic: bool,
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
    /// The scripted host elapsed time `e` of each response (nb D4).
    elapsed: AtomicU64,
    unknown: AtomicBool,
    panic: AtomicBool,
    mutation: Mutex<Mutation>,
    debug_short: AtomicBool,
    reset_ack: AtomicBool,
    held_write: Mutex<Option<(u64, Vec<u8>)>>,
    device_event: Mutex<Option<BlockingDeviceEvent>>,
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
            elapsed: AtomicU64::new(0),
            unknown: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            mutation: Mutex::new(Mutation::None),
            debug_short: AtomicBool::new(false),
            reset_ack: AtomicBool::new(true),
            held_write: Mutex::new(None),
            device_event: Mutex::new(None),
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

    /// Sets the host elapsed time each response reports (nb D4).  A
    /// non-suspending target that reports a non-zero value is a protocol
    /// failure of the access.
    pub(crate) fn set_elapsed(&self, elapsed: u64) {
        self.elapsed.store(elapsed, Ordering::SeqCst);
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

    /// Reports one device event on every `Ok` access (device D2).
    ///
    /// Only a device window may report one, so this makes a RAM target
    /// misbehave on purpose: the adapter must classify the access as a
    /// protocol failure.
    pub(crate) fn set_device_event(&self, event: BlockingDeviceEvent) {
        *self.device_event.lock().unwrap() = Some(event);
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
            local_offset_fs: transaction.local_offset().as_u64(),
            may_suspend: transaction.may_suspend(),
            atomic: transaction.atomic_extension().is_some(),
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
        let delay = self.delay.load(Ordering::SeqCst);
        if delay != 0 {
            transaction.set_response_delay(Femtoseconds::new(delay));
        }
        let elapsed = self.elapsed.load(Ordering::SeqCst);
        if elapsed != 0 {
            transaction.set_host_elapsed(Femtoseconds::new(elapsed));
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
        match transaction.command() {
            BlockingCommand::Read => {
                let bytes = self.read(transaction.address(), transaction.data().len());
                transaction.data_mut().copy_from_slice(&bytes);
            }
            BlockingCommand::Write => {
                self.write(transaction.address(), transaction.data());
            }
        }
        if let Some(event) = *self.device_event.lock().unwrap() {
            transaction.report_device_event(event);
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
    /// A grant of the queried 4 KiB chunk, but only when the queried address
    /// lies inside `first..=last`; every other address is denied.  It models a
    /// target that offers direct memory for its data region only.
    ChunkWithin {
        first: u64,
        last: u64,
        access: BlockingDirectMemAccess,
    },
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
    /// True while the next store-conditional envelope reports conditional
    /// success even though the request's reservation context does not cover it.
    misreport_sc_success: AtomicBool,
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
            misreport_sc_success: AtomicBool::new(false),
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

    /// Makes exactly the next store-conditional envelope report conditional
    /// success without writing, whatever its reservation context covers.
    pub(crate) fn misreport_next_store_conditional_success(&self) {
        self.misreport_sc_success.store(true, Ordering::SeqCst);
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
            local_offset_fs: transaction.local_offset().as_u64(),
            may_suspend: transaction.may_suspend(),
            atomic: transaction.atomic_extension().is_some(),
        });
        if transaction.atomic_extension().is_some() {
            self.atomics.fetch_add(1, Ordering::SeqCst);
            let store_conditional = transaction.atomic_extension().is_some_and(|atomic| {
                matches!(
                    atomic.operation(),
                    BlockingAtomicOperation::StoreConditional
                )
            });
            if store_conditional && self.fault_sc.swap(false, Ordering::SeqCst) {
                transaction.set_status(BlockingResponseStatus::AddressError);
                return;
            }
            if store_conditional && self.misreport_sc_success.swap(false, Ordering::SeqCst) {
                if let Some(extension) = transaction.atomic_extension_mut() {
                    extension.set_outcome(BlockingAtomicOutcome::StoreConditional(
                        crate::physical::ConditionalStatus::Success,
                    ));
                }
                transaction.set_status(BlockingResponseStatus::Ok);
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
            DirectMemReply::ChunkWithin {
                first,
                last,
                access,
            } => {
                if !(first <= query.address() && query.address() <= last) {
                    return super::BlockingDirectMemResponse::Denied;
                }
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

/// The scripted outcome of a [`SuspendingTarget`]'s data access (nb D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptedOutcome {
    /// Complete `Ok`, with the bytes or the atomic operation applied.
    Ok,
    /// Report this status instead of completing, with the bytes still applied.
    Status(BlockingResponseStatus),
    /// Report the outcome-unknown flag, which poisons the transport.
    Unknown,
}

/// A suspending test target with scripted host elapsed time (nb D2–D4).
///
/// It declares [`BlockingTarget::suspends_host_time`], delegates the bytes, the
/// atomic table and the direct-memory grants to [`super::ZeroDelayRam`], and
/// scripts each response's host elapsed time `e` and delay `d` together with the
/// outcome of its data accesses.  Fetches keep completing `Ok`, so a fixture
/// reaches the data access whose rule it is proving.
///
/// It logs every request so a test can prove which transactions the adapter
/// marked `may_suspend`, and counts its direct-memory queries so a test can
/// prove a direct copy issued no transaction at all.
pub(crate) struct SuspendingTarget {
    inner: super::ZeroDelayRam,
    declared_max: AtomicU64,
    elapsed: AtomicU64,
    delay: AtomicU64,
    data_outcome: Mutex<ScriptedOutcome>,
    /// Blocking transactions issued.
    pub transactions: AtomicUsize,
    /// Direct-memory queries issued.
    pub queries: AtomicUsize,
    /// Debug-transport requests issued.
    pub debug_requests: AtomicUsize,
    log: Mutex<Vec<Call>>,
}

impl SuspendingTarget {
    /// Creates a suspending target.
    ///
    /// The declared maximum starts at zero, exactly as the in-tree target
    /// declares; a fixture that scripts a non-zero `e` or `d` must raise it.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: super::ZeroDelayRam::new(),
            declared_max: AtomicU64::new(0),
            elapsed: AtomicU64::new(0),
            delay: AtomicU64::new(0),
            data_outcome: Mutex::new(ScriptedOutcome::Ok),
            transactions: AtomicUsize::new(0),
            queries: AtomicUsize::new(0),
            debug_requests: AtomicUsize::new(0),
            log: Mutex::new(Vec::new()),
        })
    }

    /// Sets the declared per-transaction maximum delay.
    pub(crate) fn set_declared_max_delay(&self, delay: u64) {
        self.declared_max.store(delay, Ordering::SeqCst);
    }

    /// Sets the host elapsed time `e` every response reports.
    pub(crate) fn set_elapsed(&self, elapsed: u64) {
        self.elapsed.store(elapsed, Ordering::SeqCst);
    }

    /// Sets the response delay `d` every response reports.
    pub(crate) fn set_delay(&self, delay: u64) {
        self.delay.store(delay, Ordering::SeqCst);
    }

    /// Sets the scripted outcome of every non-fetch request.
    pub(crate) fn set_data_outcome(&self, outcome: ScriptedOutcome) {
        *self.data_outcome.lock().unwrap() = outcome;
    }

    /// The logged blocking transactions.
    pub(crate) fn log(&self) -> Vec<Call> {
        self.log.lock().unwrap().clone()
    }

    /// Direct inspection read of the backing store.
    pub(crate) fn bytes_at(&self, address: u64, len: usize) -> Vec<u8> {
        self.inner.bytes_at(address, len)
    }
}

impl BlockingTarget for SuspendingTarget {
    fn suspends_host_time(&self) -> bool {
        true
    }

    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::new(self.declared_max.load(Ordering::SeqCst))
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        let access_kind = transaction.extension().access_kind();
        self.log.lock().unwrap().push(Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            access_kind,
            initiator: transaction.extension().initiator(),
            local_offset_fs: transaction.local_offset().as_u64(),
            may_suspend: transaction.may_suspend(),
            atomic: transaction.atomic_extension().is_some(),
        });
        // The bytes and the atomic operation land first, so a scripted failure
        // observes the same storage a completing target leaves.
        self.inner.b_transport(transaction);
        // The response metadata is set after the inner target, which reports
        // neither host elapsed time nor a delay of its own.
        transaction.set_host_elapsed(Femtoseconds::new(self.elapsed.load(Ordering::SeqCst)));
        transaction.set_response_delay(Femtoseconds::new(self.delay.load(Ordering::SeqCst)));
        let outcome = if access_kind == PhysicalAccessKind::Fetch {
            ScriptedOutcome::Ok
        } else {
            *self.data_outcome.lock().unwrap()
        };
        match outcome {
            ScriptedOutcome::Ok => transaction.set_status(BlockingResponseStatus::Ok),
            ScriptedOutcome::Status(status) => transaction.set_status(status),
            ScriptedOutcome::Unknown => transaction.extension_mut().mark_outcome_unknown(),
        }
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        self.debug_requests.fetch_add(1, Ordering::SeqCst);
        self.inner.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.inner.transport_reset()
    }

    fn get_direct_mem_ptr(
        &self,
        query: super::BlockingDirectMemQuery,
    ) -> super::BlockingDirectMemResponse {
        self.queries.fetch_add(1, Ordering::SeqCst);
        self.inner.get_direct_mem_ptr(query)
    }

    fn register_direct_mem_invalidator(&self, invalidator: super::BlockingDirectMemInvalidator) {
        self.inner.register_direct_mem_invalidator(invalidator);
    }
}

impl fmt::Debug for SuspendingTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SuspendingTarget")
            .field("transactions", &self.transactions.load(Ordering::SeqCst))
            .field("queries", &self.queries.load(Ordering::SeqCst))
            .finish()
    }
}

/// The deliberate misbehaviour a [`HostUart`] adds to an otherwise correct
/// response (device D2/D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HostUartFault {
    /// Answer the access correctly and report its window's event.
    #[default]
    None,
    /// Report this status with the transmit event still attached, so an event
    /// beside a non-`Ok` status is observable.
    StatusWithEvent(BlockingResponseStatus),
    /// Report this status with no event at all.
    Status(BlockingResponseStatus),
    /// Complete and report the window's event twice.
    SecondEvent,
    /// Complete and report the other window's event instead.
    WrongWindowEvent,
    /// Complete and change the payload's start address.
    ChangedPayload,
    /// Complete and annotate this response delay in exact femtoseconds.
    Delay(u64),
    /// Complete and report this host elapsed time in exact femtoseconds.
    Elapsed(u64),
    /// Complete and set the direct-memory hint.
    DmiHint,
    /// Report the window's event with the outcome-unknown flag, as a target
    /// that cannot vouch that its effect committed does.
    Unknown,
}

/// A host UART target for the device-target rows (device D2, D5, D6).
///
/// It models the little register state the in-crate guests use: `LSR` answers
/// the transmit-empty bits, an `RBR`/`THR` write with `DLAB` clear accepts the
/// byte and reports exactly one [`BlockingDeviceEvent::UartTransmit`], and an
/// `RBR` read drains the bytes the host pushed with
/// [`BlockingDeviceTarget::receive`].  Every response can be scripted to
/// misbehave on purpose, and a reset clears the register file, so a fixture
/// observes the adapter's protocol-failure rows and the Platform's reset call
/// through a target that really is outside the crate's control.
pub(crate) struct HostUart {
    base: u64,
    registers: Mutex<[u8; UART_SIZE]>,
    /// The transmit bytes this target accepted.
    transmitted: Mutex<Vec<u8>>,
    /// The bytes the host pushed into the receive direction.
    received: Mutex<Vec<u8>>,
    /// Blocking transactions issued.
    pub transactions: AtomicUsize,
    /// Transport resets issued.
    pub resets: AtomicUsize,
    /// Bytes `receive` delivered.
    pub receives: AtomicUsize,
    /// The answer `transport_reset` gives.
    reset_ack: AtomicBool,
    /// True while `transport_reset` panics instead of answering.
    reset_panic: AtomicBool,
    /// `LSR`'s answer, so a guest can be made to proceed or to spin.
    lsr: AtomicU64,
    fault: Mutex<HostUartFault>,
    log: Mutex<Vec<Call>>,
}

impl HostUart {
    /// Creates a host UART target for the window at `base`.
    ///
    /// `LSR` starts at transmit-holding-register empty plus transmitter empty,
    /// which is the answer a guest's poll loop needs to proceed.
    pub(crate) fn new(base: u64) -> Arc<Self> {
        Arc::new(Self {
            base,
            registers: Mutex::new([0; UART_SIZE]),
            transmitted: Mutex::new(Vec::new()),
            received: Mutex::new(Vec::new()),
            transactions: AtomicUsize::new(0),
            resets: AtomicUsize::new(0),
            receives: AtomicUsize::new(0),
            reset_ack: AtomicBool::new(true),
            reset_panic: AtomicBool::new(false),
            lsr: AtomicU64::new(u64::from(lsr_bits::THRE | lsr_bits::TEMT)),
            fault: Mutex::new(HostUartFault::None),
            log: Mutex::new(Vec::new()),
        })
    }

    /// Sets the misbehaviour the next responses add.
    pub(crate) fn set_fault(&self, fault: HostUartFault) {
        *self.fault.lock().unwrap() = fault;
    }

    /// Sets `LSR`'s answer.
    pub(crate) fn set_lsr(&self, value: u8) {
        self.lsr.store(u64::from(value), Ordering::SeqCst);
    }

    /// Sets whether a transport reset is acknowledged.
    pub(crate) fn set_reset_ack(&self, ack: bool) {
        self.reset_ack.store(ack, Ordering::SeqCst);
    }

    /// Makes the next transport resets panic instead of answering.
    pub(crate) fn set_reset_panic(&self, panic: bool) {
        self.reset_panic.store(panic, Ordering::SeqCst);
    }

    /// Blocking transactions issued.
    pub(crate) fn transactions(&self) -> usize {
        self.transactions.load(Ordering::SeqCst)
    }

    /// Transport resets issued.
    pub(crate) fn resets(&self) -> usize {
        self.resets.load(Ordering::SeqCst)
    }

    /// The transmit bytes this target accepted.
    pub(crate) fn transmitted(&self) -> Vec<u8> {
        self.transmitted.lock().unwrap().clone()
    }

    /// The bytes the host pushed into the receive direction.
    pub(crate) fn received(&self) -> Vec<u8> {
        self.received.lock().unwrap().clone()
    }

    /// The logged blocking transactions.
    pub(crate) fn log(&self) -> Vec<Call> {
        self.log.lock().unwrap().clone()
    }

    /// One register read at `offset` within the window.
    fn read_reg(&self, offset: u64) -> u8 {
        match usize::try_from(offset)
            .ok()
            .filter(|offset| *offset < UART_SIZE)
        {
            Some(offset) if offset == reg_offset::RBR_THR as usize => {
                *self.received.lock().unwrap().first().unwrap_or(&0)
            }
            Some(offset) if offset == reg_offset::LSR as usize => {
                self.lsr.load(Ordering::SeqCst) as u8
            }
            Some(offset) => self.registers.lock().unwrap()[offset],
            // The window is wider than the device range; an answer outside it
            // reads zero, as an unimplemented register does.
            None => 0,
        }
    }

    /// One register write at `offset` within the window, reporting a THR byte.
    fn write_reg(&self, offset: u64, byte: u8) -> Option<u8> {
        let offset = usize::try_from(offset)
            .ok()
            .filter(|offset| *offset < UART_SIZE)?;
        let mut registers = self.registers.lock().unwrap();
        let dlab = registers[reg_offset::LCR as usize] & 0x80 != 0;
        if offset == reg_offset::RBR_THR as usize && !dlab {
            self.transmitted.lock().unwrap().push(byte);
            return Some(byte);
        }
        registers[offset] = byte;
        None
    }
}

impl BlockingDeviceTarget for HostUart {
    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.transactions.fetch_add(1, Ordering::SeqCst);
        self.log.lock().unwrap().push(Call {
            address: transaction.address(),
            command: transaction.command(),
            length: transaction.data_length(),
            access_kind: transaction.extension().access_kind(),
            initiator: transaction.extension().initiator(),
            local_offset_fs: transaction.local_offset().as_u64(),
            may_suspend: transaction.may_suspend(),
            atomic: transaction.atomic_extension().is_some(),
        });
        let offset = transaction.address().wrapping_sub(self.base);
        let event = match transaction.command() {
            BlockingCommand::Read => {
                let byte = self.read_reg(offset);
                transaction.data_mut()[0] = byte;
                None
            }
            BlockingCommand::Write => self
                .write_reg(offset, transaction.data()[0])
                .map(BlockingDeviceEvent::UartTransmit),
        };
        let fault = *self.fault.lock().unwrap();
        let report = |transaction: &mut BlockingTransaction<'_>| {
            if let Some(event) = event {
                transaction.report_device_event(event);
            }
        };
        match fault {
            HostUartFault::None => report(transaction),
            HostUartFault::StatusWithEvent(status) => {
                report(transaction);
                transaction.set_status(status);
                return;
            }
            HostUartFault::Status(status) => {
                transaction.set_status(status);
                return;
            }
            HostUartFault::SecondEvent => {
                report(transaction);
                report(transaction);
            }
            HostUartFault::WrongWindowEvent => {
                transaction.report_device_event(BlockingDeviceEvent::HtifWrite(0x5a));
            }
            HostUartFault::ChangedPayload => {
                report(transaction);
                transaction.set_address(transaction.address().wrapping_add(1));
            }
            HostUartFault::Delay(femtoseconds) => {
                report(transaction);
                transaction.set_response_delay(Femtoseconds::new(femtoseconds));
            }
            HostUartFault::Elapsed(femtoseconds) => {
                report(transaction);
                transaction.set_host_elapsed(Femtoseconds::new(femtoseconds));
            }
            HostUartFault::DmiHint => {
                report(transaction);
                transaction.set_dmi_allowed(true);
            }
            HostUartFault::Unknown => {
                // The contract's case is `Ok` with a valid event and the
                // unknown flag: the flag alone must make it unknown.
                report(transaction);
                transaction.set_status(BlockingResponseStatus::Ok);
                transaction.extension_mut().mark_outcome_unknown();
                return;
            }
        }
        transaction.set_status(BlockingResponseStatus::Ok);
    }

    fn receive(&self, byte: u8) {
        self.receives.fetch_add(1, Ordering::SeqCst);
        self.received.lock().unwrap().push(byte);
    }

    fn transport_reset(&self) -> bool {
        self.resets.fetch_add(1, Ordering::SeqCst);
        if self.reset_panic.load(Ordering::SeqCst) {
            panic!("scripted host UART transport-reset panic");
        }
        if !self.reset_ack.load(Ordering::SeqCst) {
            return false;
        }
        // An acknowledged reset quiesces the target: no register and no
        // received byte of the previous generation survives.
        *self.registers.lock().unwrap() = [0; UART_SIZE];
        self.received.lock().unwrap().clear();
        true
    }
}

impl fmt::Debug for HostUart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostUart")
            .field("transactions", &self.transactions())
            .field("resets", &self.resets())
            .field("transmitted", &self.transmitted())
            .finish()
    }
}
