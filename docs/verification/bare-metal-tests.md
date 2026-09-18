# Project-Authored Bare-Metal Tests

**Status:** Current verification guide

**Authority:** Normative for guest tests under `tests/bare-metal-riscv-test/`

**Last verified:** 2026-09-18

## Role and evidence boundary

These small RISC-V assembly programs are compiled into ELF files and executed
through the public `ruscv-sim run` path. They are project regressions, not an
external architecture-compliance suite. A passing Rust component test or a
present source file is not evidence that a guest executed end to end.

The A6 trap guests are:

| Source | Architectural check | Success signal |
| --- | --- | --- |
| `rv64i/trap_ecall.S` | Machine `ECALL`, `mcause=11`, `mtval=0`, entry `mstatus`, handler `MEPC += 4`, MRET resume | `tohost` exit 0 |
| `rv64i/trap_illegal.S` | Illegal encoding, `mcause=2`, raw `mtval`, saved PC and non-retirement | `tohost` exit 0 |
| `rv64i/trap_ebreak.S` | `EBREAK`, `mcause=3`, `mtval=0`, handler recovery and resume | `tohost` exit 0 |
| `rv64i/trap_vectored.S` | Synchronous ECALL with `mtvec.MODE=Vectored`; a `BASE+4*cause` landing slot fails | `tohost` exit 0 |
| `rv64i/trap_mret_priv.S` | Legal MRET to User, User-mode MRET rejection, standard illegal trap state, handler recovery | `tohost` exit 0 |

Every guest has explicit success and failure writes. A handler that reaches an
unexpected path writes a nonzero exit code; timeout or simulator failure is not
a guest pass.

## Fresh reproducible build and run

The scripts build into a disposable target directory directly below the
canonical repository `target/` directory, remove it before each normal compile,
record the source Git HEAD in `manifest.txt`, verify ELF entry `== _start`, and
run every discovered RV64I/RV64M ELF through the public CLI. The path guard
rejects source directories, parent aliases, and symlinked roots; its regression
cases run with `bash scripts/test_riscv_elf_guards.sh`.
When a bind-mounted worktree cannot resolve its host-side `.git` file, pass the
host value as `RISCV_SOURCE_HEAD` (the Colima command below does this).
The default suite output is `target/riscv-elf-tests`; override it with
`RISCV_TEST_OUTDIR`. A missing cross-toolchain returns status **77** and prints
`[SKIP]`; assembly/link failures and guest failures return nonzero status.

```bash
# Native host, when the cross-toolchain is installed.
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=target/a6-native-cargo
export RISCV_TEST_OUTDIR=target/a6-native-riscv-elves
./scripts/compile_riscv_tests.sh
RISCV_TEST_SKIP_BUILD=1 ./scripts/run_elf_tests.sh

# Fresh Rust integration: assembles the five A6 sources in a new temp folder,
# then exercises load_and_run, RiscVSimulator, and ruscv-sim run.
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration -- --nocapture
```

`run_elf_tests.sh` also compiles afresh when `RISCV_TEST_SKIP_BUILD` is not set.
It always invokes Cargo for the default release binary path, so repeated runs
cannot silently reuse an obsolete simulator. `RUSCV_SIM_BIN` is an explicit,
caller-owned override; callers using it own binary freshness.

```bash
./scripts/run_elf_tests.sh
```

Use the skip-build form only immediately after inspecting the manifest from the
fresh compile step. Do not run checked-in or stale `.elf` files as A6 evidence.

## Colima ARM64 development image

The recorded development image is immutable:

```text
ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c
```

For a local Apple Silicon/Colima run, use a four-GB ARM64 VM and separate Cargo
and guest output directories. The image already places `/opt/riscv/bin` in
`PATH`; use a non-login `bash -c`, not `bash -lc`, so that path remains intact.
Do not install tools globally or rebuild the image for this verification.

```bash
colima start --arch arm64 --memory 4

IMAGE='ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
SOURCE_HEAD="$(git rev-parse HEAD)"
docker run --rm --init \
  --env RISCV_SOURCE_HEAD="$SOURCE_HEAD" \
  --volume "$PWD:/workspace" \
  --workdir /workspace \
  "$IMAGE" \
  bash -c 'export CARGO_BUILD_JOBS=2 \
    CARGO_TARGET_DIR=target/a6-container-cargo \
    RISCV_TEST_OUTDIR=target/a6-container-riscv-elves \
    RISCV_SOURCE_HEAD="${RISCV_SOURCE_HEAD}" \
    RISCV_REQUIRE_RISCV_TOOLCHAIN=1; \
    cargo fmt --all -- --check && \
    cargo check --all-features && \
    cargo clippy --all-features --all-targets -- -D warnings && \
    cargo test --all-features && \
    cargo doc --all-features --no-deps && \
    ./scripts/run_elf_tests.sh'
```

The final output is the authoritative case count for that run. It must name the
source HEAD from `target/a6-container-riscv-elves/manifest.txt`, the number of
ELFs, entry-point checks, and `Passed == Total`. A container smoke test is not
ACT4 evidence.

## Link, memory, and exit contract

The linker places code at `0x8000_0000` and `.tohost` at `0x8000_1000`.
Guest sources align `.tohost` to eight bytes and write
`(1 << 63) | exit_code`. The simulator also recognizes the standard HTIF
payload. The CLI uses the native RAM/UART/HTIF path; `RiscVSimulator` is tested
separately through its flat-memory library facade, where ELF addresses are
translated to storage offsets by the wrapper.

## Scope limits

The A6 guest suite verifies synchronous Machine-mode trap entry and return on
the current public 32-bit-fetch path. It does **not** certify asynchronous
interrupts, MMU/Sv39 integration, compressed-instruction execution, or the
M/A/F/D dispatch end to end. It is not an ACT4 extension suite. The A5 ACT4
baseline remains the previously recorded 51-case non-trapping result at its
approved exact head; unless a verification record explicitly reports a rerun,
that baseline has not been rerun on the current A6 Task 4 HEAD.
