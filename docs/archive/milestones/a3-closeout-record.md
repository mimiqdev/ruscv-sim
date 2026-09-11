# A3 — Shared Image Placement and Result Path Closeout

**Status:** Historical — completed milestone

**Authority:** Informational completion record; not an implementation contract

**Completion / successor approval date:** 2026-09-11

**Assessed revisions:** T1 `c67cb19`, T2 `ca78cd0`, T3 `1577437`

## Outcome

The maintainer accepted A3 with its documented limitations and approved **A4 —
one host-side path for the run-control decision and image installation**.
[The A4 contract](../../dev-plan.md) replaces A3 as the only active milestone. A3
removed the two duplicated host-side responsibilities that had produced A2's
repeated defects; it did not merge the two run loops and did not implement the
target Runner/Machine/Platform composition.

The [original A3 contract](a3-shared-image-placement-and-result-path.md) and the
[full capability assessment](a3-capability-assessment.md) are preserved. Their
"Active" and "pending acceptance" statements describe the earlier state.

## Acceptance criteria dispositions

| A3 criterion | Final disposition and evidence |
| --- | --- |
| One placement owner | Accepted. `AddressForm` and `ImagePlacement` resolve image-declared metadata in each configuration's address form; both entry points use them and the wrapper's private conversion is deleted. Five unit tests cover both forms, base zero, below-base, beyond-image, address-space overflow, absent metadata and the signature range boundaries. |
| One result owner | Accepted. `ResultInputs` assembles the exit, cycles, final PC, artifact and primary failure, with `ArtifactOutcome` describing the read and `ArtifactPolicy` an explicit input. Five unit tests cover facts assembly, both policies on the same failed read, primary-failure preservation, absent and empty artifacts, and the caller-supplied timeout shape. Exactly one `ExecutionResult` construction site remains. |
| Behavior preserved | Accepted. No existing test was modified in either change (`git diff de5f6c6..ca78cd0 -- tests/` alters none); `public_behavior` grew from 40 to 44 with T3's equivalence tests only. Independent reviews ran differential harnesses against both revisions — 45 scenarios for T1 and a 36-record harness for T2 — and observed byte-identical output. |
| Evidence is current | Accepted. Every merge revision ran the complete `main` pipeline with the project-authored ELF runner at **46 total / 46 passed / 0 failed**. Matrix, current-state and gap register were updated with each change; T3 added committed cross-entry-point equivalence tests. |
| Capability acceptance, not task counting | Accepted. The [capability assessment](a3-capability-assessment.md) maps every criterion and the retained differences to committed tests, records the limitations, and proposes one successor. |

## Recorded verification and review

Local quality gate per change (`cargo fmt --all -- --check`,
`cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`,
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`): each pull
request records the exact commands it ran. At the T3 revision the suite is 33
test binaries and 1533 tests, with `public_behavior` at 44.

| Change | Pull request | Merge-time `main` CI |
| --- | --- | --- |
| T1 | [#23](https://github.com/mimiqdev/ruscv-sim/pull/23) | [34581646250](https://github.com/mimiqdev/ruscv-sim/actions/runs/34581646250) |
| T2 | [#24](https://github.com/mimiqdev/ruscv-sim/pull/24) | [34583623314](https://github.com/mimiqdev/ruscv-sim/actions/runs/34583623314) |
| T3 | [#25](https://github.com/mimiqdev/ruscv-sim/pull/25) | [34587569987](https://github.com/mimiqdev/ruscv-sim/actions/runs/34587569987) |

Each merge-time run built and smoke-tested the release binary, compiled the guest
programs and executed the project-authored ELF runner at **46 total / 46 passed /
0 failed**. Pull-request runs skip those steps by workflow design, and the
development host has no RISC-V toolchain or Docker, so local guest execution is
not claimed.

Independent read-only review targeted each committed head in a separate context,
with the coding worktree unmodified: three rounds for T1 (approve after a
45-scenario differential; findings on the documented poll invariant, the G-11
CLI surface and the quoting of a probe address, all addressed), two for T2
(approve after a 36-record differential; findings on a spliced doc comment and a
superseded sentence, both addressed), and three for T3 (approve after mutation
testing showed each new equivalence test fails when the shared path is broken;
findings on inherited limitations, an overstated duplication claim, a record
sentence, an unasserted exit code and a missing ADR link, all addressed). One
residual nit is recorded in PR #25: the record-update sentence does not list
`docs/dev-plan.md` among the documents T3 touched.

## Accepted limitations and re-evaluated unfinished work

Repaired by A3: no gap-register entry. A3 removed a cause of A2's defects rather
than repairing a further instance, and the gap register records that explicitly
for G-01, G-10 and G-12.

Retained, not scheduled by this closeout:

- **The two run loops remain separate**: stepping, budget accounting,
  exit-detection order and signal clearing are still implemented twice. This is
  the successor's subject.
- **Image installation is duplicated**: `load_and_run` and `RiscVSimulator::load_elf`
  each build their own RAM, load the program, construct a core and reset it.
- **A2's retained gaps** — G-03, G-04, G-05, the CLI half of G-06, G-07, G-08,
  G-09 — and **G-11**, whose surface now includes the CLI `--tohost` path.
- **Inherited unverified boundaries**: the 10,000,000-cycle default is never
  exercised to exhaustion, and only the `.tohost` section path has committed
  fixtures, so the `tohost` symbol fallback stays unverified.
- **Retained configuration differences** recorded in the assessment: explicit
  tohost overrides use a bus address in the CLI and a flat offset in the library,
  device MMIO exists only on the CLI, the two configurations report an unreadable
  artifact differently by documented policy, and their instruction-error messages
  differ in wording.

Not implemented and not claimed: Runner/Machine/Platform composition, precise Hart
outcome/observation boundaries, devices inside the flat wrapper, MMU/PMP,
multi-hart, interrupts, SystemC/TLM, new ISA support, and ACT4 or any external
architecture-suite compliance. The
[unapproved full-migration candidate](a0-full-migration-candidate.md) remains
historical input.

## Re-evaluated successor

The [capability assessment](a3-capability-assessment.md#successor-recommendation)
recommended one bounded run-control path, because the run loop is the largest
remaining duplication of the kind that produced A2's defects and one loop already
drifted (G-10, where only the wrapper cleared the signal after retaining the
exit). It named image installation as the other duplicated host-side
responsibility and left whether to include it to the maintainer. The maintainer
approved **both** as the bounded successor. Selection of any *further* successor,
including the larger migration, remains a separate decision.
