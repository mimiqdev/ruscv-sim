# A10 public-path evidence: `ruscv-perf/1`

- Status: Current
- Authority: Informational
- Scope: approved P2 local smoke/evidence and P3 implementation under development; not P3 acceptance or P4 retention
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

Wrapper statuses are **0** valid report/all semantics correct (the reader), **1** semantic/schema/integrity/reporting failure, **2** measurement unavailable/inconclusive. A semantically correct **smoke `run` returns 2**, not performance PASS. Human output prints INCONCLUSIVE. `validate` can return 0 while comparison remains inconclusive: validity is not calibrated measurement acceptance. No public runtime CLI/library guest exit behavior changes. The calibrated/compare paths below are under development and have no accepted full-session evidence yet.

## Protocol and identities

Unknown versions, legacy P1 checkpoint reports, unknown typed fields, missing required fields, duplicate keys (identical or conflicting at any nesting), non-finite numbers, booleans/floats masquerading as integer counters, negative/overflowed durations and widths are rejected. All mandatory nullable fields are present; absence has a bounded, provenance-bearing reason. A raw P1-format transport is retained as `raw-report.json`; it is **not** renamed into the full schema or accepted as a baseline.

Top-level fields separate `semantic_status` from `comparison_status`. The report identifies:

- Schema digest and versioned [suite](../../tools/a10/suite-v1.json), unchanged independent [oracle](../../tools/a10/public-v1.json), [policy](../../tools/a10/smoke-v2.json) and harness digests.
- Full committed revision/tree/clean assertion, every tracked source blob, commit object and complete reconstructible Git tree; before/after source checks. Binary/tool bytes and hashes, Cargo.lock, actual target/profile/features/build argv/environment/config digests, effective verbose rustc commands and observed codegen/LTO/CPU flags are retained.
- Actual Rust/Cargo/rustfmt and cross-tool versions/executables, fresh assembler/linker argv and source/linker/ELF hashes; placement, zero-fill, signal and signature metadata. Strict pinned-tool proof is distinct from portable byte-identical artifact equivalence.
- Execution namespace and separately qualified physical host observations: CPU/model/architecture, OS/kernel, logical CPUs/memory, virtualization/container, affinity/governor/turbo and filesystem. Missing inspection is unavailable with reasons, not guessed. A strict launcher captures **both actual container and image `docker inspect`**, digest/platform and native outer-host observations. Environment-variable labels alone are not container/host inspection. Identified CI jobs record actual job/run URL and runner image/environment; native runs explicitly lack CI identity.

A known physical host requires the complete typed host structure, including every required observation's availability/value/reason/provenance. Deleting CPU/model/architecture/OS/kernel/CPU-count/memory/virtualization/affinity/governor/turbo fields, or reducing the host to a container flag, is invalid even when setup/provenance mirrors agree. Whole-host unavailability remains valid only with null value and a nonempty inspection reason/provenance.

Metadata is collector evidence, **not cryptographic host attestation**. Native OS inspection does not prove bare metal; virtualization evidence remains qualified. External Cargo configuration retains complete hashes and build-affecting sections, not private registry credentials. No secrets are required. Filesystem and file sink policy are recorded; synchronous unbuffered public `File` writes do not imply fsync/durable storage or invented buffering guarantees.

## Raw samples and replay

Ordered `sample-NNNNNN` rows have unique cell/repetition IDs, warmup flags, UTC/origin, work/iteration counts, exact phase/capture/sink policies, clock references and raw integer-nanosecond start/stop/elapsed. The clock is `std::time::Instant`; 64 empty timers and 64 successive-read deltas are captured **outside guest scopes**. The minimum nonzero delta is empirical observed resolution, not advertised resolution or calibrated sufficiency. Parent/child origins are never subtracted.

Own observed results distinguish guest and actual process/transport codes, timeout/error, PC/signature metadata/bytes/digest, progress, final RAM/GPR/MINSTRET/signal/device state where public, immutable effects and complete captured Hart/CSR/FPR details, log/facts counts/digests and diagnostics. Unsupported public introspection is explicit N/A, never an expected value relabeled as observed or borrowed companion state. Convenience APIs expose completed turns, not general attempt/retirement/trap equivalence; the unchanged independently pinned no-trap fixture and actual own result/self-check supply that specific witness. Load-only declares zero completed turns and execution-not-started, not a guest exit.

Machine load/execute rows retain actual initial full-image/BSS hash, all GPRs, PC/counter, cleared reservation, selected signal/events/UART and generation. Execute-only proves real drain, fresh reset, generation advance and stale-memory isolation before resume/start. Flat execute-only retains public fresh-reset image/BSS/state/reservation proof; generation/drain/stale-handle/device internals are unavailable through that facade. End-to-end convenience construction is inside its existing public call; no new inspection API or broken clock boundary is invented.

The reader verifies source/artifact identities, exact inventory/order, reference ownership, accounting/distributions and actual CLI/native transport against each own sample. Descriptor-relative no-follow retrieval rejects concurrent symlink swaps, and semantic replay reads a private snapshot of the exact verified retained bytes, not live untrusted paths. Native rows must match the retained child's **complete interval and clock evidence**, not merely duration and clock ID. The complete native `Sample` is independently reconstructed from `LibraryWire`, UART and the applicable retained log, so unsupported counts/GPR/RAM/MINSTRET or other introspection must remain absent. Coupled raw/evidence/clock mirrors cannot bless disagreement with original child stdout.

Then a **repository-built** reader replays every retained actual sample through the **same P0 validator**, with compiled pinned independent oracle and ordered phase-scope checks. It never runs a binary supplied by arbitrary report JSON, trusts `semantic_status=correct`, executes a second ISA interpreter, or copies companion evidence. Retained scope traces are operation-connected collector evidence, not a signer. A bad warmup/basic repetition cannot be discarded to produce a favorable aggregate; failed/inconclusive rows and unstarted successors remain in order.

P1 boundaries remain unchanged: load retains owner before stop; execute excludes setup/reset/inspection/drop but includes synchronous delivery/receipt consumption; native clocks are child-local actual public calls plus UART flush; CLI clocks launch through wait. Flat end-to-end is one continuous construct/load/run/minimum **owned final-state copy/normal drop** interval. Capture allocations/copies are timed, under `flat-live-owned-final-copy-before-drop/1`; no pauses/subtraction/segmented intervals or retained live owner. Structured reporting, validation and transport serialization occur afterward.

## Statistics, bundles and limits

Each applicable cell includes all sample IDs, warmup/basic/correct counts, explicit discards, basic interval sum, median, linearly interpolated p05/p95 and MAD. No outlier trimming. Any bad warmup/sample invalidates the accepted cell sum. Bootstrap median confidence, accepted baseline/revision and ratios are unavailable with reasons. Comparison keys explicitly identify schema/suite/oracle/harness/fixture/policy, route/phase/observation/sink/capture, actual build/toolchain/container/host/clock policy and candidate revision; baseline revision is null. Compact digest/reference keys identify complete recorded build/container/host metadata without duplicating large transcripts in each cell; cross-session compatibility normalization/acceptance remains P3 work. This is descriptive bookkeeping, **not** P3 comparable-baseline acceptance.

The smoke policy lacks ≥30 samples, ≥5/1-second warmups, ≥10-ms calibrated work, three independent sessions, noise/bootstrap confidence and an accepted comparable baseline. Every cell's comparison classification is inconclusive; no ratio, speed claim or performance PASS is emitted.

Fresh named output must be below repository `target/`, never reused/protected/escaping/symlink-controlled. `bundle.json` lists immutable exclusive-created source/binary/tool/fixture/raw/report/stdout/stderr/log/facts/environment artifacts with SHA-256/size and stable safe relative references. `bundle.sha256` hashes the final manifest (no self-hash cycle); supply its separately retained digest for external retrieval identity. Missing/tampered/truncated/aliased/escaping/symlink artifacts or write/flush errors cannot become success. Files are read-only after seal; wrapper never replaces bytes under an existing ID. Checksums detect mutation, not a malicious re-signer/host. Cargo scratch caches are excluded; used binaries are separately copied and sealed.

Bundles are local evidence, not accepted performance baselines. No P4 baseline index, upload, expiry/retention or CI policy is implemented. Historical evidence and seven component controls remain unchanged. P3 acceptance, P4 and final A10 acceptance remain deferred.

## P3 qualified local calibration (under development)

The implementation in [`calibrated.py`](../../tools/a10/calibrated.py) and
[`calibration.rs`](../../tools/a10/calibration.rs) extends the same entry point:

```bash
./scripts/perf-test.sh run --suite public-v1 --profile calibrated --out target/perf/p3-new
./scripts/perf-test.sh validate target/perf/p3-new/report.json
./scripts/perf-test.sh compare --baseline target/perf/p3-new/baseline/report.json --candidate target/perf/p3-new/candidate/report.json --out target/perf/p3-comparison
```

These procedures are implementation targets, **not verified usable evidence**.
The local policy requires native Darwin inspection and one continuing inspected
Colima Linux/aarch64 container using the pinned repository image. Every full
session is a new actual driver process with fresh fixtures and output IDs at
one clean committed HEAD. Exported baseline/candidate views select different
actual processes; copying retained bytes is not another session. Older P2
smoke remains readable but cannot become a calibrated baseline.

The [new duration-scaled workload series](a10-calibration-workloads.md) maps
all original anchors to distinct execution and initialized-payload load ELFs;
original smoke/P0 identities remain correctness controls. Calibrated reports
require explicit new workload/mapping/oracle versions and digests, and every
actual repetition is replayed against the compiled new independent oracle.
Old smoke/short-anchor evidence is not a compatible variant baseline.

[`calibrated-v1.json`](../../tools/a10/calibrated-v1.json) fixes pilots, both
warmup minima (five correct iterations AND one second of actual timed work),
20-second warmup cap, final warmed repetition calibration, thirty samples and
at least ten milliseconds of independently summed work per sample. Setup,
reset, inspection, serialization and validation never enter those sums.
Warmup/calibration failures and incomplete/unstarted batches remain explicit.
No outliers are trimmed. Median, linear p05/p95, MAD and deterministic
95% percentile-bootstrap median intervals use 10,000 resamples and seed
104729. The 5% relative half-width is sufficiency, never a regression gate.
Three independent session medians receive their own repeatability check.
One predetermined full noise rerun may replace one session; all original
records remain, and missing/control/semantic failures are not noise retries.

Raw `calibrated-raw/1` JSONL retains every independently captured and
P0-validated record. Policy version 2 buffers at most 256 frames or 8 MiB
of plain owned serialized data in the driver; it also flushes at pilot,
warmup and control boundaries. Compression occurs synchronously only during
those flushes, with every frame acknowledged before another guest timer. Complete plain owned sample/clock values may share a
retained-value ID **only after complete equality of each own fresh capture**.
This does not reuse verdicts, skip execution/inspection or retain live owners.
The phase driver similarly caches pure SHA-256 results only after complete
byte equality of each newly read observed image; every initial full-image/BSS,
register/counter/reservation/device check still runs. The pre-read immutable
input buffer is hashed once, and the per-repetition on-disk mutation guard
remains. No cached verdict or companion state replaces an own oracle.
The reader expands every row and replays every repetition with the unchanged
P0 validator. No bytes escape the owned buffer between flushes, avoiding
compression overlapped with guest measurement. Buffering never removes a
repetition or its own oracle; transport failure retains the available prefix
and cannot become a usable session.
Calibrated file sinks use an inspected 128-MiB VM-local tmpfs under the
collection's `local-sinks/session` path. Docker configuration and actual
before/after `df -PT` readback must agree. These are real synchronous public
`File` writes, not an arbitrary in-memory writer API; no durability/fsync is
claimed. Raw log/stdout/stderr copies and normal removal of private scratch
occur after stop/own P0 and before another timer. Complete byte-identical
captured files may share read-only hard-linked storage only after own byte
comparison; altered retained bytes or copy failure reject timing. Every raw
row/reference remains. Frozen baseline/candidate publications are compact
versioned selection views referencing the one independently executed root
bundle; they never copy whole payload trees or manufacture executions.
Original smoke sink behavior is unchanged.

Boundary correction (policy version 6): an earlier draft overstrictly read
dev-plan §7 as three sessions inside ONE invocation. Each complete public
calibrated `run` invocation executes and fully validates exactly ONE
independent session within its own 30-minute budget, including orchestration,
build/fixtures, inspection, ALL semantic reconstruction/publication; three
independent invocations at ONE final clean committed HEAD form the cohort.
The v5 collection at `58bc9f9` failed at 102.7973 minutes with 180/31/0 usable
cells and stays failed/inconclusive evidence. Reader/publication work is
bounded inside the budget: a persistent repository-built shared-P0 replay
server over VM-local scratch replaces per-fragment subprocesses, retained
fixture bytes are reused only after one digest-verified read, and exhaustion
produces honest bounded partial evidence (never success without
integrity/oracles, and semantic failure still exits 1 first). One successful
session never becomes a baseline; comparisons require the valid three-session
cohort plus an independently produced candidate. The committed mechanism is
the versioned `ruscv-perf-cohort/1` builder: after three independent
invocations, `./scripts/perf-test.sh cohort --reports A/report.json
B/report.json C/report.json --out target/perf/p3-cohort` revalidates every
member against its retained raw evidence, requires one clean HEAD, distinct
processes/run IDs, a shared pinned plan/policy/workload and one positively
inspected allocation, and seals `cohort.json` plus the `baseline.json`
compatibility binding; `compare --baseline …/baseline.json --candidate
…/report.json` then requires an independently produced compatible candidate
before any informational ratio. Focused negative controls reject copied/self
members, old/mixed heads, plan drift, tampered members, cohort-pid candidates
and incompatible work.

### Qualified affinity and allocation, not physical control

[`allocation.py`](../../tools/a10/allocation.py) retains actual native CPU/OS,
boot UUID/time, power source/low-power settings, local Docker context/provider,
daemon/image/container start/configuration/resources, VM boot/kernel and
process namespaces. Before/after disagreement, missing positive lineage,
whole-host absence or copied process/group identities cannot qualify.
Unobservable physical affinity/governor/turbo retain original reasons and are
**UNOBSERVED/UNCONTROLLED**, never null-equality operands or wildcards.
VM vCPU masks are not physical core mapping; power settings are not frequency,
governor or turbo readback. No sustained equal scheduler/DVFS guarantee, general
cross-runner acceptance or cryptographic host attestation is claimed.

For each real CLI launch, [`affinity.rs`](../../tools/a10/affinity.rs) inspects
**the exact driver thread**, before direct launch and after wait/capture,
outside both timer boundaries. It records PID/TID, namespace links, allowed
vCPU list, online CPUs, effective cpuset, own cgroup membership/limits and
injection-related environment. Fixed resource paths qualify only a cgroup-v2
namespace root (`0::/`); unsupported membership and malformed or missing
positive limits/boot IDs cannot qualify even if both carriers agree.
Executable bytes, actual argv and launcher
source are bound to retained build/source evidence. No shell, taskset,
pre-exec inspection or affinity-changing launch attributes are introduced.
The method is **`linux-caller-affinity-inheritance/1`**, derived initial
placement only. Direct child readback is explicitly not attempted and the
child PID is unavailable through `Command::output`; no guessed observed mask
or continuous child-mask stability is asserted.

The kernel guarantee is per-thread process-creation/exec inheritance, subject
to cpuset/online restrictions: [sched_setaffinity(2)](https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html),
[cpuset(7)](https://man7.org/linux/man-pages/man7/cpuset.7.html) and
[posix_spawn(3)](https://man7.org/linux/man-pages/man3/posix_spawn.3.html).
No literal fork syscall is claimed observed. Native library transports inspect
their own caller outside their child-local call/UART-flush interval. The P2
complete interval/clock/sample equality requirement remains unchanged.

The harmless Linux test helper reads **its own** inherited mask using the same
`Command::output` mechanism. It temporarily restricts/restores only its calling
libtest thread. Its result is not benchmark CLI or guest correctness evidence.
Caller/thread confusion, changed masks/cgroup/cpuset/online CPUs/namespaces,
injection, wrong source/argv/derived kind or false direct-readback labels fail
closed. Missing inheritance or qualification/noise/sampling evidence means
exit 2 and no ratios; semantic/schema/integrity/reporting failure means exit 1.
Compatible usable ratios and confidence intervals are informational only.

## Falsifiable checks

[`a10_p2_schema.rs`](../../tests/a10_p2_schema.rs) round-trips actual public phase captures, replays every own warmup/basic oracle and mutates results/state/effects/lifecycle/scopes/clock/origin/inventory/widths, including complete native clock/carrier negatives on both routes and off/file warmup/basic samples. [`test_schema.py`](../../tools/a10/test_schema.py) tests strict shape/duplicate/non-finite/overflow controls, immutable retrieval/path/reporting failures (including post-check symlink swaps), Cargo mixed stdout/codegen tokenization and descriptive statistics. The strict wrapper additionally runs actual sealed release-bundle round-trip and negative controls. P0/P1 scope spies, A9 lifecycle/receipt/unknown-completion regressions and the full Rust gate remain required.
