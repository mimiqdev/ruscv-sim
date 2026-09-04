# ADR-0004: Interrupt, time, scheduling, and stop-event boundaries

| Field | Value |
| --- | --- |
| Status | Proposed |
| Authority | Draft contract; normative only after acceptance |
| Date | 2026-09-04 |
| Owner | Runtime, time, and event architecture |
| Related decisions | [ADR-0001](0001-hart-execution-outcome-and-observation.md), [ADR-0002](0002-physical-access-transaction-and-fault.md), [ADR-0003](0003-runner-machine-and-platform-ownership.md) |
| Supersedes | None |

> This is a documentation-only decision. It defines semantic boundaries and
> ordering rules; it does not change Rust code or public APIs. It deliberately does
> not select layouts, constructors, trait signatures, callback or queue shapes,
> serialization formats, scheduling algorithms, device maps, or SystemC APIs. It
> does not claim that the target boundaries are implemented.

## 1. Context and scope

The first three proposed decisions establish the boundaries needed to evolve a
single-instruction ISS into a composed Virtual Platform (VP):

- [ADR-0001](0001-hart-execution-outcome-and-observation.md) makes the Hart
  transition precise and separates always-present control facts from optional
  materialized observations.
- [ADR-0002](0002-physical-access-transaction-and-fault.md) makes a Hart physical
  access transport-neutral and separates guest-visible target faults from
  simulator or adapter failures.
- [ADR-0003](0003-runner-machine-and-platform-ownership.md) assigns composition to
  the Machine and application policy, classification, and presentation to the
  Runner. It also requires the same Hart/Platform semantics in Runner-driven and
  external-kernel-driven hosting.

The remaining boundary is temporal and causal. A Hart must not fetch through an
interrupt, a device must not silently advance a Hart-owned counter, an annotated
physical delay must not become an accidental sleep or instruction count, and a
Runner must not lose the fact that a limit, trap, exit, debugger request, and
backend failure happened at one boundary. WFI makes the distinction observable:
a waiting Hart must not spin through instructions, while a timer must be able to
wake it without inventing a retirement. Multiple Harts also need a stable result
when their events share a timestamp, without this ADR prescribing one scheduler
implementation.

This ADR therefore defines:

1. the semantic quantities and their owners;
2. the Machine exchange used for budgets, absolute virtual-time deadlines,
   control requests, and observation demand;
3. the minimal functional-ISS time policy and the rules for consuming physical
   delay;
4. interrupt assertion, pending state, eligibility, sampling, acceptance, and
   block-execution precision;
5. WFI, runnable/waiting/quiescent state, timer wakeup, and legal idle jumps;
6. non-lossy control facts, deterministic same-time ordering, and primary-reason
   classification; and
7. synchronization, quiesce/drain, multi-Hart ordering, and conditional replay
   input admission.

It is a semantic contract, not a Rust design. Names in this record are descriptive
and may be represented by different types or transports later.

### 1.1 Normative vocabulary

- **Must** and **shall** state an invariant of the target contract.
- **May** describes an allowed implementation choice that cannot change the
  observable contract.
- **Minimal ISS policy** is the deterministic functional policy selected here for
  the baseline. It is not a claim about hardware cycle accuracy.
- **Precise mode** means that no architectural interrupt, stop request, or
  required time boundary is observed later than the next permitted Hart boundary.
- **Reduced-accuracy mode** is allowed only when it declares a finite interrupt
  latency and/or time-overshoot bound in the returned facts.
- **Hart turn** is one granted architectural transition: either one instruction
  attempt (which retires, enters an architectural trap, or fails) or one accepted
  interrupt entry. WFI idle time and Platform-only time advancement are not Hart
  turns.
- **Boundary** is a point at which the current Hart transition and all of its
  physical effects have completed, modeled delay has been accounted for, causal
  Platform events have been admitted, and no transaction or observation delivery
  is in flight.

## 2. Decision summary

The contract adopts these rules:

1. The Hart owns instruction attempts, retirement, `mcycle`, `minstret`, trap
   entry, and architectural pending/privilege state. The Platform owns `mtime`,
   `mtimecmp`, interrupt sources, level/edge controller state, physical routing,
   and device events. The Machine owns the virtual-time cursor, grants, event
   arbitration, and quiescent lifecycle; the Runner owns run policy and final
   presentation. Host wall time is observation/control input only.
2. A Machine exchange uses an explicit finite, zero, or unbounded **Hart-turn
   budget**, an absolute virtual-time deadline, a control request, and an
   observation demand. A zero budget performs no Hart turn. A finite budget is
   exact, including accepted interrupt entries. A deadline is an inclusive
   completion bound: work may end exactly at it, but no work may begin if its
   declared bound could cross it.
3. In the minimal ISS, each completed Hart turn advances virtual time by one
   configured `iss_tick`; each physical response delay is added once, separately;
   WFI idle jumps advance only virtual/Platform time. Minimal `mcycle` advances
   once per completed Hart turn and `minstret` advances only for retired
   instructions. These equalities are a functional policy, not an assertion that
   an instruction is one hardware cycle or that virtual time is host time.
4. A physical success and a guest-visible physical fault consume all known
   transaction delay. A simulator/adapter failure consumes any delay known to
   have elapsed, never retries or rolls back an unknown completion, and makes the
   run non-resumable until the adapter resolves or resets the state.
5. Interrupts are sampled at the pre-fetch boundary after all Platform events due
   at the current virtual time have been admitted. An accepted interrupt enters
   trap before fetch, consumes one Hart turn, and does not retire an instruction.
   An in-flight instruction is never asynchronously preempted.
6. WFI retires once when executed and then puts the Hart into `Waiting` if no
   wake condition is present at the post-retirement boundary. A scheduler may
   jump a wholly idle Machine only to the next known event or grant deadline;
   it must never simulate idle by retiring instructions.
7. Machine returns every applicable fact in canonical order. It does not collapse
   coincident facts into one reason. The Runner applies a separate deterministic
   primary-reason policy after the complete fact set is available.
8. Same-time Harts and Platform events have a stable semantic commit order based
   on timestamp, causal phase, stable source identity, Hart ID, and local sequence;
   the scheduler may use any implementation strategy that is observationally
   equivalent to that order.

## 3. Ownership of quantities

The following table is the contract boundary. A quantity may be copied into an
exchange response for reporting, but copying does not transfer ownership.

| Quantity | Owner | Meaning and update rule | Explicitly not |
| --- | --- | --- | --- |
| Instruction attempt | Hart, reported by Machine | A fetch/decode/execute attempt begins after the pre-fetch interrupt sample says no interrupt was accepted. It counts whether the instruction retires or enters a guest-visible synchronous trap; a failed attempt is reported as attempted but has no fabricated architectural outcome. | Retirement, a WFI idle jump, or an accepted interrupt entry |
| Retired instruction progress | Hart, reported by Machine/Runner | Increments exactly once for `InstructionRetired`, including a retired WFI. It does not increment for a faulting instruction, interrupt entry, idle time, Platform advancement, or a simulator failure. | Number of turns or host instructions executed by a block engine |
| Hart turn/step | Machine grant accounting | One accepted interrupt entry or one instruction attempt. It is the unit of the baseline execution budget and is reported separately from attempts and retirement. | A hardware cycle, a retired instruction, or a unit of modeled time |
| `minstret` | Hart architectural state | The selected ISA profile's retired-instruction counter. The baseline relation is one increment per retired instruction and none for interrupt entry or faulting instruction. | A Runner limit or a Platform clock |
| `mcycle` | Hart architectural state | The selected ISA profile's cycle counter. Minimal ISS policy increments it by one counter unit per completed Hart turn, including an accepted interrupt entry and WFI retirement, but not idle jumps or Platform-only advancement. A richer timing profile may replace this charge explicitly. | `minstret`, `mtime`, transaction delay, or host elapsed time |
| `mtime` | Platform | Platform clock value derived from Machine virtual-time advancement using the configured Platform clock mapping. It is shared Platform state, not a per-Hart progress counter. Normal advancement is monotonic. | A Hart counter or host wall clock |
| `mtimecmp` | Platform, normally per Hart/source | Timer compare state. It may be changed by an architectural MMIO operation according to the selected Platform contract; timer assertion is level-based on the resulting `mtime >= mtimecmp` condition. | An interrupt acceptance count or a scheduler deadline |
| Scheduler/kernel virtual time | Machine in native hosting; outer kernel is the time authority in external hosting | Monotonic modeled time used to order Platform events and grants. A native Machine advances its cursor. An external kernel grants a bounded interval and commits the returned consumed time. | `mcycle`, wall time, or an implicit instruction count |
| Physical transaction delay | PhysicalAccess/Platform response, consumed by Machine | Nonnegative modeled elapsed duration attached to a completed physical operation. The Machine converts and consumes it exactly once, whether the target response is success or a guest-visible fault. | A request to sleep the Hart or an instruction-retirement decision |
| Host elapsed time | Runner/frontend/host | Measurement for diagnostics, throughput, watchdogs, or an explicitly requested external stop. It is not guest-visible modeled time unless an explicit adapter policy maps it into a Platform input. | `mtime`, virtual time, or deterministic replay time |

### 3.1 Progress vocabulary and counter independence

A normal retired instruction produces one instruction attempt, one Hart turn, one
retirement-progress increment, one `minstret` increment, and—under the minimal
policy—one `mcycle` increment. A synchronous faulting instruction produces one
instruction attempt and one Hart turn, enters a trap, produces no retirement or
`minstret` increment, and receives the minimal `mcycle` charge because the Hart
turn completed as an architectural trap. An accepted interrupt produces one Hart
turn and one trap entry, but zero instruction attempts, zero retirement progress,
and zero `minstret` increment. A WFI instruction is a normal retired instruction;
the subsequent wait is not a turn.

These relations are deliberately stated separately. A future cycle model may
charge different `mcycle` costs, and a scheduler may advance virtual time by
transaction delay or an idle jump without changing either Hart counter. A Runner
that wants an instruction-only compatibility limit must express that as a
separate run policy or adapter; it must not relabel `mcycle` or virtual time.

## 4. Machine exchange

The Machine is the one semantic exchange point between Hart execution and outer
run control. The concrete representation is deferred, but every exchange has the
following meaning.

### 4.1 Request/grant contents

A Runner-driven or external-kernel-driven caller supplies:

- **Starting virtual time:** the caller's current time or synchronization cursor.
- **Hart-turn budget:** zero, a finite nonnegative number, or unbounded. The unit
  is always Hart turns as defined in §1.1. An accepted interrupt consumes one
  turn, so a pending-interrupt storm cannot bypass a zero or finite limit.
- **Absolute virtual-time deadline:** optional. It is an inclusive completion
  bound. A grant without one is not allowed to cross another caller-imposed
  bound; an unbounded grant still stops at mandatory semantic boundaries.
- **Control request:** continue, single-step, request external stop, or request
  quiesce/drain. A control request is sampled at boundaries and never mutates a
  Hart in the middle of a physical transaction.
- **Observation demand:** disabled, or the set of materialized observations and
  delivery guarantees requested by subscribers. The demand can force a precise
  boundary, but absence of demand never removes control facts or changes Hart
  semantics.
- **Accuracy/deadline demand:** precise, or reduced accuracy with explicit finite
  interrupt-latency and time-overshoot bounds. A precise request cannot be silently
  fulfilled by an unbounded block or temporal-decoupling quantum.
- **Admitted input watermark:** when an external source or replay stream is in
  use, the caller supplies only inputs admitted through the synchronization
  boundary. Host threads do not write Platform state directly.

The grant may be shorter than the requested budget or deadline. Returning early
is required when a mandatory boundary is reached; it is not a violation of the
request.

### 4.2 Response contents

The Machine returns, at a coherent boundary:

- per-Hart instruction attempts, retired instructions, accepted interrupt entries,
  trap entries, and wait/runnable state transitions;
- start and end virtual time, total converted transaction delay, and any
  Platform-only idle advance, kept distinguishable from Hart progress;
- the next known Platform event, timer wakeup, or grant deadline, if any;
- the current per-Hart state (`Runnable`, `Waiting`, or a terminal/stopped state)
  and whether the Machine is `Quiescent`;
- all applicable **control facts** in canonical causal order, without selecting a
  primary reason; and
- optional materialized commit/trap observations requested by subscribers, in
  the same semantic order as their transitions.

A response with no optional observation demand is still complete for control and
progress. An observer failure is a control fact; it does not turn an already
committed instruction into an unretired one.

### 4.3 Exchange boundary sequence

The following sequence is normative; the scheduler's data structures and choice
of runnable Hart are not.

```text
1. Validate the grant and establish the Machine's current virtual-time cursor.
2. Admit all Platform/input events whose admitted timestamp is <= the cursor.
3. If a stop/quiesce request is already effective, return without a Hart turn.
4. If the budget is zero, return without a Hart turn or clock advance.
5. If the cursor is at the deadline, admit due Platform events and return.
6. If no Hart is Runnable, apply the WFI/idle rules in §8.
7. Select a Runnable Hart according to the configured scheduler, subject to the
   canonical same-time ordering in §10.
8. At that Hart's pre-fetch boundary, sample interrupt inputs and architectural
   eligibility. If one is accepted, enter trap and consume one Hart turn.
9. Otherwise begin one instruction attempt. Complete its physical accesses and
   architectural transition; retire it or enter its synchronous trap.
10. Consume each returned physical delay exactly once, update Platform time and
    due events, and append causal Platform facts after the parent transition.
11. Deliver requested observations, record any delivery failure, decrement the
    budget, and return immediately for single-step, stop, quiesce, deadline,
    budget, precise-event, or other mandatory-boundary conditions.
```

Steps 2, 8, and 10 are synchronization points. No direct device callback may
mutate Hart state between them. If a block engine performs several internal
instructions, it must produce the same semantic boundaries and obey the same
sequence; it may not defer an accepted interrupt or fabricate a batch retirement.

### 4.4 Hosting modes

**Runner-driven native mode** has a Machine-associated scheduler. The Runner
submits grants and receives the same response described above; the Runner may
choose another grant after classifying or inspecting the response.

**External-kernel mode** uses the same exchange. The external kernel owns global
simulation time and the outer execution thread. It grants a start time and a
maximum end time (or a synchronization horizon); the Machine uses a local cursor
inside that grant and returns consumed time, facts, next-event information, and
whether it reached the grant boundary. The kernel commits global time only from
the returned amount. The Runner remains the ruscv-sim adapter and classifier; it
does not need to own `sc_start`, an HDL loop, or any other external kernel API.

A Machine must not advance the external kernel's global time behind its back. A
native scheduler must not invent a second Hart execution engine. Both modes use
the same interrupt sample point, delay rules, WFI rules, and fact ordering.

## 5. Modeled time and physical delay

### 5.1 Minimal ISS time policy

The baseline functional ISS uses a configured abstract duration called one
`iss_tick`:

- The virtual-time cursor advances by one `iss_tick` for each completed Hart turn:
  an instruction retirement, a synchronous architectural trap entry, or an
  accepted interrupt trap entry.
- A WFI retirement receives the normal one-`iss_tick` charge. The following
  `Waiting` interval receives no per-step charge.
- Every physical transaction contributes its converted nonnegative delay once,
  after that transaction completes. Delays from multiple accesses in one Hart
  turn are accumulated in their semantic completion order. A Platform event whose
  due time falls inside an in-flight turn is not delivered mid-turn; it is
  admitted at turn completion, with its due time retained separately from its
  effective boundary time.
- A Platform-only idle jump advances virtual time directly to the next permitted
  event or grant deadline. It does not increment `minstret`, instruction
  attempts, or Hart turns, and it does not receive an artificial `mcycle`
  instruction charge.
- The default functional mapping is one virtual-time tick to one Platform
  `mtime` tick. This is a reproducible logical-clock choice, not a hardware
  frequency claim. A configured clock ratio may replace it.
- Minimal `mcycle` increments by one counter unit for each completed Hart turn;
  minimal `minstret` increments only for retired instructions as specified in
  §3.1. Physical delay and idle advance do not silently increment either counter.
- A simulator failure does not create a completed Hart turn or a fabricated
  retirement/counter update. It reports any known attempted work and consumed
  delay, then stops the deterministic run.

The policy intentionally relates the quantities without identifying them:

```text
virtual_time_delta = completed_turns * iss_tick
                    + converted_transaction_delay
                    + Platform_idle_advance
mcycle_delta        = completed_turns                 (minimal policy)
minstret_delta      = retired_instructions
mtime               = Platform_clock_map(virtual_time)
```

The equation is a semantic accounting rule for the baseline, not a microarchitectural
model. A richer timing profile may assign instruction, trap, bus, arbitration,
cache, or idle costs and a different `mcycle` charge. It must name those mappings,
keep ownership and monotonicity intact, and return enough accounting to distinguish
them. It may not make host elapsed time the implicit guest clock.

### 5.2 Units, precision, and conversion

A Machine configuration declares the canonical virtual-time unit and its supported
precision. Physical and external-kernel delays are converted before they enter the
cursor:

1. exact conversion is used when the source unit is representable in the
   canonical timebase;
2. otherwise a positive delay is rounded **up** to the next canonical tick, while
   zero remains zero; and
3. conversion overflow, a negative delay, or an otherwise malformed annotation
   is a simulator/adapter failure, not a negative or wrapped clock update.

The round-up rule prevents a real modeled delay from disappearing and makes a
precise deadline conservative. A richer timebase may use exact rational/fixed
point accumulation, provided it gives the same monotonic no-loss guarantee. The
chosen conversion and any rounding remainder are part of the run configuration
or returned diagnostic accounting; they are not inferred from host clock
resolution.

Platform `mtime` conversion uses a declared clock mapping. A mapping may be exact
or use an integer ratio with retained remainder so repeated advances do not drift.
Normal clock advancement never decreases `mtime`. Reset establishes a new initial
value. `mtimecmp` is compared in Platform time and may move independently when an
MMIO write changes it. A timer source is asserted whenever the selected Platform
contract says the `mtime >= mtimecmp` condition is true; the assertion is updated
at the same synchronization boundary as the clock advance.

### 5.3 Delay consumption by outcome

The Hart does not sleep on a delay annotation. The Machine/outer scheduler consumes
it as modeled elapsed time.

| Physical response | Architectural result | Delay rule |
| --- | --- | --- |
| Successful access | Hart may complete its transition and retire if all other semantics succeed | Consume the entire reported delay exactly once. |
| Guest-visible target/device fault | Hart applies the original access kind's architectural fault and enters trap; the instruction does not retire | Consume the entire reported delay exactly once, including a successful earlier page-table/A-D transaction in the same attempt. |
| Known adapter/transport failure before completion | No invented guest trap or success | Consume only the delay explicitly marked as elapsed/committed before failure; then return `SimulatorFailure`. |
| Unknown completion or partial transport result | Architectural state after the operation is not trusted | Preserve known committed delay and facts, do not retry or roll back, report time/state uncertainty, and require adapter resolution or reset before another deterministic grant. |

A delay already consumed is never undone because a later access faults or a Runner
chooses a different primary reason. If a backend violates a declared deadline
bound by reporting an unanticipated delay, the completed facts remain ordered and
the response includes the overshoot plus `SimulatorFailure`; rollback is not a
baseline requirement.

A missing delay annotation means zero delay under the minimal ISS policy. A richer
profile must explicitly select a missing-delay policy; the Hart must not guess one.
Host elapsed time is never added to this accounting unless an adapter explicitly
turns it into an admitted external control/input event.

## 6. Interrupt contract

### 6.1 Source assertion, pending state, and ownership

Interrupt handling has three distinguishable stages:

1. **Source assertion:** a Platform source becomes active because of a timer,
   device condition, software request, external input, or another modeled event.
   The Platform owns source level, edge tokens, controller priority, claim/complete
   state where applicable, and the wiring target. The exact CLINT/PLIC map is
   deferred.
2. **Architectural pending:** Machine delivers a timestamped normalized input at
   a synchronization boundary. The Hart's architectural pending representation
   and privilege/delegation view reflect that input according to the selected ISA
   profile. A level source remains pending while its level is active; a queued edge
   contributes one pending token per admitted edge. Deasserting a level removes
   the source condition if no architectural latch requires it; deasserting an edge
   source does not erase already queued tokens.
3. **Acceptance:** at a Hart pre-fetch boundary, the Hart evaluates pending causes,
   enables, current privilege, delegation, and Platform/controller priority. An
   accepted cause is removed or claimed according to its source/profile semantics
   and enters the architectural trap path.

Pending is not acceptance. A masked or lower-priority source remains observable as
pending and is not reported as an accepted interrupt. A level source that remains
asserted may become pending again after an acceptance; an edge source with multiple
queued assertions cannot be collapsed into one Boolean fact. A source may be
reported as asserted even when it is not yet eligible.

The baseline priority rule is the selected architectural/profile priority table.
If that table leaves a tie, the lower architectural cause number wins, followed by
the earlier admitted source sequence. The rule is semantic; it does not prescribe
a PLIC register layout or a container iteration order.

### 6.2 Sampling and acceptance boundary

At every precise pre-fetch boundary the Machine first applies all Platform events
whose modeled timestamp is at or before the current cursor. The Hart then samples
its interrupt inputs and eligibility before fetching the next instruction.

- If an interrupt is eligible, the Hart accepts the highest-priority cause and
  enters trap before fetch. No instruction attempt starts, the current PC is not
  replaced by a fetched instruction, and `minstret` does not increment. The
  accepted interrupt is one Hart turn and one `TrapEntered` control fact.
- If no interrupt is eligible, the Hart begins exactly one instruction attempt.
  A synchronous exception raised by that instruction is ordered after its access
  effects and enters trap without retirement. An interrupt asserted while that
  instruction is in flight is not an asynchronous preemption; it is pending for
  the next boundary.
- If an interrupt and an instruction breakpoint/exception could both be observed
  at the same pre-fetch boundary, the accepted interrupt wins because the
  breakpoint instruction was not fetched. The breakpoint or other source remains
  represented by its pending/control fact as applicable.
- If a synchronous exception and a newly asserted interrupt become known while
  the instruction is executing, the synchronous exception completes first. The
  interrupt remains pending and is considered at the next pre-fetch boundary.

Trap-entry effects, saved PC/cause, and privilege transitions remain Hart-owned
under ADR-0001. This ADR fixes only when the interrupt can enter that path and how
its time/progress facts are accounted.

### 6.3 Precise versus reduced-accuracy execution

An optimized block or translated execution strategy is equivalent to single-step
execution only if it preserves the following semantic boundary:

```text
sample Platform events and interrupt eligibility
  -> accept interrupt OR execute one instruction attempt
  -> commit/retire or enter synchronous trap
  -> consume delay and admit causal events
  -> expose the next boundary
```

In precise mode, a block must end before an interrupt, stop request, timer event,
observation demand, or deadline that could become effective within the block. It
may use conservative lookahead and still execute internally, but it must expose
per-instruction outcomes where required.

Reduced-accuracy execution is permitted for a caller that explicitly requests it.
It must declare a finite maximum interrupt latency (in Hart turns and/or virtual
time) and a finite maximum deadline overshoot if applicable. It may observe a line
at block exit rather than retroactively interrupting an earlier instruction, but
it must return the bound and the fact that reduced accuracy was used. An unbounded
hidden block is not a conforming implementation.

## 7. Budgets, deadlines, and mandatory early returns

### 7.1 Budget semantics

The Hart-turn budget is consumed when a Hart turn completes or fails after being
started:

- **Zero:** admit already-arrived events at the current boundary if needed for
  reporting, but perform no Hart turn, instruction fetch, interrupt acceptance,
  WFI transition, idle clock advance, or Platform event-time jump. Return a
  `BudgetExhausted` fact. A pending eligible interrupt is reported as pending,
  not accepted.
- **Finite N:** perform at most N Hart turns. If the Nth turn retires an
  instruction, enters a trap, or accepts an interrupt, finish all causal delay and
  Platform-event accounting for that turn, append the resulting facts, then
  return `BudgetExhausted` before starting turn N+1.
- **Unbounded:** no count limit is imposed, but deadlines, stop requests, precise
  event boundaries, single-step, failures, and quiescence still force return.

A single-step control request is a one-turn budget with a `SingleStepBoundary`
fact, including when that one turn is an accepted interrupt entry. It is not
necessarily one retired instruction.

### 7.2 Deadline semantics

A deadline is an absolute virtual-time value, not a number of instructions and
not host wall time. The bound is inclusive for completion:

- if `now == deadline`, no Hart turn begins;
- a known-cost turn may begin only when its conservative completion bound is
  `<= deadline`;
- a turn that completes exactly at the deadline is valid, and the Machine admits
  all Platform events due at that timestamp before returning `DeadlineReached`;
- if the next known idle wakeup/event is after the deadline, an idle Machine may
  advance to the deadline and return `DeadlineReached`, without retiring a fake
  instruction; and
- if precise deadline safety cannot be established because a required cost/delay
  bound is unavailable, the Machine returns a simulator/adapter failure before
  speculative work, unless the caller explicitly selected reduced accuracy.

A timer or input event at the same timestamp as the deadline is not lost. It is
admitted and included in the response before `DeadlineReached`; no Hart turn is
started after the deadline in that exchange.

### 7.3 Mandatory early returns

Regardless of budget, a Machine must return at the next coherent boundary for:

- an effective external stop or quiesce/drain request;
- budget exhaustion or an inclusive deadline;
- a requested single-step boundary;
- a precise interrupt or Platform event boundary that cannot be crossed safely;
- a materialized-observation demand that requires the response now;
- Platform exit, a terminal guest trap under the active trap policy, Hart Debug
  Mode, observer failure, or simulator/adapter failure; and
- quiescence when there is no runnable Hart and no legal future event to advance to.

The list describes return conditions, not a one-value result enum. All applicable
facts are returned together.

## 8. WFI, waiting, runnable, and quiescent state

### 8.1 WFI retirement

For the baseline profile, WFI is an ordinary Hart instruction with a special
post-retirement state transition:

1. If an eligible interrupt is present at the pre-fetch boundary, it is accepted
   before WFI fetch; WFI does not retire.
2. Otherwise WFI executes and retires once. It produces one instruction attempt,
   one retirement-progress increment, one `minstret` increment, and the minimal
   `mcycle`/`iss_tick` charge.
3. At the post-retirement boundary, if a WFI wake condition is already present,
   the Hart remains `Runnable` so the next turn can accept the interrupt or
   execute according to eligibility. It is never both waiting and executing.
4. If no wake condition is present, the Hart enters `Waiting`. Waiting is a Hart
   state, not an instruction outcome and not a stop reason.

The selected ISA profile may define WFI as a legal no-op or give it additional
privilege behavior; those architectural details remain Hart-owned. The boundary
above applies whenever the profile implements WFI waiting semantics. A profile
that treats WFI as a no-op must say so in its Hart policy rather than silently
spinning in the scheduler.

The minimal ISS wake policy is conservative: an interrupt pending for the Hart,
even if currently masked and therefore not acceptable as a trap, wakes a waiting
Hart; acceptance still requires the normal eligibility checks. A Platform may
also mark a source as an explicit WFI wake source. An unrelated Platform event
does not wake a Hart unless its Platform contract says it is such a source.

### 8.2 Runnable and quiescent states

- **Runnable:** the Hart can receive a turn. It may have no eligible interrupt,
  in which case it can attempt an instruction, or it may have an eligible
  interrupt, in which case the next turn accepts it before fetch.
- **Waiting:** the Hart has retired WFI and cannot receive ordinary instruction
  turns until a permitted wake source is admitted. Waiting does not consume
  budget, increment `mcycle`, or advance time by itself.
- **Stopped/terminal:** the Hart or run has been halted by a terminal architectural
  or control fact. Its state is reported; it is not selected by a normal grant.
- **Machine Quiescent:** at a coherent boundary no Hart is Runnable, no Hart turn,
  physical transaction, or observation delivery is in flight, and there is no
  immediately admissible Platform work. Waiting Harts may remain in the Machine;
  quiescence is not the same as reset or process exit.

If one Hart is Runnable, the Machine must not idle-jump merely because another Hart
is Waiting. If all Harts are Waiting and a known timer/input/platform event exists,
the scheduler may advance to the earliest such event, wake the affected Harts, and
return to the normal pre-fetch interrupt rule. If no event exists, it returns
quiescence rather than polling WFI.

### 8.3 Legal idle jumps

An idle jump is legal only when no Hart is Runnable and no in-flight operation
exists. The destination is the minimum of:

- the earliest known timer compare crossing or other Platform event;
- the earliest admitted external/replay input timestamp;
- the external-kernel synchronization/grant end; and
- the caller's virtual-time deadline.

The Machine must not jump over an earlier event, and it must not invent an
instruction, retirement, `mcycle` charge, or interrupt acceptance during the jump.
At the destination it advances `mtime`, asserts due level sources, enqueues due
edge sources, records all Platform facts, and makes waiting Harts Runnable when
their wake policy is satisfied. An already-due event has destination `now` and
requires no positive jump.

## 9. Platform events, exit, and causal commit order

A Platform event is a fact emitted by Platform state advancement, an MMIO/device
operation, an external input, or a host service. It carries an occurrence/due time
when one is known and an effective boundary time at which it becomes observable.
An event due during an in-flight turn is effective when that turn completes; it is
not an asynchronous preemption. A Platform event is not automatically a Runner
stop. The Machine carries it with the Hart transition that caused it or with the
Platform admission that produced it.

A successful physical write that requests a Platform exit is ordered as:

```text
physical write accepted -> Hart architectural store completes
-> instruction retires and counters/observation update
-> Platform emits PlatformExit with the parent transition provenance
```

Thus a `tohost`/HTIF-style exit cannot erase a successful store or make a retired
instruction appear unretired. A target faulting write emits a guest-visible fault
instead of a successful exit; an adapter failure with unknown write completion
emits `SimulatorFailure` and never fabricates either result.

Causal Platform events are admitted after their parent transition at the same
effective boundary time. Independent events that were already due are admitted
before the next Hart pre-fetch sample. This rule makes the interrupt visible to the
next boundary without allowing it to preempt the operation that caused it.

## 10. Non-lossy facts, deterministic ordering, and primary reason

### 10.1 Fact categories

The Machine returns an ordered, non-lossy collection of facts. The representation
may be a list, set plus sequence metadata, or another structure later; the
semantics are the same. Facts include, as applicable:

- `InstructionRetired` and `InstructionAttempted` progress;
- `TrapEntered`, with synchronous versus interrupt cause and Hart provenance;
- interrupt source assertion, pending, deassertion, wakeup, and acceptance;
- timer/Platform events and `PlatformExit`;
- `BudgetExhausted`, `DeadlineReached`, `SingleStepBoundary`, and
  `ExternalStop`;
- Hart Debug Mode or trigger halt when that feature exists;
- observer/reporting failure;
- simulator/adapter failure, including unknown completion or deadline-safety
  failure; and
- `Quiescent`.

An interrupt accepted as a normal architectural transition is a fact but is not a
terminal stop in the minimal Runner policy. A pending-but-masked interrupt is not
renamed as an acceptance. A fact is never removed because another fact is selected
as primary.

### 10.2 Canonical ordering

Facts are ordered by effective boundary time, causal edges, and stable identities,
not by thread completion, hash-map order, or callback arrival order. An event due
inside an atomic turn may retain an earlier occurrence/due time for diagnostics,
but its effective boundary time is the turn's completion time and it cannot sort
before the parent transition. The canonical order is:

1. increasing effective boundary timestamp;
2. for one timestamp, causal predecessors before their descendants;
3. at a boundary, Platform/input admission before the Hart pre-fetch sample;
4. a Hart transition (interrupt acceptance or instruction outcome) before its
   causal Platform event;
5. completed transition/event facts before the boundary facts that caused the
   exchange to return (`SingleStepBoundary`, budget, deadline, or effective stop);
6. for otherwise independent Platform facts, stable Platform source identity then
   admitted source sequence; and
7. for otherwise independent Hart transitions, ascending stable Hart ID then the
   Hart's local transition sequence.

A causal event inherits its parent's position and is ordered immediately after the
parent's completed architectural effects, subject to any earlier timestamped
Platform event that must be admitted first. At one timestamp, all due timer
levels are updated before the ordered Harts sample them; Hart 0 accepting a timer
interrupt therefore cannot prevent Hart 1 from seeing its independently eligible
timer input. If two transitions contend for a shared Platform effect, the effect
is committed in this same canonical order or the configuration is rejected as
unable to provide deterministic semantics.

This is a required observable order, not a required scheduler algorithm. A
round-robin scheduler, event queue, batched interpreter, or parallel scheduler
may be used if its committed facts and shared-state effects are equivalent to the
order above.

### 10.3 Primary-reason policy

The Machine does not select a primary reason. After it has collected and ordered
all facts for a response, the minimal Runner policy selects the first applicable
terminal category by this explicit rank:

1. simulator/adapter failure;
2. observer/reporting failure;
3. effective external stop request;
4. Hart Debug Mode or trigger halt;
5. requested single-step boundary;
6. successful Platform exit;
7. terminal synchronous guest exception or breakpoint under the active
   `stop-on-synchronous-trap` policy;
8. virtual-time deadline;
9. Hart-turn budget exhaustion; and
10. Machine quiescence.

Accepted interrupts do not stop by themselves. The minimal ISS policy continues
after accepted interrupts and treats every synchronous `TrapEntered` (including a
breakpoint exception) as terminal. A Runner may explicitly select a different
trap-continuation policy, but it must report that policy and must not discard the
trap fact. Platform exit outranks a coincident budget because
the guest completed its causal exit operation. Deadline outranks budget because
its time safety is the stronger bound. External stop outranks a coincident guest
exit because it records the user's explicit control request; both facts remain.

The rank is presentation policy, not architectural precedence. For example, a
successful instruction, `PlatformExit`, `BudgetExhausted`, and an external stop
may all be present after one final boundary; the Runner reports external stop as
primary under this policy while preserving the exit and budget facts and the
retired instruction.

## 11. Multi-Hart determinism and conditional replay

### 11.1 Same-time Harts

Every Hart has a stable semantic Hart ID. For N Harts sharing a Platform:

- Platform clock advancement and due-source updates happen once for the shared
  timestamp before Hart acceptance sampling.
- Independent Hart transitions at the same timestamp are canonically ordered by
  Hart ID and each Hart's local transition sequence, after already-admitted
  Platform facts.
- A transition's physical effects, retirement/trap facts, and causal Platform
  events form one ordered unit. Another Hart cannot observe a shared effect before
  that unit's commit point.
- The scheduler is not required to run Harts serially in its implementation, but
  parallel execution must preserve the canonical commit order or prove that the
  effects commute. Host thread scheduling must not choose the guest-visible order.
- An interrupt accepted by one Hart does not clear another Hart's pending source
  unless the Platform/controller contract explicitly makes it a claimed shared
  source. A shared source's claim/complete semantics belong to Platform, not to
  Hart ID iteration.

The exact fairness algorithm, quantum size, and inter-Hart memory/DMA coherence
mechanism are deferred. The determinism rule is not deferred: an unsupported
combination must be rejected rather than silently produce host-order-dependent
facts.

### 11.2 Conditional replay input ordering

This ADR does not promise a replay file format. If deterministic replay is offered,
all asynchronous host inputs that can affect guest-visible state must first be
admitted as timestamped Platform events at a Machine synchronization boundary.
Each input carries a stable source identity and admission sequence. The canonical
ordering is `(modeled timestamp, source identity, admission sequence)` for
independent inputs, with causal ordering taking precedence. Same-timestamp inputs
therefore have one repeatable order, and an input cannot arrive by mutating a
Hart or device from an arbitrary host thread.

If no replay mode is selected, host arrival order may be nondeterministic and is
not converted into a false deterministic claim. A replay-capable adapter must
record the admitted order it replays, not merely the wall-clock order in which
host threads happened to receive data.

## 12. Synchronization, quiesce, and drain

### 12.1 Required synchronization points

A Machine synchronizes and exposes a boundary before a Hart can observe any of the
following:

- a newly asserted/queued interrupt or timer crossing;
- an external/replay input;
- a non-local physical transaction result or delay;
- a Platform event or exit;
- a debugger, trigger, stop, or quiesce request;
- a deadline or precise observation demand; or
- a DMI/translation/block-execution invalidation that can change a result.

A native scheduler may combine adjacent points when the resulting committed order
and declared precision are identical. An external kernel may use its own delta
cycles or synchronization primitive; the Machine contract remains the same.

The baseline is **no rollback**. A scheduler must use conservative grants/lookahead
when it needs a precise bound. It may use reduced accuracy only with the declared
finite bounds in §6.3 and §7.2. Completed architectural effects and consumed
modeled delay are never undone to honor a later stop or to retry an unknown
transaction.

### 12.2 Quiesce and drain

A quiesce request takes effect at the next coherent boundary. The Machine then:

1. stops starting new Hart turns and new Platform transactions;
2. completes/drains the already active atomic transaction and any already-accepted
   observation delivery, or reports a failure if completion is unknown;
3. admits and returns all facts caused by that work;
4. confirms that no Hart turn, physical transaction, callback, or observation
   delivery remains in flight; and
5. exposes a `Quiescent` lifecycle state to the Runner or external kernel.

Only in that state may lifecycle operations such as reset, image installation,
Platform composition changes, debugger memory/register mutation, or teardown
change architectural or Platform state. A future checkpoint/restore operation
must use the same drain boundary and must invalidate any cached translation,
compiled block, DMI, or derived interrupt eligibility that its implementation
uses. The checkpoint representation itself is deferred.

Quiesce is not an implicit reset, and a waiting Hart is not an in-flight Hart. An
external kernel must hold or explicitly coordinate global time while a Machine is
being drained; it must not call into the Machine concurrently with mutation.

## 13. Alternatives considered

### A. Treat one instruction as one architectural cycle

Rejected. It conflates `mcycle`, `minstret`, Hart attempts, transaction delay, and
Platform time, making WFI and external-kernel timing incorrect. The minimal ISS
uses an explicit abstract tick policy instead.

### B. Let the Hart poll interrupts opportunistically

Rejected. Polling only at Runner loop boundaries or block exits can lose precise
interrupt entry and makes latency depend on the chosen execution strategy. The
pre-fetch sample is fixed; reduced accuracy must declare a finite bound.

### C. Return only one stop reason

Rejected. A final store can retire, emit Platform exit, exhaust a budget, and
coincide with a user stop. Collapsing those facts loses evidence and makes
classification order-dependent. The Machine returns facts first; the Runner
selects a primary reason second.

### D. Drive guest time from host wall time

Rejected. Wall time is variable, hard to replay, and unrelated to guest progress.
It remains an observation/watchdog input unless explicitly admitted as a control
source.

### E. Use rollback to make deadlines exact

Rejected as a baseline. Rollback complicates device side effects, host services,
observers, and external kernels. Conservative grants provide precision without
requiring rollback; reduced accuracy is explicit when conservative bounds are not
available.

### F. Spin WFI one instruction at a time

Rejected. It consumes budgets and host work while no architectural progress is
possible, and it can prevent timer-driven wakeup when time is only advanced by
execution. Waiting plus legal idle jumps is the contract.

### G. Fix a scheduler algorithm or quantum in the ADR

Rejected. The observable boundary and same-time commit order are architectural
contracts; round-robin, event-driven, block, and parallel strategies remain
replaceable implementation choices.

## 14. Consequences

### Positive

- Interrupt visibility and trap entry have one precise location independent of
  interpreter, block, native, or external-kernel hosting.
- `mcycle`, `minstret`, `mtime`, virtual time, physical delay, and host elapsed time
  cannot be accidentally substituted for one another.
- Exact zero/finite limits and absolute deadlines can be implemented without
  retroactive unretirement or rollback.
- WFI can make a Machine genuinely idle and let a timer wake it with no fake
  instruction progress.
- Exit, trap, debug, budget, deadline, observer, and simulator facts remain
  available for diagnosis and deterministic Runner classification.
- Stable same-time ordering gives N-Hart and replay-capable implementations a
  semantic target without freezing their scheduling machinery.
- Native and external-kernel hosting share one exchange and one Hart engine.

### Costs and risks

- A physical backend must report delay semantics and, for precise bounded grants,
  a conservative completion bound.
- Optimized execution must preserve or explicitly bound interrupt/event latency.
- Multi-Hart parallelism may need ordered commits or commutativity checks.
- Platform event sources need stable identities and admission sequence metadata.
- Primary-reason policy remains Runner configuration, so callers must not mistake
  the presentation result for the complete Machine fact set.
- The minimal logical clock is useful for functional simulation but is not a
  performance or silicon-timing model; richer timing requires a later explicit
  profile.

## 15. Invariants and edge-case ordering

The implementation and later ADRs must preserve these invariants:

1. Virtual time and normal Platform clock advancement are monotonic within a run.
2. A consumed delay is counted once and never erased by a later fault, stop, or
   classification decision.
3. `minstret` counts only retired instructions; interrupt entry and faulting
   instructions never increment it.
4. No instruction is fetched after an accepted pre-fetch interrupt, and no
   interrupt preempts an in-flight instruction.
5. A retired instruction cannot be unretired because it caused exit or because an
   observer failed.
6. Zero budget and an reached deadline perform no subsequent Hart turn.
7. A waiting Hart does not consume instruction budget or `mcycle` while idle.
8. Every applicable fact is returned, including facts that are not primary.
9. Same-time independent facts and shared effects do not depend on host thread,
   map, queue, or callback iteration order.
10. Quiesce exposes no in-flight architectural, physical, or observation work.
11. Unknown completion is never retried as if it were a clean failure and never
    converted into a guest-visible trap without evidence.

The following cases make the required ordering concrete:

| Situation at one exchange boundary | Required semantic order | Facts/primary under minimal policy |
| --- | --- | --- |
| Zero budget with an eligible pending interrupt | Admit current-time inputs; do not accept or fetch; return | Pending interrupt + `BudgetExhausted`; primary budget |
| One remaining turn and the interrupt is eligible | Accept interrupt as the final turn; complete trap accounting; stop | `TrapEntered`, then `BudgetExhausted`; primary budget unless a higher fact exists |
| One remaining turn and the instruction retires then writes exit | Complete write; retire/counters; emit exit; return | Retirement, `PlatformExit`, `BudgetExhausted`; primary Platform exit |
| Deadline equals the end of the final instruction | Complete instruction and delay; admit due events; return | Retirement/events, then `DeadlineReached`; primary deadline unless exit/failure |
| Timer crosses during a non-WFI instruction | Finish instruction; advance/admit timer; sample at next boundary | Retirement, timer assertion/pending; no mid-instruction trap |
| Eligible interrupt and breakpoint at pre-fetch | Accept interrupt before fetch | Interrupt `TrapEntered`; breakpoint remains unexecuted/pending |
| Synchronous exception and interrupt asserted during that instruction | Finish synchronous exception; leave interrupt pending | Synchronous `TrapEntered`, pending interrupt; primary synchronous trap if terminal |
| WFI with no wake source | Retire WFI; enter `Waiting`; idle-jump or return quiescence | Retirement, waiting transition, then timer/wakeup or `Quiescent` |
| Timer/input exactly at deadline while all Harts wait | Advance/admit event at deadline; do not start a Hart turn | Event, wake/pending facts, `DeadlineReached`; primary deadline |
| Guest exit and external stop admitted at the same boundary | Preserve commit and both facts | `ExternalStop` primary, `PlatformExit` retained |
| Observer fails after a committed instruction | Keep commit/progress; stop delivery and return | Retirement, observer failure; primary observer failure |
| Adapter reports unknown write completion | Do not claim commit, exit, or guest trap; preserve known prefix | `SimulatorFailure` with uncertainty; primary simulator failure |
| Two Harts have due timers at the same timestamp | Update shared clock/lines; accept in stable Hart-ID order | Both interrupt facts; no Hart's acceptance hides the other's pending state |

## 16. Verification scenarios

These scenarios are acceptance evidence for a future implementation. They are
not claims about the current public ELF path.

| Scenario | Required evidence |
| --- | --- |
| Counter separation | A normal instruction, faulting instruction, accepted interrupt, WFI, and idle jump show distinct attempts, turns, `mcycle`, `minstret`, and virtual-time deltas. |
| Zero/exact/exhausted budget | Zero performs no Hart turn; N performs exactly N turns including interrupt entries; the Nth outcome and all causal facts precede `BudgetExhausted`. |
| Deadline safety | A turn ending before, exactly at, and beyond a deadline is tested; exact completion is allowed, crossing is rejected or reported with the declared reduced-accuracy/adapter-failure fact, and no rollback occurs. |
| Delay matrix | Success, guest-visible target fault, known adapter failure, and unknown completion verify the table in §5.3, including page-table/A-D delay and no retry. |
| Interrupt lifecycle | Level assertion while masked, deassertion before acceptance, queued edges, multiple pending causes, priority ties, and pre-fetch acceptance are tested. |
| Interrupt versus instruction | An interrupt asserted during fetch/load/store is accepted only at the next boundary; a synchronous exception from that instruction remains first. |
| Block equivalence | Precise block execution matches single-step outcomes and facts; reduced accuracy reports a finite measured/configured latency bound. |
| WFI/timer | WFI retires once, consumes no idle turns, idle-jumps to `mtimecmp`, wakes, and accepts the timer interrupt before the next instruction. No-event WFI returns quiescence. |
| Exit ordering | A successful exit-causing MMIO write is visible as a retired instruction before `PlatformExit`; a faulting or unknown write is not reported as a successful exit. |
| Co-incident facts | Exit, trap, budget, deadline, external stop, observer failure, and simulator failure combinations retain every fact and apply the rank in §10.3. |
| N-Hart same-time order | Repeated runs under different host thread/container iteration orders produce the same canonical facts and shared-state effects, or reject unsupported conflict configurations. |
| Hosting parity | Runner-driven and external-kernel-driven grants produce the same Hart/Platform transitions, delay accounting, interrupt boundaries, and fact order. |
| Replay admission | A replay-capable adapter reproduces same-timestamp inputs by source identity and admission sequence rather than host arrival order. |
| Quiesce/drain | A stop during an active transaction drains or reports unknown completion, leaves no work in flight, and permits reset/mutation only after `Quiescent`. |

## 17. Compatibility, explicit deferrals, and relationship to earlier ADRs

This ADR is documentation-only. It does not change current CLI behavior, the
current `load_and_run` loop, Rust APIs, or component test results. Existing cycle
limits, HTIF/tohost handling, UART behavior, and public result construction remain
as implemented until a later implementation change adopts this contract. Current
CLINT, PLIC, TLM time, debug, MMU, and executor components are evidence and
integration inputs; their presence does not prove end-to-end support.

The following are explicit deferrals, not unresolved semantic choices:

- Rust layouts, enum/field names, constructors, trait signatures, lifetimes,
  callback/channel/queue structures, serialization, and compatibility shims;
- scheduler algorithm, fairness, exact quantum, batching strategy, performance
  defaults, and the concrete representation of canonical sequence metadata;
- the richer cycle/timing model and hardware-frequency calibration beyond the
  selected minimal ISS policy;
- CLINT/PLIC/UART/HTIF device maps, register details, interrupt-controller
  claim/complete registers, and full Platform composition;
- DMA/inbound Platform masters, memory ordering, cache coherence, reservation
  granules, and full N-Hart shared-memory semantics;
- checkpoint/save-state formats and restore tooling, while retaining the required
  quiesce/drain and cache-invalidation boundary;
- full RISC-V Debug Mode, trigger-module halt, `dcsr`/`dret`, and their detailed
  Hart outcomes; and
- SystemC/TLM/HDL FFI, callback, delta-cycle, and external-kernel APIs.

The selected ISA profile still owns exact trap causes, delegation, interrupt enable
bits, WFI legality, and architectural counter access. This ADR does not invent a
new ISA profile or device map; it fixes the boundary at which those profile rules
interact with scheduling and Platform time.

ADR-0001 remains the source of truth for Hart transition effects and optional
observations. ADR-0002 remains the source of truth for raw physical transactions,
target faults, adapter failures, and delay metadata. ADR-0003 remains the source of
truth for Runner/Machine/Platform ownership, dual hosting, and final result
assembly. This ADR supplies the detailed temporal/event contract those records
left to a subsequent decision; it does not move instruction or physical-access
ownership across their boundaries.

## 18. Source and test map

The current repository evidence that motivates this contract includes:

- Hart stepping, fetch/decode/execute integration, and current result boundaries:
  [`src/core/mod.rs`](../../../src/core/mod.rs),
  [`src/execute/mod.rs`](../../../src/execute/mod.rs), and
  [`src/executor.rs`](../../../src/executor.rs).
- Trap causes/context and CSR counter/interrupt state:
  [`src/core/trap.rs`](../../../src/core/trap.rs),
  [`src/csr/mod.rs`](../../../src/csr/mod.rs), and
  [`src/isa/rv64i/system.rs`](../../../src/isa/rv64i/system.rs).
- Physical/MMU access paths and translation-stage effects:
  [`src/memory/mod.rs`](../../../src/memory/mod.rs),
  [`src/mmu/mod.rs`](../../../src/mmu/mod.rs),
  [`src/mmu/physical.rs`](../../../src/mmu/physical.rs),
  [`src/mmu/translator.rs`](../../../src/mmu/translator.rs),
  [`src/mmu/sv39.rs`](../../../src/mmu/sv39.rs), and
  [`src/mmu/tlb.rs`](../../../src/mmu/tlb.rs).
- Existing Platform/peripheral and TLM timing vocabulary:
  [`src/peripherals/clint.rs`](../../../src/peripherals/clint.rs),
  [`src/peripherals/plic.rs`](../../../src/peripherals/plic.rs),
  [`src/peripherals/uart16550.rs`](../../../src/peripherals/uart16550.rs),
  [`src/tlm/time.rs`](../../../src/tlm/time.rs),
  [`src/tlm/bus.rs`](../../../src/tlm/bus.rs),
  [`src/tlm/payload.rs`](../../../src/tlm/payload.rs),
  [`src/tlm/status.rs`](../../../src/tlm/status.rs), and
  [`src/tlm/traits.rs`](../../../src/tlm/traits.rs).
- Existing debugger and breakpoint/watchpoint boundaries:
  [`src/debug/mod.rs`](../../../src/debug/mod.rs),
  [`src/debug/breakpoint.rs`](../../../src/debug/breakpoint.rs), and
  [`src/debug/watchpoint.rs`](../../../src/debug/watchpoint.rs).
- Focused component/integration evidence for executor behavior, traps, privilege,
  CSR state, peripherals, TLM, translation, and MMU effects:
  [`tests/executor.rs`](../../../tests/executor.rs),
  [`tests/trap_test.rs`](../../../tests/trap_test.rs),
  [`tests/privilege_transition_test.rs`](../../../tests/privilege_transition_test.rs),
  [`tests/csr_basic_test.rs`](../../../tests/csr_basic_test.rs),
  [`tests/csr_access_test.rs`](../../../tests/csr_access_test.rs),
  [`tests/peripheral_tests.rs`](../../../tests/peripheral_tests.rs),
  [`tests/peripheral_proptest.rs`](../../../tests/peripheral_proptest.rs),
  [`tests/tlm_tests.rs`](../../../tests/tlm_tests.rs),
  [`tests/translation_test.rs`](../../../tests/translation_test.rs),
  [`tests/sv39_test.rs`](../../../tests/sv39_test.rs),
  [`tests/ad_bits_test.rs`](../../../tests/ad_bits_test.rs), and
  [`tests/tlb_test.rs`](../../../tests/tlb_test.rs).

These sources and tests show the current component vocabulary and gaps; they are
not evidence that interrupt/time/stop integration already exists. Future
implementation work must add focused tests for the verification scenarios above
and then update the current-state evidence with actual results. This proposed ADR
has no superseding record.
