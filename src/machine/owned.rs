//! Exclusive borrowing adapter for the compatibility facade. The actual installed
//! Hart is owned here, never copied or exposed as a core. Shared host admission,
//! composition, transition and last-owner quarantine remain the T2 mechanisms.
use super::*;
use std::sync::Arc;

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
    /// Platform effects produced by debug work. Held until the next debug
    /// boundary so a reset cannot discard an effect committed before it.
    debug_events: Vec<PlatformEvent>,
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
            debug_events: Vec::new(),
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
    pub(crate) fn write_dcsr(&mut self, value: u64) -> u64 {
        self.domain_mut().core.write_dcsr(value)
    }
    pub(crate) fn dcsr(&self) -> u64 {
        self.domain().core.dcsr()
    }
    pub(crate) fn dpc(&self) -> u64 {
        self.domain().core.dpc()
    }
    #[cfg(test)]
    pub(crate) fn set_virtual_deadline(&mut self, deadline: Option<u64>) {
        self.domain_mut().virtual_deadline = deadline;
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
        let inner = Arc::clone(&self.control.inner);
        let _instruction = super::debug_hub::InInstruction::enter(&inner.debug.in_instruction);
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
        let mut turn = self.domain_mut().turn(lease, observe, false, single_step)?;
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
        // Held debug effects belong to the replaced generation. A reset
        // inside a debug boundary retains them; an explicit new image does not.
        self.debug_events.clear();
        self.memory = self.control.memory()?;
        self.edited = false;
        self.edit_reservation = None;
        self.control.resume()
    }
    pub(crate) fn fresh_reset(&mut self) -> Result<(), MachineError> {
        let image = self.domain().image.clone();
        self.install(image)
    }

    /// The Machine owns the Debug Module. A later [`Machine::fresh_reset`]
    /// reloads the hart and leaves this module in place.
    pub(crate) fn attach_debugger(&mut self) -> DebugPort {
        self.control.inner.debug.attach();
        DebugPort {
            inner: Arc::clone(&self.control.inner),
        }
    }

    pub(crate) fn debugger_attached(&self) -> bool {
        self.control.inner.debug.attached()
    }

    /// `--halted`. The next Hart boundary enters Debug Mode before fetch.
    pub(crate) fn request_halt(&mut self) -> Result<(), MachineError> {
        let hub = Arc::clone(&self.control.inner);
        hub.debug
            .dm
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .set_haltreq(&mut self.domain_mut().core, true);
        Ok(())
    }

    pub(crate) fn arm_debug_session(&self) -> DebugSessionGuard {
        self.control.inner.debug.open_session();
        DebugSessionGuard {
            inner: Arc::clone(&self.control.inner),
        }
    }

    pub(crate) fn publish_completed_turns(&self, turns: u64) {
        self.control.inner.debug.publish_turns(turns);
    }

    /// `ndmreset` is set, or the hart is already in Debug Mode. The run loop
    /// waits for a DMI command instead of fetching.
    pub(crate) fn debug_blocked(&self) -> bool {
        let ndmreset = self
            .control
            .inner
            .debug
            .dm
            .lock()
            .map(|dm| dm.ndmreset())
            .unwrap_or(true);
        ndmreset || self.domain().core.in_debug_mode()
    }

    /// Apply queued DMI transactions. A transaction posted during an
    /// instruction stays queued: this returns without applying it.
    /// `block` waits for the next command while the hart is halted, held in
    /// `ndmreset`, or parked by the caller.
    ///
    /// Platform effects and a simulator failure that belong to this debug
    /// boundary are returned here. Debug work has no guest turn, so they are
    /// not attached to a later instruction.
    pub(crate) fn debug_boundary(&mut self, block: bool) -> Result<DebugBoundary, MachineError> {
        if self.control.inner.debug.in_instruction() {
            return Ok(DebugBoundary::default());
        }
        loop {
            if self.control.inner.debug.closed() {
                return Err(MachineError::Debug(
                    "debug run ended before the transaction was admitted".into(),
                ));
            }
            let batch = self.drain_debug_queue()?;
            if batch.is_empty() {
                // A started command advances without socket input. The caller
                // re-enters here between grants, so DMI is serviced first at
                // the next boundary.
                if self.debug_command_busy()? {
                    return self.advance_debug_command();
                }
                if !block || !self.debug_events.is_empty() {
                    return Ok(DebugBoundary {
                        events: self.take_debug_events(),
                        failure: None,
                    });
                }
                self.wait_for_debug_queue()?;
                continue;
            }
            for call in batch {
                let result = self.apply_dmi(call.addr, call.write);
                let _ = call.reply.send(result);
            }
            // Return collected effects instead of waiting again: a terminal
            // debugger effect must reach the Runner at this boundary.
            if !self.debug_events.is_empty() || !block || !self.debug_blocked() {
                return Ok(DebugBoundary {
                    events: self.take_debug_events(),
                    failure: None,
                });
            }
        }
    }

    fn drain_debug_queue(&self) -> Result<Vec<super::debug_hub::DmiCall>, MachineError> {
        let mut queue = self
            .control
            .inner
            .debug
            .queue()
            .lock()
            .map_err(|_| MachineError::Poisoned)?;
        Ok(queue.drain(..).collect())
    }

    /// Wait for the next queued transaction. The queue predicate is rechecked
    /// under the mutex, so a post or close that lands between an empty drain
    /// and this wait is not lost.
    fn wait_for_debug_queue(&self) -> Result<(), MachineError> {
        let debug = &self.control.inner.debug;
        let mut queue = debug.queue().lock().map_err(|_| MachineError::Poisoned)?;
        while queue.is_empty() && !debug.closed() {
            queue = debug
                .wake()
                .wait(queue)
                .map_err(|_| MachineError::Poisoned)?;
        }
        Ok(())
    }

    pub(crate) fn debug_command_busy(&self) -> Result<bool, MachineError> {
        Ok(self
            .control
            .inner
            .debug
            .dm
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .busy())
    }

    /// Run one program-buffer grant of the active command.
    ///
    /// The DM lock selects the opcode and records the result; it is released
    /// while the Hart executes the word and reaches any physical target. A
    /// Machine that cannot admit a Hart turn (quiesce, reset, failure, unknown
    /// completion) ends the command instead of running another word. Platform
    /// effects and a simulator failure from this word are returned at this
    /// boundary.
    pub(crate) fn advance_debug_command(&mut self) -> Result<DebugBoundary, MachineError> {
        let hub = Arc::clone(&self.control.inner);
        let Ok(lease) = Lease::hart(&hub.gate) else {
            hub.debug
                .dm
                .lock()
                .map_err(|_| MachineError::Poisoned)?
                .abort_command();
            return Ok(DebugBoundary::default());
        };
        let word = {
            let mut dm = hub.debug.dm.lock().map_err(|_| MachineError::Poisoned)?;
            let core = &self
                .installed
                .as_ref()
                .expect("exclusive installed Machine")
                .core;
            dm.next_word(core)
        };
        let Some(word) = word else {
            lease.boundary(false, None)?;
            return Ok(DebugBoundary::default());
        };
        self.sample_debug_inputs()?;
        let (outcome, failure, uncertain) = {
            let core = &mut self
                .installed
                .as_mut()
                .expect("exclusive installed Machine")
                .core;
            let outcome = core.debug_execute(word);
            let failure = match &outcome {
                crate::core::DebugExecOutcome::Failure(failure) => Some(failure.clone()),
                _ => None,
            };
            (
                outcome,
                failure,
                core.unresolved_physical_access().is_some(),
            )
        };
        hub.debug
            .dm
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .complete_grant(outcome);
        self.capture_debug_events()?;
        lease.boundary(uncertain, failure.clone())?;
        Ok(DebugBoundary {
            events: self.take_debug_events(),
            failure,
        })
    }

    /// Platform effects produced by debug work since the last debug boundary.
    /// A guest turn drains its own effects, so these belong to debug work.
    pub(crate) fn take_debug_events(&mut self) -> Vec<PlatformEvent> {
        std::mem::take(&mut self.debug_events)
    }

    /// Move the Platform queue into the held debug events. Called before a
    /// later operation can replace the Platform.
    fn capture_debug_events(&mut self) -> Result<(), MachineError> {
        self.debug_events
            .extend(self.domain().platform.take_events()?);
        Ok(())
    }

    pub(crate) fn dmi_read(&mut self, addr: u8) -> Result<u32, MachineError> {
        self.admit_direct(addr, None)
    }

    pub(crate) fn dmi_write(&mut self, addr: u8, value: u32) -> Result<(), MachineError> {
        self.admit_direct(addr, Some(value)).map(|_| ())
    }

    fn admit_direct(&mut self, addr: u8, write: Option<u32>) -> Result<u32, MachineError> {
        if self.control.inner.debug.in_instruction() {
            return Err(MachineError::Debug(
                "DMI is admitted only at a Hart boundary".into(),
            ));
        }
        self.apply_dmi(addr, write)
    }

    /// Sample Platform inputs at a debug boundary. This mirrors the guest
    /// pre-turn sample: `time` and the machine lines become current, but no
    /// clock, counter or turn advances (contract §3, ADR-0004 amendment).
    fn sample_debug_inputs(&mut self) -> Result<(), MachineError> {
        let (mtime, lines) = {
            let platform = &self.domain().platform;
            (platform.read_mtime()?, platform.machine_interrupt_lines()?)
        };
        let csr = &mut self.domain_mut().core.state_mut().csr;
        csr.set_time(mtime);
        csr.set_machine_interrupt_lines(lines.msip, lines.mtip, lines.meip);
        csr.set_seip_external(lines.seip);
        Ok(())
    }

    fn apply_dmi(&mut self, addr: u8, write: Option<u32>) -> Result<u32, MachineError> {
        let hub = Arc::clone(&self.control.inner);
        let lease = Lease::debug(&hub.gate)?;
        self.sample_debug_inputs()?;
        let uncertain = lease.boundary_state()?.1;
        let (value, before, after, failure) = {
            let mut dm = hub.debug.dm.lock().map_err(|_| MachineError::Poisoned)?;
            let refused = dm.access_triggers_command(addr)
                || write.is_some_and(|value| !dm.terminal_write_allowed(addr, value));
            if uncertain && refused {
                return Err(MachineError::UnknownCompletion);
            }
            let core = &mut self
                .installed
                .as_mut()
                .expect("exclusive installed Machine")
                .core;
            let before = dm.ndmreset();
            let value = match write {
                Some(value) => {
                    dm.dmi_write(core, addr, value);
                    0
                }
                None => dm.dmi_read(core, addr),
            };
            (
                value,
                before,
                dm.ndmreset(),
                core.unresolved_physical_access().cloned(),
            )
        };
        self.capture_debug_events()?;
        let receipt = lease.boundary(failure.is_some(), failure)?;
        let uncertain = receipt.boundary_state()?.1;
        if !before && after {
            drop(receipt);
            self.reload_for_ndmreset()?;
        }
        if before && !after && !uncertain {
            hub.debug
                .dm
                .lock()
                .map_err(|_| MachineError::Poisoned)?
                .note_reset_release();
            let core = &mut self
                .installed
                .as_mut()
                .expect("exclusive installed Machine")
                .core;
            if core.haltreq() && !core.in_debug_mode() {
                let _ = core.step_outcome();
            }
        }
        Ok(value)
    }

    /// `ndmreset` rose. Reload the installed image through
    /// [`Machine::fresh_reset`]. The Debug Module stays on the Machine.
    fn reload_for_ndmreset(&mut self) -> Result<(), MachineError> {
        let signal = self.domain().platform.selected_tohost;
        let verbose = self.domain().core.verbose();
        self.quiesce()?;
        {
            let mut slot = self
                .control
                .inner
                .installed
                .lock()
                .map_err(|_| MachineError::Poisoned)?;
            *slot = self.installed.take();
        }
        let reset = self.control.fresh_reset();
        {
            let mut slot = self
                .control
                .inner
                .installed
                .lock()
                .map_err(|_| MachineError::Poisoned)?;
            self.installed = slot.take();
        }
        self.memory = self.control.memory()?;
        self.edited = false;
        self.edit_reservation = None;
        reset?;
        self.control.resume()?;
        self.select_signal(signal);
        self.domain_mut().core.set_verbose(verbose);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_in_instruction_for_test(&self, active: bool) {
        self.control.inner.debug.set_in_instruction(active);
    }

    #[cfg(test)]
    pub(crate) fn hart_haltreq(&self) -> bool {
        self.domain().core.haltreq()
    }

    #[cfg(test)]
    pub(crate) fn hart_in_debug_mode(&self) -> bool {
        self.domain().core.in_debug_mode()
    }

    #[cfg(test)]
    pub(crate) fn debug_queue_len(&self) -> usize {
        self.control.inner.debug.queue_len()
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
