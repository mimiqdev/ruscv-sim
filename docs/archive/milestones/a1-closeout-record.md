# A1 — Public Behavior Baseline Closeout

**Status:** Historical — completed milestone

**Authority:** Informational completion record; not a production-repair contract

**Completion / successor approval date:** 2026-09-08

**Assessed runtime/test revision:** `fae4759e09c6750e4105c8be7c287e841dabf0dc`

## Outcome

The maintainer authorized completion of A1 with the explicitly documented
limitations and progression to the bounded G-02 memory-inspection objective.
[The A2 contract](../../dev-plan.md) replaces A1 as the only active milestone.
A1 completed the compatibility inventory, persistent tests/reproductions, and
acceptance/evidence batch. It did not repair the production defects or implement
the target Runner/Machine/Platform architecture.

The [full original A1 contract](a1-public-behavior-baseline.md) and
[pre-closeout acceptance assessment](a1-acceptance-assessment.md) are preserved.
This record supersedes their pending-confirmation language; it does not rewrite
their historical results. The closeout changes themselves follow the repository
review/merge workflow; this record does not claim they have already merged.

## Criterion dispositions

| A1 criterion | Final disposition and evidence |
| --- | --- |
| Scoped surfaces mapped to tests or explicit gaps | Accepted. The [matrix](../../verification/public-behavior-matrix.md) maps CLI/API, ELF, memory, limits, exit/devices/artifacts and distinct flat-library surfaces. Unverified cases remain explicitly limited. |
| Meaningful public assertions, weak assertions replaced/excluded | Accepted. [Executor tests](../../../tests/executor.rs) assert actual zero/finite limits, successful results and logs; [persistent tests](../../../tests/public_behavior.rs) assert bytes, state, output and exact cycle/PC results. Legacy weak HTIF wrappers are excluded from behavioral proof. |
| Defects separated from compatibility | Accepted. [G-01–G-09](../../verification/public-behavior-gaps.md) distinguish reproduced defects, source-only gaps and environment/documentation limitations. A successful defect reproduction is not a compatibility pass. |
| CLI/flat configurations and exercised instruction surface explicit | Accepted. Native `SystemBus` and flat `SimpleMemory` paths are not conflated. The fixture module names its bounded RV64I encodings; neither component tests nor guest passes imply extension compliance. |
| Full gate and guest results, gaps adjudicated | Accepted. Fresh local gate/strict rustdoc and exact-main CI below passed. Coverage skip, incomplete boundary coverage, historical transient reproductions and lack of a high-parallel hang stress run remain disclosed limitations, not waived failures. |
| Coverage/limits summarized and bounded successor proposed | Accepted. The preserved assessment gives detailed dispositions and the G-02-only recommendation. A2 adopts that objective; no other defect is automatically scheduled. |

## Recorded verification and review

- [PR #13](https://github.com/mimiqdev/ruscv-sim/pull/13) merged at
  `f360a97d54bf39a95f6fa4f502b4d7c2929b521b`; inventory and authorized rustdoc-only
  repair, with independent review of `64c6f976` finding no actionable defects.
- [PR #14](https://github.com/mimiqdev/ruscv-sim/pull/14) merged at the assessed
  revision. Independent follow-up review of
  `54bfdbf11491caf7e78565ddf62cc1e7e13c4e67` resolved the four initial findings
  with no new actionable findings. Test/runtime/dependency/workflow contents are
  identical between that reviewed head and the assessed merge.
- [Main CI run 34184525187](https://github.com/mimiqdev/ruscv-sim/actions/runs/34184525187)
  at the assessed merge succeeded: formatting, strict Clippy, Rust tests,
  documentation, release build/smoke, guest compilation and guest execution.
  The guest log reports **46 total / 46 passed / 0 failed**, including UART.
  Coverage was skipped. This supersedes the stacked PR's earlier no-CI gap,
  not its historical observation.
- On 2026-09-08, Rust/Cargo 1.98.0, fresh local `cargo fmt --all -- --check`,
  `cargo check --all-features`, strict all-target Clippy,
  `cargo test --all-features`, ordinary docs and `RUSTDOCFLAGS="-D warnings"`
  docs all exited 0. Constituent results include **809 library**, **57 executor**,
  **17 public-behavior**, and **27 doctests**; these are not additive to another
  full-suite count. Direct add execution printed exit 0 / 53 cycles; use of an
  existing ELF is not evidence of fresh local assembly.
- The independent test review demonstrated BSS mutation sensitivity and verified
  bounded ready/timeout/kill/reap paths. No temporary mutation entered production.
- New closeout-document verification belongs to its own PR, not these historical
  runtime/test results. No release/tag or ACT4 verification is implied.

## Accepted limitations and re-evaluated unfinished work

The detailed residual table in the archived assessment is retained in full.
The closeout accepts those as limits of an evidence milestone, not promises
that the affected behavior works:

- **G-02** remains a production hang and is the only selected A2 repair.
- **G-01** flat ELF/tohost, **G-03/G-09** logging, **G-05** verbose diagnostics
  remain defects. No automatic repair commitment is made.
- **G-04/G-06** UART boundary/signature-error behavior remain source-observed
  with incomplete public-path evidence. **G-08** remains a stale test README.
- **G-07** no longer blocks guest evidence because exact-main CI assembled and
  ran the suite; host toolchain installation is not inferred.
- Default-limit exhaustion, exact error counting, dedicated symbol-only tohost,
  post-return signal clearing, broad ELF/memory/state edges, and high-parallel
  hang stress remain outside verified claims. Known transient evidence was not
  upgraded merely because other persistent tests passed.
- Target architectural migration, precise effects, broader debug/MMU/VP support,
  ISA extension certification and external compliance remain unimplemented or
  unverified as documented. None is part of the new bounded milestone.
