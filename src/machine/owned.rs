//! Exclusive borrowing adapter for the compatibility facade. The actual installed
//! Hart is owned here, never copied or exposed as a core. Shared host admission,
//! composition, transition and last-owner quarantine remain the T2 mechanisms.
use super::*;

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignalOperation {
    Poll,
    Clear,
    Signature,
}
#[cfg(test)]
type BeforeSignal = Arc<dyn Fn(SignalOperation) + Send + Sync>;
#[cfg(test)]
type SignalLockProbe = Arc<dyn Fn(SignalOperation, bool) + Send + Sync>;
#[cfg(test)]
#[derive(Default)]
struct SignalHooks {
    before: Option<BeforeSignal>,
    probe: Option<SignalLockProbe>,
    after_clear: Option<Arc<dyn Fn() + Send + Sync>>,
}

pub(crate) struct OwnedMachine {
    control: Machine,
    installed: Option<Installed>,
    memory: SharedMemory,
    edited: bool,
    edit_reservation: Option<crate::isa::rv64a::lr_sc::ReservationSet>,
    #[cfg(test)]
    before_drain: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    signal_hooks: SignalHooks,
}
impl OwnedMachine {
    pub(crate) fn new(config: MachineConfig, image: Arc<LoadImage>) -> Result<Self, MachineError> {
        let control = Machine::new(config);
        control.install(image)?;
        let installed = control
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .take();
        let memory = control.memory()?;
        control.resume()?;
        Ok(Self {
            control,
            installed,
            memory,
            edited: false,
            edit_reservation: None,
            #[cfg(test)]
            before_drain: None,
            #[cfg(test)]
            signal_hooks: SignalHooks::default(),
        })
    }
    fn domain(&self) -> &Installed {
        self.installed
            .as_ref()
            .expect("exclusive installed Machine")
    }
    fn domain_mut(&mut self) -> &mut Installed {
        self.installed
            .as_mut()
            .expect("exclusive installed Machine")
    }
    pub(crate) fn state(&self) -> &CoreState {
        self.domain().core.state()
    }
    pub(crate) fn memory(&self) -> &SharedMemory {
        &self.memory
    }
    pub(crate) fn image(&self) -> &LoadImage {
        &self.domain().image
    }
    pub(crate) fn set_verbose(&mut self, verbose: bool) {
        self.domain_mut().core.set_verbose(verbose);
    }
    pub(crate) fn select_signal(&mut self, address: u64) {
        if self.control.status().is_ok_and(|status| !status.uncertain) {
            self.domain_mut().platform.selected_tohost = address;
        }
    }

    /// Control edits explicitly fence and finish already admitted host work.
    /// The actual borrowed state is retained, not a facade snapshot. The next
    /// facade operation reopens ordinary admission after the borrow has ended.
    pub(crate) fn state_mut(&mut self) -> &mut CoreState {
        self.quiesce()
            .expect("cannot edit failed/undrained Machine state");
        if !self.edited {
            self.edit_reservation = self.state().reservation.clone();
        }
        self.edited = true;
        self.domain_mut().core.state_mut()
    }
    pub(crate) fn admit(&self) -> Result<(), MachineError> {
        self.control.resume()
    }
    pub(crate) fn step(&mut self, observe: bool) -> Result<MachineTurn, MachineError> {
        self.step_request(observe, true, false)
    }
    pub(crate) fn step_request(
        &mut self,
        observe: bool,
        single_step: bool,
        last_slot: bool,
    ) -> Result<MachineTurn, MachineError> {
        self.admit()?;
        if self.edited {
            // Borrowed compatibility edits may retain the current reservation,
            // but cannot introduce an opaque LR snapshot from another domain.
            if self.state().reservation != self.edit_reservation {
                self.domain_mut().core.state_mut().reservation = None;
            }
            self.edited = false;
            self.edit_reservation = None;
        }
        let lease = Lease::hart(&self.control.inner.gate)?;
        // Native Runner requests the selected signal lazily after callback facts.
        let mut turn = self.domain_mut().turn(lease, observe, false)?;
        if matches!(&turn.hart.outcome, StepOutcome::SimulatorFailure(failure)
            if failure.kind == crate::core::SimulatorFailureKind::UnsupportedLegalInstruction
                && !turn.boundary.uncertain)
        {
            // Legacy facade permits host correction and a NEW run/step request
            // after an unsupported legal operation. It has no uncertain physical
            // completion, and is still a failed slot with no observation/exit.
            // Never apply this compatibility rule to transport/host failures.
            lock(&self.control.inner.gate)?.failed = false;
        }
        turn.boundary.single_step = single_step;
        turn.boundary.budget_exhausted =
            last_slot && !matches!(turn.hart.outcome, StepOutcome::SimulatorFailure(_));
        Ok(turn)
    }
    pub(crate) fn control_boundary(
        &self,
        budget_exhausted: bool,
    ) -> Result<BoundaryFacts, MachineError> {
        let status = self.control.status()?;
        Ok(BoundaryFacts {
            budget_exhausted,
            lifecycle: status.lifecycle,
            uncertain: status.uncertain,
            ..BoundaryFacts::default()
        })
    }
    #[cfg(test)]
    pub(crate) fn test_data(&self) -> crate::core::SharedDataAccess {
        self.domain().platform.data.clone()
    }
    #[cfg(test)]
    pub(crate) fn test_control(&self) -> Machine {
        self.control.clone()
    }
    #[cfg(test)]
    pub(crate) fn test_before_drain(&mut self, hook: Arc<dyn Fn() + Send + Sync>) {
        self.before_drain = Some(hook);
    }
    #[cfg(test)]
    pub(crate) fn test_replace_data(&mut self, data: crate::core::SharedDataAccess) {
        let fetch = self.domain().platform.fetch.clone();
        self.domain_mut().core.set_physical_access(fetch, data);
    }
    #[cfg(test)]
    pub(crate) fn test_before_turn(&mut self, hook: Arc<dyn Fn() + Send + Sync>) {
        self.domain_mut().before_turn = Some(hook);
    }
    #[cfg(test)]
    pub(crate) fn test_before_host(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        let view = self.control.host_view().unwrap();
        *view.writer.before_lock.lock().unwrap() = Some(hook);
    }
    #[cfg(test)]
    pub(crate) fn test_signal_hooks(
        &mut self,
        before: BeforeSignal,
        probe: SignalLockProbe,
        after_clear: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.signal_hooks = SignalHooks {
            before: Some(before),
            probe: Some(probe),
            after_clear: Some(after_clear),
        };
    }
    #[cfg(test)]
    fn probe_signal_lock(&self, operation: SignalOperation, memory: &SharedMemory) {
        if let Some(before) = &self.signal_hooks.before {
            before(operation);
        }
        if let Some(probe) = &self.signal_hooks.probe {
            // Probe only the actual selected mutex, dropping any acquired guard
            // before the normal blocking acquisition. Never used as drain proof.
            let contended = matches!(memory.try_lock(), Err(std::sync::TryLockError::WouldBlock));
            probe(operation, contended);
        }
    }
    pub(crate) fn failure(&self) -> Option<crate::core::SimulatorFailure> {
        self.control
            .status()
            .ok()
            .and_then(|status| status.last_failure)
    }
    fn signal_memory(&self) -> &SharedMemory {
        match self.control.inner.config.platform {
            // Preserve the flat facade's public clone-guard serialization.
            PlatformKind::Flat => &self.memory,
            // Native bus/device callbacks retain their independent RAM writers.
            PlatformKind::Native => &self.domain().platform.typed,
        }
    }
    pub(crate) fn observe_tohost(&self, turn: &mut MachineTurn, address: u64) {
        let signal_memory = self.signal_memory();
        #[cfg(test)]
        self.probe_signal_lock(SignalOperation::Poll, signal_memory);
        turn.tohost = Some(TohostSample {
            address,
            value: signal_memory
                .lock()
                .map_err(|_| MemoryError::Backend("poisoned Platform".into()))
                .and_then(|memory| memory.read_dword(address)),
        });
    }
    /// Decode belongs to Runner; accepted post-boundary clearing stays within
    /// the receipt and the same Platform/version domain. Retain byte-clear policy.
    /// Flat acquires the public capability before RAM, with no admission gate
    /// held. Write through the owned storage beneath that guard, not HostMemory:
    /// this already accepted receipt may finish even after quiesce stops new work.
    pub(crate) fn clear_signal(&self, _turn: &MachineTurn, address: u64, verbose: bool) {
        let signal_memory = self.signal_memory();
        #[cfg(test)]
        self.probe_signal_lock(SignalOperation::Clear, signal_memory);
        let result = (|| -> Result<(), MemoryError> {
            let _serialization =
                if self.control.inner.config.platform == PlatformKind::Flat {
                    Some(signal_memory.lock().map_err(|_| {
                        MemoryError::Backend("poisoned host memory capability".into())
                    })?)
                } else {
                    None
                };
            let mut memory = self
                .domain()
                .platform
                .typed
                .lock()
                .map_err(|_| MemoryError::Backend("poisoned Platform".into()))?;
            for i in 0..8 {
                let result = address
                    .checked_add(i)
                    .ok_or(MemoryError::InvalidAddress(address))
                    .and_then(|address| memory.write_byte(address, 0));
                if let Err(error) = result {
                    if verbose {
                        eprintln!("[WARN] Failed to clear tohost byte {}: {}", i, error);
                    }
                }
            }
            Ok(())
        })();
        if result.is_err() && verbose {
            eprintln!("[WARN] Failed to lock memory for clear_tohost");
        }
        #[cfg(test)]
        if let Some(after_clear) = &self.signal_hooks.after_clear {
            after_clear();
        }
    }
    pub(crate) fn signature(&self) -> Option<Result<Vec<u8>, String>> {
        let info = self.image().signature()?;
        if info.size == 0 {
            return Some(Ok(Vec::new()));
        }
        #[cfg(test)]
        self.probe_signal_lock(SignalOperation::Signature, self.signal_memory());
        // A flat clone can retain its guard between individually admitted writes.
        // Acquire that guard BEFORE the admission gate: the holder must remain
        // able to admit and finish its remaining writes while extraction waits.
        let _serialization = if self.control.inner.config.platform == PlatformKind::Flat {
            Some(match self.memory.lock() {
                Ok(memory) => memory,
                Err(_) => return Some(Err("poisoned host memory capability".into())),
            })
        } else {
            None
        };
        // Keep the existing coherent-inspection checks as well: raw admitted
        // owner writers need not hold the facade mutex. No drain is inferred,
        // and ordinary resumable admission is not disabled by artifact assembly.
        let mut admission = match lock(&self.control.inner.gate) {
            Ok(state) => state,
            Err(error) => return Some(Err(error.to_string())),
        };
        loop {
            if admission.uncertain {
                return Some(Err(MachineError::UnknownCompletion.to_string()));
            }
            if admission.work.hart != 0 || admission.work.boundary != 0 {
                return Some(Err(MachineError::Busy.to_string()));
            }
            if admission.work.host == 0 {
                break;
            }
            admission = match self.control.inner.work_changed.wait(admission) {
                Ok(state) => state,
                Err(_) => return Some(Err(MachineError::Poisoned.to_string())),
            };
        }
        if self.memory.is_poisoned() {
            return Some(Err("poisoned host memory capability".into()));
        }
        self.domain().platform.signature(self.image())
    }
    pub(crate) fn quiesce(&self) -> Result<DrainReport, MachineError> {
        self.control.request_quiesce()?;
        #[cfg(test)]
        if let Some(hook) = &self.before_drain {
            hook();
        }
        let mut state = lock(&self.control.inner.gate)?;
        loop {
            if state.uncertain {
                return Err(MachineError::UnknownCompletion);
            }
            if state.work.hart != 0 || state.work.boundary != 0 {
                return Err(MachineError::Busy);
            }
            if state.work.host == 0 {
                break;
            }
            state = self
                .control
                .inner
                .work_changed
                .wait(state)
                .map_err(|_| MachineError::Poisoned)?;
        }
        // The SAME drain checks and causal facts as the shared owner, not a
        // mutex-only acknowledgment or an empty-controller stand-in.
        acknowledge_drain(&mut state, self.installed.as_ref())
    }
    pub(crate) fn install(&mut self, image: Arc<LoadImage>) -> Result<(), MachineError> {
        // Validate/build before closing current admission: bad input must retain
        // the complete current association and ordinary writable behavior.
        if self.control.status()?.uncertain {
            return Err(MachineError::UnknownCompletion);
        }
        let candidate = Installed::build(&self.control.inner.config, image)?;
        self.quiesce()?;
        {
            let mut admission = lock(&self.control.inner.gate)?;
            self.control.publish(&mut admission, candidate)?;
        }
        let installed = self
            .control
            .inner
            .installed
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .take();
        self.installed = installed;
        self.memory = self.control.memory()?;
        self.edited = false;
        self.edit_reservation = None;
        self.control.resume()
    }
    pub(crate) fn fresh_reset(&mut self) -> Result<(), MachineError> {
        let image = self.domain().image.clone();
        self.install(image)
    }
}
impl Drop for OwnedMachine {
    fn drop(&mut self) {
        // Reattach the SAME domain before dropping the controller. Active leases
        // retain it through completion; Inner::drop quarantines unknown domains.
        let mut slot = self
            .control
            .inner
            .installed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = self.installed.take();
    }
}
