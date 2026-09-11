# A4 — Shared Run Control and Image Installation Closeout

**Status:** Historical implementation record; formal closeout proposed

**Authority:** Informational completion evidence; not an implementation contract

**Implementation completion / record preparation date:** 2026-09-11

**Final assessed merge:** `e47a1b0b2c8de25ad3dae17a1b9f61d7c42207d5`

## Outcome and approval boundary

A4's implementation and required evidence are complete. This record proposes
formal acceptance with the limitations below and the detailed
[A5 ACT4 RV64I contract](../../dev-plan.md) for maintainer approval through
the closeout PR merge. The successor direction is approved; neither that detailed
contract nor the closeout PR is represented as already approved or merged.

The [original contract](a4-shared-run-control-and-image-installation.md) and
[full assessment](a4-capability-assessment.md) are preserved with archive context.
Their pre-merge Active/pending statements are historical, not current blockers.
A4 shares host decisions and installation across two public loops; it does not
merge those loops or implement Runner/Machine/Platform composition.

## Acceptance criteria dispositions

All five criteria have sufficient evidence for acceptance, subject to the formal
closeout decision above.

| Criterion | Final disposition and evidence |
| --- | --- |
| One run-control decision | Satisfied. [`RunControl::start` / `after_step`](../../../src/executor.rs#L638) own continue/exit/timeout/error and retirement accounting, ordered lazy observation and shared RAM retention-before-clear. The five `test_run_decision_*` and retained `test_run_control_*` tests cover zero budget, exhaustion, failed-step non-retirement/non-observation, final-slot priority and both RAM encodings; [`a4_run_control`](../../../tests/a4_run_control.rs#L9) adds failed-first-step signal retention in both APIs and CLI HTIF priority over an unmapped lower-priority RAM observer. Initial PR #27 was insufficient alone; PR #29 completed ownership. |
| One installation path | Satisfied. [`install_image`](../../../src/executor.rs#L832) creates RAM, loads bytes, constructs the core over the caller's backend and resets it. Three owner tests cover loaded RAM and both address forms. [`a4_integrated_equivalence`](../../../tests/a4_integrated_equivalence.rs#L38) combines nonzero entry offsets with guest seed load/store/reload, artifacts and reload-derived exit; replacement restores RAM/BSS/registers/entry and metadata before agreeing with a fresh CLI installation. CLI bus devices and bare flat RAM remain distinct. |
| Behavior preserved | Satisfied. Existing A1–A3 tests and fixtures were not modified or weakened: comparing `3f30cdb..e47a1b0` adds two test files; existing integration files are unchanged. T3 itself changes no production code. The exact-head gate passes `public_behavior` 44, `executor` 58, `cli_test` 15 and both two-test A4 suites. The assessment maps every retained configuration difference. PR #29 had no runtime differential; preservation evidence is source comparison and regressions, not an invented differential. |
| Evidence current | Satisfied. The six-command gate including strict rustdoc passed at reviewed T3 head; PR and final merge CI succeeded. Final push CI separately compiled and executed **46/46** project-authored guests and release/smoke. Current-state, matrix and gap records now point to final evidence without claiming local guest execution or external ACT4 verification. |
| Capability acceptance | Satisfied for formal disposition. The assessment maps every criterion to source and committed tests, retains limitations and reviews, and recommends exactly one successor. Same-ELF expectations are independently asserted for each API, not merely equality assertions. All budget/error/data/replacement scenarios are committed and independently reviewed. |

## Recorded verification and independent review

| Contribution / merge | Review and CI evidence |
| --- | --- |
| Initial T1 `f501593`, [PR #27](https://github.com/mimiqdev/ruscv-sim/pull/27) | [Main CI 34601046791](https://github.com/mimiqdev/ruscv-sim/actions/runs/34601046791), successful with separate guest compilation/execution 46/46. Original record establishes accounting and retention, not complete decision ownership; no additional exact review head or runtime differential is inferred. |
| T2 `dd45c86`, [PR #28](https://github.com/mimiqdev/ruscv-sim/pull/28) | Independent review recorded no blockers; [main CI 34602623737](https://github.com/mimiqdev/ruscv-sim/actions/runs/34602623737), successful with separate guest compilation/execution 46/46. |
| Decision completion `b461744`, [PR #29](https://github.com/mimiqdev/ruscv-sim/pull/29) | Exact reviewed head `b788114` approved, no defects; full local gate passed. Source comparison and regressions only, **no base/head runtime differential**. [Main CI 34605659612](https://github.com/mimiqdev/ruscv-sim/actions/runs/34605659612), successful with separate guest compilation/execution 46/46. |
| T3 `e47a1b0`, [PR #30](https://github.com/mimiqdev/ruscv-sim/pull/30) | Exact head `810bf83968226e5390b15b6c73890382bbc895e3` independently approved, no defects. Full six-command gate passed; **no mutation checks or runtime differential**. [PR CI 34608196803](https://github.com/mimiqdev/ruscv-sim/actions/runs/34608196803) and [push CI 34608867200](https://github.com/mimiqdev/ruscv-sim/actions/runs/34608867200) succeeded. Push logs directly verified release/smoke and **46 total / 46 passed / 0 failed** separately compiled guests. |

T3's six commands were `cargo fmt --all -- --check`,
`cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, and
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
The assessment retains the original local counts (830 library unit tests,
all integration suites and 27 doctests), focused 2/2 T3 result, tool versions
and superseded interrupted/formatting attempts.

The independent T3 reviewer also verified older CI records and the pinned ACT4
README; the assessment's earlier GitHub rate-limit limitation no longer leaves
those records unchecked. Review evidence is tied to the implementation head,
not a formal review of this uncommitted documentation change.

The development host lacked the RISC-V cross-toolchain and Docker.
`test_add_program` returned early for a missing assembler; Cargo calls that a
pass, not an ignored test. No local separate guest suite or ACT4 generation/run
is claimed. PR CI skips guest compilation/execution by
[workflow design](../../../.github/workflows/ci.yml#L63); only recorded push
evidence supplies it here. This docs-only closeout does not claim a new Rust run.

## Limitations and re-evaluated unfinished work

- One Hart and synchronous flat/bus RAM with bounded little-endian ELF64 fixtures
  using exercised instructions only. No extension-wide ISA claim.
- Public loops retain stepping, configuration-specific signal sources/address
  forms, diagnostics and artifact policy. `RiscvCore::run` remains outside the
  shared public-entry decision. CLI HTIF precedes selected RAM; the facade
  observes only selected flat RAM.
- CLI devices, override address meanings, unreadable-artifact policies and
  instruction-error wording retain their documented differences.
- No precise Hart outcomes/observations, Machine/Platform composition, scheduler,
  interrupts, MMU/PMP, multi-Hart, SystemC/TLM or wrapper device integration.
- Default-limit exhaustion and `tohost` symbol fallback remain unverified.
  No new guarantee for extreme addresses, allocation, transactional writes,
  poisoned locks or blocking-backend termination.
- [G-03/G-04/G-05/CLI G-06/G-07/G-08/G-09/G-11](../../verification/public-behavior-gaps.md)
  retain their dispositions. A4 repaired no new public gap-register defect;
  it removed duplicated host decision/installation ownership and closed the
  initial T1 internal contract gap.
- No external-suite result or certification. Base-I FENCE remains rejected
  through `Opcode::MiscMem`; FENCE.I is Zifencei. Tool pairing is not verified
  merely because ACT4's README and revision have been checked.

## One re-evaluated successor

Recommend **A5 — ACT4 RV64I External Compatibility Baseline** rather than another
internal extraction. A4 addressed the concrete duplication; externally authored
selected public-path ISA evidence is still absent. A5 follows the current
A-series naming, not release numbering or the superseded M8 task list.

The proposed contract makes pinned tool feasibility an early gate, uses upstream
self-checking generation and a thin public CLI harness, and requires the entire
explicit non-trapping RV64I selection rather than smoke-only success. Failing
required base cases cannot be filtered away. Exclusions expose unsupported
architectural boundaries without scheduling traps/platform migration. Detailed
selection and an experimentally working ACT4/Sail/compiler pairing must be
reviewed after feasibility; this closeout does not invent their counts or pins.
No unrelated gap or further successor is automatically carried forward.
