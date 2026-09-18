# A7 T0 migration characterization

**Status:** T0 baseline only. This record is not an A7 implementation or
conformance report.

**Runtime baseline:** `9ee9c5f24c072779f06ce141b3340f953b614442`
(`docs(a7): activate non-atomic physical-access contract and close A6 (#47)`).

**Characterization fixture:** commit
`485b6891639b4c60603e74e1222b8bd95f75f446` introduced the fixture;
`d82186c4b21d073a539e791b89c97a19b7e2aa58` records the architectural SC
encoding; and `f8473b52552cc1ef5ed20b8e28e983102b70bd30` is the
review-strengthened fixture HEAD. These commits add only
`tests/a7_migration_characterization.rs`. The candidate fixture and the
baseline archive use the same test file and the same existing
`tests/common/public_elf.rs` helpers. No Rust runtime, ISA implementation,
public API, or `.qing/config.toml` was changed for T0.

## Purpose and limits

T0 records what the baseline actually dispatches and returns. It does not
replace an observed result with the architectural RISC-V result. In particular,
the successful legacy AMO/LR/SC paths are retained as regression targets even
when their current width, reservation, or indivisibility behavior is deficient.
The matrix below therefore separates:

* **preservation:** a result or side-effect boundary that migration must retain;
* **known debt:** behavior observed here and deliberately not repaired by T0; and
* **architectural contrast:** what ADR-0001/0002 would eventually require, not
  an expectation asserted by these tests.

The isolated reservation child is run four times. It uses process isolation,
not a mutex local to this test file, because `GLOBAL_RESERVATION` is a
production process-global singleton. The child serially exercises distinct
`RiscvCore` instances, a reset, and the flat runner. The only calls to
`clear_reservation` are fixture setup/teardown boundaries; no runtime code was
changed to make the observations pass.

## Self-contained baseline matrix

All instruction encodings below have primary opcode `0x2f` for AMO/LR/SC.
The test's `amo_raw` helper writes the stated `funct5`, `funct3`, `rs1`, `rs2`,
and `rd` fields; the public `RiscvCore::step_outcome` path performs decode,
prechecks, dispatch, and architectural outcome mapping.

| ID | Input encoding / address / width | Actual call order and observed result | Preservation and debt classification |
| --- | --- | --- | --- |
| A7-T0-01 | `AMOADD.W`: `funct5=00001`, `funct3=010`, `rs1=0x80`, aligned 4-byte access. `AMOADD.D`: same `funct5`, `funct3=011`, `rs1=0x88`, aligned 8-byte access. | W encoding retires and calls `read_word(0x80)` then `write_word(0x80)`. D encoding also retires but calls `read_word(0x88)` then `write_word(0x88)`; the old low word is sign-extended to `rd`, and only the low word changes. | Preserve both successful legacy paths and their typed call order during non-atomic migration. The D encoding's word helper is known width debt; T0 does not turn it into rejection or claim AMO.D conformance. |
| A7-T0-02 | Architectural LR uses `funct5=00010`, `rs2=0`; architectural SC uses `funct5=00011`; both use `funct3=010` in this fixture and 8-byte-aligned addresses. | `SC` without a reservation retires with `rd=1` and makes no write. LR in one core followed by SC in another core at the same exact address retires with `rd=0` and writes. | Preserve successful LR/SC and no-reservation outcomes. The current `execute_amo` fallback for `funct5=00011` selects `exec_sc` (the dword helper) even for the SC.W `funct3=010` encoding; this is not an ADR-0002 atomic envelope. |
| A7-T0-03 | LR at `0xa0`, then `RiscvCore::reset`, then SC at `0xa0`; LR at `0xb0`, SC at `0xb8`. | Reset does not clear the process-global reservation, so the post-reset SC succeeds. The different-address SC retires with `rd=1` and leaves `0xb8` unchanged. | Preserve the observed reset and exact-address key behavior until an explicitly scoped reservation migration. The singleton and lack of reset clearing are known defects, not per-Hart correctness. |
| A7-T0-04 | LR at `0xc0` with initial value 1; ordinary `SD` writes 2, then SC writes 7. LR at `0xd0` with initial value 3; `FSD` writes 4, then SC writes 5. | Each instruction is stepped separately. The scalar/FP values are observed as 2/4 immediately after the intervening stores; each final SC succeeds (`rd=0`) and leaves its distinct value, 7/5. | Preserve that ordinary integer and FP writes do not invalidate the current legacy reservation. This is a labelled regression, not the architectural reservation rule. |
| A7-T0-05 | LR at `0xe0` with initial value 5; `RiscVSimulator::write_mem(0xe0, 8 bytes)` writes 7; SC at `0xe0` writes 6. | The host-written bytes are observed before the second simulator step; SC still retires successfully and leaves 6. | Preserve host-write visibility and the absence of a reservation notification on the legacy path. Host writes remain outside the migrated ordinary-guest transaction guarantee. |
| A7-T0-06 | LR at `0xf0`; ordinary `SD` to invalid `0x1000` in a 0x100-byte core memory; SC retried at `0xf0` after the trap boundary. | The failed store calls the backend, enters `StoreAccessFault` with `mtval=0x1000`, leaves the target unchanged before the later SC, and the later SC succeeds. | Preserve no partial ordinary-store effect and the current reservation retention after a failed ordinary write. Do not infer that this is architectural invalidation behavior. |
| A7-T0-07 | LR at `0xf0`; SC write uses the same address but a `TraceMemory` whose `write_dword` returns `MemoryError::InvalidAddress`. | LR retires and calls `read_dword`; SC calls `write_dword`, enters `StoreAccessFault` with original `mtval=0xf0`, gives no destination result, and makes no target write. A following SC in the isolated process succeeds because the current helper returns before its reservation-clear statement. | Preserve the fault classification/no-write boundary and document the known SC-fault reservation-clear defect. This is not proof of the selected ISA's faulting-SC rule. |
| A7-T0-08 | Direct `SystemBus` dword reads/writes at `HTIF_BASE=0x4000_8000` and `HTIF_BASE+1..+7`, width 8. | Every direct read returns zero. Every direct write succeeds and invokes the HTIF callback using only the starting address; interior spans are not rejected. | Preserve existing public typed `SystemBus` behavior on the legacy route. The new raw-port full-span rule in §6 is not retroactively applied here. |
| A7-T0-09 | RAM starts at HTIF base with size 4, then with size 8; dword access at the base, width 8. | Size 4 cannot satisfy the whole dword span, so the request falls through to HTIF and invokes the callback; RAM remains unchanged. Size 8 wins RAM-first, stores/reads the dword, and does not invoke HTIF. | Preserve RAM-first full-span matching, no cross-target stitching, and device fallback. |
| A7-T0-10 | The architectural `SC.W` encoding (`funct5=00011`, `funct3=010`, nonzero `rs2`) at `HTIF_BASE+4`; LR first creates the current reservation. | The core's data precheck uses width 4, but dispatch falls through to `exec_sc`, whose typed operation is dword. `SystemBus::write_dword(HTIF_BASE+4, value)` succeeds by start-address-only HTIF routing and invokes the callback; the instruction retires with `rd=0`. | Preserve this successful width-debt callback path. It must not be changed into unsupported/rejected behavior merely to make a future raw port look conforming. |
| A7-T0-11 | UART public base `0x1000_0000`: half/word/dword reads and writes, then byte reads/writes and reserved offset `+0x80`. | Wide accesses return `InvalidAddress` before UART methods: the RX FIFO, TX FIFO, and output callback are unchanged. Byte RX pops exactly once; byte TX calls output exactly once. The public 0x100-byte SystemBus window accepts `+0x80`, where the UART returns zero/ignores the write. | Preserve width rejection and side-effect boundaries, including the native public window being wider than the UART TLM range. |
| A7-T0-12 | Flat image base `0x8000_0004`; guest address `base+4` is 8-byte aligned but maps after one subtraction to backend offset `0x4`; `LD` and `SD`, width 8. | Fetch at offset zero succeeds. The load/store backend calls use offset `0x4`, return `Misaligned`, and core mapping produces `LoadAccessFault`/`StoreAccessFault` with the original guest address in `mtval`; destination/register and RAM bytes have no partial change. | Preserve Hart guest alignment before backend access, checked single base subtraction, original-address trap values, and current backend-offset alignment classification. Do not grant this access merely because a future raw target could transfer unaligned bytes. |
| A7-T0-13 | Core PC below its configured base (`pc=0x100`, base `0x200`); flat tohost declarations below base, with `u64::MAX-7` causing offset-end overflow, and an in-range offset `0x1004` that is not dword aligned. | Core enters `InstructionAccessFault` without a backend call. Flat `load_elf` rejects below-base, overflow, and non-eight-byte-aligned tohost offset with bounded errors. | Preserve checked below-base/overflow rejection and the flat tohost poll's alignment precondition. |
| A7-T0-14 | Load a good image with signature and tohost, then attempt a rejected below-base replacement. | Rejected load leaves the old image, metadata, RAM, and signature in place; the old image still exits with code 7 after four completed turns and returns its original signature bytes. | Preserve validate-before-mutate image replacement semantics and failure-load retention. |
| A7-T0-15 | `RiscVSimulator::write_mem(4, [aa,bb,cc])`; then `write_mem(14, [1,2,3,4])` in 16-byte flat RAM; empty write at `u64::MAX`. | Offset 4 stores exact bytes. The second call returns an error after the successful prefix at offsets 14 and 15; it does not roll the prefix back. Empty write returns success without accessing memory. | Preserve host `write_mem` offset, empty-write, and prefix-on-failure behavior outside the guest transaction guarantee. |
| A7-T0-16 | A separate five-second bounded child probe calls `write_mem(u64::MAX-1, [aa,bb])` and catches a possible panic. | Baseline observation was an error from the first invalid byte (`Invalid memory address: 0xfffffffffffffffe`), not a reached arithmetic-overflow panic. The child is bounded and the result is recorded only as characterization. | Do not create a new overflow/panic contract from this probe. Any hardening is outside T0. |

The reservation child emitted the same transcript on all four repetitions:

```text
amoadd.w=retired/read_word+write_word;
sc.no-reservation=retired/rd=1/no-write;
lr->other-core-sc=success/global;
lr->reset->sc=success/reservation-retained;
lr@b0->sc@b8=retired/rd=1;
lr->sd->sc=success/no-invalidate;
lr->fsd->sc=success/no-invalidate;
lr->host-write_mem->sc=success/no-invalidate;
lr->failed-sd->sc=success/reservation-retained;
sc.write-fault=trap/store-fault/clear-deferred;
sc.w@htif+4=retired/dword-callback
```

## Revision comparison evidence

The candidate test was run directly from the worktree with an isolated target:

```bash
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-candidate \
  cargo test --test a7_migration_characterization
```

Result: **11 passed, 0 failed** at fixture commit
`f8473b52552cc1ef5ed20b8e28e983102b70bd30`.

The same candidate test file was copied into a temporary archive of the fixed
baseline (the worktree and its `.qing/config.toml` were not modified):

```bash
base=9ee9c5f24c072779f06ce141b3340f953b614442
tmp=$(mktemp -d /tmp/ruscv-a7-baseline.XXXXXX)
git archive "$base" | tar -x -C "$tmp"
cp tests/a7_migration_characterization.rs \
  "$tmp/tests/a7_migration_characterization.rs"
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$tmp/target" \
  cargo test --manifest-path "$tmp/Cargo.toml" \
  --test a7_migration_characterization
rm -rf "$tmp"
```

Result: **11 passed, 0 failed** at source archive revision
`9ee9c5f24c072779f06ce141b3340f953b614442`. The candidate and baseline test
counts, isolated-child transcripts, and all matrix outcomes matched. The
runtime/configuration proof for this T0 fixture is also:

```bash
git diff --name-status \
  9ee9c5f24c072779f06ce141b3340f953b614442..f8473b52552cc1ef5ed20b8e28e983102b70bd30 \
  -- src .qing
# (no output)

git diff --name-status \
  9ee9c5f24c072779f06ce141b3340f953b614442..f8473b52552cc1ef5ed20b8e28e983102b70bd30 \
  -- tests/a7_migration_characterization.rs
# A       tests/a7_migration_characterization.rs
```

Thus this comparison demonstrates the expected “tests only” baseline
observation; it does not prove a future migration is behavior-preserving.

## Verification record

The complete Rust verification below was run at HEAD
`757898023180ac8b4481edd5e82ccaf3cdc0576b` (before this record-only
addition), with `CARGO_BUILD_JOBS=2` and isolated target directory
`target/a7-t0-full`:

```bash
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-full cargo check --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-full \
  cargo clippy --all-features --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-full cargo test --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-full \
  cargo doc --all-features --no-deps
```

All five commands exited 0. The required focused command also exited 0:

```bash
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-focused-final \
  cargo test --test a7_migration_characterization \
  --test memory_bounds --test public_behavior \
  --test a6_task3_core_trap_test
```

It passed 85 tests (11 T0, 11 memory-bound, 46 public-behavior, and 17 A6
trap) with no failures. The required range check exited 0:

```bash
git diff --check \
  9ee9c5f24c072779f06ce141b3340f953b614442..757898023180ac8b4481edd5e82ccaf3cdc0576b
```

No T5 ACT4 or fresh guest-suite command was run for this Rust-only T0 round;
therefore this record makes no guest-suite or ACT4 pass claim.

## Review-fix verification record

The strengthened fixture was verified at test commit
`f8473b52552cc1ef5ed20b8e28e983102b70bd30` before the documentation-only
follow-up commit. The candidate and archived-baseline commands above each
passed **11 tests, 0 failures**, including the instrumented below-base fetch
and intermediate scalar/FP/host-write assertions.

The complete Rust verification at that fixture HEAD used isolated target
`target/a7-review-full`:

```bash
cargo fmt --all -- --check
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-review-full cargo check --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-review-full \
  cargo clippy --all-features --all-targets -- -D warnings
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-review-full cargo test --all-features
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-review-full \
  cargo doc --all-features --no-deps
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-review-full \
  cargo test --test a7_migration_characterization \
  --test memory_bounds --test public_behavior \
  --test a6_task3_core_trap_test
git diff --check \
  9ee9c5f24c072779f06ce141b3340f953b614442..f8473b52552cc1ef5ed20b8e28e983102b70bd30
```

All commands exited 0; the focused command passed 85 tests with no failures.
No T5 ACT4 or fresh guest-suite command was run for this review-fix round.

## Focused evidence and next dependency

Before adding this fixture, the existing focused baseline command also passed:

```bash
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a7-t0-preflight \
  cargo test --test memory_bounds --test public_behavior \
  --test a6_task3_core_trap_test
```

It passed 17 A6 trap tests, 11 memory-bound tests, and 46 public-behavior tests.
Those tests remain complementary evidence; T0 adds the mixed legacy-atomic,
HTIF-overlap, nonaligned-placement, and host-prefix assertions above rather than
claiming that component tests certify the target architecture.

T0 intentionally does **not** claim:

* a `PhysicalAccess` port, native adapter, or complete Hart/Runner migration;
* AMO/LR/SC atomicity, per-Hart reservation ownership, reset clearing, or full
  A-extension correctness;
* ADR-0002 compliance, universal physical-access unification, or A7 closeout;
* hardening of host writes or a new overflow/panic contract; or
* guest-suite/ACT4 execution evidence.

T1 may begin only after this matrix and its baseline/candidate comparison are
accepted. Any implementation that cannot retain a successful row, the shared
storage/lock domain, or the exact legacy callback behavior must escalate the
concrete call chain and result rather than silently denying the operation.
