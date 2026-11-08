//! The in-tree device targets of the UART and HTIF windows (device D3).
//!
//! They are the reference behavior a host target must reproduce: the UART
//! target wraps the existing [`Uart16550`] through the same register helpers
//! the native branch uses, and the HTIF target answers the dword read with
//! zero and the dword write with one `HtifWrite` event.  Both stay the
//! zero-delay reference: neither annotates a response delay, so an in-tree
//! device access contributes no transport delay on either route and keeps its
//! parity facts (delay D2).  A fresh target is built for every Platform
//! install, which is the whole reset rule for an in-tree window (device D6).

use super::device::{BlockingDeviceEvent, BlockingDeviceTarget};
use super::{BlockingCommand, BlockingResponseStatus, BlockingTransaction};
use crate::peripherals::uart16550::Uart16550;
use std::sync::{Arc, Mutex};

/// The in-tree UART target: one register window over the Platform's UART.
pub(crate) struct InTreeUart {
    uart: Arc<Mutex<Uart16550>>,
}

impl InTreeUart {
    /// Wraps the Platform's shared UART so host views and the device path see
    /// one register file.
    pub(crate) fn new(uart: Arc<Mutex<Uart16550>>) -> Self {
        Self { uart }
    }
}

impl BlockingDeviceTarget for InTreeUart {
    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        let mut uart = match self.uart.lock() {
            Ok(uart) => uart,
            // A poisoned UART lock is a simulator-side failure, never a
            // fabricated guest-visible target fault.
            Err(_) => {
                transaction.set_status(BlockingResponseStatus::GenericError);
                return;
            }
        };
        let offset = transaction.address().wrapping_sub(uart.base_addr());
        match transaction.command() {
            BlockingCommand::Read => {
                let Some(byte) = transaction.data_mut().first_mut() else {
                    transaction.set_status(BlockingResponseStatus::GenericError);
                    return;
                };
                *byte = uart.read_reg(offset);
            }
            BlockingCommand::Write => {
                let Some(byte) = transaction.data().first().copied() else {
                    transaction.set_status(BlockingResponseStatus::GenericError);
                    return;
                };
                // device D4: the one publication site reports the byte the
                // register helper accepted, including a write the full
                // transmit FIFO could not queue.
                if let Some(transmitted) = uart.write_reg_reporting(offset, byte) {
                    transaction.report_device_event(BlockingDeviceEvent::UartTransmit(transmitted));
                }
            }
        }
        transaction.set_status(BlockingResponseStatus::Ok);
    }

    fn receive(&self, byte: u8) {
        // A poisoned lock drops the byte: the host view has no error channel.
        if let Ok(mut uart) = self.uart.lock() {
            uart.receive_byte(byte);
        }
    }

    fn transport_reset(&self) -> bool {
        // A fresh target is built for every install, so there is no state to
        // quiesce and no association to refuse.
        true
    }
}

/// The in-tree HTIF target: the plain dword read and write of the endpoint.
///
/// The atomic read-modify-write envelope stays Platform-side and never reaches
/// this target (device D3).
pub(crate) struct InTreeHtif;

impl InTreeHtif {
    /// Creates the stateless endpoint target.
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl BlockingDeviceTarget for InTreeHtif {
    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        match transaction.command() {
            BlockingCommand::Read => {
                transaction.data_mut().fill(0);
                transaction.set_status(BlockingResponseStatus::Ok);
            }
            BlockingCommand::Write => {
                let Ok(bytes) = <[u8; 8]>::try_from(transaction.data()) else {
                    // The Platform admits only one complete eight-byte
                    // transfer, so this is a simulator-side shape failure.
                    transaction.set_status(BlockingResponseStatus::GenericError);
                    return;
                };
                transaction
                    .report_device_event(BlockingDeviceEvent::HtifWrite(u64::from_le_bytes(bytes)));
                transaction.set_status(BlockingResponseStatus::Ok);
            }
        }
    }

    fn receive(&self, _byte: u8) {
        // The HTIF window has no receive direction.
    }

    fn transport_reset(&self) -> bool {
        true
    }
}
