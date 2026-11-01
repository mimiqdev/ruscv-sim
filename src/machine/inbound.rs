//! The Platform's inbound physical path for the DM's system bus access.
//!
//! ADR-0002 §9, as amended on 2026-10-30, admits the DM as the first inbound
//! Platform master. The DM originates physical reads and writes of 8, 16, 32
//! or 64 bits at an explicit physical address, and the Platform routes them
//! with the same raw-byte, width, routing, all-or-nothing, side-effect and
//! target-fault semantics as Hart-issued transactions (contract §3).
//!
//! [`InboundPort`] is a separate validated `PhysicalAccess` view over the same
//! RAM/device backend as the Hart's data port. It is not the Hart's port: no
//! translation, privilege, PMP, trap or instruction path is involved, and the
//! view has no atomic capability, so an SBA transaction can never carry an
//! atomic envelope. Because both views reach one storage/device object, a
//! committed SBA write records the same RAM committed-write bookkeeping a Hart
//! store does; that bookkeeping is what invalidates an overlapping Hart LR/SC
//! reservation inside the store-conditional critical section. The same storage
//! or device lock covers a Hart AMO/SC, so an SBA transaction can neither
//! observe nor expose the intermediate state of one.
//!
//! Machine admission and life cycle are U3: a request is taken from the DM
//! and its result recorded between two DM lock acquisitions, so no DM lock is
//! held across the target access.

use crate::core::SharedPhysicalAccess;
use crate::dm::{SbaAccessSize, SbaFault, SbaRequest};
use crate::physical::{PhysicalAccessError, PhysicalRequest, PhysicalWidth};

/// Why the Platform could not complete a DM system bus access.
///
/// The first variant maps to the DM's `sberror`; the other two are
/// simulator-side classifications and must never become a guest trap or a
/// fabricated `sberror` (contract D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SbaAccessError {
    /// The target refused the physical span. The DM records one `sberror`
    /// value from this target fault and a debugger retries after W1C.
    Target(SbaFault),
    /// A host, backend or protocol failure. The existing simulator failure
    /// classification applies.
    Simulator(String),
    /// Completion cannot be proven. The Machine keeps the existing
    /// unknown-completion quarantine and refuses resume (ADR-0002 §4).
    UnknownCompletion(String),
}

/// The Platform-owned inbound port the DM initiates physical requests through.
pub(super) struct InboundPort {
    port: SharedPhysicalAccess,
    /// Runs at the start of one target access. The U3 tests observe the DM lock
    /// or hold an access open; production builds have no hook.
    #[cfg(test)]
    before: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

impl InboundPort {
    /// Wraps one validated port over the Platform's RAM/device backend.
    ///
    /// The port must be a view over the same backend as Hart-issued
    /// transactions; the Platform owns that construction.
    pub(super) const fn new(port: SharedPhysicalAccess) -> Self {
        Self {
            port,
            #[cfg(test)]
            before: None,
        }
    }

    #[cfg(test)]
    pub(super) fn set_before(&mut self, hook: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.before = Some(hook);
    }

    /// Submits one system bus access and returns the raw read value, or `0`
    /// for a write acknowledgement.
    ///
    /// The DM has already checked `sbaccess` and alignment, so a request that
    /// arrives here is a well-formed 1/2/4/8-byte physical transaction. A
    /// target refusal is a bad address for `sberror`; a failed transaction
    /// commits nothing, so it also invalidates no reservation.
    pub(super) fn access(&self, request: SbaRequest) -> Result<u64, SbaAccessError> {
        #[cfg(test)]
        if let Some(before) = &self.before {
            before();
        }
        let width = match request.size {
            SbaAccessSize::Byte => PhysicalWidth::Byte,
            SbaAccessSize::Half => PhysicalWidth::Halfword,
            SbaAccessSize::Word => PhysicalWidth::Word,
            SbaAccessSize::DoubleWord => PhysicalWidth::Doubleword,
        };
        // `sbdata` holds the value little-endian, and this platform profile is
        // little-endian (ADR-0002 §2). The DM masks the value to the access
        // size, so only the low bytes of the staged word are transferred.
        let payload = request.write.map(u64::to_le_bytes);
        let transfer = match &payload {
            Some(bytes) => {
                PhysicalRequest::data_write(request.address, width, &bytes[..width.bytes()])
            }
            None => PhysicalRequest::data_read(request.address, width),
        }
        .map_err(|error| SbaAccessError::Simulator(error.to_string()))?;

        let mut port = self
            .port
            .lock()
            .map_err(|_| SbaAccessError::Simulator("poisoned SBA inbound port".into()))?;
        let response = port.access(transfer).map_err(map_access_error)?;
        match request.write {
            Some(_) => Ok(0),
            None => response.read_bytes().map(read_le).ok_or_else(|| {
                SbaAccessError::Simulator("validated port returned no read bytes".into())
            }),
        }
    }
}

/// Maps a physical-port failure onto the DM boundary.
///
/// Every target refusal is a bad address for `sberror`: the Debug
/// specification has no separate code for a denied or unsupported-width
/// target, and contract D5 forbids inventing an "other" error. Alignment and
/// unsupported `sbaccess` never reach a target; the DM decides them first.
fn map_access_error(error: PhysicalAccessError) -> SbaAccessError {
    match error {
        PhysicalAccessError::TargetRejected(_) => SbaAccessError::Target(SbaFault::BadAddress),
        PhysicalAccessError::BackendFailure(failure) => SbaAccessError::Simulator(failure.context),
        PhysicalAccessError::Protocol(error) => SbaAccessError::Simulator(error.to_string()),
        PhysicalAccessError::UnknownCompletion(unknown) => {
            SbaAccessError::UnknownCompletion(unknown.context)
        }
    }
}

/// Decodes raw bytes of at most one doubleword in increasing physical-address
/// order, with the least-significant byte at the lowest address.
fn read_le(bytes: &[u8]) -> u64 {
    let mut raw = [0; 8];
    raw[..bytes.len()].copy_from_slice(bytes);
    u64::from_le_bytes(raw)
}

#[cfg(test)]
#[path = "inbound_tests.rs"]
mod tests;
