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
