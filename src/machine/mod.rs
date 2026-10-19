//! N=1 composition and explicit lifecycle, separate from Runner budgets/results.
//! Shared and exclusive ownership adapters use the same composition/turn path.
//! Returned boundary receipts
//! keep accepted control/observation work admitted until consumed (dropped).
mod admission;
mod host_memory;
mod host_port;
mod owned;
mod platform;
pub(crate) use owned::OwnedMachine;
#[cfg(test)]
pub(crate) use owned::SignalOperation;

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

/// Static topology is restricted to the existing native bus or flat RAM.
/// Callbacks are synchronous host connections, not asynchronous transports.
#[derive(Clone)]
pub struct MachineConfig {
    pub platform: PlatformKind,
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
    #[error("this configuration has no UART")]
    NoUart,
    #[error("no RAM-backed selected tohost")]
    NoRamTohost,
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
        }
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

/// A completed boundary. No mutable core/port/device capability escapes.
/// Keep this receipt through sink delivery and causal accounting, then drop it
/// before the next turn or drain acknowledgment. Historical fact copies are not
/// an attached asynchronous delivery queue.
pub struct MachineTurn {
    hart: HartTransition,
    events: Vec<PlatformEvent>,
    tohost: Option<TohostSample>,
    boundary: BoundaryFacts,
    _receipt: Lease,
}
impl MachineTurn {
    pub fn boundary(&self) -> &BoundaryFacts {
        &self.boundary
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
    /// Stops an idle jump when it is earlier than the next `mtimecmp`.
    /// Not a CLI flag.
    virtual_deadline: Option<u64>,
    #[cfg(test)]
    before_turn: Option<Arc<dyn Fn() + Send + Sync>>,
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
        if config.platform == PlatformKind::Native {
            core.set_physical_storage_alignment(image.base_addr(), image.memory_size());
        }
        core.set_verbose(config.verbose);
        core.reset(image.entry_point(), platform.form.core_translation_base());
        Ok(Self {
            core,
            platform,
            image,
            virtual_deadline: None,
            #[cfg(test)]
            before_turn: None,
        })
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
        let mut idle_stop = (false, false);
        let mut jumped = false;
        let hart = loop {
            let mtime = self.platform.read_mtime()?;
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
            match self.settle_wait() {
                WaitSettlement::NoProgress => {
                    idle_stop = (true, false);
                    break hart;
                }
                WaitSettlement::Deadline => {
                    idle_stop = (false, true);
                    break hart;
                }
                WaitSettlement::Jumped | WaitSettlement::NotWaiting => jumped = true,
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
        let failure = match &hart.outcome {
            StepOutcome::SimulatorFailure(failure) => Some(failure.clone()),
            StepOutcome::DebugHalted { .. }
            | StepOutcome::InstructionRetired(_)
            | StepOutcome::TrapEntered(_)
            | StepOutcome::Waiting => None,
        };
        let failed = failure.is_some();
        let uncertain = self.core.unresolved_physical_access().is_some();
        let events = if failed || waiting || debug_without_turn {
            Vec::new()
        } else {
            let events = self.platform.take_events()?;
            let mtime = self.platform.advance_mtime()?;
            // The instruction read the pre-turn snapshot. Park `time` and the
            // machine lines on the post-tick device so a host inspect matches
            // the store plus the tick.
            self.core.state_mut().csr.set_time(mtime);
            let lines = self.platform.machine_interrupt_lines()?;
            self.core
                .state_mut()
                .csr
                .set_machine_interrupt_lines(lines.msip, lines.mtip, lines.meip);
            self.core.state_mut().csr.set_seip_external(lines.seip);
            events
        };
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
                ..BoundaryFacts::default()
            },
            _receipt: receipt,
        })
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
        Some(domain) => (Some(domain.sample()), domain.platform.take_events()?),
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
        let mut host = self.inner.host.lock().map_err(|_| MachineError::Poisoned)?;
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
    /// Coherent inspection under admission exclusion, not raw volatile handles.
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

    /// Set the Hart `haltreq` latch. Debug 1.0 `dmcontrol.haltreq`.
    pub fn set_haltreq(&self, haltreq: bool) -> Result<(), MachineError> {
        self.with_core_mut(|core| core.set_haltreq(haltreq))
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
        Ok(self
            .host_view()?
            .writer
            .ram
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .read_bytes(offset, size)?)
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

#[cfg(test)]
mod tests;
