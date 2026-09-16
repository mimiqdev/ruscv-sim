# A5 — ACT4 RV64I External Compatibility Closeout

**Status:** Formally closed out (PR #37 merged `d1834cc6566b342824bca30772cc829953a5c5ef`).

**Authority:** Informational completion record, not a successor contract.

**Evidence completion:** 2026-09-13. **Reconciliation/profile approval:** 2026-09-15.

**Final assessed implementation merge:** `f2edf77f17a76bea3d7062f40f5f4f38ad4eb580`.

## Outcome

A5 establishes **selected external compatibility**: 51 required sources,
51 generated self-check variants, 51 executed and 51 passed, with zero missing,
skipped or failed required cases. This is not certification. The
[full capability assessment](a5-capability-assessment.md) maps every contract
clause to code, focused tests, independent reviews and actual CI/artifact evidence.
The [original approved contract](a5-act4-rv64i-external-compatibility.md)
preserves its historical body, including superseded status claims.

| Acceptance criterion | Final disposition |
| --- | --- |
| Feasible pinned generation | Satisfied: clean pinned ACT4/Sail/compiler workflow and all 1,970 source dispositions. |
| Fail-closed public execution | Satisfied: bounded release CLI execution, complete identities/diagnostics and CI negative controls. |
| Full selected compatibility | Satisfied: 51/51 full generation and passes; branch/FENCE defects reproduced and repaired with public regressions. |
| Reproducible CI evidence | Satisfied: clean final generation and controls at the exact final implementation merge; full artifact hash verified and durable results retained. |
| Capability acceptance | Satisfied: formal closeout accepted via PR #37 merge (`d1834cc6566b342824bca30772cc829953a5c5ef`) with independent review and exact-head CI recorded. |

## Explicit profile decision and provenance

The maintainer confirmed both outstanding decisions on 2026-09-15: the narrow
MXLEN provenance overlay is the A5 **test adapter**, and the **naturally aligned
EEI** excludes eight successful-misalignment cases. The
[decision record](../../verification/a5-selection-proposal.md#final-decision-and-evidence)
preserves exact IDs and rationale; this is not an inferred trap exclusion.

The 2026-09-13 successful run preceded approval but used the identical proposed
selection/configuration. Only profile/inventory status bytes changed. The
[assessment's unchanged-input proof](a5-capability-assessment.md#source-accounting-and-unchanged-input-proof)
and regression-tested replay verify old byte hashes after reversing that one
metadata substitution. No source, oracle, executable configuration or selected
denominator changed, so no new ACT4 generation was needed.

## Final verification and review

- [ACT4 CI 34766332271](https://github.com/mimiqdev/ruscv-sim/actions/runs/34766332271):
  success, 4m41s, all 51 selected ELFs and controls at exact `f2edf77`.
- [Standard push CI 34749931575](https://github.com/mimiqdev/ruscv-sim/actions/runs/34749931575):
  success at the same head, including **46 separately compiled/executed
  project-authored guests**, release/smoke and Rust checks.
- [Exact implementation reviews and six-command Rust gate](a5-capability-assessment.md#exact-implementation-reviews-and-rust-verification):
  PRs #32–#36 merged; the separately reviewed memory/FENCE merge resolution
  preserves reviewed source blobs and passes focused combined checks.
- Local closeout verification: **25 Python tests**, shell syntax, Python
  compilation, whitespace checks, relative documentation links/anchors, and
  complete hash-checked replay of both final 51/51 and historical 44/51 archives.
  Each artifact replay passed 12 accounting-control assertions. No fresh
  Rust gate, separate local guest execution or ACT4 generation is claimed for
  this Python/metadata/documentation change.

Closeout PR #37 completed its own exact-head required CI and independent review,
merging as `d1834cc6566b342824bca30772cc829953a5c5ef`; implementation review
records are not falsely relabeled as review of this closeout.

## Retrieval and durable evidence

Artifact **10320897156**, **21,675,966 bytes**, expires
**2026-09-27T15:45:37Z**. Verified SHA-256:
`bb851d5486d16fe47d68fcc757f610fda7f1d2e3a2c431848dea35b7b514beb9`.
Use the [retrieval/replay commands](a5-capability-assessment.md#retrieval-and-durable-evidence).
The [final JSON](../../verification/a5-final-results.json) retains 51-case
identities, hashes, invocations, diagnostics, tool/configuration provenance and
control outcomes after the ephemeral Actions archive expires.

## Limitations and next decision

The frozen profile is single-Hart, little-endian, nontrapping RV64I on the
native physical RAM/UART/HTIF path, requiring natural data alignment.
The inventory partitions **51 + 8 + 654 + 355 + 902 = 1,970** sources.
No successful misalignment, architectural trap/return, privilege/CSR/interrupt,
MMU/PMP, FENCE.I, multi-Hart, concurrent/device ordering, OS boot, SystemC/TLM
or whole-ISA certification is claimed. Component coverage does not enlarge it.

G-13/G-14 are repaired; G-15 is a separately authorized bounds repair, not
an EEI expansion. Remaining gaps are re-evaluated in the full assessment.
Its sole optional successor suggestion, machine-mode synchronous trap/return
end-to-end verification, is **unapproved**. No A6 contract or implementation
authorization is invented. The [one current contract](../../dev-plan.md) remains
the completed A5 forwarding record until a successor is separately approved.
PR #37 merge accepted A5 only.
