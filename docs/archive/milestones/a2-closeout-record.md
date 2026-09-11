# A2 — Reliable Flat-Library ELF Execution and Inspection Closeout

**Status:** Historical — completed milestone

**Authority:** Informational completion record; not an implementation contract

**Completion / successor approval date:** 2026-09-11

**Assessed revisions:** T1 `2769f56`, T2 `e0ad036`, T3 `60bd36e`, T4 `4572df8`

## Outcome

The maintainer accepted A2 with its documented limitations and approved **A3 —
one shared image-placement and result-construction path behind both public entry
points**. [The A3 contract](../../dev-plan.md) replaces A2 as the only active
milestone. A2 delivered a dependable flat-library workflow; it did not implement
the target Runner/Machine/Platform architecture and did not claim device, MMU,
multi-hart, interrupt or external-compliance support.

The [original A2 contract](a2-reliable-flat-library-workflow.md) and the
[full capability assessment](a2-capability-assessment.md) are preserved. Their
"active" and "not yet verified" statements describe the earlier state.

## Acceptance criteria dispositions

| A2 criterion | Final disposition and evidence |
| --- | --- |
| Load/run/result/inspect works together | Accepted. `integrated_load_run_result_inspect_workflow` drives one nonzero-base image through the whole path with exact entry, exit `42`, 16 cycles, final PC, registers, RAM byte and artifact assertions; zero/nonzero exits, base zero and a nonzero entry offset are covered by `flat_library_reports_zero_and_nonzero_guest_exits`, `flat_library_reports_base_zero_placement` and `flat_library_executes_from_a_nonzero_entry_offset`. |
| Control is bounded and truthful | Accepted. `flat_library_bounds_zero_budget_and_final_slot_exits` and `flat_library_distinguishes_guest_exit_timeout_and_execution_error` assert distinct outcomes and correct successful-step accounting; the A1 G-01 contrast became the correct-exit regression `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal`. |
| Inspection is safe | Accepted. `out_of_range_flat_read_mem_returns_error_without_hanging` returns an error inside the bounded child harness, with handshake-failure and early-exit cleanup retained; exact alignment, size, boundary, overflow and empty assertions hold, and inspection leaves PC, registers, privilege and RAM unchanged. |
| Artifacts and subsequent loads are coherent | Accepted. Guest-written signatures at a nonzero base return the guest's bytes with the original metadata address; absent, empty and unreadable regions are distinguishable; a primary timeout or execution error survives an artifact diagnostic; a second image replaces RAM and metadata without leaking the first image's exit or artifact; the documented manual flat tohost precedence is tested in both directions. |
| Compatibility and evidence are current | Accepted. Public signatures, the CLI `load_and_run` path and flat `read_mem`/`write_mem`/`set_tohost` address meanings are unchanged. The full gate and strict rustdoc were recorded per change, and every merge revision ran the guest suite on `main`. |
| Capability acceptance, not task counting | Accepted. The [capability assessment](a2-capability-assessment.md) maps every criterion and the required observable behaviour to committed tests, records the retained CLI differences, the limitations and the independent reviews, and proposes one successor. |

## Recorded verification and review

Local quality gate per change (`cargo fmt --all -- --check`,
`cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`,
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`): each pull
request records the exact commands it ran. At the T4 revision the suite is 33
test binaries and 1519 tests with `public_behavior` at 40.

| Change | Pull request | Pull-request CI | Merge-time `main` CI |
| --- | --- | --- | --- |
| T1 | [#18](https://github.com/mimiqdev/ruscv-sim/pull/18) | [34317298202](https://github.com/mimiqdev/ruscv-sim/actions/runs/34317298202) | [34563091440](https://github.com/mimiqdev/ruscv-sim/actions/runs/34563091440) |
| T2 | [#19](https://github.com/mimiqdev/ruscv-sim/pull/19) | [34563726811](https://github.com/mimiqdev/ruscv-sim/actions/runs/34563726811), [34564985487](https://github.com/mimiqdev/ruscv-sim/actions/runs/34564985487) | [34565495232](https://github.com/mimiqdev/ruscv-sim/actions/runs/34565495232) |
| T3 | [#20](https://github.com/mimiqdev/ruscv-sim/pull/20) | [34565905461](https://github.com/mimiqdev/ruscv-sim/actions/runs/34565905461), [34566390018](https://github.com/mimiqdev/ruscv-sim/actions/runs/34566390018) | [34568407238](https://github.com/mimiqdev/ruscv-sim/actions/runs/34568407238) |
| T4 | [#21](https://github.com/mimiqdev/ruscv-sim/pull/21) | [34568682493](https://github.com/mimiqdev/ruscv-sim/actions/runs/34568682493), [34571107641](https://github.com/mimiqdev/ruscv-sim/actions/runs/34571107641) | [34572206745](https://github.com/mimiqdev/ruscv-sim/actions/runs/34572206745) |

Each merge-time run compiled the guest programs and executed the
project-authored ELF runner at **46 total / 46 passed / 0 failed**, and built and
smoke-tested the release binary. Pull-request runs skip those steps by workflow
design; the development host has no RISC-V toolchain and no Docker, so local
guest execution is not claimed anywhere in this milestone.

Independent read-only review targeted each committed head, in a separate agent
context, with the coding worktree unmodified: one round for T1; three for T2
(which found misaligned declared-tohost acceptance and partial mutation on a
rejected load, both repaired with regressions, and registered a pre-existing
panic as G-11 instead of fixing it silently); two for T3 (including verification
that a documentation-only follow-up was documentation-only and that every
corrected claim matched the code); two for T4 (which found a contract-named
scenario missing from the flat library, repaired by adding
`flat_library_executes_from_a_nonzero_entry_offset`). Review findings were
addressed on the same branch and re-reviewed against the new head.

## Accepted limitations and re-evaluated unfinished work

Repaired by A2: **G-01**, **G-02**, **G-10**, **G-12**, and the flat-library half
of **G-06**.

Retained, not scheduled by this closeout:

- **G-03/G-09** public commit-log opcodes at nonzero bases and missing
  memory-access suffixes.
- **G-04** UART routing window beyond the declared range.
- **G-05** verbose tohost diagnostic inaccuracy.
- **G-06 CLI half** signature read errors remain suppressed in `load_and_run`.
  A2's non-goals excluded redesigning the CLI policy, and this closeout does not
  schedule it.
- **G-07** host toolchain requirement; guest verification is supplied by
  `main` push CI, not by host installation.
- **G-08** stale bare-metal README exit-code text.
- **G-11** a manual `set_tohost` near the top of the address space still panics
  the exit poll through flat-memory bound arithmetic. Reproduced, pre-existing,
  unrepairable inside A2's scope.

Unverified boundaries carried as limitations rather than commitments:
default-limit exhaustion, the `tohost` symbol fallback path, 32-bit host
behaviour, poisoned-lock and blocking-backend termination guarantees, and
`write_mem` transactional semantics.

Not implemented and not claimed: Runner/Machine/Platform composition, precise
Hart outcome/observation boundaries, devices inside the flat wrapper, MMU/PMP,
multi-hart, interrupts, SystemC/TLM integration, new ISA support, and ACT4 or any
external architecture-suite compliance. The
[unapproved full-migration candidate](a0-full-migration-candidate.md) remains
historical input.

## Re-evaluated successor

The [capability assessment](a2-capability-assessment.md#successor-recommendation)
recommended one bounded consolidation of image placement and result construction,
because three of A2's four repairs (G-01, G-10, G-12) were defects in the flat
wrapper's own placement, metadata and result paths, each repaired with
wrapper-specific logic that mirrors the CLI instead of sharing it. G-02 is the
loosest member of that pattern; the rejected alternative batch of individual gap
repairs is mostly CLI-side work, so its relationship is thematic rather than
causal. The maintainer approved that recommendation as
[the A3 contract](../../dev-plan.md). Selection of any *further* successor,
including the larger migration, remains a separate decision.
