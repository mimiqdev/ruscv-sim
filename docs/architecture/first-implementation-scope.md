# Implementation Scope — Navigation

**Status:** Current navigation

**Authority:** Informational; not a second milestone contract

[A1: Public Behavior Baseline](../archive/milestones/a1-closeout-record.md)
completed on 2026-09-08, [A2](../archive/milestones/a2-closeout-record.md)
completed on 2026-09-11 and [A3](../archive/milestones/a3-closeout-record.md)
completed on 2026-09-11, all with documented limitations.
[A4](../archive/milestones/a4-closeout-record.md) completed implementation and
verification on 2026-09-11, sharing the run-control decision and image
installation between the CLI and the flat-library entry points. A2's G-02-only
successor selection and its own scope correction remain historical and are not
reopened here.

The [A4 capability assessment](../verification/a4-capability-assessment.md)
records integrated evidence for the two public entry loops, not
`RiscvCore::run`. T3 exact-head independent review and final merge CI succeeded,
including the separate 46/46 guest run. Formal A4 acceptance and the sole
[current A5 contract](../dev-plan.md), ACT4 RV64I external compatibility,
were approved in [PR #31](https://github.com/mimiqdev/ruscv-sim/pull/31),
merged as `a4804341f4ae0a5344beea7ef6c53667e1912c93` on 2026-09-11.

The [original full-migration candidate](../archive/milestones/a0-full-migration-candidate.md)
is preserved as historical input. It was not approved: physical/composition
migration, precise Hart outcomes and observation, and Runner consolidation are
not commitments of the A5 contract. Those choices require separate
re-evaluation and approval; remaining gaps are not automatically scheduled.

See the [A0 closeout record](../archive/milestones/a0-closeout-record.md) for the
architecture acceptance evidence and limitations. Accepted ADRs constrain later
work but do not imply implementation completeness or schedule every capability.
