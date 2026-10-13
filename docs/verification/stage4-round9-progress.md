# Stage 4 round 9 progress

**Status:** Implementation progress for PR #84. Not a substitute for the milestone contract.

**Commits on `feat/stage4-sv39-8`:**

- `fc98f54` seals the oracle, the harness, and the first draft of this note. The quality gate and both A10 runs used that tree.
- `80f8c1e` changes only this file. It records those runs and an author check. That author check is not the §7 item 10 review.
- The commit that adds the independent-review section also tightens the `derive_s4.py` module comment. It does not change the oracle, the witness sources, the schema, or Rust.

## What was wrong in `627625c`

That commit updated the A10 harness and the cell-count assertions for 15 fixtures, and it edited three compact witnesses, but it did not regenerate `tools/a10/public-v1.json`. The oracle still listed the original 12 fixtures, `suite-v1.json` still pinned the old digest, and the new assertions could not pass. The paused run is not the oracle.

## Oracle

`tools/a10/derive_s4.py` accounts the three new public-v1 witnesses from the linked opcode words and the profile WARL rules. It does not call ruscv-sim and it does not copy a trace. `derive_oracles.py --extend` appends them, preserves the sealed producer identities, and refuses to write if any of the original 12 fixtures changes. `--check-artifacts` matches after the extension.

| Witness | What it checks | attempts | turns | retirements | traps |
| --- | --- | --- | --- | --- | --- |
| `s4_tvm` | TVM=1, S-mode `csrr satp` is illegal-instruction, `mtval` = `0x18002373` | 45 | 45 | 44 | 1 (exception, an attempt) |
| `s4_ssip` | SSIP+SSIE taken in M once MIE=1; `mcause` = `(1<<63)\|1`, `mepc` = the interrupted instruction, `mtval` = 0 | 48 | 49 | 48 | 1 (interrupt, not an attempt) |
| `s4_csr_fields` | `mstatus` drops MPP=2, FS, and SD and keeps SXL/UXL=2; `misa` stays `0x8000000000141101`; `mepc` bits 1:0 stay clear; illegal `mtvec` MODE keeps the previous legal MODE; an unsupported `satp` MODE is ignored | 70 | 70 | 70 | 0 |

The matrix is now 15 fixtures, 187 P0 route/mode cells, 228 P1 phase cells, and 402 explicit N/A rows. P0 compared these traces with the simulator and passed. The original 12 fixtures are byte-identical in the oracle.

These three witnesses are the smoke-manifest entries. The full §4 families remain the public-route guests in `tests/s4_profile_guests.rs` and `tests/s4_sv39_guests.rs`. A complete instruction-effect trace of every family guest was not added: those guests are self-checking on the CLI, library, and flat routes, and several of them walk Sv39. Putting every one of them into `public-v1.json` would be a second, much larger derivation.

## Oracle digest

`report_schema.py` no longer carries a handwritten `ORACLE_SHA`. A report's oracle digest has to equal the retained `public-v1.json` bytes, and those bytes have to equal the tree file. `suite-v1.json` `oracle_sha256` is that same file digest. `audit_fixtures.py` rejects a mismatch. `derive_oracles.py --extend` rewrites the suite pin from the file it just wrote. Adding a witness does not require pasting a new constant into the checker.

Producer pins in `tool_sha256` are unchanged. `--extend` does not reseal them.

## Spec rulings kept

- Illegal-instruction traps from TVM, TSR, and TW record `mtval` as the 32-bit faulting instruction. `s4_tvm_tsr_tw.S` compares the three trapped encodings exactly, including `0x18002373` (`csrr t1, satp`) and `0x18001073` (`csrw satp, x0`). The same exact-`mtval` check is in `s4_fp_absent.S`, `s4_counters.S`, and the compact `s4_tvm` witness.
- `medeleg` and `mideleg` read back 0 after a write of all ones. `s4_delegation.S` keeps that check, and the following page fault is taken in M with the original virtual address in `mtval`. Contract M1. No delegation was added.

## §4 families

`s4_profile_guests` covers TVM/TSR/TW, PMP CSRs, delegation, environment configuration, identity and counters, F/D absent, CSR field policy, and supervisor interrupt. `s4_sv39_guests` covers mapped read/write, permission fault, MPRV, walk fault, A/D, and SRET/vectored delivery. Both suites passed on the CLI, library, and flat routes in this round. The counters guest keeps the per-case MPP return from `c2f1252`; it did not loop.

## Gate

Docker is not available here, so the gate ran on the host rather than in the pinned image. `CARGO_BUILD_JOBS=4` and `RUST_TEST_THREADS=4`. One cargo invocation at a time, on the sources in `fc98f54`:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features -- --test-threads=4
cargo doc --all-features --no-deps
```

Result: exit 0. Wall time about 373 seconds. `cargo test` reported 1951 passed, 0 failed, 0 ignored, summed across 68 test binaries. That matches the stated baseline of 1951. This round adds no `#[test]` function. Versus `main` at `7538e41`, the branch adds one integration test, `s4_profile_guests_pass_through_cli_library_and_flat_facade`. `main` was not re-run here.

`s4_profile_guests_pass_through_cli_library_and_flat_facade` and `s4_sv39_guests_pass_through_cli_library_and_flat_facade` both passed. The log has no toolchain skip line.

The independent-review commit changes this note and the `derive_s4.py` module comment. The same host gate on that tree also exited 0, with 1951 passed, 0 failed, and 0 ignored. Oracle bytes, witness sources, and Rust were unchanged, so the A10 runs were not repeated.

## A10 baseline and smoke

Both runs used a clean tree at `fc98f54`. The harness sets `CARGO_BUILD_JOBS=2` for its own release build. Reports stay under `target/` and are not committed. The lines below are the harness output.

Baseline, 15 repetitions, exit 0:

```text
valid a10-baseline-report/1 evidence at /workspace/target/a10-baseline-s4/report.json; informational only, NO performance PASS
```

`semantic_status` is `correct`. 45 rows, 0 failed. `alu-branch-loop`, `ram-store-load-loop`, and `device-cli-hello` each have `verified_count` 15. `source.head` is `fc98f54`. Diagnostics empty.

Smoke, default 2 basic repetitions plus one warmup, process exit 2. That exit is the harness rule that a valid smoke report is still not a performance pass:

```text
P0 artifact-equivalence/oracle audit: matches; NOT pinned-toolchain evidence (no simulator invoked)
INCONCLUSIVE: valid ruscv-perf/1 evidence at /workspace/target/perf/s4-smoke/report.json; 1086 ordered rows. P3 calibration/baseline/ratios and P4 retention deferred; NO performance PASS
```

`semantic_status` is `correct`, `comparison_status` is `inconclusive`, diagnostics empty. 684 rows are `correct` and 402 are `not_applicable`. Aggregates: 228. Oracle sha256 in the report is `7229e98c1f69eeb88825983c049ca953d63f97f50a9f0cb87cfe5718f48a2917`, the same digest as `suite-v1.json`. Each of `s4_csr_fields`, `s4_ssip`, and `s4_tvm` has 48 correct rows and 26 explicit N/A rows. `source.clean` is true and `source.head` is `fc98f54`.

## §7.8 sweep

Checked against `docs/dev-plan.md` §7 item 8, on the gate run above. Each row is a guest that exited 0 on the CLI, the library route, and the flat facade.

| Clause | Where it passed |
| --- | --- |
| `medeleg`/`mideleg` read 0; page fault taken in M | `s4_delegation` |
| TVM, TSR, TW, including both U-mode WFI cases; `mtval` is the instruction bits | `s4_tvm_tsr_tw` |
| Environment configuration | `s4_envcfg` |
| Identity and counters | `s4_counters` |
| F and D absent; `mtval` is the instruction bits | `s4_fp_absent` |
| CSR field policy | `s4_csr_policy` |
| Supervisor interrupt: U, S, and M; not taken in M while MIE is 0; taken once MIE is 1; SEI before SSI before STI; vectored `mtvec` | `s4_supervisor_interrupt` |
| PMP CSR read-only zero, odd `pmpcfg` illegal | `s4_pmp_csr` |

The compact public-v1 witnesses repeat three of those rows for the smoke matrix: `s4_tvm` (`mtval` = `0x18002373`), `s4_ssip` (SSIP taken in M with MIE=1, `mtval` = 0), and `s4_csr_fields` (MPP=2, FS, SD, `misa`, `mepc`, `mtvec` MODE, unsupported `satp` MODE, `mie`/`mip` outside `0x222`). Sv39 families stay on `s4_sv39_guests`.

## §7.10 review

The author check of `fc98f54` confirmed the points below and changed no code. It does not by itself meet §7 item 10.

- `derive_s4.py` does not call the simulator. It applies the profile WARL rules, takes a supervisor interrupt before fetch without counting an attempt, and takes an S-mode `satp` access with TVM=1 as cause 2 with `mtval` equal to the instruction word, counted as an attempt. P0 compared those traces with the simulator.
- Strict facts still require the CSR journal to be exactly `minstret` then `mcycle`. S4 allows explicit CSR writes before that suffix. `mcycle`'s old value is the retirement index plus the number of trap turns at or before that index.
- The original 12 fixtures are unchanged. `--check-artifacts` matched, and `suite-v1.json` `oracle_sha256` equals the SHA-256 of `public-v1.json`. There is no handwritten `ORACLE_SHA`.
- Not every §4 family ELF is inside `public-v1.json`. The eight profile guests and six Sv39 guests are self-checking route programs.

An independent read-only review then covered `627625c..fc98f54` and this note as updated in `80f8c1e`. Outcome: no code or oracle blocker. The original 12 fixtures match `c2f1252`. The suite pin matches the oracle file. Strict facts stay exactly two counter entries. S4 allows an explicit CSR prefix and counts trap turns in `mcycle`. What that review asked for is handled as follows.

| Review point | Disposition |
| --- | --- |
| §7 item 10 needs an independent review, not only an author check | This section is that recorded outcome. |
| The note header mixed `fc98f54` with a later documentation-only commit | The header now names `fc98f54` and `80f8c1e` separately. |
| Gate and A10 results are not stored as committed reports | The harness lines quoted above are the retained evidence. Full reports stay under `target/` and are not in git. |
| `derive_s4.py` could be read as a general Stage 4 model | The module comment now limits it to the three witnesses and to one TVM path: an S-mode CSR access to `satp`. SFENCE.VMA, TSR, and TW are not modeled. An unknown opcode raises `ValueError`. |
| `s4_tvm.S` comment on the `0x188` immediate | Left in place. The written value is `0x1880` (MPIE and MPP=M). A comment edit changes `source_sha256` and would reseal the oracle. |
| `tools/a10/fixtures/linker-s4.ld` looked unused | That file is not in the tree. `fc98f54` deletes it. The three witnesses use `tools/a10/fixtures/linker.ld`. |
| Report field `nontrapping_witness` | The name is older than the trap-inclusive method text. `witness_method()` already describes trap turns. Renaming the field would change the report shape after the recorded smoke run, so the name stays. |

No open spec question. The two rulings in "Spec rulings kept" stay settled.
