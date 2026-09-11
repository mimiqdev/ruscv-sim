# Archive context

**Status:** Historical contract; formal closeout proposed

**Authority:** Informational; not the current milestone contract

**Implementation completion / record preparation date:** 2026-09-11

A4 implementation and required verification are complete at `e47a1b0`.
Formal acceptance of the [closeout record](a4-closeout-record.md) and approval
of the [prospective A5 contract](../../dev-plan.md) remain pending through
the closeout PR merge. This header does not claim that merge has occurred.
The [assessment](a4-capability-assessment.md) preserves the detailed mapping.
The original contract below is retained with relocated links; its Active and
pending-evidence statements describe the pre-T3-merge state.

---

# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A4 — One Host-Side Path for Run Control and Image Installation

**Status:** Active — bounded T1 decision and T2 installation merged; T3 integrated tests and [capability assessment](a4-capability-assessment.md) locally verified, with committed-head independent review and merge-time evidence pending

**Authority:** Normative milestone contract

**Approved / started:** 2026-09-11

This is the only current milestone contract. [A3 is completed](a3-closeout-record.md)
with documented limitations and its full [capability assessment](a3-capability-assessment.md).
Milestone identifiers are not releases.

## Objective

The CLI `load_and_run` and the `RiscVSimulator` facade install an image and reach
their stop decision through one internal path, so the two largest remaining
duplicated host-side responsibilities cannot drift between the configurations.
After A3 shared image placement and result construction, both entry points still
separately implement stepping with budget accounting, exit-detection order and
RAM-signal clearing, and both still build RAM, load the program, construct a core
and reset it. Each configuration keeps its own signal sources and its own
override precedence; sharing the decision does not merge those.

The drift evidence is concrete: G-10 was a wrapper-only defect in exactly this
code, where one loop cleared the RAM signal after decoding the exit while the
other retained it. This milestone removes the duplication that allowed it. It is
not the Runner/Machine/Platform migration and does not claim those boundaries
integrated.

## Starting evidence and boundary

- [A3 closeout](a3-closeout-record.md) and its
  [capability assessment](a3-capability-assessment.md) record
  what A3 shared, what remains duplicated, and the retained configuration
  differences that must survive this change.
- [`load_and_run`](../../../src/executor.rs) and [`RiscVSimulator`](../../../src/executor.rs)
  each own a loop and an installation sequence today.
- The [matrix](../../verification/public-behavior-matrix.md) CLI-versus-library row,
  its A3 equivalence section and the [gap register](../../verification/public-behavior-gaps.md)
  are the compatibility baseline.
- [ADR-0003 §9](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
  keeps both entry points available and requires the CLI and flat-library
  configurations to stay explicit. One run-control owner is the accepted target;
  this milestone implements a bounded step of it, not the composition.

The supported configuration stays one Hart, synchronous flat RAM or bus RAM,
little-endian ELF64 fixtures using already exercised instructions, and bounded
allocations and requests.

## Required observable behavior

### One run-control decision

- A single internal owner decides, for a retired instruction or a failure,
  whether the run continues, stops with a guest exit, stops with a timeout or
  stops with an execution error, and reports how many instructions retired.
- Budget semantics are decided there and keep their current behavior: zero budget
  executes no instruction and reports a timeout; exhaustion reports a timeout with
  the exact text in use; an exit in the final permitted slot is not a timeout.
- Exit handling is decided there with their current ordering per configuration:
  which observable signal is checked first, that a decoded exit is retained before
  its RAM signal is cleared, and that no path re-reads a cleared signal.
- Each configuration still supplies its own signal sources — the native bus
  observes the HTIF endpoint and a selected RAM address, the flat library observes
  its selected offset — and keeps its own address form for that selection.

### One image installation

- A single internal path installs a loaded image for a configuration: it creates
  the configuration's memory, loads the program, constructs the core and resets it
  to the entry point, given that configuration's memory backend.
- The CLI keeps its bus composition (RAM at the image base, UART, HTIF callback)
  and the flat library keeps its bare flat memory; installation shares the
  sequence, not the configuration.

### Preserved compatibility

- Public signatures, CLI options and output, `read_mem`/`write_mem`/`set_tohost`
  address meanings, exit encodings, device behavior, artifact policy and the
  retained differences recorded in the A3 assessment are unchanged.
- The two loops are not merged into one loop, and the milestone does not claim the
  Machine/Platform composition.

## Task decomposition and PR cadence

These are delivery tasks under one milestone, not additional milestones. They may
be split or combined into reviewable PRs without changing the acceptance goal.

| Task | Contribution | Dependency / evidence |
| --- | --- | --- |
| T1 — Shared run-control decision | One owner for the budget, stop reason, exit ordering and signal clearing, used by both loops, with the configurations still supplying their own signal sources. | The A1–A3 exit, limit and equivalence tests must pass unchanged, plus focused tests for the decision itself. |
| T2 — Shared image installation | One sequence that installs a loaded image for a configuration, given its memory backend. | The CLI and flat suites must pass unchanged, plus installation tests for both configurations. |
| T3 — Integrated equivalence and documentation | Prove through committed tests that both entry points keep their documented behavior on the shared path, and record what changed. | Depends on T1–T2; exact-head review, full gate and merge-time guest evidence. |

Preserve before changing: reproduce any behavior difference before adjusting it,
and keep new discoveries explicit in the gap register instead of silently adding
them to this milestone.

## Architectural constraints

The [accepted ADRs](../../architecture/decisions/README.md), particularly
[ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md),
and [principles](../../architecture/principles.md) remain authoritative:

- One Hart, one ISA engine. The shared path decides and installs; it does not
  execute instructions itself, and it does not change instruction semantics.
- Host image handling stays separate from Hart-initiated physical transactions.
  Storage adaptation is not guest virtual-to-physical translation.
- Public entry points keep their names and behavior; prefer private helpers. Any
  new public item needs a stated reason and rustdoc.
- The milestone does not consolidate the two loops and does not claim
  Machine/Platform boundaries integrated.

## Non-goals and retained gaps

- Runner/Machine/Platform composition, new ports or boundaries, precise Hart
  outcomes/observations, scheduler or interrupt integration, merged run loops.
- Devices in the flat wrapper, MMU/PMP/paging, multi-hart, SystemC/TLM, new ISA
  support, acceleration, and ACT4 or any external architecture-suite compliance.
- Repairing G-03, G-04, G-05, the CLI half of G-06, G-07, G-08, G-09 or G-11;
  changing CLI options or the CLI signature-failure policy; `write_mem`
  transactional semantics; arbitrary allocation hardening; poisoned-lock or
  blocking-backend termination guarantees.
- Exhausting the default cycle limit or adding `tohost` symbol-fallback fixtures;
  those remain recorded unverified boundaries.
- Performance work and benchmark targets.

These exclusions bound the milestone; they are not extra future milestones.

## Deliverables and acceptance criteria

Deliver one run-control decision, one installation path, focused regressions,
integrated equivalence evidence and updated records. Completion requires all of
the following:

1. **One run-control decision.** Both entry points reach their stop decision
   through the same internal owner; no loop keeps a private copy of the budget,
   stop-reason, exit-ordering or signal-clearing logic. Tests cover zero budget,
   exhaustion, the final-slot exit, exit retention before clearing, and the
   ordering of the signals each configuration observes.
2. **One installation path.** Both entry points install a loaded image through
   the same sequence, each supplying its own memory backend; the CLI keeps its
   device composition and the flat library stays bare.
3. **Behavior preserved.** Every A1–A3 CLI and flat-library test passes unchanged;
   none is deleted or weakened. The retained differences recorded in the A3
   assessment remain accurate.
4. **Evidence is current.** Full quality gate, strict rustdoc, public CLI/ELF
   regressions and the merge-time guest compilation/execution run are recorded for
   the reviewed implementation or verified merge. Matrix, gap register and
   current-state documents identify precisely what changed and what remains
   unverified.
5. **Capability acceptance, not task counting.** A final assessment maps these
   scenarios to committed tests, records limitations and independent review, and
   proposes at most one successor from remaining architectural needs. T1 alone, or
   merely deleting duplicated lines, cannot close A4.

The quality gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, plus
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
Use the [development environment](../../development-environment.md) and
[guest commands](../../verification/bare-metal-tests.md). Record unavailable tools and
verification gaps explicitly; unmet capability criteria cannot be silently
reclassified as optional tasks.

## Closeout

Keep this as the only active contract. After capability acceptance and required
review/merge evidence, archive completion, limitations and revision identifiers;
re-evaluate unfinished work and select one separately approved successor.
Task/PR granularity does not determine milestone granularity.
