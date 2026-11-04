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
    BlockingTarget, BlockingTransaction, Femtoseconds,
};
use crate::physical::{
    AtomicBackendResult, AtomicRequest, AtomicResponse, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalResponse, PhysicalResponseBytes,
    PhysicalResponseCompletion, PhysicalTargetRejectionReason, MAX_PHYSICAL_ACCESS_BYTES,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

/// One selectable TLM RAM route over a shared blocking target.
pub(crate) struct TlmRamAdapter {
    target: Arc<dyn BlockingTarget>,
    state: Mutex<AdapterState>,
}

#[derive(Debug, Default)]
struct AdapterState {
    /// Why the transport can no longer issue a transaction, if it cannot.
    poisoned: Option<String>,
    /// Monotonic count of completed writes and AMOs.
    committed_writes: u64,
    /// Exact total of validated annotated delay, zero until unit U3.
    delay: Femtoseconds,
}

impl TlmRamAdapter {
    /// Wraps one shared blocking target.
    pub(crate) fn new(target: Arc<dyn BlockingTarget>) -> Self {
        Self {
            target,
            state: Mutex::new(AdapterState::default()),
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

    /// Exact total annotated delay accepted so far.
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

        // Unit U3 converts annotated delays.  Until then a non-zero annotation
        // is an adapter failure, never a silently dropped zero.
        let delay = transaction.response_delay();
        if !delay.is_zero() {
            return Err(PhysicalBackendError::host(format!(
                "blocking target annotated {} fs of delay for {:?} at {:#018x}; \
                 delay conversion is not implemented until unit U3",
                delay.as_u64(),
                descriptor.category,
                descriptor.paddr
            )));
        }
        state.delay = state.delay.checked_add(delay).ok_or_else(|| {
            PhysicalBackendError::host(format!(
                "total blocking-transport delay overflowed at {:#018x}",
                descriptor.paddr
            ))
        })?;

        match transaction.status() {
            BlockingResponseStatus::Ok => {
                if command == BlockingCommand::Write {
                    state.committed_writes = state.committed_writes.saturating_add(1);
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

    /// Maps one validated atomic envelope onto one blocking transaction with
    /// the ruscv-sim atomic extension (dev-plan D5).
    ///
    /// One envelope is one `b_transport` call.  The target applies the
    /// encoded operation indivisibly and reports the old bytes, committed-write
    /// snapshot or conditional status in that same transaction.  A target
    /// that does not accept the extension answers
    /// `TLM_COMMAND_ERROR_RESPONSE`, which maps to the native load or
    /// store/AMO access fault.  The Hart-owned transform function pointer
    /// never crosses the transport: only the encoded operation does.
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
                if let Some(context) = request.reservation() {
                    let expected = context.reserved.width.bytes() * 8;
                    if context.snapshot.len() != expected {
                        return Err(PhysicalBackendError::protocol(format!(
                            "reservation snapshot has {} bytes, expected {expected} for the {} byte reserved span",
                            context.snapshot.len(),
                            context.reserved.width.bytes()
                        )));
                    }
                }
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
            state.poisoned = Some(reason.clone());
            return Err(PhysicalBackendError::unknown(reason));
        }

        if transaction.extension().is_outcome_unknown() {
            let reason = format!(
                "blocking target could not establish the outcome of atomic {:?} at {:#018x}",
                descriptor.kind, descriptor.paddr
            );
            state.poisoned = Some(reason.clone());
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

        // Unit U3 converts annotated delays.  Until then a non-zero annotation
        // is an adapter failure, never a silently dropped zero.
        let delay = transaction.response_delay();
        if !delay.is_zero() {
            return Err(PhysicalBackendError::host(format!(
                "blocking target annotated {} fs of delay for atomic {:?} at {:#018x}; \
                 delay conversion is not implemented until unit U3",
                delay.as_u64(),
                descriptor.kind,
                descriptor.paddr
            )));
        }
        state.delay = state.delay.checked_add(delay).ok_or_else(|| {
            PhysicalBackendError::host(format!(
                "total blocking-transport delay overflowed at {:#018x}",
                descriptor.paddr
            ))
        })?;

        match transaction.status() {
            BlockingResponseStatus::Ok => {
                let outcome = transaction
                    .atomic_extension()
                    .and_then(|extension| extension.outcome());
                match (descriptor.kind, outcome) {
                    (
                        crate::physical::AtomicAccessKind::Rmw,
                        Some(crate::blocking::BlockingAtomicOutcome::Rmw),
                    ) => {
                        let response = AtomicResponse::rmw_for(request, transaction.data());
                        state.committed_writes = state.committed_writes.saturating_add(1);
                        Ok(response)
                    }
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
                    ) => {
                        if status == crate::physical::ConditionalStatus::Success {
                            state.committed_writes = state.committed_writes.saturating_add(1);
                        }
                        Ok(AtomicResponse::store_conditional_for(request, status))
                    }
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
                }
            }
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
