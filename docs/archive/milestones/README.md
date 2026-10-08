# Archived Milestones

This directory contains completed and explicitly superseded milestone plans with their historical outcomes, plus clearly labeled preparation records that do not claim completion.

Archived files are not active plans. They may describe component-level completion
that predates end-to-end integration or ACT4 verification. Consult the
[approved A10 contract](../../dev-plan.md), active on actual rotation merge following approval 2026-10-03 UTC. Until then main retains A9; on merge A9 is formally completed/archived.
The [A8 snapshot](a8-single-hart-atomic-physical-convergence.md) is historical.
A9's entire contract and standalone closeout below preserve distinct implementation and document-delivery evidence; this docs-only rotation is not A10 implementation.

## Rolling process

1. `docs/dev-plan.md` contains exactly one active milestone.
2. Once all acceptance criteria are verified, move that plan here as completed.
3. If planning is deliberately reset, archive the replaced plan as superseded without claiming completion.
4. Add completion evidence or the supersession reason and known limitations to the archived record.
5. Promote the next milestone into `docs/dev-plan.md` only after it is separately
   approved. The archived A8 record documents only its own bounded completion;
   the [post-A7 roadmap proposal](../../proposals/post-a7-roadmap.md) remains Draft
   technical planning context, not a second current contract.

## Records

- [A9: Entire approved Hart facts and safe N=1 lifecycle contract](a9-hart-facts-safe-n1-machine-lifecycle.md)
  - Technical acceptance 2026-10-02; closeout document delivery 2026-10-03; successor/rotation approved 2026-10-03 UTC. Full original body preserved, formal completion/archival effective on actual rotation merge.
- [A9: Implementation acceptance and closeout record](a9-closeout-record.md)
  - PR #69 merge `0898f121…` has exact merge-head runtime CI; PR #70 document merge `32bb28e…` has separate exact-head static review/document checks and synthetic PR-checkout CI, not direct-head runtime evidence. Docs-only main push CI is not scheduled by policy.
  - [Approved Stage 3 informational snapshot](../../proposals/a10-performance-test-infrastructure.md) is not a second Current contract or implemented facility.

- [A8: Single-Hart atomic/physical convergence — archived contract](a8-single-hart-atomic-physical-convergence.md)
  - Archived by A9 activation PR #68 on 2026-09-29 UTC. It records A8's 2026-09-21 activation, implementation merged at `4c4d0a2`, and [closeout assessment](../../verification/a8-closeout-assessment.md) merged at `902a428`; its original task/activation wording is historical, not A9 scope.
  - [Approved A8 proposal snapshot](../../proposals/a8-single-hart-atomic-convergence.md) remains unchanged historical context

- [A7: Non-atomic physical-access migration — bounded closeout record](a7-closeout-record.md)
  - Bounded non-atomic acceptance recorded 2026-09-19; closeout delivery merged at `c059500a6af20f93569099c4b05ced3364a7703b`; rolling successor selection remains pending
  - [Complete archived A7 contract](a7-non-atomic-physical-access-migration.md)
  - [Capability and T5 evidence](../../verification/a7-capability-assessment.md)

- [A7: Non-atomic physical-access migration — approved proposal snapshot](a7-non-atomic-physical-access-proposal.md)
  - Full candidate `2de6e96` approved 2026-09-18; superseded by the activated A7 contract, not a second Current plan

- [A6: Machine-mode synchronous trap entry and return — formal closeout](a6-closeout-record.md)
  - Formally completed/accepted 2026-09-18; preparation merged in PR #46 as `024d15d`, activation delivery PR #47 not claimed merged
  - [Capability and acceptance assessment](../../verification/a6-capability-assessment.md)
  - [Original A6 contract snapshot](a6-machine-mode-trap-entry-return.md)

- [A6: Machine-mode synchronous trap entry and return — original proposal](a6-trap-entry-return-proposal.md)
  - Historical proposal snapshot preserved from commit `9429e6a`; superseded by the [preserved A6 contract](a6-machine-mode-trap-entry-return.md)

- [A5: ACT4 RV64I selected external compatibility — forwarding record](a5-forwarding-record.md)
  - Post-closeout forwarding record preserved upon Milestone A6 activation
  - [Closeout record](a5-closeout-record.md)
  - [Preserved approved contract](a5-act4-rv64i-external-compatibility.md)

- [A5: ACT4 RV64I selected external compatibility — closeout](a5-closeout-record.md)
  - Frozen profile approved; all 51 required ELFs generated/executed/passed
  - [Preserved approved contract](a5-act4-rv64i-external-compatibility.md)
  - [Complete capability assessment](a5-capability-assessment.md)
  - [Current assessment navigation](../../verification/a5-capability-assessment.md)
  - Formally closed out in PR #37; subsequently succeeded by the approved A6 contract (historical A5 bodies retain their original status)

- [A4: Shared run control and image installation — closeout proposal](a4-closeout-record.md)
  - Closeout and detailed A5 contract approved in PR #31; original bodies preserve pre-merge status
  - [Original milestone contract](a4-shared-run-control-and-image-installation.md)
  - [Capability assessment with final evidence](a4-capability-assessment.md)
  - [Preserved approved A5 contract](a5-act4-rv64i-external-compatibility.md)

- [A3: Shared image placement and result path — completion record](a3-closeout-record.md)
  - [Original milestone contract](a3-shared-image-placement-and-result-path.md)
  - [Capability assessment](a3-capability-assessment.md)

- [A2: Reliable flat-library ELF execution and inspection — completion record](a2-closeout-record.md)
  - [Original milestone contract](a2-reliable-flat-library-workflow.md)
  - [Capability assessment](a2-capability-assessment.md)

- [A1: Public behavior baseline — completion record](a1-closeout-record.md)
  - [Original milestone contract](a1-public-behavior-baseline.md)
  - [Pre-closeout acceptance assessment](a1-acceptance-assessment.md)

- [A0: ISS → VP architecture baseline — completion record](a0-closeout-record.md)
  - [Original milestone contract](a0-architecture-baseline.md)
  - [Pre-closeout assessment](a0-pre-closeout-assessment.md)
  - [Unapproved full-migration candidate](a0-full-migration-candidate.md)

- [M1: ISA foundation](m1-isa-foundation.md)
- [M2: Memory and peripherals](m2-memory-peripherals.md)
- [M3: Peripheral quality](m3-peripheral-quality.md)
- [M4: Debug support](m4-debug-support.md)
- [M5: ELF execution loop](m5-elf-execution.md)
- [M6: Regression test quality](m6-test-quality.md)
- [M7: Spike-compatible commit log](m7-commit-log.md)
- [M8: ACT4 RV64I baseline — superseded](m8-act4-rv64i-superseded.md)

Supporting historical reports and execution plans are stored alongside their milestone summaries.
