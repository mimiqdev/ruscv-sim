# ADR-0001: Hart Execution Outcome and Observation Records

**Status:** Proposed
**Authority:** Draft contract; normative only after acceptance
**Date:** 2026-09-03
**Owner:** Hart/core architecture

> This record defines the semantic Hart outcome, retirement, trap, and observation-ownership contracts. It is not a trace schema or a Rust implementation plan. The outcome and record names below are illustrative; this ADR does not prescribe enum or struct layouts, field types, serialization, wire formats, or migration mechanics.

## Context

The target architecture requires one RISC-V Hart implementation to serve both the standalone ISS and the future Virtual Platform. The Hart owns architectural state, instruction semantics, privilege, traps, address translation, and retirement; the Runner and Platform drive it and observe its facts. This boundary is described in the [target architecture](../README.md) and [architecture principles](../principles.md), while the [current implementation inventory](../current-state.md) records that the boundary is not yet implemented.

The current execution path cannot provide that contract:

- `RiscvCore::step` in [`src/core/mod.rs`](../../../src/core/mod.rs) returns `Result<()>`. It fetches, decodes, and invokes the instruction executor, but does not integrate `TrapHandler`, sample interrupt lines, or return an architectural outcome.
- `load_and_run` in [`src/executor.rs`](../../../src/executor.rs) snapshots registers around `step`, re-fetches the opcode for logging, increments its own cycle count, polls `tohost`, and converts every core error into an `ExecutionResult` error. `RiscVSimulator` contains a second, related loop.
- [`src/core/commits.rs`](../../../src/core/commits.rs) provides a Spike-shaped `CommitLogger` and a `MemoryAccess` helper, but the public runner currently passes no memory access and derives the register delta outside the Hart.
- [`src/core/trap.rs`](../../../src/core/trap.rs) provides causes, delegation, context, and trap-entry code. Its focused tests verify those components, but `RiscvCore::step` does not call the handler.
- [`src/execute/mod.rs`](../../../src/execute/mod.rs), [`src/memory/mod.rs`](../../../src/memory/mod.rs), and the instruction implementations expose execution and memory errors without a common distinction between an architectural exception and a simulator failure.

The result is that a Runner or observer can mistake a failed instruction for a retired instruction, reconstruct incomplete effects from snapshots, or conflate a guest trap with a host failure. That is unsuitable for differential testing, precise observation, Virtual Platform composition, or future block execution.

## Proposed decision

### 1. Hart step boundary and conceptual outcomes

One Hart architectural step starts from a coherent architectural state and completes with exactly one semantic outcome. The conceptual outcome set is:

| Conceptual outcome | Meaning | Required observation |
| --- | --- | --- |
| `InstructionRetired` | Exactly one guest instruction completed all of its architectural checks and effects and crossed the retirement boundary. | One `CommitRecord`, emitted from Hart-owned facts. |
| `TrapEntered` | An architectural synchronous exception or an eligible interrupt was accepted and trap entry completed. | One `TrapRecord`, emitted from Hart-owned facts; no instruction retires in this step. |
| `SimulatorFailure` | The Hart or an adapter could not complete the step as an architectural operation. This is not guest-visible trap entry. | A diagnostic owned by the execution-control layer; no commit or trap record for an incomplete step. |

These conceptual variants are a semantic contract, not a requirement to expose a particular Rust enum. A step with no accepted interrupt attempts one instruction. The interrupt, time, and stop-event decision owns interrupt eligibility, priority, masking, and timing; this ADR fixes the Hart boundary at which an accepted interrupt enters trap before fetching or executing an instruction.

`TrapEntered` is complete only after the architectural trap-entry state is established: the applicable saved PC, cause, and value and status state have been updated, privilege has changed as required by delegation, and the Hart PC is the selected handler target. If trap entry itself cannot be completed because of an implementation or host failure, the result is `SimulatorFailure`, not a partially recorded trap.

### 2. Retirement, trap, and fault rules

The following rules are normative for the semantic contract:

| Situation | Does an instruction retire? | Hart outcome | State visible at the boundary |
| --- | ---: | --- | --- |
| Instruction completes normally, including a taken branch, jump, CSR operation, atomic operation, or successful MMIO access | Yes, exactly once | `InstructionRetired` | All effects of that instruction and its next PC |
| Instruction fetch, decode, legality, privilege, translation, alignment, or execution raises a synchronous architectural exception | No | `TrapEntered` | Completed trap-entry effects and any other effects the selected ISA profile explicitly mandates for the faulting attempt; no partially applied instruction effects |
| An eligible interrupt is accepted at the sampling boundary | No | `TrapEntered` | Interrupt trap-entry effects only; no instruction fetch or execution effects |
| A physical port reports a guest-visible fault, after Hart classification | No | `TrapEntered` | Trap entry for the access-fault cause selected by the matrix below |
| A Hart invariant, host resource, unsupported legal operation, or physical backend failure prevents architectural completion | No completed instruction | `SimulatorFailure` | No partial Hart architectural state is exposed as a commit or trap |

A faulting instruction and interrupt entry therefore never retire an instruction. In particular, `minstret`-like retirement accounting must not count either the faulting instruction or an interrupt entry as a retired instruction. Other architectural counters follow their ISA-defined semantics and are recorded when they change; scheduler, wall-clock, and virtual-time accounting is outside this ADR and belongs to the interrupt, time, and stop-event decision.

A normal instruction's retirement is the point at which its architectural state transition becomes visible to the Hart's observers. The implementation may stage effects, use a journal, or use another mechanism; this ADR does not choose among those mechanisms. A failed instruction cannot leave partially applied GPR, floating-point register, CSR, privilege, PC, reservation, or other Hart architectural state behind. Any state change that the selected ISA profile explicitly mandates for a faulting attempt is part of the completed architectural outcome rather than a leaked partial effect.

#### Fault classification at the Hart boundary

The Hart preserves the original architectural access kind while translation is performed. Physical failures both after translation and during a page-table walk use that original kind; they do not become page faults merely because a walk was involved. Invalid or reserved PTEs, non-canonical virtual addresses, and PTE permission failures are translation faults and use the page-fault column:

| Original access kind | Physical fault after translation, or physical fault while reading/updating a PTE | Invalid/non-canonical/permission translation condition |
| --- | --- | --- |
| Instruction fetch | Instruction access fault | Instruction page fault |
| Data load | Load access fault | Load page fault |
| Data store or AMO | Store/AMO access fault | Store/AMO page fault |

The architectural trap value (`mtval`/`stval` or the corresponding profile state) for these address faults is the original faulting virtual address. The physical address, PTE address, and backend/device context may be retained as diagnostic context, but they do not replace the architectural trap value. Architectural misalignment is classified by the Hart before issuing a physical request and produces the corresponding misaligned exception.

A target-reported bus or device error for a valid routed physical request is a guest-visible physical access fault and follows this matrix. A host or adapter protocol failure, unavailable backend, or completion whose effect is unknown is a `SimulatorFailure`; it is never converted into a fabricated access-fault trap. A generic implementation error such as the current `MemoryError` must not decide this architectural mapping.

A successful translation-stage A/D-bit write is a separate physical effect. If the selected ISA profile uses hardware A/D updates and translation successfully performs that write, a later fetch/load/store/AMO physical access fault does not roll it back. The write is not a retired effect of a faulting instruction and is not included as such in its `CommitRecord`, but its successful memory effect remains visible. If the selected profile instead faults when an A/D update is required, that profile-defined behavior applies; this ADR chooses observability of a successful write, not an A/D update scheme.

An architectural `EBREAK` or other guest-generated breakpoint exception is a synchronous exception and therefore produces `TrapEntered`. A debugger breakpoint, user interruption, execution limit, scheduler stop, and platform exit are not Hart traps and do not change these rules. For example, an instruction that successfully writes a `tohost` or other platform-exit register first retires and produces its commit; the Platform/Runner may then observe the resulting platform event and stop the run.

### 3. CommitRecord semantic requirements

A `CommitRecord` describes one and only one retired instruction. It carries the following minimum semantic facts; exact representation remains open:

| Fact | Required meaning |
| --- | --- |
| Hart identity | Identifies the Hart that retired the instruction. Global multi-Hart ordering, if needed, is a Runner or scheduler concern. |
| Instruction identity | The instruction bytes or encoding observed by the Hart, with enough identity to distinguish its length. An observer must not re-fetch it. |
| PC transition | `before_pc` is the architectural PC at the start of the instruction. `after_pc` is the next architectural PC after retirement: fall-through, branch/jump target, or return target as applicable. |
| Privilege context | The privilege mode in which the instruction executed and the mode after retirement, including any defined privilege transition. |
| Architectural effects | The architecturally visible state transition caused by the retired instruction, including applicable register, CSR, counter, memory, atomic, and privilege effects. |
| Retirement status | The record itself establishes that exactly one instruction retired; no record is emitted for a failed or interrupted instruction. |

Effects are required semantically, not as a prescribed per-field trace format. Whether a representation uses deltas or old/new values, how it groups memory events, and which physical or backend diagnostics it presents are implementation and sink choices. A presentation sink may omit details without changing the Hart contract; it must not require a second state snapshot or instruction fetch to understand the architectural transition.

### 4. TrapRecord semantic requirements

A `TrapRecord` describes one completed architectural trap entry. It carries the following minimum semantic facts:

| Fact | Required meaning |
| --- | --- |
| Trap kind and cause | Whether the event is a synchronous exception or interrupt, and its architectural cause. The cause is not a generic host error string. |
| PC transition | `before_pc` is the PC at the trap boundary: the faulting instruction PC for a synchronous exception or the interrupted PC for an interrupt. `after_pc` is the selected trap-vector target after delegation and vector-mode calculation; it is not the saved return PC. |
| Saved exception PC | Identifies the architectural saved PC (`mepc`, `sepc`, or the corresponding profile context). It is not advanced as if the faulting instruction retired. |
| Privilege context | The source privilege at the boundary and the privilege selected to handle the trap after delegation. |
| Trap value | The architectural trap value, such as the original faulting virtual address or instruction value required by the ISA. Physical, PTE, and backend details are diagnostic context only and must not replace it. |
| Trap-entry effects | The architectural changes made by entry, including applicable trap CSRs, status bits, privilege state, and PC. |
| Retirement status | No faulting instruction or interrupt entry retired in this step. |

For a synchronous trap, instruction identity may be included when fetch or decode made it available; an observer must not infer missing identity by re-fetching memory. An interrupt accepted before fetch has no instruction context. The existing [`TrapContext`](../../../src/core/trap.rs) type and focused tests are implementation evidence, not this record's layout; the target contract must preserve the architecture's address and value width even if a later Rust representation changes from the current component type.

### 5. SimulatorFailure semantics

`SimulatorFailure` is reserved for an inability to complete an architectural operation, including an internal invariant violation, poisoned or unavailable host resource, unsupported implementation of an otherwise legal guest operation, physical transport/backend failure, or an access whose completion is unknown. It must not silently perform trap entry or claim retirement.

A simulator failure:

- is not written into guest trap CSRs and does not change privilege merely to make the error observable;
- does not produce a `CommitRecord` or `TrapRecord` for the incomplete step;
- carries enough diagnostic context for the Runner to report the failure, without making the diagnostic part of guest architecture; and
- leaves no partial Hart architectural state exposed as authoritative. If a backend reports an uncertain completion after a non-rollbackable external side effect, the run is a simulator failure with state requiring explicit failure handling, not a fabricated access-fault trap or commit.

A decoder's internal "not implemented" error is not automatically an illegal-instruction trap: an encoding that is architecturally legal but unsupported by the implementation is a simulator failure until the implementation supports it. Conversely, an encoding that the configured ISA profile defines as illegal is an architectural illegal-instruction trap. This classification belongs at the Hart boundary rather than in a generic Runner error conversion.

Observer delivery is downstream of the Hart outcome. If a sink fails after a completed `InstructionRetired` or `TrapEntered` outcome, the architectural transition and its record remain valid; the Runner may stop with a separate reporting/simulator failure according to its policy. An observer failure must never roll back a committed instruction or turn a completed trap into a different Hart outcome.

### 6. Visibility, atomicity, and observation ownership

The Hart owns the facts and the retirement boundary:

- It determines whether an instruction retired, a trap was entered, or the step failed.
- It constructs the semantic commit or trap facts from the same architectural transition that it applies.
- It emits each completed instruction or trap exactly once, after the corresponding state transition is complete.
- It exposes immutable observation facts to consumers; observers cannot mutate Hart state or execute a callback in the middle of an architectural transition.
- It owns architectural reservation state and the architectural LR/SC result, subject to the selected ISA profile.

The Runner owns observation delivery and run-level policy:

- It subscribes to Hart outcomes and fans the same facts out to commit loggers, traces, profilers, or other sinks.
- It decides how to handle a sink error and how to combine Hart outcomes with platform exit, debugger stop, limit, and scheduling events.
- It must not reconstruct a commit by comparing pre/post register snapshots, re-fetching an opcode, or guessing a memory access.

The Platform owns physical routing, devices, and platform events. The PhysicalAccess port supplies raw bytes and physical effects and provides an indivisible operation envelope for AMO and LR/SC operations; it reports target/device faults separately from host or transport failures. The port does not decide whether a valid physical failure is an instruction, load, or store/AMO access-fault—the Hart applies the matrix in §2 using the original access kind. Competing physical writes are observed through the port contract so the Hart can apply the selected ISA profile's reservation effects; the port does not become the owner of Hart architectural reservation state or the SC result.

#### LR/SC ownership boundary

This ADR deliberately fixes only the ownership boundary:

- the Hart owns per-Hart reservation state and the architectural result of LR/SC;
- PhysicalAccess owns the indivisible conditional-operation envelope and physical visibility of competing accesses, and reports the information needed for the Hart to apply reservation rules; and
- reservation granule, multi-Hart ordering, DMA coherence, and all reservation consumption or invalidation cases—including a faulting SC—follow the selected ISA profile. This ADR does not invent a new faulting-SC semantic from the current implementation.

For future block execution, a block may return an aggregate control result, but it must preserve one precise observation per retired instruction in architectural order. If a block encounters a trap, all earlier instructions in the block retain their individual commits, the faulting instruction has no commit, and the trap entry has one trap record. Speculative or prefetched work must not be exposed as an architectural record.

The Frontend owns presentation and serialization choices. No layer outside the Hart may create a second set of ISA semantics.

### 7. Boundaries with outer outcomes

The Hart step outcome is deliberately narrower than a Runner result:

| Event or condition | Hart step outcome | Owner of the outer run decision |
| --- | --- | --- |
| Guest instruction retires | `InstructionRetired` plus `CommitRecord` | Runner may continue or inspect resulting platform events |
| Architectural exception or accepted interrupt | `TrapEntered` plus `TrapRecord` | Hart/architecture determines entry; Runner decides whether/how to continue |
| `tohost`/HTIF or another guest-visible platform exit is written successfully | The writing instruction still returns `InstructionRetired` | Platform reports the event; Runner applies exit policy |
| Debugger breakpoint, user interruption, or debugger request | No special Hart trap outcome unless the guest executed an architectural breakpoint instruction | Runner/debug controller |
| Cycle/instruction limit | No special Hart trap outcome | Runner and, for time/scheduling details, the interrupt, time, and stop-event decision |
| Device scheduling, delay, or virtual time advancement | Not a Hart step outcome | Platform/Machine/Scheduler under the interrupt, time, and stop-event decision |
| Internal or host failure | `SimulatorFailure` if it prevents Hart completion | Runner reports/stops according to its policy |

This keeps platform exit, debugger stop, execution limits, scheduling, architectural traps, and simulator failures distinguishable as required by the architecture principles.

## Alternatives considered

### A. Keep `Result<()>` and let the Runner reconstruct effects

This is the current shape: the Runner snapshots registers, re-fetches the instruction, and turns a generic error into an execution result. It is rejected because the Runner cannot reliably observe CSR, floating-point-register, privilege, counter, atomic, or memory effects; it duplicates architectural knowledge; it already has an address-mapping/refetch failure mode; and it cannot represent a precise faulting instruction or interrupt boundary.

### B. Return only a coarse status and keep snapshots as the observation API

A status such as "continued" or "trapped" would improve control flow but still force observers to infer the commit from mutable state. Full state snapshots are expensive and ambiguous for memory/device effects, and are unsuitable for immutable per-instruction records or block execution. This is rejected as the primary contract.

### C. Emit callbacks from individual instruction implementations

Callbacks close to each write could collect details, but they expose partial state, create re-entrancy and error-propagation hazards, couple ISA code to observer lifetimes, and make rollback/atomicity harder. This is rejected as an ownership model. An implementation may use an internal event mechanism, but only the completed Hart outcome may cross the observation boundary.

### D. Treat every execution error as a guest trap

This would make the current API easy to wrap, but a poisoned lock, unsupported legal instruction, host transport failure, or unknown transaction completion is not guest architecture. It would write misleading trap state and hide simulator defects. This is rejected; architectural classification must occur before `TrapEntered`.

### E. Use a structured Hart outcome carrying completed semantic facts (chosen)

A tagged semantic outcome makes retirement, trap entry, and simulator failure mutually distinguishable, lets the Hart produce records from authoritative effects, and preserves precise boundaries for block execution and Virtual Platform adapters. It requires an atomic/staged transition strategy and a richer observer contract, which are deliberate costs accepted by this ADR; their concrete designs remain implementation work.

## Consequences

### Benefits

- The ISS and Virtual Platform can share one Hart semantics and one source of retirement/trap facts.
- Differential testing and commit observation no longer depend on Runner snapshots or instruction re-fetch.
- Faulting instructions and interrupt entry have explicit non-retirement semantics.
- Physical faults, page faults, misalignment, platform exits, debugger stops, limits, scheduling, and simulator failures remain distinguishable.
- Block execution and later acceleration can preserve precise instruction-level observability.

### Costs and constraints

- Hart execution must prevent partial architectural state from escaping a failed step; the implementation may need staging, journaling, or an equivalent mechanism.
- The PhysicalAccess contract must distinguish guest-visible physical faults from host/transport failures and provide the atomic operation guarantees required by Hart semantics.
- Existing loggers and trap components require adapters or later migration, but this ADR does not change them.
- Concrete record representation, sink behavior, multi-Hart ordering, and run-control policy remain implementation or outer-layer concerns.

## Compatibility and migration impact

This ADR is documentation-only. It changes no simulator behavior, public Rust API, test expectation, or serialization format. Existing source and focused tests remain evidence of current component behavior, not evidence that this target contract is integrated end to end.

When implementation work is authorized, the public ELF behavior and one-Hart execution model should be preserved while observation and fault classification move to the Hart boundary. The concrete migration sequence, Rust types, serialization, sink adapters, and compatibility details are deferred to implementation design and the later verification described below.

## Relationship to [ADR-0002](0002-physical-access-transaction-and-fault.md)

The companion PhysicalAccess contract and this Hart contract have distinct responsibilities:

| Boundary | PhysicalAccess responsibility | Hart responsibility |
| --- | --- | --- |
| Physical result | Distinguish successful raw data/effect, a guest-visible physical or device fault, and a host/backend/transport failure or unknown completion. | Treat a guest-visible fault as an architectural access fault using the original access kind; treat the latter failures as `SimulatorFailure`. |
| Data and operations | Transfer raw little-endian bytes and carry enough operation information for indivisible AMO/LR/SC envelopes. | Interpret load/store/atomic semantics, apply register results, and decide retirement. |
| Translation-stage access | Serve PTE reads and A/D writes as physical operations with the same fault taxonomy. | Know that a failed PTE operation belongs to the original fetch/load/store-AMO access and apply §2; preserve a successful A/D write as a separate effect. |
| Atomicity and reservation | Prevent partial failed physical transactions and expose competing-access visibility through the port. | Own architectural reservation state and SC result; apply the selected ISA profile's reservation effects. |
| Timing | Report optional physical delay metadata without defining Hart outcomes. | Leave consumption and scheduling to the outer layers under the interrupt, time, and stop-event decision. |

A valid routed device error is therefore not an unresolved integration question: it is a guest-visible physical access fault. An adapter failure or unknown completion is a `SimulatorFailure`, never an invented trap. A generic `MemoryError` conversion in the current implementation is not this contract.

This is a consistency boundary. [ADR-0002](0002-physical-access-transaction-and-fault.md) records the companion port contract; [ADR-0003](0003-runner-machine-and-platform-ownership.md) and the interrupt, time, and stop-event decision consume the Hart boundary for outer ownership. The records retain their distinct scopes and do not duplicate one another.

## Later verification when implemented

The following are implementation evidence to obtain later:

- **Outcome and retirement:** verify one commit for each successful instruction, no commit for a faulting instruction or accepted interrupt, precise saved/next PC, privilege transitions, and retirement-counter behavior.
- **Fault matrix:** exercise post-translation and page-table-walk physical faults for fetch/load/store-AMO, invalid or non-canonical translation conditions, permission failures, and architectural misalignment. Verify access-fault versus page-fault causes and original-VA trap values, with physical/PTE context remaining diagnostic.
- **A/D visibility:** verify that a successful translation-stage A/D write remains visible when a later physical access faults, without assuming a particular A/D update scheme.
- **Failure classification:** inject internal, unsupported-legal-operation, transport, device, and unknown-completion failures. Verify that guest-visible device faults become access-fault traps while host/unknown failures remain simulator failures, with no fabricated record.
- **Observer ownership:** verify that records originate at the Hart transition, are delivered once as immutable facts, and do not require register snapshots or opcode re-fetch.
- **LR/SC profile behavior:** verify the selected ISA profile's reservation and conditional-access effects through a Hart-owned reservation state and a PhysicalAccess atomic envelope; do not use current implementation behavior as the specification.
- **Block equivalence:** when block execution exists, compare its per-instruction outcomes and final architectural state with single-step execution, including a block that traps part-way through.

## Open questions and explicit deferrals

1. The interrupt, time, and stop-event decision owns interrupt eligibility, priority/masking, architectural counter timing, delay consumption, and scheduler boundaries outside a Hart step.
2. [ADR-0003](0003-runner-machine-and-platform-ownership.md) owns how the Runner represents a completed Hart outcome together with platform exit, debugger stop, execution limit, observer failure, and simulator failure.
3. The selected ISA profile determines the A/D update scheme and LR/SC reservation effects, including profile-defined faulting-SC behavior; this ADR does not select a new scheme or semantic.
4. Concrete Rust outcome/record layouts, field types, ownership/lifetime mechanics, serialization, text-log compatibility, and detailed sink or trace formats are deferred to implementation design.
5. Reservation granule, multi-Hart ordering, DMA coherence, global observation ordering, and trace back-pressure are deferred to the relevant outer-layer contracts; they must not weaken precise per-Hart retirement.

The physical-fault cause matrix, architectural trap value, A/D-write visibility, and device-fault versus simulator-failure distinction are decisions above, not open questions. There is no superseding record. This ADR remains **Proposed**.

## Source and test map

- Hart boundary and current step/run behavior: [`src/core/mod.rs`](../../../src/core/mod.rs)
- Current commit logger and memory helper: [`src/core/commits.rs`](../../../src/core/commits.rs)
- Trap causes, context, delegation, and handler: [`src/core/trap.rs`](../../../src/core/trap.rs)
- Public Runner, `ExecutionResult`, `SystemBus`, HTIF, snapshots, and re-fetch: [`src/executor.rs`](../../../src/executor.rs)
- Current execution and memory error categories: [`src/execute/mod.rs`](../../../src/execute/mod.rs), [`src/memory/mod.rs`](../../../src/memory/mod.rs)
- MMU translation and A/D component behavior: [`src/mmu/sv39.rs`](../../../src/mmu/sv39.rs), [`tests/ad_bits_test.rs`](../../../tests/ad_bits_test.rs)
- Commit logger tests: [`tests/commits_test.rs`](../../../tests/commits_test.rs)
- Trap component tests: [`tests/trap_test.rs`](../../../tests/trap_test.rs)
- Runner and SystemBus tests: [`tests/executor.rs`](../../../tests/executor.rs)
