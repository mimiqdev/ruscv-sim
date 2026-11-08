//! Blocking device targets for the UART and HTIF windows (device D1–D5,
//! htif D1–D4).
//!
//! A device window has one selected [`BlockingDeviceTarget`] per Platform
//! generation.  One validated register access becomes one
//! [`BlockingTransaction`] carrying the existing physical category and
//! initiator; the Platform answers the window's width, category and
//! start-address policy before the target sees it, so a target never observes
//! a request the Platform would reject.
//!
//! A device transaction is not a RAM transaction: its `may_suspend` is always
//! false, its local-time offset is zero, and its response must annotate zero
//! host elapsed time, no direct-memory hint and at most one
//! [`BlockingDeviceEvent`].  A Hart access may annotate a modeled delay, which
//! the Machine charges once to the turn through the guest-RAM slot's one
//! ledger; a `DebugInbound` access must still report zero (delay D4/D6).  The
//! event belongs to the target's own window, is published only for an `Ok`
//! status, and is a Platform event under ADR-0004 §9.  A response that violates
//! any of those rules, that changes the payload, or that arrives after a target
//! panic is a protocol failure of that access; an outcome-unknown flag keeps
//! its precedence and is an unknown completion.
//!
//! The atomic path never reaches a target: the HTIF read-modify-write envelope
//! and every atomic rejection stay Platform-side (device D3).

use super::{
    BlockingCommand, BlockingExtension, BlockingInitiator, BlockingResponseStatus,
    BlockingTransaction, Femtoseconds,
};
use crate::physical::{
    PhysicalAccessKind, PhysicalBackendError, PhysicalBackendResult, PhysicalRequest,
    PhysicalRequestDescriptor, PhysicalResponse, PhysicalResponseBytes, PhysicalResponseCompletion,
    PhysicalTargetRejectionReason, MAX_PHYSICAL_ACCESS_BYTES,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

/// The platform window a device target answers (device D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DeviceWindow {
    /// The UART register window (`0x1000_0000`, `0x100` bytes).
    Uart,
    /// The HTIF tohost endpoint (`0x4000_8000`, 8 bytes).
    Htif,
}

impl DeviceWindow {
    /// The window's name, for diagnostics.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Uart => "UART",
            Self::Htif => "HTIF",
        }
    }
}

/// The at-most-one guest-visible effect of an accepted device access
/// (device D4).
///
/// A UART transmit byte and an HTIF write value are the two effects a device
/// window reports today.  The value travels with the event, so a host target
/// needs no second payload field (device D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockingDeviceEvent {
    /// One byte left the UART's transmit holding register.
    UartTransmit(u8),
    /// One value was written to the HTIF endpoint.
    HtifWrite(u64),
}

impl BlockingDeviceEvent {
    /// The window that owns this event.
    pub(crate) const fn window(self) -> DeviceWindow {
        match self {
            Self::UartTransmit(_) => DeviceWindow::Uart,
            Self::HtifWrite(_) => DeviceWindow::Htif,
        }
    }

    /// The event's name, for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::UartTransmit(_) => "uart-transmit",
            Self::HtifWrite(_) => "htif-write",
        }
    }
}

/// One window's blocking device target (device D2).
///
/// All methods take `&self`: a [`crate::machine::MachineConfig`] holds the
/// target as a shared `Arc`, and a target's own synchronization must not be
/// observable through this interface.  Calls are synchronous, return before
/// the ABI entry that made them returns, are never re-entered by ruscv-sim and
/// never wait: a device target may not let host time pass, so
/// [`BlockingTransaction::set_host_elapsed`] and
/// [`BlockingTransaction::set_dmi_allowed`] are protocol failures when used.
/// [`BlockingTransaction::set_response_delay`] is the one annotation a device
/// target may make for a Hart access: the Machine converts and charges it once
/// through the turn's existing delay ledger, and it is a protocol failure on a
/// `DebugInbound` access, which reports zero (delay D4/D6).
pub trait BlockingDeviceTarget: Send + Sync {
    /// Services one complete blocking device transaction.
    ///
    /// The target must leave the payload's address, command and length
    /// unchanged, select exactly one [`BlockingResponseStatus`] (a target that
    /// forgets leaves `Incomplete`, which is a protocol failure), and write the
    /// response bytes for a read into [`BlockingTransaction::data_mut`].  It
    /// reports its window's one guest-visible effect, if the access has one,
    /// through [`BlockingTransaction::report_device_event`].  An atomic
    /// envelope never reaches a device target (device D3).
    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>);

    /// Delivers one byte the host pushes into the window (device D5).
    ///
    /// Only the UART window receives bytes; an in-tree HTIF target ignores
    /// them.  A target that cannot accept the byte drops it.
    fn receive(&self, byte: u8);

    /// Acknowledges a transport reset (device D6).
    ///
    /// `true` means the target has quiesced and accepts new accesses; `false`
    /// refuses and leaves the current association intact.  A fresh in-tree
    /// target is built for every install, which is its whole reset rule.
    fn transport_reset(&self) -> bool;
}

/// The Machine's device-target selection (device D2, htif D2).
///
/// The selection names a host-supplied target per window.  No selection, the
/// [`Self::none`] default, means the in-tree target answers each window, so a
/// run without a host device target is today's run on either route.  Each
/// window is selected independently: a host UART target composes with the
/// in-tree HTIF endpoint and the reverse.
///
/// [`crate::machine::MachineConfig::devices`] carries the selection, which
/// requires a [`super::Transport::Tlm`] route on
/// [`crate::machine::PlatformKind::Native`];
/// [`crate::machine::MachineError::UnsupportedTransport`] reports any other
/// combination.
#[derive(Clone, Default)]
pub struct DeviceTargets {
    uart: Option<Arc<dyn BlockingDeviceTarget>>,
    htif: Option<Arc<dyn BlockingDeviceTarget>>,
}

impl DeviceTargets {
    /// No host target: every window is answered by its in-tree target.
    pub const fn none() -> Self {
        Self {
            uart: None,
            htif: None,
        }
    }

    /// Selects a host target for the UART window (device D2).
    ///
    /// The target owns the window's register state in place of the in-tree
    /// `Uart16550`, is reset at each Platform build (device D6), and answers
    /// the same pre-target policy the native branch applies: byte-only width,
    /// no fetches and the `0x100` window.  A host-served window has no Rust
    /// state to view, so `uart_inspection` returns `None`, `receive_uart`
    /// reaches [`BlockingDeviceTarget::receive`], and a typed
    /// `MemoryInterface` access to the window is
    /// [`crate::memory::MemoryError::InvalidAddress`] (device D5).
    pub fn with_uart(target: Arc<dyn BlockingDeviceTarget>) -> Self {
        Self {
            uart: Some(target),
            htif: None,
        }
    }

    /// Selects a host target for the HTIF window (htif D2).
    ///
    /// The target owns the endpoint's state in place of the in-tree `InTreeHtif`
    /// and is reset at each Platform build (htif D6).  It answers the same two
    /// requests the in-tree target answers, under the same pre-target policy the
    /// Platform applies: one complete eight-byte dword starting at the window
    /// base, no fetch, and a read-modify-write envelope that never reaches a
    /// target.  The typed `MemoryInterface` view is unchanged on this window, so
    /// a typed dword read still answers zero and a typed dword write still
    /// publishes without reaching the target (htif D3).
    pub fn with_htif(target: Arc<dyn BlockingDeviceTarget>) -> Self {
        Self {
            uart: None,
            htif: Some(target),
        }
    }

    /// Selects host targets for both device windows (htif D2).
    ///
    /// The two windows stay independent; this is the selection that names a
    /// host target for each of them at once.
    pub fn with_uart_and_htif(
        uart: Arc<dyn BlockingDeviceTarget>,
        htif: Arc<dyn BlockingDeviceTarget>,
    ) -> Self {
        Self {
            uart: Some(uart),
            htif: Some(htif),
        }
    }

    /// The selected host UART target, when the selection names one.
    pub fn uart(&self) -> Option<&Arc<dyn BlockingDeviceTarget>> {
        self.uart.as_ref()
    }

    /// The selected host HTIF target, when the selection names one.
    pub fn htif(&self) -> Option<&Arc<dyn BlockingDeviceTarget>> {
        self.htif.as_ref()
    }
}

impl std::fmt::Debug for DeviceTargets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceTargets")
            .field("uart", &self.uart.is_some())
            .field("htif", &self.htif.is_some())
            .finish()
    }
}

/// One device window's adapter over its selected target (device D2/D4).
pub(crate) struct TlmDeviceAdapter {
    window: DeviceWindow,
    target: Arc<dyn BlockingDeviceTarget>,
}

/// One device access's mapped result beside its at-most-one event.
pub(crate) struct DeviceOutcome {
    /// The access's Platform result, in the existing ADR-0002 §4 categories.
    pub(crate) response: PhysicalBackendResult,
    /// The event to publish through this window's sink, only for `Ok`.
    pub(crate) event: Option<BlockingDeviceEvent>,
    /// delay D4: the delay this response is charged, in exact femtoseconds.
    ///
    /// It is the validated response's own annotation on a terminal status, and
    /// zero for an `Incomplete` response, a validation failure and an unknown
    /// completion, so the Machine charges exactly the responses that vouch for
    /// the access.
    pub(crate) delay: Femtoseconds,
}

impl TlmDeviceAdapter {
    /// Wraps one window's selected target.
    pub(crate) fn new(window: DeviceWindow, target: Arc<dyn BlockingDeviceTarget>) -> Self {
        Self { window, target }
    }

    /// Maps one validated device-window request onto one completion.
    ///
    /// The Platform has already applied the window's width, category and
    /// start-address policy, so this call validates the physical request's own
    /// shape, makes exactly one `b_transport` call, validates the response and
    /// maps its status with the RAM route's categories.  No delay is converted
    /// or charged here and no direct-memory query is issued: the outcome
    /// carries the response's validated annotation, which the Machine charges
    /// through the guest-RAM slot's one ledger (delay D4).
    pub(crate) fn transact(
        &self,
        request: &PhysicalRequest<'_>,
        initiator: BlockingInitiator,
    ) -> DeviceOutcome {
        if let Err(error) = request.validate() {
            return failed(PhysicalBackendError::protocol(error.to_string()));
        }
        let descriptor = request.descriptor();
        if !request.span().is_non_wrapping() {
            return failed(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::RangeOverflow,
                format!(
                    "device span starting at {:#018x} with width {} wraps the address space",
                    descriptor.paddr,
                    descriptor.width.bytes()
                ),
            ));
        }
        let width = descriptor.width.bytes();
        let command = match descriptor.category {
            PhysicalAccessKind::Fetch | PhysicalAccessKind::DataRead => BlockingCommand::Read,
            PhysicalAccessKind::DataWrite => BlockingCommand::Write,
        };
        let mut bytes = [0u8; MAX_PHYSICAL_ACCESS_BYTES];
        if let Some(payload) = request.write_payload() {
            bytes[..width].copy_from_slice(payload);
        }
        // htif D10 rule (2): a tohost write must report the request's eight
        // data bytes.  The value is captured before the target runs, because a
        // target may rewrite its buffer through `data_mut` and the rule
        // compares the reported value with the request, not with the buffer the
        // target leaves behind.
        let requested_value = self.requested_event_value(command, &bytes[..width]);
        let extension = BlockingExtension::new(descriptor.category, initiator);
        let mut transaction =
            BlockingTransaction::new(descriptor.paddr, command, &mut bytes[..width], extension);

        // device D4 step 1: a target panic cannot vouch for the access, so it
        // is an unknown completion before every response rule, exactly as the
        // RAM adapter classifies its own panic.
        let panic = catch_unwind(AssertUnwindSafe(|| {
            self.target.b_transport(&mut transaction);
        }));
        if panic.is_err() {
            return failed(PhysicalBackendError::unknown(format!(
                "device target panicked while servicing {} {:?} at {:#018x} on the {} window",
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name()
            )));
        }

        if let Err(error) = self.validate_response(
            &transaction,
            descriptor,
            command,
            width,
            requested_value,
            initiator,
        ) {
            return failed(error);
        }

        let status = transaction.status();
        let event = transaction.device_events().first().copied();
        // delay D4: only a terminal status that passed validation is charged
        // its annotation.  An `Incomplete` response's annotation is untrusted,
        // and no completion was ever vouched for.
        let delay = if status == BlockingResponseStatus::Incomplete {
            Femtoseconds::ZERO
        } else {
            transaction.response_delay()
        };
        let response = match status {
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
                    "device target reported {} for {:?} at {:#018x} on the {} window",
                    status.name(),
                    descriptor.category,
                    descriptor.paddr,
                    self.window.name()
                ),
            )),
            BlockingResponseStatus::GenericError => Err(PhysicalBackendError::host(format!(
                "device target reported {} for {:?} at {:#018x} on the {} window",
                BlockingResponseStatus::GenericError.name(),
                descriptor.category,
                descriptor.paddr,
                self.window.name()
            ))),
            BlockingResponseStatus::Incomplete => Err(PhysicalBackendError::protocol(format!(
                "device target left {} for {:?} at {:#018x} on the {} window",
                BlockingResponseStatus::Incomplete.name(),
                descriptor.category,
                descriptor.paddr,
                self.window.name()
            ))),
            _ => unreachable!("every remaining status is a guest-visible fault"),
        };
        DeviceOutcome {
            response,
            event,
            delay,
        }
    }

    /// Validates one device response against device D2/D4, htif D10, the
    /// 2026-11-07 and 2026-11-08 ADR-0002 amendments and delay D4/D6.
    ///
    /// Every rule runs before the status mapping; an event that passes them
    /// never changes the outcome the status selects.  `requested_value` is the
    /// value this request carries for its window's event, captured before the
    /// target could rewrite the buffer.
    fn validate_response(
        &self,
        transaction: &BlockingTransaction<'_>,
        descriptor: PhysicalRequestDescriptor,
        command: BlockingCommand,
        width: usize,
        requested_value: Option<u64>,
        initiator: BlockingInitiator,
    ) -> Result<(), PhysicalBackendError> {
        if transaction.extension().is_outcome_unknown() {
            return Err(PhysicalBackendError::unknown(format!(
                "device target could not establish the outcome of {} {:?} at {:#018x} on the {} \
                 window",
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name()
            )));
        }
        if transaction.address() != descriptor.paddr
            || transaction.command() != command
            || transaction.data_length() != width
        {
            return Err(PhysicalBackendError::protocol(format!(
                "device target changed the request payload for {} {:?} at {:#018x} on the {} \
                 window: address {:#018x}, command {}, length {} (expected address {:#018x}, \
                 command {}, length {})",
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name(),
                transaction.address(),
                command_label(transaction.command()),
                transaction.data_length(),
                descriptor.paddr,
                command_label(command),
                width,
            )));
        }
        if initiator != BlockingInitiator::Hart && !transaction.response_delay().is_zero() {
            return Err(PhysicalBackendError::protocol(format!(
                "device target annotated {} fs of delay for {} {:?} at {:#018x} on the {} window; \
                 a DebugInbound device transaction reports zero delay",
                transaction.response_delay().as_u64(),
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name(),
            )));
        }
        if !transaction.host_elapsed().is_zero() {
            return Err(PhysicalBackendError::protocol(format!(
                "device target reported {} fs of host elapsed time for {} {:?} at {:#018x} on the \
                 {} window; host time may not pass inside a device callback",
                transaction.host_elapsed().as_u64(),
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name(),
            )));
        }
        if transaction.is_dmi_allowed() {
            return Err(PhysicalBackendError::protocol(format!(
                "device target set the direct-memory hint for {} {:?} at {:#018x} on the {} \
                 window; a device window never offers direct memory",
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name(),
            )));
        }
        let events = transaction.device_events();
        if events.len() > 1 {
            return Err(PhysicalBackendError::protocol(format!(
                "device target reported {} device events for {} {:?} at {:#018x} on the {} window; \
                 one access reports at most one event",
                events.len(),
                command_label(command),
                descriptor.category,
                descriptor.paddr,
                self.window.name(),
            )));
        }
        if let Some(event) = events.first() {
            // htif D10 rule (1): a read yields neither a UART transmit byte nor
            // an HTIF write, so an event on one is this window's protocol
            // failure on both windows.
            if command == BlockingCommand::Read {
                return Err(PhysicalBackendError::protocol(format!(
                    "device target reported {} on the read of {:#018x} on the {} window; a read \
                     yields no device event",
                    event.name(),
                    descriptor.paddr,
                    self.window.name(),
                )));
            }
            if !transaction.status().is_completion() {
                return Err(PhysicalBackendError::protocol(format!(
                    "device target reported a {} event on a {} response for {} {:?} at {:#018x} \
                     on the {} window; only an accepted access publishes an event",
                    event.name(),
                    transaction.status().name(),
                    command_label(command),
                    descriptor.category,
                    descriptor.paddr,
                    self.window.name(),
                )));
            }
            if event.window() != self.window {
                return Err(PhysicalBackendError::protocol(format!(
                    "device target reported a {} event on the {} window for {} {:?} at {:#018x}; \
                     an event that is not its window's is a protocol failure",
                    event.name(),
                    self.window.name(),
                    command_label(command),
                    descriptor.category,
                    descriptor.paddr,
                )));
            }
            // htif D10 rule (2): the one window whose event carries a value
            // reports it only when it is the request's own eight data bytes.
            if let (DeviceWindow::Htif, BlockingDeviceEvent::HtifWrite(value)) =
                (self.window, event)
            {
                if Some(*value) != requested_value {
                    return Err(PhysicalBackendError::protocol(format!(
                        "device target reported the {} value {:#018x} for the write of {:#018x} \
                         on the {} window; the value must equal the request's eight data bytes {}",
                        event.name(),
                        value,
                        descriptor.paddr,
                        self.window.name(),
                        requested_value.map_or_else(
                            || "(the request is not one complete eight-byte write)".to_string(),
                            |requested| format!("{requested:#018x}"),
                        ),
                    )));
                }
            }
        }
        Ok(())
    }

    /// htif D10 rule (2): the value one request carries for its window's event.
    ///
    /// Only a tohost write reports a value, and only a complete eight-byte one
    /// carries a value to compare.  Every other request yields `None`, so a
    /// reported `HtifWrite` on it cannot equal the request.
    fn requested_event_value(&self, command: BlockingCommand, payload: &[u8]) -> Option<u64> {
        if self.window != DeviceWindow::Htif || command != BlockingCommand::Write {
            return None;
        }
        <[u8; 8]>::try_from(payload).ok().map(u64::from_le_bytes)
    }
}

/// The two selected device windows of one Platform generation (device D3).
pub(crate) struct DevicePath {
    uart: TlmDeviceAdapter,
    htif: TlmDeviceAdapter,
    /// device D5: a host target serves the UART window, so the Platform keeps
    /// no register state of its own to view or mutate.
    host_uart: bool,
}

impl DevicePath {
    /// Builds the path from each window's selected target.
    ///
    /// Each argument is the selection's host target where one is named and the
    /// in-tree target otherwise; `with_host_uart` adds the one rule that
    /// depends on a window being host-served (device D5).
    pub(crate) fn new(
        uart: Arc<dyn BlockingDeviceTarget>,
        htif: Arc<dyn BlockingDeviceTarget>,
    ) -> Self {
        Self::build(uart, htif, false)
    }

    /// Builds the path with a host-supplied UART target (device D2).
    pub(crate) fn with_host_uart(
        uart: Arc<dyn BlockingDeviceTarget>,
        htif: Arc<dyn BlockingDeviceTarget>,
    ) -> Self {
        Self::build(uart, htif, true)
    }

    fn build(
        uart: Arc<dyn BlockingDeviceTarget>,
        htif: Arc<dyn BlockingDeviceTarget>,
        host_uart: bool,
    ) -> Self {
        Self {
            uart: TlmDeviceAdapter::new(DeviceWindow::Uart, uart),
            htif: TlmDeviceAdapter::new(DeviceWindow::Htif, htif),
            host_uart,
        }
    }

    /// device D5: true when a host target serves the UART window.
    ///
    /// The window then has no Rust register state, so a typed
    /// [`crate::memory::MemoryInterface`] access refuses instead of reading or
    /// mutating a stale object.
    pub(crate) fn uart_window_is_host_served(&self) -> bool {
        self.host_uart
    }

    /// The UART window's adapter.
    pub(crate) fn uart(&self) -> &TlmDeviceAdapter {
        &self.uart
    }

    /// The HTIF window's adapter.
    pub(crate) fn htif(&self) -> &TlmDeviceAdapter {
        &self.htif
    }
}

/// Acknowledges one selected target's transport reset (device D6).
///
/// The call is a host boundary like [`TlmDeviceAdapter::transact`], so a
/// panicking target is contained here rather than unwinding through the
/// Machine's locks.  It is reported as a refused reset: a target that cannot
/// return cannot vouch that it quiesced.
pub(crate) fn acknowledge_transport_reset(
    target: &dyn BlockingDeviceTarget,
) -> Result<(), &'static str> {
    match catch_unwind(AssertUnwindSafe(|| target.transport_reset())) {
        Ok(true) => Ok(()),
        Ok(false) => Err("did not acknowledge its transport reset"),
        Err(_) => Err("panicked during its transport reset"),
    }
}

/// One access's failed outcome: no event is ever published beside a failure
/// and no delay is charged.
fn failed(error: PhysicalBackendError) -> DeviceOutcome {
    DeviceOutcome {
        response: Err(error),
        event: None,
        delay: Femtoseconds::ZERO,
    }
}

/// The command name used in device diagnostics.
fn command_label(command: BlockingCommand) -> &'static str {
    match command {
        BlockingCommand::Read => "read",
        BlockingCommand::Write => "write",
    }
}
