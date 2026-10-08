# A10 infrastructure progress — P0, P1 and P2 checkpoints

**Status:** Current

**Authority:** Informational; implementation and verification inventory, not an acceptance contract.

**Scope:** P0 correctness, P1 public-path phases and P2 local versioned evidence/schema. P2 is under new-head verification below; historical P0/P1 approval does not approve it. This is not A10 completion, an ACT4 claim or a performance verdict. P3 calibration/comparison and P4 CI/retention/final acceptance remain unimplemented.

**Contract references:** [Current A10 contract](../dev-plan.md), accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md). [A9 closeout](a9-closeout-assessment.md) remains the historical safety baseline, not performance evidence.

## Landed milestone activation

[PR #71](https://github.com/mimiqdev/ruscv-sim/pull/71) merged **2026-10-08T02:53:47Z** as `e73b12b8467fd635b398382a5cbc7ce75d842f68`, formally completing/archiving A9 and activating A10 as the sole Current contract. The reviewed rotation head was `d78f212e70fab43362febf43d3055c920db3cf69`. Their tree/content equality is documentation provenance only, not runtime, ACT4 or performance verification. [Rotation evidence](https://github.com/mimiqdev/ruscv-sim/pull/71#issuecomment-6051203681) records the bounded documentation review/CI; Coverage, standalone release and ELF jobs were skipped there. Archived contract/proposal bodies and historical JSON are unchanged.

P0 starts at that immutable merge baseline. Implementation commits are `56cf5b945343b03545c4cef2ad11c88e9a2dc530` (fixtures/oracles/public-route harness) and `02ba43863a00c6e29f482f299d515df6a5905824` (retirement-reader underflow regression). Git history identifies the subsequent checkpoint-document revision; no document can embed its own commit hash. Verification and review must bind to the exact committed PR revision, not merely these implementation revisions.

## Repository artifacts and public API inventory

- [`tools/a10/public-v1.json`](../../tools/a10/public-v1.json): `a10-oracle/1`, version 1; 12 fixture identities, capability matrix/N/A reasons, source/linker/ELF SHA-256, assembler/linker/disassembler/symbol-tool versions and ARM64 executable hashes, image digest, build flags, ELF entry/load/zero-fill/signature/tohost metadata, final registers/RAM/device output and complete instruction/effect expectations. Manifest SHA-256: `89a89bfb960484079468d419d51639b60d439561828a4e3516abd0829c627733`.
- [`build_fixtures.py`](../../tools/a10/build_fixtures.py): assembles/links into a **new** output directory under `target/`, recording exact commands/tool identities, ELF hashes, disassembly and symbols in `build.json`. No execution, simulator output recording, reuse or cleanup of historical ELFs.
- [`derive_oracles.py`](../../tools/a10/derive_oracles.py): offline assembly/algorithm accounting plus ELF64 placement audit. `--check` compares the complete derivation with the checked-in manifest without changing it. Explicit `--check-artifacts` audits identical source/linker/ELF bytes and all semantic/placement expectations while excluding only producer-tool identities; it never certifies the pinned environment. It reads linked opcode identities; it is **not** an instruction decoder, executor or alternate semantics engine.
- [`mod.rs`](../../tools/a10/mod.rs): reusable `Manifest`, `Fixture`, `Sample`, capability checks, `image_identity`, `expected_log` and `validate`. `validate` returns semantic rejection or explicit unavailable evidence, never elapsed-time success. P1 must call this same validator on **every** independently captured repetition, including warmup/calibration; P0 contains no clock or phase control.
- [`routes.rs`](../../tools/a10/routes.rs): actual flat-facade and public subprocess adapters, strict CLI capture parsing. [`probe.rs`](../../tools/a10/probe.rs), exposed as `a10-p0-probe`, is a narrow child transport for native-library `ExecutionResult` plus real UART stdout; it is not a measurement driver or alternate execution path. Existing serde dependencies suffice; crate version/dependencies and production sources are unchanged. `default-run = "ruscv-sim"` preserves the existing `cargo run -- ...` entry point.
- [`a10_p0_oracles.rs`](../../tests/a10_p0_oracles.rs): route matrix, direct immutable-effect/state assertions, negative controls, public file failure and lifecycle tests. [`verify_p0.sh`](../../tools/a10/verify_p0.sh) is explicitly P0 verification/setup plumbing, not the P1 stable `perf-test.sh` command.

Actual adapters use:

| Route | Public execution/capture APIs | Own per-sample oracle |
|---|---|---|
| CLI | `ruscv-sim run`, optional `--log-commits` | Declared guest/process code, exact completed turns/final PC, no timeout/error, signature address/size, UART, optional exact text log, guest self-check path |
| Native bytes/file | `executor::load_and_run` / `load_and_run_file` | Their own `ExecutionResult`, signature bytes/work/checksum/reader where declared, actual UART stdout, optional exact text log |
| Flat facade | `RiscVSimulator::new`, `load_image`, `run`; public state/memory reads | Own result/signature and final GPR/x0, RAM/neighbors/BSS, MINSTRET and cleared selected signal |
| Native/flat Machine | `LoadImage::parse`, `Machine::new/install/resume/step`, `MachineTurn::deliver`, `ObservationSink`, `CommitLogger::new_file`, `inspect` | Own public turns, immutable commit/effect order when enabled, final GPR/RAM/CSR/device/signal/events and signature |

No raw Executor/typed atomic/device helper replaces a public route. Machine snapshots supply **final state**, never reconstructed commits. Commit identities and effects come from delivered immutable Hart records; capture also checks hart ID, instruction length, privilege, MINSTRET/CSR progression, absence of unexpected FPR/FCSR effects, atomic indivisibility, and native/flat issued-address adaptation.

## Workloads and independent derivation

Existing sources are unchanged: `rv64i/fib.S`, `sw.S`, `hello.S` and `rv64a/{amo_w,amo_d,lrsc_loop,sc_after_store_failure}.S` under [`tests/bare-metal-riscv-test`](../../tests/bare-metal-riscv-test). New bounded sources/linker live only in [`tools/a10/fixtures`](../../tools/a10/fixtures).

All selected sequences complete without synchronous traps: **attempts = completed turns = retirements**, traps = 0. This is a property of these fixtures, not a redefinition of `ExecutionResult.cycles`: generally that field counts completed synchronous traps too. All instruction counts include immediate/address expansions, counter reader, signature stores and the completing exit store. No guest writes MINSTRET.

| Fixture | Completed turns | Declared work | Checksum/result | Guest/process code |
|---|---:|---:|---:|---:|
| fib | 68 | 9 recurrence iterations | 55 | 0 |
| sw | 103 | 12 ordinary stores | 4,868,748,099 (final word sum) | 0 |
| hello | 58 | 7 UART bytes | 543 (byte sum) | 0 |
| amo_w | 29 | 4 AMOs | 6 | 0 |
| amo_d | 28 | 4 AMOs | 4 | 0 |
| lrsc_loop | 16 | 1 LR/SC success | 1 | 0 |
| sc_after_store_failure | 20 | 1 invalidated SC | 2 (unchanged by failed SC) | 0 |
| control_loop | 155 | 32 iterations | 528 | 3 |
| ram_loop | 142 | 16 store/load iterations | 136 | 3 |
| mixed_w | 191 | 8 mixed iterations | 24 | 3 |
| mixed_d | 191 | 8 mixed iterations | 24 | 3 |
| native_device | 6 | 1 UART byte | 65 | 3 |

Examples of independent linked-sequence accounting:

- fib: 4 setup + 9 × 6 loop + 1 terminating branch + 2 self-check + 7 exit = 68; recurrence is independently expanded in the derivation.
- hello: 1 setup + 7 × 7 call/poll/write + 1 exit branch + 7 exit = 58. Output is exactly `Hello!\n`.
- sw: 97 successful-path instructions + 6 exit = 103. Explicit immediate expansions and each store/read value are enumerated, not interpreted by another executor.
- Atomic anchors: 24/23/11/15 successful-path instructions respectively, plus 5 exit = 29/28/16/20.
- control: 3 + 32 × 4 + 24 = 155; sum 1…32 = 528.
- RAM: 9 + 16 × 7 + 21 = 142; sum 1…16 = 136.
- Mixed W/D: 10 + 8 × 20 + 21 = 191; each iteration leaves 3, checksum 8 × 3 = 24.

Each new RAM/control/mixed fixture checks its own checksum, initialized neighbor (`0x123`), BSS scratch and zero neighbor, then writes three 64-bit signature words: work, checksum and the MINSTRET value **before** the reader retires. Six instructions remain counting that reader, so reader values are 149/136/185/185, not total retirements. The checker compares the observed bytes both to the declared signature and independently to work/checksum/retirement accounting. Guest exit payload 7 declares code 3; that nonzero code is correctness success only when all declared evidence matches. An injected checksum fault must take the failure path on every real public route.

The mixed fixtures cover ordinary store, sign-extended AMO.W old value `-2`, AMO.D old value, LR, successful SC, ordinary-store-invalidated failed SC, failed SC with suppressed x0 destination, and unchanged neighboring bytes. Old/read/new bytes, statuses and reservation transitions are pinned per turn; absence of a failed-SC write is distinct from a write of unchanged bytes.

## Applicable routes and explicit N/A

The test validates **148 fixture/route/mode cells**: 10 device-independent fixtures × 13 cells, plus 2 native-only fixtures × 9. Totals: native Machine 36, flat Machine 30, native bytes 24, native file 24, CLI 24, flat facade 10.

- Machine native/flat: `off`, `facts`, `file`. Facts are delivered through the public sink; file mode chains those facts and public file logger on the same turn.
- CLI and native bytes/file: `off`, `file`; arbitrary subscriber-only mode is N/A because there is no public sink argument.
- Flat facade: `off`; logging and subscriber toggles are N/A, not zero-cost successes.
- `hello` and `native_device`: flat routes N/A because flat memory has no native device map.
- CLI/native ELF-loading routes do not expose preload/execute-only operation; no P0 phase is simulated around them.
- In-memory serialized writer mode is deferred/N/A: no supported public arbitrary writer constructor is invented.

CLI exposes signature address/size, **not bytes or GPR/RAM/counter internals**. Its oracle is its own self-check path, exact instruction-work count/result and UART/log evidence. No native/flat companion signature is copied into a CLI sample. Native libraries expose signature bytes but not arbitrary final state/facts. Direct Machine/flat assertions strengthen route-specific evidence without blessing a wrong CLI/library result. Anchors without signatures declare their existing guest self-check/result and direct-route state/effect oracles; no extra native-only introspection is claimed.

The native fixed-HTIF fixture produces ordered events `uart(65)` then `htif(7)`; RAM-tohost guests complete through the selected ELF symbol instead. Native Machine preserves the completing signal, flat Machine preserves its relocated RAM value, and the flat facade clears it after completion. These are explicitly different public policies, not fixes or normalization.

Text logs are a separate exact presentation oracle: equal-value GPR writes are omitted by the real logger, while immutable facts retain them. No memory effects are fabricated merely to fill a text record. Observation off produces no facts; on/file require exact full per-turn effects and order. Missing mandatory facts/log/result/state is **Unavailable**, never a correctness pass.

## Focused regressions and negative/lifecycle evidence

Nine tests in `a10_p0_oracles.rs`:

1. `versioned_manifest_and_capabilities_fail_closed`: version/schema, required capabilities/N/A and manifest expectations.
2. `all_mandatory_fixtures_own_their_public_route_oracle`: all 148 cells, independent captures and applicable negative mutations for result/code/PC/signature/counters/GPR/x0/RAM/UART/device/signal/events/log/facts; missing evidence fails closed.
3. `atomic_and_observation_mutations_reject_real_machine_facts`: AMO old/new bytes, ordinary stores, LR/SC success/failure/reservation effects and order, suppressed x0 writes and fabricated failed-SC memory effects.
4. `receipt_drain_live_reservation_bss_stale_handles_and_equivalent_rerun`: native/flat observations off/on; retain receipts to refuse next turn/mutation/install/reset/teardown/drain, then consume/drop them. Request quiescence, prove drain, reset live reservations, compare restored full image/BSS/UART/signal/register/PC/counter state, reject stale handles and validate a fresh equivalent rerun.
5. `malformed_cli_capture_is_not_a_correctness_pass`: malformed/missing CLI evidence is rejected, not filled from a companion.
6. `p0_oracle_audits_and_regressions_are_not_simulator_recordings`: fresh-source derivation audit and discovered UART/capture regressions.
7. `corrupted_guest_and_mutated_work_expectations_fail_real_public_samples`: actual faulty linked guest fails every route; wrong work/checksum/reader expectations and invalid reader underflow reject real captures.
8. `public_file_write_failure_never_becomes_a_correct_sample`: Unix `/dev/full`, using only public logger construction; native library, CLI and both Machine kinds reject failed reporting while preserving the already-retired first instruction and receipt/drain obligations.
9. `cli_signature_metadata_from_real_capture_is_exact_and_unique`: validate a real CLI capture, retain its signature size, then reject wrong size, conflicting/identical duplicate fields, malformed complete fields and missing signature metadata. The same validator checks size on every applicable CLI matrix sample; no substring search or companion bytes can bless contradictory evidence.

Development failures were reproduced and repaired **only in the harness**:

- Signature/tohost sections initially lacked allocatable writable flags; corrected fixture placement is pinned and audited.
- hello initially expected UART LSR `0x60` for every poll. Existing UART retains TX bytes: only the first read is `0x60`, later reads are `0x20`. The source-derived oracle and regression pin that contract, without changing the device.
- Flat Machine's empty UART capture was initially not checked; a mutation now proves rejection.
- A zero retirement expectation underflowed `retirements - 6`; the reproducer panicked. Checked subtraction now returns semantic rejection, covered by the same real-sample negative test.
- CLI capture formerly discarded signature size and accepted an expected metadata line anywhere in stdout, allowing a contradictory zero-byte field followed by the correct field. A real-capture wrong-size regression reproduced the false pass. Parsing now requires exactly one complete canonical optional field, retaining both address and size for validation; duplicate/malformed fields fail during capture and size mismatch fails the shared validator.
- [PR #72 CI run 37724337992](https://github.com/mimiqdev/ruscv-sim/actions/runs/37724337992) failed the harness at `931fe310a90a250952d1dad79aaa2c6697c734de`: Ubuntu binutils 2.42 produced the exact pinned ELF hashes but differed from the development image's 2.40 version string. Ordinary cross-toolchain correctness now explicitly checks artifact equivalence and records actual producer identities, without claiming tool pins. Required checkpoint verification remains strict under `RISCV_REQUIRE_A10_PINNED_TOOLS=1`: exact version and ARM64 tool-byte identities plus the complete `--check` audit. A fresh-build metadata-mutation regression proves that portable audit accepts identity-only differences, strict audit rejects them, and **both** reject changed ELF identity. No CI workflow/performance facility changed.

Existing A9 tests remain unchanged. Their separate Hart/lifecycle/facade/CLI suites are included in the exact-HEAD verification command, in addition to the full Rust suite. Unknown physical completion remains quarantined; no forced drain/recovery, guest trap repair, fixed-width fetch change or host nondeterminism suppression is added.

## Verification identity, reproduction and remaining gate

Development observation on 2026-10-08: the repaired focused suite ran **8 passed / 0 failed / 0 ignored**, printed all **148 validated cells**, and strict all-features/all-targets Clippy passed in the pinned image. The compiled harness corresponds to implementation `02ba438…`; the worktree had a pending documentation clarification. This is **development evidence only**, not the final clean-HEAD checkpoint gate. An earlier all-suite invocation exceeded the outer command timeout; its incomplete output is not recorded as a pass.

Subsequent **clean committed** container verification at `b13b7c20620b39d2266fdb4cf8878e90aebbf1a0` passed the full wrapper (2026-10-08T03:29:21Z–03:34:24Z), including fresh artifacts, full Rust gates, 148-cell focused suite and A9 suites. Local output was `target/a10-p0-verify-XoIdwH/verification.log`; fresh build-report SHA-256 was `8f97daa860e69bab5305a801354041410047488e106050e112d78541e396bf7f`. The separate host six-check run passed five requirements but **failed strict Clippy**: Rust 1.98 introduced `map_or_identity` on a harness-only `lr.map_or(0, |i| i)`. It was replaced with `lr.unwrap_or(0)`, not suppressed. The changed checkpoint HEAD must rerun **all** gates; the earlier passed container observation is not carried forward.

At `931fe310a90a250952d1dad79aaa2c6697c734de`, all six host requirements and the fresh pinned-container wrapper passed and were recorded at the clean committed HEAD; [public evidence](https://github.com/mimiqdev/ruscv-sim/pull/72#issuecomment-6051768319) includes command/environment/binary/ELF identities. The subsequent ordinary-CI producer-identity failure described above required a same-scope harness repair. That record remains historical, **not** evidence for the repaired PR head. [PR #72](https://github.com/mimiqdev/ruscv-sim/pull/72) carries the successive bounded infrastructure checkpoints; those earlier records do not approve a later revision or authorize merging.

The independently reviewed P0 revision `5d3a6ce7b32f721316e6688be440ac77d4abf1cf` had passing [public verification evidence](https://github.com/mimiqdev/ruscv-sim/pull/72#issuecomment-6051979166) and [CI Quality and tests](https://github.com/mimiqdev/ruscv-sim/actions/runs/37725234480/job/113141781010), with Coverage skipped. Static read-only review nevertheless found the CLI signature ambiguity and non-public reproduction instructions in this document. Both are addressed in the source/tests and repository-only commands below; earlier-head verification is historical, not proof of the repair revision.

Environment inspected: Docker image `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`, Linux/aarch64, Rust/Cargo 1.97.1, GNU RISC-V binutils 2.40. Host checks are separate: Darwin/arm64, Rust/Cargo 1.98.1. A host test run without a cross toolchain explicitly prints **UNAVAILABLE** for those fixtures; its process success cannot substitute for required-toolchain container evidence.

From a clean committed P0 HEAD, reproduce the bounded toolchain/artifact/public-route verification with:

```bash
bash tools/a10/verify_p0.sh
```

The wrapper uses non-login `bash -c`, `CARGO_BUILD_JOBS=2`, `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`, `RISCV_REQUIRE_A10_PINNED_TOOLS=1`, `--locked`, a new `target/a10-p0-verify-XXXXXX/cargo` build output, and a separate fresh fixture output. It runs fmt, all-features check, strict all-target Clippy, all-features tests/docs, focused P0 with `--nocapture`, A9 regression suites, derivation audit, baseline-to-HEAD diff check and unchanged production/historical-source scope audit. It records source HEAD/tree, timestamps, OS/tool/image identities, actual source/manifest/Cargo.lock/binary/ELF/build-report hashes and logs under that local ignored output. It refuses a dirty worktree and checks that HEAD/tree cleanliness did not change during execution; it deletes nothing.

The separate host Rust quality gate and baseline whitespace audit are repository-owned commands:

```bash
git rev-parse HEAD
git status --porcelain
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
git diff --check e73b12b8467fd635b398382a5cbc7ce75d842f68..HEAD
```

These instructions are **not claims of a passed repair revision**. Record the full committed revision/tree and clean state with each command's output, environment and artifact hashes in public PR verification evidence. Run the pinned-container wrapper as well: a host test result with missing cross tools cannot establish the public-route fixture assertions. Any later source change invalidates prior verification and requires fresh checks and independent PR-head review under [repository guidance](../../AGENTS.md).

P0 coverage is limited to the correctness artifacts above. P1 adds the bounded timing driver below, not a calibrated performance verdict, full repeated-run schema, comparison mode, benchmark CI, full 58-guest rebuild matrix, final nine-command P4 acceptance, merge or release.

## P1 — actual phase driver and smoke command

The reviewed P0 boundary is `29bc58729ade28d611d17c524a26bd3369a64140`: [checkpoint evidence and zero-finding independent review](https://github.com/mimiqdev/ruscv-sim/pull/72#issuecomment-6052545885). It is a historical correctness checkpoint, not verification of a P1 revision. P1 implementation is in [`phases.rs`](../../tools/a10/phases.rs), [`driver.rs`](../../tools/a10/driver.rs), [`command.py`](../../tools/a10/command.py), [`audit_fixtures.py`](../../tools/a10/audit_fixtures.py) and [`scripts/perf-test.sh`](../../scripts/perf-test.sh). Production APIs/semantics, P0 manifest/fixtures/oracles, accepted contracts/ADRs, historical guests/evidence, Cargo dependencies/version and CI workflows are unchanged.

From a clean committed checkout with Rust and the cross tools available:

```bash
./scripts/perf-test.sh run --suite public-v1 --profile smoke --out target/perf/p1-new
# Optional basic repetitions (1..16); every cell also has one warmup:
./scripts/perf-test.sh run --suite public-v1 --profile smoke --out target/perf/p1-other --repetitions 1
```

Each output must be a **new** named directory below canonical `target/`. Existing output, protected/source paths, parent escapes, symlinked roots/ancestors, missing tools, dirty/uncommitted source, mismatched source revisions and stale fixture/source/linker/ELF identities are refused; nothing is deleted or overwritten. Setup builds release/all-features driver, real CLI and P0 transport with locked dependencies in isolated output, then freshly builds/audits the unchanged P0 fixtures **outside all phase clocks**. Source/tree/argv/UTC/tool/build/binary identities are recorded in `setup.json`. Python setup disables repository bytecode caches so it cannot dirty its own evidence checkout.

### Concrete boundaries and capture policies

| Route/phase | Timed scope | Outside the interval |
|---|---|---|
| Native/flat Machine `load_only` | Pre-read bytes → `LoadImage::parse` → `Machine::new` allocation/composition → `install`; owner retained | Callback/buffer preparation, filesystem read, image/segment/signature metadata checks, complete initial RAM/zero-fill/PC/GPR/MINSTRET/device/signal checks, drain/teardown/drop |
| Native/flat Machine `execute_only` | First real `step` through terminal boundary, synchronous checked sink/file delivery and consumption of every receipt | Parse/install, requested/proven drain, `fresh_reset`, full restored-image checks, `resume`, logger/sink/buffer preparation; log close, final inspection, P0 validation and destruction |
| Preloaded flat facade `execute_only` | Actual public `run`, including its own result/artifact/selected-signal policy | Construct/load, public coordinated `fresh_reset`, full initial image/BSS/GPR/counter check; direct final-state copy, P0 validation and drop |
| Flat facade `end_to_end` | **One continuous** construct → `load_elf` → real `run`/`ExecutionResult` → owned minimum final GPR/RAM/MINSTRET/selected-signal copy via public live APIs → actual normal facade drop | Fixture disk read/hash/build and metadata preparation; semantic expectation comparison and JSON/report assembly |
| Native bytes/file `end_to_end` | **In the child:** actual `load_and_run` (bytes pre-read) / `load_and_run_file` (its file I/O included), normal API-local teardown and explicit UART stdout flush | Probe startup, bytes-route input read, argument preparation, structured transport JSON/exit, parent wait/parsing/log readback/validation |
| CLI `end_to_end` | Direct `Command` launch through wait: normal CLI input/load/run/artifacts/output/log/process destruction plus stdout/stderr pipe capture | Command/argv preparation, build/hash; report parsing, log readback, P0 validation and outer JSON |

The flat continuous interpretation includes allocation/copy cost needed to preserve **that same run's** complete own oracle before destruction. It is explicitly **not** pure execution or a capture-free facade cost: scope and `flat-live-owned-final-copy-before-drop/1` identity appear in every corresponding row and aggregation key. Retained data is plain owned `Sample` values, never a facade/Memory/Machine/Arc handle or receipt postponing drop. There is no pause/restart, subtraction or segmented sum in this end-to-end interval. Capture errors preserve available owned public-result data but invalidate timing. Compare only identical route/phase/capture policy; the native call and CLI child policies are distinct.

`MachineRun::prepare`, `execute`, `close_log` and `inspect` split the old P0 capture wrapper without changing its validator. The wrapper remains available to all P0 regressions. Off has no materialized/delivered facts; on/file consume immutable Hart observations, not snapshot/refetch reconstructions. The public file logger uses synchronous unbuffered `File` writes: delivery/write errors reject the sample, close is outside Machine execute-only, and no buffered deferred flush or `fsync` guarantee is claimed. CLI/library file lifetime is included in their actual public call/process scope. Native-library UART is flushed explicitly before its child function-scope clock stops; serialization remains after stop.

### Matrix, repetitions and eligibility

**180 applicable phase cells:** 22 Machine load (no observation variant), 66 Machine execute, 10 flat-facade execute, 82 real CLI/native-bytes/native-file/flat end-to-end. This exercises all 148 P0 fixture/route/mode cells through their actual APIs. **324 explicit N/A cells** cover unsupported phase/mode/device combinations: convenience CLI/library preload/execute, non-Machine combined load, Machine end-to-end not included as a facade metric, unsupported subscribers/flat logger, and native devices on flat configurations. Load has mode `none`, execution-not-started and zero turns/counter, not an invented guest exit.

Default smoke uses **one warmup and two basic repetitions per applicable cell**: 540 independently validated intervals plus 324 N/A rows. Every repetition—including warmup—uses the same reusable P0 `validate`; preflight/companion success cannot bless a later result. Fresh reset/resume and initial-state proof precede every preloaded execution. Basic interval sums group only identical fixture/route/phase/mode/capture policy; setup/reset is never in a timed batch and warmups are retained but excluded from the basic sum. Any bad warmup/basic, transport/reporting/capture failure, or unavailable mandatory evidence invalidates its aggregate. Failed evidence is retained; that cell stops without automatic retry/reuse. Unknown completion remains quarantined, never forced to drain/reset.

At the historical reviewed P1 head `766c223be36520a4555b6e98e66a95e4d797d22f`, `report.json` was deliberately **`a10-p1-checkpoint/1`, not full `ruscv-perf/1`**. The following status/interface paragraph describes that P1 checkpoint, not the current P2 command contract below. It retains raw per-repetition UTC/monotonic-ns intervals, origins, actual owned oracle fields/verdicts, CLI/native argv/raw stdout/stderr/logs, load evidence, N/A reasons, scope/capture identities and separate semantic/measurement statuses. Invalid/nonmonotonic scopes have their own `scope_error`: the same run's P0 oracle can still be correct while timing/aggregation is unavailable; a clock failure is not falsely labeled an ISA failure. [`check_smoke.py`](../../tools/a10/check_smoke.py) checks bounded retained bookkeeping only, not a full P2 schema/identity parser or second semantic engine. Measurement/comparison eligibility is always **INCONCLUSIVE: smoke uncalibrated**. Exit 0 means the bounded report's semantics are correct, never a speed pass. Failure is nonzero; calibrated/compare/schema commands explicitly return unavailable (2). No 30-sample sufficiency, three-session calibration, baseline ratios, retention/index guarantees or performance CI is implemented.

### Scope and negative tests

[`a10_p1_phases.rs`](../../tests/a10_p1_phases.rs) contains six focused tests. Injectable clocks/events are owned by the harness and placed at the actual public operations; the real matrix runs in addition to deterministic weighted clocks. `ordered_real_flat_scope_includes_owned_copy_and_actual_drop` proves start/construct/load/run/final-copy/drop/stop/validate/report order. Mutations of those actual traces reject missing drop/copy, timed validation/report/reset/read, execute-only parse/install/prepare/inspect/drop/log-close contamination, load-only guest steps and unconsumed receipts. Reset/inspection have large deterministic costs outside the measured sums. Native tests prove no parent clock surrounds probe launch and reject changed scope/origin/interval metadata.

`bad_warmup_or_basic_repetition_cannot_be_blessed_by_preflight` injects wrong exit/PC/signature/work/counter/RAM/x0/device/facts or delayed reporting error into actual captured repetitions after a successful preflight; no rejected row or aggregate receives accepted timing. Real malformed-load, final-copy failure, nonmonotonic-clock and timeout paths retain failure evidence, exclude load cleanup from its clock and exercise flat normal drop; public `/dev/full` proves file errors cannot become success. Retained Machine receipts refuse mutation/drain, consumption permits proven drain/restoration. Existing nine P0 tests (148 cells) and A9 Hart/lifecycle/facade/CLI regressions remain verification requirements.

### Evidence and reproduction

Development-only observation: clean implementation revision `737e8e5dc9dfc72bf5ca13ed6e801371e35d23f0` ran the release command in the pinned Linux/ARM64 image, Rust/Cargo 1.97.1, GNU binutils 2.40, strict pinned producer identities. It produced **540 correct raw intervals / 180 cells / 324 N/A**, with measurement status **inconclusive-smoke-uncalibrated**. Local `target/perf-p1-development-737e8e5/report.json` SHA-256: `fede3cb9930f2e253cac38fc7d91ca6bf06d5f514ce958b4d8be20c1ea3b7399`. This predates the final documentation/verification revision and is not carried forward as final checkpoint evidence. Initial container attempts failed before measurements because a linked worktree's absolute Git paths/ownership were unavailable; no failed setup was relabeled passed.

Reproduce the full exact-committed-revision P1 verification, including fresh isolated Rust and release-smoke builds, with:

```bash
bash tools/a10/verify_p1.sh
```

The wrapper runs the Rust gate, focused P1/P0 suites, A9 regressions, unchanged-source scope audit, real smoke command and bounded report audit, retaining logs/hashes locally under a new `target/a10-p1-verify-*`. It mounts the worktree at its original absolute path and common Git metadata **read-only**, disables optional index writes and sets `safe.directory` for only that process/explicit checkout (no global/wildcard config). Non-login shell, two Cargo jobs, required cross tools and pinned ARM64 tool identities are explicit. That historical P1 wrapper applies at its exact checkpoint, not the current P2 source/exit contract. Its missing physical-machine metadata was a P2 limitation; container identity alone was not calibrated-host proof. Use the new P2 wrapper below on current source.

Public PR evidence must identify the actual new committed HEAD/tree, applicable CI, command output and artifact hashes; source changes require new verification and independent read-only review. P1 ends with that bounded historical driver/checkpoint. Its [durable zero-finding review/evidence](https://github.com/mimiqdev/ruscv-sim/pull/72#issuecomment-6056645684) binds `766c223be36520a4555b6e98e66a95e4d797d22f`, not any new P2 head. P3/P4 and final A10 acceptance remain separate work.

## P2 — versioned local evidence/schema (new-head verification pending)

[`a10-perf-schema.md`](a10-perf-schema.md) documents the full `ruscv-perf/1` format, current commands, statuses and explicit limits. [`ruscv-perf-1.schema.json`](../../tools/a10/ruscv-perf-1.schema.json) is the machine-readable definition; [`report_schema.py`](../../tools/a10/report_schema.py) validates strict unique-key finite typed JSON, complete identities, accounting and sealed retrieval; [`replay.rs`](../../tools/a10/replay.rs) replays every retained actual sample through the same unchanged pinned P0 validator and phase/lifecycle invariants. The former provisional transport is separately retained as `raw-report.json`, never accepted or renamed as a baseline.

```bash
./scripts/perf-test.sh run --suite public-v1 --profile smoke --out target/perf/p2-new
# A semantically correct smoke run returns 2 / INCONCLUSIVE, not performance PASS.
./scripts/perf-test.sh validate target/perf/p2-new/report.json
# Reader exit 0 = schema/integrity/own-P0 validity only; performance inconclusive.
bash tools/a10/verify_p2.sh
```

P2 adds versioned suite/policy/harness/schema identities, complete source blob/tree/commit snapshots and race checks, actual verbose release build/tool/binary/fixture identities, qualified execution/physical-host/container inspection, clock controls, unique ordered raw rows, own transport/effect/log/facts digests and initial image/BSS/counter/reservation/device/generation/drain/reset/stale-isolation proof where public. Nullable unavailable introspection has precise reasons, never fabricated counters, companion state or new APIs. Flat capture-inclusive continuous timing and all P1 scopes remain unchanged. Warmups/basic failures and unstarted successors are retained; no trimming or favorable sum over a bad warmup.

Per-cell descriptive median/p05/p95/MAD, counts/discards and exact identity keys are available. Bootstrap confidence, calibrated sufficiency, accepted baseline/revision, ratios and performance classification remain explicitly INCONCLUSIVE/deferred. Immutable exclusive-created local checksummed bundles enforce safe relative owned references, source reconstruction and retrieval/digest checks; reporting errors cannot become success. No P3 calibration/comparison or P4 CI/index/upload/retention change is included.

Focused [`a10_p2_schema.rs`](../../tests/a10_p2_schema.rs) exercises actual public-route round-trip/oracle replay and result/effect/lifecycle/scope/clock/identity negatives. [`test_schema.py`](../../tools/a10/test_schema.py) covers strict widths, duplicates, non-finite values, required identities, immutable guards/retrieval/reporting failures and descriptive statistics; [`test_bundle.py`](../../tools/a10/test_bundle.py) exercises fresh actual sealed release evidence and reader mutations. P0/P1 matrices/spies and A9 lifecycle/safety regressions remain required. The new strict pinned-container wrapper records actual outer-host and container/image inspection, runs the full Rust gate/focused suites, fresh release smoke, retrieval/negative controls and fixed-base scope audit.

Development release smoke at `16f05c1e686c2a289028b971ff27045909a60509` / tree `3aa68cf5eefe4ddafab3a2025984dcdec303bc02` ran 2026-10-08T11:03:22Z–11:04:21Z in the inspected pinned Linux/ARM64 image, with separately collected native Darwin/arm64 host observations. It retained **540 correct independently validated intervals / 180 phase cells + 324 N/A**, `semantic_status=correct`, `comparison_status=inconclusive`, smoke exit 2, sealed retrieval/reader exit 0 and **50 negative reader controls**. Report SHA-256 `b98c1b7989ef5524923467bc50972113482024e86903b5829a6d2f48cd30b049`; bundle digest `6aa38b957c3315200aa05242ffe1d7a868d185de0d9ef80b15bbbcae3d542ce1`. Local artifacts are not portable accepted baselines. This development-head evidence is historical, not proof for the final subsequent PR revision.

Required final exact committed-head evidence must be recorded on [PR #72](https://github.com/mimiqdev/ruscv-sim/pull/72): full Rust gate, focused 5 P2 + 9 P0 + 6 P1 tests, 11 Python schema controls, 50 release reader negatives, A9 regressions, fresh strict pinned-container smoke/artifact audits and fixed-base preserved-source diff. Source changes require new verification; prior-head logs or review cannot approve a new revision. No independent P2 review acceptance, P3/P4, calibrated baseline, speed verdict, merge or A10-completion claim is made here.
