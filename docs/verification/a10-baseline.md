# A10 baseline v1 observation

**Status:** Current
**Authority:** Informational; recorded baseline evidence, not a speed/regression gate.

**Scope:** The right-sized A10 baseline profile (`a10-baseline-report/1`): exactly three
representative workloads, one session, ≥15 repetitions each, public-CLI end-to-end timing.
Correctness floor per repetition: the CLI child's process exit code plus exact expected
stdout bytes. No cohort, no ratios, no speed target. Certification
(cohort/baseline-sealing gates, full matrix, CI/retention) is deferred per the
[dev-plan amendment](../dev-plan.md#8-future-implementation-deliverables-and-falsifiable-acceptance).

## Recorded run

- Source HEAD: `5c52533a05db35d5534f71f5af2b7c90858cd661` (clean tree)
- Report SHA-256: `2b99398c5bbce81927c6cc67f0a974cab0aebf34c854ce2a6f40ed813ed3b805`
- Completed (UTC): `2026-10-09T14:51:18.515278Z`; target `aarch64-apple-darwin`, release, locked
- CLI binary SHA-256: `3b794fc8bdf104ddd76b9d14cfe19fc668b5205f40539464d2da0408acb1eaff`
- Cargo.lock SHA-256 prefix: `3458721e23c22739…`; embedded build head bound via
  `RISCV_PERF_BUILD_HEAD` and recorded in `build.env`/`build.embedded_build_head`
- Environment identity: full `environment` block in the report (execution namespace,
  qualified physical host, container/CI observations, filesystem, sink/concurrency policy)
- Fixture producers: audited in the pinned container
  (`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2…`) with `RISCV_REQUIRE_A10_PINNED_TOOLS=1`

| Workload | Fixture | Verified | Median ns | p05 ns | p95 ns |
| --- | --- | ---: | ---: | ---: | ---: |
| alu-branch-loop | control_loop | 15/15 | 3,006,625 | 2,615,221 | 3,572,817 |
| ram-store-load-loop | ram_loop | 15/15 | 2,899,167 | 2,731,325 | 3,214,958 |
| device-cli-hello | hello | 15/15 | 2,888,292 | 2,696,642 | 3,124,804 |

All 45 repetitions passed the exit+stdout floor; semantic status `correct`; exit 0.
Numbers are informational host-cost observations of the real CLI end-to-end child
(launch through wait), never modeled guest cycles or a performance verdict.

The original run above is preserved verbatim; its pre-merge source HEAD
`5c52533…` was an unmerged PR-#72 branch commit (the branch later merged as
`5d23d337a6732d178a2e14445be9f5c28154f270` via squash). Its numbers are not
rewritten, re-derived or compared against the merged-main re-run below.

## Merged-main re-run (2026-10-10, additional evidence)

Re-executed on this machine exactly as the re-run instructions above specify,
at clean committed merged `main`:

```bash
git rev-parse HEAD    # 5d23d337a6732d178a2e14445be9f5c28154f270, clean tree
./scripts/perf-test.sh run --suite public-v1 --profile baseline --out target/a10-baseline-5d23d33-r2
# exit 0
```

| Field | Value |
| --- | --- |
| Source HEAD / tree | `5d23d337a6732d178a2e14445be9f5c28154f270` / `a2920abf949a6d6e8c753b9f05a8ef9f06a41fc9` (clean) |
| Report SHA-256 | `842c4c86d4098402f08c585a673d98e630a35deb475bc1667769cb3c47934be9` |
| Run interval (UTC) | `2026-10-10T06:49:29.070619Z` → `2026-10-10T06:49:58.910693Z` (run id `aaa5e2bc-06f1-4376-86a6-7973cf0263bb`) |
| Host | Dell OptiPlex Tower Plus 7020, Intel Core i9-14900K, 32 logical CPUs, 62 GiB RAM, Linux `7.2.5-3-omarchy`, governor `powersave`, turbo enabled; no container detected in the execution namespace, CI identity unavailable (not a runner) |
| Toolchain | Rust/Cargo 1.98.0 (`88d9e12ae`, host `x86_64-unknown-linux-gnu`), `--locked`, target `x86_64-unknown-linux-gnu`, release |
| CLI binary SHA-256 | `e83a2ba41ff733c48b498c7f9c4e5fd1b5b1e913ec6685a7ccf344bb7f29b4c9` |
| Cargo.lock SHA-256 prefix | `3458721e23c22739…` (identical to the original record) |
| Fixture producers | pinned container `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c` with `RISCV_REQUIRE_A10_PINNED_TOOLS=1`; ARM64 sub-manifest producer hashes `as 0159aa61…` / `ld ad207835…` / `nm 08488f38…` / `objdump dac962bc…` match `public-v1.json` exactly |

| Workload | Verified | Median ns | p05 ns | p95 ns |
| --- | ---: | ---: | ---: | ---: |
| alu-branch-loop | 15/15 | 869,676 | 745,458 | 1,185,946 |
| ram-store-load-loop | 15/15 | 2,178,769 | 1,429,175 | 2,535,514 |
| device-cli-hello | 15/15 | 2,040,975 | 1,485,498 | 2,371,411 |

All 45 repetitions passed the exit+stdout floor; `semantic_status` `correct`;
exit 0. These medians are **not comparable** with the original record's: the
host (Linux/amd64 i9-14900K here vs. `aarch64-apple-darwin` there), toolchain
and binary identity all differ, and the comparison policy requires a new
baseline series for CPU/OS/toolchain changes. They remain informational
host-cost observations only. Both sessions used the same locked fixtures,
producer identities and correctness floor.

### Re-run environment note (host, not tool, difference)

This host is Linux/amd64 without the RISC-V cross tools. The wrapper's
pinned-container path invokes `docker run` without `--platform`; on an amd64
host it resolves the digest's **amd64** sub-manifest, whose producer binaries
are x86-64 and fail the ARM64 pin audit (`required pinned ARM64 producer
mismatch`, exit 1, no report). The digest is a multi-arch OCI index whose
**arm64** sub-manifest carries the pinned ARM64 tool bytes. Exporting
`DOCKER_DEFAULT_PLATFORM=linux/arm64` for the session (a host environment
selection, not a repository-tool change) made the same unmodified command
select the arm64 sub-manifest and pass the strict audit; the bounded 15-minute
re-run budget was not exceeded (attempt 06:32–06:39 UTC failed on this exact
mismatch after the image pull; the corrected attempt 06:49:29–06:49:58 UTC
completed in ~29 s). The failed attempt's output directory
(`target/a10-baseline-5d23d33`) is retained as the honest failure record.

## Linux/x86_64 record after the per-platform pin (2026-10-10, additional evidence)

Tony authorized a pin change on 2026-10-10: fixture producers are pinned per image platform, not to the ARM64 hashes only. The change is commit [`25f3a3d0f10b9949b7624c741fc963b411b06665`](https://github.com/mimiqdev/ruscv-sim/commit/25f3a3d0f10b9949b7624c741fc963b411b06665) (`feat(a10)!: pin fixture producers per image platform, not ARM64-only`). The pinned identity is the container image digest plus the platform the container actually runs as (`uname -m`). This session is a third observation, recorded after that commit. It does not replace the `aarch64-apple-darwin` record above, and its numbers are not comparable with either earlier session.

The report records a clean tree at source HEAD `25f3a3d0f10b9949b7624c741fc963b411b06665` (tree `ab483eab5a5a65db889a512c50e3def4be25be20`). Output directory: `target/a10-baseline-x86_64`. Command:

```bash
./scripts/perf-test.sh --suite public-v1 --profile baseline --out target/a10-baseline-x86_64
```

| Field | Value |
| --- | --- |
| Source HEAD / tree | `25f3a3d0f10b9949b7624c741fc963b411b06665` / `ab483eab5a5a65db889a512c50e3def4be25be20` (report `source.clean` true) |
| Report SHA-256 | `55e84d64afbf31bf4aa0838a4f10893a5f7ee9faf77ca6c069f523bdae8d9690` |
| Run interval (UTC) | `2026-10-10T08:15:33.317300Z` → `2026-10-10T08:15:59.842280Z` (run id `ea6486b5-4d3e-4af9-8aff-24768404f454`) |
| Host | Dell OptiPlex Tower Plus 7020, Intel Core i9-14900K, 32 logical CPUs, MemTotal 65,511,900 kB, Linux `7.2.5-3-omarchy`, governor `powersave`, turbo enabled (`no_turbo=0`); no container detected in the execution namespace; `systemd-detect-virt` unavailable (exit 1); CI identity unavailable (not a runner) |
| Toolchain | rustc 1.98.0 (`88d9e12ae`, host `x86_64-unknown-linux-gnu`), cargo 1.98.0, `--locked`, target `x86_64-unknown-linux-gnu`, release |
| CLI binary SHA-256 | `e83a2ba41ff733c48b498c7f9c4e5fd1b5b1e913ec6685a7ccf344bb7f29b4c9` |
| Cargo.lock SHA-256 | `3458721e23c22739ea87ca4e7b186136a29b35f448b7ea217f86ddb1a7285b49` |
| Embedded build head | `25f3a3d0f10b9949b7624c741fc963b411b06665` (`RISCV_PERF_BUILD_HEAD`) |
| Image and platform | `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`, platform `x86_64` (`fixtures.producers.platform` and `fixtures/build.json`). The baseline report schema does not embed the image digest; the digest is the one `tools/a10/baseline.py` passes to `docker run` for this profile. |
| Fixture producers | strict audit with `RISCV_REQUIRE_A10_PINNED_TOOLS=1`. x86_64 pin from `public-v1.json` `tool_sha256.x86_64`: `as 5d693231db1242b89b73e7329117e8f35d29c498e9e765c10b62e2a9429983f9`, `ld 1ed449b500697d187754c10799543d91571fc9a875a3feee5a1eada761c50baa`, `nm 783030bd9b56ab0b6e8dc2a7c20b15dd0e4668748c0e1bd9c10d2dd1e3b0702f`, `objdump 8a9215d2d7ab2d670d741fa14d1e77df6a43e1a66861fe1e5b5c00114c9697d6`. Those hashes match `fixtures.producers.tools` in this report. GNU binutils 2.40. |
| Status | `semantic_status` `correct`; `measurement_status` `informational-baseline-v1; no speed/regression gate`; diagnostics empty |

Percentiles below are copied exactly from `workloads[]` in `report.json`. They are not rounded.

| Workload | Fixture | Verified | Median ns | p05 ns | p95 ns |
| --- | --- | ---: | ---: | ---: | ---: |
| alu-branch-loop | control_loop | 15/15 | 955319.0 | 833159.0 | 1175547.9999999998 |
| ram-store-load-loop | ram_loop | 15/15 | 1550402.0 | 1393219.4 | 2125099.6999999997 |
| device-cli-hello | hello | 15/15 | 1807604.0 | 1297012.2 | 2259583.6999999997 |

All 45 repetitions passed the exit+stdout floor. These medians are not comparable with the `aarch64-apple-darwin` record or with the merged-main re-run above. Host, toolchain, binary, and producer-platform identity differ. The comparison policy requires a new baseline series for a CPU, OS, or toolchain change.

## Re-run and compare later

```bash
git rev-parse HEAD          # must be clean and committed
./scripts/perf-test.sh run --suite public-v1 --profile baseline \
  --out target/a10-baseline-$(git rev-parse --short HEAD) [--repetitions 15..64]
python3 -c "import json;r=json.load(open('<out>/report.json'));print(r['semantic_status'],r['workloads'])"
```

- Exit 0 = valid report with the floor verified for every repetition; exit 2 = honest
  INCONCLUSIVE (verification floor failure or unavailable tool); failures retain all rows.
- Output must be a fresh directory below `target/`; existing guards refuse dirty trees,
  reuse/overwrite, symlinked ancestors and parent escapes.
- Compare later sessions by reading two reports and comparing per-workload medians
  yourself; the profile intentionally emits no ratios and no classification.
- `smoke` remains unchanged; the deferred calibration/certification layer was
  deleted from the branch (user-authorized 2026-10-10) and never produced
  baseline, ratio or acceptance evidence.
