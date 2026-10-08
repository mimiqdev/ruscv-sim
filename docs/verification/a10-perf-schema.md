# A10 public-path evidence: `ruscv-perf/1`

- Status: Draft
- Authority: Informational
- Scope: A10 P2 local smoke/evidence, not P3 calibration or P4 retention
- Contract: [current contract](../dev-plan.md), [ADRs](../architecture/decisions/README.md)
- Implementation: [`report_schema.py`](../../tools/a10/report_schema.py), [`replay.rs`](../../tools/a10/replay.rs)
- Machine-readable definition: [`ruscv-perf-1.schema.json`](../../tools/a10/ruscv-perf-1.schema.json) (Draft 2020-12)

## Commands and status

From a clean committed checkout with Rust, Python 3.11+ and RISC-V GNU tools:

```bash
./scripts/perf-test.sh run --suite public-v1 --profile smoke --out target/perf/p2-new
./scripts/perf-test.sh validate target/perf/p2-new/report.json
./scripts/perf-test.sh validate target/perf/p2-new/report.json --bundle-sha256 <retained-external-digest>
bash tools/a10/verify_p2.sh
```

`run` freshly builds release/all-features/locked binaries and fixtures before clocks. `--repetitions 1..16` selects basic repetitions; every cell also has one independently validated warmup. It retains 180 phase cells and 324 explicit N/A rows (864 total rows at the default two basic repetitions). Each actual repetition uses the unchanged reusable P0 oracle; no preflight/companion run blesses another sample. The 148 P0 route/mode matrix remains separately tested.

Wrapper statuses are **0** valid report/all semantics correct (the reader), **1** semantic/schema/integrity/reporting failure, **2** measurement unavailable/inconclusive. A semantically correct **smoke `run` returns 2**, not performance PASS. Human output prints INCONCLUSIVE. `validate` can return 0 while comparison remains inconclusive: validity is not calibrated measurement acceptance. No public runtime CLI/library guest exit behavior changes. Compare/calibrated commands return 2 and are not implemented.

## Protocol and identities

Unknown versions, legacy P1 checkpoint reports, unknown typed fields, missing required fields, duplicate keys (identical or conflicting at any nesting), non-finite numbers, booleans/floats masquerading as integer counters, negative/overflowed durations and widths are rejected. All mandatory nullable fields are present; absence has a bounded, provenance-bearing reason. A raw P1-format transport is retained as `raw-report.json`; it is **not** renamed into the full schema or accepted as a baseline.

Top-level fields separate `semantic_status` from `comparison_status`. The report identifies:

- Schema digest and versioned [suite](../../tools/a10/suite-v1.json), unchanged independent [oracle](../../tools/a10/public-v1.json), [policy](../../tools/a10/smoke-v2.json) and harness digests.
- Full committed revision/tree/clean assertion, every tracked source blob, commit object and complete reconstructible Git tree; before/after source checks. Binary/tool bytes and hashes, Cargo.lock, actual target/profile/features/build argv/environment/config digests, effective verbose rustc commands and observed codegen/LTO/CPU flags are retained.
- Actual Rust/Cargo/rustfmt and cross-tool versions/executables, fresh assembler/linker argv and source/linker/ELF hashes; placement, zero-fill, signal and signature metadata. Strict pinned-tool proof is distinct from portable byte-identical artifact equivalence.
- Execution namespace and separately qualified physical host observations: CPU/model/architecture, OS/kernel, logical CPUs/memory, virtualization/container, affinity/governor/turbo and filesystem. Missing inspection is unavailable with reasons, not guessed. A strict launcher captures **both actual container and image `docker inspect`**, digest/platform and native outer-host observations. Environment-variable labels alone are not container/host inspection. Identified CI jobs record actual job/run URL and runner image/environment; native runs explicitly lack CI identity.

Metadata is collector evidence, **not cryptographic host attestation**. Native OS inspection does not prove bare metal; virtualization evidence remains qualified. External Cargo configuration retains complete hashes and build-affecting sections, not private registry credentials. No secrets are required. Filesystem and file sink policy are recorded; synchronous unbuffered public `File` writes do not imply fsync/durable storage or invented buffering guarantees.

## Raw samples and replay

Ordered `sample-NNNNNN` rows have unique cell/repetition IDs, warmup flags, UTC/origin, work/iteration counts, exact phase/capture/sink policies, clock references and raw integer-nanosecond start/stop/elapsed. The clock is `std::time::Instant`; 64 empty timers and 64 successive-read deltas are captured **outside guest scopes**. The minimum nonzero delta is empirical observed resolution, not advertised resolution or calibrated sufficiency. Parent/child origins are never subtracted.

Own observed results distinguish guest and actual process/transport codes, timeout/error, PC/signature metadata/bytes/digest, progress, final RAM/GPR/MINSTRET/signal/device state where public, immutable effects and complete captured Hart/CSR/FPR details, log/facts counts/digests and diagnostics. Unsupported public introspection is explicit N/A, never an expected value relabeled as observed or borrowed companion state. Convenience APIs expose completed turns, not general attempt/retirement/trap equivalence; the unchanged independently pinned no-trap fixture and actual own result/self-check supply that specific witness. Load-only declares zero completed turns and execution-not-started, not a guest exit.

Machine load/execute rows retain actual initial full-image/BSS hash, all GPRs, PC/counter, cleared reservation, selected signal/events/UART and generation. Execute-only proves real drain, fresh reset, generation advance and stale-memory isolation before resume/start. Flat execute-only retains public fresh-reset image/BSS/state/reservation proof; generation/drain/stale-handle/device internals are unavailable through that facade. End-to-end convenience construction is inside its existing public call; no new inspection API or broken clock boundary is invented.

The reader verifies source/artifact identities, exact inventory/order, reference ownership, accounting/distributions and actual CLI/native transport against each own sample. Then a **repository-built** reader replays every retained actual sample through the **same P0 validator**, with compiled pinned independent oracle and ordered phase-scope checks. It never runs a binary supplied by arbitrary report JSON, trusts `semantic_status=correct`, executes a second ISA interpreter, or copies companion evidence. Retained scope traces are operation-connected collector evidence, not a signer. A bad warmup/basic repetition cannot be discarded to produce a favorable aggregate; failed/inconclusive rows and unstarted successors remain in order.

P1 boundaries remain unchanged: load retains owner before stop; execute excludes setup/reset/inspection/drop but includes synchronous delivery/receipt consumption; native clocks are child-local actual public calls plus UART flush; CLI clocks launch through wait. Flat end-to-end is one continuous construct/load/run/minimum **owned final-state copy/normal drop** interval. Capture allocations/copies are timed, under `flat-live-owned-final-copy-before-drop/1`; no pauses/subtraction/segmented intervals or retained live owner. Structured reporting, validation and transport serialization occur afterward.

## Statistics, bundles and limits

Each applicable cell includes all sample IDs, warmup/basic/correct counts, explicit discards, basic interval sum, median, linearly interpolated p05/p95 and MAD. No outlier trimming. Any bad warmup/sample invalidates the accepted cell sum. Bootstrap median confidence, accepted baseline/revision and ratios are unavailable with reasons. Comparison keys explicitly identify schema/suite/oracle/harness/fixture/policy, route/phase/observation/sink/capture, actual build/toolchain/container/host/clock policy and candidate revision; baseline revision is null. This is descriptive bookkeeping, **not** P3 comparable-baseline acceptance.

The smoke policy lacks ≥30 samples, ≥5/1-second warmups, ≥10-ms calibrated work, three independent sessions, noise/bootstrap confidence and an accepted comparable baseline. Every cell's comparison classification is inconclusive; no ratio, speed claim or performance PASS is emitted.

Fresh named output must be below repository `target/`, never reused/protected/escaping/symlink-controlled. `bundle.json` lists immutable exclusive-created source/binary/tool/fixture/raw/report/stdout/stderr/log/facts/environment artifacts with SHA-256/size and stable safe relative references. `bundle.sha256` hashes the final manifest (no self-hash cycle); supply its separately retained digest for external retrieval identity. Missing/tampered/truncated/aliased/escaping/symlink artifacts or write/flush errors cannot become success. Files are read-only after seal; wrapper never replaces bytes under an existing ID. Checksums detect mutation, not a malicious re-signer/host. Cargo scratch caches are excluded; used binaries are separately copied and sealed.

Bundles are local evidence, not accepted performance baselines. No P4 baseline index, upload, expiry/retention or CI policy is implemented. Historical evidence and seven component controls remain unchanged. P3/P4 and final A10 acceptance remain deferred.

## Falsifiable checks

[`a10_p2_schema.rs`](../../tests/a10_p2_schema.rs) round-trips actual public phase captures, replays every own warmup/basic oracle and mutates results/state/effects/lifecycle/scopes/clock/origin/inventory/widths. [`test_schema.py`](../../tools/a10/test_schema.py) tests strict shape/duplicate/non-finite/overflow controls, immutable retrieval/path/reporting failures and descriptive statistics. The strict wrapper additionally runs actual sealed release-bundle round-trip and negative controls. P0/P1 scope spies, A9 lifecycle/receipt/unknown-completion regressions and the full Rust gate remain required.
