# A10 infrastructure progress — P0 checkpoint

**Scope/status:** P0 correctness infrastructure only. Implementation is committed; independent checkpoint review and exact-clean-HEAD gates are still required before P1 authorization. This is not A10 completion, an ACT4 claim, or a performance verdict. P1–P4 (measurement phases, stable command/schema, comparison and CI/final acceptance) are not implemented here.

**Authority:** [Current A10 contract](../dev-plan.md), accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md). [A9 closeout](a9-closeout-assessment.md) remains the historical safety baseline, not performance evidence.

## Landed milestone activation

[PR #71](https://github.com/mimiqdev/ruscv-sim/pull/71) merged **2026-10-08T02:53:47Z** as `e73b12b8467fd635b398382a5cbc7ce75d842f68`, formally completing/archiving A9 and activating A10 as the sole Current contract. The reviewed rotation head was `d78f212e70fab43362febf43d3055c920db3cf69`. Their tree/content equality is documentation provenance only, not runtime, ACT4 or performance verification. [Rotation evidence](https://github.com/mimiqdev/ruscv-sim/pull/71#issuecomment-6051203681) records the bounded documentation review/CI; Coverage, standalone release and ELF jobs were skipped there. Archived contract/proposal bodies and historical JSON are unchanged.

P0 starts at that immutable merge baseline. Implementation commits are `56cf5b945343b03545c4cef2ad11c88e9a2dc530` (fixtures/oracles/public-route harness) and `02ba43863a00c6e29f482f299d515df6a5905824` (retirement-reader underflow regression). Git history identifies the subsequent checkpoint-document revision; no document can embed its own commit hash. Final verification must bind to the actual committed handoff HEAD, not merely these implementation revisions.

## Repository artifacts and public API inventory

- [`tools/a10/public-v1.json`](../../tools/a10/public-v1.json): `a10-oracle/1`, version 1; 12 fixture identities, capability matrix/N/A reasons, source/linker/ELF SHA-256, assembler/linker/disassembler/symbol-tool versions and ARM64 executable hashes, image digest, build flags, ELF entry/load/zero-fill/signature/tohost metadata, final registers/RAM/device output and complete instruction/effect expectations. Manifest SHA-256: `89a89bfb960484079468d419d51639b60d439561828a4e3516abd0829c627733`.
- [`build_fixtures.py`](../../tools/a10/build_fixtures.py): assembles/links into a **new** output directory under `target/`, recording exact commands/tool identities, ELF hashes, disassembly and symbols in `build.json`. No execution, simulator output recording, reuse or cleanup of historical ELFs.
- [`derive_oracles.py`](../../tools/a10/derive_oracles.py): offline assembly/algorithm accounting plus ELF64 placement audit. `--check` compares the derivation with the checked-in manifest without changing it. It reads linked opcode identities; it is **not** an instruction decoder, executor or alternate semantics engine.
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

Eight tests in `a10_p0_oracles.rs`:

1. `versioned_manifest_and_capabilities_fail_closed`: version/schema, required capabilities/N/A and manifest expectations.
2. `all_mandatory_fixtures_own_their_public_route_oracle`: all 148 cells, independent captures and applicable negative mutations for result/code/PC/signature/counters/GPR/x0/RAM/UART/device/signal/events/log/facts; missing evidence fails closed.
3. `atomic_and_observation_mutations_reject_real_machine_facts`: AMO old/new bytes, ordinary stores, LR/SC success/failure/reservation effects and order, suppressed x0 writes and fabricated failed-SC memory effects.
4. `receipt_drain_live_reservation_bss_stale_handles_and_equivalent_rerun`: native/flat observations off/on; retain receipts to refuse next turn/mutation/install/reset/teardown/drain, then consume/drop them. Request quiescence, prove drain, reset live reservations, compare restored full image/BSS/UART/signal/register/PC/counter state, reject stale handles and validate a fresh equivalent rerun.
5. `malformed_cli_capture_is_not_a_correctness_pass`: malformed/missing CLI evidence is rejected, not filled from a companion.
6. `p0_oracle_audits_and_regressions_are_not_simulator_recordings`: fresh-source derivation audit and discovered UART/capture regressions.
7. `corrupted_guest_and_mutated_work_expectations_fail_real_public_samples`: actual faulty linked guest fails every route; wrong work/checksum/reader expectations and invalid reader underflow reject real captures.
8. `public_file_write_failure_never_becomes_a_correct_sample`: Unix `/dev/full`, using only public logger construction; native library, CLI and both Machine kinds reject failed reporting while preserving the already-retired first instruction and receipt/drain obligations.

Development failures were reproduced and repaired **only in the harness**:

- Signature/tohost sections initially lacked allocatable writable flags; corrected fixture placement is pinned and audited.
- hello initially expected UART LSR `0x60` for every poll. Existing UART retains TX bytes: only the first read is `0x60`, later reads are `0x20`. The source-derived oracle and regression pin that contract, without changing the device.
- Flat Machine's empty UART capture was initially not checked; a mutation now proves rejection.
- A zero retirement expectation underflowed `retirements - 6`; the reproducer panicked. Checked subtraction now returns semantic rejection, covered by the same real-sample negative test.

Existing A9 tests remain unchanged. Their separate Hart/lifecycle/facade/CLI suites are included in the exact-HEAD verification command, in addition to the full Rust suite. Unknown physical completion remains quarantined; no forced drain/recovery, guest trap repair, fixed-width fetch change or host nondeterminism suppression is added.

## Verification identity, reproduction and remaining gate

Development observation on 2026-10-08: the repaired focused suite ran **8 passed / 0 failed / 0 ignored**, printed all **148 validated cells**, and strict all-features/all-targets Clippy passed in the pinned image. The compiled harness corresponds to implementation `02ba438…`; the worktree had a pending documentation clarification. This is **development evidence only**, not the final clean-HEAD checkpoint gate. An earlier all-suite invocation exceeded the outer command timeout; its incomplete output is not recorded as a pass.

Environment inspected: Docker image `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`, Linux/aarch64, Rust/Cargo 1.97.1, GNU RISC-V binutils 2.40. Host checks are separate: Darwin/arm64, Rust/Cargo 1.98.1. A host test run without a cross toolchain explicitly prints **UNAVAILABLE** for those fixtures; its process success cannot substitute for required-toolchain container evidence.

From a clean committed P0 HEAD, reproduce the bounded toolchain/artifact/public-route verification with:

```bash
bash tools/a10/verify_p0.sh
```

The wrapper uses non-login `bash -c`, `CARGO_BUILD_JOBS=2`, `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`, `--locked`, a new `target/a10-p0-verify-XXXXXX/cargo` build output, and a separate fresh fixture output. It runs fmt, all-features check, strict all-target Clippy, all-features tests/docs, focused P0 with `--nocapture`, A9 regression suites, derivation audit, baseline-to-HEAD diff check and unchanged production/historical-source scope audit. It records source HEAD/tree, timestamps, OS/tool/image identities, actual source/manifest/Cargo.lock/binary/ELF/build-report hashes and logs under that local ignored output. It refuses a dirty worktree and checks that HEAD/tree cleanliness did not change during execution; it deletes nothing.

Worker protocol records **separate exact-HEAD observations**, not prose-derived pass hints:

```bash
qing verify a10-performance-infrastructure --check-set a10-rust --json
qing verify a10-performance-infrastructure --check a10-p0-public-oracles --json -- bash tools/a10/verify_p0.sh
```

The immutable six captured requirements retain their existing IDs: `a9-fmt`, `a9-check`, `a9-clippy`, `a9-tests`, `a9-doc`, `a10-implementation-diff`. All must be observed at the final committed handoff HEAD. These commands are reproduction instructions, **not claims that the final observations have already passed**. Exact-HEAD command output and artifact hashes must accompany checkpoint delivery/review; any later code change requires new verification and independent review.

Remaining: final exact-clean-HEAD observations, typed P0-only handoff and independent committed-head review; supervisor authorization before P1. No timings, throughput, speed verdict, repeated-run schema/driver, comparison mode, benchmark CI, full 58-guest rebuild matrix, final nine-command P4 acceptance, merge, release or successor activation is claimed.
