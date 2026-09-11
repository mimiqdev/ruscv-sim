# Archive context

**Status:** Historical

**Authority:** Informational; the capability acceptance assessment as of A2 closeout

**Completion / archival date:** 2026-09-11

This is the full A2 capability assessment as accepted. Current navigation lives
in [the verification index](../../verification/a2-capability-assessment.md) and
the only active plan is [A3](../../dev-plan.md).

---

# A2 Capability Assessment

**Status:** Current

**Authority:** Informational; not the milestone contract, not an acceptance decision and not a milestone closeout

**Assessed revisions:** T1 `2769f56`, T2 `e0ad036`, T3 `60bd36e`. The T4 change under
review is the head of its own pull request; its merge revision belongs in the
closeout record.

**Last reviewed:** 2026-09-11

## Purpose

[A2](../../dev-plan.md) defines one bounded capability: a library caller can load a
supported RAM-only ELF, run it under a finite budget, receive the actual guest
exit or a distinguishable timeout/execution error, and inspect the resulting
registers, RAM and declared signature without hanging or confusing ELF addresses
with flat storage offsets.

This document maps the contract's acceptance criteria to committed tests and
recorded runs, states the limitations it does not claim, and proposes one
successor. It does not close A2: `docs/dev-plan.md` remains the only active
contract until the maintainer accepts completion.

## Delivered capability

| Task | Change | Revision |
| --- | --- | --- |
| T1 | Bounded flat inspection: `read_mem` returns exact bytes or an explicit error instead of retrying without progress (G-02). | `2769f56` |
| T2 | Image-derived tohost adapted to a flat offset with checked arithmetic; declared placements that cannot be observed are rejected at load; the decoded exit is retained before the RAM signal is cleared (G-01, G-10). | `e0ad036` |
| T3 | Signature artifacts are read from the corresponding flat bytes, artifact failures are reported explicitly, and a second image load replaces RAM and metadata without leaking the first image's exit or artifact (G-12, flat half of G-06). | `60bd36e` |
| T4 | Integrated acceptance and this assessment. | PR head |

## Acceptance criteria mapping

### 1. Load, run, result and inspection work together

| Scenario | Committed test | Observed |
| --- | --- | --- |
| Nonzero base, declared tohost, guest work, signature and a nonzero exit | `integrated_load_run_result_inspect_workflow` | entry `0x8000_0000`, exit `42`, 16 cycles, final PC `0x8000_0040`, `x6=0x8000_0100`, `x7=69`, RAM byte `0x45`, signature `[0x5a, 17, 34, 51, 68, 85, 102, 119]` |
| Zero and nonzero guest exits | `flat_library_reports_zero_and_nonzero_guest_exits` | codes `0`, `1`, `42` with the writing instruction's cycle and PC |
| Base zero | `flat_library_reports_base_zero_placement` | raw offsets, exit `7`, 4 cycles |
| Nonzero entry offset | `flat_library_executes_from_a_nonzero_entry_offset` | entry `0x8000_0100`, exit `1` at cycle 4, final PC `0x8000_0110`, file bytes visible at flat `0x100` |
| Declared tohost selected from the image | `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` | CLI and library both exit at cycle 4 |
| File bytes and BSS preserved by the loader | `elf_segments_preserve_file_bytes_and_zero_fill`, `elf_loader_clears_bss_in_prefilled_memory`, `public_zero_fill_is_observed_by_guest_execution` | committed A1 evidence, unchanged |

### 2. Control is bounded and truthful

| Scenario | Committed test | Observed |
| --- | --- | --- |
| Zero budget executes nothing | `flat_library_bounds_zero_budget_and_final_slot_exits` | `cycles=0`, `final_pc` at the entry, timeout, `Timeout after 0 cycles` |
| Explicit exhaustion | same test | `cycles=3` of 3, timeout |
| Exit in the final permitted slot | same test | exit observed, `timed_out=false` |
| Distinct exit, timeout and instruction error | `flat_library_distinguishes_guest_exit_timeout_and_execution_error` | retained exit with no error; timeout text; execution error with `cycles=0` |
| The A1 G-01 contrast became a correct-exit regression | `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` | the A1 reproduction was replaced, not deleted |

### 3. Inspection is safe

| Scenario | Committed test | Observed |
| --- | --- | --- |
| The original G-02 request returns an error | `out_of_range_flat_read_mem_returns_error_without_hanging` | child receives `Err` inside the two-second window; a hang is still killed and reaped |
| Harness cleanup paths | `out_of_range_flat_read_mem_handshake_failure_is_reaped`, `out_of_range_flat_read_mem_early_exit_is_reaped` | ready-timeout against a live child, and early exit, both reaped |
| Alignment, size, boundary, overflow, empty | `flat_read_mem_returns_exact_bytes_for_aligned_unaligned_and_mixed_lengths`, `flat_read_mem_rejects_out_of_range_crossing_and_overflow_without_wrapping`, `flat_read_mem_empty_requests_do_not_access_memory` | exact bytes or explicit errors |
| Inspection leaves state unchanged | `flat_read_mem_does_not_execute_or_mutate_after_guest_step`, `integrated_load_run_result_inspect_workflow` | PC, registers, privilege and RAM unchanged |

### 4. Artifacts and subsequent loads are coherent

| Scenario | Committed test | Observed |
| --- | --- | --- |
| Guest-written artifact at a nonzero base | `flat_library_returns_guest_written_signature_bytes_at_nonzero_base` | guest metadata address `0x8000_2000` with the guest's own byte first |
| Base zero artifact | `flat_library_returns_signature_bytes_at_base_zero` | same bytes with raw offsets |
| Absent, empty and unreadable are distinguishable | `flat_library_distinguishes_absent_empty_and_unreadable_signatures` | `None`/`None`, `Some(addr)`/empty, `Some(addr)`/`None` + diagnostic |
| A primary failure survives an artifact failure | `flat_library_keeps_the_run_when_the_signature_is_unreadable` | exit, cycles, final PC and guest RAM identical; timeout or execution error preserved alongside the diagnostic |
| Image replacement | `flat_library_replaces_image_metadata_and_ram_on_a_second_load` | the second image's RAM and metadata replace the first; no artifact leak; reload restores |
| Declared tohost that cannot be observed | `flat_library_rejects_a_declared_tohost_the_flat_image_cannot_represent` | below-base, beyond-memory, overflowing and misaligned placements fail the load |
| Rejected placement leaves the loaded image intact | `flat_library_rejected_placement_leaves_the_previous_image_runnable` | the previous image still exits and keeps its signature metadata |
| Manual flat tohost remains tested and documented | `flat_library_manual_flat_tohost_overrides_image_metadata`, `flat_library_manual_tohost_before_load_is_superseded_by_image_metadata`, `flat_library_manual_tohost_survives_a_load_without_metadata`, `flat_library_image_without_metadata_does_not_reuse_the_previous_tohost` | the documented precedence holds in both directions |

### 5. Compatibility and evidence are current

The public signatures, the CLI `load_and_run` path, the flat `read_mem`,
`write_mem` and `set_tohost` address meanings, and the CLI device behaviour are
unchanged. The full quality gate, strict rustdoc and the public CLI/ELF
regressions are recorded in the pull requests below; each record names the
commands it actually covers, and this assessment's head carries a fresh local
strict-rustdoc run. Guest program
compilation and execution is verified by the `main` push CI at the merge
revisions: release build, binary smoke test, guest compilation and the
project-authored ELF runner at **46 total / 46 passed / 0 failed** for both
`e0ad036` and `60bd36e`.

### 6. Capability acceptance

This document. No acceptance decision is claimed: the successor below requires
separate approval, and the closeout record belongs to the maintainer.

## Retained differences from the CLI

| Aspect | CLI `load_and_run` | Flat `RiscVSimulator` | Evidence |
| --- | --- | --- | --- |
| Image placement | `SystemBus` maps RAM at the ELF base; core reset with base `0` | flat `SimpleMemory` holding the image relative to its base; core reset with the image base | matrix CLI-versus-library row |
| Declared RAM tohost | polled at the guest address through the bus | converted to a checked flat offset; unobservable placements rejected at load | `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` |
| Manual tohost configuration | `--tohost` is a guest/physical address | `set_tohost` is a flat storage offset | documented on `RiscVSimulator` |
| Devices | UART at `0x1000_0000` and the fixed HTIF endpoint at `0x4000_8000` | no device mapping; device MMIO fails as an execution error | `integrated_workflow_records_the_retained_cli_device_difference` |
| Signature failure policy | read errors still suppressed by `.ok().flatten()` ([G-06](../../verification/public-behavior-gaps.md#g-06--signature-read-errors-are-suppressed-in-run-results)) | explicit diagnostic through the existing error surface | A2 T3 tests |
| Shared | declared tohost discovery, exit decoding, zero-budget and final-slot semantics, signature metadata address meaning | | A2 T2/T3 tests |

## Limitations and unverified areas

- **Guest suite on the development host.** No RISC-V toolchain and no Docker are
  available here, so the guest programs are built and run by `main` push CI, not
  locally. Pull requests intentionally skip those steps.
- **Default-limit exhaustion.** The 10,000,000-cycle default is not exercised to
  exhaustion.
- **tohost symbol fallback.** Only the `.tohost` section path has committed
  fixtures; the symbol path remains unverified.
- **G-11.** A manual `set_tohost` near the top of the address space still panics
  the poll through flat-memory bound arithmetic. Pre-existing, reproduced, and
  not repaired.
- **Out of scope by contract.** Device-backed flat execution, MMU/PMP, multi-hart,
  interrupts, Runner/Machine/Platform migration, `write_mem` transactional
  semantics, poisoned-lock and blocking-backend termination guarantees, and
  ISA-wide or external (ACT4) compliance are not claimed.
- **Single configuration.** The capability is verified for one hart, synchronous
  flat RAM, little-endian ELF64 fixtures using already exercised instructions.

## Independent review

Each change was reviewed against its committed head by a separate read-only
review context, with the coding worktree unmodified:

| Change | Pull request | Outcome |
| --- | --- | --- |
| T1 | [#18](https://github.com/mimiqdev/ruscv-sim/pull/18) | No actionable defects; the reviewer reproduced the pre-repair hang and verified the fix. |
| T2 | [#19](https://github.com/mimiqdev/ruscv-sim/pull/19) | Three rounds. The first round independently reproduced both documented defects (G-01 and G-10) at base and their fixes at head; later rounds found misaligned-placement acceptance, partial mutation on a rejected load, and a wording error, all repaired with regressions. A pre-existing panic was registered as G-11 rather than silently fixed. |
| T3 | [#20](https://github.com/mimiqdev/ruscv-sim/pull/20) | A reviewer reproduced `33 passed; 4 failed` at base and `37 passed` at head, then verified the documentation-only follow-up as documentation-only with all corrected claims matching the code. |

Review findings were addressed on the same branch and re-reviewed against the
new head. A review of the T4 change is recorded in its own pull request.

## Successor recommendation

**Recommended: one bounded consolidation of image placement and result
construction.** A2's repairs all had the same shape: the flat wrapper drifted
from the CLI because a second run loop re-implements image placement and result
construction (G-01, G-02, G-10, G-12). [ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
already accepts one run-control owner; the next evidence-shaped slice is to make
both public entry points share one internal path for loading an image, resolving
its declared metadata and building the result, without moving to the full
Runner/Machine/Platform composition. Its acceptance evidence would be that the
existing CLI and flat-library suites, including the A2 workflow test, pass
against the shared path.

**Considered and not recommended now:** another batch of individual gap repairs
(for example G-03, G-05, the CLI half of G-06, G-08). Those are bounded and
useful, and the batch is mostly CLI-side work, so the shape argument against it
is thematic rather than causal. The stronger reason to prefer the consolidation
first is that three of A2's four repairs (G-01, G-10, G-12) were defects in the
flat wrapper's own placement, metadata and result paths, each repaired with
wrapper-specific logic that mirrors the CLI rather than sharing it; G-02 is the
loosest member of that pattern because it is an inspection-loop defect. The
larger full-migration candidate in
[the A0 record](a0-full-migration-candidate.md) remains
unapproved reference material and is a separate, bigger decision than this
recommendation.

Neither option is approved here. Selecting, scheduling and approving a successor
is the maintainer's decision, and the next milestone contract must be written
into `docs/dev-plan.md` with exactly one active milestone.

## What this document does not claim

- It does not close A2 or replace `docs/dev-plan.md`.
- It does not claim device, MMU, multi-hart, interrupt, ACT4 or external-suite
  support.
- It does not treat component tests, old logs or workflow definitions as
  successful runs.
- It does not claim the recommended successor is approved or scheduled.
