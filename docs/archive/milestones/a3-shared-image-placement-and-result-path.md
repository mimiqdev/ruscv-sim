# Archive context

**Status:** Historical

**Authority:** Informational; not the active milestone contract

**Completion / archival date:** 2026-09-11

The maintainer accepted A3 with its documented limitations and approved A4, one
host-side path for the run-control decision and image installation. The
[A3 closeout record](a3-closeout-record.md) holds the criterion dispositions,
verification and limitations, and the
[A3 capability assessment](a3-capability-assessment.md) holds the full mapping.
[The active plan](../../dev-plan.md) is A4. The full contract text below is
preserved with relocated links; its Active and pending-acceptance statements
describe the earlier state.

---

# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A3 — Shared Image Placement and Result Path

**Status:** Active — T1–T3 delivered; capability acceptance pending

**Authority:** Normative milestone contract

**Approved / started:** 2026-09-11

This is the only current milestone contract. [A2 is completed](a2-closeout-record.md)
with documented limitations and its full [capability assessment](a2-capability-assessment.md).
Milestone identifiers are not releases.

## Objective

The CLI `load_and_run` and the `RiscVSimulator` facade resolve an image's declared
metadata and build their `ExecutionResult` through one internal path, so the two
public entry points cannot drift on placement, exit retention or artifact
reporting, while their distinct configurations and their distinct externally
visible policies stay explicit.

The observed defect shape is concrete: three of A2's four repairs (G-01 tohost
placement, G-10 exit retention, G-12 signature placement) were divergences
between the wrapper's own placement/result logic and the CLI's, each repaired
with wrapper-specific code that mirrors the CLI rather than sharing it. This
milestone removes the duplication that kept producing them. It is not the
Runner/Machine/Platform migration and does not claim those boundaries
integrated.

## Starting evidence and boundary

- [A2 closeout](a2-closeout-record.md) and its
  [capability assessment](a2-capability-assessment.md) record
  the four repairs, the retained CLI-versus-library differences and the
  limitations carried forward.
- [`load_and_run`](../../../src/executor.rs) and [`RiscVSimulator`](../../../src/executor.rs)
  each own their placement arithmetic and their result construction today.
- The [matrix](../../verification/public-behavior-matrix.md) CLI-versus-library row and
  the assessment's retained-differences table are the compatibility baseline;
  the [gap register](../../verification/public-behavior-gaps.md) records what remains
  unrepaired.
- [ADR-0003 §9](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
  keeps both public entry points available and requires the CLI and flat-library
  configurations to stay explicit. One run-control owner is the accepted target;
  this milestone implements a bounded step of it, not the composition.

The supported configuration stays one Hart, synchronous flat RAM or bus RAM,
little-endian ELF64 fixtures using already exercised instructions, and bounded
allocations and requests.

## Required observable behavior

### One placement path

- A single internal owner maps image-declared metadata (`.tohost`/`tohost`,
  `.signature`) plus a configuration (image base, and whether the caller needs a
  bus address or a flat storage offset) to the address form that configuration
  polls or reads, using checked arithmetic.
- Base zero, nonzero base, a nonzero entry offset, a below-base address, an
  address-space overflow and a range that leaves the image memory are handled in
  that one place. Per-use requirements such as the exit poll's eight-byte
  alignment stay explicit at the calling site rather than becoming a hidden
  property of the shared conversion.
- Both entry points obtain their polling and artifact addresses from it. No
  second implementation of the same arithmetic remains.

### One result path

- A single internal owner builds `ExecutionResult` from the observed exit, the
  completed cycle count, the final PC, the signature artifact and any primary
  failure.
- A decoded guest exit is retained before any RAM signal is cleared, and no path
  re-reads a signal it has already cleared.
- The artifact failure policy is an explicit input to the shared path: the CLI
  keeps its current suppression behavior and the flat library keeps its explicit
  diagnostic. Changing the CLI policy is not part of this milestone.
- Zero budget, an explicit or configured limit, an exit in the final permitted
  slot, a timeout and an instruction error keep their current distinct shapes and
  successful-step accounting in both configurations.

### Preserved compatibility

- Public signatures, CLI options, `read_mem`/`write_mem`/`set_tohost` flat
  address meanings, exit encodings, device behavior and the documented manual
  tohost precedence are unchanged.
- The retained CLI-versus-library differences recorded in the A2 assessment stay
  as documented, except where this contract explicitly changes behavior; none of
  them change here.

## Task decomposition and PR cadence

These are delivery tasks under one milestone, not additional milestones. They may
be split or combined into reviewable PRs without changing the acceptance goal.

| Task | Contribution | Dependency / evidence |
| --- | --- | --- |
| T1 — Shared placement resolution | One checked mapping from image metadata to the configuration's address form, used by both entry points; per-use alignment stays explicit. | The A2 placement tests must pass unchanged, plus focused equivalence and boundary tests. |
| T2 — Shared result construction | One builder for exit, cycles, PC, artifact and primary failure, with the artifact policy passed in explicitly and the exit retained before clearing. | The A2 exit, limit, artifact and replacement tests must pass unchanged, plus policy tests for both configurations. |
| T3 — Integrated equivalence and documentation | Prove through committed tests that both entry points keep their documented behavior on the shared path, and record what changed. | Depends on T1–T2; exact-head review, full gate and merge-time guest evidence. |

Preserve before changing: reproduce any behavior difference before adjusting it,
and keep new discoveries explicit in the gap register instead of silently adding
them to this milestone.

## Architectural constraints

The [accepted ADRs](../../architecture/decisions/README.md), particularly
[ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md),
and [principles](../../architecture/principles.md) remain authoritative:

- One Hart, one ISA engine. This milestone shares host-side helpers; it does not
  add a second execution loop or change instruction semantics.
- Host image handling and inspection stay separate from Hart-initiated physical
  transactions. Storage adaptation is not guest virtual-to-physical translation.
- Public entry points keep their names and behavior; prefer private helpers. Any
  new public item needs a stated reason and rustdoc.
- The milestone does not consolidate the two run loops and does not claim
  Machine/Platform boundaries integrated.

## Non-goals and retained gaps

- Runner/Machine/Platform composition, new ports or boundaries, precise Hart
  outcomes/observations, scheduler or interrupt integration.
- Devices in the flat wrapper, MMU/PMP/paging, multi-hart, SystemC/TLM, new ISA
  support, acceleration, and ACT4 or any external architecture-suite compliance.
- Repairing G-03, G-04, G-05, the CLI half of G-06, G-07, G-08, G-09 or G-11; changing CLI
  options or the CLI signature-failure policy; `write_mem` transactional
  semantics; arbitrary allocation hardening; poisoned-lock or blocking-backend
  termination guarantees.
- Performance work and benchmark targets.

These exclusions bound the milestone; they are not extra future milestones.

## Deliverables and acceptance criteria

Deliver one shared placement path and one shared result path, focused
regressions, integrated equivalence evidence and updated records. Completion
requires all of the following:

1. **One placement owner.** Both public entry points resolve image-declared
   metadata through the same internal path; no duplicated guest-to-configuration
   arithmetic remains. Tests cover base zero, nonzero base, a nonzero entry
   offset, below-base, beyond-image, overflow and the per-use alignment rule.
2. **One result owner.** Exit retention, timeout/error shapes and artifact
   composition are constructed in one place, with the artifact policy explicit
   per configuration; no path re-reads a cleared signal. Zero budget, exhaustion
   and final-slot exits keep their current assertions in both configurations.
3. **Behavior preserved.** Every A1 and A2 CLI and flat-library test passes
   unchanged; none is deleted or weakened. The documented retained differences
   remain accurate.
4. **Evidence is current.** Full quality gate, strict rustdoc, public CLI/ELF
   regressions and the merge-time guest compilation/execution run are recorded
   for the reviewed implementation or verified merge. Matrix, gap register and
   current-state documents identify precisely what changed and what remains
   unverified.
5. **Capability acceptance, not task counting.** A final assessment maps these
   scenarios to committed tests, records limitations and independent review, and
   proposes at most one successor from remaining architectural needs. T1 alone,
   or merely deleting duplicated lines, cannot close A3.

The quality gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, plus
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
Use the [development environment](../../development-environment.md) and
[guest commands](../../verification/bare-metal-tests.md). Record unavailable tools and
verification gaps explicitly; unmet capability criteria cannot be silently
reclassified as optional tasks.

## Closeout

Keep this as the only active contract. After capability acceptance and required
review/merge evidence, archive completion, limitations and revision identifiers;
re-evaluate unfinished work and select one separately approved successor.
Task/PR granularity does not determine milestone granularity.
