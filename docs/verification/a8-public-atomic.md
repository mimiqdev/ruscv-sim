# A8 T4 — public atomic bridge exit and equivalence

**Status:** T4 implementation evidence only; this is not an A8 closeout.

**Authority:** `docs/dev-plan.md` §5.5, §5.6 D-c, §7.1–§7.3, §8 T4,
§10, and §11 items 6–7. The approved ADRs remain unchanged.

## Source and route audit

`load_and_run` (and therefore the CLI `run` command) and
`RiscVSimulator::load_elf` both install their core through
`src/executor.rs::install_image_with_physical_ports`. The typed inspection
handle and raw ports refer to the same installed storage/domain:

| Standard route | Core/backend installation | Atomic route |
| --- | --- | --- |
| CLI / `load_and_run` | The native closure creates instruction and data `ValidatedPhysicalAccess` handles over `SystemBus::physical_backend` for the same shared `Arc<Mutex<SystemBus>>` used by the typed observer. The bus holds the existing RAM, UART, and HTIF targets. | `SystemBus` implements the atomic target behind the validated data port. |
| `RiscVSimulator::new` / `load_elf` | The flat facade creates instruction and data `ValidatedPhysicalAccess` handles over `NativeRamBackend` and the same `Arc<Mutex<SimpleMemory>>` exposed by `memory()`. Image reload replaces the core and its storage together. | `NativeRamBackend` implements the atomic target over that existing RAM and bookkeeping domain. |
| `RiscvCore::new` without physical ports | No ports are installed; `step_outcome` reaches `LegacyTypedMemoryAdapter` for the old typed helpers. | Explicitly retained as the labeled non-conforming typed compatibility adapter; it is not a standard facade. |

In `src/core/mod.rs::RiscvCore::step_outcome`, the `Opcode::Amo` plus
installed-data-port branch locks the validated data port once, creates
`PhysicalAtomicAdapter`, and calls `execute_amo_port`. That branch has no
`MemoryInterface` read/write call. `src/isa/rv64a/dispatch.rs::execute_amo_port`
has one `access_atomic` call in each mutually exclusive RMW, LR, and SC arm;
Hart-side illegal-encoding, alignment, and guest-to-port conversion failures
occur before envelope dispatch. No-reservation and live-uncovered SC are not
Hart-side short circuits: each sends one StoreConditional envelope so the
target can validate the store span and capability before conditional failure.
`tests/a8_hart_atomic.rs::hart_side_rejections_issue_zero_requests` covers
only the Hart-side zero-request cases and the SC target checks.
The independent `a8_atomic_targets` spy verifies one target-visible backend
transaction for each admitted envelope. The T4 route guard in
`tests/a8_public_atomic_equivalence.rs` binds these source routes to the two
standard facade installers and ensures the typed constructor remains labeled.

This audit does not treat typed host inspection, tohost polling/clearing,
signature reads, or the old constructor as guest atomic instructions. It does
not add a port, memory, lock domain, reservation owner, or ISA arithmetic to a
backend.

## Public behavior covered

`tests/a8_public_atomic_equivalence.rs` creates mixed ordinary-plus-atomic ELF
fixtures and exercises the Cargo-built CLI binary, `load_and_run`, and the flat
facade. It asserts shared exit/cycle/final-PC/artifact facts, flat architectural
registers and bytes, and the AMO exit commit as the final retired instruction
before platform exit observation. Commit log entries are compared to the
already-retired instruction/PC sequence; logging consumes the retired fact and
does not refetch the opcode.

The suite also checks zero, short, exact, and final-permitted-slot budgets;
valid-RAM no-reservation SC conditionally fails without a platform exit, while
an unsupported-target SC faults without callback or exit; a rejected LR also
produces no platform exit. It checks P2a-precise between-run host-write
invalidation and disjoint-writer success; budget resume
without a writer; resume after exit and tohost clear; readable/unreadable
artifact differences; and image reload replacing storage and clearing the
Hart reservation. Native UART/HTIF availability, flat address offsets, and the
existing artifact-error policy remain documented configuration differences.

The native public facade dynamically exercises LR rejection at HTIF without a
platform exit and AMO-to-HTIF callback/exit ordering. D-c's covered-reservation
SC rejection and exact callback counts are additionally exercised against the
same native `SystemBus` data-port composition with a seeded reservation in the
core seam. A standard guest cannot create a reservation covering HTIF because
LR at that endpoint is rejected; therefore the covered-HTIF SC case is not
literally injectable through the public runner and remains explicitly labeled
as a direct-core test. In the follow-up SC correction, public ELF guests now
also execute LR.W/D on RAM then SC.W/D at HTIF with a live uncovered
reservation. Their trap handlers assert cause, original `mtval`, unchanged
`rd`, untouched RAM, retained reservation via a successful retry, and no
premature tohost exit. `tests/a8_atomic_targets.rs` provides independent target
policy and exactly-once callback proof. This boundary remains explicit without
adding a production injection API.

**Follow-up SC correction:** the previous T4 observation that no-reservation
SC was a Hart-side rejection is superseded by the maintainer-authorized
spec-first correction. A legal aligned SC now converts its guest address and
issues exactly one StoreConditional envelope even without reservation
context. Valid RAM validates the target then returns conditional failure
(`rd=1`, no guest-byte read/write or bookkeeping bump); HTIF D-c, UART, and
unmapped targets reject with store/AMO access fault before any conditional
failure. A live uncovered context follows the same target-check order, and a
faulting SC retains that reservation. Focused contract, target, Hart, and
facade regressions assert the new results; they are a separate follow-up and
do not retroactively change the behavior observed at this T4 evidence head.

## Fresh-built guest suite

The fresh-build scripts now keep the existing RV64I/RV64M sequence intact and
append the project-authored RV64A guests under `tests/bare-metal-riscv-test/rv64a/`.
The existing five A6 trap guests remain required. The new guests cover an
LR/SC retry loop; AMOSWAP/ADD/MIN/MAX at W and D widths; aq/rl encodings;
SC-after-overlapping-store failure; an atomic access at the final aligned RAM
dword; and an AMO that writes the tohost exit signal. `atomic_near_ram_end.S`
uses LR.D/SC.D at the last aligned dword of the 64-KiB image RAM and signals
success separately through `.tohost`; the address derivation and observed
public-runner result are recorded below. Historical guest-set identities are
not merged or redefined here, and this T4 record does not state a new suite
total.

**Observed fresh build/run:** source revision
`7f1780de0f87048b81455b4da3176bf8e3599b71` was mounted from this task
worktree into the local development image
`ghcr.io/mimiqdev/ruscv-sim-dev:main`
(`sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`).
The image reported GNU assembler and linker 2.40. Cargo used
`target/container-cargo`; fresh guest artifacts used
`target/container-riscv-elves`; `RISCV_SOURCE_HEAD` was passed from the host
because the mounted worktree's `.git` indirection is not container-resolvable.
The rebuilt `atomic_near_ram_end.elf` has a PT_LOAD segment at
`0x80000000` with `p_memsz=0x48` (confirmed by the image's
`riscv64-unknown-elf-readelf -l`). `load_elf_file` therefore allocates
`max(next_power_of_two(0x48), 0x10000) = 0x10000` bytes, covering
`[0x80000000, 0x80010000)`; `0x8000fff8` is its actual final aligned dword.
The guest's successful CLI exit demonstrates LR.D/SC.D success at that address
and its subsequent success signal through `.tohost`. The invocation was:

```bash
head=$(git rev-parse HEAD)
docker run --rm --init --env RISCV_SOURCE_HEAD="$head" \
  --volume "$PWD:/workspace" --workdir /workspace \
  ghcr.io/mimiqdev/ruscv-sim-dev:main bash -c \
  'export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/container-cargo RISCV_TEST_OUTDIR=target/container-riscv-elves RISCV_SOURCE_HEAD="${RISCV_SOURCE_HEAD}" RISCV_REQUIRE_RISCV_TOOLCHAIN=1; ./scripts/run_elf_tests.sh'
```

`run_elf_tests.sh` freshly assembled/linked the source suite and completed
public CLI execution successfully, including all seven added A8 atomic guests:

| Guest | CLI cycles | Result |
| --- | ---: | --- |
| `rv64a/amo_d.S` | 28 | exit 0 |
| `rv64a/amo_w.S` | 29 | exit 0 |
| `rv64a/aq_rl_set.S` | 23 | exit 0 |
| `rv64a/atomic_near_ram_end.S` | 12 | exit 0 |
| `rv64a/atomic_tohost_exit.S` | 4 | exit 0 |
| `rv64a/lrsc_loop.S` | 16 | exit 0 |
| `rv64a/sc_after_store_failure.S` | 20 | exit 0 |

The historical RV64I/RV64M cases remained ahead of the RV64A additions and
also passed in that invocation. This is a T4 run record, not an A8 closeout or
an aggregate-suite-total claim; the updated total belongs to T5. A missing
RISC-V cross-toolchain follows the existing skip policy (status 77) and is
unavailable evidence, not a guest pass. CI continues to treat a missing
toolchain as a failure when `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`.

## Explicit boundaries

This is not T5 evidence collection or milestone closeout. It does not run ACT4,
claim §7.3 attainment, certify an ISA extension, or assert full ADR-0002
conformance. It does not establish multi-Hart/DMA coherence, MMU/TLM
integration, or any of the other §10 non-claims.
