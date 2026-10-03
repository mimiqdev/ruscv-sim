# A9 bounded technical acceptance and merged delivery assessment

**Assessment dates:** local checkpoint 2026-10-01; final technical acceptance and merge evidence 2026-10-02 (UTC). **Status:** Current bounded evidence navigation — implementation/evidence accepted; detailed A10 successor/rotation approved 2026-10-03 UTC, formal completion/archival effective on actual rotation merge. **Authority:** Informational assessment constrained by [the preserved approved A9 contract](../archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md) and accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).

A9's bounded technical implementation and evidence are accepted as of 2026-10-02: PR #69 has merged and exact merge-head push CI succeeded (§7). Detailed A10 successor and documentation-only archival/rotation were approved 2026-10-03 UTC. Formal rolling milestone completion/archival takes effect on actual rotation merge; until then main retains A9, and on merge [A10](../dev-plan.md) becomes sole Current. The [entire A9 contract](../archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md) is preserved without rewriting its historical wording. Successor implementation is not an A9 acceptance requirement. This assessment neither rotates the plan nor amends an ADR or certifies an ISA/VP. T4 added evidence and navigation only: the cumulative source/test audit found no additional same-scope defect requiring a production change. The [T0–T3 progress record](a9-stage2-progress.md) preserves earlier checkpoints and R1–R4 reproductions; their old checks are not substituted for the fresh execution below.

## 1. Exact identities and evidence levels

| Identity | Meaning |
| --- | --- |
| `b36b4d08e10b6919e096209be4817ab26441d526` | Immutable A9 baseline, merged plan rotation PR #68. |
| `1f723b6e7186a0e3796f8f7d9167d05e3a0fe059` | Clean committed T0–T3/R1–R4 checkpoint audited and freshly executed for this assessment. Branch `feat/a9-foundation`. |
| `7ddd705ea5edb4b3dc855c84d4a726c6d3c5657a` / `0f35391055eadc652833c938aa014617c0c1bf25` | Git trees for `src` / `tests` at that checkpoint. |
| `315aade5ea2235fc71a33d1d5a854232c1dea683` / `61c9f68128e67b902924f83b7950f25426bfb731` | Git trees for `scripts` / `ruscv-macros`. |
| `2a8ee26506188c29c7731125bf579fd39426071d` / `79e557f5205bb9b70b00bbd0ee0bf138866db71e` | `Cargo.toml` / `Cargo.lock` Git blobs; crate version remains `0.1.0`. |
| `ae30762a346f3ff029c74e302d3c8e720b6aa225` | Repository Dockerfile Git blob; no image rebuild or definition change in T4. |

The evidence JSON [a9-t4-guest-evidence.json](a9-t4-guest-evidence.json) records execution timestamps, exact commands, environment/tool identities, source and ELF SHA-256 values, simulator digest, both fresh build manifests, all 58 CLI outcomes, and Rust suite totals. It is an observation record, not an executable test framework. Raw local transcripts are retained under `target/a9-t4-checkpoint-evidence/`; this document and JSON remain usable without those disposable files or a private task system.

The commit introducing this assessment is a later **documentation/evidence-only delivery**, identifiable in Git history. The checkpoint run is not relabeled as a run at that delivery SHA. At the local assessment checkpoint, delivery still required a new clean exact-HEAD full gate and captured Rust validation after commit; equal source trees alone do not prove execution. Those final PR/merge observations are now recorded separately in §7, not substituted into the checkpoint JSON.

Evidence levels below are deliberate: **H** = direct Hart/physical seam, **M** = directly composed Machine owner, **F** = actual standard facade/CLI, **G** = freshly assembled project guests. A passing component does not establish public integration.

## 2. Cumulative acceptance audit

All tests cited here are included in the checkpoint all-feature run in §3 and the separately recorded final-head verification in §7. Anchors identify functions/tests, not moving line numbers. The matrix preserves the original local audit dispositions; §7 establishes bounded final technical acceptance for T0–T4, without expanding any row's evidence level or limitations.

| Contract criterion | Actual implementation and deterministic evidence | Local disposition / boundary |
| --- | --- | --- |
| **T0: frozen routes, observations and omissions** | Progress §§2–5 inventories both loops, fetched opcode, Runner snapshots, missing memory/atomic/CSR/FPR/equal-write log effects, callbacks, image/reset/reuse and host clones at `b36b4d0`. `tests/a9_baseline_characterization.rs` was committed at `1e2733d7789988402d0a63ce92de37b04dc4e042`; its blob is unchanged through the assessed checkpoint. | Evidenced, H/F. Frozen behavior is kept separate from later architecture; current text-log omissions do not mean rich Hart facts are absent. |
| **T0: failure/exit, reset/reload, cross-thread and late effects before migration** | Baseline tests `completed_exit_wins_final_budget_and_step_does_not_poll_or_clear`, `failed_last_slot_does_not_present_pending_exit_or_timeout_and_facade_can_reuse`, `core_only_reset_clears_hart_but_preserves_mutated_ram_and_ports`, clone/reload tests, gated native callbacks and `unresolved_completion_survives_core_reset_while_late_backend_effect_is_not_drained`. | Evidenced, H/F. Core-only reset remains explicitly unsafe as a fresh lifecycle operation. |
| **T1: authoritative always-present control and one completed builder** | `src/core/mod.rs::step_transition` executes the existing staged `execute_turn` once; `src/core/observation.rs::build` runs only after accepted state. `tests/a9_hart_facts.rs::gpr_csr_counter_write_intent_and_branch_identity_are_hart_owned` checks PC/instruction/privilege/attempt/retirement/counter facts. | Evidenced, H; the same engine is wired into F by T3. No new interrupt or time semantics. |
| **T1: GPR/FPR/CSR/counter/ordinary-memory effects** | Executor dispatch supplies actual destination intent; CSR/FPR files journal accepted writes. `ObservedMemory` forwards once and captures accepted response/payload bytes. Hart tests cover equal/suppressed GPR writes, CSR read-only/equal/explicit MINSTRET precedence, FP equal writes/FCSR, B/H/W/D loads/stores and flat address conversion. | Evidenced, H. No Runner snapshot or memory readback constructs these facts; existing FP/profile behavior is observed, not certified. |
| **T1: atomic effects, failed/faulting SC and x0** | `PhysicalAtomicAdapter::access_atomic` captures the accepted envelope; `lr_sc_amo_wd_facts_include_old_new_reservation_and_suppressed_destinations` tests W/D, old/new bytes, aq/rl, reservations and x0. Fault tests and M `failed_and_faulting_sc_keep_the_t1_fact_and_a8_target_boundaries` distinguish retired conditional failure (rd=1 iff non-x0, no write) from target fault (no rd/retirement). | Evidenced, H/M/F via A8 public tests. Faulting SC retains reservation under the inherited A8 profile; typed effects carry `indivisible=false`. |
| **T1: trap identity, original value, saved PC, entry/return** | `enter_trap` stages completed entry; Hart tests `completed_trap_and_mret_include_precise_entry_return_effects`, `fetch_alignment_and_data_fault_traps_have_no_fabricated_destinations` and A6 trap tests check cause, original mtval, privilege, optional fetched identity, saved/vector PC and unchanged MINSTRET. | Evidenced, H/F. Public synchronous-trap continuation is retained; no fabricated asynchronous interrupt. |
| **T1: subscriber gate and off parity** | `step_transition(false)` creates no observation snapshot/journal/record; CSR/FPR observation journals are disabled. Builder spy tests build zero records for eight off turns and failures; allocator spy measures only warmed ADDI staging; `observation_off_has_state_control_physical_parity_and_no_sink_calls` checks state/outcome/port-call parity. | Evidenced, H, with actual native logged/unlogged result parity in baseline tests. Not allocation-free execution, a performance result or a block-engine claim. |
| **T1: immutable, ordered, post-completion, non-reentrant delivery** | Returned observations own values; `HartTransition::deliver` lends them read-only. HTIF builder spy proves callback precedes materialization. Sink-failure tests preserve retirement/trap and prior records after later state changes; M receipts refuse another turn and lifecycle mutation during gated delivery. | Evidenced, H/M/F. Synchronous receipts, not an asynchronous observer queue. |
| **T2: one Hart association and one active Platform storage/atomic domain** | `Installed::build` composes the existing native SystemBus/RAM/UART/HTIF or flat RAM, wiring typed/fetch/data/atomic views to the same storage. Shared `Machine` controls and exclusive `OwnedMachine` reuse this construction and `Installed::turn`. | Evidenced, M/F; source-route audit and ordinary/atomic public oracles. Immutable image restoration bytes are not a second running RAM. |
| **T2: metadata separate from placement/inspection** | `LoadImage::parse` supplies ordered file bytes, zero tails, entry/signature/tohost and flags/p_paddr metadata; `Platform::place` installs them. Lifecycle tests compare all RAM to the legacy loader, pure-zero overlap and BSS; inspection retains identity/metadata. | Evidenced, M/F. p_vaddr placement and flat base subtraction are storage adaptation, not MMU/permission enforcement. |
| **T2: coordinated fresh state** | `Machine::fresh_reset` / exclusive install rebuild a fresh generation from the same immutable image/config. `metadata_placement_fresh_reset_zero_fill_and_rerun_match_native_and_flat`, UART/HTIF fresh-state tests and public fresh-rerun test check bytes/zero fill, PC, privilege, GPR/FPR/FCSR/CSR/counters/reservation, devices/events and configured callbacks. | Evidenced, M/F. Host output already emitted is not rolled back. Facade budgets/verbosity/manual signal survive. |
| **T2: mutation only after requested/proven drain** | `Admission::{mutation_allowed,drain_allowed}` tracks Hart, host and boundary work independently of target locks. `retained_boundary_no_progress_or_run_return_never_acknowledges_lifecycle_drain` rejects implicit drain; gated sink and callback tests block install/reset/edit/teardown before and after Hart completion until receipt consumption. | Evidenced, M/F. A9 does not implement NoProgress scheduling; an outer idle claim is not a drain proof. |
| **T2: every admitted host path stays real and fences replacement** | `HostMemory` admits typed B/H/W/D, accepted-prefix `write_mem` and `clear_tohost` into the same bytes/version domain. Channel-gated M writer tests exercise native/flat × three paths × reset/reload; public clone tests cover admitted-turn overlap/disjoint SC and writer drain before reset/reload/edit. | Evidenced, M/F. Quiesce explicitly refuses new live writers; started writers finish. A mutable Runner borrow alone is not serialization. |
| **T2: stale generations, transactional replacement and state edits** | Generation publication detaches old RAM; replacement and rejected-placement tests preserve identity/PC/config on error and isolate stale writers. M owned edits invalidate opaque reservations; F borrowed edits retain only the legitimate current token and reject imported old-generation context. | Evidenced, M/F. Detached old handles remain writable; raw handles are volatile, not coherent snapshots. |
| **T2: unknown completion, failed drain, no retry/late recovery** | Admission retains original failure/uncertainty; one injected request later performs a real RAM write or native HTIF callback through the original port. M `unknown_delayed_ram_write_or_native_callback_never_permits_new_generation_or_retry` and F unknown tests reject repeat turns/edit/install/reset/drain/teardown before and after the late effect. Callback unwind also fails closed. | Evidenced negative behavior, M/F internal seams. There is **no adapter resolution API**; joining a test thread does not authorize recovery. |
| **T2: owner lifetime and teardown safety** | Leases retain the entire owner; `boundary_receipts_and_admitted_host_work_keep_the_owner_alive` verifies last-control drop during work. Uncertain/poisoned last-owner cleanup quarantines rather than disconnects the domain; normal drained teardown and detached RAM are tested. | Evidenced, M/F. Uncertain resources intentionally remain until process exit, not safe recovery or proven prior-state correctness. |
| **T3: both standard facades use one semantic path** | Native Runner and flat `run`/`step` call `OwnedMachine::step_request` → `Installed::turn` → `RiscvCore::step_transition`. `source_audit_standard_facades_have_no_core_composition_invocation_or_effect_reconstruction` rejects facade core construction/invocation, decoder, snapshots and legacy installer. Same-ELF mixed operations and A7/A8 oracles test behavior, not just strings. | Evidenced, F/G. Separate configuration/policy loops remain Runner orchestration, not ISA engines. |
| **T3: non-lossy exit/failure/final-budget/reporting/lifecycle causality** | `MachineTurn` retains Hart/raw Platform/signal and supported boundary facts. Actual native Runner tests retain completed store exit + budget + reporting error, trap + error + budget, and quiesce during delivery. Unknown failures have no fabricated exit/observation/final-slot budget. A4 lazy HTIF priority and A6 started/completed accounting pass. | Evidenced, F. Runner selects presentation; cycles count completed turns including traps, not MINSTRET/mcycle/time. Zero budget starts no turn. |
| **T3/R1: actual CLI error precedence and stable logs** | `CommitLogger::log_record` formats Hart facts, keeping prior equal-write/memory/CSR/FPR/trap text omissions. Self-modifying-store and fetch-call spies prove no refetch. `a9_cli_reporting` runs child-only file-limit fault injection: error + guest 0 retains PC/count/code but prints FAILED/status 1; healthy sink prints SUCCESS/status 0. | Evidenced, F. CLI process-fault injection is Unix-only; requested reporting failure cannot unretire. |
| **T3/R2/R4: signatures and coherent flat clone serialization** | M signature regression covers absent, unmapped empty, readable and unreadable regions on both variants before/after reset. F zero-budget multipart test waits for the public guard and returns final bytes, acquiring capability before admission; absent/empty tests bypass mutex/target; native test retains raw-Platform behavior. A7/A8/public artifact policy tests pass. | Evidenced, M/F. No guest load or retirement for artifacts. Native suppresses unreadable artifacts; flat reports error while preserving primary result. |
| **T3/R3: signal sampling/clearing** | `OwnedMachine::signal_memory`, `observe_tohost`, `clear_signal` serialize flat poll/byte-clear with the public clone mutex, then clear owned RAM under the receipt without new host admission. Actual Runner channel tests hold multipart signal guards, decode final 0x107 as code 131, retain decoded code during clear, and finish clearing after quiesce. | Evidenced, F. Poll and clear remain separate guards, not an atomic read-and-clear guarantee; native callback/HTIF priority is unchanged. |
| **T3: public API, budgets, resume/reload, native/flat differences and helper exceptions** | Borrowed API compile/runtime test retains `&CoreState`, `&mut CoreState`, `&Arc<Mutex<dyn MemoryInterface>>`. A4/A6/A7/A8/public behavior tests cover default/configured/zero/exact limits, guest errors, native-only devices, tohost precedence, reload/artifacts and post-exit/budget resume. Unsupported-legal host patch permits only a new facade request. | Evidenced, F/G. Core-only `reset_core`/`step_once`/`get_core_state` and typed `RiscvCore::new` remain labeled unmanaged compatibility helpers, not safe Machine lifecycle. `state_mut` must panic when its legacy infallible API cannot prove drain. |
| **T4: fresh gate, identity, per-case evidence and independent acceptance** | §3 records all nine required gate commands and the committed-range diff check. §4 and the JSON record 58 fresh CLI guests, matching rebuild hashes and tool/source/image identities. §5 records exact-head checkpoint reviews; §6 states debt/non-claims. | Local execution/audit evidenced, F/G. Final exact-PR-head independent review and applicable CI remain pending; this criterion is not declared fully accepted. |

## 3. Fresh local execution — not carried-forward checks

The host had no `riscv64-unknown-elf-as` available. Verification used the available immutable repository development image, matching [the environment instructions](../development-environment.md):

```text
ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c
```

Observed local image ID is the same SHA-256; platform `linux/arm64`, Rust host `aarch64-unknown-linux-gnu`. `rustc 1.97.1 (8bab26f4f 2026-07-14)` (full compiler commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6); Cargo `1.97.1 (c980f4866 2026-06-30)`; Clippy `0.1.97`; rustfmt `1.9.0-stable`. Assembler/linker/readelf/nm are `/usr/bin/riscv64-unknown-elf-*`, GNU binutils `2.40-2+4+b1`; GCC `12.2.0-14+deb12u1+11+b2`. The JSON retains executable hashes. Spike 1.1.0 is installed but **was not used for a reference/ACT4 run**.

Exact outer execution shape (the payload ran the commands below sequentially, logging start/end/exit status separately):

```bash
docker run --rm --init --volume "$PWD:/workspace" --workdir /workspace \
  --env RISCV_SOURCE_HEAD=1f723b6e7186a0e3796f8f7d9167d05e3a0fe059 \
  --env A9_EVIDENCE_DIR=target/a9-t4-checkpoint-evidence \
  ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c \
  bash -c 'bash target/a9-t4-gate.sh'
```

The disposable payload exported `CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR=target/a9-container-cargo`, `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`, `RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves`, and `CARGO_TERM_COLOR=never`; unset `RISCV_TEST_SKIP_BUILD` and `RUSCV_SIM_BIN`. This is non-login `bash -c`, not PATH-resetting login shell. Container Cargo output never mixes with host Cargo output. `RISCV_SOURCE_HEAD` explicitly binds the mounted source because a linked worktree's host Git pointer is not resolved inside `/workspace`.

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration -- --nocapture
RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/run_elf_tests.sh
# Host-side committed range, separate from the container:
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
```

| Check | Fresh observation at `1f723b6` |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0 |
| `cargo check --all-features` | Exit 0 |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0, warnings denied |
| `cargo test --all-features` | Exit 0; 868 library + 940 integration + 27 doctests = 1,835 passed; 0 failed/ignored |
| `cargo doc --all-features --no-deps` | Exit 0 |
| `bash scripts/test_riscv_elf_guards.sh` | Exit 0; 11 path cases plus final summary |
| `RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration -- --nocapture` | Exit 0; 5 newly assembled trap guests × CLI/native/flat, one integration test |
| `RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/compile_riscv_tests.sh` | Exit 0; 58 compiled, 0 failed |
| `RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/run_elf_tests.sh` | Exit 0; own rebuild 58/58, public CLI 58 passed, 0 failed/skipped |
| `git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD` | Host exit 0 at the same committed checkpoint |

Run interval: **2026-10-01T04:13:31Z–04:18:38Z**. The JSON records each command’s individual interval and log hash. A9 focused suites included T0 10, Hart facts 13, Machine lifecycle 14, facade composition 6, CLI reporting 2; the 13 facade, 4 lifecycle, 2 builder/boundary and 3 reporting unit tests also ran within the full library suite.

The A6 required-toolchain command builds all five trap ELFs in a new temporary directory and checks each through CLI process, native `load_and_run`, and flat `RiscVSimulator`; `--nocapture` records all five PASS lines. Toolchain availability is mandatory, including in the all-feature suite; this is not a status-77/optional skip presented as a pass.

## 4. All 58 fresh project guests and artifact identity

Compile and run scripts were read before invocation. Each compilation clears only its validated named disposable output under `target/`, assembles **current** `.S` files using `-march=rv64ima_zicsr -mabi=lp64`, and links with `tests/bare-metal-riscv-test/linker.ld`. The explicit compile and the run script's **own second rebuild** both produced 58/58 with zero failures. No `RISCV_TEST_SKIP_BUILD=1`, caller binary override, old ELF or source-tree artifact was used. All 58 ELF SHA-256 values matched between those two fresh builds.

The run script rebuilt the release simulator with `cargo build --release --all-features` in the separate container target. For each ELF it checked real entry against `_start` with cross readelf/nm, then invoked `target/a9-container-cargo/release/ruscv-sim run <fresh-elf> --max-cycles 100000`. Every process returned 0 and reported guest code 0/SUCCESS; `hello` additionally emitted `Hello!`. Linker RWX-segment warnings were nonfatal, not missing guest evidence.

The companion JSON stores every source hash, rebuilt ELF hash, entry/_start, exact CLI argv and output, guest code, cycles, final PC, process status and group; hashes identify artifacts but do not themselves prove a pass. Compact per-case observations from this run:

| # | Fresh guest | Cycles | Final PC | CLI / guest code |
| ---: | --- | ---: | --- | --- |
| 1 | `rv64i/add.elf` | 53 | `0x0000000080000048` | 0 / 0 |
| 2 | `rv64i/addi.elf` | 32 | `0x0000000080000084` | 0 / 0 |
| 3 | `rv64i/and.elf` | 45 | `0x00000000800000b8` | 0 / 0 |
| 4 | `rv64i/andi.elf` | 44 | `0x00000000800000b4` | 0 / 0 |
| 5 | `rv64i/beq.elf` | 23 | `0x0000000080000070` | 0 / 0 |
| 6 | `rv64i/bge.elf` | 29 | `0x000000008000008c` | 0 / 0 |
| 7 | `rv64i/bgeu.elf` | 34 | `0x00000000800000a8` | 0 / 0 |
| 8 | `rv64i/blt.elf` | 29 | `0x0000000080000088` | 0 / 0 |
| 9 | `rv64i/bltu.elf` | 29 | `0x0000000080000088` | 0 / 0 |
| 10 | `rv64i/bne.elf` | 26 | `0x000000008000007c` | 0 / 0 |
| 11 | `rv64i/csrrc.elf` | 48 | `0x00000000800000c8` | 0 / 0 |
| 12 | `rv64i/csrrci.elf` | 46 | `0x00000000800000c0` | 0 / 0 |
| 13 | `rv64i/csrrs.elf` | 48 | `0x00000000800000c8` | 0 / 0 |
| 14 | `rv64i/csrrsi.elf` | 39 | `0x00000000800000a4` | 0 / 0 |
| 15 | `rv64i/csrrw.elf` | 44 | `0x00000000800000b8` | 0 / 0 |
| 16 | `rv64i/csrrwi.elf` | 28 | `0x0000000080000078` | 0 / 0 |
| 17 | `rv64i/fib.elf` | 68 | `0x0000000080000054` | 0 / 0 |
| 18 | `rv64i/hello.elf` | 58 | `0x0000000080000088` | 0 / 0 |
| 19 | `rv64i/jal.elf` | 28 | `0x0000000080000074` | 0 / 0 |
| 20 | `rv64i/jalr.elf` | 34 | `0x00000000800000a0` | 0 / 0 |
| 21 | `rv64i/lb.elf` | 63 | `0x0000000080000100` | 0 / 0 |
| 22 | `rv64i/lh.elf` | 57 | `0x00000000800000e8` | 0 / 0 |
| 23 | `rv64i/lw.elf` | 49 | `0x00000000800000cc` | 0 / 0 |
| 24 | `rv64i/lwu.elf` | 87 | `0x0000000080000164` | 0 / 0 |
| 25 | `rv64i/or.elf` | 45 | `0x00000000800000b8` | 0 / 0 |
| 26 | `rv64i/ori.elf` | 47 | `0x00000000800000c0` | 0 / 0 |
| 27 | `rv64i/sb.elf` | 127 | `0x0000000080000200` | 0 / 0 |
| 28 | `rv64i/sh.elf` | 105 | `0x00000000800001a8` | 0 / 0 |
| 29 | `rv64i/sll.elf` | 36 | `0x0000000080000094` | 0 / 0 |
| 30 | `rv64i/slli.elf` | 36 | `0x0000000080000094` | 0 / 0 |
| 31 | `rv64i/sra.elf` | 40 | `0x00000000800000a4` | 0 / 0 |
| 32 | `rv64i/srai.elf` | 34 | `0x000000008000008c` | 0 / 0 |
| 33 | `rv64i/srl.elf` | 36 | `0x0000000080000094` | 0 / 0 |
| 34 | `rv64i/srli.elf` | 36 | `0x0000000080000094` | 0 / 0 |
| 35 | `rv64i/sub.elf` | 28 | `0x0000000080000074` | 0 / 0 |
| 36 | `rv64i/sw.elf` | 103 | `0x00000000800001a0` | 0 / 0 |
| 37 | `rv64i/trap_ebreak.elf` | 41 | `0x00000000800000a8` | 0 / 0 |
| 38 | `rv64i/trap_ecall.elf` | 51 | `0x00000000800000d4` | 0 / 0 |
| 39 | `rv64i/trap_illegal.elf` | 44 | `0x00000000800000b4` | 0 / 0 |
| 40 | `rv64i/trap_mret_priv.elf` | 65 | `0x0000000080000108` | 0 / 0 |
| 41 | `rv64i/trap_vectored.elf` | 41 | `0x00000000800000d4` | 0 / 0 |
| 42 | `rv64i/xor.elf` | 44 | `0x00000000800000b4` | 0 / 0 |
| 43 | `rv64i/xori.elf` | 48 | `0x00000000800000c4` | 0 / 0 |
| 44 | `rv64m/div.elf` | 71 | `0x0000000080000124` | 0 / 0 |
| 45 | `rv64m/divu.elf` | 59 | `0x00000000800000f4` | 0 / 0 |
| 46 | `rv64m/mul.elf` | 49 | `0x00000000800000cc` | 0 / 0 |
| 47 | `rv64m/mulh.elf` | 52 | `0x00000000800000d8` | 0 / 0 |
| 48 | `rv64m/mulhsu.elf` | 48 | `0x00000000800000c8` | 0 / 0 |
| 49 | `rv64m/mulhu.elf` | 58 | `0x00000000800000f0` | 0 / 0 |
| 50 | `rv64m/rem.elf` | 62 | `0x0000000080000100` | 0 / 0 |
| 51 | `rv64m/remu.elf` | 54 | `0x00000000800000e0` | 0 / 0 |
| 52 | `rv64a/amo_d.elf` | 28 | `0x0000000080000074` | 0 / 0 |
| 53 | `rv64a/amo_w.elf` | 29 | `0x0000000080000078` | 0 / 0 |
| 54 | `rv64a/aq_rl_set.elf` | 23 | `0x0000000080000060` | 0 / 0 |
| 55 | `rv64a/atomic_near_ram_end.elf` | 12 | `0x0000000080000030` | 0 / 0 |
| 56 | `rv64a/atomic_tohost_exit.elf` | 4 | `0x0000000080000010` | 0 / 0 |
| 57 | `rv64a/lrsc_loop.elf` | 16 | `0x0000000080000044` | 0 / 0 |
| 58 | `rv64a/sc_after_store_failure.elf` | 20 | `0x0000000080000054` | 0 / 0 |

This is **43 RV64I + 8 RV64M + 7 RV64A project-authored cases**, including the five A6 trap programs. The seven A8 guests exercise W/D old-value arithmetic, accepted aq/rl encodings, final-RAM-dword LR/SC, atomic tohost retirement, LR/SC loop success and overlap-store SC failure. Their single-Hart outcomes do not prove RVWMO or full RV64A/Zalrsc/Zamo conformance. The 58 CLI runs are not 58 flat-facade cross-toolchain runs: A6's five required guests and Rust public mixed-operation tests supply the separately labeled flat evidence.

## 5. Independent review history — head-bound, not PR acceptance

Read-only independent checkpoint reviews inspected the cumulative range from `b36b4d0` to each exact head below. These are recorded review verdicts, **not reviewer-executed checks**; reviewers ran no builds/tests. Review prose/check hints are not used as executable evidence. The actual fresh execution in §3 stands separately.

| Reviewed exact HEAD | Bounded result |
| --- | --- |
| `2a14ea154fa991d44a2bdb68a1d845c56fe93787` | T0: no actionable findings. |
| `051ba1ac7e4366946d85c048d9d36f8e850c122a` | T1: R1, source defect despite passing checks; zero guest exit masked CLI reporting failure. |
| `1a45cd257f751adbfa1d86ba480176170a810508` | T0/T1 + R1: no actionable findings. |
| `314d9d7162af3e508b2f35527276bc08270178d6` | T2: R2, empty signature resolved before empty check. |
| `ae1a5397feadc35945df261b81ac469bf433c34c` | T0–T2 + R2: no actionable findings. |
| `62e4ad31e620f048568139242ac4710d985fcf93` | T3: R3, flat signal poll/clear bypassed public clone guard. |
| `903e84006aa6f454f028080faea5393c7323174c` | R3 resolved; R4 identified analogous nonempty artifact bypass. |
| `1f723b6e7186a0e3796f8f7d9167d05e3a0fe059` | T0–T3 + R1–R4: no actionable findings; R4 guard-before-admission repair and actual Runner regressions inspected. |

The review history and reproductions are summarized here so the assessment does not depend on access to private execution stores. No earlier verdict approves a later T4 evidence/documentation commit. At this 2026-10-01 checkpoint, final exact-PR-head independent review and applicable PR CI were pending. §7 supersedes that delivery status only: it records the actual final PR head, review, CI and merge, not a retroactive change to these checkpoint verdicts.

## 6. Residual debt and explicit non-claims

- **Unknown completion:** no supported adapter termination/state-resolution proof API. Ordinary retry, fresh reset, reload, mutation and safe teardown stay refused even after a test-confirmed late effect. Failed/poisoned last-owner resources intentionally remain until process exit; no prior uncertain state is retroactively validated.
- **Host concurrency:** live writes retain true version-domain effects; raw handles/reads remain volatile and host arrival may be nondeterministic. Guard serialization of flat fetch/data/signal/nonempty artifact paths is not deterministic replay, an atomic poll-and-clear operation, or multi-master coherence. Direct-owner coherent inspection is distinct from raw-handle reads.
- **Compatibility exceptions:** native devices vs flat storage, native unreadable-artifact suppression vs flat reporting, continued synchronous traps, old typed/core-only helpers, infallible borrowed edit panic, and unsupported-legal correction on a new facade request remain explicit. Typed-only constructors do not gain physical atomic certification.
- **ISA and architecture:** fixed 32-bit instruction fetch/identity, inherited FPU/CSR/atomic profile limitations and existing semantics remain. No full ADR/ISA certification, board/topology, MMU/Sv39/PMP or page-walk/A-D integration, interrupt/WFI/time/deadline scheduler, debugger/Debug Mode/GDB wiring, TLM/SystemC/DMI/FFI, multi-Hart/DMA/RVWMO, block/JIT, checkpoint or Stage 3 performance facility is delivered.
- **External evidence:** the frozen ACT4 51-case nontrapping RV64I selection remains historical at its own heads and artifact identities in [A8 assessment §4.2](a8-closeout-assessment.md#42-frozen-act4-selection--a-distinct-51-case-run) and [A7 replay](a7-act4-replay-35432032788.json). T4 does not rerun/retrieve/replay it, add an atomic selection, repair a historical artifact retrieval limitation, or combine its denominator with the 58 project guests.
- **Acceptance boundary:** local checkpoint gates and reviews are not final-head execution. §7 supplies separate final PR-head acceptance and merged delivery evidence. A9 technical implementation/evidence is accepted; detailed successor/rotation approval is recorded 2026-10-03 UTC, with formal completion/archival effective on actual rotation merge (main retains A9 until then). No later documentation-only SHA is relabeled as tested runtime.


## 7. Final PR-head acceptance and exact merge-head evidence

**Verified 2026-10-02 through GitHub CLI/API and the full run/job logs**, separately
from the local checkpoint above. Durable sources are the PR, review comment,
workflow jobs and their command logs, not disposable transcripts or private
execution references.

| Exact identity | Durable evidence and disposition |
| --- | --- |
| Activation `b36b4d08e10b6919e096209be4817ab26441d526` | [PR #68](https://github.com/mimiqdev/ruscv-sim/pull/68) activated A9 on 2026-09-29 UTC, following approval that day. A8's contract was archived; the rotation itself implemented no Rust. |
| Final PR head `d0bff0847795057a3141f1ac0562512131f623ad` | [PR #69](https://github.com/mimiqdev/ruscv-sim/pull/69), cumulative range from `b36b4d0`. The [durable final review/evidence comment](https://github.com/mimiqdev/ruscv-sim/pull/69#issuecomment-5926146762), posted 2026-10-01T06:41:33Z, records all six Rust requirements and a separate exact-head pinned-container nine-command gate/evidence audit passing: 1,835 Rust/doc results, 58/58 fresh CLI guests, five required-toolchain A6 guests on CLI/native/flat and output-path guards. This is a final-head execution attestation, not reassignment of the checkpoint JSON's hashes/timestamps. |
| Independent final PR-head review | Same comment: separate read-only reviewer, `no_actionable_findings`, 0 findings. Static source/assertion review; reviewer executed no tests/builds/edits. T0–T4 and R1–R4 accepted only within the matrix and residual limitations above. |
| PR CI `36819008505` associated with head `d0bff084…` | [Run](https://github.com/mimiqdev/ruscv-sim/actions/runs/36819008505), [Quality and tests job 110230290057](https://github.com/mimiqdev/ruscv-sim/actions/runs/36819008505/job/110230290057): success, completed 2026-10-01T05:24:01Z. Run metadata names PR head `d0bff084…`; checkout log names synthetic PR merge `909267461dacfa54866a0c90c62d72bbc371fa90` (merge of that head into `b36b4d0`). Logs independently total 1,835 passed, 0 failed/ignored at that checkout, not a direct-head runtime run. Release/smoke and standalone 58-guest compile/run steps were skipped on PR policy; those passes come from the separate container gate, not PR CI. Coverage skipped, not passed. |
| Merge `0898f1215c4508e98d4db958080f6dd01b5745bf` | PR API records authorized merge at **2026-10-02T14:49:55Z**. Git tree `c583a213aebfc389fa65b54c8d81571a35fa5afa` equals the reviewed PR-head tree. This is content equality only, not execution. Crate remains `0.1.0`; no release/tag implied. |
| Main push CI `37022637776` at exact `0898f121…` | [Run](https://github.com/mimiqdev/ruscv-sim/actions/runs/37022637776), [Quality and tests job 110889281263](https://github.com/mimiqdev/ruscv-sim/actions/runs/37022637776/job/110889281263): **success**, job 2026-10-02T14:50:02Z–14:56:28Z, run completed 14:56:29Z. Checkout log names exact merge SHA. Rust/doc logs total 1,835 passed, 0 failed/ignored. Release build, smoke, fresh assembly/link and all 58 actual CLI executions passed. Coverage skipped, never coverage evidence. |

The six final PR Rust requirements are formatting, all-feature check, strict
all-target clippy, all-feature tests, no-dependency docs, and committed-range
`git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD`.
The separately attested nine container commands are exactly the nine command
rows in §3 (A6 with `-- --nocapture`); image pinned to
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`.
The final-head aggregate attestation does not provide a new per-case artifact
manifest in this repository; §4/JSON remain the original checkpoint identities.

### Merge-head command and guest audit

The actual [CI definition](../../.github/workflows/ci.yml) uses
`CARGO_BUILD_JOBS=2`, `CARGO_TARGET_DIR=target/ci-cargo`,
`RISCV_REQUIRE_RISCV_TOOLCHAIN=1`, `RISCV_TEST_OUTDIR=target/ci-riscv-elves`.
The merge job logs show stable `x86_64-unknown-linux-gnu` Rust 1.99.0
(`b940084d7`, 2026-09-28), unlike the pinned ARM64 local container. These are
separate executions/environments, not a performance comparison. Commands:

```bash
cargo fmt --all -- --check
python3 -m unittest discover -s scripts/a5 -p 'test_*.py'
bash -n scripts/a5/experiment.sh
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash -n scripts/riscv_elf_paths.sh scripts/compile_riscv_tests.sh scripts/run_elf_tests.sh scripts/test_riscv_elf_guards.sh
bash scripts/test_riscv_elf_guards.sh
cargo build --release --all-features
./target/ci-cargo/release/ruscv-sim --version
./scripts/compile_riscv_tests.sh
RISCV_TEST_SKIP_BUILD=1 ./scripts/run_elf_tests.sh
```

Here **skip-build is not stale guest reuse**: the immediately preceding compile
step freshly assembled/linked 58 sources at the merge SHA. Its summary and
consumed manifest both name `source_head=0898f1215c4508e98d4db958080f6dd01b5745bf`,
`source_count=58`, `compiled_count=58`, failed 0. The run script requires and prints that
manifest and rebuilds/uses the release simulator; the preceding compile step and
log audit establish its freshness/head identity (the script itself does not validate
all manifest identities). It skips only a redundant
second guest compilation in this CI job. This differs explicitly from the
local container's two fresh rebuilds. Each actual CLI invocation uses
`target/ci-cargo/release/ruscv-sim run <target/ci-riscv-elves/...elf> --max-cycles 100000`.
The full log contains unique cases 1–58, 58 `CLI exit status 0` observations,
all guest code 0/SUCCESS with entry/PC/count output, Hello! output, and
`Total: 58 / Passed: 58 / Failed: 0 / Cases: 58` plus the final all-fresh-cases
PASS summary. The inventory remains 43 RV64I + 8 RV64M + 7 RV64A; it is not
58 flat runs or external certification. Required A6 CLI/native/flat evidence
remains separately identified in the final container gate.

Reinspection commands (read-only; not runtime re-execution):

```bash
gh pr view 69 --repo mimiqdev/ruscv-sim --json url,state,headRefOid,mergeCommit,mergedAt
gh api repos/mimiqdev/ruscv-sim/issues/comments/5926146762
gh run view 36819008505 --repo mimiqdev/ruscv-sim --json headSha,event,status,conclusion,jobs
gh run view 36819008505 --repo mimiqdev/ruscv-sim --log
gh run view 37022637776 --repo mimiqdev/ruscv-sim --json headSha,event,status,conclusion,jobs
gh run view 37022637776 --repo mimiqdev/ruscv-sim --log
git rev-parse 0898f1215c4508e98d4db958080f6dd01b5745bf^{tree} d0bff0847795057a3141f1ac0562512131f623ad^{tree}
```

**Disposition:** T0 characterization, T1 Hart facts, T2 safe N=1 lifecycle,
T3 public-facade migration and T4 final verification/review meet the bounded
technical acceptance contract. Implementation delivery is merged with exact
merge-head CI evidence. The [standalone closeout record](../archive/milestones/a9-closeout-record.md)
records this boundary and PR #70 document delivery: exact head `3adf1c8338716ef307401fd653fcc6d5dfbbdb77`, independent static review no_actionable_findings/0, four document checks and 352-link audit; PR CI `37152906932` / job `111290267911` succeeded at synthetic checkout `479b34351b7de8ffc582c2be890f41d8f03ec627` into `0898f121…`, not direct-head execution. Coverage and standalone release/ELF steps skipped. PR #70 merged 2026-10-03T20:58:10Z at `32bb28e84fd433446caa93f870d921a86b3ddab3`, tree equal to reviewed document head (not execution). Docs-only push paths-ignore means no merge-head main CI is scheduled.

Detailed successor and rotation approved 2026-10-03 UTC; formal A9 completion/archival is effective on actual rotation merge. The [approved Stage 3 snapshot](../proposals/a10-performance-test-infrastructure.md) is informational, not a second Current contract or additional A9 criterion. [A10](../dev-plan.md) activates on that merge; implementation is subsequent scoped work. No runtime, ACT4 or performance experiment was run by this documentation consolidation.
