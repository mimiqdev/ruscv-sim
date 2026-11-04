//! Blocking-transport adapter vocabulary (dev-plan D3/D4).
//!
//! This module is separate from [`crate::tlm`].  The existing `src/tlm`
//! components model a TLM-2.0-style payload but cannot express the standard
//! seven-status set, the ruscv-sim request extensions, debug transport or
//! femtosecond delays, so the Blocking TLM and SystemC Transport contract adds
//! this adapter vocabulary and leaves `src/tlm` unchanged.
//!
//! One [`crate::physical::PhysicalRequest`] becomes exactly one blocking
//! `b_transport`-style [`BlockingTransaction`]: an explicit physical address,
//! a [`BlockingCommand`], raw little-endian bytes whose length equals the
//! access width (the streaming width is that same length, and there are no
//! byte enables), and a [`BlockingExtension`] carrying the existing
//! [`PhysicalAccessKind`] and the initiator.  The adapter never splits,
//! merges, caches or retries an access.
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
mod target;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
mod zero_delay;

pub use atomic::{BlockingAtomicExtension, BlockingAtomicOperation, BlockingAtomicOutcome};
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
    /// Debug Module inbound system bus or program-buffer access.
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
/// declared default is 1 ns (`1_000_000` fs).  Unit U3 fixes that default for
/// the Machine configuration; the C-ABI grant (unit U5) carries the period
/// explicitly.
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
}

impl<'a> BlockingTransaction<'a> {
    /// Creates one transaction whose status starts incomplete and whose delay
    /// starts at zero.
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

    /// Annotates the response delay.
    pub fn set_response_delay(&mut self, delay: Femtoseconds) {
        self.response_delay = delay;
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
