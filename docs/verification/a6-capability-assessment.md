# A6 Task 4 Capability and Evidence Matrix

**Status:** Current Task 4 evidence record; not a milestone closeout

**Last verified:** 2026-09-18

**Authority:** Informational evidence record. `docs/dev-plan.md` remains the
sole active milestone contract, and source plus executed tests are authoritative
for implementation claims.

**Scope:** Bare-metal synchronous-trap ELF integration and induced-fault
verification for A6 Task 4.

## Evidence identity

The final evidence rows must be read with the exact committed HEAD printed by
`git rev-parse HEAD` and by the guest compiler's `manifest.txt`. A documentation
commit does not retroactively turn an earlier guest run into evidence for a
later source HEAD; repeat the guest command after any implementation change.

Required reproducible environment:

```text
Image:   ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c
Host:    Colima ARM64, 4 GB VM (or CI Ubuntu runner)
Shell:   non-login bash -c; /opt/riscv/bin retained in PATH
Build:   CARGO_BUILD_JOBS=2, isolated CARGO_TARGET_DIR and RISCV_TEST_OUTDIR
```

The A5 ACT4 baseline is existing evidence at its approved exact head and is not
silently refreshed here. A6 Task 4 does not extend ACT4 certification.

## Recorded final verification

The implementation and guest sources were tested at exact committed HEAD
`7aacc0a929a57387564b0bda667669b076924baa`. The Colima ARM64 run supplied that
value as `RISCV_SOURCE_HEAD`, and the fresh manifest reported the same value.
The repository was mounted read/write only for disposable `target/` outputs;
the worktree remained clean of tracked changes.

- Image: `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`
- Host: Apple Silicon ARM64 macOS host with Colima ARM64, 4 GB VM; Docker
  reported `linux aarch64`.
- Quality gate: `cargo fmt --all -- --check`, `cargo check --all-features`,
  strict all-target Clippy, `cargo test --all-features`, and
  `cargo doc --all-features --no-deps` all passed.
- Fresh guest build: `source_count=51`, `compiled_count=51`,
  `failed_count=0`; every ELF entry matched `_start`.
- Fresh public CLI run: `Total: 51`, `Passed: 51`, `Failed: 0`.
  The five A6 trap guests and the 46 pre-existing project guests all passed.
- Focused induced-fault coverage: `cargo test --test trap_test` passed 38
  tests, including causes 0/1/4/5/6/7 and physical transaction assertions.
- Exact-range hygiene: `git diff --check
  51c6e6332f07dceecc0cfae8d54fbfd7c7fb2cab..HEAD` passed.

The native host had no RISC-V assembler, so the host-side A6 integration test
explicitly skipped there; the required-toolchain Colima run above executed the
fresh five-guest CLI/library integration instead. This record does not extend
ACT4 or claim MMU, asynchronous interrupt, compressed-instruction, or extension
end-to-end support.

## Acceptance/evidence matrix

| A6 contract criterion | Task 4 evidence | Status and boundary |
| --- | --- | --- |
| 1. Synchronous trap entry and causes 0/1/4/5/6/7 | `tests/trap_test.rs` induced execution cases; `tests/a6_task3_core_trap_test.rs` retained regression coverage | Rust Hart-boundary evidence checks fault PC, `mtval`, non-retirement, destination/store preservation, and alignment-before-transaction behavior. It is not MMU/PMP evidence. |
| 2. Vectoring and MRET guest behavior | `trap_ecall.S`, `trap_illegal.S`, `trap_ebreak.S`, `trap_vectored.S`, `trap_mret_priv.S` | Each guest self-checks CSRs and writes explicit pass/fail `tohost`; vectored guest contains a failure sentinel at `BASE+4*cause`. Requires fresh ELF execution. |
| 3. Multi-trap, counter, recursive timeout, final-slot failure, exit priority | Existing A6 Task 3 tests plus focused executor/run-control tests; guest handlers exercise exit after trap | Component and runner boundaries are distinct. The guest suite does not replace Rust tests for simulator failure or budget precedence. |
| 4. Public CLI and library facade | `tests/a6_trap_elf_integration.rs` fresh-builds each guest and checks `ruscv-sim run`, `load_and_run`, and `RiscVSimulator` | End-to-end only when the test reports a real five-guest build/run; a missing toolchain is `[SKIP]` locally or a required failure under CI environment control. |
| 5. Fresh project ELF suite and true entry | `scripts/compile_riscv_tests.sh` + `scripts/run_elf_tests.sh`; manifest and `readelf`/`nm` `_start` check | The normal run deletes its output directory before assembly/link. It reports total project case count separately from the five A6 cases. |
| 6. CI integration | `.github/workflows/ci.yml` installs cross-tools, requires them for the Rust A6 integration, and runs the fresh project suite on pushes | Pull-request Rust tests cover the fresh five-guest integration; push CI covers the full discovered project ELF suite. |
| 7. Reproducibility | `docs/verification/bare-metal-tests.md` records immutable image digest, Colima ARM64/4 GB procedure, non-login shell, and isolated builds | The digest and procedure are reproducibility inputs, not proof of a run. The exact run identity must be recorded after execution. |
| 8. Zero regressions and quality gate | `fmt`, `check`, strict `clippy`, all-feature `test`, and `doc` commands from AGENTS | Record only commands actually run on the final exact HEAD. Guest ELF execution is never inferred from `cargo test` alone. |
| 9. Capability/closeout boundaries | This matrix, `current-state.md`, and the active `dev-plan.md` | A6 is not archived or closed by Task 4. No claim is made for asynchronous interrupts, MMU, C execution, or ACT4 extension certification. |

## Required final command set

From the repository root, with the RISC-V toolchain available:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps

git diff --check 51c6e6332f07dceecc0cfae8d54fbfd7c7fb2cab..HEAD
./scripts/run_elf_tests.sh
```

The guest command must be run with the immutable image procedure when native
cross-tools are unavailable. Record the actual `source_head`, total case count,
per-case exit status, and whether the five A6 guests and the pre-existing guest
set both passed. Do not replace any row with an old CI result or with a component
presence claim.
