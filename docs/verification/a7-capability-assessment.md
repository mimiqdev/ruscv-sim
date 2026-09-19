# A7 T5 final verification and bounded closeout evidence

**Status:** Current — coding-side T5 evidence is supplemented at the bounded
scope below; cumulative independent source review remains pending; A7 is **not
formally closed** by this record.

**Authority:** Informational evidence record. `docs/dev-plan.md` remains the
sole active milestone contract; accepted ADRs and source/tests remain
authoritative. This record does not move the plan to `docs/archive/`, change
`.qing/config.toml`, or declare a milestone transition.

**Implementation evidence head:**
`fb6f51c771f32585f1547422c9633379f7ae370b`

**Verified:** 2026-09-19

## Scope and coding-side audit (not independent review)

This coding round audited the T0--T4 records against the source and focused
tests at the implementation evidence head, then ran the full Rust gate, the
focused migration/regression set, a fresh project-authored ELF archive, the
required A6 ELF integration, a fresh pinned ACT4 workflow, and a separate
pre-/post-migration public-facade observation. These are coding-side evidence
and do not constitute an independent reviewer audit. In particular, the
runtime baseline and the cumulative source range have not yet been re-reviewed
by an independent reviewer; PR review must verify the checklist in
[`a7-t5-cumulative-review-checklist.md`](a7-t5-cumulative-review-checklist.md)
at the final PR head. Component presence and historical A5/A6 results are not
counted as new A7 evidence.

The implementation range is the five A7 delivery commits `fc68fcf` through
`fb6f51c` (T0 characterization, T1 contract, T2 adapters, T3 Hart wiring, and
T4 facade equivalence). The final source tree was clean before this record was
written. No unapproved compatibility change, second ISA engine, storage copy,
or atomic-capability denial was found by this coding-side audit.

| Task | Coding-side source/test audit | Focused result at the implementation evidence head |
| --- | --- | --- |
| T0 | `tests/a7_migration_characterization.rs` and the baseline/candidate record were checked for preservation-vs-debt labeling, including successful legacy AMO/LR/SC, HTIF overlap, nonaligned placement, and host-write prefix behavior. | 11 passed |
| T1 | `src/physical.rs` and `tests/a7_physical_contract.rs` were checked for raw bytes, Fetch/Read/Write categories, exact widths, fault taxonomy, and terminal unknown completion. | 12 passed |
| T2 | `src/physical.rs`, `src/executor.rs`, and `tests/a7_native_targets.rs` were checked for shared RAM/device objects, full-span routing, side-effect fences, and legacy typed behavior. | 13 passed |
| T3 | `src/core/mod.rs` and `tests/a7_hart_physical.rs` were checked for Hart-owned address/alignment/extension/trap/retirement work; `tests/a7_legacy_atomic_compat.rs` was checked for the explicit typed bridge and lock domain. | 6 + 8 passed |
| T4 | `src/executor.rs` route points and `tests/a7_public_equivalence.rs` were checked for both public facades, shared ordinary results, configuration differences, logging, reload, and bounded failure seams. | 10 passed |

The focused run also included the A4/A6, atomic, CLI, commit, CSR, executor,
memory-bound, MRET, public-behavior, and trap regressions: **354 tests passed,
0 failed**, plus all ELF output-path guard cases. The complete Rust gate reported
**1,667 tests passed, 0 failed** across 45 Rust/doc test result groups. This is
coding-side verification; the cumulative independent review status is tracked
separately in the checklist linked above.

## §8 six-item acceptance matrix

This matrix records evidence disposition, not formal milestone closure. “Verified”
means the bounded A7 contract is supported at the cited head; it does not mean
ADR-0002's atomic portion is complete.

| # | Acceptance item | Evidence disposition | Exact evidence and limits |
| --- | --- | --- | --- |
| 1 | Standard public configurations use the raw boundary for fetch and ordinary integer/FP loads/stores, with native negative and side-effect tests. | **Verified** | T3/T4 route audits, `a7_hart_physical`, `a7_native_targets`, and `a7_public_equivalence`; the focused 354-test run and full gate passed. AMO/LR/SC, old typed constructors, and host inspection remain explicit typed exceptions. |
| 2 | Hart address/alignment/extension/trap/retirement ownership and A6 budget/exit facts are unchanged; no MMU or new ISA behavior is introduced. | **Verified** | T3 Hart traces, T4 public workflow assertions, A6 trap/CSR/MRET regressions, fresh project ELFs (including five trap guests), and the full gate passed. The evidence does not claim MMU/PMP or asynchronous interrupt integration. |
| 3 | Existing atomic success/failure and reservation behavior is regression-tested across the shared domain; the legacy seam is enumerated, storage/locking is shared, and no second reservation state exists. | **Verified as preservation, not conformance** | T0/T3 matrices and `a7_legacy_atomic_compat` cover ordinary↔legacy visibility, reservation-sensitive behavior, faults, reset/reload, HTIF width debt, and lock re-entry. The known global reservation and atomic-width defects remain deferred. |
| 4 | §6 conservative defaults hold; deviations have approval and known host/API limits are documented outside the ordinary guest-transaction guarantee. | **Verified** | T0/T3/T4 records and focused tests cover HTIF spans, host-write prefix semantics, nonaligned image offsets, third-party typed backends, RAM/UART routing, artifacts, and facade differences. High-level native failure/unknown injection remains unavailable through a safe public API. |
| 5 | Rust, fresh project ELF, and frozen ACT4 evidence is complete at the reviewed implementation head; no historical result is relabeled. | **Verified** | Full/focused local evidence and fresh source archive are bound to `fb6f51c`. The new ACT4 workflow [35432032788](https://github.com/mimiqdev/ruscv-sim/actions/runs/35432032788) ran at that head and produced 51/51 frozen cases; its artifact and offline replay are recorded below. |
| 6 | The bounded closeout statement is “non-atomic physical-access migration completed”; §5.3 remains the unscheduled atomic convergence checklist. | **Prepared, not formally enacted** | This record supports that bounded statement, but intentionally does not archive/replace `docs/dev-plan.md`, mark A7 formally closed, or claim complete ADR-0002/atomic convergence. |

## Exact-head verification evidence

### Rust gate and focused regression set

The isolated ARM64 development environment used the pinned image
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`,
`RUSTUP_TOOLCHAIN=1.97.1`, and `CARGO_BUILD_JOBS=2`. The retained logs are
locally under `target/a7-final-verification-evidence/`.

```text
cargo fmt --all -- --check                                      passed
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust \
  cargo check --all-features                                   passed
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust \
  cargo clippy --all-features --all-targets -- -D warnings     passed
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust \
  cargo test --all-features                                    passed
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-final-verification-rust \
  cargo doc --all-features --no-deps                            passed
```

The focused target set was the T0--T4 rows above plus
`a4_integrated_equivalence`, `a4_run_control`, `a6_task3_core_trap_test`,
`a6_trap_elf_integration`, `amo_test`, `cli_test`, `commits_test`,
`csr_access_test`, `executor`, `memory_bounds`, `mret_conformance_test`,
`public_behavior`, and `trap_test`. `bash scripts/test_riscv_elf_guards.sh`
also passed all guard cases. The full and focused status lines are retained in
`full-gate.log` and `focused-and-guards.log`.

The documentation-only evidence commit `57e5dd3c42ac471fc4e81ff48844e30716998b91`
then repeated the full gate and focused target set at its own HEAD: the final
logs report the same 0-failure results and the same ELF guards. This follow-up
changed only the evidence documents and replay tool; it does not relabel the
fresh guest/ACT4 evidence below, which remains bound to implementation head
`fb6f51c771f32585f1547422c9633379f7ae370b`.

### Fresh project-authored ELF and A6 integration

A fresh `git archive` was created from `fb6f51c`; it contained 51 project-authored
assembly sources. In a fresh output directory, **51 sources compiled, 51 ELFs
ran through the public CLI, 51 passed, and 0 failed**, including the five A6
trap guests. The separate `a6_trap_elf_integration` test passed and exercised
those five guests through CLI, `load_and_run`, and `RiscVSimulator`. No existing
ELF was reused. The exact source identity, inventory, and combined log are
retained as `fresh-source.txt`, `fresh-project-elf-inventory.txt`, and
`fresh-project-elfs-and-a6.log` under the ignored evidence directory.

### Fresh ACT4 workflow and artifact integrity

The workflow was dispatched with the frozen selection unchanged:

```text
workflow: a5-feasibility.yml
selection: proposal
run: 35432032788
head: fb6f51c771f32585f1547422c9633379f7ae370b
conclusion: success
artifact: a5-feasibility-fb6f51c771f32585f1547422c9633379f7ae370b
artifact id: 10580339624
artifact bytes: 21,675,677
artifact sha256: a8d587f8310cb54e923c9a48c5b83f931ecf6b2054fd698a4b4d356802d95877
```

The workflow generated and executed the approved frozen profile's **51
self-check ELFs and 51 separately accounted signature ELFs**, with 51/51 public
CLI guest passes, 51 linked audits, no unsupported linked instructions, and six
public guest controls (pass, guest-fail, timeout, unsupported, malformed, and
missing). The retained inventory has 1,970 sources, of which 51 are the required
base-I selection. The GitHub artifact digest and byte count were independently
verified before ZIP extraction; the artifact expires 2026-10-03.

The compact durable replay is [`a7-act4-replay-35432032788.json`](a7-act4-replay-35432032788.json).
It was produced by [`scripts/a7/replay_act4.py`](../../scripts/a7/replay_act4.py),
which validates the ZIP safety/digest, frozen plan and checked-in input hashes,
all generated artifact hashes and identities, strict CLI result blocks, all 51
linked audits, six controls, and twelve fail-closed accounting mutations. It
performs no GitHub call, regeneration, or guest execution:

```bash
python3 scripts/a7/replay_act4.py \
  path/to/a5-feasibility-fb6f51c771f32585f1547422c9633379f7ae370b.zip \
  --write docs/verification/a7-act4-replay-35432032788.json
```

The replay tool's focused negative tests are independently runnable without the
large artifact:

```bash
python3 -m unittest discover -s scripts/a7 -p 'test_*.py' -v
```

They reject digest mismatch, path traversal, duplicate ZIP members, mutated
summary accounting, and missing/duplicate/extra result identities. After a
real replay, `scripts/a7/verify_evidence_tree.py` bridges the generated report
to the committed JSON, GitHub artifact ID/size/digest, exact implementation
head, and an empty protected runtime/tests/CI tree change set.

The large ZIP and generated ELFs are retained externally/under ignored local
`target/` evidence, not committed. The replay report is the compact committed
record.

## Performance observation

### Existing component observations (not a migration substitute)

The existing Criterion benches were retained as current-head observations only:

```bash
RUSTUP_TOOLCHAIN=1.97.1 CARGO_BUILD_JOBS=2 \
  CARGO_TARGET_DIR=target/a7-final-verification-bench \
  cargo bench --all-features --bench decode_bench -- --noplot
RUSTUP_TOOLCHAIN=1.97.1 CARGO_BUILD_JOBS=2 \
  CARGO_TARGET_DIR=target/a7-final-verification-bench \
  cargo bench --all-features --bench memory_bench -- --noplot
```

Representative current observations were decode ADD **4.2264 ns**, mixed decode
**19.651 ns**, typed word read **12.446 ns**, typed word write **63.335 ns**, and
4 KiB read throughput **335.29 MiB/s**. These benches do not execute the real
standard facade against the pre-migration revision and are not used as the
comparison below.

### Real public-facade comparison

[`a7-physical-loop-observation.json`](a7-physical-loop-observation.json) is a
reusable, no-runtime-change comparison harness. It uses `git archive` for the
exact pre-migration runtime `9ee9c5f24c072779f06ce141b3340f953b614442` and A7
implementation head `fb6f51c771f32585f1547422c9633379f7ae370b`, assembles one
fixture once with `-march=rv64ima_zicsr -mabi=lp64`, and builds the same
temporary `a7_physical_loop` binary inside each archive with Rust 1.97.1,
`--release --all-features --locked`, and two isolated target directories. Both
revisions compile and run through the same public
`ruscv_sim::executor::load_and_run` facade; the harness never constructs the
old typed `RiscvCore` directly.

The fixture is 4,096 iterations of `SD`/`LD`/`ADDI`/`ADDI`/`BNEZ` against one
RAM word, followed by the same ELF-declared `.tohost` store. Each revision had
three repetitions of 15 timed samples, with every sample retained. A separate
commit-logged verification run per repetition recorded exit code **0**,
**20,490** completed turns, **20,490** commit records, final PC
`0x8000003c`, and no timeout/error. The combined distributions were:

| Revision | Median | Min--max | P95 | Relative range |
| --- | ---: | ---: | ---: | ---: |
| `9ee9c5f` typed public route | 2,776,624 ns | 2,755,398--2,871,528 ns | 2,822,491 ns | 4.18% |
| `fb6f51c` raw-port public route | 3,765,507 ns | 3,563,680--3,880,571 ns | 3,857,693 ns | 8.42% |

The raw-port median divided by the old typed median was **1.3015** for this
sample and environment. This is deliberately not a regression verdict or a
performance threshold: the result is reported as observed, including the
slower current samples, and no optimization or filtering was added. The run
was on the pinned development image
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`,
Rust 1.97.1, GNU binutils 2.40, and the recorded ARM64 Linux hardware
environment in the JSON report.

The static route bridge in the report confirms the old facade's typed
`RiscVCore::new` path and the implementation facade's
`install_image_with_physical_ports` → `new_with_physical_access` → validated
`SystemBus::physical_backend` path. Source inspection records one image
allocation, two shared raw-port handles, fixed-size ordinary request storage,
and the raw port → bus → child target lock order; no allocator/lock profiler was
added, so no allocation or lock-overhead claim is made.

## Deferred debt and explicit non-claims

A7 deliberately stops at the non-atomic boundary. The following remain known
debt or out of scope:

- AMO/LR/SC still use the typed legacy bridge and are not an ADR-0002 atomic
envelope. The T0/T3 record preserves the observed AMO width behavior, global
exact-address reservation, missing ordinary-store/FP/host-write invalidation,
reset retention, and SC write-fault behavior; no second reservation state was
introduced.
- No claim is made for full A-extension compliance, AMO `aq`/`rl`, per-Hart
reservation ownership, multi-Hart/DMA/coherence ordering, or eventual atomic
convergence. Section 5.3 of the active plan remains unscheduled.
- MMU/Sv39/PMP/page faults, asynchronous interrupts/CLINT/PLIC/WFI, TLM/DMI or
SystemC/FFI wiring, Debug Mode, and a full Machine/Platform lifecycle remain
outside A7.
- The 51 fresh project guests and the 51 frozen ACT4 cases are separate bounded
sets. ACT4 is a nontrapping adapted RV64I selection; it is not whole-ISA,
privilege, misalignment-success, or operating-system certification.
- Native high-level host-failure and unknown-completion injection has no safe
public API. T4's reachable typed-core and flat-host seams are not enlarged into
a facade-wide failure-injection claim.

The coding-side executable observations above were available and passed, and no
unavailable tool was silently counted as a pass. This record is nevertheless
**not an independent-review-complete record**: the cumulative source/runtime
review and final PR-head acceptance review remain pending and must use the
checklist linked above. The only CI annotation was the platform's Node.js 20
deprecation warning for pinned third-party actions; the workflow job and
artifact checks succeeded.
