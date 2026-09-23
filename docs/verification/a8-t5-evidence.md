# A8 T5 — exact-head evidence and bounded acceptance audit

**Status:** T5 exact-head evidence is recorded for reviewed commit
`45a170ff67aef850acc835f5f8084cef186fa3b8`. This provenance correction is a
later documentation-only follow-up; it does not assert fresh verification or
§7.3 attainment at its new HEAD. PR review and repository delivery remain
separate. `docs/dev-plan.md` remains the sole current contract; no successor is
approved or implied.

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
`a78bf1d85e45a501aee9952e15160198a674a31d`.

**Exact reviewed/verified T5 HEAD:**
`45a170ff67aef850acc835f5f8084cef186fa3b8`, commit subject
`docs(a8): record T5 exact-head evidence`, parent
`1b4bd2acaa970b4ac297cfacaac982150dfd4eac`. The checks, commands, toolchain
identity, fresh ELF manifest source identity, and all 58 per-case results below
are scoped to this full commit SHA. This repository record is the durable
verification provenance; qing check IDs are intentionally not used as evidence
because they are execution-system references.

The follow-up that adds this explicit identity changes only this evidence
record. The results below remain exact evidence for `45a170...` and are marked
**stale for the later follow-up HEAD**. No §7.3 attainment claim is made for
that new HEAD unless its required checks are freshly recorded there.

## Acceptance audit — T0 through T4

The table separates component-level proof, Hart-route proof, and public
end-to-end proof. A source module existing is not counted as integration.

| Contract row | Audited source and executable assertions | Evidence boundary / disposition |
| --- | --- | --- |
| **T0 — A7 atomic fixture reclassification and executable old baseline** | `docs/verification/a8-fixture-reclassification.md` maps every A7 atomic fixture/transcript row to keep/flip/retire/relabel and cites its approving §6 row. `tests/a8_atomic_baseline.rs` retains the old-column baseline in history; at T3 the file was deliberately converted to the approved new behavior. The current tests, `tests/a7_migration_characterization.rs`, `tests/a7_legacy_atomic_compat.rs`, and `tests/amo_test.rs` cover the reclassified outcomes. | Ledger is the durable classification; the implementation no longer claims the old expected outcomes. The focused T0 command passed at exact HEAD `45a170ff67aef850acc835f5f8084cef186fa3b8`. |
| **T1 — envelope vocabulary and Hart-owned arithmetic** | `src/physical.rs` separates `AtomicRequest`/`AtomicResponse` and `ValidatedAtomicAccess` from ordinary request pairs. `src/hart_amods.rs` is the sole operation arithmetic/transform implementation. `tests/a8_atomic_contract.rs` covers only 4/8-byte widths, payload/transform/request/response binding, snapshots, all AMO operations at W/D, aq/rl information, taxonomy, wrapping spans, and terminal unknown completion. Its `ordinary_read_write_pairs_cannot_represent_amo_or_sc` and `atomic_unknown_completion_is_terminal_and_never_retried` assertions exercise the envelope boundary. | Component/vocabulary and arithmetic proof, not target wiring or public behavior. Exact-head focused result recorded below. |
| **T2 — native target transactions, D-c devices, and write versions** | `src/physical.rs::{native_ram_atomic_transact, NativeRamBackend, NativeSystemBusBackend}` and `src/memory/mod.rs` implement atomic critical sections and committed-write bookkeeping in the shared storage. `tests/a8_atomic_targets.rs` covers RAM validation/effects, rejected spans and no-partial-effect, UART width rejection, HTIF D-c, callback-once, one externally visible transaction, competing readers, backend arithmetic parity, poisoned locks, unknown completion after possible effect, and each storage write/prefix bookkeeping rule. | Native target/component evidence. It is not by itself Hart decode or public-facade evidence. Exact-head focused result recorded below. |
| **T3 — ISA decode, Hart reservation, faults, and writers W1–W8** | `src/isa/rv64a/dispatch.rs`, `src/isa/rv64a/lr_sc.rs`, `src/hart_amods.rs`, and `src/core/mod.rs::step_outcome` implement the Hart semantics. `tests/a8_hart_atomic.rs` and `tests/a8_atomic_baseline.rs` exercise the exhaustive 32-`funct5` × W/D matrix, malformed LR, result widths/sign extension, `rd=x0`, aq/rl, alignment/zero-request cases, span containment, reset/reload, per-Core isolation, faulting-LR preservation, faulting-SC retain, and Hart-side rejection. Writer tests cover overlapping and disjoint stores/AMOs/host changes, the `memory()` handle, failed-prefix writes, cross-thread exclusion, exit/`clear_tohost` resume, and budget resume. `GLOBAL_RESERVATION` and the exported `clear_reservation` production API are absent. | Hart/test-object evidence is not multi-Hart product support. The typed `RiscvCore::new` compatibility adapter shares Hart reservation state but cannot observe storage write-version snapshots; it remains expressly non-conforming. Exact-head focused result recorded below. |
| **T4 — standard-facade bridge exit and public equivalence** | `src/core/mod.rs::step_outcome` sends standard-port atomics through one validated envelope; `src/executor.rs::install_image_with_physical_ports` installs the shared native/flat domains. `tests/a8_public_atomic_equivalence.rs::standard_facades_route_admitted_atomics_only_through_one_validated_envelope`, mixed guest equivalence, budget, writer-resume, artifact/reload, and HTIF cases exercise the CLI, `load_and_run`, and flat facade. The T4 route audit is `docs/verification/a8-public-atomic.md`. | Public end-to-end evidence for the exercised cases. For D-c, a valid-reservation SC cannot be seeded through the public runner API; its endpoint rejection is separately tested at the direct-core/native-domain seam, not represented as a public-runner execution. The old typed constructor is not a standard facade. |

## T5 exact-head observations

The commands and outcomes below were observed on clean committed HEAD
`45a170ff67aef850acc835f5f8084cef186fa3b8`. This table is the durable
provenance and does not rely on private qing check identifiers. All results
are stale for the later documentation-only provenance follow-up HEAD; this
record explicitly makes no fresh exact-head claim for that follow-up.

| Check | Exact command | Observed result at `45a170ff67aef850acc835f5f8084cef186fa3b8` |
| --- | --- | --- |
| Rust formatting | `cargo fmt --all -- --check` | PASS |
| Rust check | `cargo check --all-features` | PASS |
| Strict Clippy | `cargo clippy --all-features --all-targets -- -D warnings` | PASS |
| All-feature Rust tests | `cargo test --all-features` | PASS |
| Rust docs | `cargo doc --all-features --no-deps` | PASS |
| ELF script guards | `bash scripts/test_riscv_elf_guards.sh` | PASS |
| Required A6 guest integration | `RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration` | PASS in pinned container; 1 integration test |
| Focused T0 | `cargo test --test a8_atomic_baseline --test a7_migration_characterization --test a7_legacy_atomic_compat --test amo_test` | PASS |
| Focused T1 | `cargo test --test a8_atomic_contract --test a7_physical_contract` | PASS |
| Focused T2 | `cargo test --test a8_atomic_targets --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests` | PASS |
| Focused T3 | `cargo test --test a8_hart_atomic --test a8_atomic_baseline --test amo_test --test a7_migration_characterization --test a7_legacy_atomic_compat --test a6_task3_core_trap_test --test trap_test --test csr_access_test --test mret_conformance_test` | PASS |
| T3 in-crate ISA regressions | `cargo test --lib isa::rv64a` | PASS; 34 tests |
| Focused T4 | `cargo test --test a8_public_atomic_equivalence --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test` | PASS |
| Pinned-base whitespace/error check | `git diff --check f386b34ede9f184ed6fe3deff9ca19b2b0b758e3..45a170ff67aef850acc835f5f8084cef186fa3b8` | PASS |
| Fresh project guest build and public CLI run | Exact container invocation below; `run_elf_tests.sh` recompiles after the explicit compile | PASS; 58/58 compiled and 58/58 public CLI cases |

The full Rust gate ran in the local ARM64 development image. Its exact
invocation at HEAD `45a170ff67aef850acc835f5f8084cef186fa3b8` was:

```bash
docker run --rm --init \
  --volume /Users/shinymimiq/Developer/personal/ruscv-sim-a8-t5-bounded-evidence:/workspace \
  --workdir /workspace ghcr.io/mimiqdev/ruscv-sim-dev:main bash -c \
  'set -euo pipefail; export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a8-t5-container-cargo; \
    cargo fmt --all -- --check && cargo check --all-features && \
    cargo clippy --all-features --all-targets -- -D warnings && \
    cargo test --all-features && cargo doc --all-features --no-deps'
```

The host lacked `riscv64-unknown-elf` assembler/linker tools, so no host skip
was taken: the A6 integration and fresh guest build/run used the already-local
development image, mounted this exact task worktree at `/workspace`. The
actual invocation included the required A6 test and captured full guest logs
under ignored `target/` paths:

```bash
docker run --rm --init --env RISCV_SOURCE_HEAD=45a170ff67aef850acc835f5f8084cef186fa3b8 \
  --volume /Users/shinymimiq/Developer/personal/ruscv-sim-a8-t5-bounded-evidence:/workspace \
  --workdir /workspace ghcr.io/mimiqdev/ruscv-sim-dev:main bash -c \
  'set -euo pipefail; export CARGO_BUILD_JOBS=2 \
    CARGO_TARGET_DIR=target/a8-t5-container-cargo \
    RISCV_TEST_OUTDIR=target/a8-fresh-riscv-elves \
    RISCV_REQUIRE_RISCV_TOOLCHAIN=1 \
    RISCV_SOURCE_HEAD="${RISCV_SOURCE_HEAD}"; \
    cargo test --test a6_trap_elf_integration; \
    ./scripts/compile_riscv_tests.sh 2>&1 | tee target/a8-t5-compile.log; \
    ./scripts/run_elf_tests.sh 2>&1 | tee target/a8-t5-run.log'
```

The host passed `RISCV_SOURCE_HEAD=45a170ff67aef850acc835f5f8084cef186fa3b8`
to Docker because the mounted worktree's `.git` indirection is not container-
resolvable. The container used non-login `bash -c` so `/opt/riscv/bin` remained
on `PATH`. The full gate used the isolated container Cargo target directory
and ran the five listed cargo commands in order. Focused test commands ran on
the host Rust toolchain. The source versions actually observed were:

| Environment | Identity |
| --- | --- |
| Host focused-test toolchain | `rustc 1.98.1 (48a229cea 2026-09-01)`; `cargo 1.98.1 (797e8a9bc 2026-08-05)`; host has no `riscv64-unknown-elf-as`/`gcc` |
| Container image | `ghcr.io/mimiqdev/ruscv-sim-dev:main`, locally resolved immutable digest `sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`; `aarch64` |
| Container toolchain | `rustc 1.97.1 (8bab26f4f 2026-07-14)`; `cargo 1.97.1 (c980f4866 2026-06-30)`; GNU RISC-V assembler/linker/readelf 2.40 (`2.40-2+4+b1`) |
| Container settings | `CARGO_BUILD_JOBS=2`; `CARGO_TARGET_DIR=target/a8-t5-container-cargo`; `RISCV_TEST_OUTDIR=target/a8-fresh-riscv-elves`; `RISCV_REQUIRE_RISCV_TOOLCHAIN=1` |

The compile and run scripts each freshly assembled/linked 58 guests (the run
script removed/rebuilt its output itself). `manifest.txt` recorded `source_head=45a170ff67aef850acc835f5f8084cef186fa3b8`,
`source_count=58`, `compiled_count=58`, and `failed_count=0`.
`run_elf_tests.sh` reported `Total: 58`, `Passed: 58`, `Failed: 0`; all
entries were checked against `_start`. GNU ld emitted non-fatal RWX PT_LOAD
warnings for 12 small ELF fixtures; none failed to link or execute. No check
failed, skipped, or remained unavailable.

### Fresh project guests

The retained project set and the A8 additions are distinct from ACT4. The
retained 51 sources are 43 RV64I plus 8 RV64M guests (including the five A6
trap guests); the seven A8 atomic cases are appended without reclassifying the
retained set. Every case below freshly compiled and ran through the public CLI
with exit code 0 and status `SUCCESS` at the exact source HEAD in the manifest.
Cycles are the public CLI results from `target/a8-t5-run.log`.

**Fresh project total: 58 compiled and 58 passed (0 failed).** This is not
merged with the separate ACT4 selection.

| # | Guest | CLI result |
| ---: | --- | --- |
| 1 | `rv64i/add.elf` | exit 0; 53 cycles; PASS |
| 2 | `rv64i/addi.elf` | exit 0; 32 cycles; PASS |
| 3 | `rv64i/and.elf` | exit 0; 45 cycles; PASS |
| 4 | `rv64i/andi.elf` | exit 0; 44 cycles; PASS |
| 5 | `rv64i/beq.elf` | exit 0; 23 cycles; PASS |
| 6 | `rv64i/bge.elf` | exit 0; 29 cycles; PASS |
| 7 | `rv64i/bgeu.elf` | exit 0; 34 cycles; PASS |
| 8 | `rv64i/blt.elf` | exit 0; 29 cycles; PASS |
| 9 | `rv64i/bltu.elf` | exit 0; 29 cycles; PASS |
| 10 | `rv64i/bne.elf` | exit 0; 26 cycles; PASS |
| 11 | `rv64i/csrrc.elf` | exit 0; 48 cycles; PASS |
| 12 | `rv64i/csrrci.elf` | exit 0; 46 cycles; PASS |
| 13 | `rv64i/csrrs.elf` | exit 0; 48 cycles; PASS |
| 14 | `rv64i/csrrsi.elf` | exit 0; 39 cycles; PASS |
| 15 | `rv64i/csrrw.elf` | exit 0; 44 cycles; PASS |
| 16 | `rv64i/csrrwi.elf` | exit 0; 28 cycles; PASS |
| 17 | `rv64i/fib.elf` | exit 0; 68 cycles; PASS |
| 18 | `rv64i/hello.elf` | exit 0; 58 cycles; PASS |
| 19 | `rv64i/jal.elf` | exit 0; 28 cycles; PASS |
| 20 | `rv64i/jalr.elf` | exit 0; 34 cycles; PASS |
| 21 | `rv64i/lb.elf` | exit 0; 63 cycles; PASS |
| 22 | `rv64i/lh.elf` | exit 0; 57 cycles; PASS |
| 23 | `rv64i/lw.elf` | exit 0; 49 cycles; PASS |
| 24 | `rv64i/lwu.elf` | exit 0; 87 cycles; PASS |
| 25 | `rv64i/or.elf` | exit 0; 45 cycles; PASS |
| 26 | `rv64i/ori.elf` | exit 0; 47 cycles; PASS |
| 27 | `rv64i/sb.elf` | exit 0; 127 cycles; PASS |
| 28 | `rv64i/sh.elf` | exit 0; 105 cycles; PASS |
| 29 | `rv64i/sll.elf` | exit 0; 36 cycles; PASS |
| 30 | `rv64i/slli.elf` | exit 0; 36 cycles; PASS |
| 31 | `rv64i/sra.elf` | exit 0; 40 cycles; PASS |
| 32 | `rv64i/srai.elf` | exit 0; 34 cycles; PASS |
| 33 | `rv64i/srl.elf` | exit 0; 36 cycles; PASS |
| 34 | `rv64i/srli.elf` | exit 0; 36 cycles; PASS |
| 35 | `rv64i/sub.elf` | exit 0; 28 cycles; PASS |
| 36 | `rv64i/sw.elf` | exit 0; 103 cycles; PASS |
| 37 | `rv64i/trap_ebreak.elf` | exit 0; 41 cycles; PASS |
| 38 | `rv64i/trap_ecall.elf` | exit 0; 51 cycles; PASS |
| 39 | `rv64i/trap_illegal.elf` | exit 0; 44 cycles; PASS |
| 40 | `rv64i/trap_mret_priv.elf` | exit 0; 65 cycles; PASS |
| 41 | `rv64i/trap_vectored.elf` | exit 0; 41 cycles; PASS |
| 42 | `rv64i/xor.elf` | exit 0; 44 cycles; PASS |
| 43 | `rv64i/xori.elf` | exit 0; 48 cycles; PASS |
| 44 | `rv64m/div.elf` | exit 0; 71 cycles; PASS |
| 45 | `rv64m/divu.elf` | exit 0; 59 cycles; PASS |
| 46 | `rv64m/mul.elf` | exit 0; 49 cycles; PASS |
| 47 | `rv64m/mulh.elf` | exit 0; 52 cycles; PASS |
| 48 | `rv64m/mulhsu.elf` | exit 0; 48 cycles; PASS |
| 49 | `rv64m/mulhu.elf` | exit 0; 58 cycles; PASS |
| 50 | `rv64m/rem.elf` | exit 0; 62 cycles; PASS |
| 51 | `rv64m/remu.elf` | exit 0; 54 cycles; PASS |
| 52 | `rv64a/amo_d.elf` | exit 0; 28 cycles; PASS |
| 53 | `rv64a/amo_w.elf` | exit 0; 29 cycles; PASS |
| 54 | `rv64a/aq_rl_set.elf` | exit 0; 23 cycles; PASS |
| 55 | `rv64a/atomic_near_ram_end.elf` | exit 0; 12 cycles; PASS |
| 56 | `rv64a/atomic_tohost_exit.elf` | exit 0; 4 cycles; PASS |
| 57 | `rv64a/lrsc_loop.elf` | exit 0; 16 cycles; PASS |
| 58 | `rv64a/sc_after_store_failure.elf` | exit 0; 20 cycles; PASS |

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

At exact reviewed/verified HEAD
`45a170ff67aef850acc835f5f8084cef186fa3b8`, the required T0–T5 source/test
criteria have the recorded exact-head evidence above and the fresh project
suite is 58/58. The bounded attainment statement supported **for that exact
HEAD only** is exactly the approved §7.3 text:

> **Single-Hart atomic/physical convergence completed for the standard
> facades' native domain: AMO/LR/SC execute as indivisible envelope
> operations with per-Hart reservation state and explicit writer visibility,
> with the legacy typed bridge retired from the standard facades.**

This is not full ADR-0002 conformance, multi-Hart/DMA or RVWMO support, RV64A
certification, a new ACT4 result, a performance result, or successor approval.
This provenance-only follow-up changes the record after HEAD `45a170...`; the
results above are explicitly **stale for the follow-up HEAD**. They are not a
closeout claim at that new revision. The PR requires fresh exact-head evidence
before any attainment statement is carried forward. The record does not
archive/replace `docs/dev-plan.md` or declare the overall workflow finished;
independent review, merge, and any rolling milestone transition remain outside
this coding handoff.
