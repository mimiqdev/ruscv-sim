# A6 Capability and Closeout-Preparation Assessment

**Status:** Current — merged closeout-preparation evidence; final rolling switch pending

**Authority:** Informational evidence record. `docs/dev-plan.md` remains the
sole active normative contract, and source plus executed tests are authoritative
for implementation claims.

**Last verified:** 2026-09-18

**Scope:** Milestone A6's nine acceptance criteria, the exact Task 4 verification
identity, and the retained A5 ACT4 replay boundary. This record does not replace
`docs/dev-plan.md` or approve a successor. Closeout preparation merged in
[PR #46](https://github.com/mimiqdev/ruscv-sim/pull/46) on 2026-09-18 as
`024d15d546dc3b711f593cd44bb107612fd8b600`; final rolling closeout is separate.

## Evidence identity and status boundary

Tasks 1–4 were delivered by PRs #42–#45. The implementation merge is
`f12b1f80907e92b1c82b33ac669b961cc17f59c9`; the exact Task 4 source head used by
fresh verification is
`845c63325db5ac87ab2ff0ed260453dc3b396ae9`. These revisions have the same Git
tree (`ce60a0a9c984fd42e43ecbb156e860176ecd6c47`), but they are not the same
commit. The Task 4 evidence is therefore cited as evidence for the identical
implementation tree, never relabeled as a run performed at a later
closeout-documentation HEAD.

| Task | PR | implementation/source identity | merge identity |
| --- | --- | --- | --- |
| Trap CSRs, causes, vectors, counter/WARL foundations | [#42](https://github.com/mimiqdev/ruscv-sim/pull/42) | PR head `68495ab14190486b165a14c3907a1f7c8c78e20d` | `ece94463e476075fa89ba6e2dff56e4cf2e2abfc` |
| MRET privilege restoration | [#43](https://github.com/mimiqdev/ruscv-sim/pull/43) | PR head `8cbf98369de3693c7977aa81bf282f86049d93b1` | `2f14cc5e83869c7a383cb4b9c1ffd706e19cb053` |
| Core outcomes, trap continuation, runner accounting | [#44](https://github.com/mimiqdev/ruscv-sim/pull/44) | PR head `8b509939728c7229de359671f82ef800e03c9560` | `51c6e6332f07dceecc0cfae8d54fbfd7c7fb2cab` |
| Fresh trap guests, induced faults, ELF guards | [#45](https://github.com/mimiqdev/ruscv-sim/pull/45) | `845c63325db5ac87ab2ff0ed260453dc3b396ae9` | `f12b1f80907e92b1c82b33ac669b961cc17f59c9` |

PR #45's CI run [35328370857](https://github.com/mimiqdev/ruscv-sim/actions/runs/35328370857)
completed successfully at the exact source head. Its `Quality and tests` job
passed; the separate coverage job was skipped. The local fresh verification
recorded for that same source tree used the immutable ARM64 development image
`ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c`,
Colima ARM64/4 GB, `bash -c` (not `bash -lc`), and `CARGO_BUILD_JOBS=2`. It ran
fresh archive-source quality checks (`fmt`, `check`, strict all-target Clippy,
all-feature tests, docs, and ELF guard tests), then freshly compiled and ran the
project-authored ELF suite: **51/51**, including the five A6 trap guests. Those
51 project guests are not ACT4 cases.

The retained ACT4 run is separately replayed by
[`scripts/a6/replay_act4.py`](../../scripts/a6/replay_act4.py). Its compact
report is [`a6-act4-replay-35325844246.json`](a6-act4-replay-35325844246.json).
The read-only artifact is run **35325844246**, source head `845c633...`, artifact
`10538929657`, `21,675,875` bytes, SHA-256
`9c33a06e458bb99331aad141c44f3604979139d82502216d04d73290bbabf452`. The
replay verified the frozen A5 nontrapping selection, all 51 public-CLI results,
all 51 linked audits, 12 negative accounting controls, and six guest controls.
It did not execute a guest or regenerate an ELF. This is A5 external
compatibility evidence, not A6 trap certification or extension-wide
certification. The historical `scripts/a5/replay_accounting.py` remains
unchanged; its refusal of this newer run because it intentionally hard-codes an
older run is expected.

## Merged preparation evidence

GitHub metadata checked on 2026-09-18 confirms PR #46's final head
`90e7dfe470e93be799f1e8e1047ab438e0285516` and successful
[CI 35331709689](https://github.com/mimiqdev/ruscv-sim/actions/runs/35331709689)
`Quality and tests` job. Coverage and the release/fresh project ELF compile/run
steps were skipped. This PR-head result is not a new guest-suite run at
`024d15d...`. The [closeout record](../archive/milestones/a6-closeout-record.md)
retains the earlier `b8c0bba...` local checks and the Task 4 fresh source/tree
identities; those results are not reassigned to the merge or this document's HEAD.

Exact-merge [main CI 35332790906](https://github.com/mimiqdev/ruscv-sim/actions/runs/35332790906)
separately succeeded at `024d15d546dc3b711f593cd44bb107612fd8b600`.
The observed job/log includes Rust checks, release build/smoke, fresh project
ELF compile/run and **51 total / 51 passed / 0 failed**, including all five A6
trap guests; Coverage was skipped. This supplies new merge-head regression and
project evidence for the matrix, without changing the older rows' evidence
identities. It does not rerun ACT4; run 35325844246 remains the frozen external
baseline at its original revision.

## Nine-criterion matrix

Each disposition below is based on the cited source and assertion, not on the
fact that a task PR merged. `Evidenced for closeout preparation` means the
criterion has repository evidence at the stated identity; formal milestone
closeout still requires the final rolling-switch decision and must not be
inferred merely from the merged preparation. The nine rows retain their original
evidence revisions and limitations.

| # | Contract clause → source | Concrete test assertion(s) | Command / exact revision evidence | Disposition and limits |
| --- | --- | --- | --- | --- |
| 1 | Synchronous entry and causes 0/1/4/5/6/7 → `src/core/mod.rs::step_outcome`, `enter_trap`, `classify_execute_error`; `src/core/trap.rs::handle_trap_checked`; `src/memory/mod.rs` | `test_machine_trap_entry_all_a6_synchronous_causes`; `task4_induced_faults_preserve_boundary_state_and_physical_transactions`; `fetch_load_store_and_control_alignment_faults_have_no_partial_side_effects`; `atomic_misalignment_enters_store_trap_before_any_data_transaction` assert cause, `mepc`, `mtval`, vector PC, unchanged `rd`/memory, `minstret == 0`, and zero transactions for Hart-side misalignment. | `cargo test --test trap_test`; `cargo test --test a6_task3_core_trap_test`; Task 4 fresh Rust/ELF evidence at source `845c633...`, tree equal to merge `f12b1f8...`. | **Evidenced for closeout preparation.** Physical flat-bus faults are covered; MMU/PMP/page faults and unknown adapter completion remain out of scope. |
| 2 | Direct/vectored target calculation and guest trap behavior → `src/core/trap.rs::vector_trap`; `tests/bare-metal-riscv-test/rv64i/trap_{ecall,illegal,ebreak,vectored}.S` | `test_vector_trap_vectored_mode`, `test_vectored_mode_synchronous_exceptions_use_base`, and `test_vectored_mode_interrupt_offsets` distinguish synchronous BASE from interrupt offsets; the vectored guest's `BASE + 4*cause` sentinel fails if incorrectly reached. | `cargo test --test trap_test`; fresh five-guest build/run in the recorded PR45 verification at `845c633...`/tree `ce60a0a...`; CI `35328370857`. | **Evidenced for closeout preparation.** Only the A6 Machine-mode synchronous profile is claimed; asynchronous interrupt delivery is not integrated into the public core. |
| 3 | ADR-0001/0003/0004 outcomes, started-slot/completed-turn accounting, `minstret` and write precedence → `src/core/mod.rs`, `src/csr/mod.rs`, `src/executor.rs::RunControl` | `normal_and_explicit_minstret_retirement_are_distinct`; `zero_budget_does_not_start_a_turn_or_retire`; `runner_counts_trap_entry_as_a_completed_turn_but_not_a_retirement`; `runner_consumes_each_recursive_trap_slot_until_timeout`; `last_slot_host_failure_consumes_the_slot_without_a_completed_turn`; CSR access tests assert CSRRW/CSRRWI always-write and CSRRS/CSRRC/CSRRSI/CSRRCI identity/zimm gating. | `cargo test --test a6_task3_core_trap_test`; `cargo test --test csr_basic_test`; `cargo test --test csr_access_test`; `cargo test --lib executor::tests`; source implementation merge `f12b1f8...`; focused closeout assertions are kept in this branch without changing ISA implementation. | **Evidenced for closeout preparation.** `mcycle`, shadow counters, and counter access controls are explicitly deferred. `ExecutionResult.cycles` is completed turns, not a hardware-cycle claim. |
| 4 | Continue-to-guest-handler and non-lossy trap boundaries → `src/core/mod.rs::StepOutcome`; `src/executor.rs` matches `TrapContinuationPolicy::ContinueToGuestHandler` and charges a completed turn before the next loop iteration. | `trap_entry_then_handler_and_mret_have_typed_boundaries` proves handler execution and return; `recursive_and_heterogeneous_traps_replace_the_saved_record_without_retiring` proves distinct consecutive Hart facts; recursive runner test proves each boundary consumes a slot and reaches timeout rather than host `ExecutionError`; the five guests exit through handlers. | `cargo test --test a6_task3_core_trap_test`; `cargo test --test a6_trap_elf_integration` with required toolchain; fresh project ELF run 51/51 at `845c633...`; merge tree checked against `f12b1f8...`. | **Evidenced for closeout preparation.** The current public runner consumes the typed fact internally; no durable per-instruction observer stream or target ADR Machine abstraction is claimed. |
| 5 | Legal MRET restoration and MPRV rule → `src/isa/rv64i/system.rs::exec_mret` | `mret_restores_all_mie_mpie_combinations_and_privilege_modes` checks PC WARL alignment, MIE/MPIE/MPP, privilege and MPRV preservation/clearance; `trap_entry_then_handler_and_mret_have_typed_boundaries` checks retirement; `trap_mret_priv.S` checks guest-visible behavior. | `cargo test --test mret_conformance_test`; `cargo test --test a6_task3_core_trap_test`; fresh guest/CI evidence at `845c633...` and `35328370857`. | **Evidenced for closeout preparation.** Supervisor `SRET`, delegation as a public A6 path, and user trap delegation are not claimed. |
| 6 | MRET privilege rejection and standard illegal trap entry → `src/isa/rv64i/system.rs::exec_mret`; `src/core/mod.rs::classify_execute_error` | `mret_from_user_or_supervisor_is_typed_illegal_instruction_without_side_effects` checks no return side effects; `lower_privilege_mret_error_composes_with_standard_illegal_trap_entry` checks `mcause=2`, `mtval`, `mepc`, MPP, MIE/MPIE, MPRV, Machine privilege and BASE vector; `trap_mret_priv.S` repeats the check in a guest. | `cargo test --test mret_conformance_test`; `cargo test --test a6_task3_core_trap_test`; fresh five-guest run at `845c633...`; CI `35328370857`. | **Evidenced for closeout preparation.** This is the supported User/Supervisor/Machine profile only; no broader privilege architecture is implied. |
| 7 | Five project-authored trap ELFs through public CLI and `tohost` → `tests/bare-metal-riscv-test/rv64i/trap_*.S`, `tests/a6_trap_elf_integration.rs`, `scripts/compile_riscv_tests.sh`, `scripts/run_elf_tests.sh` | Each guest has explicit pass/fail writes; integration asserts CLI process success plus `load_and_run` and `RiscVSimulator` `exit_code == 0`, not timeout/error. Fresh suite manifest checks each true `_start` entry. | Fresh source-archive/container verification at `845c633...`: project total 51, passed 51; CI run `35328370857`; `cargo test --test a6_trap_elf_integration` requires the cross-toolchain and skips only when not required. | **Evidenced for closeout preparation.** These are project regressions, not ACT4/Sail cases; the host's absent native cross-toolchain is an explicit skip unless `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`. |
| 8 | Public runner options, exit ordering, UART and cycle limits → `src/executor.rs`, `src/main.rs`, `tests/executor.rs`, `tests/peripheral_tests.rs` | Executor tests assert zero/finite timeout shapes, HTIF decoding/clear-after-retain, successful exit and commit ordering; peripheral tests cover UART MMIO; trap guests assert handler-tohost exit after trap. | `cargo test --test executor`; `cargo test --test peripheral_tests`; fresh CLI trap run and project suite at `845c633...`; quality CI `35328370857`. | **Evidenced for closeout preparation.** No claim is made for a future Machine/Platform result taxonomy; UART is the current public byte-MMIO path. |
| 9 | No regressions and quality gate → repository tests/scripts and CI | The fresh run reports `fmt`, `check`, strict all-target Clippy, all-feature tests, docs, ELF guards, and project ELF `51/51`; the ACT4 replay separately reclassifies every 51 output and six controls. | PR45 CI `35328370857` at exact `845c633...` (`Quality and tests: success`); immutable ARM64 fresh verification at the same tree; ACT4 replay report for run `35325844246`; local replay/tool tests below. | **Evidenced for closeout preparation.** The evidence is bounded to the recorded exact heads. It is not a whole-ISA, ACT4-extension, MMU, interrupt, compressed-instruction, or OS-boot certification. |

## Reproducible offline replay and local checks

The replay reads the retained ZIP, validates its recorded byte count and
SHA-256 before inspection, and never writes into the artifact. It uses only
stdlib Python, the checked-in plan/configuration hashes, archive members, and a
local Git tree comparison; it does not call GitHub, Docker, `gh`, an orchestration
service, a private absolute path, or the old hard-coded A5 replay. It does not
extract or commit the large ZIP/ELFs.

From the repository root, with the read-only artifact supplied separately:

```bash
python3 scripts/a6/replay_act4.py \
  "$HOME/a5-feasibility-845c63325db5ac87ab2ff0ed260453dc3b396ae9.zip" \
  --write docs/verification/a6-act4-replay-35325844246.json
python3 -m unittest discover -s scripts/a6 -p 'test_*.py'
```

The durable JSON records the artifact ID/hash/size, source-to-merge tree
identity, plan/config hashes, exact 51/51 classification counts, 51 audit
checks, 12 rejected accounting mutations, six guest-control classifications,
and the A5-only scope boundary. The ZIP remains external retained evidence.

## Closeout and rolling-plan boundary

This assessment records merged closeout preparation, not final rolling closeout.
The active normative contract remains [`docs/dev-plan.md`](../dev-plan.md), including its
§9 sequence. No successor milestone has been approved. Therefore this change
must not replace `docs/dev-plan.md`, create an A5 forwarding record, enable A7,
move old backlog items, weaken documentation policy, or alter ISA behavior. The
archive copy and closeout record distinguish the now-merged preparation from the
later final rolling switch: a maintainer must verify the nine criteria again and
separately approve a successor before replacing the active plan. The
[A7 non-atomic migration candidate](../proposals/a7-physical-access-contract.md)
is Draft and non-normative. The scope direction preserves legacy atomic behavior
as debt and rejects disabling it, but the full revised contract and preservation
ledger still await approval. Documentation policy keeps A6 effective until that
approval; the candidate specifies how one activation change can finalize A6 and
rotate the active plan together without relabeling historical evidence.

Known boundaries retained in the eventual closeout record are: flat physical
RAM/UART/HTIF execution; synchronous Machine-mode traps only; fixed 32-bit
fetch/IALIGN=32 with C disabled; no MMU/Sv39 wiring, PMP, asynchronous
interrupts, SRET/delegation integration, Debug Mode, multi-Hart ordering,
SystemC/TLM public integration, or end-to-end M/A/F/D/C certification. The
A5 51-case replay is a nontrapping RV64I external baseline and does not enlarge
any of those claims.
