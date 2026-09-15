# Development Plan

**Project:** `ruscv-sim`

**Current milestone:** A5 — ACT4 RV64I External Compatibility Baseline

**Status:** Implementation and acceptance evidence complete; formal closeout
pending independent review and separately authorized merge.

**Authority:** The sole current milestone contract. This forwarding record
incorporates the [preserved approved A5 contract](archive/milestones/a5-act4-rv64i-external-compatibility.md)
(PR #31, merged `a4804341f4ae0a5344beea7ef6c53667e1912c93`) with the explicit
profile decision below. The archived snapshot is not a second active plan.

**Updated:** 2026-09-15

## Approved scope and decision

The objective, constraints, non-goals, deliverables and five acceptance criteria
remain those of the preserved contract. The maintainer explicitly confirmed both
decisions in the [selection record](verification/a5-selection-proposal.md) on
2026-09-15: the narrow MXLEN provenance overlay is the A5 test adapter, and the
naturally aligned EEI excludes the eight successful-misalignment cases. These
are not trap exclusions or a whole-machine capability declaration.

The frozen selection is 51 sources/51 XLEN=64 self-check ELFs out of the
fully classified 1,970-source pinned inventory. All 51 passed clean ACT4/Sail
generation and public CLI execution at `f2edf77f17a76bea3d7062f40f5f4f38ad4eb580`.
That run preceded approval; only approval status changed, not semantic inputs.
The [closeout](archive/milestones/a5-closeout-record.md) and
[assessment](verification/a5-capability-assessment.md) map every criterion and
explain the hash-preserving semantic comparison, reviews and limits.

## Closeout and successor boundary

The closeout PR proposes A5 capability acceptance only. Merge is not yet
authorized. No A6 or other successor contract is approved, and no successor
implementation is scheduled.

The rolling workflow requires an **approved** successor before replacement.
Therefore A5 remains the one current completed/pending-closeout contract, with
its original body archived for history, rather than substituting an invented
successor. After formal closeout it remains completed until a separately approved
contract replaces this forwarding record. Historical plans are not revived.
