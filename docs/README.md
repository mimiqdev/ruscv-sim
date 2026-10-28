# Documentation

**Status:** Current index

**Authority:** Normative navigation

**Last reviewed:** 2026-10-19

This index is the entry point for project decisions and technical documentation. Files under `archive/` preserve history but must not drive current implementation.

## Sources of truth

1. [Milestone plan and activation status](dev-plan.md) — Stage 6 is the sole Current contract, activated by PR #96 as `a8851c4ac57500eb54dadc89919cd7314b626594` at 2026-10-19T13:49:34Z.
2. [Target architecture](architecture/README.md) — intended product boundaries and ISS → VP evolution.
3. [A7-era implementation snapshot](architecture/current-state.md) — historical source-to-target inventory; use current source/tests, the [A8 assessment](verification/a8-closeout-assessment.md) for the atomic route, and the [A9 acceptance assessment](verification/a9-closeout-assessment.md) for Hart facts/Machine integration, separating historical checkpoint and final PR/merge-head evidence.
4. [Architecture principles](architecture/principles.md) — normative ownership, language, address, and error boundaries.
5. [Development environment](development-environment.md) — normative container toolchain and usage.
6. [Documentation policy](documentation-policy.md) — status, authority, and archival rules.
7. Source code and verified tests — authority for what is implemented today.

The authority split is deliberate: the milestone contract owns scope and acceptance criteria; accepted ADRs own cross-cutting architecture decisions; source code plus verified tests own current behavior and integration evidence; and Git history records change provenance.

Target architecture is not implementation status. Component presence is not end-to-end support.

## Architecture

- [Architecture diagrams](architecture/README.md)
- [A7-era implementation and gap snapshot](architecture/current-state.md)
- [Architecture principles](architecture/principles.md)
- [Architecture decision records](architecture/decisions/README.md) — ADR-0001 through ADR-0004 accepted on 2026-09-07
- [A0 closeout record](archive/milestones/a0-closeout-record.md) — accepted architecture, completion evidence and limitations
- [Implementation scope navigation](architecture/first-implementation-scope.md) — A1–A4 completed; A5 ACT4 detailed contract approved in PR #31; broader migration remains unapproved

## Current contract and proposal history

- [Approved Stage 6 contract](dev-plan.md) — sole Current since PR #96. RISC-V Debug Mode, Debug Specification 1.0. Decisions D0–D15 were owner approved on 2026-10-19. D0–D14 are the recommended option. D15 is `cmderr=3`.
- [Program-buffer debug successor proposal](proposals/post-stage6-program-buffer-debug.md) — Draft, informational recommendation D1–D9. Owner approval and accepted ADR amendments are required before activation.
- [Entire approved Stage 5 contract archive](archive/milestones/stage5-platform-interrupts-wfi-and-counters.md) — Platform interrupts, WFI, modeled time, and counters. Decisions D0–D9 were owner approved, recommended option, on 2026-10-16 13:08 CST. Implementation is PR #90 through PR #94. The full original body is preserved. Formal completion and archival took effect on the PR #96 activation merge.
- [Entire approved Stage 4 contract archive](archive/milestones/stage4-virtual-memory-and-protection.md) — Virtual Memory and Protection. Decisions recorded by PR #75, owner-authorized activation 2026-10-11 UTC, implementation and review at PR #88. The full original body is preserved. Formal completion and archival take effect on the Stage 4 to Stage 5 rotation merge.
- [Stage 5 approved informational proposal snapshot](proposals/stage5-contract-draft.md) — the complete drafting record of the decided contract. Original proposal wording is historical. Not a second Current contract, and not an implementation.
- [Stage 4 approved informational proposal snapshot](proposals/stage4-contract-draft.md) — the complete drafting record of the decided Stage 4 contract. Original proposal wording is historical. Not a second Current contract, and not an implementation.
- [Entire approved A10 contract archive](archive/milestones/a10-independent-public-path-performance-test-infrastructure.md) — Independent Public-Path Performance-Test Infrastructure, activated by PR #71 on 2026-10-08 and implemented by PR #72 (`5d23d33`). The full original body is preserved. Formal completion and archival took effect on the A10 to Stage 4 rotation merge.
- [A10 implementation closeout record](archive/milestones/a10-closeout-record.md) — P0–P3′ criterion-by-criterion evidence with commit IDs, merge-head CI, merged-main baseline re-run, limitations and re-evaluated unfinished work. The independent final PR-head review returned APPROVE for `c4fda56`.
- [A10 baseline observation](verification/a10-baseline.md) — three-workload baseline v1; original Darwin record kept, plus the merged-main re-run and the Linux/x86_64 session after the per-platform pin; informational, never a speed gate.
- [A10 infrastructure progress](verification/a10-stage3-progress.md) — P0/P1/P2 exact-head checkpoints, P3′ baseline delivery, P4′ close-out status and deferred-certification rationale.
- [Entire approved A9 contract archive](archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md) — full original activation/tasks/acceptance preserved; A9 formally completed/archived by PR #71 on 2026-10-08. PR #69 implementation accepted 2026-10-02, PR #70 closeout documents 2026-10-03.

- [A8 single-Hart atomic/physical convergence contract archive record](archive/milestones/a8-single-hart-atomic-physical-convergence.md) — archived by PR #68; the snapshot preserves A8's approved profile and full scope. Implementation evidence at `4c4d0a295f620ed85557b10475d46bef1be5ea17` and limitations are in the [A8 closeout assessment](verification/a8-closeout-assessment.md). [Approved A8 proposal snapshot](proposals/a8-single-hart-atomic-convergence.md) remains drafting history.

- [Archived A7 non-atomic physical-access contract](archive/milestones/a7-non-atomic-physical-access-migration.md) — approved and activated 2026-09-18, following formal A6 acceptance; fetch and ordinary integer/FP loads/stores migrated to the raw physical boundary while legacy atomics retained behavior on shared storage/locking as explicit debt. Activation merged in PR #47 as `9ee9c5f24c072779f06ce141b3340f953b614442`; the A7 closeout delivery is merged at `c059500a6af20f93569099c4b05ced3364a7703b`; [closeout record](archive/milestones/a7-closeout-record.md) and [proposal history](archive/milestones/a7-non-atomic-physical-access-proposal.md) remain archived.

- [Post-A7 short-term technical development roadmap proposal](proposals/post-a7-roadmap.md) — Draft A7-after technical implementation planning; not an overall product roadmap and does not activate a successor contract.


- [A10 / Stage 3 approved informational proposal snapshot](proposals/a10-performance-test-infrastructure.md) — detailed scope approved 2026-10-03; original decision-request phrasing is historical. Not a second Current contract, not implemented, no performance result. The A7-era roadmap's global-reservation/current-A7/host-writer-choice claims are historical and superseded by A8/A9, not inherited work.

- [A8 single-Hart atomic/physical convergence proposal snapshot](proposals/a8-single-hart-atomic-convergence.md) — the unchanged 2026-09-21 drafting source of the A8 archive record; not a second active contract.

## Verification

- [Verification architecture](verification/README.md)
- [A1 closeout and acceptance evidence](archive/milestones/a1-closeout-record.md)
- [A4 acceptance and closeout](verification/a4-capability-assessment.md) — final criterion evidence, independent review and merge CI; closeout and the A5 contract approved in PR #31
- [A5 capability assessment and closeout](verification/a5-capability-assessment.md) — frozen profile approved; 51/51 selected ACT4 passes, exact CI/review evidence and limitations; formally closed out in PR #37
- [A5 feasibility experiment](verification/a5-feasibility.md) — preserved setup and smoke history, not the final selection result
- [A1 public behavior compatibility matrix](verification/public-behavior-matrix.md)
- [A1 public behavior defect and gap register](verification/public-behavior-gaps.md)
- [Project-authored bare-metal tests](verification/bare-metal-tests.md)
- [A6 capability and acceptance assessment](verification/a6-capability-assessment.md)
- [A6 retained A5 ACT4 replay summary](verification/a6-act4-replay-35325844246.json)
- [A7 bounded closeout record](archive/milestones/a7-closeout-record.md) — six-item acceptance, exact PR/source identities, independent cumulative review outcome, two 51-case boundaries, and residual debt
- [A8 bounded closeout assessment](verification/a8-closeout-assessment.md) — T0–T5 evidence matrix, exact implementation/CI identities, fresh 58-case project guest inventory, separate frozen ACT4 evidence, and residual limitations; historical A8 evidence, not A9 implementation evidence
- [A9 stage progress](verification/a9-stage2-progress.md) — frozen T0–T3 checkpoints, R1–R4 repairs and separately labeled local T4 evidence
- [A9 bounded acceptance assessment](verification/a9-closeout-assessment.md) — criterion audit, preserved checkpoint identity, separate final PR gate/review and exact merge-head CI; 58 fresh project CLI guests, Coverage skipped
- [A9 implementation closeout record](archive/milestones/a9-closeout-record.md) — technical acceptance 2026-10-02, closeout documents delivered 2026-10-03; approved successor and formal completion on actual rotation merge
- [A10 infrastructure progress](verification/a10-stage3-progress.md) — P0/P1/P2 exact-head checkpoints, P3′ baseline delivery, P4′ close-out status and deferred-certification rationale
- [A10 baseline v1 observation](verification/a10-baseline.md) — three-workload baseline; original Darwin record, merged-main re-run, and Linux/x86_64 session; informational, no speed gate
- [A10 public-path evidence schema](verification/a10-perf-schema.md) — `ruscv-perf/1` commands/statuses and the deferred calibration/certification boundary
- [A10 contract-rotation executable audit](verification/a10-contract-rotation.md) — historical audit of the A9 to A10 rotation; documentation checks only
- [Stage 4 contract-rotation executable audit](verification/stage4-contract-rotation.md) — historical audit of the A10 to Stage 4 rotation; the promoted-text check is pinned to `9f6f678`
- [Stage 5 contract-rotation executable audit](verification/stage5-contract-rotation.md) — historical audit of the Stage 4 to Stage 5 rotation; the promoted-text check is pinned to `02f20b1`
- [Stage 6 acceptance overview](verification/stage6-closeout/README.md) — §7 items 1–9, 433 baseline, corrected 5c code review and CI, local-gate boundary, and rerunnable evidence audit. This is not formal milestone completion.
- [Stage 6 cumulative final review](verification/stage6-final-review.md) — initial REQUEST_CHANGES at 433 and corrected code APPROVE at 5c, with execution evidence recorded separately.
- [Stage 6 U5 tool record](verification/stage6-exit.md) — preserved original OpenOCD/GDB/Spike session and negative controls, read with the cumulative overview.
- [Stage 6 contract-rotation executable audit](verification/stage6-contract-rotation.md) — whole-body preservation of the Stage 5 contract and a bounded check of the Stage 6 contract; no runtime or performance claims
- [A9 T4 per-case source/tool/artifact evidence](verification/a9-t4-guest-evidence.json) — self-contained checkpoint execution record, not a new ACT4 run
- [External RISC-V test integration contract](verification/external-riscv-tests.md)
- [Commit tracing and differential testing](verification/commit-tracing.md)

## Integration

- [Integration boundaries](integration/README.md)
- [SystemC/TLM boundary](integration/systemc-tlm.md)

## Reference

- [Reference index and authority rules](reference/README.md)
- [Code-generation component status](reference/code-generation.md)

## Research

- [Research index](research/README.md)
- [Execution performance directions](research/performance.md)
- [Heterogeneous platform directions](research/heterogeneous-platforms.md)
- [Linux boot requirements](research/linux-boot-requirements.md)

Research records options and constraints. It does not add work to the active milestone.

## Archive

- [Archive policy and categories](archive/README.md)
- [Milestone records](archive/milestones/README.md)

Archived files retain useful context, pseudocode, measurements, and rejected or superseded approaches. Any idea returning from the archive must be revalidated and accepted through the current architecture process.
