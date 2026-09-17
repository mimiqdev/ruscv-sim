# Preserved Milestone Forwarding Record: A5 — ACT4 RV64I External Compatibility Baseline

> Archived A5 forwarding record snapshot, archived on 2026-09-17 upon maintainer approval and activation of Milestone A6.
> The body below preserves the post-closeout forwarding record and its historical status as recorded prior to A6 activation.
> The [current active milestone contract](../../dev-plan.md) is now Milestone A6 (Machine-Mode Synchronous Trap Entry and Return).

**Project:** `ruscv-sim`

**Current milestone:** A5 — ACT4 RV64I External Compatibility Baseline

**Status:** Formally closed out (PR #37 merged `d1834cc6566b342824bca30772cc829953a5c5ef`);
sole current milestone forwarding record until a successor is approved.

**Authority:** Historical forwarding record; preserved for change and milestone provenance.

**Updated:** 2026-09-15 (Archived: 2026-09-17)

## Approved scope and decision

The objective, constraints, non-goals, deliverables and five acceptance criteria
remain those of the preserved contract. The maintainer explicitly confirmed both
decisions in the [selection record](../../verification/a5-selection-proposal.md) on
2026-09-15: the narrow MXLEN provenance overlay is the A5 test adapter, and the
naturally aligned EEI excludes the eight successful-misalignment cases. These
are not trap exclusions or a whole-machine capability declaration.

The frozen selection is 51 sources/51 XLEN=64 self-check ELFs out of the
fully classified 1,970-source pinned inventory. All 51 passed clean ACT4/Sail
generation and public CLI execution at `f2edf77f17a76bea3d7062f40f5f4f38ad4eb580`.
That run preceded approval; only approval status changed, not semantic inputs.
The [closeout](a5-closeout-record.md) and
[assessment](../../verification/a5-capability-assessment.md) map every criterion and
explain the hash-preserving semantic comparison, reviews and limits.

## Closeout and successor boundary

PR #37 formally closed out A5 capability acceptance (merged
`d1834cc6566b342824bca30772cc829953a5c5ef`) with recorded independent closeout
review and exact-head CI. No A6 or other successor contract is approved, and no
successor implementation is scheduled.

The rolling workflow requires an **approved** successor before replacement.
Therefore the completed A5 forwarding record remains the sole current milestone,
with its original body archived for history, rather than substituting an invented
successor. It remains completed until a separately approved contract replaces
this forwarding record. Historical plans are not revived.
