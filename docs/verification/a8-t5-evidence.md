# A8 T5 — exact-head evidence and bounded acceptance audit

**Status:** T5 evidence/assessment record; this is not a milestone transition.
`docs/dev-plan.md` remains the sole current contract. No successor is approved
or implied, and the A8 result is bounded to the evidence below.

**Authority:** `docs/dev-plan.md` §§7.3, 8 T0–T5, 9–11; accepted ADR-0001
through ADR-0004; the A8 T0 and T4 records; current source and tests. This
record does not amend an ADR or replace the active plan.

## Revision and evidence identity

The task integration base is exactly
`f386b34ede9f184ed6fe3deff9ca19b2b0b758e3`. The A8 implementation audited
here is T4 commit `f386b34ede9f184ed6fe3deff9ca19b2b0b758e3`; earlier task
increments are T0 `c8bf500b7dc77f89b50df20711ad3d19bb3caba7`, T1
`3ae2aa8a46042f8962fcb805cad01a5cb6026d4b`, T2
`fc2bf2c1d1db90e8058a962674f22f9e4ead636a`, and T3
`a78bf1d85e45a501aee9952e15160198a674a31d`. T0–T4 implementation/test
claims below are independently checked again at the exact T5 evidence HEAD.
That exact commit is the commit containing this record; `qing verify` binds
each observed command to that immutable HEAD.

## Acceptance audit — T0 through T4

The table separates component-level proof, Hart-route proof, and public
end-to-end proof. A source module existing is not counted as integration.

| Contract row | Audited source and executable assertions | Evidence boundary / disposition |
| --- | --- | --- |
| **T0 — A7 atomic fixture reclassification and executable old baseline** | `docs/verification/a8-fixture-reclassification.md` maps every A7 atomic fixture/transcript row to keep/flip/retire/relabel and cites its approving §6 row. `tests/a8_atomic_baseline.rs` retains the old-column baseline in history; at T3 the file was deliberately converted to the approved new behavior. The current tests, `tests/a7_migration_characterization.rs`, `tests/a7_legacy_atomic_compat.rs`, and `tests/amo_test.rs` cover the reclassified outcomes. | Ledger is the durable classification; the implementation no longer claims the old expected outcomes. T0 command is rerun at the T5 head. |
| **T1 — envelope vocabulary and Hart-owned arithmetic** | `src/physical.rs` separates `AtomicRequest`/`AtomicResponse` and `ValidatedAtomicAccess` from ordinary request pairs. `src/hart_amods.rs` is the sole operation arithmetic/transform implementation. `tests/a8_atomic_contract.rs` covers only 4/8-byte widths, payload/transform/request/response binding, snapshots, all AMO operations at W/D, aq/rl information, taxonomy, wrapping spans, and terminal unknown completion. Its `ordinary_read_write_pairs_cannot_represent_amo_or_sc` and `atomic_unknown_completion_is_terminal_and_never_retried` assertions exercise the envelope boundary. | Component/vocabulary and arithmetic proof, not target wiring or public behavior. Exact-head focused result recorded below. |
| **T2 — native target transactions, D-c devices, and write versions** | `src/physical.rs::{native_ram_atomic_transact, NativeRamBackend, NativeSystemBusBackend}` and `src/memory/mod.rs` implement atomic critical sections and committed-write bookkeeping in the shared storage. `tests/a8_atomic_targets.rs` covers RAM validation/effects, rejected spans and no-partial-effect, UART width rejection, HTIF D-c, callback-once, one externally visible transaction, competing readers, backend arithmetic parity, poisoned locks, unknown completion after possible effect, and each storage write/prefix bookkeeping rule. | Native target/component evidence. It is not by itself Hart decode or public-facade evidence. Exact-head focused result recorded below. |
| **T3 — ISA decode, Hart reservation, faults, and writers W1–W8** | `src/isa/rv64a/dispatch.rs`, `src/isa/rv64a/lr_sc.rs`, `src/hart_amods.rs`, and `src/core/mod.rs::step_outcome` implement the Hart semantics. `tests/a8_hart_atomic.rs` and `tests/a8_atomic_baseline.rs` exercise the exhaustive 32-`funct5` × W/D matrix, malformed LR, result widths/sign extension, `rd=x0`, aq/rl, alignment/zero-request cases, span containment, reset/reload, per-Core isolation, faulting-LR preservation, faulting-SC retain, and Hart-side rejection. Writer tests cover overlapping and disjoint stores/AMOs/host changes, the `memory()` handle, failed-prefix writes, cross-thread exclusion, exit/`clear_tohost` resume, and budget resume. `GLOBAL_RESERVATION` and the exported `clear_reservation` production API are absent. | Hart/test-object evidence is not multi-Hart product support. The typed `RiscvCore::new` compatibility adapter shares Hart reservation state but cannot observe storage write-version snapshots; it remains expressly non-conforming. Exact-head focused result recorded below. |
| **T4 — standard-facade bridge exit and public equivalence** | `src/core/mod.rs::step_outcome` sends standard-port atomics through one validated envelope; `src/executor.rs::install_image_with_physical_ports` installs the shared native/flat domains. `tests/a8_public_atomic_equivalence.rs::standard_facades_route_admitted_atomics_only_through_one_validated_envelope`, mixed guest equivalence, budget, writer-resume, artifact/reload, and HTIF cases exercise the CLI, `load_and_run`, and flat facade. The T4 route audit is `docs/verification/a8-public-atomic.md`. | Public end-to-end evidence for the exercised cases. For D-c, a valid-reservation SC cannot be seeded through the public runner API; its endpoint rejection is separately tested at the direct-core/native-domain seam, not represented as a public-runner execution. The old typed constructor is not a standard facade. |

## T5 exact-head observations

All rows in this section must be observed on the clean committed HEAD
containing this record. `qing verify` is the exact-HEAD observation mechanism;
its recorded `passed`/`failed`/`not_observed` status is authoritative. Commands
listed here are not inferred from an earlier task head.

| Check | Exact command / invocation | Result |
| --- | --- | --- |
| Rust formatting | `cargo fmt --all -- --check` | Pending exact-head observation |
| Rust check | `cargo check --all-features` | Pending exact-head observation |
| Strict Clippy | `cargo clippy --all-features --all-targets -- -D warnings` | Pending exact-head observation |
| All-feature Rust tests | `cargo test --all-features` | Pending exact-head observation |
| Rust docs | `cargo doc --all-features --no-deps` | Pending exact-head observation |
| ELF script guards | `bash scripts/test_riscv_elf_guards.sh` | Pending exact-head observation |
| Required A6 guest integration | `RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration` | Pending exact-head observation; run in the pinned development container if host tools are absent |
| Focused T0 | `cargo test --test a8_atomic_baseline --test a7_migration_characterization --test a7_legacy_atomic_compat --test amo_test` | Pending exact-head observation |
| Focused T1 | `cargo test --test a8_atomic_contract --test a7_physical_contract` | Pending exact-head observation |
| Focused T2 | `cargo test --test a8_atomic_targets --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests` | Pending exact-head observation |
| Focused T3 | `cargo test --test a8_hart_atomic --test a8_atomic_baseline --test amo_test --test a7_migration_characterization --test a7_legacy_atomic_compat --test a6_task3_core_trap_test --test trap_test --test csr_access_test --test mret_conformance_test` | Pending exact-head observation |
| T3 in-crate ISA regressions | `cargo test --lib isa::rv64a` | Pending exact-head observation |
| Focused T4 | `cargo test --test a8_public_atomic_equivalence --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test` | Pending exact-head observation |
| Pinned-base whitespace/error check | `git diff --check f386b34ede9f184ed6fe3deff9ca19b2b0b758e3..HEAD` | Pending exact-head observation |
| Fresh project guest build and public CLI run | The two commands below, in the exact pinned local development image and worktree mount | Pending; per-case results and the 51 + 7 total are recorded below only after observed |

Fresh guest invocation (the second script builds afresh as well):

```bash
head=$(git rev-parse HEAD)
docker run --rm --init --env RISCV_SOURCE_HEAD="$head" \
  --volume "$PWD:/workspace" --workdir /workspace \
  ghcr.io/mimiqdev/ruscv-sim-dev:main bash -c \
  'export CARGO_BUILD_JOBS=2 \
    CARGO_TARGET_DIR=target/a8-t5-container-cargo \
    RISCV_TEST_OUTDIR=target/a8-fresh-riscv-elves \
    RISCV_REQUIRE_RISCV_TOOLCHAIN=1 \
    RISCV_SOURCE_HEAD="${RISCV_SOURCE_HEAD}"; \
    ./scripts/compile_riscv_tests.sh && ./scripts/run_elf_tests.sh'
```

Container identity observed locally: the `:main` tag resolved to
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`.
The image must be invoked by non-login `bash -c` so `/opt/riscv/bin` remains
on `PATH`; the host's linked-worktree `.git` file is not container-resolvable,
so `RISCV_SOURCE_HEAD` is passed from the host. The image's Rust/binutils
versions and architecture are recorded in the observed run below.

### Fresh project guests

The retained project set and the A8 additions are distinct from ACT4. The
retained 51 sources are the `rv64i/` and `rv64m/` cases; the seven A8 cases are
`rv64a/amo_d.S`, `rv64a/amo_w.S`, `rv64a/aq_rl_set.S`,
`rv64a/atomic_near_ram_end.S`, `rv64a/atomic_tohost_exit.S`,
`rv64a/lrsc_loop.S`, and `rv64a/sc_after_store_failure.S`.

| Case group | Cases | Fresh compile | Public CLI outcome |
| --- | ---: | --- | --- |
| Retained RV64I/RV64M guest set | 51 | Pending per-case result | Pending per-case result |
| New A8 RV64A atomic guests (listed above) | 7 | Pending per-case result | Pending per-case result |
| **Fresh project guest total** | **58 if every required case is present and passes** | Pending | Pending |

Per-case results (all guest names and observed public CLI outcomes) are
populated from the fresh `compile_riscv_tests.sh` and `run_elf_tests.sh` logs.
The claimed total is **not** combined with the separate ACT4 case count.

### ACT4 identity — historical and unchanged

The frozen ACT4 selection remains the historical 51-case nontrapping RV64I
selection recorded in `docs/verification/a7-act4-replay-35432032788.json`:
workflow run `35432032788`, source head
`fb6f51c771f32585f1547422c9633379f7ae370b`, artifact ID `10580339624`,
21,675,677 bytes, SHA-256
`a8d587f8310cb54e923c9a48c5b83f931ecf6b2054fd698a4b4d356802d95877`.
This T5 task does **not** replay, regenerate, or expand ACT4 and does not
relabel that A7 result as current-head verification. It supplies no atomic or
extension certification claim.

## Residual-debt ledger and explicit limits

This updates the status of the debt named by the approved-for-planning
post-A7 roadmap §8 without scheduling a successor or changing the active plan.
The T5 task does not turn component presence into integration evidence.

| Debt / boundary | Current A8 T5 disposition | Evidence needed to revisit |
| --- | --- | --- |
| Stage 2 — Hart facts and safe N=1 Machine lifecycle | **Deferred / not delivered by A8.** A8's `InstructionRetired`/trap core seams do not establish the accepted ADR-0001 complete observation plane, a Machine composition root, safe reset/image lifecycle, or lifecycle quiesce/drain. | Separately approved Stage 2 scope and acceptance evidence; do not infer it from A8 facade tests. |
| MMU / Sv39 / A/D / PMP | **Component only / not public-path integration.** The public core still uses image-base/storage adaptation; MMU page walks/A-D writes are not routed through the A8 physical atomic/data port, and PMP enforcement is not demonstrated. Existing translation/A-D component tests are not public ELF integration. | Approved translation/profile scope, one physical route for walks and A/D, public guest/fault evidence, and selected PMP behavior. |
| TLM / DMI / SystemC / FFI | **Component/adapter vocabulary only.** Existing Rust TLM and DMI tests do not prove a Hart physical adapter, native/TLM parity, atomic envelope/serialization, DMI invalidation, or an external-kernel integration. No such route is claimed. | Approved external integration consumer, pinned environment, adapter atomic/fault/delay/lifecycle tests, and DMI invalidation proof. |
| Performance facility and runtime performance | **Not measured or delivered by A8.** No performance facility, optimization, or performance acceptance gate was run. The A7 1.2858 public-loop ratio is a historical observation bound to its own A7 heads/environment; it is not current A8 performance evidence. | Independent Stage 3 performance-infrastructure decision/evidence; any optimization remains separate scope. |
| Multi-Hart, DMA, ordering, and RVWMO | **Not supported or claimed.** Two-Core tests establish only independent test-object reservations; they do not provide multi-Hart execution, DMA coherence, global ordering, or RVWMO proof. | Separate inbound-master/multi-Hart contracts and conformance evidence. |
| ISA/ACT4 scope | **No RV64A/Zalrsc/Zamo/full-ISA certification.** Seven project-authored atomic guests and Rust tests are project regressions, not an external conformance suite. ACT4 remains its unchanged, separately identified nontrapping RV64I set. | A separately approved profile, suite/toolchain, and acceptance budget. |
| Public D-c valid-reservation SC injection | **Bounded seam limitation.** The standard public runner cannot seed a live reservation before HTIF because its LR is rejected; the valid-reservation HTIF SC target rejection is evidenced at the native core seam, not misreported as a public-runner case. | No production injection API is added for this T5 task; revisit only with a separately justified public test/configuration need. |

Other A8 §10 non-claims remain: no DMA/inbound masters, multi-Hart coherence,
RVWMO proof, MMU/PMP/page-walk integration, interrupt/WFI/time behavior,
Debug Mode, Machine lifecycle, or performance optimization. There is no ACT4
rerun or atomic extension certification.

## Bounded A8 conclusion

The §7.3 attainment statement is authorized **only if** every T0–T5 criterion
above has a `qing verify`-bound exact-head pass and the fresh guest total is
confirmed. If any required criterion/check is missing, stale, failed, skipped,
or inconclusive, replace this conclusion with that exact limitation and do not
close A8. No plan archival/replacement, successor approval, merge, tag,
publication, or workflow completion is part of this record.
