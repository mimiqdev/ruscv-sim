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
Hart-side illegal-encoding, alignment, and SC-reservation/span prechecks occur
before an envelope is admitted. The precheck zero-request matrix is also
executable in `tests/a8_hart_atomic.rs::hart_side_rejections_issue_zero_requests`.
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
Hart-rejected SC and a rejected LR produce no platform exit; P2a-precise
between-run host-write invalidation and disjoint-writer success; budget resume
without a writer; resume after exit and tohost clear; readable/unreadable
artifact differences; and image reload replacing storage and clearing the
Hart reservation. Native UART/HTIF availability, flat address offsets, and the
existing artifact-error policy remain documented configuration differences.

The native public facade dynamically exercises LR rejection at HTIF without a
platform exit and AMO-to-HTIF callback/exit ordering. D-c's valid-reservation
SC rejection and exact callback counts are additionally exercised against the
same native `SystemBus` data-port composition with a seeded reservation in the
core seam. A standard guest cannot create that reservation: LR at the D-c
endpoint is rejected, and `load_and_run` exposes no initial-Hart-state or
backend-injection API. Therefore the valid-reservation SC case is not literally
injectable through the public runner; the direct-core case is explicit and is
not represented as a public-runner execution. `tests/a8_atomic_targets.rs`
provides the independent target policy and exactly-once callback proof. This
boundary must remain visible rather than being papered over by a new production
injection API.

The public facade's no-reservation SC is a Hart-side rejection, not a test of
D-c target rejection: it retires with `rd=1` and issues no envelope, as
specified by §5.6. A valid-reservation SC target fault is checked separately
as described above.

## Fresh-built guest suite

The fresh-build scripts now keep the existing RV64I/RV64M sequence intact and
append the project-authored RV64A guests under `tests/bare-metal-riscv-test/rv64a/`.
The existing five A6 trap guests remain required. The new guests cover an
LR/SC retry loop; AMOSWAP/ADD/MIN/MAX at W and D widths; aq/rl encodings;
SC-after-overlapping-store failure; an atomic access at the final aligned RAM
dword; and an AMO that writes the tohost exit signal. Historical guest-set
identities are not merged or redefined here, and this T4 record does not state
a new suite total.

A missing RISC-V cross-toolchain follows the existing skip policy (status 77);
that outcome is unavailable evidence, not a guest pass. CI continues to treat
a missing toolchain as a failure when `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`.

## Explicit boundaries

This is not T5 evidence collection or milestone closeout. It does not run ACT4,
claim §7.3 attainment, certify an ISA extension, or assert full ADR-0002
conformance. It does not establish multi-Hart/DMA coherence, MMU/TLM
integration, or any of the other §10 non-claims.
