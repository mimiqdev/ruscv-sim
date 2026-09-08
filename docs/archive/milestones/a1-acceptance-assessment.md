# Archive context

**Status:** Historical

**Authority:** Informational; not the active milestone contract

**Completion / archival date:** 2026-09-08

The maintainer authorized A1 closeout with its documented limitations and the
bounded G-02 successor. See the [A1 closeout record](a1-closeout-record.md) for
the final disposition and [the active plan](../../dev-plan.md) for A2. The full
pre-handoff text below is preserved with relocated links; its Active, Draft,
recommendation and awaiting-confirmation statements describe the earlier state.

---

# A1 Acceptance Assessment

**Status:** Draft — technical acceptance recommendation for maintainer confirmation

**Authority:** Informational; assesses the active contract without replacing it

**Assessment date:** 2026-09-08

**Assessed implementation/test revision:** `fae4759e09c6750e4105c8be7c287e841dabf0dc`

## Verdict and boundaries

**Recommend accepting A1's technical deliverables with the explicit limitations
below.** The six criteria in [the active A1 contract](a1-public-behavior-baseline.md) have the
repository evidence listed here. A1 establishes an auditable public-behavior
baseline; it does not require production defects to be fixed, exhaustive behavior
coverage, or an ISA-compliance claim. A disclosed gap is not a passing behavior.

This assessment does not close/archive A1, approve a successor, certify the target
architecture, or authorize production repairs. A1 remains the sole active plan.
Maintainer confirmation of the limitation dispositions and approval of a successor
are still required for the milestone handoff. This new assessment is not itself
an independent review or an already-merged artifact.

## Assessed artifacts and review evidence

- [Compatibility matrix](../../verification/public-behavior-matrix.md): requirements, source/test
  references, assertion strength and verified/defect/unverified dispositions.
- [Gap register](../../verification/public-behavior-gaps.md): G-01 through G-09, with reproduction
  evidence or an explicit source-only/environment/documentation classification.
- [Persistent tests](../../../tests/public_behavior.rs) and
  [minimal ELF builder](../../../tests/common/public_elf.rs): 17 tests using named
  RV64I encodings and explicit CLI/native versus flat-library configurations.
- [Executor tests](../../../tests/executor.rs): 57 tests, including strengthened
  zero-limit, no-logger, file-loading, result, log-shape and signature assertions.
- [PR #13](https://github.com/mimiqdev/ruscv-sim/pull/13) merged at
  `f360a97d54bf39a95f6fa4f502b4d7c2929b521b`: inventory and authorized rustdoc-only
  repairs. Independent review found no actionable defects at `64c6f976`.
- [PR #14](https://github.com/mimiqdev/ruscv-sim/pull/14) merged into `main` at the
  assessed revision. Initial review found four issues: insufficient zero-fill
  evidence, unsafe/ambiguous hang reproduction, revision scoping, and weak HTIF
  tests incorrectly grouped as strong. Follow-up review at
  `54bfdbf11491caf7e78565ddf62cc1e7e13c4e67` found them addressed, with no new
  actionable findings. Review summaries and evidence are recorded in PR history.
- The assessed merge and reviewed `54bfdbf` have identical `src/`, `tests/`,
  `Cargo.toml`, `Cargo.lock` and workflow contents (explicit Git diff checked).
  Post-merge CI below independently covers the merged revision; prior review is
  not relabeled as a review of this assessment document.

## Criterion-by-criterion assessment

| A1 criterion | Evidence | Technical disposition |
| --- | --- | --- |
| 1. Every scoped compatibility surface has an exact test reference or explicit defect/unverified rationale. | Matrix sections cover CLI/API inputs, ELF/loading/addressing, RAM, limits, exit/tohost/UART/signature/logs, and separate flat-library state/memory/configuration. Named tests exist in `tests/public_behavior.rs`, `tests/executor.rs` and the cited component targets. | Satisfied for the scoped inventory; verified labels apply only to named cases. Residual unverified cases are adjudicated below, not promoted to support. |
| 2. Key public tests assert actual state/output/artifacts; weak assertions are replaced or excluded. | `exact_limits_include_zero_and_the_final_exit_slot`, entry/RAM/CLI-exit/UART/signature tests assert specific outputs and results. Executor zero-limit no longer uses a tautological result assertion; logger checks result/line count/PC shape. Legacy weak HTIF wrapper tests are explicitly excluded in the matrix. | Satisfied. Some legacy smoke tests remain, but are not the evidence for strengthened claims. |
| 3. Incorrect behavior is separated from compatibility, with reproduction or explicit limitation. | Persistent G-01, G-02 and G-03/G-09 reproductions; G-05 retains transient observations. G-04/G-06 remain source-observed, G-07 environment-specific and G-08 a documentation discrepancy. | Satisfied with retained defects, not production correctness. The hang harness has ready/timeout separation, kill/reap ownership and failure-path tests. |
| 4. CLI/flat evidence stays distinct and names its exercised profile. | Test module names ADDI, AUIPC, LUI, ORI, LBU, LD, SB, SD and SLLI. Same-ELF comparison explicitly asserts CLI exit versus flat-library timeout; memory-helper bytes are checked separately. Component BSS clearing and public extent tests are separate. | Satisfied for those configurations; neither 17 test passes nor 46 guest passes establish extension-wide support. |
| 5. Full quality gate and guest suite have recorded results, with skips/gaps adjudicated. | Fresh local full gate and strict rustdoc on the assessed source/test revision; exact-main CI run below, including guest compilation and 46/46 execution. | Satisfied. The former stacked-PR no-CI limitation is superseded for the merge by successful main CI. Coverage is skipped, not passed; host toolchain installation is not inferred from cached ELF execution. |
| 6. Coverage/defects/limits are summarized and one bounded successor objective is proposed. | This assessment aggregates the matrix/register, gives explicit limitation dispositions and proposes bounded flat-memory inspection progress below. | Satisfied as an assessment/proposal deliverable. The proposal is not successor approval or implementation authorization. |

## Verification at the assessed revision

### Post-merge CI

[Run 34184525187](https://github.com/mimiqdev/ruscv-sim/actions/runs/34184525187),
`headSha=fae4759e09c6750e4105c8be7c287e841dabf0dc`, completed successfully on
2026-09-08. The check and step conclusions and actual log summaries were inspected,
not inferred from workflow configuration.

| Check/step | Recorded result |
| --- | --- |
| Quality and tests | Success; formatting, strict Clippy, full Rust tests and documentation succeeded. |
| Rust tests | 809 library tests, 57 executor tests, 17 public-behavior tests and 27 doctests passed; other reported integration suites also passed. These are constituent targets, not numbers to add to the full-suite total again. |
| Release binary and smoke | Both succeeded. |
| Guest compilation | All project-authored ELF programs compiled successfully. |
| Guest runner | 46 total, 46 passed, 0 failed; `hello.elf` UART scenario included. |
| Coverage | Skipped. No coverage percentage or coverage-job pass is claimed. |

The PR originally targeted another feature branch and had no applicable PR CI.
After retargeting/merging to `main`, the push workflow executed the guest and
release steps. The earlier no-CI statement remains historical, not current merge
status. This run is not ACT4 or differential-compliance evidence.

### Fresh local acceptance checks

On 2026-09-08, Rust/Cargo 1.98.0, the checkout based on the assessed revision
completed the following checks before any assessment-document edits. Runtime,
test and dependency contents were unchanged.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0. |
| `cargo check --all-features` | Exit 0. |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0. |
| `cargo test --all-features` | Exit 0; all reported suites passed, including 809 library, 57 executor, 17 public-behavior and 27 doctests. |
| `cargo doc --all-features --no-deps` | Exit 0. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Exit 0; no rustdoc warnings. |
| `cargo test --all-features --test test_add_direct -- --nocapture` | Exit 0, guest exit 0 and 53 cycles actually printed; not the test's skip path. An existing ELF may satisfy this test, so this is not proof of fresh host assembly. |

No new local Docker/guest-suite run was needed or claimed: the fresh 46/46 result
above comes from exact-revision main CI. The independently reviewed temporary
BSS mutations remain supporting review evidence at `54bfdbf`: ignoring `p_memsz`
and removing explicit BSS clearing each caused their respective test to fail.
Those mutations were not part of production code and were not repeated during
this acceptance check.

## Explicit residual-gap dispositions

These are proposed acceptance dispositions for the maintainer, not silent waivers
or promises that the affected behaviors work.

| Residual item | Disposition for A1 and consequence |
| --- | --- |
| G-01 flat-library ELF tohost mismatch | Retain as reproduced defect. Public facade exists but is not CLI-equivalent for ELF exit. Production repair was excluded from A1; do not claim equivalence. |
| G-02 out-of-range aligned `read_mem` hang | Retain as reproduced safety defect, not correct inspection behavior. The test safely bounds and reaps the reproduction. Recommend this as the single next repair objective; its importance does not retroactively turn A1 into a repair milestone. |
| G-03/G-09 incorrect nonzero-base opcodes and missing log memory suffixes | Retain as reproduced defects. Logs are not authoritative instruction/memory-effect traces. Trace repair requires separate scope and the accepted Hart-originated observation contract; no Runner-side workaround is approved here. |
| G-04 UART routing-range mismatch | Retain as source-observed, public boundary unverified. Byte TX and width rejection evidence does not decide whether the intended native aperture is 8 or 256 bytes. |
| G-05 verbose tohost diagnostic | Retain as transiently reproduced diagnostic defect; no persistent exact-message regression claimed. It is not evidence that runtime tohost selection is wrong. |
| G-06 signature read-error suppression | Retain as source-observed, failure path unverified. Successful signature bytes do not certify error propagation; no result/API change is authorized. |
| G-07 host toolchain limitation | No longer blocks guest-suite evidence: current main CI assembles/runs 46 programs. Keep host-vs-container/CI provenance explicit; do not claim the local toolchain was installed. |
| G-08 old add-test README exit code | Retain as disclosed documentation discrepancy. Guest source and runner expectations, not the stale text, define the tested exit code. |
| Default-limit exhaustion and exact execution-error count | Explicitly unverified. Default-option early exit and finite/zero/final-slot limits are covered, not the 10-million exhaustion case or complete error-count taxonomy. |
| Symbol-only tohost fallback, post-return RAM signal clearing | Keep dedicated public assertions unverified. Successful section/override/fixed-endpoint paths do not prove these properties. New CI guest files do not by themselves upgrade a previously skipped or incomplete dedicated assertion. |
| ELF overlaps/permissions/malformed variants; memory overflow edges; broad state mutation/reset isolation | Keep outside verified cases. A1's criterion permits named unverified boundaries with rationale; sampled layouts/helper observations are not exhaustive verification. |
| No coverage job, no high-parallel hang stress, transient E2–E5 fixtures | Explicit limits, not passes. A1 does not require a coverage percentage or stress campaign. Relevant persistent tests supersede only the specific transient scenarios they actually assert; other transient observations remain historical evidence. |

No additional production implementation is required to recommend A1 technical
acceptance under its existing contract. If a retained limitation is rejected at
maintainer acceptance, resolve that decision explicitly rather than labeling it
verified or automatically adding a fix to A1.

## One bounded successor proposal — not approved

**Proposed objective: make flat-memory inspection terminate with bytes or an
explicit error for bounded requests (G-02).** The reproduced non-progress paths
in [`RiscVSimulator::read_mem`](../../../src/executor.rs) and the now-safe child test
provide a focused starting point without requiring a runtime migration.

Recommended scope for a separately approved next contract:

- Repair only `read_mem` progress/error handling; preserve its public signature,
  flat-address meaning, successful byte ordering and unrelated run behavior.
- Retain successful in-range reads; cover aligned/unaligned byte/half/word/dword
  and boundary-crossing requests, zero length and address-overflow handling.
  Define error expectations before implementation; do not promise bounded host
  allocation for arbitrary enormous sizes or progress of arbitrary custom devices.
- Convert the known-hang reproduction into a bounded regression expecting an
  error; retain handshake/cleanup tests so a regression cannot hang the suite.
- Verify no returned success contains fabricated/partial data, and record the
  full gate plus applicable guest regression results.

Exclude G-01 and logging/UART/signature fixes, other memory-helper redesign,
public API changes, architecture migration, new ISA/device support and ACT4.
This is selected from the observed safety defect, not carried forward from the
old full-migration candidate. The maintainer must approve the next contract
before any production implementation or A1 archival/replacement.
