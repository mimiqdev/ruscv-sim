use super::{Inner, MachineError};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// Lifecycle acknowledgments, not run stop reasons or scheduler states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    DrainComplete,
    Running,
    QuiesceRequested,
    TornDown,
}

/// Synchronous physical/native callbacks are contained by `hart` work.
/// Returned boundary receipts contain causal facts and accepted observation work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkCounts {
    pub hart: usize,
    pub host: usize,
    pub boundary: usize,
}
impl WorkCounts {
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

pub(super) struct Admission {
    pub lifecycle: Lifecycle,
    pub generation: u64,
    pub work: WorkCounts,
    pub uncertain: bool,
    pub failed: bool,
    pub failure: Option<crate::core::SimulatorFailure>,
    pub owner: Weak<Inner>,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::DrainComplete,
            generation: 0,
            work: WorkCounts::default(),
            uncertain: false,
            failed: false,
            failure: None,
            owner: Weak::new(),
        }
    }
}
pub(super) type Gate = Arc<Mutex<Admission>>;
pub(super) fn lock(gate: &Gate) -> Result<MutexGuard<'_, Admission>, MachineError> {
    gate.lock().map_err(|_| MachineError::Poisoned)
}
impl Admission {
    pub fn mutation_allowed(&self) -> Result<(), MachineError> {
        if self.uncertain {
            return Err(MachineError::UnknownCompletion);
        }
        if !self.work.is_empty() {
            return Err(MachineError::Busy);
        }
        if self.lifecycle != Lifecycle::DrainComplete {
            return Err(MachineError::NotDrained);
        }
        Ok(())
    }
    fn running(&self) -> Result<(), MachineError> {
        if self.uncertain {
            return Err(MachineError::UnknownCompletion);
        }
        if self.failed {
            return Err(MachineError::NeedsFreshReset);
        }
        if self.lifecycle != Lifecycle::Running {
            return Err(MachineError::NotRunning);
        }
        Ok(())
    }
}

pub(super) enum Work {
    Hart,
    Host,
    Boundary,
}
/// Admission precedes all lock-held work; lifecycle never infers completion from
/// the Platform lock. An unwound/unfinished Hart operation remains uncertain.
pub(super) struct Lease {
    gate: Gate,
    kind: Work,
    finished: bool,
    // Keep the entire composed domain alive through physical/callback/observer
    // work, even if the caller drops its last control handle.
    _owner: Arc<Inner>,
}
impl Lease {
    pub fn hart(gate: &Gate) -> Result<Self, MachineError> {
        let mut state = lock(gate)?;
        state.running()?;
        if state.work.hart != 0 || state.work.boundary != 0 {
            return Err(MachineError::Busy);
        }
        let owner = state.owner.upgrade().ok_or(MachineError::TornDown)?;
        state.work.hart += 1;
        Ok(Self {
            gate: gate.clone(),
            kind: Work::Hart,
            finished: false,
            _owner: owner,
        })
    }
    pub fn host(gate: &Gate, generation: u64) -> Result<Option<Self>, MachineError> {
        let mut state = lock(gate)?;
        // Detached RAM has no ports/devices/events and cannot reach this Machine.
        if generation != state.generation {
            return Ok(None);
        }
        state.running()?;
        let owner = state.owner.upgrade().ok_or(MachineError::TornDown)?;
        state.work.host += 1;
        Ok(Some(Self {
            gate: gate.clone(),
            kind: Work::Host,
            finished: true,
            _owner: owner,
        }))
    }
    pub fn boundary(
        mut self,
        uncertain: bool,
        failure: Option<crate::core::SimulatorFailure>,
    ) -> Result<Self, MachineError> {
        {
            let mut state = lock(&self.gate)?;
            state.uncertain |= uncertain;
            state.failed |= failure.is_some();
            if failure.is_some() {
                state.failure = failure;
            }
            state.work.hart -= 1;
            state.work.boundary += 1;
        }
        self.kind = Work::Boundary;
        self.finished = true;
        Ok(self)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        // Poison is a permanent refusal via lock(), never a recovery shortcut.
        if let Ok(mut state) = self.gate.lock() {
            if !self.finished {
                state.uncertain = true;
                state.failed = true;
            }
            match self.kind {
                Work::Hart => state.work.hart -= 1,
                Work::Host => state.work.host -= 1,
                Work::Boundary => state.work.boundary -= 1,
            }
        }
    }
}
