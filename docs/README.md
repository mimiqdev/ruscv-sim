# Documentation

**Status:** Current index

**Authority:** Normative navigation

**Last reviewed:** 2026-10-01

This index is the entry point for project decisions and technical documentation. Files under `archive/` preserve history but must not drive current implementation.

## Sources of truth

1. [Milestone plan and activation status](dev-plan.md) — objective, scope boundaries, non-goals, constraints, deliverables, acceptance criteria, and whether the proposed successor is active.
2. [Target architecture](architecture/README.md) — intended product boundaries and ISS → VP evolution.
3. [A7-era implementation snapshot](architecture/current-state.md) — historical source-to-target inventory; use current source/tests, the [A8 assessment](verification/a8-closeout-assessment.md) for the atomic route, and the [A9 local assessment](verification/a9-closeout-assessment.md) for Hart facts/Machine integration. Local evidence is not final PR-head acceptance.
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

- [Current A9 contract](dev-plan.md) — activated by documentation-only plan rotation PR #68 at `b36b4d08e10b6919e096209be4817ab26441d526`; post-A7 roadmap Stage 2, limited to the existing native/flat single-Hart configurations. Hart-owned optional observation, Machine composition/lifecycle, and unchanged public CLI/library behavior are the bounded goal; no new board, MMU, interrupt scheduler, TLM or multi-Hart support is implied. The rotation itself implemented no Rust. Subsequent local A9 evidence does not replace final PR-head CI/review or declare milestone completion.

- [A8 single-Hart atomic/physical convergence contract archive record](archive/milestones/a8-single-hart-atomic-physical-convergence.md) — archived by PR #68; the snapshot preserves A8's approved profile and full scope. Implementation evidence at `4c4d0a295f620ed85557b10475d46bef1be5ea17` and limitations are in the [A8 closeout assessment](verification/a8-closeout-assessment.md). [Approved A8 proposal snapshot](proposals/a8-single-hart-atomic-convergence.md) remains drafting history.

- [Archived A7 non-atomic physical-access contract](archive/milestones/a7-non-atomic-physical-access-migration.md) — approved and activated 2026-09-18, following formal A6 acceptance; fetch and ordinary integer/FP loads/stores migrated to the raw physical boundary while legacy atomics retained behavior on shared storage/locking as explicit debt. Activation merged in PR #47 as `9ee9c5f24c072779f06ce141b3340f953b614442`; the A7 closeout delivery is merged at `c059500a6af20f93569099c4b05ced3364a7703b`; [closeout record](archive/milestones/a7-closeout-record.md) and [proposal history](archive/milestones/a7-non-atomic-physical-access-proposal.md) remain archived.

- [Post-A7 short-term technical development roadmap proposal](proposals/post-a7-roadmap.md) — Draft A7-after technical implementation planning; not an overall product roadmap and does not activate a successor contract.

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
- [A9 bounded local closeout assessment](verification/a9-closeout-assessment.md) — cumulative criterion audit, pinned-image full gate and 58 fresh project guests; final PR-head acceptance remains pending
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
