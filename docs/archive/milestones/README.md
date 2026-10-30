# Archived Milestones

This directory preserves approved milestone contracts and explicitly superseded plans, including their scope and limitations. Clearly labeled preparation records do not claim completion.

Archived files are not active plans. Consult the [sole current contract](../../dev-plan.md). The owner-approved System Bus Access Debug target succeeds Program-buffer Debug Execution on the actual rotation merge. The [entire Program-buffer Debug Execution contract](program-buffer-debug-execution.md) and the [entire Stage 6 contract](stage6-riscv-debug-mode.md) are preserved here for historical scope; activation and completion do not claim implementation of the successor. CI and PR history retain execution and review outcomes.

## Rolling process

1. `docs/dev-plan.md` contains exactly one active milestone.
2. Once all acceptance criteria are verified, move that plan here as completed.
3. If planning is deliberately reset, archive the replaced plan as superseded without claiming completion.
4. Preserve the approved contract, completion boundary/date, source identifiers and known limitations. Keep execution and review outcomes in CI/PR history.
5. Promote the next milestone into `docs/dev-plan.md` only after it is separately
   approved. The archived A8 record documents only its own bounded completion;
   the [post-A7 roadmap proposal](../../proposals/post-a7-roadmap.md) remains Draft
   technical planning context, not a second current contract.

## Records

- [Program-buffer Debug Execution: Entire approved contract](program-buffer-debug-execution.md)
  - Preserves owner-approved D1–D9, the two-word/implicit-EBREAK profile, the Zifencei addition, and the bounded debug-execution scope. Implementation is PR #114 through PR #118, completed 2026-10-30 at merge head `3085f9e`. Formal completion and archival take effect on the System Bus Access Debug rotation merge; no execution results are copied here.

- [Stage 6: Entire approved RISC-V Debug Mode contract](stage6-riscv-debug-mode.md)
  - Preserves owner-approved D0–D15 and the temporary abstract-command profile. Formal completion and archival take effect on the Program-buffer Debug Execution rotation merge; no execution results are copied here.

- [Stage 5: Entire approved platform interrupt, WFI, and counter contract](stage5-platform-interrupts-wfi-and-counters.md)
  - Decisions D0–D9 were owner approved, recommended option, on 2026-10-16 13:08 CST. Implementation is PR #90 through PR #94, head `fb8399b`. The [final review](../../verification/stage5-final-review.md) found no instruction-behavior defect. The full original body is preserved. Formal completion and archival take effect on the Stage 5 to Stage 6 rotation merge.
- [Stage 4: Entire approved virtual memory and protection contract](stage4-virtual-memory-and-protection.md)
  - Decisions recorded by PR #75 on 2026-10-11; owner-authorized activation 2026-10-11 UTC; implementation and review at PR #88, `ee134b4`. The full original body is preserved. Formal completion and archival take effect on the Stage 4 to Stage 5 rotation merge.
- [A10: Entire approved independent public-path performance-test infrastructure contract](a10-independent-public-path-performance-test-infrastructure.md)
  - Activated by PR #71 on 2026-10-08; implementation PR #72 merged 2026-10-10; closeout PR #73 reviewed and merged 2026-10-10. The full original body is preserved. Formal completion and archival take effect on the A10 to Stage 4 rotation merge.
- [A10: Implementation acceptance and closeout record](a10-closeout-record.md)
  - Technical acceptance recorded 2026-10-10: PR #72 merge `5d23d337…` with exact-merge-head CI (run 38019391250), P0–P3′ criterion to commit evidence, merged-main baseline re-run, limitations and re-evaluated unfinished work. The independent final PR-head review returned APPROVE for `c4fda56`.
  - [Stage 4 approved informational snapshot](../../proposals/stage4-contract-draft.md) records the decided contract. It is not a second Current contract, and it is not an implementation.

- [A9: Entire approved Hart facts and safe N=1 lifecycle contract](a9-hart-facts-safe-n1-machine-lifecycle.md)
  - Technical acceptance 2026-10-02; closeout document delivery 2026-10-03; successor/rotation approved 2026-10-03 UTC. Full original body preserved; formally completed/archived by rotation PR #71 on 2026-10-08.
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

Legacy supporting records remain historical context. New milestone archives preserve contracts, not execution ledgers.
