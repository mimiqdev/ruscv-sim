//! Blocking device targets for the UART and HTIF windows (device D1–D5).
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
//! delay, zero host elapsed time, no direct-memory hint and at most one
//! [`BlockingDeviceEvent`].  The event belongs to the target's own window, is
//! published only for an `Ok` status, and is a Platform event under ADR-0004
//! §9.  A response that violates any of those rules, that changes the payload,
//! or that arrives after a target panic is a protocol failure of that access;
//! an outcome-unknown flag keeps its precedence and is an unknown completion.
//!
//! The atomic path never reaches a target: the HTIF read-modify-write envelope
//! and every atomic rejection stay Platform-side (device D3).

use super::{
    BlockingCommand, BlockingExtension, BlockingInitiator, BlockingResponseStatus,
    BlockingTransaction,
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
/// never wait: a device target may not let host time pass or annotate delay,
/// so [`BlockingTransaction::set_response_delay`],
/// [`BlockingTransaction::set_host_elapsed`] and
/// [`BlockingTransaction::set_dmi_allowed`] are protocol failures when used.
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

/// The Machine's device-target selection (device D2).
///
/// The selection names a host-supplied target per window.  No selection, the
/// [`Self::none`] default, means the in-tree target answers each window, so a
/// run without a host device target is today's run on either route.  Only the
/// UART window accepts a host target in this contract; the HTIF window is an
/// in-tree target and host HTIF service stays deferred.  Unit U2 adds the
/// `MachineConfig` field and the host UART selection; unit U1 defines the shape
/// and selects the in-tree targets.
#[derive(Clone, Default)]
pub struct DeviceTargets {
    uart: Option<Arc<dyn BlockingDeviceTarget>>,
}

impl DeviceTargets {
    /// No host target: every window is answered by its in-tree target.
    pub const fn none() -> Self {
        Self { uart: None }
    }

    /// The selected host UART target, when the selection names one.
    pub fn uart(&self) -> Option<&Arc<dyn BlockingDeviceTarget>> {
        self.uart.as_ref()
    }
}

impl std::fmt::Debug for DeviceTargets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceTargets")
            .field("uart", &self.uart.is_some())
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
    /// or charged and no direct-memory query is issued: a device transaction
    /// enters no turn's accounting (device D4).
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

        if let Err(error) = self.validate_response(&transaction, descriptor, command, width) {
            return failed(error);
        }

        let status = transaction.status();
        let event = transaction.device_events().first().copied();
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
        DeviceOutcome { response, event }
    }

    /// Validates one device response against device D2/D4 and the 2026-11-07
    /// ADR-0002 amendment.
    ///
    /// Every rule runs before the status mapping; an event that passes them
    /// never changes the outcome the status selects.
    fn validate_response(
        &self,
        transaction: &BlockingTransaction<'_>,
        descriptor: PhysicalRequestDescriptor,
        command: BlockingCommand,
        width: usize,
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
        if !transaction.response_delay().is_zero() {
            return Err(PhysicalBackendError::protocol(format!(
                "device target annotated {} fs of delay for {} {:?} at {:#018x} on the {} window; \
                 a device transaction reports zero delay",
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
        }
        Ok(())
    }
}

/// The two selected device windows of one Platform generation (device D3).
pub(crate) struct DevicePath {
    uart: TlmDeviceAdapter,
    htif: TlmDeviceAdapter,
}

impl DevicePath {
    /// Builds the path from each window's selected target.
    ///
    /// The Platform owns the selection: the in-tree target for a window the
    /// selection leaves alone, or the selected host target (device D2).
    pub(crate) fn new(
        uart: Arc<dyn BlockingDeviceTarget>,
        htif: Arc<dyn BlockingDeviceTarget>,
    ) -> Self {
        Self {
            uart: TlmDeviceAdapter::new(DeviceWindow::Uart, uart),
            htif: TlmDeviceAdapter::new(DeviceWindow::Htif, htif),
        }
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

/// One access's failed outcome: no event is ever published beside a failure.
fn failed(error: PhysicalBackendError) -> DeviceOutcome {
    DeviceOutcome {
        response: Err(error),
        event: None,
    }
}

/// The command name used in device diagnostics.
fn command_label(command: BlockingCommand) -> &'static str {
    match command {
        BlockingCommand::Read => "read",
        BlockingCommand::Write => "write",
    }
}
