//! N=1 composition and explicit lifecycle, separate from Runner budgets/results.
//! Shared and exclusive ownership adapters use the same composition/turn path.
//! Returned boundary receipts
//! keep accepted control/observation work admitted until consumed (dropped).
mod admission;
mod debug_hub;
mod host_memory;
mod host_port;
mod inbound;
mod owned;
mod platform;
use debug_hub::DebugHub;
pub(crate) use owned::OwnedMachine;
#[cfg(test)]
pub(crate) use owned::SignalOperation;

use crate::blocking::{TickPeriod, Transport};
use crate::core::observation::{HartTransition, ObservationSink};
use crate::core::{CoreState, RiscvCore, StepOutcome};
use crate::elf::LoadImage;
use crate::memory::{MemoryError, MemoryInterface};
use crate::peripherals::uart16550::UartInspection;
use admission::{lock, Admission, Gate, Lease};
pub use admission::{Lifecycle, WorkCounts};
use host_memory::HostMemory;
pub use platform::PlatformEvent;
use platform::{Platform, SharedMemory};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    Native,
    Flat,
}

/// Selectable Machine ISA profile (contract D3).
///
/// The profile is Machine composition configuration fixed for the Machine's
/// lifetime. The Hart applies every profile-dependent rule: parcel fetch,
/// IALIGN, `misa`, `xepc`/`dpc` legality, and the floating-point state and
/// CSR rules (contract D5/D6). The Machine-level default is `rv64imafdc`
/// (ADR-0003 2026-11-04 default-profile amendment, contract D2); the raw
/// `RiscvCore` compatibility constructors stay `rv64ima` by explicit
/// selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IsaProfile {
    /// RV64IMA/Zicsr + Zifencei, IALIGN=32, `misa` `0x8000000000141101`.
    Rv64ima,
    /// The `rv64ima` profile plus Zca, IALIGN=16, `misa` `0x8000000000141105`.
    Rv64imac,
    /// RV64IMA + F + D, IALIGN=32, `misa` `0x8000000000141129`.
    Rv64imafd,
    /// `rv64imafd` plus Zca and Zcd, IALIGN=16, `misa`
    /// `0x800000000014112D`. The Machine composition default (contract D2).
    #[default]
    Rv64imafdc,
}

impl IsaProfile {
    /// Read-only `misa` for this profile (contract D3).
    pub fn misa_value(self) -> u64 {
        match self {
            Self::Rv64ima => crate::csr::MISA_VALUE,
            Self::Rv64imac => crate::csr::MISA_VALUE | crate::csr::MISA_C_BIT,
            Self::Rv64imafd => {
                crate::csr::MISA_VALUE | crate::csr::MISA_F_BIT | crate::csr::MISA_D_BIT
            }
            Self::Rv64imafdc => {
                crate::csr::MISA_VALUE
                    | crate::csr::MISA_F_BIT
                    | crate::csr::MISA_D_BIT
                    | crate::csr::MISA_C_BIT
            }
        }
    }

    /// IALIGN in bytes: 4 for `rv64ima`/`rv64imafd`, 2 for the compressed
    /// profiles (contract D5).
    pub fn ialign_bytes(self) -> u64 {
        if self.allows_compressed() {
            2
        } else {
            4
        }
    }

    /// True for the IALIGN=16 profiles (`rv64imac`, `rv64imafdc`).
    pub fn allows_compressed(self) -> bool {
        matches!(self, Self::Rv64imac | Self::Rv64imafdc)
    }

    /// True for the profiles with the F extension (contract D2/D3).
    ///
    /// This is an explicit capability check, not an inference from
    /// compression.
    pub fn has_f(self) -> bool {
        matches!(self, Self::Rv64imafd | Self::Rv64imafdc)
    }

    /// True for the profiles with the D extension (contract D2/D3).
    pub fn has_d(self) -> bool {
        matches!(self, Self::Rv64imafd | Self::Rv64imafdc)
    }

    /// True for the profiles with architectural floating-point state (D5/D6).
    pub fn has_fp(self) -> bool {
        self.has_f() && self.has_d()
    }

    /// True for the profiles with the compressed FP loads and stores of Zcd
    /// (contract D8).
    ///
    /// Zcd is Zca plus D, so only `rv64imafdc` has it: `rv64imac` has the
    /// compressed encodings but no FP state, and `rv64imafd` has D but
    /// IALIGN=32.
    pub fn has_zcd(self) -> bool {
        self.allows_compressed() && self.has_d()
    }

    /// Parse the approved CLI/facade profile name (contract D3).
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "rv64ima" => Some(Self::Rv64ima),
            "rv64imac" => Some(Self::Rv64imac),
            "rv64imafd" => Some(Self::Rv64imafd),
            "rv64imafdc" => Some(Self::Rv64imafdc),
            _ => None,
        }
    }

    /// The profile's canonical name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rv64ima => "rv64ima",
            Self::Rv64imac => "rv64imac",
            Self::Rv64imafd => "rv64imafd",
            Self::Rv64imafdc => "rv64imafdc",
        }
    }
}

/// Static topology is restricted to the existing native bus or flat RAM.
/// Callbacks are synchronous host connections, not asynchronous transports.
#[derive(Clone)]
pub struct MachineConfig {
    pub platform: PlatformKind,
    /// Machine-owned ISA profile; the Hart applies it at construction.
    pub isa: IsaProfile,
    /// Guest-RAM transport selection (dev-plan D2).
    ///
    /// [`Transport::Native`] keeps the existing `SimpleMemory` route and is the
    /// default.  [`Transport::Tlm`] shares a caller-supplied blocking target and
    /// requires [`PlatformKind::Native`].
    pub transport: Transport,
    /// Direct-memory selection for the selected guest-RAM target
    /// (direct-memory D2).
    ///
    /// The default is `false`, which keeps the blocking route exactly as it was:
    /// the adapter issues no direct-memory query and ignores every
    /// `dmi_allowed` hint.  `true` requires a [`Transport::Tlm`] selection;
    /// [`MachineError::UnsupportedTransport`] reports a native combination.
    pub dmi: bool,
    /// The temporal-coupling quantum in Platform ticks (quantum D2).
    ///
    /// Zero (the default) is off: every turn advances committed time directly
    /// on exactly the pre-quantum path.  A non-zero selection lets the Hart
    /// lead committed time by a bounded local offset that synchronizes at the
    /// threshold and at the contract's forced points (quantum D3–D5).
    ///
    /// A non-zero value requires a [`Transport::Tlm`] selection and
    /// [`MachineError::Placement`] reports a bound that does not fit the
    /// declared tick period.
    pub quantum: u64,
    /// The declared Platform tick period (dev-plan D7).
    ///
    /// One tick is one virtual-time unit and one `mtime` unit.  A blocking
    /// target's annotated delays and its declared per-transaction maximum are
    /// converted under this period (ADR-0004 §5.2), so a host that declares a
    /// different unit gets the same conversion the response uses.  The default
    /// is 1 ns; the C ABI (unit U5) takes it from its create call.
    pub tick_period: TickPeriod,
    pub verbose: bool,
    /// Native guest/bus address; flat storage offset used only without ELF tohost.
    pub tohost_override: Option<u64>,
    pub uart_output: Option<Arc<dyn Fn(u8) + Send + Sync>>,
    pub htif_write: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}
impl MachineConfig {
    pub fn new(platform: PlatformKind) -> Self {
        Self {
            platform,
            // Machine composition default is `rv64imafdc` (contract D2).
            isa: IsaProfile::default(),
            transport: Transport::Native,
            dmi: false,
            quantum: 0,
            tick_period: TickPeriod::DEFAULT,
            verbose: false,
            tohost_override: None,
            uart_output: None,
            htif_write: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("Machine work is still admitted/in flight")]
    Busy,
    #[error("explicit quiesce/drain is required before lifecycle mutation")]
    NotDrained,
    #[error("Machine is not admitting ordinary work")]
    NotRunning,
    #[error(
        "unknown completion: no adapter proof excludes late effects; drain/reset/reuse refused"
    )]
    UnknownCompletion,
    #[error("failed run must be drained and freshly reset before another turn")]
    NeedsFreshReset,
    #[error("Machine has no installed image")]
    NoImage,
    #[error("Machine has been torn down")]
    TornDown,
    #[error("poisoned Machine/Platform resource; cannot prove safe lifecycle")]
    Poisoned,
    #[error("placement failed: {0}")]
    Placement(String),
    #[error(
        "unsupported transport configuration: a TLM transport requires PlatformKind::Native (Flat \
         has no TLM route), direct memory requires a TLM transport (the native route has no direct \
         memory), and a quantum requires a TLM transport (the native route has no local offset)"
    )]
    UnsupportedTransport,
    #[error("this configuration has no UART")]
    NoUart,
    #[error("no RAM-backed selected tohost")]
    NoRamTohost,
    #[error("{0}")]
    Debug(String),
    #[error(transparent)]
    Memory(#[from] MemoryError),
}

/// Raw selected signal, without guest-exit decoding or run-stop classification.
#[derive(Debug)]
pub struct TohostSample {
    pub address: u64,
    pub value: Result<u64, MemoryError>,
}

/// Supported outer boundary facts, not a selected Runner terminal reason.
#[derive(Debug)]
pub struct BoundaryFacts {
    pub budget_exhausted: bool,
    pub single_step: bool,
    pub reporting_error: Option<String>,
    pub lifecycle: Lifecycle,
    pub uncertain: bool,
    /// The hart is waiting and this call retired nothing.
    pub waiting: bool,
    /// A wait has no future platform event.
    pub no_progress: bool,
    /// The idle jump stopped at a virtual-time deadline.
    pub deadline_stop: bool,
    /// Total consumed blocking-transport delay in exact femtoseconds.
    ///
    /// This is the guest-turn delay converted and charged to Platform time so
    /// far under ADR-0004 §5.3.  It is zero on the native route and for a
    /// zero-delay target, and excludes SBA/Debug Mode delay, which is
    /// validated but not charged.
    pub transport_delay_femtoseconds: u64,
    /// The boundary's quantum facts (quantum D10); `None` when the quantum
    /// is off, whose stdout stays byte-identical.
    pub quantum: Option<QuantumFacts>,
}
impl Default for BoundaryFacts {
    fn default() -> Self {
        Self {
            budget_exhausted: false,
            single_step: false,
            reporting_error: None,
            lifecycle: Lifecycle::Running,
            uncertain: false,
            waiting: false,
            no_progress: false,
            deadline_stop: false,
            transport_delay_femtoseconds: 0,
            quantum: None,
        }
    }
}

/// quantum D9: the in-crate conservation ledger of one Platform generation.
///
/// Committed time equals `base` plus the run's completed turns, charged delay
/// ticks and idle-jump distance, with any guest or debug `mtime` store
/// rebasing `base` exactly as in a parity run. It is checked in ticks because
/// no public fact exposes them (quantum D9).
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeLedger {
    /// Committed time at install, a fresh Platform, or the last rebase.
    base: u64,
    /// Completed Hart turns charged since that rebase.
    completed_turns: u64,
    /// Converted blocking-transport delay ticks charged since that rebase.
    delay_ticks: u64,
    /// Idle-jump distance in ticks since that rebase.
    jump_ticks: u64,
}

#[cfg(test)]
impl TimeLedger {
    /// Everything charged since the last rebase.
    pub(crate) fn charged(&self) -> u64 {
        self.completed_turns + self.delay_ticks + self.jump_ticks
    }

    /// The committed time this ledger's identity requires.
    pub(crate) fn expected_mtime(&self) -> u64 {
        self.base.wrapping_add(self.charged())
    }

    /// A rebase: a guest, debug or in-crate `mtime` store replaces committed
    /// time, so the identity restarts from the stored value.
    fn rebase(&mut self, committed: u64) {
        self.base = committed;
        self.completed_turns = 0;
        self.delay_ticks = 0;
        self.jump_ticks = 0;
    }

    fn charge_turn(&mut self, delay_ticks: u64, completed: bool) {
        if completed {
            self.completed_turns += 1;
        }
        self.delay_ticks += delay_ticks;
    }

    fn charge_jump(&mut self, ticks: u64) {
        self.jump_ticks += ticks;
    }
}

/// The reduced-accuracy facts of one boundary under a selected quantum
/// (quantum D10).  Present only when the quantum is non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantumFacts {
    /// The selected quantum `Q` in Platform ticks.
    pub ticks: u64,
    /// The declared interrupt-latency bound: zero for `Q` = 1, otherwise
    /// `Q - 1 + t`, with `t` the selected route's turn completion bound
    /// (quantum D8/D10).
    pub interrupt_latency_bound_ticks: u64,
    /// True for `Q` >= 2: the explicit ADR-0004 §6.3 reduced-accuracy request.
    pub reduced_accuracy: bool,
    /// The cumulative `D3` sync count for the current Platform generation.
    pub syncs: u64,
    /// Whether quantum D5's threshold test passed at this boundary.
    pub reached: bool,
}

/// The declared interrupt-latency bound of a selected quantum (quantum D10).
///
/// It is zero for `Q` = 1 and `Q - 1 + t` otherwise.  The build-time quantum
/// validation guarantees the sum is representable.
pub(crate) fn interrupt_latency_bound_ticks(quantum: u64, turn_bound: u64) -> u64 {
    if quantum == 1 {
        0
    } else {
        quantum - 1 + turn_bound
    }
}

/// What an idle wait did. The jump itself retires nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitSettlement {
    /// The hart was not waiting.
    NotWaiting,
    /// `mtime` moved to the next timer compare.
    Jumped,
    /// No locally enabled interrupt can ever become pending.
    NoProgress,
    /// `mtime` stopped at the virtual-time deadline, before the next compare.
    Deadline,
}

/// Facts from debug service at one Hart boundary.
///
/// Debug work has no guest turn, so these facts carry their own provenance:
/// a Platform effect or simulator failure produced by a debug instruction, or
/// by debugger DMI, since the previous Hart boundary. The Runner classifies a
/// terminal effect without inventing guest retirement.
#[derive(Debug, Default)]
pub(crate) struct DebugBoundary {
    pub(crate) events: Vec<PlatformEvent>,
    pub(crate) failure: Option<crate::core::SimulatorFailure>,
}
impl DebugBoundary {
    pub(crate) fn merge(&mut self, other: Self) {
        self.events.extend(other.events);
        if self.failure.is_none() {
            self.failure = other.failure;
        }
    }
}

/// A completed boundary. No mutable core/port/device capability escapes.
/// Keep this receipt through sink delivery and causal accounting, then drop it
/// before the next turn or drain acknowledgment. Historical fact copies are not
/// an attached asynchronous delivery queue.
pub struct MachineTurn {
    hart: HartTransition,
    events: Vec<PlatformEvent>,
    tohost: Option<TohostSample>,
    boundary: BoundaryFacts,
    /// A wake turn was refused because its conservative bound did not fit the
    /// configured virtual deadline (ADR-0004 §7.2).
    wake_blocked: bool,
    _receipt: Lease,
}
impl MachineTurn {
    pub fn boundary(&self) -> &BoundaryFacts {
        &self.boundary
    }
    /// True when an idle jump settled before the deadline but the wake turn's
    /// conservative completion bound did not fit it, so no turn started.
    pub(crate) const fn wake_blocked(&self) -> bool {
        self.wake_blocked
    }
    /// Attach downstream reporting context and the lifecycle facts now visible
    /// after delivery, without changing the immutable completed Hart record.
    pub(crate) fn finish_reporting(&mut self, error: Option<String>) -> Result<(), MachineError> {
        self.boundary.reporting_error = error;
        let (lifecycle, uncertain) = self._receipt.boundary_state()?;
        self.boundary.lifecycle = lifecycle;
        self.boundary.uncertain = uncertain;
        Ok(())
    }
    pub fn hart(&self) -> &HartTransition {
        &self.hart
    }
    pub fn events(&self) -> &[PlatformEvent] {
        &self.events
    }
    pub fn tohost(&self) -> Option<&TohostSample> {
        self.tohost.as_ref()
    }
    /// The receipt covers the entire callback, even if another control clone
    /// requests quiesce. Errors leave all completed Hart/Platform facts intact.
    pub fn deliver<S: ObservationSink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        self.hart.deliver(sink)
    }
}

#[derive(Debug)]
pub struct MachineInspection {
    pub generation: u64,
    pub image: Arc<LoadImage>,
    pub hart: CoreState,
    pub uart: Option<UartInspection>,
    pub tohost: TohostSample,
    pub events: Vec<PlatformEvent>,
    pub signature: Option<Result<Vec<u8>, String>>,
}
/// Acknowledgment and causal Platform facts at proven synchronous drain.
#[derive(Debug)]
pub struct DrainReport {
    pub generation: u64,
    pub lifecycle: Lifecycle,
    pub tohost: Option<TohostSample>,
    pub events: Vec<PlatformEvent>,
}
#[derive(Debug, Clone)]
pub struct MachineStatus {
    pub lifecycle: Lifecycle,
    pub generation: u64,
    pub work: WorkCounts,
    pub uncertain: bool,
    pub failed: bool,
    /// Original failure diagnostic, retained without attempting another turn.
    pub last_failure: Option<crate::core::SimulatorFailure>,
}
struct Installed {
    core: RiscvCore,
    platform: Platform,
    image: Arc<LoadImage>,
    /// The Machine's selected temporal-coupling quantum in Platform ticks
    /// (quantum D2/D3); zero is off.
    quantum: u64,
    /// Stops an idle jump when it is earlier than the next `mtimecmp`.
    /// Not a CLI flag.
    virtual_deadline: Option<u64>,
    #[cfg(test)]
    before_turn: Option<Arc<dyn Fn() + Send + Sync>>,
    /// quantum D9: see [`TimeLedger`].
    #[cfg(test)]
    ledger: std::cell::Cell<TimeLedger>,
}
impl Installed {
    fn build(config: &MachineConfig, image: Arc<LoadImage>) -> Result<Self, MachineError> {
        let platform = Platform::build(config, &image)?;
        let mut core = RiscvCore::new_with_physical_access(
            platform.typed.clone(),
            platform.typed.clone(),
            platform.fetch.clone(),
            platform.data.clone(),
        );
        // The Machine owns the ISA profile and passes it to the Hart at
        // construction (contract D3, ADR-0003 2026-11-03).
        core.set_isa_profile(config.isa);
        if config.platform == PlatformKind::Native {
            core.set_physical_storage_alignment(image.base_addr(), image.memory_size());
        }
        core.set_verbose(config.verbose);
        core.reset(image.entry_point(), platform.form.core_translation_base());
        Ok(Self {
            core,
            platform,
            image,
            quantum: config.quantum,
            virtual_deadline: None,
            #[cfg(test)]
            before_turn: None,
            #[cfg(test)]
            ledger: std::cell::Cell::new(TimeLedger::default()),
        })
    }
    /// quantum D9: the generation's conservation ledger, for an in-crate check
    /// against `C` (`OwnedMachine::platform_mtime`).
    #[cfg(test)]
    fn ledger(&self) -> TimeLedger {
        self.ledger.get()
    }
    /// quantum D9: a guest or debug `mtime` store rebases the ledger.
    #[cfg(test)]
    fn reconcile_ledger(&mut self) -> Result<(), MachineError> {
        if let Some(committed) = self.platform.take_mtime_store()? {
            let mut ledger = self.ledger.get();
            ledger.rebase(committed);
            self.ledger.set(ledger);
        }
        Ok(())
    }
    /// quantum D9: an in-crate `mtime` store rebases the ledger.
    #[cfg(test)]
    fn rebase_ledger(&mut self, committed: u64) {
        let mut ledger = self.ledger.get();
        ledger.rebase(committed);
        self.ledger.set(ledger);
    }
    /// Sole Machine-to-Hart invocation used by shared controls and facades.
    fn turn(
        &mut self,
        lease: Lease,
        observe: bool,
        sample: bool,
        single_step: bool,
    ) -> Result<MachineTurn, MachineError> {
        #[cfg(test)]
        if let Some(hook) = &self.before_turn {
            hook();
        }
        // `time` and the hart lines are the values from the previous boundary.
        // A CLINT or PLIC write below shows up on the next boundary
        // (privileged §3.2.1, contract D1). This pre-fetch sample is the only
        // acceptance sample. An idle jump republishes and uses that same
        // sample again. It is not a second Hart entry point.
        self.platform.begin_transport_turn()?;
        let mut idle_stop = (false, false);
        let mut jumped = false;
        let mut wake_blocked = false;
        // Whether a boundary sync moved committed time during this exchange,
        // so the turn-end publish re-presents it (quantum D3).
        let mut synced = false;
        let mut hart = loop {
            // quantum D3: each turn start presents the Hart's local time `L`,
            // which is committed time when the quantum is off.
            let mtime = self.platform.read_local_time()?;
            self.core.state_mut().csr.set_time(mtime);
            let lines = self.platform.machine_interrupt_lines()?;
            self.core
                .state_mut()
                .csr
                .set_machine_interrupt_lines(lines.msip, lines.mtip, lines.meip);
            self.core.state_mut().csr.set_seip_external(lines.seip);
            let hart = self.core.step_transition(observe);
            if !matches!(hart.outcome, StepOutcome::Waiting) || single_step || jumped {
                if jumped && matches!(hart.outcome, StepOutcome::Waiting) {
                    idle_stop = (true, false);
                }
                break hart;
            }
            // quantum F3: a waiting turn syncs ahead of the wake check, so
            // `settle_wait` and the idle jump see committed local time and
            // the jump lands with a zero offset.
            if self.quantum != 0 {
                synced |= self.platform.commit_local_time(!synced)?;
            }
            match self.settle_wait() {
                WaitSettlement::NoProgress => {
                    idle_stop = (true, false);
                    break hart;
                }
                WaitSettlement::Deadline => {
                    idle_stop = (false, true);
                    break hart;
                }
                WaitSettlement::Jumped | WaitSettlement::NotWaiting => {
                    // A settlement at or after the inclusive deadline admits the
                    // event due there but starts no normal turn (ADR-0004 §7.2),
                    // so the exchange returns `DeadlineReached` instead of a
                    // wake turn the granted horizon did not reserve.
                    if self.reached_virtual_deadline() {
                        idle_stop = (false, true);
                        break hart;
                    }
                    // The idle jump consumed slack the exchange was granted for
                    // the waiting turn only.  A wake turn may begin only when its
                    // own conservative completion bound still fits the deadline
                    // (ADR-0004 §7.2); otherwise the exchange ends here and the
                    // host re-evaluates the bound before any further work.
                    if !self.wake_turn_fits() {
                        wake_blocked = true;
                        break hart;
                    }
                    jumped = true;
                }
            }
        };
        let waiting = matches!(hart.outcome, StepOutcome::Waiting);
        let debug_without_turn = matches!(
            hart.outcome,
            StepOutcome::DebugHalted {
                completed_turn: false,
                ..
            }
        );
        let outcome_failed = matches!(hart.outcome, StepOutcome::SimulatorFailure(_));
        let uncertain = self.core.unresolved_physical_access().is_some();
        let completed_turn = !(outcome_failed || waiting || debug_without_turn);
        // A completed turn contributes one framework `iss_tick` plus the
        // converted transport delay; a started failed turn contributes only
        // its known delay and no turn tick (ADR-0004 §5.3/§5.1).  Delay never
        // enters `mcycle` or `minstret`.
        let delay = self.platform.take_transport_turn_delay();
        let advance = delay.ticks + u64::from(completed_turn);
        // quantum D9: a guest store to `mtime` during this turn is a rebase that
        // starts the identity at the stored value, before this turn's charges.
        #[cfg(test)]
        self.reconcile_ledger()?;
        let events = if completed_turn {
            self.platform.take_events()?
        } else {
            Vec::new()
        };
        let mut clock_failure = None;
        let mut quantum_reached = false;
        if self.quantum == 0 {
            if advance != 0 {
                // Native and zero-delay turns keep the existing wrapping one-tick
                // path.  A transport-derived advance is checked, and an overflow
                // becomes a simulator failure that retains the completed facts
                // and the already-accounted delay instead of rewinding time.
                let advanced = if delay.ticks == 0 {
                    self.platform.advance_mtime()
                } else {
                    self.platform.advance_mtime_by(advance)
                };
                match advanced {
                    // The instruction read the pre-turn snapshot. Park `time`
                    // and the machine lines on the post-tick device so a host
                    // inspect matches the store plus the tick.
                    Ok(_) => self.publish_platform_time()?,
                    Err(error) => clock_failure = Some(error),
                }
            }
        } else {
            // quantum D4: with a quantum selected the same once-converted
            // `advance` enters the Platform's local offset `O` in place of
            // committed time.  The two overflow rules apply to `L` at the same
            // point: a turn with no delay ticks wraps, a turn with delay ticks
            // is checked, and a failed check is today's simulator failure,
            // retaining the completed facts.  That turn's advance enters
            // neither `C` nor `O`.
            if advance != 0 {
                let checked = delay.ticks != 0;
                if let Err(error) = self.platform.accumulate_local_offset(advance, checked) {
                    clock_failure = Some(error);
                }
            }
            // quantum F10: a zero-delay turn that left local time below
            // committed time wrapped past `u64::MAX`.
            let wrapped = advance != 0
                && delay.ticks == 0
                && self.platform.read_local_time()? < self.platform.read_mtime()?;
            // quantum D5: the threshold test runs first, on `O` after the
            // advance, so a coincident forced sync cannot clear `reached`.
            // A refused advance left `O` unchanged, so the test still reads
            // the pre-turn offset, which D5 keeps below `Q`.
            quantum_reached = self.platform.local_offset()? >= self.quantum;
            if quantum_reached {
                synced |= self.platform.commit_local_time(!synced)?;
            }
            // quantum F4: a debug halt of either form ends the turn in a sync,
            // so a debugger and its DMI work see parity time.
            if matches!(hart.outcome, StepOutcome::DebugHalted { .. }) {
                synced |= self.platform.commit_local_time(!synced)?;
            }
            // quantum F1: a marked turn ends with a sync, even when the flush
            // found `O` zero and this turn added nothing, so `C` = `L` and
            // `E` is re-anchored before the boundary returns.
            if self.platform.device_access_marked()? {
                synced |= self.platform.commit_local_time(!synced)?;
            }
            // quantum F6: a failed turn, including one that fails only this
            // D4 clock check, syncs the offset it knows, including known
            // delay, without touching the transport.
            if outcome_failed || clock_failure.is_some() {
                synced |= self.platform.commit_local_time(!synced)?;
            } else if single_step {
                // quantum F5: a single-step turn ends in a sync, so every
                // shared Machine control runs at a zero offset.
                synced |= self.platform.commit_local_time(!synced)?;
            }
            if wrapped {
                // quantum F10: the wrapped value presents as in a parity run,
                // so committed time takes the wrapped offset at turn end and
                // MTIP is never stale high.
                synced |= self.platform.commit_local_time(!synced)?;
            }
            if synced || (clock_failure.is_none() && advance != 0) {
                self.publish_platform_time()?;
            }
        }
        // quantum D9: charged ticks, the completed tick and no idle jump here.
        // A turn refused by the D4 clock check adds nothing.
        #[cfg(test)]
        if clock_failure.is_none() {
            let mut ledger = self.ledger.get();
            ledger.charge_turn(delay.ticks, completed_turn);
            self.ledger.set(ledger);
        }
        if let Some(error) = clock_failure {
            // Preserve a transport-failure diagnostic already produced
            // by this turn and append the clock failure, so the
            // original overshoot/maximum context is not lost.
            match &mut hart.outcome {
                StepOutcome::SimulatorFailure(existing) => {
                    existing.message.push_str("; clock advance also failed: ");
                    existing.message.push_str(&error.to_string());
                }
                _ => {
                    hart.outcome = StepOutcome::SimulatorFailure(crate::core::SimulatorFailure {
                        pc: self.core.state().pc,
                        kind: crate::core::SimulatorFailureKind::Invariant,
                        message: format!("Platform time: {error}"),
                    });
                }
            }
        }
        let failure = match &hart.outcome {
            StepOutcome::SimulatorFailure(failure) => Some(failure.clone()),
            StepOutcome::DebugHalted { .. }
            | StepOutcome::InstructionRetired(_)
            | StepOutcome::TrapEntered(_)
            | StepOutcome::Waiting => None,
        };
        let failed = failure.is_some();
        let tohost = (sample && !failed).then(|| self.sample());
        let receipt = lease.boundary(uncertain, failure)?;
        let (lifecycle, uncertain) = receipt.boundary_state()?;
        Ok(MachineTurn {
            hart,
            events,
            tohost,
            boundary: BoundaryFacts {
                single_step,
                lifecycle,
                uncertain,
                waiting,
                no_progress: idle_stop.0,
                deadline_stop: idle_stop.1,
                transport_delay_femtoseconds: self.platform.transport_delay_femtoseconds(),
                quantum: self.quantum_facts(quantum_reached)?,
                ..BoundaryFacts::default()
            },
            wake_blocked,
            _receipt: receipt,
        })
    }
    /// quantum D10: this boundary's quantum facts, present only when a
    /// quantum is selected.  `reached` is the D5 threshold test's result.
    fn quantum_facts(&self, reached: bool) -> Result<Option<QuantumFacts>, MachineError> {
        if self.quantum == 0 {
            return Ok(None);
        }
        Ok(Some(QuantumFacts {
            ticks: self.quantum,
            interrupt_latency_bound_ticks: interrupt_latency_bound_ticks(
                self.quantum,
                self.platform.turn_completion_bound_ticks().unwrap_or(0),
            ),
            reduced_accuracy: self.quantum >= 2,
            syncs: self.platform.quantum_syncs()?,
            reached,
        }))
    }
    /// Publish the post-advance local time `L` and the machine lines derived
    /// from committed time (quantum D3).
    fn publish_platform_time(&mut self) -> Result<(), MachineError> {
        let mtime = self.platform.read_local_time()?;
        self.core.state_mut().csr.set_time(mtime);
        let lines = self.platform.machine_interrupt_lines()?;
        self.core
            .state_mut()
            .csr
            .set_machine_interrupt_lines(lines.msip, lines.mtip, lines.meip);
        self.core.state_mut().csr.set_seip_external(lines.seip);
        Ok(())
    }

    /// True when one more turn's conservative completion bound fits the
    /// configured virtual deadline.
    ///
    /// The reserve is one completed turn plus the selected RAM target's declared
    /// per-transaction maximum for the turn's worst-case transaction count.  A
    /// Machine without a virtual deadline has no such bound to honor.
    fn wake_turn_fits(&self) -> bool {
        let Some(deadline) = self.virtual_deadline else {
            return true;
        };
        let Some(reserve) = self
            .platform
            .turn_completion_bound_ticks()
            .unwrap_or(0)
            .checked_add(1)
        else {
            return false;
        };
        self.platform.read_mtime().is_ok_and(|mtime| {
            mtime
                .checked_add(reserve)
                .is_some_and(|end| end <= deadline)
        })
    }

    /// True when the cursor has reached the configured virtual deadline.
    ///
    /// The deadline is an inclusive completion bound: an exchange may settle on
    /// it and admit the event due there, but it starts no normal turn.
    fn reached_virtual_deadline(&self) -> bool {
        let Some(deadline) = self.virtual_deadline else {
            return false;
        };
        self.platform
            .read_mtime()
            .is_ok_and(|mtime| mtime >= deadline)
    }

    /// Jump `mtime` to the next wake, or stop. Does not retire an instruction.
    /// Privileged §3.3.3 wakes on `mip & mie` only. The only future event
    /// this profile can raise while the hart waits is `mtime` reaching
    /// `mtimecmp` with MTIE set.
    fn settle_wait(&mut self) -> WaitSettlement {
        const MTIE: u64 = 1 << 7;
        let Ok(mtime) = self.platform.read_mtime() else {
            return WaitSettlement::NoProgress;
        };
        let timer = if self.core.state().csr.mie() & MTIE == 0 {
            None
        } else {
            match self.platform.read_mtimecmp() {
                Ok(Some(compare)) if compare > mtime => Some(compare),
                _ => None,
            }
        };
        let deadline = self.virtual_deadline;
        let (target, settlement) = match (timer, deadline) {
            (Some(compare), Some(deadline)) if deadline < compare => {
                (deadline, WaitSettlement::Deadline)
            }
            (Some(compare), _) => (compare, WaitSettlement::Jumped),
            (None, Some(deadline)) => (deadline, WaitSettlement::Deadline),
            (None, None) => return WaitSettlement::NoProgress,
        };
        // A deadline that is not strictly ahead of `mtime` has already
        // arrived. Stop there. Do not store a smaller `mtime`.
        if settlement == WaitSettlement::Deadline && target <= mtime {
            return WaitSettlement::Deadline;
        }
        if self.platform.set_mtime(target).is_err() {
            return WaitSettlement::NoProgress;
        }
        // quantum D9: the idle jump is charged as its distance in ticks.
        #[cfg(test)]
        {
            let mut ledger = self.ledger.get();
            ledger.charge_jump(target.saturating_sub(mtime));
            self.ledger.set(ledger);
        }
        let _ = self.platform.read_mtime().map(|mtime| {
            self.core.state_mut().csr.set_time(mtime);
        });
        settlement
    }

    fn sample(&self) -> TohostSample {
        TohostSample {
            address: self.platform.selected_tohost,
            value: self.platform.tohost_value(),
        }
    }
}
fn acknowledge_drain(
    admission: &mut Admission,
    installed: Option<&Installed>,
) -> Result<DrainReport, MachineError> {
    admission.drain_allowed()?;
    let (tohost, events) = match installed {
        Some(domain) => {
            // quantum F8: commit the local offset at this stopping point before
            // the drain facts are sampled. `quiesce` takes `&self` and this
            // holds `Option<&Installed>`, so it is the bus-lock update alone:
            // it moves `C`, zeroes `O`, re-anchors `E` and counts under D3, and
            // omits only the Hart CSR republish.
            domain.platform.commit_local_time(true)?;
            (Some(domain.sample()), domain.platform.take_events()?)
        }
        None => (None, Vec::new()),
    };
    admission.lifecycle = Lifecycle::DrainComplete;
    Ok(DrainReport {
        generation: admission.generation,
        lifecycle: Lifecycle::DrainComplete,
        tohost,
        events,
    })
}

#[derive(Clone)]
struct HostView {
    memory: SharedMemory,
    writer: HostMemory,
    tohost_offset: Option<u64>,
}
struct Inner {
    config: MachineConfig,
    gate: Gate,
    installed: Mutex<Option<Installed>>,
    host: Mutex<Option<HostView>>,
    work_changed: Condvar,
    debug: DebugHub,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Active leases own Arc<Inner>, so the last control drop cannot tear
        // down a running request/callback or an outstanding boundary receipt.
        let quarantine = match self.gate.lock() {
            Ok(mut state) => {
                let uncertain = state.uncertain || !state.work.is_empty();
                if !uncertain {
                    state.lifecycle = Lifecycle::TornDown;
                    state.generation = state.generation.saturating_add(1);
                }
                uncertain
            }
            Err(_) => true,
        };
        if quarantine {
            // Rust Drop cannot return the explicit teardown refusal. Retain the
            // failed domain rather than disconnect an unresolved transport by
            // fiat. This intentionally consumes resources until process exit;
            // it is not DrainComplete, recovery, or fresh reconstruction.
            let slot = self
                .installed
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(domain) = slot.take() {
                std::mem::forget(domain);
            }
        }
    }
}

/// Cloneable control handles refer to ONE owned Hart/Platform association.
/// The exclusive facade adapter retains the same composition/admission machinery;
/// the old typed Hart constructors remain explicit unmanaged compatibility routes.
#[derive(Clone)]
pub struct Machine {
    inner: Arc<Inner>,
}
impl Machine {
    pub fn new(config: MachineConfig) -> Self {
        Self {
            inner: Arc::new_cyclic(|owner| Inner {
                config,
                gate: Arc::new(Mutex::new(Admission {
                    owner: owner.clone(),
                    ..Admission::default()
                })),
                installed: Mutex::new(None),
                host: Mutex::new(None),
                work_changed: Condvar::new(),
                debug: DebugHub::new(),
            }),
        }
    }
    pub fn status(&self) -> Result<MachineStatus, MachineError> {
        let state = lock(&self.inner.gate)?;
        Ok(MachineStatus {
            lifecycle: state.lifecycle,
            generation: state.generation,
            work: state.work,
            uncertain: state.uncertain,
            failed: state.failed,
            last_failure: state.failure.clone(),
        })
    }
    /// Explicit image placement while drained; validation failure is atomic with
    /// respect to the existing composition. Replacing RAM isolates stale handles.
    pub fn install(&self, image: Arc<LoadImage>) -> Result<(), MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        admission.mutation_allowed()?;
        self.replace(&mut admission, image)
    }
    fn replace(
        &self,
        admission: &mut Admission,
        image: Arc<LoadImage>,
    ) -> Result<(), MachineError> {
        let installed = Installed::build(&self.inner.config, image)?;
        self.publish(admission, installed)
    }
    fn publish(
        &self,
        admission: &mut Admission,
        mut installed: Installed,
    ) -> Result<(), MachineError> {
        admission.mutation_allowed()?;
        let generation = admission
            .generation
            .checked_add(1)
            .ok_or_else(|| MachineError::Placement("generation exhausted".into()))?;
        let writer = HostMemory {
            ram: installed.platform.ram.clone(),
            gate: self.inner.gate.clone(),
            generation,
            size: installed.image.memory_size(),
            #[cfg(test)]
            before_lock: Arc::new(Mutex::new(None)),
        };
        let view = HostView {
            memory: Arc::new(Mutex::new(writer.clone())),
            writer,
            tohost_offset: installed.platform.ram_tohost_offset,
        };
        host_port::attach(
            &mut installed,
            view.memory.clone(),
            self.inner.config.platform,
        );
        // Acquire all publishing locks before changing any state.
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let dm = self
            .inner
            .debug
            .dm
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let mut host = self.inner.host.lock().map_err(|_| MachineError::Poisoned)?;
        dm.apply_haltreq(&mut installed.core);
        *domain = Some(installed);
        *host = Some(view);
        admission.generation = generation;
        admission.failed = false;
        admission.failure = None;
        Ok(())
    }
    /// Restore immutable installed image/zero fill and fresh Hart/devices/events.
    /// Static configuration/connections and image identity are preserved. This is
    /// NOT legacy core.reset() over mutated RAM, nor unknown-completion recovery.
    pub fn fresh_reset(&self) -> Result<(), MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        admission.mutation_allowed()?;
        let image = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .as_ref()
            .ok_or(MachineError::NoImage)?
            .image
            .clone();
        self.replace(&mut admission, image)
    }
    pub fn resume(&self) -> Result<(), MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        if admission.uncertain {
            return Err(MachineError::UnknownCompletion);
        }
        if admission.failed {
            return Err(MachineError::NeedsFreshReset);
        }
        if admission.lifecycle == Lifecycle::TornDown {
            return Err(MachineError::TornDown);
        }
        if !matches!(
            admission.lifecycle,
            Lifecycle::DrainComplete | Lifecycle::Running
        ) {
            return Err(MachineError::NotDrained);
        }
        if self
            .inner
            .host
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .is_none()
        {
            return Err(MachineError::NoImage);
        }
        admission.lifecycle = Lifecycle::Running;
        Ok(())
    }
    /// Virtual-time deadline for an idle jump. Earlier than the next
    /// `mtimecmp`, the jump stops there and retires nothing. Not a CLI flag.
    pub fn set_virtual_deadline(&self, deadline: Option<u64>) -> Result<(), MachineError> {
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let installed = domain.as_mut().ok_or(MachineError::NoImage)?;
        installed.virtual_deadline = deadline;
        Ok(())
    }

    /// Idle-jump a waiting hart. A single-step does not call this.
    /// `mtime` changes on [`WaitSettlement::Jumped`], and on
    /// [`WaitSettlement::Deadline`] only when that deadline is strictly
    /// ahead of the current `mtime`. A deadline that has already arrived
    /// stops the wait and leaves `mtime` where it is.
    pub fn settle_wait(&self) -> Result<WaitSettlement, MachineError> {
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let installed = domain.as_mut().ok_or(MachineError::NoImage)?;
        if !installed.core.is_wfi_waiting() {
            return Ok(WaitSettlement::NotWaiting);
        }
        Ok(installed.settle_wait())
    }

    /// Execute exactly one existing Hart transition; no Runner budget/exit policy.
    pub fn step(&self, observe: bool) -> Result<MachineTurn, MachineError> {
        let lease = Lease::hart(&self.inner.gate)?;
        let _instruction = debug_hub::InInstruction::enter(&self.inner.debug.in_instruction);
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let installed = domain.as_mut().ok_or(MachineError::NoImage)?;
        installed.turn(lease, observe, true, true)
    }
    /// Stop admission, including new host writes to the live generation. Already
    /// admitted writes/turns/callbacks/boundary deliveries are allowed to finish.
    pub fn request_quiesce(&self) -> Result<(), MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        if admission.lifecycle == Lifecycle::TornDown {
            return Err(MachineError::TornDown);
        }
        if admission.lifecycle != Lifecycle::DrainComplete {
            admission.lifecycle = Lifecycle::QuiesceRequested;
        }
        Ok(())
    }
    /// Nonblocking acknowledgment: counts AND synchronous completion/receipt
    /// consumption establish drain. Idle/NoProgress/run-return/timeout alone do
    /// not. No resolution API guesses that an unknown backend has terminated.
    pub fn try_drain(&self) -> Result<DrainReport, MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        admission.drain_allowed()?;
        let domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        acknowledge_drain(&mut admission, domain.as_ref())
    }
    /// Debugger updates between boundaries. The installed lock serializes with
    /// the hart turn, and the latch is sampled at the next `execute_turn`, so
    /// this path skips the admission gate. Gating it on drain completion would
    /// refuse a `haltreq` while the machine is still running.
    fn with_core_mut<T>(
        &self,
        update: impl FnOnce(&mut RiscvCore) -> T,
    ) -> Result<T, MachineError> {
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let core = &mut domain.as_mut().ok_or(MachineError::NoImage)?.core;
        Ok(update(core))
    }

    /// Set the DM's Hart 0 request and drive its Hart input.
    pub fn set_haltreq(&self, haltreq: bool) -> Result<(), MachineError> {
        self.with_core_mut(|core| {
            self.inner
                .debug
                .dm
                .lock()
                .map_err(|_| MachineError::Poisoned)?
                .set_haltreq(core, haltreq);
            Ok(())
        })?
    }

    pub fn dcsr(&self) -> Result<u64, MachineError> {
        self.with_core_mut(|core| core.dcsr())
    }

    /// Debugger write of `dcsr`. Returns the stored value.
    pub fn write_dcsr(&self, value: u64) -> Result<u64, MachineError> {
        self.with_core_mut(|core| core.write_dcsr(value))
    }

    pub fn dpc(&self) -> Result<u64, MachineError> {
        self.with_core_mut(|core| core.dpc())
    }

    /// Leave Debug Mode. See [`RiscvCore::debug_resume`].
    pub fn debug_resume(&self) -> Result<bool, MachineError> {
        self.with_core_mut(|core| core.debug_resume())
    }

    /// Coherent inspection under admission exclusion, not raw volatile handles.
    pub fn inspect(&self) -> Result<MachineInspection, MachineError> {
        let admission = lock(&self.inner.gate)?;
        if admission.uncertain {
            return Err(MachineError::UnknownCompletion);
        }
        if !admission.work.is_empty() {
            return Err(MachineError::Busy);
        }
        let domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let installed = domain.as_ref().ok_or(MachineError::NoImage)?;
        let signature = installed.platform.signature(&installed.image);
        Ok(MachineInspection {
            generation: admission.generation,
            image: installed.image.clone(),
            hart: installed.core.state().clone(),
            uart: installed.platform.uart_inspection()?,
            tohost: installed.sample(),
            events: installed.platform.queued_events()?,
            signature,
        })
    }
    /// Explicit drained control mutation from an owned inspection/edit value.
    /// Control edits invalidate reservations: opaque snapshots from another RAM
    /// generation cannot be imported. No mutable core reference escapes.
    pub fn set_hart_state(&self, mut state: CoreState) -> Result<(), MachineError> {
        let admission = lock(&self.inner.gate)?;
        admission.mutation_allowed()?;
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        state.reservation = None;
        *domain
            .as_mut()
            .ok_or(MachineError::NoImage)?
            .core
            .state_mut() = state;
        Ok(())
    }
    /// Cloneable storage-offset RAM view, including the native image RAM. It
    /// cannot reach devices/ports/callbacks. Current writes use admission; stale
    /// writes remain usable on detached RAM and cannot affect a fresh generation.
    pub fn memory(&self) -> Result<SharedMemory, MachineError> {
        Ok(self.host_view()?.memory)
    }
    fn host_view(&self) -> Result<HostView, MachineError> {
        self.inner
            .host
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .clone()
            .ok_or(MachineError::NoImage)
    }
    pub fn write_mem(&self, offset: u64, bytes: &[u8]) -> Result<(), MachineError> {
        self.host_view()?.writer.write_bytes(offset, bytes)?;
        Ok(())
    }
    /// Volatile host RAM read, not a coherent MachineInspection.
    pub fn read_mem(&self, offset: u64, size: usize) -> Result<Vec<u8>, MachineError> {
        Ok(self.host_view()?.writer.read_bytes(offset, size)?)
    }
    pub fn clear_tohost(&self) -> Result<(), MachineError> {
        let view = self.host_view()?;
        let offset = view.tohost_offset.ok_or(MachineError::NoRamTohost)?;
        let mut memory = view.writer;
        memory.write_dword(offset, 0)?;
        Ok(())
    }
    pub fn receive_uart(&self, byte: u8) -> Result<(), MachineError> {
        let generation = self.status()?.generation;
        let _lease = Lease::host(&self.inner.gate, generation)?.ok_or(MachineError::Busy)?;
        let domain = self
            .inner
            .installed
            .try_lock()
            .map_err(|_| MachineError::Busy)?;
        domain
            .as_ref()
            .ok_or(MachineError::NoImage)?
            .platform
            .receive_uart(byte)
    }
    pub fn teardown(&self) -> Result<(), MachineError> {
        let mut admission = lock(&self.inner.gate)?;
        admission.mutation_allowed()?;
        let generation = admission
            .generation
            .checked_add(1)
            .ok_or_else(|| MachineError::Placement("generation exhausted".into()))?;
        let mut domain = self
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        let mut host = self.inner.host.lock().map_err(|_| MachineError::Poisoned)?;
        *domain = None;
        *host = None;
        admission.generation = generation;
        admission.lifecycle = Lifecycle::TornDown;
        Ok(())
    }
}

/// Cloneable DMI endpoint. Transactions are applied on the hart owner at a
/// Hart boundary, not on this thread and not mid-instruction.
pub struct DebugPort {
    inner: Arc<Inner>,
}

impl DebugPort {
    /// Read one DMI register. The low 7 bits are the address.
    pub fn dmi_read(&self, addr: u8) -> Result<u32, MachineError> {
        self.inner.debug.post(addr, None)
    }

    /// Write one DMI register. The low 7 bits are the address.
    pub fn dmi_write(&self, addr: u8, value: u32) -> Result<(), MachineError> {
        self.inner.debug.post(addr, Some(value)).map(|_| ())
    }

    /// Completed Hart turns the attached run has counted. This is the value
    /// `ExecutionResult.cycles` will report. It does not move while the hart
    /// is halted.
    pub fn completed_turns(&self) -> u64 {
        self.inner.debug.completed_turns()
    }
}

impl Clone for DebugPort {
    fn clone(&self) -> Self {
        self.inner.debug.acquire_port();
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for DebugPort {
    fn drop(&mut self) {
        if self.inner.debug.release_port() {
            self.inner.debug.close();
        }
    }
}

/// Wakes a debugger blocked in [`DebugPort`] when the run returns.
pub(crate) struct DebugSessionGuard {
    inner: Arc<Inner>,
}

impl Drop for DebugSessionGuard {
    fn drop(&mut self) {
        if self.inner.debug.attached() {
            self.inner.debug.close();
        }
    }
}

#[cfg(test)]
mod tests;
