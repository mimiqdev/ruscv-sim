# Stage 4 progress and closeout

**Status:** Implementation and review recorded. Not a formal completion. The [contract](../dev-plan.md) stays Current until a later, separately authorized rotation merges.

**Base:** `main` at `20660471cf675851943cbeb39a7270a88841bb2a`.

This note covers the work that was still open after the Stage 4 guests landed: the oracle digest lock, CI coverage of the §7 item 9 gate, the §7 item 10 review, and this record.

## 1. Oracle digest lock

Owner decision, 2026-10-13: do not seal or re-seal `tools/a10/public-v1.json` while features are unfinished.

What changed:

- `tools/a10/suite-v1.json` no longer carries `oracle_sha256`.
- `derive_oracles.py --check` and `--check-artifacts` still compare counts, effects, placement, and exit behavior with the checked-in oracle. A difference that exists only in `source_sha256`, `linker_sha256`, or `elf_sha256` is not a failure.
- `audit_fixtures.py` still requires `build.json` to record the hash of the file it just produced. It does not compare that hash with the oracle's recorded digest.
- The P1 measurement path and the P2 reader no longer reject a run because the executed ELF's digest differs from the oracle field. They still check the architectural result, and the reader still checks that the recorded input digest equals the retained ELF bytes.
- Producer-tool pins selected by `RISCV_REQUIRE_A10_PINNED_TOOLS` are unchanged.

The behavioral regression stays. `tests/a10_p0_oracles.rs` still runs every fixture on the public routes and still rejects a mutated guest and a changed linked opcode. A digest-only edit of `build.json` does not fail `--check-artifacts`.

## 2. CI gate (§7 item 9)

Quality-and-tests now runs, on pull requests and on pushes to `main`:

- `cargo fmt --all -- --check`
- `cargo check --all-features`
- `cargo clippy --all-features --all-targets -- -D warnings`
- `cargo test --all-features`
- `cargo doc --all-features --no-deps`
- `scripts/compile_riscv_tests.sh` and `scripts/run_elf_tests.sh`

`run_elf_tests.sh` fails if any Stage 4 guest is missing. Those guests are `s4_ad_observation`, `s4_counters`, `s4_csr_policy`, `s4_delegation`, `s4_envcfg`, `s4_fp_absent`, `s4_mapped_rw`, `s4_mprv`, `s4_permission_fault`, `s4_pmp_csr`, `s4_privilege_transition`, `s4_sfence_vma`, `s4_sret_vectored`, `s4_supervisor_interrupt`, `s4_tvm_tsr_tw`, and `s4_walk_fault`. The route suites `s4_sv39_guests` and `s4_profile_guests` run inside `cargo test`.

Coverage remains manual.

## 3. Review (§7 item 10)

The review is [stage4-final-review.md](stage4-final-review.md). No public-path specification defect was fixed in this pull request, because none was found. Follow-ups F1–F5 stay open. One open question is recorded there: a reserved FENCE `fm` value, including FENCE.TSO with Ztso absent, currently retires as a fence. The contract requires FENCE.I to trap, which it does. D8 and D10 stay decided.

## 4. Local gate

Docker was not available in this environment. The gate below is plain cargo, with `CARGO_BUILD_JOBS=4` and `RUST_TEST_THREADS=4`, one command at a time, then the ELF suite. Log: the run that produced these lines.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass, 1s |
| `cargo check --all-features` | pass, 12s |
| `cargo clippy --all-features --all-targets -- -D warnings` | pass, 24s |
| `cargo test --all-features -- --test-threads=4` | pass, 448s. 1952 tests, 0 failed, including 27 doc-tests |
| `cargo doc --all-features --no-deps` | pass, 2s |
| `./scripts/run_elf_tests.sh` | pass, 210s. 75 fresh ELF cases, 0 failed, including every `s4_*` guest linked with `linker-s4.ld` |

A green GitHub Quality-and-tests check on this pull request is the CI half of §7 item 9. That job also runs the A5 and A10 Python unit tests and the guest-script guards. The A10 suite was run locally after the cargo gate: `python3 -m unittest discover -s tools/a10 -p 'test_*.py'` — 29 tests, 1 skipped, 0 failed.

## 5. What this record does not do

It does not archive the Stage 4 contract, replace `docs/dev-plan.md`, merge, tag, or declare the milestone formally complete. Those steps stay separately authorized. A10's certification layer stays deferred.
