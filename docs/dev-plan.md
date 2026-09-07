# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A1 — Public Behavior Baseline for Architecture Migration

**Status:** Active — compatibility characterization and verification

**Authority:** Normative milestone contract

**Approved / started:** 2026-09-07

This is the only current milestone contract. The maintainer approved this bounded
successor to [A0](archive/milestones/a0-closeout-record.md). The larger migration
candidate is [archived reference](archive/milestones/a0-full-migration-candidate.md),
not approved implementation scope. Milestone identifiers do not designate releases.

## Objective

Establish trustworthy automated evidence for existing CLI/ELF and flat-library
compatibility requirements before migrating architectural boundaries. Distinguish
intended compatibility, observed behavior, known defects and unverified behavior
so a later refactor can identify regressions without preserving incorrect behavior.

A1 implements tests, fixtures and evidence documentation, not the target runtime.
It does not select an extension-wide ISA/compliance profile or certify ADR
implementation. Tests must name the instructions and configurations they exercise.

## Scope

- Inventory existing public APIs and CLI options, address/load behavior, observable
  results and run-limit semantics, citing source and focused tests.
- Strengthen or add public-path tests and minimal executable ELF fixtures for
  segment bytes/zero-fill, entry and nonzero load base, RAM effects, successful
  and nonzero exit, default/zero/exact limits, tohost precedence and final-slot
  exit, UART bytes, signature data and actual commit-log contents.
- Characterize `RiscVSimulator` and its public state/memory helpers separately;
  do not assume the flat configuration has the native CLI device/address behavior.
- Reproduce defects discovered within these surfaces and document intended
  behavior and precise residual gaps. Do not turn known incorrect logs or
  swallowed observer failures into compatibility promises.
- Record the resulting coverage, quality-gate and guest-suite evidence, and use
  it to propose one bounded successor objective.

## Non-goals

- Runner/Machine/Platform migration, physical-port implementation or production
  runtime refactoring, including the consolidation of existing run loops.
- Precise Hart outcomes, effect staging/journaling or a new observation pipeline.
- Production defect repair, new ISA support, a broad ISA repair campaign, changed
  public APIs, address maps or limit semantics. A needed repair requires a
  separately approved scope change, reproduction and regression test.
- MMU/PMP, compressed fetch, new devices, interrupt/time integration, production
  debugging, multi-Hart, DMA, SystemC/TLM, acceleration, Linux or ACT4 integration.
- A fixed delivery date or automatic approval of the larger migration candidate.

## Architectural and compatibility constraints

The [accepted ADRs](architecture/decisions/README.md) and
[principles](architecture/principles.md) remain authoritative. In particular:

1. Preserve one architectural engine; test helpers must not implement another ISA.
2. Keep CLI/ELF and flat-library configuration differences explicit.
3. Use [ADR-0003 §9](architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product)
   for compatibility requirements. Measure legacy successful-step limits as
   such, not as retirement, started-turn budgets, ISA cycles or virtual time.
4. Separate component tests, public-path integration and external compliance.
5. Compare intended behavior with actual results. If source/test evidence conflicts
   with an accepted contract, record the concrete conflict and seek a decision;
   do not silently change the contract, repair production code or bless the defect.
6. Regression tests for supported behavior must assert meaningful values and
   effects, not merely execution success or tautologies. A known-defect repro
   must identify the incorrect observation and expected behavior separately;
   excluded/ignored expected-failure checks must be visible in the gap register
   and must not count as passing compatibility coverage.

## Deliverables

- A public-behavior compatibility matrix under `docs/verification/`, with each
  required surface mapped to source, test/fixture and an evidence disposition:
  verified, reproduced defect, or unverified with a concrete reason.
- Focused tests and minimal fixtures with clear setup, expected results and
  provenance; CLI and flat-library evidence remain distinguishable.
- A bounded defect/gap register linked from that matrix, with reproductions where
  available and no implied production-fix commitment.
- Recorded verification commands, revisions, results, skipped checks and tool
  limitations; documentation distinguishes old evidence from A1 runs.
- An evidence-based successor proposal, not a second active milestone contract.

## Delivery cadence

Use three reviewable batches: compatibility inventory; tests and reproductions;
then acceptance evidence and successor recommendation. Each batch may use several
small PRs. These are delivery units, not additional milestones or fixed dates.

At each batch boundary, review evidence and scope. Escalate new obligations rather
than accumulating unrelated work. Keep only A1 active; plan the next milestone
in detail when the evidence supports doing so.

## Acceptance criteria

1. Every scoped compatibility surface appears in the matrix with an exact test
   reference or an explicit defect/unverified disposition and concrete rationale.
   Unverified entries are disclosed limitations, not proven compatibility.
2. Key public-path tests assert actual state, output or artifact contents. Weak
   assertions are replaced or explicitly excluded from behavioral proof.
3. Known incorrect behavior is separated from compatibility promises; in-scope
   defects have reproducible evidence or an explicit reproduction limitation.
4. CLI and flat-library evidence preserves their distinct configurations and
   names the exercised instruction/configuration surface without ISA-wide claims.
5. The full repository quality gate and project-authored guest suite have recorded
   run results. Unavailable dependencies, skips and failures are not passes;
   unresolved verification gaps must be explicitly adjudicated at acceptance,
   never silently waived. No repair outside this scope is implied by a failure.
6. Coverage, defects and verification limitations are summarized, and a bounded
   successor objective is proposed from that evidence rather than copied from
   the old migration candidate.

The full quality gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, and `cargo doc --all-features --no-deps`.
Use the repository [development environment](development-environment.md) and
[bare-metal verification commands](verification/bare-metal-tests.md) for guest
runs. Workflow definitions alone are not successful-run evidence.

## Closeout

Assess each criterion using committed tests and recorded verification. Record
completion evidence and limitations, re-evaluate unfinished work, and obtain
explicit approval for the next milestone before archiving A1 and replacing this
file. Do not automatically carry defects, migration increments or old backlog
forward. A1 approval does not authorize commits, pushes, PR creation or merging.
