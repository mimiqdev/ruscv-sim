//! The ruscv-sim adapter that maps one physical access onto one blocking
//! transaction (dev-plan D3/D4/D6).
//!
//! The adapter owns every classification rule:
//!
//! * one [`PhysicalRequest`] becomes at most one `b_transport` call, never a
//!   split, merge or retry; a Hart fetch or load a cached grant covers copies
//!   through the target's pointer instead (direct-memory D4);
//! * a changed payload or a left-incomplete status is a protocol failure;
//! * the four fault statuses are guest-visible target faults, generic error is
//!   a host failure, and an unknown flag, panic or non-zero annotation is
//!   never silently accepted;
//! * an unknown completion poisons the transport, so no further access is
//!   issued until [`TlmRamAdapter::transport_reset`] obtains the target's
//!   acknowledgement;
//! * every completed write and AMO is recorded before the response returns
//!   (ADR-0002 §6 committed-write visibility).

use super::direct_mem::DirectMemRegions;
use super::{
    BlockingCommand, BlockingDebug, BlockingDirectMemAccess, BlockingDirectMemGrant,
    BlockingDirectMemQuery, BlockingDirectMemResponse, BlockingExtension, BlockingInitiator,
    BlockingResponseStatus, BlockingTarget, BlockingTransaction, DelayConversionError,
    Femtoseconds, TickPeriod, TurnCompletionBound,
};
use crate::physical::{
    AtomicBackendResult, AtomicRequest, AtomicResponse, PhysicalAccessKind, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalRequestDescriptor, PhysicalResponse,
    PhysicalResponseBytes, PhysicalResponseCompletion, PhysicalTargetRejectionReason,
    MAX_PHYSICAL_ACCESS_BYTES,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

/// One selectable TLM RAM route over a shared blocking target.
pub(crate) struct TlmRamAdapter {
    target: Arc<dyn BlockingTarget>,
    /// The Machine's declared Platform tick period for delay conversion.
    period: TickPeriod,
    /// The target's declared per-transaction maximum, read once at
    /// construction so the response limit and the turn bound use one snapshot.
    declared_max: Femtoseconds,
    /// The target's declared maximum delay reserved by one guest turn.
    bound: TurnCompletionBound,
    /// The cached grants, when direct memory is selected (direct-memory D4/D8).
    /// `None` means the adapter issues no query and ignores every hint.
    /// Dropping the adapter drops the cache, which is the teardown rule; the
    /// target's registration becomes inert because it holds only a weak handle.
    direct: Option<Arc<DirectMemRegions>>,
    state: Mutex<AdapterState>,
}

#[derive(Debug, Default)]
struct AdapterState {
    /// Why the transport can no longer issue a transaction, if it cannot.
    poisoned: Option<String>,
    /// Monotonic count of completed writes and AMOs.
    committed_writes: u64,
    /// True while a guest load-reserved's reservation may be live: the
    /// store-suspension gate of direct-memory D7.  A target's store-conditional
    /// check reads committed-write versions that a direct store never updates,
    /// so while this is set every store takes `b_transport` and the target
    /// records it.  A conclusive guest store-conditional clears it; a faulted
    /// one leaves it set, because the Hart keeps its reservation.
    writes_suspended: bool,
    /// Exact running total of charged guest-turn delay in femtoseconds.
    delay: Femtoseconds,
    /// True between [`TlmRamAdapter::begin_turn`] and its take/discard.
    /// Only an active guest turn charges Platform time; debug work is dropped.
    active: bool,
    /// Delay converted since the last turn boundary.
    pending: PendingDelay,
}

/// Delay accumulated since the last turn boundary.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingDelay {
    /// Converted Platform ticks.
    pub(crate) ticks: u64,
    /// Exact annotated femtoseconds.
    pub(crate) femtoseconds: Femtoseconds,
}

/// One access whose annotated delay is being accounted.
#[derive(Clone, Copy)]
enum DelayAccess {
    Physical(crate::physical::PhysicalAccessKind),
    Atomic(crate::physical::AtomicAccessKind),
}

impl DelayAccess {
    fn label(self) -> String {
        match self {
            Self::Physical(kind) => format!("{kind:?}"),
            Self::Atomic(kind) => format!("atomic {kind:?}"),
        }
    }
}

impl TlmRamAdapter {
    /// Wraps one shared blocking target under the Machine's declared tick
    /// period (dev-plan D7).
    ///
    /// The target's declared per-transaction maximum must be representable in
    /// that period; an overflowing or malformed bound is a configuration
    /// failure before any access is issued.
    ///
    /// With direct memory selected the adapter registers its own invalidation
    /// handle with the target here, before its first transport reset, and keeps
    /// one dedicated direct-memory lock for the cached grants (direct-memory
    /// D8/D9).
    pub(crate) fn with_tick_period(
        target: Arc<dyn BlockingTarget>,
        period: TickPeriod,
        direct_mem: bool,
    ) -> Result<Self, DelayConversionError> {
        let declared_max = target.declared_max_delay();
        let bound = TurnCompletionBound::new(declared_max, period)?;
        let direct = if direct_mem {
            let regions = DirectMemRegions::new();
            target.register_direct_mem_invalidator(regions.invalidator());
            Some(regions)
        } else {
            None
        };
        Ok(Self {
            target,
            period,
            declared_max,
            bound,
            direct,
            state: Mutex::new(AdapterState::default()),
        })
    }

    /// Opens one guest turn's delay accounting.
    pub(crate) fn begin_turn(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.pending = PendingDelay::default();
            state.active = true;
        }
    }

    /// One guest turn's conservative completion bound in Platform ticks.
    pub(crate) fn turn_completion_bound_ticks(&self) -> u64 {
        self.bound.ticks()
    }

    /// Consumes the guest turn's accumulated delay into Platform time
    /// accounting and returns it once.
    ///
    /// The per-transaction and per-turn limits are enforced where the response
    /// arrives, so this step cannot fail and never discards known elapsed
    /// time: an overshooting or overflowing response fails the access that
    /// produced it, and the failed turn still consumes the delay accumulated
    /// before it (ADR-0004 §5.3).
    pub(crate) fn take_turn_delay(&self) -> PendingDelay {
        match self.state.lock() {
            Ok(mut state) => {
                state.active = false;
                std::mem::take(&mut state.pending)
            }
            Err(_) => PendingDelay::default(),
        }
    }

    /// Drops the delay validated since the last boundary without charging
    /// Platform time.
    ///
    /// SBA and Debug Mode RAM accesses (including program-buffer execution)
    /// are converted and validated the same way, but the 2026-10-28 and
    /// 2026-10-30 ADR-0004 amendments keep them out of Platform time.
    pub(crate) fn discard_turn_delay(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.pending = PendingDelay::default();
            state.active = false;
        }
    }

    /// Number of completed writes and AMOs recorded for committed-write
    /// visibility.  A direct store is one of them (direct-memory D7).
    #[cfg(test)]
    pub(crate) fn committed_write_version(&self) -> u64 {
        match self.state.lock() {
            Ok(state) => state.committed_writes,
            Err(_) => 0,
        }
    }

    /// Exact total charged guest-turn delay accepted so far.
    pub(crate) fn consumed_delay(&self) -> Femtoseconds {
        match self.state.lock() {
            Ok(state) => state.delay,
            Err(_) => Femtoseconds::ZERO,
        }
    }

    /// Why the transport refuses further transactions, if it does.
    #[cfg(test)]
    pub(crate) fn poison_reason(&self) -> Option<String> {
        match self.state.lock() {
            Ok(state) => state.poisoned.clone(),
            Err(_) => Some("TLM adapter state lock is poisoned".into()),
        }
    }

    /// Validates and accounts one completed response's annotated delay
    /// (ADR-0004 §5.2/§5.3, dev-plan D7).
    ///
    /// The caller records a committed write before this step.  An incomplete
    /// payload is not a completion and its annotation is not trusted as
    /// elapsed.
    fn account_response_delay(
        &self,
        state: &mut AdapterState,
        transaction: &BlockingTransaction<'_>,
        access: DelayAccess,
    ) -> Result<(), PhysicalBackendError> {
        self.account_delay(
            state,
            transaction.address(),
            transaction.response_delay(),
            transaction.status() != BlockingResponseStatus::Incomplete,
            access,
        )
    }

    /// Converts and accounts one access's transport delay.
    ///
    /// This is the one accounting path for both completions: a `b_transport`
    /// response that annotates a delay and a direct access that charges its
    /// grant's latency, so the conversion, the debug-not-charged rule, the
    /// boundary total, the overshoot rule and the turn bound are identical
    /// (direct-memory D6).
    ///
    /// The delay is accumulated before a limit is enforced, so an over-limit
    /// response still consumes its known time on the failed turn.
    fn account_delay(
        &self,
        state: &mut AdapterState,
        address: u64,
        delay: Femtoseconds,
        counted: bool,
        access: DelayAccess,
    ) -> Result<(), PhysicalBackendError> {
        let ticks = delay.to_platform_ticks(self.period).map_err(|error| {
            PhysicalBackendError::host(format!(
                "blocking target annotated a malformed delay of {} fs for {} at {address:#018x}: \
                 {error}",
                delay.as_u64(),
                access.label()
            ))
        })?;
        if counted && state.active {
            // Accumulate this response before classifying a limit failure, so
            // a known over-limit delay is still consumed once by the failed
            // turn.  An unrepresentable total fails the access without
            // dropping delay already known.
            let pending_ticks = state.pending.ticks.checked_add(ticks).ok_or_else(|| {
                PhysicalBackendError::host(format!(
                    "blocking-transport tick accounting overflowed while converting {} fs \
                     for {} at {address:#018x}",
                    delay.as_u64(),
                    access.label()
                ))
            })?;
            let pending_femtoseconds =
                state
                    .pending
                    .femtoseconds
                    .checked_add(delay)
                    .ok_or_else(|| {
                        PhysicalBackendError::host(format!(
                            "blocking-transport delay accounting overflowed after {} fs",
                            state.pending.femtoseconds.as_u64()
                        ))
                    })?;
            let total = state.delay.checked_add(delay).ok_or_else(|| {
                PhysicalBackendError::host(format!(
                    "total blocking-transport delay overflowed after {} fs",
                    state.delay.as_u64()
                ))
            })?;
            state.pending.ticks = pending_ticks;
            state.pending.femtoseconds = pending_femtoseconds;
            state.delay = total;
        }
        if delay > self.declared_max {
            let overshoot = Femtoseconds::new(delay.as_u64() - self.declared_max.as_u64());
            return Err(PhysicalBackendError::host(format!(
                "target reported {} fs of delay for one {} transaction at {address:#018x}, above \
                 its declared per-transaction maximum of {} fs (overshoot {} fs); the turn's \
                 conservative completion bound is {} ticks",
                delay.as_u64(),
                access.label(),
                self.declared_max.as_u64(),
                overshoot.as_u64(),
                self.bound.ticks(),
            )));
        }
        if state.active && state.pending.ticks > self.bound.ticks() {
            return Err(PhysicalBackendError::host(format!(
                "the turn consumed {} Platform ticks of transport delay, above its conservative \
                 completion bound of {} ticks ({} ticks per transaction x {} transactions); \
                 overshoot {} ticks",
                state.pending.ticks,
                self.bound.ticks(),
                self.bound.ticks_per_transaction(),
                self.bound.transactions(),
                state.pending.ticks - self.bound.ticks(),
            )));
        }
        Ok(())
    }

    /// Maps one validated physical request onto one completion.
    ///
    /// `initiator` distinguishes a Hart-issued access from the Debug Module's
    /// inbound system bus access (dev-plan D3).  A Hart-initiated read that a
    /// cached grant covers completes through the target's host pointer without
    /// a `b_transport` call; every other request takes one blocking transaction
    /// unchanged (direct-memory D4).
    pub(crate) fn transact(
        &self,
        request: &PhysicalRequest<'_>,
        initiator: BlockingInitiator,
    ) -> PhysicalBackendResult {
        request
            .validate()
            .map_err(|error| PhysicalBackendError::protocol(error.to_string()))?;
        if !request.span().is_non_wrapping() {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::RangeOverflow,
                format!(
                    "blocking span starting at {:#018x} with width {} wraps the address space",
                    request.paddr(),
                    request.width().bytes()
                ),
            ));
        }
        if let Some(result) = self.direct_transact(request, initiator) {
            return result;
        }
        self.blocking_transact(request, initiator)
    }

    /// The direct-memory read and write path (direct-memory D4–D7).
    ///
    /// Returns `None` when this request must take `b_transport`: direct memory
    /// is off, the access was not Hart-initiated, no single cached grant covers
    /// the whole span with the required permission, or a store arrives while
    /// the D7 write-suspension gate is open.  A span that crosses a grant's end
    /// therefore stays one transaction instead of being split.
    ///
    /// The adapter's state lock is taken first and the direct-memory lock
    /// second, and only for the copy itself: the direct-memory lock stays a
    /// leaf that is never held across a target call, so an invalidation may
    /// arrive from inside `b_transport`.
    fn direct_transact(
        &self,
        request: &PhysicalRequest<'_>,
        initiator: BlockingInitiator,
    ) -> Option<PhysicalBackendResult> {
        let regions = self.direct.as_deref()?;
        if !matches!(initiator, BlockingInitiator::Hart) {
            return None;
        }
        let descriptor = request.descriptor();
        let store = match descriptor.category {
            PhysicalAccessKind::Fetch | PhysicalAccessKind::DataRead => false,
            PhysicalAccessKind::DataWrite => true,
        };
        let width = descriptor.width.bytes();
        let mut state = self.state.lock().ok()?;
        // A poisoned transport refuses the direct path exactly as it refuses
        // `b_transport`, and the blocking path owns that diagnostic.
        if state.poisoned.is_some() {
            return None;
        }
        if store && state.writes_suspended {
            // direct-memory D7: a live guest reservation must observe every
            // store through the target's committed-write record, so the store
            // takes `b_transport` until a guest store-conditional completes.
            return None;
        }
        if store {
            let payload = request.write_payload()?;
            let latency = regions.write(descriptor.paddr, width, payload)?;
            // The bytes are already in the target's storage, so the write is
            // recorded before delay accounting can fail (ADR-0002 §6).
            state.committed_writes = state.committed_writes.saturating_add(1);
            Some(
                self.account_delay(
                    &mut state,
                    descriptor.paddr,
                    latency,
                    true,
                    DelayAccess::Physical(descriptor.category),
                )
                .map(|()| {
                    PhysicalResponse::new(
                        descriptor,
                        PhysicalResponseCompletion::WriteAcknowledgement,
                    )
                }),
            )
        } else {
            let mut bytes = [0u8; MAX_PHYSICAL_ACCESS_BYTES];
            let latency = regions.read(descriptor.paddr, width, &mut bytes[..width])?;
            Some(
                self.account_delay(
                    &mut state,
                    descriptor.paddr,
                    latency,
                    true,
                    DelayAccess::Physical(descriptor.category),
                )
                .map(|()| {
                    PhysicalResponse::new(
                        descriptor,
                        PhysicalResponseCompletion::Read(PhysicalResponseBytes::from_slice(
                            &bytes[..width],
                        )),
                    )
                }),
            )
        }
    }

    /// Maps one validated physical request onto one blocking transaction.
    fn blocking_transact(
        &self,
        request: &PhysicalRequest<'_>,
        initiator: BlockingInitiator,
    ) -> PhysicalBackendResult {
        let mut state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        if let Some(reason) = &state.poisoned {
            return Err(PhysicalBackendError::unknown(format!(
                "the TLM RAM transport is poisoned by an earlier unknown completion ({reason}); \
                 the target's transport reset must be acknowledged before any further access"
            )));
        }

        let descriptor = request.descriptor();
        let width = descriptor.width.bytes();
        let command = match descriptor.category {
            crate::physical::PhysicalAccessKind::Fetch
            | crate::physical::PhysicalAccessKind::DataRead => BlockingCommand::Read,
            crate::physical::PhysicalAccessKind::DataWrite => BlockingCommand::Write,
        };
        let mut bytes = [0u8; MAX_PHYSICAL_ACCESS_BYTES];
        if let Some(payload) = request.write_payload() {
            bytes[..width].copy_from_slice(payload);
        }
        let extension = BlockingExtension::new(descriptor.category, initiator);
        let mut transaction =
            BlockingTransaction::new(descriptor.paddr, command, &mut bytes[..width], extension);

        let panic = catch_unwind(AssertUnwindSafe(|| {
            self.target.b_transport(&mut transaction);
        }));
        if panic.is_err() {
            let reason = format!(
                "blocking target panicked while servicing {} {:?} at {:#018x}",
                command_label(command),
                descriptor.category,
                descriptor.paddr
            );
            self.poison(&mut state, reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.extension().is_outcome_unknown() {
            let reason = format!(
                "blocking target could not establish the outcome of {} {:?} at {:#018x}",
                command_label(command),
                descriptor.category,
                descriptor.paddr
            );
            self.poison(&mut state, reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.address() != descriptor.paddr
            || transaction.command() != command
            || transaction.data_length() != width
        {
            return Err(PhysicalBackendError::protocol(format!(
                "blocking target changed the request payload for {:?} at {:#018x}: \
                 address {:#018x}, command {}, length {} (expected address {:#018x}, \
                 command {}, length {width})",
                descriptor.category,
                descriptor.paddr,
                transaction.address(),
                command_label(transaction.command()),
                transaction.data_length(),
                descriptor.paddr,
                command_label(command),
            )));
        }

        let status = transaction.status();
        // An `Ok` write committed its effects before any delay accounting or
        // overshoot classification can fail; record it first so
        // committed-write visibility stays truthful even when the access ends
        // in a simulator failure (ADR-0002 §6).
        if status == BlockingResponseStatus::Ok && command == BlockingCommand::Write {
            state.committed_writes = state.committed_writes.saturating_add(1);
        }
        self.account_response_delay(
            &mut state,
            &transaction,
            DelayAccess::Physical(descriptor.category),
        )?;

        // A target may offer direct memory only on a completed non-atomic
        // transaction, and the adapter queries it once, for the access
        // direction of a Hart-initiated read or write, before the response
        // returns (direct-memory D3).  A store that took `b_transport` because
        // the D7 gate is open may still answer with a write or read-write
        // grant; the gate alone decides whether a store may use it.
        if status == BlockingResponseStatus::Ok
            && initiator == BlockingInitiator::Hart
            && transaction.is_dmi_allowed()
        {
            if let Some(regions) = self.direct.as_deref() {
                let access = match command {
                    BlockingCommand::Read => BlockingDirectMemAccess::Read,
                    BlockingCommand::Write => BlockingDirectMemAccess::Write,
                };
                self.query_grant(regions, &mut state, &descriptor, access)?;
            }
        }

        match status {
            BlockingResponseStatus::Ok => {
                if command == BlockingCommand::Write {
                    Ok(PhysicalResponse::new(
                        descriptor,
                        PhysicalResponseCompletion::WriteAcknowledgement,
                    ))
                } else {
                    Ok(PhysicalResponse::new(
                        descriptor,
                        PhysicalResponseCompletion::Read(PhysicalResponseBytes::from_slice(
                            transaction.data(),
                        )),
                    ))
                }
            }
            status if status.is_guest_visible_fault() => Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::TargetError,
                format!(
                    "target reported {} for {:?} at {:#018x}",
                    status.name(),
                    descriptor.category,
                    descriptor.paddr
                ),
            )),
            BlockingResponseStatus::GenericError => Err(PhysicalBackendError::host(format!(
                "target reported {} for {:?} at {:#018x}",
                BlockingResponseStatus::GenericError.name(),
                descriptor.category,
                descriptor.paddr
            ))),
            BlockingResponseStatus::Incomplete => Err(PhysicalBackendError::protocol(format!(
                "target left {} for {:?} at {:#018x}",
                BlockingResponseStatus::Incomplete.name(),
                descriptor.category,
                descriptor.paddr
            ))),
            _ => unreachable!("every blocking status is handled"),
        }
    }

    /// Marks the transport poisoned and drops every cached grant.
    ///
    /// An unknown completion means no proof excludes late effects, so no grant
    /// may survive it: the next access takes `b_transport`, which the poisoned
    /// transport refuses until a reset is acknowledged (direct-memory D9).
    fn poison(&self, state: &mut AdapterState, reason: String) {
        state.poisoned = Some(reason);
        if let Some(regions) = &self.direct {
            regions.clear();
        }
    }

    /// Queries the target once for one completed access and caches a valid
    /// grant (direct-memory D3/D5).
    ///
    /// A denial caches nothing and is not an error.  A malformed grant is a
    /// protocol failure of the access that queried it.  A panic in the query, or
    /// a target's contained failure, is an unknown completion of that access and
    /// poisons the transport.  No grant failure is guest-visible.
    fn query_grant(
        &self,
        regions: &DirectMemRegions,
        state: &mut AdapterState,
        descriptor: &PhysicalRequestDescriptor,
        access: BlockingDirectMemAccess,
    ) -> Result<(), PhysicalBackendError> {
        let query = BlockingDirectMemQuery::new(descriptor.paddr, descriptor.width.bytes(), access);
        // Sampled before the query begins: an invalidation that arrives while
        // the query is in flight discards the grant (direct-memory D8).
        let epoch = regions.epoch();
        let reply = catch_unwind(AssertUnwindSafe(|| self.target.get_direct_mem_ptr(query)));
        let reply = match reply {
            Ok(reply) => reply,
            Err(_) => {
                let reason = format!(
                    "blocking target panicked in its direct-memory query for a {} at {:#018x}",
                    access.name(),
                    descriptor.paddr
                );
                self.poison(state, reason.clone());
                return Err(PhysicalBackendError::unknown(reason));
            }
        };
        let grant = match reply {
            BlockingDirectMemResponse::Granted(grant) => grant,
            BlockingDirectMemResponse::Denied => return Ok(()),
            BlockingDirectMemResponse::Failed => {
                let reason = format!(
                    "blocking target could not complete its direct-memory query for a {} at \
                     {:#018x}",
                    access.name(),
                    descriptor.paddr
                );
                self.poison(state, reason.clone());
                return Err(PhysicalBackendError::unknown(reason));
            }
        };
        self.validate_grant(&grant, query)?;
        // A grant the target races with an invalidation is discarded rather
        // than installed (direct-memory D8).
        regions.install(grant, epoch);
        Ok(())
    }

    /// Rejects a grant whose shape a target cannot have promised
    /// (direct-memory D3/D10).
    ///
    /// The adapter checks only the shape.  The pointer's validity is the
    /// target's `unsafe` promise, which an invalidation must end.
    fn validate_grant(
        &self,
        grant: &BlockingDirectMemGrant,
        query: BlockingDirectMemQuery,
    ) -> Result<(), PhysicalBackendError> {
        let malformed = |detail: String| {
            PhysicalBackendError::protocol(format!(
                "the blocking target's direct-memory grant for the {} at {:#018x} is malformed: \
                 {detail}",
                query.access().name(),
                query.address()
            ))
        };
        if grant.start() > grant.end() {
            return Err(malformed(format!(
                "its inclusive range {:#018x}..={:#018x} is inverted",
                grant.start(),
                grant.end()
            )));
        }
        if grant
            .end()
            .checked_sub(grant.start())
            .and_then(|span| span.checked_add(1))
            .is_none()
        {
            return Err(malformed(
                "its inclusive range does not fit a representable length".into(),
            ));
        }
        if !grant.access().has_access() {
            return Err(malformed("it grants no access".into()));
        }
        if grant.pointer().is_null() {
            return Err(malformed("its pointer is null".into()));
        }
        if !(grant.start() <= query.address() && query.address() <= grant.end()) {
            return Err(malformed(format!(
                "it does not contain the queried address {:#018x}",
                query.address()
            )));
        }
        for (latency, direction) in [
            (grant.read_latency(), "read"),
            (grant.write_latency(), "write"),
        ] {
            if latency > self.declared_max {
                return Err(malformed(format!(
                    "its {direction} latency of {} fs is above the declared per-transaction \
                     maximum of {} fs",
                    latency.as_u64(),
                    self.declared_max.as_u64()
                )));
            }
        }
        Ok(())
    }

    /// Drops every cached grant (direct-memory D9).
    pub(crate) fn drop_regions(&self) {
        if let Some(regions) = &self.direct {
            regions.clear();
        }
    }

    /// Installs the test-only pause point inside a direct copy.
    #[cfg(test)]
    pub(crate) fn set_direct_copy_hook(&self, hook: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Some(regions) = &self.direct {
            regions.set_copy_hook(hook);
        }
    }

    /// True while a direct copy holds the direct-memory lock, for tests.
    #[cfg(test)]
    pub(crate) fn direct_memory_lock_held(&self) -> bool {
        self.direct
            .as_ref()
            .is_some_and(|regions| regions.lock_held())
    }

    /// Number of cached grants, for tests.
    #[cfg(test)]
    pub(crate) fn cached_regions(&self) -> usize {
        match &self.direct {
            Some(regions) => regions.len(),
            None => 0,
        }
    }

    /// Maps one validated atomic envelope onto one blocking transaction with
    /// the ruscv-sim atomic extension (dev-plan D5).
    ///
    /// One envelope is one `b_transport` call, whatever grants the adapter has
    /// cached: no load-reserved, store-conditional, AMO or translation A/D leg
    /// is ever served through a host pointer (direct-memory D7).  The target
    /// applies the encoded operation indivisibly and reports the old bytes,
    /// committed-write snapshot or conditional status in that same transaction.
    /// A target that does not accept the extension answers
    /// `TLM_COMMAND_ERROR_RESPONSE`, which maps to the native load or
    /// store/AMO access fault.  The Hart-owned transform function pointer
    /// never crosses the transport: only the encoded operation does.
    ///
    /// A guest load-reserved suspends direct stores until a guest
    /// store-conditional conclusively completes, and a translation A/D
    /// envelope, marked by the walker, changes neither direction of that gate
    /// (direct-memory D7).
    pub(crate) fn transact_atomic(&self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        request
            .validate()
            .map_err(|error| PhysicalBackendError::protocol(error.to_string()))?;
        if !request.span().is_non_wrapping() {
            let descriptor = request.descriptor();
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::RangeOverflow,
                format!(
                    "blocking atomic span starting at {:#018x} with width {} wraps the address space",
                    descriptor.paddr,
                    descriptor.width.bytes()
                ),
            ));
        }

        let descriptor = request.descriptor();
        let width =
            crate::physical::AmoWidth::from_physical_width(descriptor.width).ok_or_else(|| {
                PhysicalBackendError::protocol(format!(
                    "atomic envelope width {} is not a 4/8-byte width",
                    descriptor.width.bytes()
                ))
            })?;
        let atomic = match descriptor.kind {
            crate::physical::AtomicAccessKind::Rmw => {
                let operation = request
                    .transform()
                    .and_then(|transform| transform.operation());
                let operation = operation.ok_or_else(|| {
                    PhysicalBackendError::protocol(
                        "RMW envelope carries no encoded operation identity for the transport",
                    )
                })?;
                crate::blocking::BlockingAtomicExtension::rmw(operation, width, request.ordering())
            }
            crate::physical::AtomicAccessKind::LoadReserved => {
                crate::blocking::BlockingAtomicExtension::load_reserved(width, request.ordering())
            }
            crate::physical::AtomicAccessKind::StoreConditional => {
                crate::blocking::BlockingAtomicExtension::store_conditional(
                    width,
                    request.ordering(),
                    request.reservation(),
                )
            }
        };

        let mut state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        if let Some(reason) = &state.poisoned {
            return Err(PhysicalBackendError::unknown(format!(
                "the TLM RAM transport is poisoned by an earlier unknown completion ({reason}); \
                 the target's transport reset must be acknowledged before any further access"
            )));
        }
        let page_table_update = request.is_page_table_update();
        if !page_table_update
            && matches!(
                descriptor.kind,
                crate::physical::AtomicAccessKind::LoadReserved
            )
        {
            // direct-memory D7: this load-reserved may create the Hart's
            // reservation, whose later store-conditional check reads the
            // target's committed-write versions that a direct store never
            // updates, so every store takes `b_transport` from here until a
            // guest store-conditional completes.  A load-reserved that faults
            // or never completes therefore opens the gate too: a gate left open
            // with no store-conditional is correct and only slower, so the
            // conservative direction is the safe one.
            state.writes_suspended = true;
        }

        let command = match descriptor.kind {
            crate::physical::AtomicAccessKind::LoadReserved => BlockingCommand::Read,
            crate::physical::AtomicAccessKind::Rmw
            | crate::physical::AtomicAccessKind::StoreConditional => BlockingCommand::Write,
        };
        let access_kind = match command {
            BlockingCommand::Read => crate::physical::PhysicalAccessKind::DataRead,
            BlockingCommand::Write => crate::physical::PhysicalAccessKind::DataWrite,
        };
        let mut bytes = [0u8; MAX_PHYSICAL_ACCESS_BYTES];
        if let Some(payload) = request.operand_bytes().or_else(|| request.store_payload()) {
            bytes[..width.bytes()].copy_from_slice(payload);
        }
        let extension = BlockingExtension::atomic(access_kind, BlockingInitiator::Hart, atomic);
        let mut transaction = BlockingTransaction::new(
            descriptor.paddr,
            command,
            &mut bytes[..width.bytes()],
            extension,
        );

        let panic = catch_unwind(AssertUnwindSafe(|| {
            self.target.b_transport(&mut transaction);
        }));
        if panic.is_err() {
            let reason = format!(
                "blocking target panicked while servicing atomic {:?} at {:#018x}",
                descriptor.kind, descriptor.paddr
            );
            self.poison(&mut state, reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.extension().is_outcome_unknown() {
            let reason = format!(
                "blocking target could not establish the outcome of atomic {:?} at {:#018x}",
                descriptor.kind, descriptor.paddr
            );
            self.poison(&mut state, reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.address() != descriptor.paddr
            || transaction.command() != command
            || transaction.data_length() != width.bytes()
        {
            return Err(PhysicalBackendError::protocol(format!(
                "blocking target changed the atomic request payload for {:?} at {:#018x}: \
                 address {:#018x}, command {}, length {} (expected address {:#018x}, \
                 command {}, length {})",
                descriptor.kind,
                descriptor.paddr,
                transaction.address(),
                command_label(transaction.command()),
                transaction.data_length(),
                descriptor.paddr,
                command_label(command),
                width.bytes(),
            )));
        }

        let status = transaction.status();
        let outcome = transaction
            .atomic_extension()
            .and_then(|extension| extension.outcome());
        // A successful RMW or SC committed its effects before delay
        // accounting can fail, so record it first (ADR-0002 §6).
        if status == BlockingResponseStatus::Ok {
            match (descriptor.kind, outcome) {
                (
                    crate::physical::AtomicAccessKind::Rmw,
                    Some(crate::blocking::BlockingAtomicOutcome::Rmw),
                )
                | (
                    crate::physical::AtomicAccessKind::StoreConditional,
                    Some(crate::blocking::BlockingAtomicOutcome::StoreConditional(
                        crate::physical::ConditionalStatus::Success,
                    )),
                ) => {
                    state.committed_writes = state.committed_writes.saturating_add(1);
                }
                _ => {}
            }
        }
        // direct-memory D7: only a conclusively completed guest
        // store-conditional closes the gate, successful or failed.  A faulted
        // or unresolved one leaves it open, because the Hart keeps the
        // reservation that authored it, and a translation A/D leg never closes
        // it at all.
        //
        // A reported success is decisive only if the validated boundary will
        // accept it.  The boundary rejects a success that the Hart's reservation
        // context does not cover (physical.rs's
        // `store_conditional_success_rejection`), and that rejection is a
        // protocol failure of the envelope, so the Hart keeps its reservation
        // and the gate stays open.
        let conclusive = status == BlockingResponseStatus::Ok
            && match outcome {
                Some(crate::blocking::BlockingAtomicOutcome::StoreConditional(
                    crate::physical::ConditionalStatus::Success,
                )) => crate::physical::store_conditional_success_rejection(request).is_none(),
                Some(crate::blocking::BlockingAtomicOutcome::StoreConditional(
                    crate::physical::ConditionalStatus::Failure,
                )) => true,
                _ => false,
            };
        if !page_table_update
            && matches!(
                descriptor.kind,
                crate::physical::AtomicAccessKind::StoreConditional
            )
            && conclusive
        {
            state.writes_suspended = false;
        }
        self.account_response_delay(
            &mut state,
            &transaction,
            DelayAccess::Atomic(descriptor.kind),
        )?;

        match status {
            BlockingResponseStatus::Ok => match (descriptor.kind, outcome) {
                (
                    crate::physical::AtomicAccessKind::Rmw,
                    Some(crate::blocking::BlockingAtomicOutcome::Rmw),
                ) => Ok(AtomicResponse::rmw_for(request, transaction.data())),
                (
                    crate::physical::AtomicAccessKind::LoadReserved,
                    Some(crate::blocking::BlockingAtomicOutcome::LoadReserved { snapshot }),
                ) => Ok(AtomicResponse::load_reserved_for(
                    request,
                    transaction.data(),
                    snapshot,
                )),
                (
                    crate::physical::AtomicAccessKind::StoreConditional,
                    Some(crate::blocking::BlockingAtomicOutcome::StoreConditional(status)),
                ) => Ok(AtomicResponse::store_conditional_for(request, status)),
                (_, Some(outcome)) => Err(PhysicalBackendError::protocol(format!(
                    "target returned an atomic completion of kind {:?} for a {:?} request",
                    outcome.kind(),
                    descriptor.kind
                ))),
                (_, None) => Err(PhysicalBackendError::protocol(format!(
                    "target returned {} for atomic {:?} at {:#018x} without an atomic \
                         completion; it did not accept the atomic extension",
                    BlockingResponseStatus::Ok.name(),
                    descriptor.kind,
                    descriptor.paddr
                ))),
            },
            status if status.is_guest_visible_fault() => Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::TargetError,
                format!(
                    "target reported {} for atomic {:?} at {:#018x}",
                    status.name(),
                    descriptor.kind,
                    descriptor.paddr
                ),
            )),
            BlockingResponseStatus::GenericError => Err(PhysicalBackendError::host(format!(
                "target reported {} for atomic {:?} at {:#018x}",
                BlockingResponseStatus::GenericError.name(),
                descriptor.kind,
                descriptor.paddr
            ))),
            BlockingResponseStatus::Incomplete => Err(PhysicalBackendError::protocol(format!(
                "target left {} for atomic {:?} at {:#018x}",
                BlockingResponseStatus::Incomplete.name(),
                descriptor.kind,
                descriptor.paddr
            ))),
            _ => unreachable!("every blocking status is handled"),
        }
    }

    /// Reads bytes through the target's debug transport (dev-plan D6).
    pub(crate) fn debug_read(
        &self,
        address: u64,
        len: usize,
    ) -> Result<Vec<u8>, PhysicalBackendError> {
        let mut bytes = vec![0; len];
        self.debug_transfer(address, BlockingCommand::Read, &mut bytes)?;
        Ok(bytes)
    }

    /// Writes bytes through the target's debug transport (dev-plan D6).
    pub(crate) fn debug_write(
        &self,
        address: u64,
        bytes: &[u8],
    ) -> Result<(), PhysicalBackendError> {
        let mut local = bytes.to_vec();
        self.debug_transfer(address, BlockingCommand::Write, &mut local)
    }

    fn debug_transfer(
        &self,
        address: u64,
        command: BlockingCommand,
        bytes: &mut [u8],
    ) -> Result<(), PhysicalBackendError> {
        let mut state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        if let Some(reason) = &state.poisoned {
            return Err(PhysicalBackendError::unknown(format!(
                "the TLM RAM transport is poisoned by an earlier unknown completion ({reason}); \
                 the target's transport reset must be acknowledged before any further access"
            )));
        }
        let len = bytes.len();
        let mut transaction = BlockingDebug::new(address, command, bytes);
        let transferred = catch_unwind(AssertUnwindSafe(|| {
            self.target.transport_dbg(&mut transaction)
        }));
        let transferred = transferred.map_err(|_| {
            let reason = format!(
                "blocking target panicked in its debug transport for {} at {address:#018x}",
                command_label(command)
            );
            self.poison(&mut state, reason.clone());
            PhysicalBackendError::unknown(reason)
        })?;
        if transferred != len {
            return Err(PhysicalBackendError::host(format!(
                "target debug transport transferred {transferred} of {len} bytes for {} at \
                 {address:#018x}",
                command_label(command)
            )));
        }
        Ok(())
    }

    /// Performs the target's transport reset and clears any poison only when
    /// the target acknowledges it.
    ///
    /// The adapter drops its own grants and closes its write-suspension gate
    /// before it calls the target, as the Platform's reset entry point requires
    /// (direct-memory D7/D9).  That covers this composition; the target must
    /// still invalidate every registration before it frees storage, which
    /// covers a composition that is being replaced and stays current until
    /// publication.
    pub(crate) fn transport_reset(&self) -> Result<(), PhysicalBackendError> {
        self.drop_regions();
        let mut state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        state.writes_suspended = false;
        let acknowledged = catch_unwind(AssertUnwindSafe(|| self.target.transport_reset()));
        match acknowledged {
            Ok(true) => {
                state.poisoned = None;
                Ok(())
            }
            Ok(false) => {
                let reason = "blocking target did not acknowledge the transport reset".to_string();
                self.poison(&mut state, reason.clone());
                Err(PhysicalBackendError::host(reason))
            }
            Err(_) => {
                let reason = "blocking target panicked during its transport reset".to_string();
                self.poison(&mut state, reason.clone());
                Err(PhysicalBackendError::unknown(reason))
            }
        }
    }
}

const fn command_label(command: BlockingCommand) -> &'static str {
    match command {
        BlockingCommand::Read => "read",
        BlockingCommand::Write => "write",
    }
}
