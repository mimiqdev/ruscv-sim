# A0 — Architecture Baseline Closeout

**Status:** Historical — completed milestone

**Authority:** Informational completion record; not an implementation contract

**Completion / successor approval date:** 2026-09-07

## Outcome

The maintainer accepted ADR-0001 through ADR-0004, including the placement split,
and approved A1: a public behavior baseline before runtime migration. The
[approved A1 contract](../../dev-plan.md) replaces A0 as the sole active plan.
A0 completion means the architecture-definition deliverables are accepted, not
that the target architecture is implemented. This handoff document still follows
the normal branch/review/merge process; its preparation is not a claim that the
handoff change has already merged.

The [original A0 plan](a0-architecture-baseline.md) is preserved in full, as are
its [pre-closeout assessment](a0-pre-closeout-assessment.md) and the
[unapproved larger migration candidate](a0-full-migration-candidate.md).
Historical pending-approval statements in those snapshots are not current status.

## Acceptance evidence

| A0 criterion | Recorded evidence and disposition |
| --- | --- |
| Active architecture documents state status and ownership. | [Target views](../../architecture/README.md), [principles](../../architecture/principles.md), [inventory](../../architecture/current-state.md) and [four ADRs/index](../../architecture/decisions/README.md) state their authority and boundaries. Acceptance alignment merged in PR #11. |
| Development image builds and runs the quality gate and guest toolchain. | The original plan already records this criterion satisfied by development-image commits `f7db92d59b86b886d30919e25e863be614ffed2b` and `6fc097661bada37984376a4ba421d44e2b299ab7`. Preserve that established evidence; neither workflow presence nor this handoff is a fresh image/guest pass. |
| Dependency direction is unambiguous. | ADR-0003 §1 and target views define Frontend → Runner → Machine → {Hart, Platform}. The final PR #11 correction removes the ambiguous loader → Machine dependency: Runner obtains metadata and requests installation; Platform performs writes. |
| Hart, Runner, Machine, Platform, physical access, interrupts, time and observation ownership is agreed. | ADR-0001–0004 accepted on 2026-09-07 and merged through PR #11. This is semantic agreement, not runtime verification. |
| ISS and VP share one architectural engine. | Target views §6 and ADR-0003 §8 explain native/external hosting around the same Hart; N=1 is the initial configuration, not a claim of VP integration. |
| Current implementation is mapped without overstated integration. | The current-state inventory and pre-closeout assessment cite `src/core/mod.rs`, `src/executor.rs` and focused tests. The assessment records 136 focused tests and identifies weak assertions rather than treating them as proof. |
| Open decisions and deferred performance work are recorded. | The four ADRs' deferrals and principles distinguish semantic obligations from future mechanisms, APIs, scheduling/coherence and acceleration. These do not become A1 tasks automatically. |
| An approved successor replaces A0 as the only active contract. | The maintainer approved the bounded public-behavior-baseline proposal on 2026-09-07. `docs/dev-plan.md` now contains A1 only; the broader migration candidate was not approved. This criterion is realized by the handoff change containing this record. |

## Revision and verification evidence

- Architecture/documentation reset: `50d8c739f686b218a7e4fcc99619079d97833705`.
- ADR acceptance and alignment: [PR #11](https://github.com/mimiqdev/ruscv-sim/pull/11),
  merged 2026-09-07 at `34cf537c55859dc09ef2d5a525af15f6ac5f6e2a`.
- Final reviewed PR head: `fefa8fe475e05b8b1a4460049c0955a579f6ee8d`.
  Independent architecture review covered the earlier preparation; acceptance
  review found one stale placement chain, corrected in the final head. Follow-up
  review found that issue resolved and no new actionable findings. PR history
  records the review summary; review does not certify implementation.
- [Exact-head CI run](https://github.com/mimiqdev/ruscv-sim/actions/runs/34101336013):
  Quality and tests succeeded. Coverage, release/smoke and guest steps were
  skipped, not passed. This is evidence for the reviewed PR head, not a fresh
  verification of the milestone-handoff documentation or a release identifier.
- The pre-closeout assessment records 136 passing focused Rust tests on unchanged
  sources (executor 57, CLI 15, commits 13, ELF loader 15, traps 36), with zero
  failed/ignored. It separately scopes older documentation checks to `8339795`.
- Acceptance document checks at `36566d7` covered 12 files and 209 local
  links/anchors; the final one-line correction had clean primary LSP and diff
  checks. These historical results do not verify future document changes.

## Known limitations and re-evaluated work

- No target-runtime implementation, integrated VP, extension-wide ISA compliance,
  ACT4 verification or new public-path MMU/debug/TLM capability is established.
- Local Docker socket access was denied and the host RISC-V guest compiler was
  absent during the assessment. No fresh container build or graphical Mermaid
  rendering was recorded. Preserve prior image acceptance without inventing new
  local evidence. A1 must record its own applicable gate and guest-run results.
- Weak public-path assertions and logging/observer concerns motivate A1 evidence
  work. A1 does not authorize production fixes or freeze incorrect behavior.
- Runtime migration, precise Hart outcomes, physical-port integration, observation
  precision and Runner consolidation remain unapproved implementation work.
  Concrete execution profiles and API/time policy choices for that migration are
  deferred, not prerequisites for A1's characterization work.
- The larger candidate is retained only as historical input. A1's evidence will
  inform the next proposal; unfinished work is not automatically carried forward.
