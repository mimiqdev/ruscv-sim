# Documentation

**Status:** Current index

**Authority:** Normative navigation

**Last reviewed:** 2026-09-19

This index is the entry point for project decisions and technical documentation. Files under `archive/` preserve history but must not drive current implementation.

## Sources of truth

1. [Current milestone contract](dev-plan.md) — objective, scope boundaries, non-goals, constraints, deliverables, and acceptance criteria.
2. [Target architecture](architecture/README.md) — intended product boundaries and ISS → VP evolution.
3. [Current implementation architecture](architecture/current-state.md) — descriptive source-to-target inventory and gap matrix.
4. [Architecture principles](architecture/principles.md) — normative ownership, language, address, and error boundaries.
5. [Development environment](development-environment.md) — normative container toolchain and usage.
6. [Documentation policy](documentation-policy.md) — status, authority, and archival rules.
7. Source code and verified tests — authority for what is implemented today.

The authority split is deliberate: the milestone contract owns scope and acceptance criteria; accepted ADRs own cross-cutting architecture decisions; source code plus verified tests own current behavior and integration evidence; and Git history records change provenance.

Target architecture is not implementation status. Component presence is not end-to-end support.

## Architecture

- [Architecture diagrams](architecture/README.md)
- [Current implementation and gap matrix](architecture/current-state.md)
- [Architecture principles](architecture/principles.md)
- [Architecture decision records](architecture/decisions/README.md) — ADR-0001 through ADR-0004 accepted on 2026-09-07
- [A0 closeout record](archive/milestones/a0-closeout-record.md) — accepted architecture, completion evidence and limitations
- [Implementation scope navigation](architecture/first-implementation-scope.md) — A1–A4 completed; A5 ACT4 detailed contract approved in PR #31; broader migration remains unapproved

## Current contract and proposal history

- [Current A7 non-atomic physical-access contract](dev-plan.md) — approved and activated 2026-09-18, following formal A6 acceptance. Fetch and ordinary integer/FP loads/stores are in scope; legacy atomics retain behavior on shared storage/locking as explicit debt, not disabled or certified. Conservative compatibility defaults are approved. Activation merged in PR #47 as `9ee9c5f24c072779f06ce141b3340f953b614442`; the A7 closeout delivery is merged at `c059500a6af20f93569099c4b05ced3364a7703b`; the contract remains the sole current technical specification until a successor is approved. [Proposal history](archive/milestones/a7-non-atomic-physical-access-proposal.md).

- [Post-A7 short-term technical development roadmap proposal](proposals/post-a7-roadmap.md) — Draft A7-after technical implementation planning; not an overall product roadmap and does not activate a successor contract.

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
