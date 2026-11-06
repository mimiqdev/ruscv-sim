//! The C-ABI grant host (dev-plan D8, ADR-0004 §4.4/§7).
//!
//! One grant carries an authoritative start time, a maximum end time and a
//! measurable Hart-turn budget, and returns the consumed modeled time, the
//! boundary facts and whether a grant bound was reached.  The host is a
//! Runner-side adapter: it grants budgets and classifies results.  It never
//! becomes a second Hart path, never advances the caller's clock, and never
//! turns a transport status, a callback return or a host stop into a ruscv-sim
//! terminal result.
//!
//! Time authority.  The Machine owns its Platform cursor in ticks; the caller
//! owns global time in exact femtoseconds.  A grant converts its own slack
//! (`max_end - start`) downward to whole ticks, so its inclusive deadline can
//! only be conservative, and returns `consumed` as the exact advance of the
//! Machine's committed time.  A start earlier than the Machine's last committed
//! time is an adapter failure (ADR-0004 §4.4); reset restarts the epoch at zero,
//! which is the exception that clause names.
//!
//! Deadline safety.  Before a turn starts, the host forms the turn's
//! conservative completion bound: one completed-turn tick plus the target's
//! declared per-transaction maximum for every worst-case transaction of that
//! turn (dev-plan D7).  A turn that would cross the inclusive deadline is not
//! started, and the grant reports `DeadlineBlocked` with the residual slack and
//! the refused bound (ADR-0004 §7.2).  At the deadline itself no normal turn
//! begins.

use super::target::CBlockingTarget;
use super::{ApiError, Status};
use crate::blocking::{Femtoseconds, TickPeriod, TurnCompletionBound};
use crate::core::commits::CommitLogger;
use crate::core::{SimulatorFailure, StepOutcome};
use crate::elf::LoadImage;
use crate::executor::{is_host_view_failure, try_extract_exit_code, RunControl};
use crate::machine::{
    Lifecycle, MachineConfig, MachineError, MachineTurn, OwnedMachine, PlatformEvent,
};
use std::path::Path;
use std::sync::Arc;

/// Completed Hart turns one grant step can produce.
///
/// A waiting Hart retires its `WFI` turn and, when the wake is earlier than the
/// deadline, takes the wake turn inside the same step (the Machine settles the
/// idle jump before it returns).  The lookahead therefore reserves two completed
/// turns, each with its own worst-case transaction delay, so a step can never
/// need more time than the bound it was granted.
const GRANT_STEP_TURNS: u64 = 2;

/// Applies the caller-frame horizon facts to one boundary.
///
/// The internal deadline is the last whole tick before the caller's horizon, so
/// only the caller's own time base can say whether the horizon was reached; a
/// remaining fraction is slack.  Residual slack is reported whenever a grant
/// bound applies, in the caller's time base.
fn finalize_horizon(
    facts: &mut GrantFacts,
    caller_end: u64,
    max_end_fs: u64,
    deadline_stop_seen: bool,
) {
    if caller_end >= max_end_fs {
        facts.deadline_reached = true;
        facts.deadline_blocked = false;
    } else {
        facts.deadline_reached = false;
        facts.deadline_blocked |= deadline_stop_seen;
    }
    if facts.deadline_blocked || facts.deadline_reached || facts.budget_exhausted {
        facts.residual_slack_fs = max_end_fs.saturating_sub(caller_end);
    }
}

/// What one completed boundary's exit observation produced.
#[derive(Debug)]
enum Exit {
    /// The guest named a terminal exit code.
    Code(u32),
    /// No terminal signal, or an ordinary unmapped/unmisaligned address.
    Absent,
    /// The debug transport failed while polling or clearing the signal.
    HostFailure(String),
}

/// How a run stands after one grant (the C ABI's `ruscv_run_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    /// The grant ended at a grant bound; the guest may still make progress.
    Running,
    /// The guest signal named a terminal exit code.
    Exited,
    /// A simulator, host or transport failure ended the run.
    Failed,
}

impl RunState {
    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::Running => 0,
            Self::Exited => 1,
            Self::Failed => 2,
        }
    }
}

/// The facts of one grant (the C ABI's `ruscv_grant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GrantFacts {
    /// The Machine's committed virtual time at grant entry, in femtoseconds.
    pub(crate) committed_start_fs: u64,
    /// The Machine's committed virtual time after the grant, in femtoseconds.
    pub(crate) committed_end_fs: u64,
    /// `committed_end_fs - committed_start_fs`: turns, idle jumps and delay.
    pub(crate) consumed_fs: u64,
    /// The started-slot budget ended the grant.
    pub(crate) budget_exhausted: bool,
    /// The cursor reached the inclusive grant deadline.
    pub(crate) deadline_reached: bool,
    /// No Runnable turn's conservative completion bound fit the slack.
    pub(crate) deadline_blocked: bool,
    /// Remaining slack when a deadline was reached or blocked, in femtoseconds.
    pub(crate) residual_slack_fs: u64,
    /// The refused turn's conservative completion bound, in femtoseconds.
    pub(crate) candidate_bound_fs: u64,
    /// The Hart is waiting and this grant retired nothing.
    pub(crate) waiting: bool,
    /// A wait has no future Platform event.
    pub(crate) no_progress: bool,
    /// An unknown completion left the run non-resumable.
    pub(crate) uncertain: bool,
    /// A simulator or host failure ended the run.
    pub(crate) failed: bool,
    /// Total consumed blocking-transport delay so far, in femtoseconds.
    pub(crate) transport_delay_fs: u64,
    /// Machine lifecycle acknowledgment at the grant boundary.
    pub(crate) lifecycle: Lifecycle,
    /// Completed Hart turns since the current image was installed.
    pub(crate) turns: u64,
}

impl Default for GrantFacts {
    fn default() -> Self {
        Self {
            committed_start_fs: 0,
            committed_end_fs: 0,
            consumed_fs: 0,
            budget_exhausted: false,
            deadline_reached: false,
            deadline_blocked: false,
            residual_slack_fs: 0,
            candidate_bound_fs: 0,
            waiting: false,
            no_progress: false,
            uncertain: false,
            failed: false,
            transport_delay_fs: 0,
            lifecycle: Lifecycle::Running,
            turns: 0,
        }
    }
}

impl GrantFacts {
    /// True when a grant bound, rather than a terminal result, ended the grant.
    pub(crate) const fn boundary_reached(self) -> bool {
        self.budget_exhausted
            || self.deadline_reached
            || self.deadline_blocked
            || self.waiting
            || self.no_progress
    }
}

/// One installed image and the Machine driving it for the C ABI.
///
/// Every operation is synchronous and runs on the creating thread only; the FFI
/// layer owns thread affinity, re-entry rejection and panic poisoning.
pub(crate) struct Session {
    config: MachineConfig,
    period: TickPeriod,
    image: Arc<LoadImage>,
    machine: Option<OwnedMachine>,
    tohost: u64,
    logger: Option<CommitLogger>,
    facts: GrantFacts,
    state: RunState,
    exit_code: u32,
    /// The failed run cannot continue without a fresh reset.
    failure: Option<String>,
    /// The current failure is an unknown completion, resolvable only by the
    /// target's transport-reset acknowledgement.
    unknown: bool,
    /// The caller's committed global time after its last grant, in femtoseconds.
    ///
    /// The kernel commits global time only from the amount a grant returns
    /// (ADR-0004 §4.4), so the next grant's authoritative start may not precede
    /// this value.  `None` after a load or reset, which restarts the epoch.
    caller_committed_fs: Option<u64>,
}

impl Session {
    /// Builds one Machine for the installed image through the caller's target.
    pub(crate) fn new(config: MachineConfig, image: Arc<LoadImage>) -> Result<Self, ApiError> {
        let machine = OwnedMachine::new(config.clone(), image.clone()).map_err(|error| {
            ApiError::new(Status::Elf, format!("cannot install the image: {error}"))
        })?;
        Ok(Self {
            period: config.tick_period,
            config,
            image,
            tohost: machine.selected_tohost(),
            machine: Some(machine),
            logger: None,
            facts: GrantFacts::default(),
            state: RunState::Running,
            exit_code: 0,
            failure: None,
            unknown: false,
            caller_committed_fs: None,
        })
    }

    /// Moves the configured commit-log sink into this session.
    ///
    /// Replacing the image keeps the observer connection topology, so a host
    /// that named a path keeps logging after a reload or reset (ADR-0003 §3).
    pub(crate) fn adopt_logger(&mut self, logger: Option<CommitLogger>) {
        if logger.is_some() {
            self.logger = logger;
        }
    }

    /// Takes the configured commit-log sink out of this session.
    pub(crate) fn take_logger(&mut self) -> Option<CommitLogger> {
        self.logger.take()
    }

    /// Routes the commit log to `path` from now on (dev-plan D8).
    ///
    /// The file is created and truncated here; the commits of every later grant
    /// are streamed to it.  A host that names a path wants observation, so
    /// observation is enabled exactly while a path is set.
    pub(crate) fn set_commit_log(&mut self, path: &Path) -> Result<(), ApiError> {
        let logger = CommitLogger::new_file(path).map_err(|error| {
            ApiError::new(
                Status::Io,
                format!(
                    "cannot create the commit log at {}: {error}",
                    path.display()
                ),
            )
        })?;
        self.logger = Some(logger);
        Ok(())
    }

    /// The last grant's facts.
    pub(crate) const fn facts(&self) -> GrantFacts {
        self.facts
    }

    /// The run's current classification.
    pub(crate) const fn state(&self) -> RunState {
        self.state
    }

    /// The guest exit code, valid once [`RunState::Exited`].
    pub(crate) const fn exit_code(&self) -> u32 {
        self.exit_code
    }

    /// The final program counter of the last boundary.
    pub(crate) fn pc(&self) -> u64 {
        self.machine
            .as_ref()
            .map_or(0, |machine| machine.state().pc)
    }

    /// The most recent terminal failure or classification message.
    pub(crate) fn diagnostic(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Runs one grant and classifies its result.
    ///
    /// A classified failure (`SimulatorFailure`, a host-view failure, an
    /// unknown completion) is a *result*, not an API error: it is reported
    /// through the grant facts and [`Session::state`] so the host can
    /// distinguish it from a guest exit.  An invalid grant, a refused Machine
    /// operation or an already terminal run is an API error.
    pub(crate) fn run_grant(
        &mut self,
        start_fs: u64,
        max_end_fs: u64,
        turn_budget: u64,
    ) -> Result<(), ApiError> {
        if self.state != RunState::Running {
            return Err(self.terminal_error());
        }
        // The Machine is driven through one local handle for the whole grant so
        // a classified failure can still read its clock and reinstall it.
        let mut machine = self
            .machine
            .take()
            .ok_or(ApiError::status(Status::NoImage))?;
        let outcome = self.run_grant_inner(&mut machine, start_fs, max_end_fs, turn_budget);
        self.machine = Some(machine);
        outcome
    }

    fn run_grant_inner(
        &mut self,
        machine: &mut OwnedMachine,
        start_fs: u64,
        max_end_fs: u64,
        turn_budget: u64,
    ) -> Result<(), ApiError> {
        let period = self.period.femtoseconds().as_u64();
        let start_ticks = machine.platform_mtime().map_err(MachineRefusal::status)?;
        let committed_start_fs = start_ticks
            .checked_mul(period)
            .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the committed time overflows"))?;
        // ADR-0004 §4.4: the Machine never rewinds its cursor to honor a start.
        if start_fs < committed_start_fs {
            return Err(ApiError::new(
                Status::TimeAuthority,
                format!(
                    "grant start {start_fs} fs is earlier than the Machine's last committed time \
                     {committed_start_fs} fs; reset restarts the epoch"
                ),
            ));
        }
        if max_end_fs < start_fs {
            return Err(ApiError::new(
                Status::TimeAuthority,
                format!("grant end {max_end_fs} fs is earlier than its start {start_fs} fs"),
            ));
        }
        // The kernel commits global time only from the amount a grant returned
        // (ADR-0004 §4.4), so its own clock cannot rewind either.  This check is
        // the caller-facing half of the earlier-start rule; the Machine-cursor
        // check above is the other.
        if let Some(committed) = self.caller_committed_fs {
            if start_fs < committed {
                return Err(ApiError::new(
                    Status::TimeAuthority,
                    format!(
                        "grant start {start_fs} fs is earlier than the caller's committed time \
                         {committed} fs"
                    ),
                ));
            }
        }
        // The horizon is the caller's inclusive end.  The Machine's deadline is
        // the last whole tick that starts at or before it, so a step can never be
        // granted time the caller did not give, and a remaining fraction stays
        // visible as slack rather than time.
        let slack_ticks = (max_end_fs - start_fs) / period;
        let deadline = start_ticks
            .checked_add(slack_ticks)
            .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the grant deadline overflows"))?;
        machine.set_virtual_deadline(Some(deadline));

        let mut control = RunControl::new(turn_budget);
        let mut facts = GrantFacts {
            committed_start_fs,
            committed_end_fs: committed_start_fs,
            lifecycle: Lifecycle::Running,
            turns: self.facts.turns,
            ..GrantFacts::default()
        };
        // One step can complete a waiting turn and its wake, and each reserves
        // its own turn tick plus the target's declared per-transaction maximum
        // for the worst-case transaction count.  The arithmetic is checked: a
        // configuration that cannot be bounded is a failure, never a wrap.
        let turn_bound = machine.turn_completion_bound_ticks().unwrap_or(0);
        let bound_ticks =
            GRANT_STEP_TURNS
                .checked_mul(turn_bound.checked_add(1).ok_or_else(|| {
                    ApiError::new(Status::Config, "the completion bound overflows")
                })?)
                .ok_or_else(|| ApiError::new(Status::Config, "the completion bound overflows"))?;
        let wake_bound_ticks = turn_bound
            .checked_add(1)
            .ok_or_else(|| ApiError::new(Status::Config, "the completion bound overflows"))?;
        let committed = |machine: &mut OwnedMachine| -> Result<u64, ApiError> {
            let now = machine.platform_mtime().map_err(MachineRefusal::status)?;
            now.checked_mul(period)
                .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the committed time overflows"))
        };
        // The caller's own clock: its start plus everything the grant consumed.
        let caller_now = |consumed_fs: u64| -> Result<u64, ApiError> {
            start_fs
                .checked_add(consumed_fs)
                .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the caller clock overflows"))
        };
        let mut deadline_stop_seen = false;

        loop {
            let now_fs = committed(machine)?;
            let elapsed_fs = now_fs - facts.committed_start_fs;
            let at_horizon = caller_now(elapsed_fs)? >= max_end_fs;
            if !control.may_execute() || at_horizon {
                // ADR-0004 §4.3 phase 3: report every coincident fact, so a zero
                // budget at a reached horizon is not collapsed into one.
                facts.budget_exhausted = !control.may_execute();
                facts.deadline_reached = at_horizon;
                break;
            }
            let now_ticks = machine.platform_mtime().map_err(MachineRefusal::status)?;
            // ADR-0004 §7.2: start a step only when its conservative completion
            // bound fits the inclusive deadline; otherwise report the residual
            // slack and the refused bound instead of speculative work.
            // A reserve that cannot be added to the cursor cannot fit either:
            // overflow is refusal, never a wrapped or saturated "fits".
            if now_ticks
                .checked_add(bound_ticks)
                .is_none_or(|end| end > deadline)
            {
                facts.deadline_blocked = true;
                facts.candidate_bound_fs = bound_ticks.saturating_mul(period);
                break;
            }
            control.start_slot();
            let last_slot = !control.may_execute();
            let observe = self.logger.is_some();
            let mut turn = match machine.step_request(observe, false, last_slot) {
                Ok(turn) => turn,
                Err(error) => return Err(self.refusal(error)),
            };
            facts.lifecycle = turn.boundary().lifecycle;
            facts.transport_delay_fs = turn.boundary().transport_delay_femtoseconds;
            facts.waiting |= turn.boundary().waiting;
            facts.no_progress |= turn.boundary().no_progress;
            deadline_stop_seen |= turn.boundary().deadline_stop;
            facts.uncertain |= turn.boundary().uncertain;

            // A completed Hart outcome is accounted before fallible observer
            // delivery: a commit-log write cannot unretire or drop a committed
            // transition (ADR-0004 §4.3/§5.1).
            let completed = matches!(
                turn.hart().outcome,
                StepOutcome::InstructionRetired(_)
                    | StepOutcome::TrapEntered(_)
                    | StepOutcome::DebugHalted {
                        completed_turn: true,
                        ..
                    }
            );
            if completed {
                control.complete_turn();
            }
            if let Some(message) = self.deliver(&turn) {
                return self.finish_failure(
                    machine,
                    facts,
                    start_fs,
                    max_end_fs,
                    control.cycles(),
                    last_slot && completed,
                    message,
                    false,
                );
            }
            match &turn.hart().outcome {
                StepOutcome::SimulatorFailure(failure) => {
                    // A started failed slot consumes its budget but completes no
                    // turn, and it never becomes a guest exit or passing result.
                    let message = format_failure(failure);
                    let unknown = turn.boundary().uncertain;
                    return self.finish_failure(
                        machine,
                        facts,
                        start_fs,
                        max_end_fs,
                        control.cycles(),
                        false,
                        message,
                        unknown,
                    );
                }
                StepOutcome::Waiting => {
                    // A parked wait retires nothing here: the WFI transition that
                    // completed is already counted, and a wait that starts no
                    // turn consumes no budget slot (ADR-0004 §7.1).  A jump that
                    // left too little slack for the wake turn ends the step, and
                    // the refused bound is reported like any other blocked turn.
                    control.refund_uncompleted_slot();
                    if turn.wake_blocked() {
                        facts.deadline_blocked = true;
                        facts.candidate_bound_fs = wake_bound_ticks.saturating_mul(period);
                    }
                    break;
                }
                StepOutcome::DebugHalted {
                    completed_turn: false,
                    ..
                } => {
                    control.refund_uncompleted_slot();
                    return self.finish_failure(
                        machine,
                        facts,
                        start_fs,
                        max_end_fs,
                        control.cycles(),
                        false,
                        "the Hart is halted in Debug Mode and no debugger can resume it".into(),
                        false,
                    );
                }
                StepOutcome::InstructionRetired(_)
                | StepOutcome::TrapEntered(_)
                | StepOutcome::DebugHalted {
                    completed_turn: true,
                    ..
                } => {}
            }
            match self.observe_exit(machine, &mut turn) {
                Exit::Code(code) => {
                    facts.turns = self.facts.turns + control.cycles();
                    facts.committed_end_fs = committed(machine)?;
                    facts.consumed_fs = facts.committed_end_fs - facts.committed_start_fs;
                    let caller_end = caller_now(facts.consumed_fs)?;
                    // A terminal exit does not discard coincident bound facts.
                    facts.budget_exhausted = !control.may_execute();
                    finalize_horizon(&mut facts, caller_end, max_end_fs, false);
                    self.commit_run(facts, caller_end, RunState::Exited, code)?;
                    return Ok(());
                }
                Exit::Absent => {}
                Exit::HostFailure(message) => {
                    return self.finish_failure(
                        machine,
                        facts,
                        start_fs,
                        max_end_fs,
                        control.cycles(),
                        last_slot,
                        message,
                        false,
                    );
                }
            }
        }

        facts.committed_end_fs = committed(machine)?;
        facts.consumed_fs = facts.committed_end_fs - facts.committed_start_fs;
        facts.turns = self.facts.turns + control.cycles();
        let caller_end = caller_now(facts.consumed_fs)?;
        finalize_horizon(&mut facts, caller_end, max_end_fs, deadline_stop_seen);
        self.commit_run(facts, caller_end, RunState::Running, 0)?;
        Ok(())
    }

    /// Records one non-terminal grant boundary and the caller's committed time.
    fn commit_run(
        &mut self,
        facts: GrantFacts,
        caller_end: u64,
        state: RunState,
        code: u32,
    ) -> Result<(), ApiError> {
        self.facts = facts;
        self.state = state;
        self.exit_code = code;
        self.caller_committed_fs = Some(caller_end);
        Ok(())
    }

    /// Reinstalls the image through the Machine's own reset lifecycle.
    ///
    /// An ordinary reset (running, drained or failed) uses the composition's
    /// coordinated fresh reset: it quiesces, zero-fills the RAM window and
    /// republishes the same image, so the installed generation is replaced
    /// rather than silently reused (ADR-0003 §3).
    ///
    /// After an unknown completion the poisoned composition is never resumed,
    /// rewired or force-drained.  Its transport is resolved by the target's
    /// transport-reset acknowledgement, which a fresh composition obtains while
    /// it zero-fills and reinstalls (dev-plan D6), and the failed domain stays
    /// quarantined exactly as the Machine's own teardown records it.  A target
    /// that withholds the acknowledgement keeps the handle unusable.
    pub(crate) fn reset(&mut self) -> Result<(), ApiError> {
        if self.machine.is_none() {
            return Err(ApiError::status(Status::NoImage));
        }
        // A refused reset leaves the session exactly as it was: the failed
        // Machine stays installed (and quarantined), so a host that later
        // obtains the target's acknowledgement can retry.
        if self.unknown {
            let fresh =
                OwnedMachine::new(self.config.clone(), self.image.clone()).map_err(|error| {
                    ApiError::new(
                        Status::Machine,
                        format!("cannot reset the Machine: {error}"),
                    )
                })?;
            self.tohost = fresh.selected_tohost();
            self.machine = Some(fresh);
        } else {
            self.machine
                .as_mut()
                .ok_or(ApiError::status(Status::NoImage))?
                .fresh_reset()
                .map_err(|error| {
                    ApiError::new(
                        Status::Machine,
                        format!("cannot reset the Machine: {error}"),
                    )
                })?;
        }
        self.facts = GrantFacts::default();
        self.state = RunState::Running;
        self.exit_code = 0;
        self.failure = None;
        self.unknown = false;
        self.caller_committed_fs = None;
        Ok(())
    }

    /// Delivers one completed Hart transition to the commit log.
    ///
    /// An observer failure is a control fact: it ends the run and retains the
    /// committed transition, never rewrites it.
    fn deliver(&mut self, turn: &MachineTurn) -> Option<String> {
        let logger = self.logger.as_mut()?;
        turn.deliver(logger)
            .err()
            .map(|error| format!("commit log reporting failure: {error}"))
    }

    /// Decodes a terminal guest signal, retaining the code before clearing it.
    ///
    /// Only RAM-resident signals are polled through the selected RAM slot.  A
    /// failed or short debug transport while polling or clearing is a host
    /// failure, never a silent absence and never a guest exit (dev-plan D6), so
    /// it is returned for failure classification with the completed transition
    /// and its consumed time intact.
    fn observe_exit(&mut self, machine: &mut OwnedMachine, turn: &mut MachineTurn) -> Exit {
        let htif = turn
            .events()
            .iter()
            .filter_map(|event| match event {
                PlatformEvent::HtifWrite(value) => try_extract_exit_code(*value),
                _ => None,
            })
            .find(|code| *code != u32::MAX);
        if let Some(code) = htif {
            return Exit::Code(code);
        }
        if !turn.hart().control.retired {
            return Exit::Absent;
        }
        let tohost = self.tohost;
        machine.observe_tohost(turn, tohost);
        // quantum F2: a non-zero RAM `tohost` value syncs before the exit rule
        // decodes it; the sample itself is RAM and does not depend on time.
        if let Err(error) = machine.sync_after_tohost(turn) {
            return Exit::HostFailure(format!("tohost sync failed: {error}"));
        }
        let Some(sample) = turn.tohost() else {
            return Exit::Absent;
        };
        let observed = match &sample.value {
            Ok(value) => *value,
            Err(error) => {
                return if is_host_view_failure(error) {
                    Exit::HostFailure(format!("tohost read failed: {error}"))
                } else {
                    Exit::Absent
                };
            }
        };
        let (code, clear_error) =
            RunControl::take_ram_exit(observed, || machine.clear_signal(turn, tohost, false));
        if let Some(error) = clear_error.filter(is_host_view_failure) {
            return Exit::HostFailure(format!("tohost clear failed: {error}"));
        }
        code.map_or(Exit::Absent, Exit::Code)
    }

    /// Classifies a terminal failure and records it as the grant result.
    // The grant window and the Machine are both required: the caller's clock
    // and the horizon are not derivable from the Machine cursor.
    #[allow(clippy::too_many_arguments)]
    fn finish_failure(
        &mut self,
        machine: &mut OwnedMachine,
        mut facts: GrantFacts,
        start_fs: u64,
        max_end_fs: u64,
        completed_turns: u64,
        budget_exhausted: bool,
        message: String,
        unknown: bool,
    ) -> Result<(), ApiError> {
        let period = self.period.femtoseconds().as_u64();
        let now = machine.platform_mtime().map_err(MachineRefusal::status)?;
        facts.committed_end_fs = now
            .checked_mul(period)
            .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the committed time overflows"))?;
        facts.consumed_fs = facts.committed_end_fs - facts.committed_start_fs;
        facts.turns = self.facts.turns + completed_turns;
        facts.failed = true;
        facts.uncertain |= unknown;
        facts.budget_exhausted |= budget_exhausted;
        // The caller's grant context, not the Machine cursor, gives its
        // committed time; the coincident horizon facts are retained too.  A
        // failed attempt never adds `budget_exhausted` merely because it used
        // the last slot (ADR-0004 §7.1).
        let caller_end = start_fs
            .checked_add(facts.consumed_fs)
            .ok_or_else(|| ApiError::new(Status::TimeAuthority, "the caller clock overflows"))?;
        finalize_horizon(&mut facts, caller_end, max_end_fs, false);
        self.facts = facts;
        self.state = RunState::Failed;
        self.exit_code = 0;
        self.caller_committed_fs = Some(caller_end);
        self.unknown = unknown;
        self.failure = Some(message);
        Ok(())
    }

    /// The API error for a grant attempted on an already terminal run.
    fn terminal_error(&self) -> ApiError {
        match (&self.failure, self.unknown) {
            (message, true) => ApiError::new(
                Status::UnknownCompletion,
                message
                    .clone()
                    .unwrap_or_else(|| "the transport has an unknown completion; reset it".into()),
            ),
            (Some(message), false) => ApiError::new(
                Status::Machine,
                format!("the run already ended ({message}); reset before another grant"),
            ),
            (None, false) => ApiError::new(
                Status::Machine,
                "the run already ended; reset before another grant",
            ),
        }
    }

    /// Classifies one Machine refusal.
    fn refusal(&mut self, error: MachineError) -> ApiError {
        let refusal = MachineRefusal::new(error);
        if refusal.status == Status::UnknownCompletion {
            self.unknown = true;
            self.state = RunState::Failed;
            self.failure = Some(refusal.message.clone());
        }
        refusal.into()
    }
}

/// One Machine error as an API status plus its diagnostic.
struct MachineRefusal {
    status: Status,
    message: String,
}

impl MachineRefusal {
    fn new(error: MachineError) -> Self {
        let status = match &error {
            MachineError::UnknownCompletion => Status::UnknownCompletion,
            MachineError::NoImage => Status::NoImage,
            MachineError::Placement(_) | MachineError::UnsupportedTransport => Status::Config,
            _ => Status::Machine,
        };
        Self {
            status,
            message: error.to_string(),
        }
    }

    fn status(error: MachineError) -> ApiError {
        Self::new(error).into()
    }
}

impl From<MachineRefusal> for ApiError {
    fn from(refusal: MachineRefusal) -> Self {
        ApiError::new(refusal.status, refusal.message)
    }
}

/// Formats one Hart-owned simulator failure for the host diagnostic.
fn format_failure(failure: &SimulatorFailure) -> String {
    format!("{:?}: {}", failure.kind, failure.message)
}

/// The composition one C-ABI handle uses.
///
/// The C ABI is a hosting adapter for the native Platform; a caller-supplied
/// TLM target becomes that Platform's selected RAM slot (dev-plan D2).
pub(crate) fn native_config() -> MachineConfig {
    MachineConfig::new(crate::machine::PlatformKind::Native)
}

/// One declared Platform tick period, validated.
pub(crate) fn tick_period(femtoseconds: u64) -> Result<TickPeriod, ApiError> {
    if femtoseconds == 0 {
        return Err(ApiError::new(
            Status::InvalidArgument,
            "the tick period must be positive",
        ));
    }
    Ok(TickPeriod::new(Femtoseconds::new(femtoseconds)))
}

/// Validates a target's declared maximum before a Machine is built.
pub(crate) fn validate_target(
    target: &CBlockingTarget,
    period: TickPeriod,
) -> Result<(), ApiError> {
    TurnCompletionBound::new(target.declared_max_delay(), period)
        .map(|_| ())
        .map_err(|error| {
            ApiError::new(
                Status::Config,
                format!(
                    "the target's declared per-transaction maximum delay {} fs is not \
                     representable under the declared tick period: {error}",
                    target.declared_max_delay().as_u64()
                ),
            )
        })
}
