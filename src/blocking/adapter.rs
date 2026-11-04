//! The ruscv-sim adapter that maps one physical access onto one blocking
//! transaction (dev-plan D3/D4/D6).
//!
//! The adapter owns every classification rule:
//!
//! * one [`PhysicalRequest`] becomes exactly one `b_transport` call, never a
//!   split, merge, cache or retry;
//! * a changed payload or a left-incomplete status is a protocol failure;
//! * the four fault statuses are guest-visible target faults, generic error is
//!   a host failure, and an unknown flag, panic or non-zero annotation is
//!   never silently accepted;
//! * an unknown completion poisons the transport, so no further access is
//!   issued until [`TlmRamAdapter::transport_reset`] obtains the target's
//!   acknowledgement;
//! * every completed write and AMO is recorded before the response returns
//!   (ADR-0002 §6 committed-write visibility).

use super::{
    BlockingCommand, BlockingDebug, BlockingExtension, BlockingInitiator, BlockingResponseStatus,
    BlockingTarget, BlockingTransaction, DelayConversionError, Femtoseconds, TickPeriod,
    TurnCompletionBound,
};
use crate::physical::{
    AtomicBackendResult, AtomicRequest, PhysicalBackendError, PhysicalBackendResult,
    PhysicalRequest, PhysicalResponse, PhysicalResponseBytes, PhysicalResponseCompletion,
    PhysicalTargetRejectionReason, MAX_PHYSICAL_ACCESS_BYTES,
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
    state: Mutex<AdapterState>,
}

#[derive(Debug, Default)]
struct AdapterState {
    /// Why the transport can no longer issue a transaction, if it cannot.
    poisoned: Option<String>,
    /// Monotonic count of completed writes and AMOs.
    committed_writes: u64,
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

impl TlmRamAdapter {
    /// Wraps one shared blocking target under the declared default tick
    /// period.
    pub(crate) fn new(target: Arc<dyn BlockingTarget>) -> Result<Self, DelayConversionError> {
        Self::with_tick_period(target, TickPeriod::DEFAULT)
    }

    /// Wraps one shared blocking target under an explicit tick period.
    ///
    /// The target's declared per-transaction maximum must be representable in
    /// that period; an overflowing or malformed bound is a configuration
    /// failure before any access is issued.
    pub(crate) fn with_tick_period(
        target: Arc<dyn BlockingTarget>,
        period: TickPeriod,
    ) -> Result<Self, DelayConversionError> {
        let declared_max = target.declared_max_delay();
        let bound = TurnCompletionBound::new(declared_max, period)?;
        Ok(Self {
            target,
            period,
            declared_max,
            bound,
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
    #[cfg(test)]
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
    /// visibility.
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

    /// Maps one validated physical request onto one blocking transaction.
    ///
    /// `initiator` distinguishes a Hart-issued access from the Debug Module's
    /// inbound system bus access (dev-plan D3).
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
            state.poisoned = Some(reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.extension().is_outcome_unknown() {
            let reason = format!(
                "blocking target could not establish the outcome of {} {:?} at {:#018x}",
                command_label(command),
                descriptor.category,
                descriptor.paddr
            );
            state.poisoned = Some(reason.clone());
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

        // Convert and account the annotated delay before classifying the
        // response, so a completed fault or host failure keeps the modeled
        // elapsed time exactly once (ADR-0004 §5.3).  An incomplete or
        // target-modified payload is not a completion, so its annotation is
        // not trusted as elapsed.
        let delay = transaction.response_delay();
        let ticks = delay.to_platform_ticks(self.period).map_err(|error| {
            PhysicalBackendError::host(format!(
                "blocking target annotated a malformed delay of {} fs for {:?} at {:#018x}: \
                 {error}",
                delay.as_u64(),
                descriptor.category,
                descriptor.paddr
            ))
        })?;
        let status = transaction.status();
        if status != BlockingResponseStatus::Incomplete && state.active {
            // Accumulate this response before classifying a limit failure, so
            // a known over-limit delay is still consumed once by the failed
            // turn.  An unrepresentable total fails the access without
            // dropping delay already known.
            let pending_ticks = state.pending.ticks.checked_add(ticks).ok_or_else(|| {
                PhysicalBackendError::host(format!(
                    "blocking-transport tick accounting overflowed while converting {} fs \
                     for {:?} at {:#018x}",
                    delay.as_u64(),
                    descriptor.category,
                    descriptor.paddr
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
        // An `Ok` write committed its effects before an overshoot is detected;
        // record it so committed-write visibility stays truthful even when the
        // turn ends in a simulator failure.
        if status == BlockingResponseStatus::Ok && command == BlockingCommand::Write {
            state.committed_writes = state.committed_writes.saturating_add(1);
        }
        let declared_max = self.declared_max;
        if delay > declared_max {
            let overshoot = Femtoseconds::new(delay.as_u64() - declared_max.as_u64());
            return Err(PhysicalBackendError::host(format!(
                "target reported {} fs of delay for one {:?} transaction at {:#018x}, above its \
                 declared per-transaction maximum of {} fs (overshoot {} fs); the turn's \
                 conservative completion bound is {} ticks",
                delay.as_u64(),
                descriptor.category,
                descriptor.paddr,
                declared_max.as_u64(),
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

    /// Refuses an atomic envelope without issuing a transaction (unit U2).
    ///
    /// LR, SC and every AMO fail closed with the native access-fault cause
    /// until the atomic extension lands; read-then-write emulation is
    /// forbidden, so no transaction reaches the target.
    pub(crate) fn transact_atomic(&self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        request
            .validate()
            .map_err(|error| PhysicalBackendError::protocol(error.to_string()))?;
        let state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        if let Some(reason) = &state.poisoned {
            return Err(PhysicalBackendError::unknown(format!(
                "the TLM RAM transport is poisoned by an earlier unknown completion ({reason}); \
                 the target's transport reset must be acknowledged before any further access"
            )));
        }
        Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::UnsupportedCategory,
            "the TLM RAM route does not carry atomics until unit U2; \
             the request is refused before any transaction",
        ))
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
            state.poisoned = Some(reason.clone());
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
    pub(crate) fn transport_reset(&self) -> Result<(), PhysicalBackendError> {
        let mut state = self.state.lock().map_err(|_| {
            PhysicalBackendError::host("TLM adapter state lock is poisoned by an earlier panic")
        })?;
        let acknowledged = catch_unwind(AssertUnwindSafe(|| self.target.transport_reset()));
        match acknowledged {
            Ok(true) => {
                state.poisoned = None;
                Ok(())
            }
            Ok(false) => {
                let reason = "blocking target did not acknowledge the transport reset".to_string();
                state.poisoned = Some(reason.clone());
                Err(PhysicalBackendError::host(reason))
            }
            Err(_) => {
                let reason = "blocking target panicked during its transport reset".to_string();
                state.poisoned = Some(reason.clone());
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
