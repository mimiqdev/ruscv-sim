# Post-A7 Roadmap Proposal

**Status:** Draft

**Authority:** Informational proposal awaiting maintainer/user approval; it is not a
milestone contract and does not replace [`docs/dev-plan.md`](../dev-plan.md).

**Planning baseline:** repository commit `c059500a6af20f93569099c4b05ced3364a7703b`,
which contains the merged A7 closeout delivery. Runtime evidence cited below is
bound to the recorded A7 implementation head
`fb6f51c771f32585f1547422c9633379f7ae370b`; the closeout and later documentation
commits must not be mistaken for a new runtime or ACT4 execution.

**Scope:** post-A7 priority and dependency proposal for the existing ISS toward a
composable Virtual Platform (VP). No implementation, successor activation, ADR
change, release, or product commitment is made by this document.

## 1. Decision requested

The repository has completed the bounded A7 statement **“non-atomic
physical-access migration completed.”** The next decision should be a priority
choice, not a mechanical continuation of an old backlog. This proposal recommends:

1. make **single-Hart physical/atomic convergence** the next implementation
   candidate;
2. build the **Hart fact/observation and safe N=1 Machine lifecycle** boundary
   immediately after, with limited design/test preparation able to proceed in
   parallel;
3. only then integrate Hart virtual-memory/privilege protection and
   Platform-input interrupt/time behavior as separately bounded candidates that
   may run in parallel once their shared prerequisites are met;
4. wire public debug through the same lifecycle and fact boundary; and
5. defer native-to-TLM/SystemC, DMI, and especially multi-Hart/DMA/coherence until
   the preceding single-Hart contracts have evidence.

This is a recommended order, not an approved schedule. It deliberately does not
name the next work A8/A9 or assume that composition must precede interrupts under
those labels. A future approved successor must select one bounded objective and
rewrite `docs/dev-plan.md` through the repository rolling process. Until then,
A7 remains the sole Current technical contract.

### Product goal and explicit non-goals

The product goal is an **executable, verifiable single-Hart ISS that can become
one configuration of a composable VP**: the same Hart semantics should operate
behind a native platform today and, conditionally, behind richer platform,
time, debug, and external-kernel adapters later. N=1 is the present ISS baseline;
it is not a promise of multi-Hart support.

This roadmap does **not** promise Linux boot, an OS, full RV64I/M/A/F/D/C or
whole-ISA certification, successful misaligned execution, a particular privilege
profile, multi-Hart, DMA, cache coherence, SystemC support, or a performance
number. Any of those would require an explicit product decision, compatible
architecture work, and a separately approved contract. The Linux research note
is context only, not a target ([`docs/research/linux-boot-requirements.md`](../research/linux-boot-requirements.md)).

### Authority boundary

The accepted target architecture, ownership principles, and semantic contracts
remain authoritative:

- [`architecture/README.md`](../architecture/README.md) defines the ISS → VP
  direction and the target Frontend → Runner → Machine → Hart/Platform shape.
- [`architecture/principles.md`](../architecture/principles.md) keeps architectural
  semantics in the Hart, physical routing in the Platform, composition/lifecycle
  in the Machine, and run policy in the Runner.
- ADR-0001 through ADR-0004 define Hart outcomes/observations, physical
  transactions, ownership/lifecycle, and temporal/event boundaries. They are not
  changed by this proposal and do not themselves schedule implementation.
- [`dev-plan.md`](../dev-plan.md) remains the only Current milestone contract.
- Archived plans and closeout bodies are historical evidence. They are not a
  backlog to reactivate.

## 2. Current state: implementation, public integration, and evidence

The following inventory separates three claims that must not be conflated:
**component implementation**, **public-path integration**, and **verification
strength**. It is based on source and named assertions, not on type presence.

| Area | Component implementation | Public integration today | Verification strength and limit |
| --- | --- | --- | --- |
| Hart outcomes and synchronous traps | `RiscvCore::step_outcome` in [`src/core/mod.rs`](../../src/core/mod.rs) returns `InstructionRetired`, `TrapEntered`, or `SimulatorFailure`; staged state and `minstret` handling are present. | Both standard runners consume the typed outcome and continue to a guest trap handler; the outer loops still live in [`src/executor.rs`](../../src/executor.rs), not in a standalone Machine. No asynchronous input sampling exists. | [`tests/a6_task3_core_trap_test.rs`](../../tests/a6_task3_core_trap_test.rs) asserts ECALL entry, `mepc`/`mtval`, no retirement on traps, recursive trap accounting, zero budget, and final-slot host failure. This is bounded synchronous evidence, not a complete Hart profile. |
| Ordinary physical accesses | [`src/physical.rs`](../../src/physical.rs) has explicit Fetch/DataRead/DataWrite, widths 1/2/4/8, raw bytes, complete-span validation, target/host/protocol/unknown categories, and native RAM/UART/HTIF adapters. | `load_and_run` and `RiscVSimulator` install separate raw fetch/data views over shared RAM/device objects. Hart alignment, address conversion, extension, FP interpretation, trap mapping, and retirement remain in `src/core/mod.rs`. | A7 T1/T2/T3 tests cover exact bytes, widths, side-effect fences, routing, and failure taxonomy. [`tests/a7_hart_physical.rs`](../../tests/a7_hart_physical.rs) asserts fetch/load/store causes 1/5/7 and original `mtval`; [`tests/a7_native_targets.rs`](../../tests/a7_native_targets.rs) asserts no partial native writes. This is non-atomic convergence only. |
| AMO/LR/SC | The active executor and [`src/isa/rv64a/`](../../src/isa/rv64a/) implement legacy typed helpers. The implementation has known AMO width/dispatch, `aq`/`rl`, reservation, and fault-path debt. | A7 deliberately routes AMO/LR/SC through the typed compatibility bridge while ordinary accesses use raw ports. Both views use the same storage/device domain, but there is no ADR-0002 atomic envelope. | [`tests/a7_migration_characterization.rs`](../../tests/a7_migration_characterization.rs) records successful and defective behavior; [`tests/a7_legacy_atomic_compat.rs`](../../tests/a7_legacy_atomic_compat.rs) asserts ordinary↔legacy visibility, retained reservation behavior, MMIO width debt, and lock non-reentry. It is preservation evidence, not A-extension or atomicity certification. |
| Runner, placement, and platform | `SystemBus`, `RunControl`, ELF placement helpers, UART, HTIF, and result/artifact assembly are implemented in [`src/executor.rs`](../../src/executor.rs). | The CLI has one `run` path; `load_and_run` and `RiscVSimulator` share selected placement/run-control decisions but remain two configuration-specific loops. There is no explicit Machine/Platform lifecycle root. | [`tests/public_behavior.rs`](../../tests/public_behavior.rs), [`tests/a4_run_control.rs`](../../tests/a4_run_control.rs), and [`tests/a7_public_equivalence.rs`](../../tests/a7_public_equivalence.rs) assert entry/base, tohost precedence, zero/exact/final-slot budgets, artifacts, reload, and intentional native/flat differences. This does not prove the ADR-0003 lifecycle. |
| Observation and commit log | ADR-0001-shaped typed Hart facts exist; [`src/core/commits.rs`](../../src/core/commits.rs) can format register and optional memory changes. | The native runner logs a retired fact without refetching, but captures Runner-side GPR snapshots and passes `mem_access = None`; trap/failure records are not a public subscriber-gated observation plane. | `tests/a7_public_equivalence.rs::public_fetch_trace_and_trap_observation_do_not_refetch_or_commit_a_trap` and commit-log assertions cover the current no-refetch/no-trap-commit boundary. Full architectural effects, memory effects, sink delivery, and block-equivalent observation remain debt. |
| MMU, privilege, PMP, page walks, A/D | [`src/mmu/`](../../src/mmu/) provides Sv39 translation, TLBs, page permissions, and A/D writeback; CSR/privilege components are tested. `MmuConfig::pmp_entries` and `MmuError::PmpViolation` exist, but current source does not enforce PMP in the public core. | The public core uses image-base/storage adaptation instead of MMU translation. Page-table accesses use the MMU-specific `PhysicalMemoryInterface`, not the A7 `PhysicalAccess` route. | [`tests/translation_test.rs`](../../tests/translation_test.rs) asserts Sv39 miss/hit, permissions, user access, and TLB flushes; [`tests/ad_bits_test.rs`](../../tests/ad_bits_test.rs) asserts A/D writeback and TLB-hit behavior. These are component tests, not public ELF MMU integration or page-fault evidence. |
| Interrupts, WFI, time, counters | [`src/peripherals/clint.rs`](../../src/peripherals/clint.rs) and [`src/peripherals/plic.rs`](../../src/peripherals/plic.rs) model timer/software/external interrupt state, claim/complete, and callbacks. CSR state has interrupt-related fields. | CLINT/PLIC are TLM-side components; the public `SystemBus` has no interrupt-line input, the Hart does not sample asynchronous lines, WFI is recognized but unsupported, and no Machine scheduler advances virtual time. Public `cycles` is completed Hart-turn accounting, not `mcycle` or `mtime`. | Peripheral/TLM tests assert local register and callback behavior, but no public interrupt guest or WFI scheduling path exists. ADR-0004's event, time, wait-state, and non-lossy fact rules are accepted targets, not implementation evidence. |
| Debug and breakpoints/watchpoints | [`src/debug/`](../../src/debug/) has RSP parsing, `DebugTarget`, breakpoint/watchpoint managers, and a server. | Tests use mock `DebugTarget` implementations. No production DebugTarget is connected to `RiscvCore`, a Runner/Machine, or the public CLI; Debug Mode/trigger state is not implemented. | Unit tests assert protocol, manager limits/hits, and mock stepping. They do not prove public register/memory mutation, stop arbitration, quiescence, or architectural breakpoint/debug distinction. |
| TLM, SystemC, DMI | [`src/tlm/`](../../src/tlm/) has Rust TLM-style payloads, statuses, delays, routing, and a DMI cache. The optional `RiscvCore::tlm_interface` can be set. | `step_outcome` does not read `tlm_interface`; no SystemC/C++ FFI or public TLM physical adapter is connected. The TLM payload has no proof of the ADR-0002 atomic envelope or DMI invalidation semantics. | [`tests/tlm_tests.rs`](../../tests/tlm_tests.rs) and component tests cover payloads, routing, delay accumulation, and cache clearing. They do not establish native parity, external-kernel hosting, or end-to-end Hart integration. |
| ISA/external validation | RV64I/M/A/F/D paths are dispatched and RV64C code exists separately. | The active public fetch path is fixed 32-bit; compressed execution is not selected. Component presence is not extension support. | A7 retained two independent 51-case sets: 51 fresh project-authored ELF guests (including five A6 trap guests), and 51 frozen nontrapping ACT4 RV64I cases. Neither certifies all dispatched extensions, atomics, privilege, MMU, interrupts, or OS behavior. |

### A7 exact evidence boundary

The recorded A7 closeout reports, at the implementation evidence head,
354 focused migration/regression tests and 1,667 full Rust/doc results with no
failures, fresh project-authored guests 51/51, and a separately scoped pinned
ACT4 51-case run. The closeout deliberately records the implementation head,
the later documentation/merge identities, artifact hashes, and unavailable
replay limitations separately ([closeout](../archive/milestones/a7-closeout-record.md),
[capability assessment](../verification/a7-capability-assessment.md)). This
proposal does not rerun or relabel that evidence.

The public equivalence assertions are useful but bounded: they compare shared
architectural/result facts across CLI, `load_and_run`, and the flat facade while
retaining UART, fixed HTIF, flat offsets, artifact policy, and diagnostics as
configuration differences. A7's unknown-completion evidence is a narrow public
core seam; there is no safe high-level native injector. Those limitations are
inputs to the next candidate, not reasons to claim a broader failure contract.

## 3. Priority decision and candidate comparison

### Candidate 1 — single-Hart physical/atomic convergence **(recommended)**

**Objective:** close the largest semantic hole left by A7: implement an explicit
atomic operation envelope on the same physical domain and replace the global,
typed AMO/LR/SC bridge for the selected single-Hart profile.

**Why it is first:** ADR-0002 makes atomicity, reservation ownership, and writer
visibility part of the physical boundary. A Machine, TLM adapter, DMI path, or
multi-agent platform built on a read-then-write atomic approximation would have
to be redesigned when this debt is corrected. A7 already supplies the ordinary
raw seam, shared storage, lock-order characterization, and negative-result
vocabulary needed to bound this next step.

**Boundary:** one selected RV64A profile, one Hart, native RAM/MMIO first, one
shared physical domain, explicit compatibility decision for host writes. It may
retain the old public constructor as an adapter while it is proven, but it may
not retain a hidden second atomic authority.

**Non-goals:** multi-Hart/DMA ordering, cache coherence, TLM/SystemC, full ISA
certification, Linux/OS, and an unbounded cleanup of unrelated ISA defects.

**Main risk/cost:** high semantic risk and medium-to-high compatibility cost.
The hard choices are exact AMO/LR/SC widths and encodings, `aq`/`rl` profile
semantics, reservation granule/key, faulting-SC behavior, and whether public
host writes are quiescent control mutation or participating physical writers.

**Exit shape:** an atomic operation is one target-visible envelope; per-Hart
reservations and all in-scope committed writers are observable; old bridge calls
are gone from the selected public route or explicitly limited to a tested
compatibility adapter. Existing successful paths are preserved only where the
approved profile says they remain valid, not by retaining known-deficient
semantics silently.

### Candidate 2 — observation plus N=1 Machine/lifecycle first

**Objective:** introduce Hart-owned commit/trap facts, an optional observation
plane, an explicit N=1 Machine/Platform composition, and safe construct/install/
reset/quiesce/drain behavior while retaining A7's explicit legacy atomic bridge.

**Advantages:** addresses ADR-0001/0003 debt, enables public debug and later
interrupt/time work, and provides a clean place for unknown-completion recovery.
It is a credible alternative if the immediate product value is lifecycle and
observability rather than atomic correctness.

**Risk/cost:** high structural churn while the physical contract is still
non-conforming for atomics. If the new Machine owns an incomplete bridge, later
atomic envelopes and writer notifications can change Machine inspection,
observation, lock, and drain APIs. It should be selected only with an explicit
statement that atomic convergence is a near follow-up, not assumed complete.

**Exit boundary if selected:** N=1 only, native backend only, no interrupts,
MMU, TLM, debug server, or multi-Hart; unknown completion blocks unsafe reuse
until adapter resolution/drain evidence exists.

### Candidate 3 — interrupt/WFI/time first

**Objective:** connect CLINT/PLIC-like sources, Hart/profile input sampling,
WFI waiting, time/counter accounting, and stop facts.

**Advantages:** visible platform behavior and a path toward a VP scheduler.

**Why not first:** there is no public Machine exchange, Hart input boundary,
quiesce/drain contract, or integrated observation plane on which to return
interrupt and time facts. Implementing it directly in the current executor would
further fuse Runner, Platform, and Hart policy and would repeat the rejected
opportunistic-polling shape. It must not be pre-labelled as a later fixed
milestone; it is conditional on Candidate 2's boundaries and a selected Hart
profile.

### Candidate 4 — MMU/privilege/PMP first

**Objective:** connect the existing Sv39/TLB/A-D components and privilege rules
to the Hart and common physical port, with PMP and page-walk fault mapping.

**Advantages:** the repository has substantial component tests and this work
closes address ownership before more system behavior is added.

**Risk/cost:** high Hart/physical integration cost. Page-table reads and A/D
writes have to use the same raw physical result taxonomy, preserve original
access-kind fault mapping, and avoid treating ELF base subtraction as
translation. Without Candidate 1/2's physical and outcome boundaries, the
integration would likely create another special path.

### Recommendation

Select Candidate 1 as the next successor **only after the user approves its
profile and compatibility choices**. Allow design/test preparation for Candidate
2 in parallel, but do not activate either from this proposal. If product value
requires lifecycle first, Candidate 2 is the viable alternative; it must carry
an explicit atomic debt gate and must not be described as full ADR-0002
convergence. Candidates 3 and 4 are later, independently bounded choices, with
Candidate 4 and Candidate 3 potentially parallel after the shared Machine and
Hart-fact foundations.

## 4. Recommended dependency graph

The graph is a dependency recommendation, not a schedule or a second current
contract.

```text
Recorded A7 ordinary raw path + A7 negative/equivalence evidence
        │
        ├───────────────┐
        ▼               ▼
Performance/evidence  Atomic profile decision
baseline gate         (widths, aq/rl, reservation, host-writer policy)
        │               │
        │               ▼
        │       Atomic operation envelope
        │               │
        │               ▼
        │       Per-Hart reservation + writer visibility
        │               │
        │               ▼
        │       Legacy atomic bridge exit
        │               │
        └───────┬───────┘
                ▼
       Hart fact/observation plane
                │
                ▼
       N=1 Machine/Platform composition
       + install/reset/quiesce/drain/
         unknown-completion recovery
          │              │
          │              ├───────────────┐
          ▼              ▼               ▼
  MMU/privilege/PMP   Interrupt/time/   Public debug
  + page walks/A-D    WFI/counters      adapter
          │              │               │
          └──────┬───────┴───────┬───────┘
                 ▼               │
       Native ↔ Rust TLM parity  │
       ↔ blocking SystemC/FFI    │
                 │               │
                 ▼               ▼
              DMI with invalidation and
              lifecycle/observation hooks
                         │
                         ▼
          Conditional multi-Hart/DMA/inbound-master
          ordering, coherence, reservation and replay contract
```

### Hard prerequisites and parallel opportunities

- **Hard before atomic bridge exit:** a selected atomic/profile semantics;
  operation-level backend capability; per-Hart reservation state; a defined
  writer participation policy; exact failure/unknown behavior; and evidence
  that native and compatibility views share one domain without lock re-entry.
- **Hard before Machine/lifecycle implementation:** typed Hart outcomes;
  physical result/unknown semantics; image installation ownership; observation
  delivery boundary; and a drain proof that can distinguish `DrainComplete`
  from `NoProgress`.
- **Hard before MMU integration:** one physical route for instruction/data/PTE/A-D
  effects, selected privilege/profile rules, original-VA fault mapping, and TLB
  invalidation rules. PMP is not satisfied by the existing configuration field.
- **Hard before interrupt/time integration:** Machine exchange, Platform input
  admission, Hart/profile sampling point, `mcycle`/`minstret` ownership, and a
  WFI waiting state that the Machine does not invent.
- **Hard before public debug:** a quiescent Machine control path and stable Hart
  inspection/stop facts. Breakpoint/watchpoint managers can be unit-tested in
  parallel, but public mutation cannot bypass Platform routing or in-flight work.
- **Hard before TLM/SystemC/DMI:** stable raw/atomic transaction and fault
  semantics, delay ownership, lifecycle drain, and observation/invalidation
  boundaries. Blocking transport is the first viable adapter; non-blocking
  transport is not a prerequisite.
- **Parallel work:** performance baseline design and fixture preparation can
  begin before any implementation candidate; component-level MMU, peripheral,
  TLM, and debug tests can be maintained independently; Candidate 3 and 4
  integration work may be parallel after Candidate 2's shared prerequisites.
- **Not a prerequisite claim:** Linux, an external OS, multi-Hart, and DMA are
  not hidden reasons to pull later stages forward.

## 5. Candidate delivery stages

These stages describe bounded future candidates. They are not approved milestone
contracts; each needs its own scope, acceptance, and exact-head evidence.

### Stage 0 — evidence and performance gate (cross-cutting, not a successor)

**Target:** make later comparisons reproducible without claiming a performance
threshold.

**Boundary:** measurement harnesses and evidence retention only; no runtime
optimization in this roadmap task.

**Non-goals:** changing `Runtime`, adding a fast path, or treating the A7 ratio
as a regression gate.

**Deliverables:**

- one fixed guest fixture and toolchain/container identity for public
  `load_and_run` measurements;
- separated load/ELF-install, preloaded execute, and full public-run timings;
- typed legacy, A7 raw ordinary, and (when available) atomic route cases;
- observation disabled versus enabled/logged comparisons with the same result
  oracle;
- retained samples, distributions, revision identities, environment, fixture
  hashes, and commit/retirement counts;
- a policy for long-term weekly trend storage and threshold calibration.

**Exit evidence:** a clean baseline can be reproduced at an exact commit, the
same guest result/retirement tuple is checked before timing is compared, and no
threshold is declared until variance and runner stability are measured. This
gate is required before evidence-driven optimization, not before ordinary
correctness work.

**Cost/risk:** medium measurement effort; high risk of misleading conclusions
if load cost, execution cost, observer cost, and host noise are not separated.

### Stage 1 — single-Hart physical/atomic convergence (recommended next candidate)

**Target:** converge the selected atomic operations with the accepted physical
contract while preserving the existing ISS product where compatibility is
approved.

**Boundary:** one Hart and one native physical domain; explicit Fetch/Data and
Atomic request envelopes; Hart-owned arithmetic, result, reservation, and trap
mapping; Platform-owned atomic target effect and visibility.

**Non-goals:** multi-Hart/DMA/coherence, TLM/SystemC, DMI, MMU, async
interrupts/WFI, full A-extension certification, or broad ISA repair.

**Deliverables:**

- an approved RV64A profile table for AMO/LR/SC widths, encodings, signedness,
  `aq`/`rl`, reservation granule/key, and faulting-SC behavior;
- a complete physical atomic envelope that cannot expose AMO/SC as a visible
  read-then-write pair;
- per-Hart reservation state owned by the Hart, with reset/reload behavior and
  selected invalidation semantics;
- visibility/invalidation from every writer included in the chosen domain;
- exact legacy bridge audit and removal/adapter boundary, with no copied RAM,
  duplicate reservation, or lock re-entry;
- mixed ordinary/atomic public ELF and native negative/failure tests;
- compatibility decisions recorded for old typed constructors, host writes,
  legacy MMIO behavior, and any changed success/failure result.

**Verifiable exit standard:**

1. W/D encodings and dispatch are tested independently, including old-defect
   fixtures changed only when the approved profile requires the change.
2. Two Harts (if test-only Hart objects are used) cannot share a reservation;
   reset/reload and same-address/different-address cases follow the selected
   profile.
3. A successful AMO/SC produces one target-visible atomic event; no competing
   writer can observe an intermediate value.
4. Every in-scope committed writer has a tested reservation effect. A writer
   outside the domain is either quiescence-protected and explicitly documented
   or is rejected from the selected capability; it is not silently ignored.
5. Rejected operations prove no partial physical side effects; host/protocol
   failures and unknown completions never fabricate a retirement or trap. An
   unknown-after-effect case proves no retry and safe uncertain-state recovery;
   a faulting SC follows the approved profile.
6. Public ordinary/atomic mixed fixtures preserve approved CLI/library behavior,
   and the old bridge is not claimed gone until these tests pass.
7. Rust/component, focused public ELF, negative-side-effect, full gate, and
   exact-head evidence are retained. ACT4 remains separately scoped and does
   not certify A-extension behavior.

**Cost/risk:** high. The compatibility matrix is the principal product risk.

### Stage 2 — Hart facts and safe N=1 Machine lifecycle

**Target:** make the accepted Hart/Runner/Platform ownership executable for one
native Platform without adding a second ISA engine.

**Boundary:** one Hart, one native Platform, construct → install → reset → run /
control → inspect → quiesce/drain → teardown; always-present control facts and
subscriber-gated architectural observations.

**Non-goals:** asynchronous interrupt delivery, MMU/PMP, SystemC, multi-Hart,
checkpoint format, Debug Mode, and a new CLI result taxonomy without approval.

**Deliverables:**

- Hart-owned commit/trap facts containing the required PC, privilege, register/
  CSR, memory/atomic, and trap effects when subscribed;
- no Runner opcode refetch or pre/post snapshot reconstruction as architectural
  truth; disabled observation has the same outcomes without per-instruction
  delivery;
- an explicit N=1 Machine/Platform composition around the current native
  backend, with Runner-owned image loading and terminal classification;
- lifecycle states and safe image/reset/debug mutation boundaries;
- quiesce/drain evidence for normal work, observer delivery, and unresolved
  physical completion; unknown completion blocks unsafe reset/reuse until
  adapter resolution or a proven fresh restoration;
- compatibility adapters for `load_and_run`, `RiscVSimulator`, artifacts, HTIF,
  and current run-control facts.

**Verifiable exit standard:**

- commit and trap records match the same final state with observation on/off;
  faulting instructions have no commit, and a successful exit-causing store is
  retired before `PlatformExit` is reported;
- an observer failure does not unretire a completed transition;
- zero/exact budget, final-slot failure, guest exit, synchronous trap, and
  unknown completion return all applicable control facts;
- image install/reset/debug mutation during in-flight work is rejected, while a
  successful `DrainComplete` proves no Hart, physical, callback, or observation
  work remains in flight; `NoProgress` is not used as a substitute;
- public CLI and flat/library compatibility tests retain their documented
  differences; no new public API or log schema is accepted without a decision.

**Cost/risk:** high structural change, but it reduces later integration churn.

### Stage 3 — selected Hart virtual-memory and protection profile

**Target:** connect a deliberately selected privilege/address-translation
profile to the common physical port.

**Boundary:** start with the supported profile and Sv39 path needed by the
chosen ISS use case; instruction/data translation, page-table reads, A/D writes,
TLB invalidation, privilege checks, and PMP decisions remain Hart/profile work.

**Non-goals:** Sv48/Sv57 by implication, Linux/OS boot, full privilege
certification, successful misaligned accesses, or an unselected PMP/profile
model.

**Deliverables:**

- page-table reads and A/D writeback through the same physical result taxonomy;
- original virtual access kind and address retained when mapping physical/page
  faults; successful A/D effects remain separate and visible;
- Hart-owned translation/TLB integration and explicit PMP decision;
- small public ELF guests for bare, mapped, permission-fault, page-walk-fault,
  A/D, TLB flush, and protection cases;
- exact compatibility decision for image-base adaptation versus identity/bare
  configuration.

**Verifiable exit standard:** component tests remain green; public guests prove
fetch/load/store translation and negative fault mapping without calling the MMU
component directly; PTE/A/D transactions are traceable to the common physical
port; no page-table access bypasses the target fault/unknown distinction; fresh
ELF and exact-head evidence is recorded.

**Cost/risk:** high; page-walk and architectural fault boundaries are easy to
misclassify. This stage may proceed in parallel with Stage 4 only after Stage 2,
with separate acceptance evidence.

### Stage 4 — Platform interrupts, WFI, modeled time, and counters

**Target:** implement the selected minimal ISS event/time profile at the accepted
Machine boundary.

**Boundary:** selected Hart/profile interrupt eligibility and trap transitions;
Platform-owned sources/controllers and `mtime`/`mtimecmp`; Machine-owned input
admission, grants, framework time, delay consumption, waiting/idle exchange, and
non-lossy facts.

**Non-goals:** a universal interrupt priority table, Linux/SBI, multi-Hart
fairness, SystemC kernel hosting, hardware-accurate timing, or silently defining
WFI behavior in the Machine.

**Deliverables:**

- normalized Platform input admission and pre-fetch Hart/profile sampling;
- CLINT/PLIC/native device wiring for the selected map, with source assertion,
  pending, acceptance, and claim/complete kept distinct;
- Hart/profile-owned WFI legality/wake state and Machine legal idle jumps;
- separate accounting for Hart attempts, retired instructions, accepted trap
  entries, `mcycle`/`minstret`, framework ticks, physical delay, `mtime`, and
  host elapsed time;
- zero/finite budget, absolute deadline, delay, no-progress, and co-incident
  fact tests from ADR-0004.

**Verifiable exit standard:** synthetic public guests and Rust tests prove timer /
software / external inputs, pre-fetch interrupt acceptance, no mid-instruction
preemption, WFI with event/no event/deadline/single-step, exact counter rules,
causal exit ordering, and stable fact ordering. A component CLINT/PLIC pass
alone is insufficient. The selected profile and any compatibility change to
`ExecutionResult.cycles` require approval before activation.

**Cost/risk:** high semantic and test cost; time units and profile behavior are
product decisions, not implementation defaults.

### Stage 5 — public debug wiring

**Target:** connect the existing debug protocol to the Machine/Runner boundary.

**Boundary:** one-Hart GDB/RSP and library control through quiescent inspection,
with external stop, guest breakpoint, watchpoint, and single-step kept distinct.

**Non-goals:** RISC-V Debug Mode/trigger modules, multi-Hart thread control,
software breakpoint patching during an in-flight quantum, or a new debugger CLI
contract without approval.

**Deliverables:**

- a production `DebugTarget` adapter over Machine control and inspection;
- register/memory access through Platform/physical inspection rather than direct
  concrete-memory mutation;
- breakpoint/watchpoint hit facts, stop requests, continue/single-step, and
  lifecycle mutation checks;
- public integration tests using an ELF fixture and a real RSP exchange or
  equivalent protocol harness.

**Verifiable exit standard:** public debug operations cannot race an in-flight
transaction; memory/register changes require the declared quiescent boundary;
stop facts remain distinct from guest traps and Platform exit; enabled debug
observation preserves the same Hart state and result oracle. This stage can be
prepared in parallel with Stages 3/4 after Stage 2.

**Cost/risk:** medium-to-high; compatibility risk is concentrated in control
and inspection API shape.

### Stage 6 — native ↔ Rust TLM ↔ SystemC and DMI (conditional integration)

**Target:** replaceable platform transport without a second Hart execution path.

**Boundary:** first a blocking Rust TLM adapter, then a narrow C/C++/SystemC
lifecycle/transaction boundary; raw/atomic statuses, bytes, delays, events, and
lifecycle facts remain the ruscv-sim semantic vocabulary.

**Non-goals:** non-blocking transport as an initial requirement, arbitrary
SystemC API exposure in ISA code, transport-specific atomic weakening, or DMI
for devices without side-effect/invalidation proof.

**Deliverables:**

- native/TLM parity tests for widths, bytes, target faults, host/protocol/
  unknown completion, atomic envelopes, and delay metadata;
- blocking `b_transport` mapping and external-kernel grant/return behavior;
- explicit FFI ownership/thread/lifetime/error policy;
- DMI only for a qualified RAM region, with permissions, latency, invalidation,
  breakpoint/self-modifying/MMU/atomic interactions, and lifecycle drain;
- SystemC integration smoke evidence at an exact pinned external revision.

**Verifiable exit standard:** the same public fixture under native and adapted
transport has equivalent architectural, fault, event, and time facts; DMI and
non-DMI runs match after invalidation; no `sc_stop`, TLM status, or callback
implicitly becomes a ruscv-sim terminal result; a failure/unknown completion
cannot leave a late callback mutating a reset image.

**Cost/risk:** high external-tool and lifetime risk. Blocking transport is the
safe first cut; non-blocking transport requires a separately approved timing
contract.

### Stage 7 — conditional multi-Hart/DMA/coherence direction

**Target:** only if a concrete VP use case requires multiple Harts or inbound
masters, define the shared physical-world contract first.

**Boundary:** an explicit Platform-owned inbound-master port, Hart IDs, same-time
ordering, reservation invalidation, DMI invalidation, arbitration, and
coherence/visibility model.

**Non-goals:** assuming “same host buffer” is coherence, silently reusing the
single-Hart global reservation, or adding multi-Hart merely because the Machine
cardinality in ADR-0003 permits it.

**Entry conditions:** Stage 1 per-Hart reservations and writer policy, Stage 2
lifecycle/facts, Stage 4 event/time ordering, and Stage 6 adapter/invalidation
parity are verified; a user-approved product use case and separate contract
exist.

**Exit standard:** repeated runs under different host/container ordering produce
the declared canonical facts and shared effects, or unsupported conflicts are
rejected; DMA does not pass through Hart translation or enter guest traps; fresh
multi-agent evidence is separate from all single-Hart claims.

**Cost/risk:** very high and entirely conditional. It is a far direction, not
an automatic successor.

## 6. Atomic dependency and writer policy in detail

A7's raw route must not be mistaken for a nearly complete atomic route. The
following sequence is the minimum reasoning required before any bridge-exit
milestone:

1. **Encoding and width:** current `execute_amo` dispatch and LR/SC helpers have
   characterized W/D selection debt. Decide the profile and test the exact
   instruction bits before changing behavior. A successful old fixture is a
   preservation target only until the profile says otherwise.
2. **Envelope:** an AMO is one operation with an old-value/result contract; LR
   establishes reservation only after a successful physical read; SC is one
   conditional operation. Two ordinary port calls cannot be renamed an atomic
   envelope.
3. **Hart reservation:** move authority from the process-global
   `GLOBAL_RESERVATION` to per-Hart architectural state. Define granule/address
   representation, reset/reload, successful and failed SC, and faulting-SC
   behavior from the selected profile.
4. **Writer visibility:** list every writer in the supported domain before
   claiming invalidation: ordinary integer/FP Hart stores, AMO/SC, image or
   platform writes admitted during execution, and any public host write that is
   allowed while a Hart is runnable. A7's tests intentionally prove that scalar,
   FP, and host writes currently do **not** invalidate the legacy reservation.
   That observation cannot become the next contract by inertia.
5. **Host-write choice:** the recommended conservative choice is to make
   `RiscVSimulator::write_mem` and equivalent direct host mutation quiescence-only
   inspection/control operations. If the product instead permits them during a
   run, they must enter the writer/invalidation domain and get negative and
   unknown-completion evidence. The choice changes observable SC behavior and
   therefore needs explicit approval.
6. **Bridge exit:** only after operation-level traces, side-effect tests, and
   public mixed fixtures pass may the typed bridge be removed from the selected
   standard route. Keeping an old constructor as a compatibility adapter is
   acceptable only if its atomic capability and limits are explicit; it cannot
   silently claim the common contract.

This ordering is why a generic Machine-first or interrupt-first route is not the
recommended immediate priority. It avoids repeating a roadmap that schedules
composition while leaving the most important physical contract incomplete.

## 7. Performance position and evidence plan

### What the repository actually measures today

Cargo declares seven Criterion benchmark groups:

| Group | Actual code exercised | What it does not measure |
| --- | --- | --- |
| `decode_bench` | Direct `InstructionDecoder` format, batch, and throughput calls. | No fetch port, Hart step, public ELF, trap, or bus. |
| `execute_bench` | Direct `Executor::execute` arithmetic/immediate/mixed loops over a `SimpleMemory` lock; a synthetic CPI loop. | No `step_outcome`, raw physical adapter, native bus, MMIO, Runner, or lifecycle. |
| `memory_bench` | Typed `SimpleMemory` byte/half/word operations, sequential/random loops, cache-line-shaped access, and throughput. | No raw bytes through `PhysicalAccess`, shared `SystemBus`, UART/HTIF, lock topology, or ELF execution. |
| `branch_predict_bench` | A self-contained predictor model defined in the benchmark file. | No active branch predictor is wired into `RiscvCore`. |
| `pipeline_bench` | A self-contained five-stage pipeline simulator in the benchmark file. | No active pipeline execution path; the public core is an interpreter. |
| `cache_bench` | A self-contained cache simulator and synthetic access patterns. | No active cache or DMI path in the public execution route. |
| `rv64d_bench` | Direct RV64D helper calls such as `exec_fadd_d`/`exec_fdiv_d` with constructed state. | No public FP load/store route, full Hart step, ELF result, or external compatibility claim. |

The scheduled workflow is [`.github/workflows/bench-scheduled.yml`](../../.github/workflows/bench-scheduled.yml):
it runs all seven groups on `ubuntu-latest` with the stable toolchain on a
weekly cron (`0 2 * * 0`), despite the workflow's “Daily” label, and retains an
artifact for seven days. It does not pin a benchmark host, compare against an
exact prior revision, establish a checked threshold, or separate build/load/run
cost. The benchmark README's old manual `benchmark.yml` name is not the actual
workflow. These are useful component observations, not a public-path regression
monitor.

### A7 observations are deliberately narrow

- [`a7-performance-observation.json`](../verification/a7-performance-observation.json)
  records current-head component observations such as decode ADD 4.2264 ns,
  typed word read 12.446 ns, typed word write 63.335 ns, and 4 KiB read
  throughput 335.29 MiB/s. It explicitly has no before/after baseline and does
  not isolate the raw adapter or lock path.
- [`a7-physical-loop-observation.json`](../verification/a7-physical-loop-observation.json)
  compares the same 4,096-iteration guest through `load_and_run` at
  `9ee9c5f...` and `fb6f51c...`, with 45 samples per revision in one pinned
  ARM64/Rust 1.97.1 environment. The raw-route median divided by the old typed
  median was **1.2858**; both revisions produced the same recorded exit code,
  20,490 completed turns/commit records, final PC, and no error. This is a
  one-workload, one-environment load/run observation, not a universal regression
  claim, a release threshold, or an optimization mandate.

### Required baseline progression

Before a future optimization or any stage that changes the execution hot path,
use a repeatable harness with:

1. **Load-only:** ELF parse, allocation, placement, metadata resolution, and
   installation measured separately.
2. **Execute-only:** one preloaded core/image and a fixed number of Hart turns,
   excluding ELF load and teardown.
3. **Public end-to-end:** the real CLI/library facade, with exit, cycle/turn,
   final-PC, signature, and commit-count or Hart-fact oracle checked for every
   sample.
4. **Route comparison:** typed compatibility, A7 ordinary raw, selected atomic,
   and later native/TLM/DMI paths on identical fixtures.
5. **Observation comparison:** observation disabled, commit/trap observation
   enabled, and log serialization where applicable; results must match before
   overhead is interpreted.
6. **Environment/evidence:** exact revision, toolchain/container, CPU/OS,
   compiler flags, fixture/linker hashes, warm-up/repetition policy, all samples,
   distributions, and artifact retention.

The first baselines should be observations, not gates. Thresholds require a
calibration period across stable runners and representative workloads. Stage 1
needs atomic-envelope and writer-visibility measurements only to catch accidental
cost or lock changes; Stage 2 needs observer-on/off overhead and public
load/execute separation; Stage 6 needs native/TLM/DMI parity and delay accounting.
No stage may optimize by weakening side-effect, observation, deadline, or
invalidation semantics.

Longer term, a weekly exact-revision trend artifact can become a regression
monitor after its host/toolchain and retention policy are repaired. Block
execution, translation, direct RAM, DMI, and temporal decoupling are conditional
strategies after the semantic/lifecycle contracts and invalidation tests exist.
This task ran no new performance experiment and proposes no runtime optimization.

## 8. Debt ledger and re-evaluation triggers

| Debt | Handling recommendation | Why deferred or selected | Re-evaluate when |
| --- | --- | --- | --- |
| AMO/LR/SC width and encoding defects | Stage 1, after an explicit profile table and baseline characterization. | A7 was forbidden to repair them incidentally; repeating the old route without this decision would make the atomic envelope ambiguous. | A selected user workload requires atomics, or an encoding/width mismatch blocks Stage 1 exit. |
| Global reservation, reset retention, faulting-SC behavior | Stage 1 per-Hart migration with profile-defined fault semantics. | Current tests preserve the singleton and its defects; this is not architectural evidence. | Any claim of atomic conformance, a second Hart, or a public writer that can run concurrently. |
| Visibility from all writers | Stage 1 writer inventory and notification/quiescence policy. | A7 intentionally retained no invalidation for scalar/FP/host writes; future DMA is a separate inbound-master contract. | Host mutation or an additional platform master is needed while a Hart is active. |
| Complete atomic envelope / legacy bridge | Stage 1; do not exit bridge until one-operation and failure evidence exists. | ADR-0002 makes this a physical contract, not a scheduler or TLM naming exercise. | A new backend or transport advertises atomics, or a test can observe an intermediate read/write. |
| Hart memory/atomic/log effects | Stage 2. | Current typed outcomes are useful, but Runner snapshots and `mem_access = None` are not full ADR-0001 observations. | A public trace/differential consumer or block/DMI strategy needs authoritative effects. |
| Machine/Platform composition and lifecycle | Stage 2. | Current `executor.rs` has partial shared decisions but no safe composition root or drain proof. | A second platform backend, debug mutation, external host, or reset-after-failure use case is approved. |
| Unknown-completion recovery | Stage 2 and every later adapter. | A7 has terminal no-retry state at a narrow seam, not lifecycle-wide quiesce/drain evidence. | Any backend can complete asynchronously, callback late, or report uncertainty after side effects. |
| Sv39/MMU page walks and A/D | Stage 3, selected profile only. | Components are tested but the core uses no MMU and page walks use a separate interface. | A mapped public guest or protection-sensitive workload is approved. |
| PMP and privilege profile | Stage 3 scope decision. | Configuration fields and CSR tests are not enforcement. | A user chooses a supervisor/user or protected platform target. |
| Interrupts, WFI, time, counters | Stage 4, after Machine/input boundaries. | Components exist but no public line, wait, or time exchange exists; do not assume an A8/A9 split. | A concrete timer/device/event workload is approved and the Hart profile is selected. |
| Public debug wiring | Stage 5 after lifecycle/inspection. | Mock `DebugTarget` support cannot safely mutate the active core. | A debugger-driven workflow is a product requirement. |
| Native/TLM/SystemC/DMI | Stage 6 conditional. | Existing TLM/DMI vocabulary has no Hart adapter, FFI, atomic proof, or invalidation contract. | An external platform/integration consumer and pinned toolchain are available. |
| Multi-Hart/DMA/coherence | Stage 7 conditional, separate contract. | No product decision, inbound-master contract, ordering, or coherence model is approved. | A concrete multi-agent VP use case plus resource/verification approval exists. |
| Full extension/ACT4 scope | Keep bounded; do not auto-enlarge. | The two 51-case suites are not full-ISA or atomic certification. | A profile, external suite, toolchain, and acceptance budget are separately approved. |
| Linux/OS boot | Keep out of this roadmap. | It would add SBI, device tree, devices, privilege/MMU, interrupts, and product commitments beyond the goal. | User explicitly chooses an OS/firmware product target and approves a prerequisite roadmap. |
| Performance regression/optimization | Stage 0 baselines, then evidence-driven optimization only. | Current seven groups are mostly component/synthetic; 1.2858 is one historical sample. | Repeated public baseline has calibrated variance and a real hotspot tied to a stable contract. |

## 9. Compatibility changes requiring explicit approval

A successor contract must state old→new behavior and tests for any of these:

- AMO/LR/SC width or encoding results, `aq`/`rl`, reservation granule, reset,
  SC failure, or faulting-SC effects;
- whether `RiscVSimulator::write_mem`, native host writes, image installation,
  and other host mutations are quiescent control operations or physical writers;
- replacing typed atomic calls with an envelope, changing lock/callback order,
  or removing/deprecating old `MemoryInterface`/core constructors;
- public `ExecutionResult.cycles`, trap continuation, stop/fact taxonomy,
  commit-log contents, memory-effect fields, or observation allocation behavior;
- native address maps, UART/HTIF widths, flat offsets, artifact/reporting
  policies, or image replacement/reset semantics;
- selected privilege/MMU/PMP profile, page-fault/access-fault mapping, A/D
  behavior, interrupt/WFI rules, and time/counter units;
- debug API/CLI/GDB behavior, direct memory/register mutation, or breakpoint
  patching policy;
- TLM/SystemC FFI ownership, thread model, blocking/non-blocking transport,
  DMI permissions/latencies/invalidation, and external-kernel stop behavior; and
- any benchmark threshold, workload selection, or CI retention/host policy.

No compatibility change is approved by this proposal. If implementation evidence
conflicts with an accepted ADR, the conflict must stop the candidate and be
surfaced for a decision rather than hidden in an adapter.

## 10. Verification strategy for every future candidate

### Rust unit and component evidence

Use focused tests for the local contract: request/response binding, atomic
operation envelopes, reservations, lifecycle state machines, fact ordering,
page walks, interrupt sources, debug managers, and adapter status/delay mapping.
A passing component test proves only that component behavior. Existing tests such
as `tests/translation_test.rs`, `tests/ad_bits_test.rs`, `tests/tlm_tests.rs`,
and `src/debug/*` tests must be retained and supplemented rather than relabelled
as public support.

### Public ELF and facade evidence

Every integration candidate needs fresh, isolated project-authored ELF fixtures
through the real CLI and library facades where applicable. Preserve the existing
51-guest project suite as one bounded set; add small purpose-built guests for
atomic, page-fault, timer/WFI, debug, and lifecycle cases. Check guest exit,
completed turns, final PC, architectural state, memory/device side effects,
artifacts, and observer output. Do not reuse pre-existing ELF output as a fresh
build claim.

### Differential and ACT4 selection

Use a differential/reference suite only when the chosen profile and feature set
match the suite's scope. ACT4's frozen 51 cases remain **nontrapping RV64I
selection evidence**; they are not A-extension, privilege, MMU, interrupt,
misalignment-success, whole-ISA, or OS certification. Atomic and privileged
candidates may use an explicitly selected Spike/Sail/reference comparison or a
small independent semantic oracle, but its profile, revision, exclusions, and
failure controls must be recorded separately. Never merge the two 51-case counts
into one “102-case full coverage” claim.

### Negative failure and side-effect evidence

For each physical, lifecycle, and adapter result, test the negative path as well
as success:

- rejected spans, unsupported width/category, and malformed responses;
- for a rejected operation, no partial RAM/MMIO mutation, FIFO dequeue,
  callback, exit, reservation invalidation, retirement, or fabricated trap;
- host/protocol failures tested against their declared side-effect contract,
  without fabricated retirement or trap;
- unknown completion tested both before and after a non-rollbackable external
  effect; the latter requires explicit uncertain-state handling, no retry, and
  safe drain/recovery before reset or reuse, not proof that the effect vanished;
- atomic AMO/SC indivisibility and writer visibility;
- original architectural address and access-kind cause mapping;
- observer failure after a committed fact; and
- no DMI/translated-block use after the relevant invalidation.

### Exact-head evidence and retention

Each candidate closeout must record the exact committed HEAD, source/test/CI
identities, toolchain/container, commands, per-case results, unavailable tools,
artifact hashes, and known limitations. Run the repository quality gate
proportional to the change and use the immutable task validation contract; static
links, copied check IDs, or prose are not proof of a passing run. The reviewed
worktree is frozen during independent review; findings apply only to their exact
PR head and require same-scope fix/reverification.

The roadmap task itself is documentation-only. It must not alter ADRs,
`.qing/config.toml`, runtime code, tests, historical evidence, or the current
A7 contract. A later implementation task must independently run and record its
checks; this proposal does not claim new performance, ELF, ACT4, or runtime
verification.

## 11. User decisions before a successor contract

The following bounded choices should be answered before activating any successor:

1. **Next candidate:**
   - A — atomic/physical convergence first (recommended);
   - B — observation plus N=1 Machine/lifecycle first, retaining an explicit
     atomic debt gate; or
   - defer implementation and perform only the Stage 0 evidence work.
2. **Atomic profile:** which RV64A widths/encodings, `aq`/`rl`, reservation
   granule, SC-fault behavior, and compatibility corrections are in scope?
3. **Host-writer policy:** are direct host writes forbidden during a run and
   admitted only after drain, or do they participate in the physical writer/
   reservation domain?
4. **Public compatibility:** which old typed constructors, `ExecutionResult`,
   cycle terminology, commit-log format, and native/flat differences must remain
   source-compatible versus receive an adapter/deprecation plan?
5. **Hart/profile boundary:** which privilege, MMU/PMP, interrupt/WFI, and
   counter behaviors are the first supported profile, without implying Linux or
   full-ISA support?
6. **External integration:** is there a concrete SystemC/TLM consumer and pinned
   toolchain to justify Stage 6, or should it remain conditional research?
7. **Performance policy:** which workloads and stable runners are acceptable for
   baseline calibration, and when may a threshold become a required gate?

These choices are proposal inputs, not decisions silently made here.

## 12. Navigation and status boundary

The current A7 contract, accepted ADRs, and A7 closeout/evidence remain the
sources of truth until a successor is separately approved. The archived A7
closeout keeps its original historical wording and evidence identity; the merged
closeout status is corrected only in current navigation where necessary. This
proposal is the reviewed planning input for a later decision, not a replacement
for `docs/dev-plan.md`.

Related records:

- [A7 closeout record](../archive/milestones/a7-closeout-record.md)
- [A7 capability/T5 evidence](../verification/a7-capability-assessment.md)
- [A7 physical contract](../verification/a7-physical-contract.md)
- [A7 native targets](../verification/a7-native-targets.md)
- [A7 Hart and legacy bridge](../verification/a7-hart-physical.md)
- [A7 public equivalence](../verification/a7-public-equivalence.md)
- [A7 migration characterization](../verification/a7-migration-characterization.md)
- [Target architecture](../architecture/README.md)
- [Current implementation architecture](../architecture/current-state.md)
- [Documentation policy](../documentation-policy.md)
