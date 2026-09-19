# A7 — Non-Atomic Physical-Access Boundary Migration Closeout

**Status:** Closeout record prepared; bounded A7 acceptance is evidenced. The
closeout delivery change was not merged when this record was written.

**Authority:** Informational historical acceptance and provenance record. The
sole current technical contract remains [`docs/dev-plan.md`](../../dev-plan.md)
until a successor is separately approved and the required rolling transition is
recorded. This record does not create an A8/A9 schedule or replace the current
contract with a forwarding record.

**Closeout date:** 2026-09-19 (UTC)

**Authorization:** The maintainer/user authorized the A7 repository closeout
record on 2026-09-19. PR #53 had already merged at
`c20675615097c48a0cd93f70036b4b10b0a6d165`, which is the fixed base for this
closeout work. This record is prepared on a dedicated branch/worktree; its own
delivery PR and merge are still pending.

**Scope:** Record the completed bounded non-atomic physical-access migration and
its evidence. No runtime, ISA, `.qing/config.toml`, or protected source change is
part of this closeout.

## Outcome and boundary

A7 completed the approved **non-atomic** migration boundary: in the two
standard public facades, Hart instruction fetch and ordinary integer/FP
load/store accesses use the validated raw physical vocabulary, while Hart-owned
address/alignment/extension/trap/retirement behavior remains explicit. The
legacy typed AMO/LR/SC route remains over the same storage/device domain as an
intentional compatibility bridge. This is not a claim that all physical
accesses are unified or that ADR-0002's atomic portion is implemented.

The bounded closeout statement is:

> **Non-atomic physical-access migration completed.**

That statement is limited to the approved A7 boundary. It does not certify full
ADR-0002 convergence, atomicity, per-Hart reservations, the A extension, MMU,
interrupts, or the target Machine/Platform architecture.

The complete approved A7 contract is preserved in
[`a7-non-atomic-physical-access-migration.md`](a7-non-atomic-physical-access-migration.md).
The activation proposal snapshot remains separate historical drafting context.
The detailed implementation evidence remains in the
[A7 capability assessment](../../verification/a7-capability-assessment.md),
[T5 cumulative review checklist](../../verification/a7-t5-cumulative-review-checklist.md),
and T0--T4 records under `docs/verification/`.

## Six-item acceptance record

The following disposition maps each §8 item to source, tests, and exact evidence
rather than inferring support from component presence. The implementation
claims are bound to `fb6f51c771f32585f1547422c9633379f7ae370b`; documentation
and closeout evidence at later heads do not relabel that runtime identity.

| # | §8 acceptance item | Disposition and exact evidence | Boundary / limitation |
| --- | --- | --- | --- |
| 1 | Both standard public configurations use the typed raw boundary for fetch and ordinary integer/FP loads/stores, with native negative and side-effect tests. | **Accepted for the A7 boundary.** `src/core/mod.rs::step_outcome` selects the raw fetch/data views for the migrated operations; `src/executor.rs::install_image_with_physical_ports` installs separate raw views over shared domains; `src/physical.rs` supplies validation and adapters. `tests/a7_hart_physical.rs`, `tests/a7_native_targets.rs`, and `tests/a7_public_equivalence.rs` assert route selection, widths, full-span rejection, side-effect fences, and public workflows. The focused T0--T4 run and full Rust gate passed at the implementation evidence head. | AMO/LR/SC, old typed constructors, and typed host inspection remain explicit compatibility exceptions. They are not hidden behind a universal-migration claim. |
| 2 | Hart address/alignment/extension/trap/retirement ownership and A6 budget/exit facts are unchanged; no MMU or new ISA behavior is introduced. | **Accepted for the A7 boundary.** `src/core/mod.rs` retains Hart-side effective/original address, alignment, raw-byte interpretation, extension, and trap mapping. `tests/a7_hart_physical.rs`, `tests/a7_public_equivalence.rs`, `tests/a6_task3_core_trap_test.rs`, `tests/a6_trap_elf_integration.rs`, `tests/trap_test.rs`, `tests/csr_access_test.rs`, and `tests/mret_conformance_test.rs` cover the migration and A6 regressions. The exact T0--T4 focused command passed; PR53's exact-merge CI also passed its Rust and fresh guest jobs. | The public path still has no integrated MMU/PMP or asynchronous interrupt scheduler. A7 adds no such behavior. |
| 3 | Existing atomic success/failure and reservation behavior is regression-tested across the shared domain; the legacy seam is enumerated, storage/locking is shared, and no second reservation state exists. | **Accepted as preservation, not conformance.** `src/core/mod.rs` retains the typed legacy bridge and its complete outer lock coverage. `tests/a7_migration_characterization.rs`, `tests/a7_legacy_atomic_compat.rs`, `tests/amo_test.rs`, and the T3 Hart record cover ordinary↔legacy visibility, successful legacy operations, HTIF width debt, reservation-sensitive writes/faults, reset/reload behavior, and lock re-entry. The tests assert shared handles/domain identity and no second reservation state. | Legacy behavior is intentionally not repaired: AMO width defects, global exact-address reservation, absent scalar/FP/host invalidation, `aq`/`rl` absence, faulting-SC behavior, and the lack of an atomic operation envelope remain debt. |
| 4 | §6 conservative defaults hold; deviations have approval and known host/API limits are documented outside the ordinary guest-transaction guarantee. | **Accepted with recorded limits.** T0--T4 source/test records cover HTIF full-span versus legacy start-address behavior, host-write prefix semantics, nonaligned image offsets, old `MemoryInterface` compatibility, RAM/UART routing, side-effect rejection, artifacts, replacement, and facade differences. | The old typed constructor/API surface and host `write_mem` byte-loop remain usable. High-level native failure/unknown-completion injection has no safe public API; the reachable core/flat seams are not enlarged into a facade-wide claim. |
| 5 | Rust, fresh project ELF, and frozen ACT4 evidence is complete at the reviewed implementation head, with no historical result relabeled as a new run. | **Accepted with exact identity separation.** At `fb6f51c`, the recorded full gate reported 1,667 tests passed across 45 Rust/doc result groups, the focused T0--T4/A6 regression run reported 354 passed, and ELF guards passed. A fresh project-authored archive compiled and ran 51/51 guests, including five A6 trap guests. Separately, ACT4 workflow [35432032788](https://github.com/mimiqdev/ruscv-sim/actions/runs/35432032788) ran at `fb6f51c` with its frozen 51-case selection; the compact report and artifact identity are retained in [`a7-act4-replay-35432032788.json`](../../verification/a7-act4-replay-35432032788.json). The latest exact-merge main CI [35450169073](https://github.com/mimiqdev/ruscv-sim/actions/runs/35450169073) ran at `c206756` and succeeded, including fresh project-ELF compilation/execution. | The project-authored 51 and ACT4 51 are independent sets. ACT4 is frozen nontrapping RV64I evidence, not whole-ISA, privilege, atomic, or OS certification. This closeout did not rerun or regenerate ACT4. |
| 6 | Closeout states only “non-atomic physical-access migration completed”; §5.3 remains the unscheduled atomic convergence checklist. | **Accepted for bounded A7 delivery.** This record uses exactly that statement and carries §5.3 debt forward only as an explicit unscheduled checklist. The archive copy preserves the full contract. | The closeout PR represented by this record was not merged at this record head, and no successor contract or rolling `docs/dev-plan.md` replacement has been approved. |

## Acceptance-to-command map

The following are the exact task-row commands from the approved contract. The
results below are retained implementation/T5 evidence at `fb6f51c` or exact
merge CI at `c206756`; they are not claimed as a new ACT4 execution at this
closeout documentation head.

```bash
# T0
cargo test --test a7_migration_characterization --test memory_bounds \
  --test public_behavior --test a6_task3_core_trap_test

# T1
cargo test --test a7_physical_contract

# T2
cargo test --test a7_native_targets --test memory_bounds --test executor \
  --test peripheral_tests

# T3
cargo test --test a7_hart_physical --test a7_legacy_atomic_compat \
  --test amo_test --test a6_task3_core_trap_test --test trap_test \
  --test csr_access_test --test mret_conformance_test

# T4
cargo test --test a7_public_equivalence --test public_behavior \
  --test a4_integrated_equivalence --test a4_run_control --test executor \
  --test commits_test --test cli_test

# T5 quality and project-guest evidence
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
RISCV_TEST_OUTDIR=target/a7-fresh-riscv-elves ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a7-fresh-riscv-elves ./scripts/run_elf_tests.sh
python3 -m unittest discover -s scripts/a7 -p 'test_*.py' -v
```

The historical T5 record reports 354 focused tests and 1,667 full Rust/doc
results with zero failures, fresh project guests 51/51, and the pinned ACT4
workflow's separate 51-case result. The closeout worktree itself ran the Python
negative-path command and observed 10 passed, 0 failed. The full artifact replay
and protected-tree bridge were not rerun here because the external ZIP was not
completely retrievable; that absence is recorded as a limitation below.

## Delivery and exact Git provenance

All six A7 delivery PRs were merged before this closeout branch was based. The
short IDs below are only display labels; the full commit identities are the
binding evidence.

| Task / PR | Scope | Merged commit |
| --- | --- | --- |
| T0 / [PR #48](https://github.com/mimiqdev/ruscv-sim/pull/48) | Baseline characterization of memory, HTIF/UART routing, host writes, image offsets, and legacy atomic behavior | `fc68fcfe39055bd8f13255e0199bb47c7d3c9dee` |
| T1 / [PR #49](https://github.com/mimiqdev/ruscv-sim/pull/49) | Validated non-atomic request/response vocabulary and fault taxonomy | `36e5754fa438f147212ba376b863bd6a3ed890a0` |
| T2 / [PR #50](https://github.com/mimiqdev/ruscv-sim/pull/50) | Native RAM/UART/HTIF adapters and side-effect/routing tests | `46620797b2d443e7a653c56f423694edc7b54093` |
| T3 / [PR #51](https://github.com/mimiqdev/ruscv-sim/pull/51) | Hart ordinary fetch/data connection and explicit typed legacy bridge | `a75f8b290a3c359793f1f834476b44c2a485f952` |
| T4 / [PR #52](https://github.com/mimiqdev/ruscv-sim/pull/52) | Public-facade equivalence, failure boundaries, artifacts, reload, and route audit | `fb6f51c771f32585f1547422c9633379f7ae370b` |
| T5 / [PR #53](https://github.com/mimiqdev/ruscv-sim/pull/53) | Documentation-side final evidence, replay tools, comparison observation, and cumulative-review checklist | `c20675615097c48a0cd93f70036b4b10b0a6d165` |

The T4 implementation commit `fb6f51c` is the **implementation evidence head**.
PR #53's merge commit `c206756` is later documentation/evidence provenance, not
a runtime implementation head. The original T5 evidence/review documentation
head was `84875711e9d0cc789da17e0557272c4e6c8ea852`. Its tree is identical to the
PR #53 merge tree for the protected runtime/test/CI inputs; `c206756` is a
separate merge identity with the same protected source identity. The closeout
must not report `c206756` as a new A7 runtime implementation or as a fresh ACT4
source head.

## Cumulative independent review

The cumulative independent review did inspect the T0--T4 source and test
dependencies named by the checklist. The first review round is retained by the
repository provenance at `d8a0bd927ccda2b52b0460773a285a28696ab01f` and reported
no additional runtime defect. Its only finding was an R4 documentation issue:
the checklist incorrectly assigned execution responsibility to the reviewer.

Commit `84875711e9d0cc789da17e0557272c4e6c8ea852` changed only the cumulative
review checklist to correct that responsibility wording. The fresh five-item
verification/review pass after that documentation-only correction passed, with
no new runtime finding. This closeout records the result from the durable
repository provenance and review outcome; it does not depend on a private
execution path or private review storage. The [checklist](../../verification/a7-t5-cumulative-review-checklist.md)
and [assessment](../../verification/a7-capability-assessment.md) are the durable
repository inputs.

This review conclusion is distinct from the coding-side T5 audit and from the
later closeout PR review. Any finding against this new closeout HEAD must be
addressed and reviewed against that exact new commit; the prior review does not
silently approve future documentation changes.

## Verification evidence and exact commands

### Rust, project guests, and latest CI

The implementation-head evidence recorded by T5 used the pinned development
image
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`,
Rust `1.97.1`, `CARGO_BUILD_JOBS=2`, and isolated target directories. The exact
commands were:

```text
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust cargo check --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust cargo clippy --all-features --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust cargo test --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
```

Those commands are historical exact-head evidence at `fb6f51c` (and the
assessment records the same documentation-only follow-up gate at the T5
verification head). They are not claimed as a new runtime run at this closeout
head. The [latest exact-merge CI run 35450169073](https://github.com/mimiqdev/ruscv-sim/actions/runs/35450169073)
had head `c20675615097c48a0cd93f70036b4b10b0a6d165`, concluded `success` on
2026-09-19, passed its `Quality and tests` job, and passed release/smoke,
protected-output guards, fresh project-ELF compilation, and public CLI execution.
Coverage was skipped by workflow policy. The only recorded annotations were
platform action deprecation notices; they did not fail the job.

The two 51-case suites are deliberately separate:

* The fresh project-authored suite compiled and ran 51 repository guests through
the public CLI, including five A6 trap guests. The exact-merge CI also freshly
ran this repository suite at `c206756`.
* The frozen ACT4 workflow generated/executed its separate 51-case selected
nontrapping RV64I set at `fb6f51c`, with 51 linked audits, six guest controls,
and separately accounted signature ELFs. It is retained as workflow run
`35432032788`, artifact `10580339624`, 21,675,677 bytes, SHA-256
`a8d587f8310cb54e923c9a48c5b83f931ecf6b2054fd698a4b4d356802d95877`.

The ACT4 ZIP was not present in this closeout worktree. An authenticated
retrieval attempt did not complete before the bounded environment timeout and
left only an unusable partial file, so this closeout does **not** claim a new
ZIP replay or a fresh ACT4 execution. The compact committed replay is retained
historical evidence from the T5 verification, not a run at this closeout HEAD.

### Replay and negative-path tools

The committed replay tool is [`scripts/a7/replay_act4.py`](../../../scripts/a7/replay_act4.py);
the protected-tree bridge is
[`scripts/a7/verify_evidence_tree.py`](../../../scripts/a7/verify_evidence_tree.py).
The focused local negative-path command was run at the fixed PR53 base:

```bash
python3 -m unittest discover -s scripts/a7 -p 'test_*.py' -v
```

Result: **10 tests passed, 0 failed**. The tests reject digest mismatch, ZIP
path traversal, duplicate members, mutated summary/accounting, artifact-ID
mismatch, and every logged/sample result-tuple mismatch. This is tool-test
evidence, not ACT4 execution.

The full bridge requires the external ZIP and `artifact-api.json`; because that
artifact was not completely retrievable here, the bridge was not reported as
passed in this closeout. The durable bridge contract still explicitly compares
the committed replay JSON with regenerated replay output, checks artifact
ID/size/digest/source identity, rejects protected runtime/test/CI changes, and
checks the corrected T0 identity.

### Protected source identity bridge

The implementation evidence head, original T5 documentation head, and PR #53
merge have identical protected subtree identities:

| Protected path | `fb6f51c` / `8487571` / `c206756` identity |
| --- | --- |
| `src` | `273a52e350f9efc1f78280918868e9489936d5b0` |
| `tests` | `efaad839cf66e3344a0b3d9b1ccc926cceb46d4e` |
| `benches` | `dceeadb23715afe1e00fd3b14b1c95798fd78641` |
| `ruscv-macros` | `61c9f68128e67b902924f83b7950f25426bfb731` |
| `.github/workflows` | `6daa6bba3506dfcf35536f4017f9ed1169884903` |
| `.qing/config.toml` | `b03c17d5b2318aa34bf75391ff74c6a79ff426c0` |
| `Cargo.toml` | `2a8ee26506188c29c7731125bf579fd39426071d` |
| `Cargo.lock` | `79e557f5205bb9b70b00bbd0ee0bf138866db71e` |

The exact range comparison was empty for the protected paths from `fb6f51c` to
`c206756`, and the same identity is retained by this docs-only closeout branch.
Verification-only `docs/` and `scripts/a7/` additions do not alter the runtime,
ISA, tests, build inputs, CI workflow, or `.qing/config.toml` protected tree.

## Performance observation

The real public-facade physical-loop observation used the same fixture, public
`load_and_run` facade, and isolated release builds at the pre-migration
`9ee9c5f24c072779f06ce141b3340f953b614442` and implementation
`fb6f51c771f32585f1547422c9633379f7ae370b` revisions. It retained 45 samples
per revision and observed exit code 0, 20,490 completed turns/commit records,
final PC `0x8000003c`, and no timeout/error for each revision.

The raw-port median divided by the old typed median was **1.2858** in that one
pinned ARM64/Rust 1.97.1 environment. This is a load/run sample observation,
not an overall performance-regression conclusion, a benchmark acceptance
threshold, or a reason to optimize A7. The complete distributions and fixture
hashes remain in [`a7-physical-loop-observation.json`](../../verification/a7-physical-loop-observation.json)
and [`a7-performance-observation.json`](../../verification/a7-performance-observation.json).

## Residual debt and explicit non-claims

### Approved deferred debt and non-goals

These are approved A7 boundaries, not hidden acceptance failures:

* Legacy atomic width behavior, global reservation state, missing `aq`/`rl`
  semantics, absent per-Hart reservation ownership/invalidation, and the lack of
  an indivisible AMO/LR/SC operation envelope remain for a separately approved
  future convergence contract. Existing successful atomic behavior remains
  regression-preserved rather than denied or reclassified.
* MMU/Sv39/PMP/page-fault integration, asynchronous interrupts and
  CLINT/PLIC/WFI, TLM/DMI/SystemC/FFI public wiring, Debug Mode, multi-Hart/DMA
  coherence, and the full Machine/Platform lifecycle are outside A7.
* A7 does not certify full RV64I/M/A/F/D/C, whole-ISA behavior, external
  architectural compatibility beyond the frozen bounded ACT4 selection, OS boot,
  or any future scheduler/Runner architecture.

### In-scope compatibility and evidence limits

These limitations are documented preservation boundaries, not silently widened
claims:

* The old typed `MemoryInterface`/`RiscvCore::new` construction path remains
  callable for compatibility. The two standard public facades use the raw route;
  an old typed caller is not retroactively advertised as a conforming raw backend.
* Host inspection and `RiscVSimulator::write_mem` remain typed APIs. Host writes
  retain byte-loop prefix-on-failure behavior and are outside the all-or-nothing
  migrated guest transaction guarantee.
* Native high-level host-failure and unknown-completion injection has no safe
  public API. The reachable typed-core and flat-host seams are the evidence
  actually claimed; no facade-wide injection coverage is inferred.
* UART/HTIF, flat offset, artifact, diagnostic, and reload differences between
  the native and flat configurations remain intentional and tested configuration
  differences.

No residual item authorizes a compatibility tightening, an ISA repair, or an
unapproved backlog migration in this closeout.

## Next rolling decision

No successor milestone is approved by this record. The requested next step is
first to form a reviewed and approved roadmap, then select a successor contract
from that roadmap. This record does not invent an A8/A9 order, carry forward an
old backlog, or promise a technical sequence.

Until that decision exists, `docs/dev-plan.md` remains the sole Current
technical specification. The A7 contract is archived here as a complete
historical copy for provenance, while the closeout record documents bounded
acceptance and the unmerged delivery state. The rolling switch to a future
approved contract remains pending; this closeout does not bypass the
`AGENTS.md` rolling-milestone sequence.

## Closeout limitations

* The A7 implementation evidence is exact at `fb6f51c`; PR53's merge/evidence
  head is `c206756`; the original T5 documentation/review head is `8487571`.
  These identities are not interchangeable.
* ACT4 35432032788 and its artifact/replay are historical evidence at `fb6f51c`.
  No ACT4 execution was rerun at `c206756` or at this closeout head.
* The full external artifact was not locally available for a new replay/bridge
  in this bounded closeout environment. The committed compact replay and the
  ten local negative-path tests remain separately labelled evidence.
* A closeout PR review must target the exact commit produced by this branch.
  The earlier cumulative T0--T4 review conclusion does not pre-approve this
  new documentation head.
