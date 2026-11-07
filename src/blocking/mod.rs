//! Blocking-transport adapter vocabulary (dev-plan D3/D4).
//!
//! This module is separate from [`crate::tlm`].  The existing `src/tlm`
//! components model a TLM-2.0-style payload but cannot express the standard
//! seven-status set, the ruscv-sim request extensions, debug transport or
//! femtosecond delays, so the Blocking TLM and SystemC Transport contract adds
//! this adapter vocabulary and leaves `src/tlm` unchanged.
//!
//! One [`crate::physical::PhysicalRequest`] becomes at most one blocking
//! `b_transport`-style [`BlockingTransaction`]: an explicit physical address,
//! a [`BlockingCommand`], raw little-endian bytes whose length equals the
//! access width (the streaming width is that same length, and there are no
//! byte enables), and a [`BlockingExtension`] carrying the existing
//! [`PhysicalAccessKind`] and the initiator.  The adapter never splits or
//! merges an access.  The one completion that is not a transaction is the
//! direct-memory read path: a Hart fetch or load that a target-granted host
//! pointer covers copies its bytes through that pointer instead, under the
//! same address, width and all-or-nothing rule (direct-memory D4).
//!
//! Status mapping follows ADR-0002 §4:
//!
//! | [`BlockingResponseStatus`] | ruscv-sim classification |
//! | --- | --- |
//! | `Ok` | completion |
//! | `AddressError`, `CommandError`, `BurstError`, `ByteEnableError` | guest-visible target fault |
//! | `GenericError` | host failure |
//! | `Incomplete` | protocol failure |
//!
//! A target that leaves [`BlockingResponseStatus::Incomplete`], or that changes the
//! payload's address, command or length, has not completed the transaction;
//! an outcome-unknown flag or a target panic is likewise never silently
//! accepted.  The adapter enforces every rule, so a target cannot turn a
//! simulator-side failure into a guest trap.
//!
//! Annotated delays are converted under the declared [`TickPeriod`] (default
//! 1 ns) per ADR-0004 §5.2: exact when the delay is a whole number of ticks,
//! otherwise rounded up, with zero staying zero.  Overflow and a malformed
//! annotation are adapter failures, and a response above the target's
//! declared per-transaction maximum is a §5.3 overshoot failure.

use crate::physical::PhysicalAccessKind;
use std::fmt;
use std::sync::Arc;

pub(crate) mod adapter;
mod atomic;
#[cfg(test)]
#[path = "atomic_tests.rs"]
mod atomic_tests;
mod device;
#[cfg(test)]
#[path = "device_tests.rs"]
mod device_tests;
mod direct_mem;
#[cfg(test)]
#[path = "direct_mem_tests.rs"]
mod direct_mem_tests;
pub(crate) mod in_tree;
mod target;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
mod zero_delay;

pub use atomic::{BlockingAtomicExtension, BlockingAtomicOperation, BlockingAtomicOutcome};
pub(crate) use device::{acknowledge_transport_reset, DeviceOutcome, DevicePath};
pub use device::{BlockingDeviceEvent, BlockingDeviceTarget, DeviceTargets};
pub use direct_mem::{
    BlockingDirectMemAccess, BlockingDirectMemGrant, BlockingDirectMemInvalidator,
    BlockingDirectMemQuery, BlockingDirectMemRegistry, BlockingDirectMemResponse,
};
pub(crate) use in_tree::{InTreeHtif, InTreeUart};
pub use target::BlockingTarget;
pub use zero_delay::ZeroDelayRam;

/// The seven IEEE 1666 blocking response statuses.
///
/// A transaction starts [`BlockingResponseStatus::Incomplete`] (dev-plan D3): a target
/// must set a terminal status explicitly.  A target that returns without
/// changing it has not completed the transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlockingResponseStatus {
    /// The target has not produced a terminal response.
    #[default]
    Incomplete,
    /// `TLM_OK_RESPONSE`: the transaction completed.
    Ok,
    /// `TLM_ADDRESS_ERROR_RESPONSE`: a guest-visible target fault.
    AddressError,
    /// `TLM_COMMAND_ERROR_RESPONSE`: a guest-visible target fault.
    CommandError,
    /// `TLM_BURST_ERROR_RESPONSE`: a guest-visible target fault.
    BurstError,
    /// `TLM_BYTE_ENABLE_ERROR_RESPONSE`: a guest-visible target fault.
    ByteEnableError,
    /// `TLM_GENERIC_ERROR_RESPONSE`: a host failure, never a guest trap.
    GenericError,
}

impl BlockingResponseStatus {
    /// True for `TLM_OK_RESPONSE`, the only completion status.
    pub const fn is_completion(self) -> bool {
        matches!(self, Self::Ok)
    }

    /// True for the four target-fault statuses of dev-plan D4.
    pub const fn is_guest_visible_fault(self) -> bool {
        matches!(
            self,
            Self::AddressError | Self::CommandError | Self::BurstError | Self::ByteEnableError
        )
    }

    /// The IEEE 1666 status name, for diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Incomplete => "TLM_INCOMPLETE_RESPONSE",
            Self::Ok => "TLM_OK_RESPONSE",
            Self::AddressError => "TLM_ADDRESS_ERROR_RESPONSE",
            Self::CommandError => "TLM_COMMAND_ERROR_RESPONSE",
            Self::BurstError => "TLM_BURST_ERROR_RESPONSE",
            Self::ByteEnableError => "TLM_BYTE_ENABLE_ERROR_RESPONSE",
            Self::GenericError => "TLM_GENERIC_ERROR_RESPONSE",
        }
    }
}

/// Direction of one blocking transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockingCommand {
    /// Fetch or data read in request order.
    Read,
    /// Data write in request order.
    Write,
}

/// Which Platform initiator issued a physical request.
///
/// The extension carries the existing [`PhysicalAccessKind`] and this
/// initiator; dev-plan D3 adds no new access-category taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockingInitiator {
    /// Hart fetch, data or atomic access (including page-table and A/D
    /// accesses issued by the Hart/MMU).
    Hart,
    /// Debug Module inbound system bus access.
    ///
    /// Program-buffer execution and abstract-command memory access are not
    /// this initiator: both reach RAM through the Hart's own port and report
    /// [`BlockingInitiator::Hart`] at a debug boundary.
    DebugInbound,
}

/// ruscv-sim payload extension for one blocking transaction.
///
/// The ordinary fetch/read/write mapping of unit U1 is carried by
/// `access_kind` and `initiator`.  Unit U2 adds the optional
/// [`BlockingAtomicExtension`]: an atomic transaction carries its operation,
/// width, ordering and store-conditional reservation context as data instead
/// of an ordinary read/write pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockingExtension {
    access_kind: PhysicalAccessKind,
    initiator: BlockingInitiator,
    outcome_unknown: bool,
    atomic: Option<BlockingAtomicExtension>,
}

impl BlockingExtension {
    /// Creates the ordinary request extension.
    pub const fn new(access_kind: PhysicalAccessKind, initiator: BlockingInitiator) -> Self {
        Self {
            access_kind,
            initiator,
            outcome_unknown: false,
            atomic: None,
        }
    }

    /// Creates an atomic request extension.
    pub const fn atomic(
        access_kind: PhysicalAccessKind,
        initiator: BlockingInitiator,
        atomic: BlockingAtomicExtension,
    ) -> Self {
        Self {
            access_kind,
            initiator,
            outcome_unknown: false,
            atomic: Some(atomic),
        }
    }

    /// The existing physical access kind.
    pub const fn access_kind(self) -> PhysicalAccessKind {
        self.access_kind
    }

    /// The Platform initiator that issued the request.
    pub const fn initiator(self) -> BlockingInitiator {
        self.initiator
    }

    /// The atomic payload extension, when this is an atomic transaction.
    pub const fn atomic_extension(self) -> Option<BlockingAtomicExtension> {
        self.atomic
    }

    /// Mutable access to the atomic payload extension.
    pub fn atomic_extension_mut(&mut self) -> Option<&mut BlockingAtomicExtension> {
        self.atomic.as_mut()
    }

    /// True when the target cannot vouch that the transaction completed.
    pub const fn is_outcome_unknown(self) -> bool {
        self.outcome_unknown
    }

    /// Marks the transaction outcome as unknown.
    ///
    /// A target sets this when it cannot establish whether the effects were
    /// applied (for example a timeout with an in-flight external write).  The
    /// adapter turns it into an unknown completion, poisons the transport and
    /// never issues another access until a transport reset is acknowledged.
    pub fn mark_outcome_unknown(&mut self) {
        self.outcome_unknown = true;
    }
}

/// Exact femtosecond delay with checked arithmetic (dev-plan D3/D7).
///
/// The adapter converts an annotated response delay to Platform ticks under a
/// declared [`TickPeriod`] (ADR-0004 §5.2).  A delay that is not a whole
/// number of ticks is rounded up, so a positive sub-tick delay stays visible;
/// zero remains zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Femtoseconds(u64);

impl Femtoseconds {
    /// Zero delay.
    pub const ZERO: Self = Self(0);

    /// Creates an exact femtosecond value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the exact femtosecond count.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the exact femtosecond count as a wider value.
    pub const fn as_u128(self) -> u128 {
        self.0 as u128
    }

    /// True for an exact zero delay.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Checked addition; `None` on overflow.
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Checked multiplication; `None` on overflow.
    pub const fn checked_mul(self, factor: u64) -> Option<Self> {
        match self.0.checked_mul(factor) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Converts this delay to Platform ticks under the declared `period`
    /// (ADR-0004 §5.2).
    ///
    /// Exact conversion is used when the delay is a whole number of ticks;
    /// otherwise a positive delay is rounded up and zero stays zero.  A
    /// malformed period or an overflowing conversion is an adapter failure,
    /// never a negative or silently dropped clock update.
    pub fn to_platform_ticks(self, period: TickPeriod) -> Result<u64, DelayConversionError> {
        let tick = period.femtoseconds().as_u64();
        if tick == 0 {
            return Err(DelayConversionError::Malformed);
        }
        let quotient = self.0 / tick;
        let remainder = self.0 % tick;
        quotient
            .checked_add(u64::from(remainder != 0))
            .ok_or(DelayConversionError::Overflow)
    }
}

/// The Machine's declared Platform tick period (dev-plan D7, ADR-0004 §5.2).
///
/// One Platform tick is one virtual-time unit and one `mtime` unit.  The
/// declared default is 1 ns (`1_000_000` fs); [`crate::machine::MachineConfig`]
/// carries the declaration so a target's annotated delays and its declared
/// per-transaction maximum are converted under the same period, and the C ABI
/// takes it from its create call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TickPeriod(Femtoseconds);

impl TickPeriod {
    /// The declared default: 1 ns per Platform tick.
    pub const DEFAULT: Self = Self(Femtoseconds::new(1_000_000));

    /// Creates a declared tick period from an exact femtosecond value.
    pub const fn new(femtoseconds: Femtoseconds) -> Self {
        Self(femtoseconds)
    }

    /// The tick period in exact femtoseconds.
    pub const fn femtoseconds(self) -> Femtoseconds {
        self.0
    }

    /// True for a period that can represent a positive delay.
    pub const fn is_valid(self) -> bool {
        self.0.as_u64() != 0
    }
}

impl Default for TickPeriod {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A delay that cannot enter the Platform cursor (ADR-0004 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayConversionError {
    /// The declared tick period cannot represent a positive delay.
    Malformed,
    /// The converted value does not fit the Platform tick count.
    Overflow,
}

impl fmt::Display for DelayConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => formatter.write_str("the declared tick period is zero"),
            Self::Overflow => formatter.write_str("the converted delay overflows the tick count"),
        }
    }
}

impl std::error::Error for DelayConversionError {}

/// Worst-case guest RAM transactions in one guest Hart turn (dev-plan D7).
///
/// A complete instruction fetch occupies up to two parcels, and each parcel is
/// translated separately through three Sv39 levels.  A walk that must set the
/// A/D bits reads each level's PTE and performs the walk-private LR/SC pair
/// on the leaf, then issues its own access: `3 + 2 + 1 = 6` transactions per
/// parcel, or 12 for a straddling instruction.  One data or atomic access
/// adds the same six.  The conservative completion bound therefore uses 18
/// transactions.
///
/// A `b_transport` response is bounded separately by the target's declared
/// per-transaction maximum, so this count only sizes the turn-level bound.
pub const MAX_GUEST_TRANSACTIONS_PER_TURN: u64 = 18;

/// One guest Hart turn's conservative completion bound (dev-plan D7).
///
/// ADR-0004 §7.2 adds the target's declared per-transaction maximum delay once
/// per worst-case transaction.  The maximum is converted with the same
/// rounding rule as a response, so a sub-tick declared maximum still reserves
/// one tick per transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnCompletionBound {
    per_transaction_ticks: u64,
    transactions: u64,
    ticks: u64,
}

impl TurnCompletionBound {
    /// Builds the bound for one guest turn.
    pub fn new(max_delay: Femtoseconds, period: TickPeriod) -> Result<Self, DelayConversionError> {
        let per_transaction_ticks = max_delay.to_platform_ticks(period)?;
        let ticks = per_transaction_ticks
            .checked_mul(MAX_GUEST_TRANSACTIONS_PER_TURN)
            .ok_or(DelayConversionError::Overflow)?;
        Ok(Self {
            per_transaction_ticks,
            transactions: MAX_GUEST_TRANSACTIONS_PER_TURN,
            ticks,
        })
    }

    /// Converted delay reserved for one worst-case transaction.
    pub const fn ticks_per_transaction(self) -> u64 {
        self.per_transaction_ticks
    }

    /// Worst-case transactions this bound reserves delay for.
    pub const fn transactions(self) -> u64 {
        self.transactions
    }

    /// The whole turn's delay bound in Platform ticks.
    pub const fn ticks(self) -> u64 {
        self.ticks
    }
}

/// One blocking generic-payload transaction.
///
/// The initial status is [`BlockingResponseStatus::Incomplete`]; only the
/// target sets a terminal status.  `data` holds the access-width raw bytes in
/// increasing physical-address order (little-endian for the current profile),
/// and `data_length` is mutable so a target can be proven not to change the
/// transfer length.
pub struct BlockingTransaction<'a> {
    address: u64,
    command: BlockingCommand,
    data: &'a mut [u8],
    data_length: usize,
    status: BlockingResponseStatus,
    extension: BlockingExtension,
    response_delay: Femtoseconds,
    dmi_allowed: bool,
    local_offset: Femtoseconds,
    may_suspend: bool,
    host_elapsed: Femtoseconds,
    /// device D2: the at-most-one guest-visible effect this access reports.
    /// A RAM target never reports one; a device target reports its window's
    /// event on an `Ok` status.
    device_events: Vec<BlockingDeviceEvent>,
}

impl<'a> BlockingTransaction<'a> {
    /// Creates one transaction whose status starts incomplete, whose delay
    /// starts at zero and whose local offset starts at zero.
    pub fn new(
        address: u64,
        command: BlockingCommand,
        data: &'a mut [u8],
        extension: BlockingExtension,
    ) -> Self {
        let data_length = data.len();
        Self {
            address,
            command,
            data,
            data_length,
            status: BlockingResponseStatus::Incomplete,
            extension,
            response_delay: Femtoseconds::ZERO,
            dmi_allowed: false,
            local_offset: Femtoseconds::ZERO,
            may_suspend: false,
            host_elapsed: Femtoseconds::ZERO,
            device_events: Vec::new(),
        }
    }

    /// The explicit physical start address.
    pub const fn address(&self) -> u64 {
        self.address
    }

    /// The transfer direction.
    pub const fn command(&self) -> BlockingCommand {
        self.command
    }

    /// The raw transfer bytes.
    pub fn data(&self) -> &[u8] {
        self.data
    }

    /// Mutable access to the raw transfer bytes.
    ///
    /// A read target writes its bytes here; a write target observes the
    /// request payload.
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }

    /// The length reported by the payload.
    pub const fn data_length(&self) -> usize {
        self.data_length
    }

    /// The streaming width, which the issue mapping sets equal to the data
    /// length.
    pub const fn streaming_width(&self) -> usize {
        self.data_length
    }

    /// The current response status.
    pub const fn status(&self) -> BlockingResponseStatus {
        self.status
    }

    /// Sets the terminal response status.
    pub fn set_status(&mut self, status: BlockingResponseStatus) {
        self.status = status;
    }

    /// The ruscv-sim request extension.
    pub const fn extension(&self) -> BlockingExtension {
        self.extension
    }

    /// Mutable access to the ruscv-sim request extension.
    pub fn extension_mut(&mut self) -> &mut BlockingExtension {
        &mut self.extension
    }

    /// The atomic payload extension, when this is an atomic transaction.
    pub const fn atomic_extension(&self) -> Option<BlockingAtomicExtension> {
        self.extension.atomic_extension()
    }

    /// Mutable access to the atomic payload extension.
    pub fn atomic_extension_mut(&mut self) -> Option<&mut BlockingAtomicExtension> {
        self.extension.atomic_extension_mut()
    }

    /// The annotated response delay.
    pub const fn response_delay(&self) -> Femtoseconds {
        self.response_delay
    }

    /// The Hart's local time offset for this request (quantum D11).
    ///
    /// Read-only timing metadata, the counterpart of the IEEE 1666
    /// `b_transport` delay argument: the Hart's local time `L` minus the anchor
    /// `E`, plus the delay ticks this turn already charged, in exact
    /// femtoseconds.  It is zero when no quantum is selected, for every
    /// `DebugInbound` request and for debug transport.  Address, width, bytes,
    /// category and status never depend on it, and it is never charged as a
    /// delay.
    pub const fn local_offset(&self) -> Femtoseconds {
        self.local_offset
    }

    /// Sets the local-time offset of this request.
    ///
    /// Only the adapter sets it, once, before the target sees the transaction; a
    /// target must not do this.  It is request metadata, so a target that
    /// reports something else in a copied payload changes nothing here.
    pub(crate) fn set_local_offset(&mut self, offset: Femtoseconds) {
        self.local_offset = offset;
    }

    /// Annotates the response delay.
    pub fn set_response_delay(&mut self, delay: Femtoseconds) {
        self.response_delay = delay;
    }

    /// True when this request is a charged transaction to a suspending target
    /// (nb D3).
    ///
    /// Read-only timing metadata: address, width, bytes, category and status
    /// never depend on it.  The adapter sets it once at issue time, from the
    /// target's [`BlockingTarget::suspends_host_time`], the initiator and the
    /// open guest-turn window, so it is false for every request that is not a
    /// charged Hart transaction -- Debug Mode Hart-port work, system bus
    /// access, debug transport, transport reset and a direct-memory query --
    /// and for every request to a non-suspending target.
    ///
    /// A true value is the only licence to let host time pass inside
    /// [`BlockingTarget::b_transport`] and to report it through
    /// [`Self::set_host_elapsed`].
    pub const fn may_suspend(&self) -> bool {
        self.may_suspend
    }

    /// Sets the request's [`Self::may_suspend`] flag.
    ///
    /// Only the adapter sets it, once, before the target sees the transaction;
    /// a target that copies a payload cannot change it here.
    pub(crate) fn set_may_suspend(&mut self, may_suspend: bool) {
        self.may_suspend = may_suspend;
    }

    /// The host time that passed inside this transaction's callback (nb D4).
    ///
    /// Exact femtoseconds between the callback's entry and its return.  The
    /// adapter charges it under the same rounding, declared-maximum and turn
    /// rules as the response delay, and only for a charged guest-turn
    /// transaction.  It never enters `mcycle` or `minstret`.
    pub const fn host_elapsed(&self) -> Femtoseconds {
        self.host_elapsed
    }

    /// Reports the host time that passed inside this transaction's callback.
    ///
    /// A target sets this before it returns, for a request whose
    /// [`Self::may_suspend`] is true; a non-zero value on any other request is
    /// a protocol failure of the access (nb D4).  A target that lets no host
    /// time pass leaves it zero, which is every existing target.
    pub fn set_host_elapsed(&mut self, elapsed: Femtoseconds) {
        self.host_elapsed = elapsed;
    }

    /// The `dmi_allowed` response hint (direct-memory D3).
    ///
    /// A target that can serve this address through a host pointer sets it on a
    /// completed non-atomic transaction.  It is the only licence for the
    /// adapter's direct-memory query, and the adapter ignores it for an atomic
    /// transaction, a non-completion and with direct memory disabled.
    pub const fn is_dmi_allowed(&self) -> bool {
        self.dmi_allowed
    }

    /// Sets or clears the `dmi_allowed` response hint.
    pub fn set_dmi_allowed(&mut self, allowed: bool) {
        self.dmi_allowed = allowed;
    }

    /// Changes the payload address.
    ///
    /// A target must not do this; the adapter reports a changed payload as a
    /// protocol failure (dev-plan D4).
    pub fn set_address(&mut self, address: u64) {
        self.address = address;
    }

    /// Changes the payload command.
    ///
    /// A target must not do this; the adapter reports a changed payload as a
    /// protocol failure (dev-plan D4).
    pub fn set_command(&mut self, command: BlockingCommand) {
        self.command = command;
    }

    /// Changes the payload length.
    ///
    /// A target must not do this; the adapter reports a changed payload as a
    /// protocol failure (dev-plan D4).
    pub fn set_data_length(&mut self, length: usize) {
        self.data_length = length;
    }

    /// The device events this access reports (device D2).
    ///
    /// Empty for a RAM target and for a device access with no guest-visible
    /// effect.  An accepted access reports at most one event, and it belongs
    /// to the target's own window; the device adapter rejects any other shape
    /// as a protocol failure.
    pub fn device_events(&self) -> &[BlockingDeviceEvent] {
        &self.device_events
    }

    /// Reports one guest-visible device event (device D2/D4).
    ///
    /// Only a device window publishes an event.  A RAM target that calls this
    /// is a protocol failure of that access, as is a device target that
    /// reports more than one event, reports one on a status other than `Ok`,
    /// or reports another window's event.
    pub fn report_device_event(&mut self, event: BlockingDeviceEvent) {
        self.device_events.push(event);
    }
}

impl fmt::Debug for BlockingTransaction<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingTransaction")
            .field("address", &format_args!("{:#018x}", self.address))
            .field("command", &self.command)
            .field("data_length", &self.data_length)
            .field("status", &self.status)
            .field("extension", &self.extension)
            .field("response_delay", &self.response_delay)
            .field("dmi_allowed", &self.dmi_allowed)
            .field("local_offset", &self.local_offset)
            .field("may_suspend", &self.may_suspend)
            .field("host_elapsed", &self.host_elapsed)
            .field("device_events", &self.device_events)
            .finish()
    }
}

/// One debug-transport request (dev-plan D6).
///
/// Debug transport carries no extension and no delay, has no device side
/// effects on the target's modeled state beyond the addressed bytes, and is
/// not counted as a transaction.
pub struct BlockingDebug<'a> {
    address: u64,
    command: BlockingCommand,
    data: &'a mut [u8],
}

impl<'a> BlockingDebug<'a> {
    /// Creates one debug-transport request.
    pub const fn new(address: u64, command: BlockingCommand, data: &'a mut [u8]) -> Self {
        Self {
            address,
            command,
            data,
        }
    }

    /// The explicit physical start address.
    pub const fn address(&self) -> u64 {
        self.address
    }

    /// The transfer direction.
    pub const fn command(&self) -> BlockingCommand {
        self.command
    }

    /// The raw transfer bytes.
    pub fn data(&self) -> &[u8] {
        self.data
    }

    /// Mutable access to the raw transfer bytes.
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }

    /// The requested transfer length.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when the request carries no bytes.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl fmt::Debug for BlockingDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockingDebug")
            .field("address", &format_args!("{:#018x}", self.address))
            .field("command", &self.command)
            .field("len", &self.data.len())
            .finish()
    }
}

/// The Machine's guest-RAM transport selection (dev-plan D2).
///
/// Native is the default and keeps the existing `SimpleMemory` route.  A TLM
/// selection is a caller-supplied [`BlockingTarget`] shared through an `Arc`,
/// so [`crate::machine::MachineConfig`] stays `Clone`.
#[derive(Clone, Default)]
pub enum Transport {
    /// The existing native `SimpleMemory` RAM route (default).
    #[default]
    Native,
    /// A caller-supplied blocking RAM target.
    Tlm(Arc<dyn BlockingTarget>),
}

impl Transport {
    /// The native route.
    pub const fn native() -> Self {
        Self::Native
    }

    /// A TLM route over one shared blocking target.
    pub fn tlm(target: Arc<dyn BlockingTarget>) -> Self {
        Self::Tlm(target)
    }

    /// True for the native route.
    pub const fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }

    /// The blocking target, when this is a TLM selection.
    pub fn target(&self) -> Option<&Arc<dyn BlockingTarget>> {
        match self {
            Self::Native => None,
            Self::Tlm(target) => Some(target),
        }
    }
}

impl fmt::Debug for Transport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native => formatter.write_str("Transport::Native"),
            Self::Tlm(target) => formatter
                .debug_struct("Transport::Tlm")
                .field("max_delay", &target.declared_max_delay())
                .finish_non_exhaustive(),
        }
    }
}
