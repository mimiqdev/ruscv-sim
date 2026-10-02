# A10 candidate — independent public-path performance-test infrastructure

**Status:** Draft

**Authority:** Informational detailed successor proposal awaiting approval; not `docs/dev-plan.md`, not active and not an implementation or performance result.

**Prepared:** 2026-10-02. **Source baseline:** merged A9 `0898f1215c4508e98d4db958080f6dd01b5745bf`. The [A9 acceptance](../verification/a9-closeout-assessment.md#7-final-pr-head-acceptance-and-exact-merge-head-evidence) establishes bounded native/flat integration and evidence, not a performance facility. A9 remains the sole [Current contract](../dev-plan.md) until separately approved rolling replacement.

## 1. Decision requested and objective

Approve this bounded successor contract, including its workload/matrix, correctness-first measurement, report/retention and non-gating calibration policy, as the basis for a **later A9→A10 rotation**. Alternatively request a bounded revision or defer the successor while retaining A9. Approval of this draft must be explicit; this document does not itself authorize rotation, implementation or merge. A9's technical acceptance does not require implementing A10.

Objective: leave one repository-owned command that later implementation PRs can run unchanged to validate already-runnable public guest work, measure distinct host-cost phases, retain raw evidence and compare a compatible baseline. Deliver infrastructure and calibrated evidence classifications, **not a faster simulator**.

This follows Stage 3 of the [post-A7 roadmap](post-a7-roadmap.md), after verified A8 atomic convergence and A9 facts/lifecycle, before selecting later integrated VP capabilities. That is recommended sequencing, not an ADR technical dependency or date promise. The roadmap remains A7-era Draft input: its current-A7 statements, global-reservation/legacy atomic debt and proposed quiescence-only host-writer choice were superseded by verified A8/A9. Its historical bodies and A7 observations remain unchanged. No old debt is automatically inherited.

Accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md) constrain all routes and measurements. Host elapsed nanoseconds are not completed turns, `minstret`, `mcycle`, virtual time or hardware cycles.

## 2. Entry evidence and existing controls

Actual available seams, not proposed capability claims:

- [ELF `LoadImage::parse`](../../src/elf.rs) describes segments/zero fill/entry/signature/tohost without placement; [Machine `install`, `resume`, `step`, `inspect`, `request_quiesce`, `try_drain`, `fresh_reset`, `teardown`](../../src/machine/mod.rs) compose either `PlatformKind::Native` or `Flat`. `step(observe)` invokes the same `Installed::turn` used by standard facades and returns a retained `MachineTurn`, Hart control/optional [observations](../../src/core/observation.rs), raw events and selected signal. Receipts must be consumed before another step/drain.
- [Native `load_and_run` / `load_and_run_file` and flat `RiscVSimulator`](../../src/executor.rs), reached by the [real CLI](../../src/main.rs), preserve public result/budget/artifact policy. Native logger subscribes when requested. Flat `run`/`step` has no public logging/observation toggle; do not invent one in the inventory. The internal `OwnedMachine` and `NativeHooks` are not exported APIs.
- [Facade tests](../../tests/a9_facade_composition.rs) prove the shared turn route, ordinary/AMO/LR/SC/exit and fresh rerun; [lifecycle tests](../../tests/a9_machine_lifecycle.rs) prove restoration and receipt-covered delivery; [Runner tests](../../src/executor_facade_tests.rs) cover co-incident facts and flat signal/artifact guards; [Hart tests](../../tests/a9_hart_facts.rs) distinguish off/on materialization and exact atomic effects.

[Cargo.toml](../../Cargo.toml) declares seven Criterion targets. Keep all as **component controls**, reported in a separate namespace:

| Existing target | Actual exercised code | Not public-path evidence |
| --- | --- | --- |
| [decode_bench](../../benches/decode_bench.rs) | Direct `InstructionDecoder` calls | No fetch/Machine/ELF |
| [execute_bench](../../benches/execute_bench.rs) | Direct `Executor::execute`, constructed state and typed `SimpleMemory` | No Hart transition/public ports or lifecycle |
| [memory_bench](../../benches/memory_bench.rs) | Typed byte/half/word storage and throughput loops | No native routing/device/public run |
| [branch_predict_bench](../../benches/branch_predict_bench.rs) | Bench-local predictors, partly random sequences | No predictor in active interpreter |
| [pipeline_bench](../../benches/pipeline_bench.rs) | Bench-local pipeline/OOO models | No active public pipeline |
| [cache_bench](../../benches/cache_bench.rs) | Bench-local cache/latency model | No active cache/DMI or host-time proxy |
| [rv64d_bench](../../benches/rv64d_bench.rs) | Direct FP helpers with constructed registers | No public FP guest/profile certification |

[bench-scheduled.yml](../../.github/workflows/bench-scheduled.yml) runs `cargo bench --all-features -- --verbose` on weekly Sunday 02:00 UTC cron, despite its “Daily” label, on `ubuntu-latest`/stable with seven-day Criterion retention. It does not pin a host, compare exact revisions or prove public workload correctness. [ci.yml](../../.github/workflows/ci.yml) supplies fresh guest/toolchain/build seams, not this proposed facility. Existing A7 timings are historical one-environment observations, not A10 baselines. No benchmark was run for this draft.

## 3. Selected scope and exclusions

In scope after approval: a small versioned workload/oracle manifest, a Rust measurement driver plus repository command wrapper, schema/parser and negative controls, three cost phases, route/observation matrix, environment capture, raw sample retention, baseline/PR comparison, weekly reporting and documentation. Recommend standard Rust `Instant`, existing Criterion for component controls, and a small dedicated driver for exact phase boundaries; Rust type/module choices do not need separate user micro-decisions.

Non-goals: hotspot optimization, fast paths or second ISA engine; runtime semantic fixes/weakening; universal percentage gate; new board/topology; MMU/PMP/page walks, interrupts/WFI/time scheduler, debug/GDB, TLM/SystemC/DMI, OS/Linux, multi-Hart/DMA/coherence; full ADR/ISA/RV64A certification or new ACT4 selection/run; hardware timing; external runner purchase; platform/public API break. No recovery/force-reset API. If a measurement cannot be made through existing APIs, label it unavailable or surface a bounded compatibility decision, not expose internals or alter semantics silently.

## 4. Initial workload and oracle contract

Select a **small mandatory suite**, not all 58 guests as benchmarks. Existing small fixtures are correctness anchors; bounded deterministic loop variants can be added as harness fixtures after approval to obtain useful durations. Every variant gets a new manifest/version, source/linker hash and hand-derived oracle; existing guests and historical JSON are not rewritten. All instructions must already run at the merged A9 baseline.

| Mandatory family / existing runnable anchor | Required routes and declared oracle |
| --- | --- |
| Integer/control flow: [rv64i/fib.S](../../tests/bare-metal-riscv-test/rv64i/fib.S), optionally bounded loop form of its ADD/branch sequence | Native CLI/library, flat facade, native/flat Machine. Existing F10=55, exit 0; checkpoint CLI anchor 68 completed turns and final PC `0x80000054`. Declare iteration count, final relevant GPRs/checksum, exact attempts/retirements and exit-store boundary for each pinned ELF. No traps or explicit retirement-counter writes in this family. |
| Ordinary RAM: [rv64i/sw.S](../../tests/bare-metal-riscv-test/rv64i/sw.S) and signature/BSS fixture from [tests/common/public_elf.rs](../../tests/common/public_elf.rs) / [A7 public equivalence](../../tests/a7_public_equivalence.rs) | Both configurations and real facades. Check exact bytes including untouched neighbors/zero fill and load-result registers, signature bytes/digest, guest result, PC and declared work/retirements. Add a deterministic bounded store/load loop fixture rather than timing typed memory helpers. |
| Native device: [rv64i/hello.S](../../tests/bare-metal-riscv-test/rv64i/hello.S), and UART/fixed-HTIF fixture in [A9 facade tests](../../tests/a9_facade_composition.rs) | Native CLI/library and Machine only. Existing hello output exactly `Hello!\n`, exit 0, checkpoint 58 completed turns, PC `0x80000088`; Machine checks seven UART transmit events in order. Fixed HTIF fixture emits `A`, event value 7, exit 3 after six retirements. Nonzero declared success must not be confused with a failed oracle. Flat device execution is explicitly N/A, not native parity. |
| Mixed ordinary/atomic W and D: [amo_w.S](../../tests/bare-metal-riscv-test/rv64a/amo_w.S), [amo_d.S](../../tests/bare-metal-riscv-test/rv64a/amo_d.S), [lrsc_loop.S](../../tests/bare-metal-riscv-test/rv64a/lrsc_loop.S), [sc_after_store_failure.S](../../tests/bare-metal-riscv-test/rv64a/sc_after_store_failure.S), A9 same-ELF ordinary→AMO→LR/SC fixture | Native/flat Machine and both facades/CLI for RAM-tohost ELFs. Existing W final word 6 and D final dword 4; old results and W sign extension must match. Pin exact ordinary writes, W/D atomic widths/old/new values, successful/failed SC result and unchanged bytes on conditional failure, no leaked reservation after fresh reset. A9 D mixed fixture exits 3 after 14 retirements, rd results `[3,6,0,1]`, RAM dword 3. Require W mixed ordinary/LR/SC variant too using already-supported encodings; it is a future harness fixture, not claimed existing A9 test coverage. |
| A9 observation/lifecycle: same RAM/mixed ELF, native/flat [receipt/drain/fresh rerun tests](../../tests/a9_machine_lifecycle.rs) | Public Machine off/on runs must give identical architectural/device/result oracles. Consumed receipts → requested/proven drain → fresh reset → fresh rerun must restore bytes/zero fill/counters/reservation/devices, isolate stale handle and reproduce the oracle. Reset/drain is setup outside execute-only timer, not a new fourth throughput metric. Retained receipt refuses mutation. |

Checkpoint counts above are **fixture anchors**, not new benchmark passes. Manifest counts must be derived from the exact linked instruction sequence and guest algorithm, checked against the real paths, and kept separate from budgets. `ExecutionResult.cycles` includes completed trap turns; it cannot automatically be renamed retirement. For these nontrapping fixtures the declared sequence establishes equality; trap-inclusive extensions must report attempts, completed turns, traps and retirements independently. Prefer a guest-written signature containing work/checksum and retirement-counter reading for new longer fixtures, with explicit accounting for the reader/exit instructions, so real CLI samples expose the oracle without a new CLI API. No instruction may write MINSTRET in the initial suite.

**Every measured sample**, including warmup/calibration and every repetition within an aggregate, must pass its own declared status/exit, final PC or signature (prefer both), work/retirement count, relevant memory/register/device and observation/log effects **before its timing is accepted**. Preflight alone cannot bless later samples. For CLI/library routes not exposing registers, the guest self-check/signature and exact no-trap work/control-flow oracle encode those effects; Machine companion runs provide richer assertions but never replace a sample's own public oracle. Missing evidence is inconclusive, not guessed. Changed result, timeout, transport/reporting error or wrong count/signature is `semantic_failure`, never performance pass—even if elapsed time looks faster.

Observation oracles: off has no materialized records; on has one commit per retirement/one trap per completed trap, exact fetched PC/instruction/effects/order, no fabricated x0/SC-failure memory write. Text logs retain current omission rules, checked against [CommitLogger](../../src/core/commits.rs), not mistaken for full CSR/FPR/memory export. On/off must preserve final architecture, counters, devices and result. File write/flush failure fails the oracle; no retrospective unretirement.

## 5. Route, observation and phase matrix

Required matrix, with explicit unsupported cells:

| Route | Off | Fact materialization + checked sink | Serialized log |
| --- | --- | --- | --- |
| Public native and flat `Machine::step` driver | Required, same `Installed::turn` | Required, same fixtures; control-only accounting remains even off | Required using existing `CommitLogger` as a sink; separately label in-memory serialization versus file serialization |
| Native library `load_and_run` / file variant | Required | No public arbitrary sink toggle: N/A; Machine rows cover facts | Required via existing log path |
| Real CLI `ruscv-sim run` | Required | N/A, not a new CLI flag | Required `--log-commits` |
| Flat `RiscVSimulator` facade | Required | N/A: no public subscriber toggle | N/A: no public logger; flat Machine rows cover on/serialization without calling them facade benchmarks |

Each applicable fixture × route × observation cell must produce these distinct layers; load-only observation toggles are N/A because no Hart executes:

1. **`load_only`**: fixture bytes already read; timer starts before `LoadImage::parse` and ends after `Machine::install`, including composition/allocation/placement. Retain installed owner; exclude drop/teardown, guest turns, fixture compilation, filesystem read, observer construction and oracle inspection. Metadata/initial bytes/PC/counter=0/no device effects checked after timer. Optional parse-only and install-only submetrics may accompany, never replace this combined layer.
2. **`execute_only`**: preloaded public Machine (native/flat) or flat facade; drain/reset/resume and sink/output buffers prepared before timer. Start immediately before the first actual turn/run, stop at the declared completed boundary after required synchronous observation delivery; timer contains no ELF parse/install/reset or owner destruction. Consume each receipt before next turn. Keep owner/results alive until after timer; inspect/assert outside it. Sinks consume immutable facts; no raw Executor/typed constructor substitute. Native convenience library/CLI cannot preload: their execute-only cells are N/A and must not be synthesized by subtracting load times. Machine execution is labeled Machine execution, not CLI execution.
3. **`end_to_end`**: actual CLI child launch through wait (including input read, load, run, artifacts, output/log flush and process teardown), actual native `load_and_run_file`, or flat construct+`load_elf`+`run`+result+drop. Native bytes-only `load_and_run` gets a separate label excluding file I/O. Include normal facade teardown; reject failed semantics. Never mix child-start costs into preloaded execution. No shell startup in CLI interval beyond declared direct child invocation; capture stdout/stderr and process status.

For file logs, declare sink buffering, filesystem/output location and flush boundary; host I/O cost belongs in the timed logging variant. Do not claim `fsync`/durability unless measured explicitly (not initially required). Native public library UART stdout and CLI capture differ from Machine callback capture: label both and compare only within identical sink policy. Fixture compilation and simulator build always occur before measurement. No host-writer concurrency in timed suite; existing A9 safety regressions remain tests, not nondeterministic timing workloads. Retain unknown-domain quarantine and refusal semantics; failed samples are not reset by fiat.

## 6. Proposed stable command and versioned outputs

**Future deliverable, not an existing command:** recommend one stable entry point:

```bash
./scripts/perf-test.sh run --suite public-v1 --profile calibrated --out target/perf/candidate
./scripts/perf-test.sh compare --baseline target/perf/base/report.json --candidate target/perf/candidate/report.json --out target/perf/comparison
```

Same command supplies a quick `smoke` profile (small correctness matrix, no comparative performance pass), a calibrated full profile and schema validation. It runs at a clean exact checkout, builds one release/all-features driver and real CLI outside timers with locked dependencies, assembles fresh isolated ELFs with required cross-tools, records argv/env and returns human summary plus structured JSON. Missing tools may emit an explicit unavailable/inconclusive report, never a passing measurement. Reject unsafe output paths using the [existing guard pattern](../../scripts/riscv_elf_paths.sh); no deleting source/baseline directories. No runtime CLI/output changes are required.

Recommend schema **`ruscv-perf/1`**, checked by a versioned parser with unknown major versions rejected. Deliver a documented JSON schema and round-trip/negative tests. Required fields:

- `schema`, suite/oracle/harness versions and digests, UTC run interval, exact full Git revision/tree and clean-state assertion; driver/CLI binary SHA-256, lockfile/features/profile, target triple, build argv/RUSTFLAGS/codegen/LTO/CPU flags.
- Full `rustc -Vv`, Cargo/rustfmt as relevant, cross assembler/linker versions and executable hashes, fixture source/linker/ELF SHA-256, source revision, entry/base/segment/tohost/signature identities and manifest build argv. No mutable tag as sole image identity.
- Container digest/platform (pin an approved repository development image, initially the A9 digest after confirming tools), or explicit native/no-container value; physical host CPU/model/architecture, OS/kernel, runner image/job URL if CI, logical CPUs, memory, virtualization/container status, affinity/governor/turbo policy where observable and missing-field reasons. Container identity alone is not host identity.
- Clock `std::time::Instant`, monotonic elapsed integer `ns`, observed resolution/empty timer calibration, no modeled clock conversion; ordered raw samples with sample/repetition IDs, route/phase/observation/sink/fixture IDs, start/end or duration, iteration/work sizes, warmup flag and all expected/observed oracle fields.
- Oracle verdict and diagnostics for **each repetition**, including guest vs process code, error/timeout, PC/signature, attempts/turns/traps/retirements, relevant effects, observation/log counts/digests and lifecycle generation/reset proofs where applicable. Load-only records state execution-not-started/count 0 rather than inventing exit.
- Distribution (median, p05/p95, MAD, bootstrap median confidence interval), sample count/discards with reasons, comparable-baseline key/revision, ratios only for comparable accepted samples, classification and bounded reasons. Raw failed/inconclusive samples stay in the report; never silently trim a semantic failure.

Recommend top-level semantic status separate from comparison status. Command exit 0 means valid report with all semantics correct; exit 1 semantic/schema failure; exit 2 measurement unavailable/inconclusive. Human output must print INCONCLUSIVE prominently. CI wrappers may publish exit-2 reports non-gating, not call them performance passes. Document these as the future command contract, not current Rust/CLI exits.

## 7. Measurement, comparison and retention policy

Initial practical defaults, to be calibrated and versioned during implementation:

- Serial sampling, no concurrent benches; release/all-features/locked build. Warm up each cell for at least 1 second and five correct iterations, cap 20 seconds with explicit insufficiency. Default 30 accepted samples/cell. Run three independent same-revision sessions to calibrate clock/noise and repeatability before accepting a comparative baseline.
- Calibrate toward at least 10 ms total timed work/sample without changing fixture semantics. Short fixtures may repeat independently restored runs; reset/setup/drop stay outside each execute-only interval, sum the actual execution intervals and retain each repetition/oracle. No hidden timer around a batch containing reset. Same repetition count/workload/sink policy at base and candidate. If resolution/overhead dominates or minimum samples cannot be obtained in the budget, mark inconclusive.
- Keep all raw samples; report outliers/MAD and confidence intervals rather than cherry-pick. Provisional relative CI half-width target 5% for a usable median; this is an evidence-sufficiency rule to test in calibration, **not a regression percentage gate**. When noise fails it, one bounded rerun then inconclusive with environment/load diagnostics. Persist the calibrated policy and any justified changes before final acceptance.
- Comparison key includes schema/suite/oracle/harness/fixture digests, phase/route/observation/sink, build settings, toolchain/container and compatible host/clock policy. Revision differs deliberately; CPU/OS/toolchain/fixture/policy changes require a new baseline series, not ratios against an incompatible artifact. Missing baseline, insufficient samples, unstable clock/noise or incomparable metadata explicitly classify `inconclusive`. Semantic failure takes precedence and blocks all favorable timing conclusions.
- Base and candidate builds at exact revisions in separate clean outputs, preferably alternate base/candidate sessions on the same runner allocation. Compare with unchanged fixture/oracle/harness identities. If a base predates the facility or cannot run it without runtime modification, report missing baseline and establish a new series; do not backport a different semantic engine or relabel tree equality as execution. Initial A10 calibration compares independently executed same-revision sessions; later PRs use the retained compatible base.
- Ratios/confidence intervals and suspected-change annotations are informational until calibration and a separate threshold decision. No universal percentage fail gate, optimization mandate or semantic waiver. Genuine correctness regression is a test failure independently of timing.

Recommend repository versioned suite/oracle/policy manifests plus compact baseline indexes identifying full revision, artifact URL and SHA-256 (not a Git copy of every large log). Full report, raw samples, stdout/stderr, logs, build/fixture manifests and environment metadata are checksummed artifact bundles. Use GitHub-hosted runners already available; no purchased or dedicated runner prerequisite. Weekly/manual runs retain 90 days, PR comparisons 30 days, selected baseline bundles 90 days with refresh before expiry; verify retrieval/digests during acceptance and record expiry/missing baseline as inconclusive. Repository compact baseline indexes retain history after expiry without pretending expired bytes are available. Never replace baseline bytes under an existing ID.

CI policy: preserve existing seven component controls/weekly evidence; add public suite weekly/manual reporting using the same command. Main correctness CI runs smoke/schema/negative-control tests with required tools. PR performance is explicit/manual, trusted-code-only, informational and reuses identical command/schema; never execute untrusted PR code with secrets or compare cross-run hosted-machine times blindly. A job reports revision/runner identity, semantic failures, inconclusive reasons and per-cell distribution; it cannot print a blanket performance PASS for N/A/inconclusive cells. Recommended runtime budget 30 minutes for calibrated public reporting, 45 minutes total if component controls also run; if exhausted, preserve partial raw results and mark affected cells inconclusive. CI permissions remain read-only apart from artifact upload.

## 8. Delivery tasks and falsifiable acceptance after approval

Task names and commands below are **proposed deliverables**, not existing passes. Implementation can refine ordinary Rust mechanisms within these boundaries; architectural/product or public compatibility changes require a separate decision.

| Task / prerequisite | Deliverable and executable exit |
| --- | --- |
| P0 — manifest and oracle, after approval/rotation | Pin the mandatory families in §4, exact ELF/tool/source identities and independently derived counts/signatures/results. Run each through native/flat applicable public routes. Mutated exit, PC/signature, work/retirement and device/atomic expectations must each fail. No missing future VP guest is required. |
| P1 — driver and phase boundaries, after P0 | Implement the one command and three distinct layers/matrix. Setup/drop clock-scope spies or deterministic driver tests prove no load/reset/teardown in execute-only and zero Hart turns in load-only. Real CLI process and both library facades exercised; no raw Executor substitute. Off/on and serialized logs match oracles; N/A cells explicit. |
| P2 — evidence/schema, after P1 | Valid schema and complete per-sample metadata/raw output; parser rejects missing identities, wrong schema, mismatched counts, semantic failures and unsafe output paths. Fixture build cannot reuse unproven stale ELFs. Failed reporting cannot become pass. Fresh lifecycle/repeated runs reproduce work; A9 negative safety regressions retained. |
| P3 — calibration/comparison, after P2 | Three correct full sessions at exact clean HEAD, ≥30 usable samples for each required measurement cell, retained baseline bundle and same-revision comparison via the same command. Publish timer/noise/sufficiency calibration. Synthetic missing baseline/changed host or tools/insufficient samples/noise tests return inconclusive, wrong semantic oracle returns failure. Ratios only on comparable cells. No speed target. |
| P4 — CI/retention/docs, after P3 | Smoke/negative/schema CI and weekly/manual public reporting; at least one actual exact-head CI artifact downloaded and digest/schema/oracle validated, selected baseline retrievable with declared retention/expiry. Document one local and one PR comparison procedure reusing the facility, N/A and inconclusive examples, seven component-control limits. Obtain independent final PR-head review and applicable CI. |

Final proposed acceptance requires all P0–P4 artifacts, a clean committed implementation head, bounded report with each mandatory cell present and correct, calibrated comparable baseline evidence and required CI/review. Unavailable required tools or measurements block infrastructure acceptance; demonstration of intentional inconclusive negative controls is required, not a substitute for obtaining the mandatory usable samples. No benchmark speedup is required. Run the repository full Rust gate:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

Also run required-toolchain A6 integration, fresh compile/run of the existing 58 project guests in isolated output, focused driver/schema/oracle/matrix/negative tests, output guards, full proposed `perf-test.sh run` and baseline comparison on the exact final head. Record commands/revisions/environment/counts in a bounded closeout. ACT4 remains frozen historical evidence, not a new Stage 3 requirement. No skipped tool or documentation-only SHA becomes runtime evidence.

## 9. Risks and approval boundary

Main risks are a false oracle, measuring setup instead of execution, overloaded hosted runners, expired baselines, and accidental semantic differences between logging/device configurations. Explicit per-sample semantics, clock scopes, matrix labels and inconclusive classification address these without weakening A9. Unknown completion still refuses reset/reuse/drain, quarantine lasts to process exit; clone-host nondeterminism and separate poll/clear, native/flat artifact/device differences, infallible borrowed edits and typed/core-only helper limits remain. Unix-only fault injection stays labeled; no new platform API is assumed.

Concrete approval requested: accept **this bounded infrastructure objective and policy** (small existing-capability suite, public routes and three phases, correctness-first schema, hosted-runner informational comparison, 90/30-day retention, calibration before any hard threshold) for later rolling activation, revise it, or defer. Not requested: Rust type choices, optimizations, new boards, later integrated feature contracts, runner purchase or runtime API breaks. Approval is not automatic debt inheritance or authorization to merge this documentation candidate. Until that decision and actual rotation, A9 stays active and all proposed commands/tests/schema/CI remain future deliverables.
