# Project-Authored Bare-Metal Tests

**Status:** Current verification guide

**Authority:** Normative for guest tests under `tests/bare-metal-riscv-test/`

**Last verified:** 2026-11-03

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

The Zca U4 compressed guests build with
`-march=rv64imac_zicsr_zifencei` and run only under `--isa rv64imac`:

| Source | Architectural check | Success signal |
| --- | --- | --- |
| `rv64i/zca_ctrl_flow.S` | Compressed `c.beqz`/`c.bnez` taken and not-taken, `c.j` over a poison word, and a 32-bit instruction at `pc % 4 == 2` | `tohost` exit 0 |
| `rv64i/zca_link_values.S` | `c.jalr` links `pc+2`, a 32-bit `jal` links `pc+4`, and `c.jr` transfers to a 2-byte target | `tohost` exit 0 |
| `rv64i/zca_ebreak_trap.S` | `C.EBREAK` raises `mcause=3`, `mtval=0`; the handler sets `mepc += 2` and MRET resumes in the compressed stream | `tohost` exit 0 |
| `rv64i/zca_hints.S` | D2 HINT parcels (`c.addi rd,0`, `c.nop nzimm`, `c.li x0`, `c.lui x0`, `c.slli x0`, `c.srli` zero shift, `c.mv x0`, `c.add x0`) retire with no architectural effect | `tohost` exit 0 |
| `rv64i/zca_straddle_fetch.S` | Under Sv39, a 32-bit instruction whose first parcel is page A's last 2 bytes and whose second parcel is the R-only page B faults with `mtval=0x2000` and retires nothing (contract D4) | `tohost` exit 0 |

The F/D U5 guests build with `-march=rv64imafd_zicsr_zifencei` and run under
`--isa rv64imafd`, except `zcd_transfer`, which builds with
`-march=rv64imafdc_zicsr_zifencei` and runs under `--isa rv64imafdc`:

| Source | Architectural check | Success signal |
| --- | --- | --- |
| `rv64ifd/fd_rounding.S` | All five rounding modes on `FDIV.S` of 1/3 and on `FCVT.S.D` of the exact tie 1+2^-24, so RMM differs from RNE; dynamic `frm`; a static `rm` overriding `frm`; reserved static `rm` 5 and 6 and dynamic `frm` 5–7 raising illegal-instruction | `tohost` exit 0 |
| `rv64ifd/fd_flags.S` | NX, UF, OF, DZ and NV accrual with tininess after rounding, an exact result leaving `fflags` clear, an exact subnormal leaving it clear, and `fflags`/`frm`/`fcsr` aliasing and masking | `tohost` exit 0 |
| `rv64ifd/fd_nanbox.S` | `FLW` and `FMV.W.X` boxing, canonical NaN from an unboxed single operand, FSGNJ payload preservation, `FSW`/`FMV.X.W` raw low bits, arithmetic/`FSQRT`/`FMIN`/`FMAX` canonicalization, and double payload transfers | `tohost` exit 0 |
| `rv64ifd/fd_fs_state.S` | FS Off raising illegal-instruction for every F/D instruction class here and for `fflags`/`frm`/`fcsr` with `mtval` equal to the encoding; FS Dirty from an FPR write, from flag accrual with no FPR write, and from a `CSRRW`; FS unchanged by `FSW`, an exception-free compare and a `CSRR`; SD mirroring FS in `mstatus` and `sstatus` | `tohost` exit 0 |
| `rv64ifd/fd_fma.S` | Fused single rounding for FMADD, FMSUB, FNMSUB and FNMADD in both formats, including a case whose value differs from an unfused implementation | `tohost` exit 0 |
| `rv64ifd/fd_convert.S` | The `FCVT` saturation table, RV64 sign extension of 32-bit results, `FCVT` rounding, exact widening and narrowing, all ten `FCLASS` classes, quiet/signaling NaN compare rules, and `FEQ`/`FLT`/`FLE` on ±0 | `tohost` exit 0 |
| `rv64ifd/fd_zcd_absent.S` | Under `--isa rv64imafd`, `C.FLD`, `C.FSD`, `C.FLDSP` and `C.FSDSP` stay illegal-instruction with the parcel in `mtval` and no architectural effect | `tohost` exit 0 |
| `rv64ifd/zcd_transfer.S` | Under `--isa rv64imafdc`, the four Zcd parcels transfer 64-bit values at their scaled offsets and preserve a double NaN payload; a Zcd load makes FS Dirty and a Zcd store leaves it unchanged | `tohost` exit 0 |

The F/D U6 debug-session guest builds with
`-march=rv64imafdc_zicsr_zifencei` and runs under `--isa rv64imafdc`:

| Source | Architectural check | Success signal |
| --- | --- | --- |
| `rv64ifd/fdc_u6_attach.S` | Unattached, FS is Off at reset, then `fcsr` and every FPR read zero. `scripts/debug_attach.sh` sets `fd_u6_marker`; the guest then verifies the debugger-written `f0`–`f31` and `fcsr`, checks that abstract access left FS Off, and executes an FLD, FADD.D and FSW that the debugger breaks on and single-steps with FS checks | `tohost` exit 0 |

The debug script compares that session's named values with pinned Spike run
with `--isa=rv64imafdc_zicsr`. The connect PC is the only named difference
(Spike halts in its boot ROM).

Every guest has explicit success and failure writes. A handler that reaches an
unexpected path writes a nonzero exit code; timeout or simulator failure is not
a guest pass.

## Fresh reproducible build and run

The scripts build into a disposable target directory directly below the
canonical repository `target/` directory, remove it before each normal compile,
record the source Git HEAD in `manifest.txt`, verify ELF entry `== _start`, and
run the original RV64I/RV64M guests followed by the A8 RV64A atomic additions
and the F/D U5 additions through the public CLI. The five A6 trap guests, the
five Zca U4 compressed guests, the eight F/D U5 guests and the U6 FP attach
guest remain required members; the Zca guests run with `--isa rv64imac`, the
`fd_*` guests with `--isa rv64imafd`, `zcd_transfer` and `fdc_u6_attach` with
`--isa rv64imafdc`, and every other guest runs under the default `rv64ima`. The path guard
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
the current public 32-bit-fetch path. The Zca U4 guests add end-to-end
compressed execution under `--isa rv64imac`: mixed-length control flow, link
values, the D2 HINT set, `C.EBREAK` trap entry and a page-straddling fetch. The
F/D U5 guests add end-to-end floating point under `--isa rv64imafd` and
`--isa rv64imafdc`: rounding modes, exception-flag accrual, NaN boxing, FS Off
and Dirty, fused multiply-add, conversions and classification, and the Zcd
transfers. None of these sets certifies asynchronous interrupts, MMU/Sv39
integration as a whole, the M/A/F/D dispatch end to end, or every Zca, F, D or
Zcd encoding; component and ACT4 verification own those claims. It is not an ACT4 extension suite. The A5 ACT4 baseline remains a 51-case nontrapping result and is not an A6
trap or extension certification. A separate retained run `35325844246` reran
that frozen selection at source head
`845c63325db5ac87ab2ff0ed260453dc3b396ae9`, with 51/51 public-CLI passes;
its offline replay and artifact identity are recorded in
[`a6-act4-replay-35325844246.json`](a6-act4-replay-35325844246.json). The
implementation merge `f12b1f80907e92b1c82b33ac669b961cc17f59c9` has the same
Git tree, but the source and merge commits remain distinct evidence identities.
