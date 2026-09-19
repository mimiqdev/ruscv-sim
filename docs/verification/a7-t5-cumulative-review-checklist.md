# A7 T5 cumulative source-review checklist

**Status:** Coding-side dependency map; **not** an independent review result.

**Implementation evidence head:**
`fb6f51c771f32585f1547422c9633379f7ae370b`

**Cumulative implementation range:** T0 `fc68fcf` through T4
`fb6f51c`. The final PR head also contains only the bounded T5 harness,
replay-validation tests, tree bridge, and evidence-document updates. A future
independent reviewer must review the complete current source/test wiring, not a
relabelled historical diff range and not only the new documentation files.

## Review protocol

The coding-side audit below records where each T5 conclusion comes from. It is
not a certification of those rows. The independent reviewer should:

1. freeze the exact PR head and inspect the complete T0--T5 range against
   `docs/dev-plan.md` and ADR-0001--0004;
2. re-open the implementation symbols named below and the exact test assertions,
   including the pre-migration public-facade route at
   `9ee9c5f24c072779f06ce141b3340f953b614442`;
3. run the applicable full gate, focused tests, replay negative tests, the
   public-facade comparison, and the protected-tree bridge at that exact head;
4. distinguish component tests, public-facade integration, fresh project ELF
   execution, and frozen ACT4 replay; and
5. record any finding against the final PR head. Historical A6/T3/T4 review
   conclusions do not automatically approve this cumulative head.

No independent reviewer has completed this checklist in this coding round.
The assessment therefore uses “coding-side audit” and “pending independent
review,” not “independently reviewed,” for the rows below.

## Contract-to-source-and-assertion map

| Contract row | Source dependencies to inspect | Concrete assertions/tests to inspect | Coding-side disposition | Independent-review disposition |
| --- | --- | --- | --- | --- |
| **T0 — preserve the pre-migration boundary and label debt** | `src/core/mod.rs` legacy typed `RiscvCore::new` path; `src/executor.rs` typed `SystemBus` methods, HTIF start-address behavior, flat host APIs; the full baseline record in `a7-migration-characterization.md`. | `tests/a7_migration_characterization.rs::opcode_funct5_dispatch_preserves_current_amo_width_debt`; `direct_htif_dword_uses_start_address_only_for_base_through_interior`; `ram_first_full_span_and_htif_fallback_are_distinct`; `guest_aligned_access_to_misaligned_flat_offset_is_an_access_fault`; `failed_flat_image_load_retains_the_previous_image_and_runtime_artifact`; `host_write_mem_keeps_empty_offset_and_prefix_on_failure_semantics`; `global_reservation_characterization_isolated`. Baseline/candidate fixture comparison is bound to the full `9ee9c5f` identity. | Audited. The coding record preserves successful legacy AMO/LR/SC behavior as debt rather than converting it to a denial. | **Pending.** Recheck that the baseline comparison uses the same public facade and that no T0 observation is relabelled as conformance. |
| **T1 — raw non-atomic vocabulary and validation** | `src/physical.rs::{PhysicalRequest, PhysicalResponse, ValidatedPhysicalAccess, PhysicalBackendResult}`; explicit `Fetch`, `DataRead`, `DataWrite`, widths, raw bytes, and typed target/host/protocol/unknown errors. | `tests/a7_physical_contract.rs::complete_fetch_read_and_write_ack_cover_all_explicit_widths`; `malformed_width_and_payload_requests_fail_before_backend_dispatch`; `short_and_long_read_responses_are_protocol_failures_not_success`; `wrong_category_and_mismatched_request_binding_are_protocol_failures`; `contradictory_completion_kinds_are_protocol_failures`; `unknown_completion_is_terminal_and_is_never_retried_or_mapped`. | Audited; 12 focused tests passed at the implementation evidence head. | **Pending.** Confirm validation precedes backend dispatch and that unknown is not guessed from diagnostic text. |
| **T2 — native targets share storage and side effects** | `src/executor.rs::SystemBus::{native_transact,physical_backend}`; `src/physical.rs::{NativeRamBackend,NativeSystemBusBackend,SharedNativeBackend}`; RAM/UART/HTIF child locks and full-span routing. | `tests/a7_native_targets.rs::old_typed_and_native_ram_views_share_one_storage_and_locking_domain`; `native_system_bus_old_and_new_views_share_ram_uart_and_callbacks`; `native_system_bus_never_stitches_ram_into_uart_or_htif`; `native_htif_requires_the_complete_raw_endpoint_and_does_not_fetch`; `native_uart_rejections_preserve_fifo_status_and_output_then_success_has_one_effect`; `poisoned_native_locks_are_backend_failures_without_retry_or_target_mutation`. | Audited; 13 focused tests passed. The same storage, no-copy, full-span, side-effect fence, and lock-domain facts are recorded. | **Pending.** Recheck both address-map forms and lock order; do not infer native facade injection coverage from component-only seams. |
| **T3 — Hart ordinary route and legacy bridge** | `src/core/mod.rs::{step_outcome,uses_non_atomic_access,physical_adapter,new_with_physical_access,new_with_physical_ports,clear_unresolved_physical_access}`; `src/executor.rs::install_image_with_physical_ports`; `src/physical.rs::PhysicalMemoryAdapter` and native backends. | `tests/a7_hart_physical.rs::real_fetch_integer_and_fp_accesses_use_raw_ports_and_one_ram`; `hart_alignment_and_misaligned_storage_offset_issue_no_data_request`; `native_non_aligned_storage_offsets_retain_fetch_load_and_store_faults`; `target_rejections_map_to_fetch_load_store_causes_with_original_mtval`; `host_protocol_and_unknown_failures_are_simulator_failures_not_traps`; `unknown_completion_is_terminal_and_does_not_retry_the_physical_domain`. | Audited; 6 focused Hart tests passed. Hart owns address/alignment/extension/trap/retirement; the raw port is not a second ISA engine. | **Pending.** Recheck no ordinary integer/FP path falls back to typed memory and no failure fabricates retirement or trap state. |
| **T3 — shared storage, lock re-entry, two address forms** | Native `SystemBus` uses two raw handles over one `Arc<Mutex<SystemBus>>`; flat `RiscVSimulator` uses two raw handles over one `Arc<Mutex<SimpleMemory>>`; legacy AMO/LR/SC uses the typed bridge without reacquiring the same outer lock. `src/executor.rs::{load_and_run,RiscVSimulator}` owns native/bus versus flat/storage-offset forms. | `tests/a7_legacy_atomic_compat.rs::ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain`; `ordinary_store_and_fp_store_interleave_with_unchanged_legacy_lr_sc`; `rejected_ordinary_write_and_faulting_sc_preserve_characterized_reservation`; `legacy_lr_survives_reset_and_replacement_storage_with_global_key_behavior`; `public_flat_reload_replaces_storage_but_retains_legacy_lr_key`; `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection`; `legacy_lock_reentry_child`. | Audited; 8 legacy-compatibility tests passed. The child timeout/reap is bounded evidence, not concurrency safety. | **Pending.** Explicitly review shared RAM/lock identity, both native guest-address and flat storage-offset forms, old typed constructors, and the re-entry child. |
| **T3 — retained legacy debt** | `src/execute` AMO dispatch and `src/isa/rv64a`; `GLOBAL_RESERVATION`; typed `SystemBus` compatibility methods; no second reservation state. | `tests/a7_migration_characterization.rs` T0 rows; `tests/a7_legacy_atomic_compat.rs::successful_legacy_sc_width_debt_mmio_callback_is_retained`; ordinary-store/FP/host-write and reset/reload tests above; `tests/amo_test.rs`. | Audited as preservation only. Known global reservation, invalidation, reset, SC-fault, and AMO-width debt remains explicit. | **Pending.** Confirm the record does not claim atomic envelopes, `aq/rl`, per-Hart reservations, coherence, or full A-extension support. |
| **T4 — every standard public route** | `src/executor.rs::load_and_run` and `RiscVSimulator::{load_elf,run}`; `install_image_with_physical_ports`; CLI `src/main.rs`; typed host inspection/observer paths; `src/core` retired/trap/failure boundary. | `tests/a7_public_equivalence.rs::public_workflow_equivalence_covers_entry_trap_integer_fp_legacy_and_exit`; `double_low_half_mutation_reaches_the_shared_failure_oracle`; `public_budget_zero_exact_exit_and_final_slot_are_distinct`; `recursive_traps_preserve_started_completed_and_minstret_accounting`; `final_slot_host_failure_is_started_but_not_completed_at_the_flat_public_boundary`; `unknown_completion_is_terminal_until_public_host_resolution_and_never_retried`; `old_memory_interface_backend_is_compiled_called_with_width_order_and_independent_handles`; `public_fetch_trace_and_trap_observation_do_not_refetch_or_commit_a_trap`; `signature_artifacts_keep_absent_empty_readable_and_unreadable_native_flat_policies`; `reload_and_configuration_boundaries_keep_their_explicit_differences`. | Audited; 10 public-equivalence tests passed. Native CLI, native library, and flat library differences are retained as explicit configuration facts. | **Pending.** Recheck all public route assertions and the old typed `MemoryInterface` compatibility exception; do not treat `RiscvCore::new` as evidence of the migrated standard facade. |
| **T5 — real migration comparison** | `scripts/a7/compare_physical_loop.py`; fixture `physical_access_loop.S`/`.ld`; archived source revisions; current `load_and_run` installation and `ValidatedPhysicalAccess`; committed `a7-physical-loop-observation.json`. | Harness builds both archived revisions through `ruscv_sim::executor::load_and_run`, uses the same ELF bytes and compile flags, retains 45 samples per revision, checks exit/result/final PC/cycles, and compares commit-record count to `ExecutionResult.cycles`. Static route checks distinguish old typed facade from current physical-port facade. | Coding-side observation complete: both public facades compiled and all samples returned exit 0 with 20,490 retirements; no speed gate. | **Pending.** Re-run or inspect the exact harness/report, including the slower current distribution, environment, no-filter policy, and static allocation/lock observation. |
| **T5 — replay and evidence identity** | `scripts/a7/replay_act4.py`; committed replay JSON; actual ZIP and GitHub artifact metadata; `scripts/a7/verify_evidence_tree.py`. | `scripts/a7/test_replay_act4.py` negative paths cover digest mismatch, path traversal, duplicate members, summary mutation, and accounting mutations. The tree bridge compares regenerated replay JSON to the committed JSON, checks ZIP digest/size/ID/source head, and rejects protected runtime/tests/CI changes. | Coding-side tool checks complete when the final exact-head commands are recorded; historical ACT4 success remains bound to `fb6f51c`. | **Pending.** Verify the real ZIP replay and the protected-tree bridge; do not use a prior review as tool-test evidence. |

## Required final review evidence

At the frozen final PR head, the independent review record should cite:

- the full Rust gate and focused/guard suite at that exact head;
- the archived public-facade comparison and its complete retained sample
  distributions, with the old typed route and current raw-port route identified;
- `python3 -m unittest discover -s scripts/a7 -p 'test_*.py' -v`;
- a real ZIP replay and `verify_evidence_tree.py` result, including artifact
  size/digest, committed/generated JSON equality, and an empty protected-tree
  change list;
- the exact T0 identity `fc68fcf`, the runtime baseline full identity, and the
  implementation evidence full identity; and
- any unresolved finding or explicit limitation. No “evidence complete” or
  “cumulative audit complete” conclusion should be recorded until those review
  steps are independently performed.

## Explicitly unreviewed or unclaimed

This checklist does not claim independent review of the current runtime
baseline, cumulative T0--T4 source wiring, high-level native host-failure or
unknown-completion injection, MMU/PMP, asynchronous interrupts, Debug Mode,
full Machine/Platform integration, atomic convergence, or any performance
acceptance threshold. It also does not close A7, change the active plan, or
modify `.qing/config.toml`.
