# A8 bounded closeout assessment and cumulative acceptance audit

**Assessment date:** 2026-09-24 (UTC)

**Status:** Evidence assessment for the current A8 contract; not a successor contract or rolling-milestone transition.

**Authority:** `docs/dev-plan.md` remains the sole current milestone contract. This record does not amend ADR-0001–0004, archive or replace the plan, declare full ADR conformance, or certify an ISA extension.

## 1. Evidence identity and scope

This audit is anchored to the exact merged implementation commit, not to a moving branch name:

| Identity | Full commit / tree | Meaning |
| --- | --- | --- |
| PR #65 review/CI candidate | `dca3698ee105db5ad5dfaa0a704b6d4730ab8ab2`; tree `698ea3ba5a86e7fce9481d2f12373edbbc7ad34b` | PR #65 head and the source identity used by its PR CI and the separately dispatched A5/ACT4 workflow. |
| Merged implementation HEAD | `4c4d0a295f620ed85557b10475d46bef1be5ea17`; tree `698ea3ba5a86e7fce9481d2f12373edbbc7ad34b` | PR #65 squash merge to `main`; the exact source identity for the main-push CI run below. |
| Main before PR #65 | `391f2688a2abc4a2667e8d9ec6e9f0b25e2f1221` | Base of PR #65 and comparison identity for the recorded A8 diff check. |

The GitHub PR object reports PR #65 merged at `2026-09-24T06:16:04Z`, with head `dca3698...` and merge commit `4c4d0a...` ([PR #65](https://github.com/mimiqdev/ruscv-sim/pull/65)). Equal Git trees establish content identity only: the PR-head CI is not relabeled as a run at the merge SHA, and the merge-head CI is not relabeled as a run at the PR SHA. Run identities remain separate below.

The evidence in the earlier [A8 T5 record](a8-t5-evidence.md) is bound to `45a170ff67aef850acc835f5f8084cef186fa3b8`; that record explicitly marks its checks stale for later revisions. Its old-head command results and §7.3 claim are **not** used as current evidence here. This audit instead cites the PR-head checks and the exact merge-head CI, and inspects the final source/test tree. The pre-correction observations in the [A8 T4 route audit](a8-public-atomic.md) are used only where its SC-correction addendum agrees with the current plan and final implementation.

### Evidence collection limits

The current CodingTask context reports that qing check references `check:a8-dca-full-gate` and `check:a8-dca-diff` passed at PR head `dca3698...`, with the latter compared against `391f268...`. It also reports the full-gate container image as `ghcr.io/mimiqdev/ruscv-sim-dev:main`, digest `sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`. The former task's qing evidence record was not available to this closeout worktree, and qing does not persist ad-hoc argv. Accordingly, those check IDs are provenance references only: this document does not invent their command lines, and the durable exact command/results counterpart is the published GitHub CI evidence below. The image digest and its local tool versions were independently inspected; they are not a substitute for a qing record.

The GitHub PR #65 reviews and review-comments APIs returned no review objects during this collection. This document therefore records `dca3698...` as the PR review/CI candidate identity supplied for this assessment, but does not quote a reviewer verdict, reviewer identity, or review finding count. This does not change the independently inspectable commit, tree, CI, or merge identities.

## 2. Contract and cross-ADR boundary

The normative source is [`docs/dev-plan.md`](../dev-plan.md), especially §§2, 5–8, 10–11. The following accepted decisions remain authoritative and unchanged:

- **ADR-0001** — Hart-owned instruction outcome, retirement/trap distinction, original address context, and simulator-failure boundary. A8’s atomic tests exercise the bounded atomic Hart path; they do not establish ADR-0001’s entire observation or interrupt contract.
- **ADR-0002** — raw physical transactions, target-vs-host/protocol/unknown taxonomy, atomic operation envelope, no partial failed effects, Hart-owned arithmetic/reservation/result, and committed-writer visibility. A8 establishes the approved atomic slice only for the native single-Hart storage domain of the two standard facades. It does not route MMU page walks/A-D writes or certify other adapters.
- **ADR-0003** — Runner/Machine/Platform ownership and lifecycle. A8 adds no Machine composition/lifecycle and claims none.
- **ADR-0004** — time, interrupt admission, scheduler and stop boundaries. A8 adds no scheduler, interrupt, WFI, or time behavior.

The approved A8 profile remains the one in §11: encoding repair; faulting-SC retain; P2a-precise committed-write visibility; HTIF D-c (LR/SC rejected, AMO allowed); removal of the global reservation/free clear API; retention of the typed constructor as an explicitly non-conforming compatibility adapter; project atomic guests; and the maintainer-authorized SC target-check order. These choices do not amend an ADR.

### SC contract/evidence reconciliation

The amended current plan (§2, §§5.4, 5.6, C13/C21, §8 T0/T2/T3/T4, §11 item 8) selects: Hart legality/alignment, guest-to-port conversion, one StoreConditional envelope with optional reservation context, target span/capability validation, then conditional failure on a valid RAM target if the context is absent or uncovered. The target order is a selected implementation policy, **not** a claim that Zalrsc prescribes that backend sequence, and it is not MMU/PMP permission enforcement.

The final source implements this order: `src/isa/rv64a/dispatch.rs::execute_amo_port` converts the SC address and always submits the SC envelope; `src/physical.rs::native_ram_atomic_transact` validates the configured and actual backing-store span under the memory lock before it can return conditional failure; the native bus routes HTIF LR/SC to D-c target rejection. `tests/a8_atomic_targets.rs` checks no-context/uncovered RAM failure versus invalid backing spans and HTIF policy. `tests/a8_hart_atomic.rs` checks one request, no `rd`/retirement on target failure, and faulting-SC reservation retention. `tests/a8_public_atomic_equivalence.rs::native_public_facade_sc_fault_matrix_checks_mtval_rd_ram_uart_htif_and_exit` checks public guest-visible cause, original `mtval`, unchanged destination/RAM, retry, and no premature exit. Thus earlier no-request/no-reservation observations remain historical; they are not the current contract or final expected behavior.

## 3. T0–T5 cumulative acceptance matrix

“Pass” below means the bounded T-row deliverable is evidenced at the implementation source identity above and covered by the cited current full-suite/CI evidence. It does not mean that components absent from the public path are integrated. No focused command result from the stale `45a170...` T5 record is carried forward as a current-head run.

| Row | Contract criterion | Inspected implementation and tests | Disposition at the exact implementation identity |
| --- | --- | --- | --- |
| **T0 — fixture ledger and executable baseline** | Every A7 atomic behavior has a keep/flip/retire/relabel disposition; the old behavior was characterized before changes; approved C13/C21 SC correction replaces the original short-circuit expectation. | [`a8-fixture-reclassification.md`](a8-fixture-reclassification.md) records A7 rows and the spec-first correction. `tests/a8_atomic_baseline.rs`, `tests/a7_migration_characterization.rs`, `tests/a7_legacy_atomic_compat.rs`, and `tests/amo_test.rs` preserve/replace the relevant assertions; current cases are in `tests/a8_hart_atomic.rs` and `tests/a8_public_atomic_equivalence.rs`. | **PASS (bounded).** Ledger plus current full test suite; the original T0 baseline is historical characterization, not a claim that pre-A8 expectations remain current. |
| **T1 — envelope and Hart arithmetic** | Separate AMO/LR/SC envelope vocabulary, exact request/response binding and taxonomy; one Hart-owned W/D arithmetic/transform path; an ordinary read/write pair cannot stand in for an atomic operation. | `src/physical.rs` (`AtomicRequest`, `AtomicResponse`, validated atomic access); `src/hart_amods.rs`; `tests/a8_atomic_contract.rs` and `tests/a7_physical_contract.rs`. | **PASS (bounded).** Envelope/transform and malformed/unknown-completion cases are part of the all-feature tests. No backend is credited with owning ISA arithmetic. |
| **T2 — native transactions, D-c, and writer versions** | One locked RAM envelope; complete target-span validation before mutation/failure; target rejection has no partial effect; UART atomic rejection; HTIF D-c; exact overlap-only committed-write bookkeeping across admitted storage writers. | `src/physical.rs::native_ram_atomic_transact` and native target adapters; `src/memory/mod.rs::SimpleMemory` committed-write bookkeeping and atomic primitives; `src/executor.rs::SystemBus::native_transact_atomic`; `tests/a8_atomic_targets.rs`, `tests/a7_native_targets.rs`, `tests/memory_bounds.rs`, and `tests/executor.rs`. | **PASS (bounded).** Tests cover rejected spans/backing RAM, callback-once AMO, HTIF LR/SC rejection, no-write conditional failure, overlap and non-overlap, committed prefixes, and writer bookkeeping. This does not grant atomic capability to unrelated targets. |
| **T3 — decode, Hart reservations, and W1–W8 writers** | Exhaustive `funct5` × W/D legality/operation table; per-Hart reservation/snapshot, reset/reload, success/failure/trap behavior; faulting SC retain; SC target-check order; explicit supported-writer visibility. | `src/isa/rv64a/dispatch.rs::decode_amo` / `execute_amo_port`; `src/core/mod.rs::CoreState::reservation` and `RiscvCore::step_outcome`; `src/hart_amods.rs`; `tests/a8_hart_atomic.rs`, `tests/a8_atomic_baseline.rs`, A7 migration/typed-compat tests, and the in-crate `src/isa/rv64a` unit tests included in `cargo test --all-features`. | **PASS (bounded).** The 32-value decode matrix, reserved encodings, W/D results, `rd=x0`, alignment/no-request cases, reservation lifecycle, exact-overlap and cross-thread cases are in current tests. Two-Core tests are test-object isolation only—not multi-Hart support. The typed-only constructor remains non-conforming. |
| **T4 — standard-facade bridge exit and public behavior** | CLI/`load_and_run` and flat facade use the validated atomic envelope on their shared physical domain; mixed public workflows, budgets, retirement-before-exit, reload and writer-resume behavior remain tested. | `src/executor.rs::install_image_with_physical_ports`; `src/core/mod.rs::step_outcome`; [`a8-public-atomic.md`](a8-public-atomic.md); `tests/a8_public_atomic_equivalence.rs` and A7/A6 compatibility regressions; seven appended `tests/bare-metal-riscv-test/rv64a/*.S` guests. | **PASS (bounded).** Public and direct-core seams are distinguished. A covered-reservation SC directly to HTIF cannot be seeded through the public runner because D-c rejects HTIF LR; the covered case is tested at the native core/backend seam, while public guests test no-reservation and live-uncovered SC. No production injection API is implied. |
| **T5 — exact evidence and cumulative audit** | Required checks at the final implementation tree; fresh project set = historic 51 plus seven A8 guests; distinct frozen ACT4 51-case result; debt and limitations; only §7.3 bounded attainment. | PR-head CI run `35951041896` at `dca3698...`; main push CI `35963679300` at exact merge HEAD `4c4d0a...`; frozen A5/ACT4 workflow run `35952692590` at `dca3698...`; source/test audit and per-case guest results in this assessment. | **PASS with stated retrieval limitation.** All 58 project guests and ACT4 run summary are in exact-head workflow logs. The ACT4 ZIP was not fully downloaded, so this assessment does not claim an independent local replay of that new artifact. No atomic certification or new ACT4 selection is claimed. |

## 4. Verification records, commands, and toolchains

### 4.1 PR candidate and merge-head CI

- **PR #65 CI:** [run 35951041896](https://github.com/mimiqdev/ruscv-sim/actions/runs/35951041896), event `pull_request`, exact head `dca3698ee105db5ad5dfaa0a704b6d4730ab8ab2`; [Quality and tests job 107479445657](https://github.com/mimiqdev/ruscv-sim/actions/runs/35951041896/job/107479445657) succeeded. Its formatting, Python A5 controls, strict clippy, all-feature tests, docs, and guest-script/path-guard steps passed. Guest compile/run, release build/smoke, and Coverage were skipped by workflow policy for a PR event; this run is not represented as a fresh project-guest run.
- **Main push CI:** [run 35963679300](https://github.com/mimiqdev/ruscv-sim/actions/runs/35963679300), event `push`, exact head `4c4d0a295f620ed85557b10475d46bef1be5ea17`; [Quality and tests job 107517458176](https://github.com/mimiqdev/ruscv-sim/actions/runs/35963679300/job/107517458176) succeeded. Coverage job `107518416766` was **skipped**, not failed. The job log's named steps provide the command/result record; `target/ci-riscv-elves/manifest.txt` is printed in the compile/run steps and identifies `source_head=4c4d0a...`, `source_count=58`, `compiled_count=58`.

The commands below are the exact commands in `.github/workflows/ci.yml` and the corresponding run logs. Both CI runs use the formatting, Python-control, clippy, test, docs and script-guard commands; the push-only commands ran at the merge commit.

```bash
# CI environment/toolchain setup
sudo apt-get update
sudo apt-get install --yes gcc-riscv64-unknown-elf binutils-riscv64-unknown-elf

# CI quality and guards
cargo fmt --all -- --check
python3 -m unittest discover -s scripts/a5 -p 'test_*.py'
bash -n scripts/a5/experiment.sh
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash -n scripts/riscv_elf_paths.sh scripts/compile_riscv_tests.sh scripts/run_elf_tests.sh scripts/test_riscv_elf_guards.sh
bash scripts/test_riscv_elf_guards.sh

# Push-only delivery and fresh guest checks
cargo build --release --all-features
./target/ci-cargo/release/ruscv-sim --version
./scripts/compile_riscv_tests.sh
# Step environment: RISCV_TEST_SKIP_BUILD=1
./scripts/run_elf_tests.sh
```

| Environment | Exact identity observed | Result |
| --- | --- | --- |
| GitHub CI Rust | `rustc 1.98.1 (48a229cea 2026-09-01)`; workflow `stable`, `rustfmt` and `clippy`. Cargo version was not printed in the retained job log and is not inferred here. | PR quality and main push quality passed. |
| GitHub CI RISC-V GNU packages | Ubuntu packages `gcc-riscv64-unknown-elf 13.2.0-11ubuntu1+12` and `binutils-riscv64-unknown-elf 2.42-1ubuntu1+6`; log identifies GNU assembler and linker `2.42`. | Fresh compilation of all 58 project guests passed. |
| qing full-gate image reported for PR head | `ghcr.io/mimiqdev/ruscv-sim-dev:main`, immutable digest `sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`; local image inspection reports `aarch64/linux`, rustc `1.97.1 (8bab26f4f 2026-07-14)`, cargo `1.97.1 (c980f4866 2026-06-30)`, RISC-V GCC `12.2.0-14+deb12u1+11+b2`, GNU assembler `2.40-2+4+b1`. | The task context reports the qing full-gate and diff checks passed at `dca3698...`; no qing ad-hoc argv is reconstructed. The published GitHub commands above are the durable exact-command evidence. |

The task context reports the qing full-gate included format/check/strict-clippy/all-feature-tests/docs/ELF guards/A6 toolchain integration and fresh project guest build/run `58/58`; `check:a8-dca-diff` passed against `391f268...`. Since the prior qing record is unavailable here and qing does not preserve ad-hoc command argv, these are cited as check references and are not used to fabricate command lines. The PR CI run and exact merge-head main-push CI are the reproducible repository records. The public CI workflow's listed steps do not include a standalone `cargo check` command; no such command is attributed to GitHub CI.

### 4.2 Frozen ACT4 selection — a distinct 51-case run

The new frozen-selection execution is [workflow run 35952692590](https://github.com/mimiqdev/ruscv-sim/actions/runs/35952692590), workflow **A5 ACT4 selected compatibility**, event `workflow_dispatch`, exact source head `dca3698ee105db5ad5dfaa0a704b6d4730ab8ab2`; job `107484412421` succeeded. The log records `A5_SELECTION=proposal`, checks out ACT4 source commit `a7c99303516f4e668f7488f172043392e23b9dfd` (ACT4 4.0.0), and reports:

| ACT4 workflow summary | Result |
| --- | ---: |
| Required sources | 51 |
| Generated self-check ELFs | 51 |
| Executed ELFs | 51 |
| Passed ELFs | 51 |
| Workflow/job conclusion | success |

This is a new run of the same frozen, approved `a5-rv64i-nontrapping-proposal-v1` selection, **not** an atomic/RV64A test and **not** the historical A7 run. The existing [`a5-selection-proposal.md`](a5-selection-proposal.md) and [`a7-act4-replay-35432032788.json`](a7-act4-replay-35432032788.json) pin its upstream scope: ACT4 commit `a7c99303516f4e668f7488f172043392e23b9dfd`, profile `a5-rv64i-nontrapping-proposal-v1`, plan SHA-256 `85c88859f7ab0cb76ea8d62f2dcf69ce6d0fc63177345a496b90cca2515ec24b`, inventory SHA-256 `78132f2f0bb001ba4a075f75ca95213189e7da98a2bac0e8d9b6409e0a8e659e`, profile SHA-256 `d08675968630ce4d75ed106de844b5690116036bd1a3e87a2fafe67116a0a1d8`, and tool-pins SHA-256 `1fe4c4882e338b79ffd75f6fa216c4589cf90a5e649d4d3157eff38b2f7fe9f1`. The same existing pin record gives ACT4 release 4.0.0 and its pinned toolchain inputs; the run log visibly records Ruby 3.4.9, Bundler 4.0.8, uv 0.11.6, and rustc 1.93.1. Because the new run's artifact was not fully retrieved, its internal `versions.txt` and per-case files are not independently replayed or asserted here.

GitHub's [artifact API](https://github.com/mimiqdev/ruscv-sim/actions/runs/35952692590/artifacts/10789676102) reports artifact **10789676102**, name `a5-feasibility-dca3698ee105db5ad5dfaa0a704b6d4730ab8ab2`, size **21,675,925 bytes**, digest `sha256:6f5a08a81fa1c50a84ecb6a581508146ff621ba49e0014c802441a19ef11bd5d`, expiry `2026-10-08T03:48:19Z`. A read-only artifact download attempt timed out after 120 seconds and left only a partial 2,522,129-byte file whose SHA-256 (`125c5408c0407e11740ca15853a1cce028decacf3d29a78d645cb5bb5c08cf77`) does not match the server-reported digest. The partial was not extracted or replayed. **Validated:** run identity, workflow/job success, logged 51/51 accounting, and server-reported artifact metadata. **Not validated:** a complete local copy of this new ZIP, its local SHA-256, or an offline artifact replay of its case files. No replay success is claimed.

For identity separation, the historical A7 ACT4 run remains workflow `35432032788` at source `fb6f51c771f32585f1547422c9633379f7ae370b`, artifact `10580339624`, 21,675,677 bytes, SHA-256 `a8d587f8310cb54e923c9a48c5b83f931ecf6b2054fd698a4b4d356802d95877` (see the committed A7 record). It is not reused as the new A8-adjacent run. The ACT4 51 and fresh project 58 are independent denominators; they are never added together or called one 109-case suite.

### 4.3 Fresh project guest inventory — exact merge-head run

The main-push CI run compiled fresh from `4c4d0a...` and executed every rebuilt ELF through the public CLI. CI job `107517458176`, steps **“Compile RISC-V test programs in a fresh output directory”** and **“Run RISC-V ELF tests through the public CLI”**, prints the manifest identity and each case's CLI output. All cases below had exit code `0`, status `SUCCESS`, and `[PASS]`; cycles are copied from that exact run log. The 58 comprise the retained project set of 43 RV64I and 8 RV64M guests, followed by seven A8 RV64A guests. The five A6 trap guests remain within the RV64I group. Linker PT_LOAD RWX warnings were non-fatal; compile summary reports 58 compiled and 0 failures.

**RV64I (43 cases)**

| # | Guest | Cycles |
| ---: | --- | ---: |
| 1 | `rv64i/add.elf` | 53 |
| 2 | `rv64i/addi.elf` | 32 |
| 3 | `rv64i/and.elf` | 45 |
| 4 | `rv64i/andi.elf` | 44 |
| 5 | `rv64i/beq.elf` | 23 |
| 6 | `rv64i/bge.elf` | 29 |
| 7 | `rv64i/bgeu.elf` | 34 |
| 8 | `rv64i/blt.elf` | 29 |
| 9 | `rv64i/bltu.elf` | 29 |
| 10 | `rv64i/bne.elf` | 26 |
| 11 | `rv64i/csrrc.elf` | 48 |
| 12 | `rv64i/csrrci.elf` | 46 |
| 13 | `rv64i/csrrs.elf` | 48 |
| 14 | `rv64i/csrrsi.elf` | 39 |
| 15 | `rv64i/csrrw.elf` | 44 |
| 16 | `rv64i/csrrwi.elf` | 28 |
| 17 | `rv64i/fib.elf` | 68 |
| 18 | `rv64i/hello.elf` | 58 |
| 19 | `rv64i/jal.elf` | 28 |
| 20 | `rv64i/jalr.elf` | 34 |
| 21 | `rv64i/lb.elf` | 63 |
| 22 | `rv64i/lh.elf` | 57 |
| 23 | `rv64i/lw.elf` | 49 |
| 24 | `rv64i/lwu.elf` | 87 |
| 25 | `rv64i/or.elf` | 45 |
| 26 | `rv64i/ori.elf` | 47 |
| 27 | `rv64i/sb.elf` | 127 |
| 28 | `rv64i/sh.elf` | 105 |
| 29 | `rv64i/sll.elf` | 36 |
| 30 | `rv64i/slli.elf` | 36 |
| 31 | `rv64i/sra.elf` | 40 |
| 32 | `rv64i/srai.elf` | 34 |
| 33 | `rv64i/srl.elf` | 36 |
| 34 | `rv64i/srli.elf` | 36 |
| 35 | `rv64i/sub.elf` | 28 |
| 36 | `rv64i/sw.elf` | 103 |
| 37 | `rv64i/trap_ebreak.elf` | 41 |
| 38 | `rv64i/trap_ecall.elf` | 51 |
| 39 | `rv64i/trap_illegal.elf` | 44 |
| 40 | `rv64i/trap_mret_priv.elf` | 65 |
| 41 | `rv64i/trap_vectored.elf` | 41 |
| 42 | `rv64i/xor.elf` | 44 |
| 43 | `rv64i/xori.elf` | 48 |

**RV64M (8 cases)**

| # | Guest | Cycles |
| ---: | --- | ---: |
| 44 | `rv64m/div.elf` | 71 |
| 45 | `rv64m/divu.elf` | 59 |
| 46 | `rv64m/mul.elf` | 49 |
| 47 | `rv64m/mulh.elf` | 52 |
| 48 | `rv64m/mulhsu.elf` | 48 |
| 49 | `rv64m/mulhu.elf` | 58 |
| 50 | `rv64m/rem.elf` | 62 |
| 51 | `rv64m/remu.elf` | 54 |

**RV64A project-authored additions (7 cases)**

| # | Guest | Cycles |
| ---: | --- | ---: |
| 52 | `rv64a/amo_d.elf` | 28 |
| 53 | `rv64a/amo_w.elf` | 29 |
| 54 | `rv64a/aq_rl_set.elf` | 23 |
| 55 | `rv64a/atomic_near_ram_end.elf` | 12 |
| 56 | `rv64a/atomic_tohost_exit.elf` | 4 |
| 57 | `rv64a/lrsc_loop.elf` | 16 |
| 58 | `rv64a/sc_after_store_failure.elf` | 20 |

**Exact merge-head guest result:** manifest `source_count=58`, `compiled_count=58`; compile failures `0`; public CLI `Total: 58`, `Passed: 58`, `Failed: 0`. This is the project-authored suite only. It does not subsume or extend ACT4.

## 5. Bounded §7.3 attainment and overall milestone status

At implementation HEAD `4c4d0a295f620ed85557b10475d46bef1be5ea17`, the exact merge-head CI and the equal-tree PR-head checks support the one bounded attainment sentence authorized by `docs/dev-plan.md` §7.3:

> **Single-Hart atomic/physical convergence completed for the standard facades' native domain: AMO/LR/SC execute as indivisible envelope operations with per-Hart reservation state and explicit writer visibility, with the legacy typed bridge retired from the standard facades.**

This sentence is restricted to the **native single-Hart domain of the standard facades** and the A8 profile tested here. It does not mean full ADR-0002 conformance, all physical accesses through one port, MMU/PMP integration, full Machine/Runner architecture, RV64A/Zalrsc/Zamo certification, RVWMO proof, multi-Hart or DMA support, or a new ACT4 result. `docs/dev-plan.md` remains the sole Current contract. This assessment does not archive or replace it, activate a successor, or assert that the repository's rolling-milestone transition is complete.

## 6. Residual limitations and debt

| Boundary | Evidence-based status and non-claim |
| --- | --- |
| MMU/Sv39, page walks/A-D writes and PMP | Components may have independent tests, but page walks/A-D writes are not demonstrated on the A8 public physical port and PMP permission behavior is not established. A8 target-span validation is not virtual permission checking. |
| TLM/DMI/SystemC/FFI | Existing components are not proof of a Hart-facing atomic-envelope adapter, native/TLM parity, DMI invalidation, or external-kernel integration. No such support is claimed. |
| Machine lifecycle and full ADR-0001/0003 boundaries | A8 retains existing reusable facades; it does not deliver Machine composition, coordinated reset/quiesce/drain, complete observation ownership, or a new lifecycle contract. |
| Interrupts, WFI, time and debug | Not implemented by this A8 slice; no asynchronous interrupt, timer scheduling, Debug Mode or external debug end-to-end support is claimed. |
| Multi-Hart, DMA, ordering and RVWMO | Two-core test fixtures establish independent test-object reservations only. They do not implement a Hart scheduler, inbound master, coherence, multi-Hart ordering, or RVWMO. |
| Atomic profile strength | `aq`/`rl` are accepted with the contract's no-additional-ordering single-Hart strength. The overlap-only P2a writer rule and span-containment choices are profile decisions/deviations, not an external certification result. |
| Typed-only `RiscvCore::new` route | Retained as a labeled non-conforming compatibility adapter: it shares per-Hart state but lacks the standard target permission/capability and committed-write snapshot guarantees. The standard facades' bridge exit does not certify this constructor. |
| HTIF covered-reservation SC seam | Public guests cannot create a reservation at HTIF because D-c rejects HTIF LR. Covered HTIF SC target rejection and callback count are tested at the direct native core/backend seam; public tests cover no-reservation and live-uncovered SC. No production reservation-injection API is added. |
| ISA certification and broader conformance | The seven project-authored RV64A guests and Rust regressions are project tests, not external RV64A/Zalrsc/Zamo certification. ACT4 remains the separately selected frozen 51-case nontrapping RV64I profile only. |
| Performance | No A8 performance facility, threshold, or performance result is part of these checks. Historical A7 performance observations are not current A8 evidence. |

## 7. Historical/navigation corrections

The existing A8 T5 record remains untouched as a historical exact-head record; its `45a170...` command results are stale for this implementation. The A7 closeout and archived A7 contract are also unchanged. The current T0 ledger and T4 route audit preserve their history while the correction addenda and final source/tests control current SC expectations.

The root `README.md` and `docs/README.md` previously described A8 implementation as not started and/or the final evidence as still pending. This assessment is linked from those navigation points and replaces only those stale status sentences. No active contract, ADR, archived record, source, test or CI configuration is changed by this documentation PR.

## 8. Source and evidence index

- Current A8 authority and acceptance rows: [`docs/dev-plan.md`](../dev-plan.md) §§2, 5–8, 10–11.
- Accepted architecture: [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- A8 T0 fixture ledger and SC correction: [`a8-fixture-reclassification.md`](a8-fixture-reclassification.md).
- A8 T4 public route audit and correction addendum: [`a8-public-atomic.md`](a8-public-atomic.md).
- Historical T5 evidence bound to `45a170...`: [`a8-t5-evidence.md`](a8-t5-evidence.md); not used for current check results.
- Source/test anchors: `src/physical.rs`, `src/memory/mod.rs`, `src/hart_amods.rs`, `src/isa/rv64a/dispatch.rs`, `src/core/mod.rs`, `src/executor.rs`; `tests/a8_atomic_contract.rs`, `tests/a8_atomic_targets.rs`, `tests/a8_hart_atomic.rs`, `tests/a8_public_atomic_equivalence.rs`, `tests/a8_atomic_baseline.rs`.
- A7 historical acceptance and debt: [`a7-closeout-record.md`](../archive/milestones/a7-closeout-record.md); historic ACT4 identity: [`a7-act4-replay-35432032788.json`](a7-act4-replay-35432032788.json).
- Frozen A5/ACT4 selection and pin records: [`a5-selection-proposal.md`](a5-selection-proposal.md), [`a5-feasibility-pins.json`](a5-feasibility-pins.json).
- CI command source: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml); ACT4 workflow source: [`.github/workflows/a5-feasibility.yml`](../../.github/workflows/a5-feasibility.yml).
