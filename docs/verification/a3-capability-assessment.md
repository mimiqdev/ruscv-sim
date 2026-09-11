# A3 Capability Assessment

**Status:** Current

**Authority:** Informational; not the milestone contract, not an acceptance decision and not a milestone closeout

**Assessed revisions:** T1 `c67cb19`, T2 `ca78cd0`. The T3 change under review is
the head of its own pull request; its merge revision belongs in the closeout
record.

**Last reviewed:** 2026-09-11

## Purpose

[A3](../dev-plan.md) defines one bounded capability: the CLI `load_and_run` and
the `RiscVSimulator` facade resolve an image's declared metadata and build their
`ExecutionResult` through one internal path, so the two public entry points cannot
drift on placement, exit retention or artifact reporting.

This document maps the contract's acceptance criteria to committed tests and
recorded runs, states the limitations it does not claim, and proposes one
successor. It does not close A3: `docs/dev-plan.md` remains the only active
contract until the maintainer accepts completion.

## Delivered capability

| Task | Change | Revision |
| --- | --- | --- |
| T1 | One internal placement owner: `AddressForm` expresses the bus pass-through and the checked flat conversion, and `ImagePlacement` holds an image's declared metadata and resolves it in the requested form. Both entry points use it; the wrapper's private conversion is deleted (PR #23). | `c67cb19` |
| T2 | One internal result owner: `ResultInputs` assembles the exit, cycles, final PC, artifact and primary failure, with `ArtifactOutcome` describing the read and `ArtifactPolicy` an explicit input (PR #24). | `ca78cd0` |
| T3 | Committed equivalence evidence across both entry points, and the records updated. | PR head |

## Acceptance criteria mapping

### 1. One placement owner

| Scenario | Committed test | Observed |
| --- | --- | --- |
| Both address forms for the same metadata | `test_placement_resolves_both_address_forms` | The bus form returns the guest address, the flat form the offset, and the artifact address stays the guest metadata address |
| Base zero | `test_placement_base_zero_uses_raw_offsets` | Both forms coincide |
| Below base, beyond image, address-space overflow | `test_placement_rejects_unaddressable_flat_ranges` | Flat conversion errors; the bus form passes through unchanged |
| Absent metadata | `test_placement_absent_metadata_is_none` | No signal and no artifact in either form |
| Signature range boundaries | `test_placement_signature_range_boundaries` | The last addressable byte resolves, one past it fails, an empty range needs no bytes while an offset outside the memory is still refused |
| No duplicated arithmetic | `git diff de5f6c6..c67cb19` | The wrapper's `image_flat_offset` is deleted; the only remaining base-relative arithmetic is the CLI commit log's PC refetch, which is core PC-to-bus logging rather than image-declared metadata |

### 2. One result owner

| Scenario | Committed test | Observed |
| --- | --- | --- |
| Facts assembly | `test_result_inputs_assembles_observed_facts` | Exit, cycles, final PC, timeout flag, artifact bytes and metadata address carried through |
| Artifact policy | `test_result_inputs_reports_or_suppresses_artifact_failures` | The same failed read yields a diagnostic under `Report` and silent absence under `Suppress` |
| Primary failure preserved | `test_result_inputs_preserves_a_primary_failure` | Timeout or execution error survives alongside the artifact diagnostic, primary first |
| Absent and empty artifacts | `test_result_inputs_absent_and_empty_artifacts` | Absent yields no artifact; empty yields an empty artifact rather than a failure |
| Limit and error shapes | `test_result_inputs_timeout_shape_is_caller_supplied` | The caller-supplied failure keeps its distinct shape and accounting |
| One construction site | `src/executor.rs` | Exactly one `ExecutionResult` construction remains, inside the owner |

### 3. Behavior preserved

| Evidence | Observed |
| --- | --- |
| A1 and A2 suites unchanged | `git diff de5f6c6..ca78cd0 -- tests/` modifies no existing test; `public_behavior` grew from 40 to 44 with the T3 equivalence tests only |
| T1 differential | An independent review compiled a 45-scenario harness against both `de5f6c6` and the T1 head and observed byte-identical output |
| T2 differential | An independent review compiled a 36-record harness against both `c67cb19` and the T2 head: `sha256`-identical output covering both exits, an HTIF callback exit, an instruction error, budgets 3/6/2/0, the final-slot exit, all four artifact shapes, manual overrides, replacement and a cleared-signal probe |
| Retained differences still hold | `shared_result_keeps_each_configurations_artifact_policy`, `shared_result_shapes_hold_for_timeout_and_instruction_error`, `integrated_workflow_records_the_retained_cli_device_difference` |

### 4. Evidence is current

The full quality gate and strict rustdoc are recorded per change in the pull
requests below. Every merge revision ran the complete `main` pipeline — release
build, binary smoke test, guest compilation and the project-authored ELF runner at
**46 total / 46 passed / 0 failed**: `c67cb19` and `ca78cd0`. The matrix,
gap register and current-state documents were updated with each change, and the
gap register gained the CLI `--tohost` surface for the pre-existing G-11 panic.

### 5. Capability acceptance

This document. No acceptance decision is claimed: the successor below requires
separate approval, and the closeout record belongs to the maintainer.

## Retained differences between the configurations

| Aspect | CLI `load_and_run` | Flat `RiscVSimulator` | Evidence |
| --- | --- | --- | --- |
| Image placement | Bus address; RAM is mapped at the image base | Checked offset in the image buffer | `test_placement_resolves_both_address_forms` |
| Explicit tohost override | A bus address through the `--tohost` option | A flat storage offset through `set_tohost` | `shared_placement_selection_rule_holds_in_both_entry_points` |
| Devices | UART and the fixed HTIF endpoint | No device mapping; device MMIO fails | `integrated_workflow_records_the_retained_cli_device_difference` |
| Unreadable signature region | Documented silent absence | Explicit diagnostic | `shared_result_keeps_each_configurations_artifact_policy` |
| Instruction-error message | Names the PC in the message | Reports the boundary through `final_pc` | `shared_result_shapes_hold_for_timeout_and_instruction_error` |
| Shared | Declared tohost selection rule, exit decoding, timeout text, zero-budget and final-slot semantics, artifact metadata address | | `shared_result_shapes_hold_for_timeout_and_instruction_error`, `shared_result_returns_the_same_artifact_bytes_in_both_entry_points` |

## Limitations and unverified areas

- **The two run loops are still separate.** Stepping, budget accounting, exit
  detection order and signal clearing remain implemented in each entry point;
  A3's contract kept merging them out of scope.
- **Guest suite only in CI.** No RISC-V toolchain or Docker is available on the
  development host, and pull requests skip those steps by design.
- **Instruction-error wording** is caller-supplied, so the two configurations can
  differ in text even where the shape is identical.
- **G-11 family.** Unchecked bound arithmetic still panics for extreme addresses:
  the wrapper's manual tohost and, newly recorded, the CLI's `--tohost`. Not
  repaired and not scheduled.
- **A2's retained gaps** — G-03, G-04, G-05, the CLI half of G-06, G-07, G-08,
  G-09 — remain open and unscheduled.
- **Out of scope by contract**: Runner/Machine/Platform composition, precise Hart
  observations, devices in the wrapper, MMU/PMP, multi-hart, interrupts,
  SystemC/TLM, new ISA support, and ACT4 or any external-suite compliance.
- The capability is verified for one hart, synchronous flat or bus RAM,
  little-endian ELF64 fixtures using already exercised instructions.

## Independent review

Each change was reviewed against its committed head by a separate read-only
review context, with the coding worktree unmodified:

| Change | Pull request | Outcome |
| --- | --- | --- |
| T1 | [#23](https://github.com/mimiqdev/ruscv-sim/pull/23) | Approve with no blocking findings after a 45-scenario differential harness produced byte-identical output against the base. Findings addressed: document the poll invariant, extend G-11 to the CLI surface, quote the probe address in a runnable form. |
| T2 | [#24](https://github.com/mimiqdev/ruscv-sim/pull/24) | Approve with no blocking findings after a 36-record differential harness produced identical output, confirming all four CLI result sites, the wrapper path, and exactly one construction site. Findings addressed: a spliced doc comment and a superseded sentence. |
| T3 | [#25](https://github.com/mimiqdev/ruscv-sim/pull/25) | Review recorded in its own pull request. |

## Successor recommendation

**Recommended: one bounded run-control path.** A3 removed the last two
duplicated responsibilities in the host-side path — placement and result
construction — and each removal was justified by defects that duplication had
already produced (G-01, G-10, G-12 for placement and retention; the A2 exit
sequence for results). The one remaining duplication of the same kind is the run
loop itself: both entry points separately implement stepping, budget accounting,
exit detection order and RAM-signal clearing, and that duplication already
produced a wrapper-only defect (G-10, where only one loop cleared the signal
after retaining the exit). A bounded successor would give both entry points one
internal stop policy — the same discipline as A3 — with the existing A1–A3
suites required to pass unchanged and new evidence that both entry points reach
the same stop decision for the same image, budget and signal. It would not
implement the Machine/Platform composition, which stays a separate decision.

**Considered and not recommended now:** a bounded batch repairing the retained
public-behavior defects (G-03, G-04, G-05, the CLI half of G-06, G-11). Those are
reproduced or source-observed defects worth repairing eventually, but they are
independent of each other and of the architecture sequence, so a batch would mix
unrelated work into one milestone without the single testable capability A2 and
A3 each had. The evidence for the run-control path is also stronger: it is the
last shared responsibility in the public path, and one loop has already drifted.

Neither option is approved here. Selecting, scheduling and approving a successor
is the maintainer's decision, and the next milestone contract must be written
into `docs/dev-plan.md` with exactly one active milestone.

## What this document does not claim

- It does not close A3 or replace `docs/dev-plan.md`.
- It does not claim the Runner/Machine/Platform boundaries are integrated, or that
  the two run loops are merged.
- It does not claim device, MMU, multi-hart, interrupt, ACT4 or external-suite
  support.
- It does not treat component tests, old logs or workflow definitions as
  successful runs.
- It does not claim the recommended successor is approved or scheduled.
