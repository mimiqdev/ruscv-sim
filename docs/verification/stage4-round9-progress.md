# Stage 4 round 9 progress

**Status:** Implementation progress for PR #84. Not a substitute for the milestone contract.

**Head under review:** the commit that adds this note, on `feat/stage4-sv39-8`.

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

Docker is not available in this environment. The gate ran on the host with `CARGO_BUILD_JOBS=4` and `RUST_TEST_THREADS=4`, one cargo invocation at a time:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features -- --test-threads=4
cargo doc --all-features --no-deps
```

Result: exit 0. Wall time about 373 seconds. `cargo test` reported 1951 passed, 0 failed, 0 ignored, summed across 68 test binaries. That matches the stated baseline of 1951. This round adds no `#[test]` function. The branch as a whole, versus `main` at `7538e41`, adds one integration test, `s4_profile_guests_pass_through_cli_library_and_flat_facade`. `main` was not re-run here.

The Rust, Python, schema, and fixture files in the commit that introduces this section are the tree that gate ran on. A10 baseline and smoke are recorded in the next section after they run on that commit. Until that section names their exits, it is not A10 evidence.

## A10 baseline and smoke

Pending on the clean commit above.

## §7.10 review

Pending on that same committed head. No open spec question. The two settled rulings in "Spec rulings kept" stay as checked.
