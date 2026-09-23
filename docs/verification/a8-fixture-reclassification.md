# A8 T0 — A7 atomic fixture reclassification ledger

**Status:** T0 deliverable of the A8 contract (`docs/dev-plan.md` §8 T0, §9).
This ledger is complete: every A7 atomic fixture row has a definite
classification, and every row without existing expectation coverage is
recorded as a T0 new row with an executable baseline. No behavior changed in
T0: no `src/` file is modified by this task, and the exit command below is
green with the A7 fixtures unmodified.

**Runtime baseline (this record's verification head):** worktree
`ruscv-sim-a8-t0-baseline`, branch `agent/a8-t0-baseline`, base commit
`6c0a6df89c7cac7d2be6009fd41ad547b418d38d` (A8 activation, #57). All
"verified" claims below were checked by reading the cited source at this head
and by running the cited tests on the unchanged code.

**Executable baseline:** `tests/a8_atomic_baseline.rs` (18 tests, all green at
this head). Its assertions are the old column of the A8 §6 compatibility
matrix in executable form; every later flip is a diff against it.

**Exit command (run at this head, all green):**

```bash
cargo test --test a8_atomic_baseline --test a7_migration_characterization \
  --test a7_legacy_atomic_compat --test amo_test
# a8_atomic_baseline 18 passed; a7_migration_characterization 11 passed;
# a7_legacy_atomic_compat 8 passed; amo_test 31 passed.
```

## Method

* "Old" is the verified §3 behavior of the A8 contract; "new expected value"
  is the approved profile (§2, §5, §6, §11), not an implementation claim.
* Classification vocabulary is §9: **flip** (expectation changes, fixture
  retained), **retire** (debt machinery removed, replaced by a new-equivalent
  assertion), **keep** (expectation unchanged; "relabel" is the keep
  sub-case where the same observable is reclassified from accident to
  approved profile), and **T0 new row** (no A7 fixture covered the behavior;
  the executable baseline now pins it).
* Every T0 baseline assertion was verified against the running code, not
  inferred from documentation. Rows the baseline exercises cite their test
  name; rows covered by the unchanged A7 fixtures cite the fixture itself as
  the old-column evidence that flips at T3.

## 1. `tests/a7_migration_characterization.rs`

### 1.1 Named atomic tests

| Fixture row | Old verified expectation | Class | New expected value (post-A8) | Approving row | T0 baseline |
| --- | --- | --- | --- | --- | --- |
| `opcode_funct5_dispatch_preserves_current_amo_width_debt` — sub-case W (`00001`/`010`) retires AMOADD arithmetic via `read_word`/`write_word` | `rd = sext(old)`, memory `old + rs2` | **Flip** | Real AMOSWAP.W: `rd = sext(old)`, memory = low 32 bits of `rs2` | C2, §11 item 1 | `dispatch_matrix_pins_all_32_funct5_values` (row `00001`), `amoswap_encoding_executes_amoadd_arithmetic` |
| same test — sub-case D (`00001`/`011`) retires the word helper with a sign-extended `rd` | word RMW, only low word changes | **Flip** | Real AMOSWAP.D: full 64-bit swap, full-width `rd` | C2, C3, §11 item 1 | `dispatch_matrix_pins_all_32_funct5_values` (row `00001`/D), `amoswap_encoding_executes_amoadd_arithmetic` |
| `direct_htif_dword_uses_start_address_only_for_base_through_interior` — typed `SystemBus` dword start-address HTIF routing with callback | every starting address `base..base+7` reads 0 and callbacks | **Keep** | Typed-helper surface unchanged; it becomes the labeled non-conforming compatibility adapter, and the Hart-level endpoint outcomes flip separately (C21) | C21, C24 | typed-helper behavior remains observable in `htif_*` rows (the callbacks fired by aligned dword writes) |
| `ram_first_full_span_and_htif_fallback_are_distinct`, `uart_rejects_wide_accesses_without_mmio_side_effects_and_keeps_byte_window` | RAM-first full-span routing; UART wide-access rejection with no side effects | **Keep** | Unchanged (ordinary raw path, C22) | C22 | not re-pinned; covered by the unchanged A7 fixture |
| `guest_aligned_access_to_misaligned_flat_offset_is_an_access_fault`, `core_below_base_fetch_is_rejected_before_backend_access`, `flat_tohost_validation_rejects_below_base_overflow_and_misaligned_offsets`, `failed_flat_image_load_retains_the_previous_image_and_runtime_artifact`, `host_write_mem_keeps_empty_offset_and_prefix_on_failure_semantics`, `host_write_mem_overflow_probe_is_bounded` | ordinary-path and host-write boundary behavior (non-atomic context rows) | **Keep** | Unchanged; the atomic-relevant change is C20's reservation effect, not these mechanics | C20 (writer semantics context), C28 | `ordinary_load_context_row_still_retires` (guard), `lr_and_sc_amo_class_faults_keep_their_cause_mapping` (Hart precheck context) |
| `global_reservation_characterization_isolated` — the isolated-child harness itself | four identical transcripts from fresh processes | **Retire (mechanics)** | Per-Hart reservation state is asserted by in-process tests (§9); the process-isolation harness is retained only if a T3 test still needs it (§9's explicit criterion, applied by T3) | C16, §9 | global-scope old column re-pinned in-process by `global_reservation_is_shared_across_cores_and_survives_reset`, `global_reservation_survives_flat_image_replacement`, `host_write_mem_does_not_invalidate_the_global_reservation` |

### 1.2 Isolated-child transcript rows

| Transcript row | Old verified expectation | Class | New expected value (post-A8) | Approving row | T0 baseline |
| --- | --- | --- | --- | --- | --- |
| `amoadd.w=retired/read_word+write_word` | `00001`/`010` retires AMOADD arithmetic through word helpers | **Flip** | AMOSWAP.W semantics (memory = `rs2` low word) | C2 | `amoswap_encoding_executes_amoadd_arithmetic`, dispatch-matrix row `00001` |
| `sc.no-reservation=retired/rd=1/no-write` | SC with no reservation retires `rd=1`, no write | **Relabel (typed compatibility only)** | Same only on the explicitly non-conforming `RiscvCore::new` typed route; it has no physical target permission/capability check. The standard physical route follows the SC correction addendum below: valid RAM conditionally fails after target validation; invalid/unsupported targets fault. | C13, §11 item 8 | dispatch-matrix rows `00010`/`00011` exercise the typed adapter; `a8_hart_atomic` and public-facade regressions exercise the standard route |
| `lr->other-core-sc=success/global` | LR in one `RiscvCore`, SC in another at the same address succeeds | **Flip** | Each Hart owns its reservation: the second core's SC fails `rd=1` | C16, §11 item 2 | `global_reservation_is_shared_across_cores_and_survives_reset` (old column) |
| `lr->reset->sc=success/reservation-retained` | `RiscvCore::reset` does not clear the reservation | **Flip** | Reset installs a fresh `CoreState`, so the post-reset SC fails `rd=1` | C16 | `global_reservation_is_shared_across_cores_and_survives_reset` (old column) |
| `lr@b0->sc@b8=retired/rd=1` | SC at a different exact address fails | **Keep** | Same observable: the SC span is not contained in the reservation span | C30, §9 | `global_reservation_is_shared_across_cores_and_survives_reset` (exact-key row), `cross_width_sc_rows_pin_the_width_less_exact_address_key` |
| `lr->sd->sc=success/no-invalidate` | overlapping scalar store does not invalidate; SC overwrites | **Flip** | Overlapping committed write ⇒ SC fails `rd=1`; non-overlapping write ⇒ SC succeeds | C17, §11 item 3 | A7 fixture itself is the old column; T3 asserts the flip (the baseline file does not duplicate the store-interleave rows) |
| `lr->fsd->sc=success/no-invalidate` | overlapping FP store does not invalidate | **Flip** | Same as the scalar-store row | C17, §11 item 3 | A7 fixture itself (as above) |
| `lr->host-write_mem->sc=success/no-invalidate` | host `write_mem` does not invalidate | **Flip** | Approved P2a-precise: committed overlapping host write ⇒ `rd=1`; committed prefix invalidates iff its bytes overlap | C20, §11 item 3 | `host_write_mem_does_not_invalidate_the_global_reservation` (old column) |
| `lr->failed-sd->sc=success/reservation-retained` | rejected ordinary write leaves the reservation intact | **Keep** | Same: no commit → no invalidation | C19, §9 | `write_faulting_sc_retains_the_reservation_for_a_later_sc` (retain principle, write-fault variant) |
| `sc.write-fault=trap/store-fault/clear-deferred` | write-faulting SC traps and *accidentally* retains (early return before clear); a later SC succeeds | **Relabel** | Same observable (store access fault, retain, later SC succeeds); the accident becomes the approved faulting-SC retain profile rule | C15, §11 item 2 | `write_faulting_sc_retains_the_reservation_for_a_later_sc` |
| `sc.w@htif+4=retired/dword-callback` | `SC.W` at `HTIF+4` with a live reservation retires `rd=0` through the dword callback | **Retire** | Replaced by D-c HTIF tests: LR/SC at the endpoint are target rejections → load/store-AMO access fault with no callback; AMO is one envelope with callback-exactly-once | C21, §11 item 4 | `htif_sc_family_dword_callback_needs_alignment_and_live_reservation` (SC.W@base+4 old column) |

## 2. `tests/a7_legacy_atomic_compat.rs`

| Fixture row | Old verified expectation | Class | New expected value (post-A8) | Approving row | T0 baseline |
| --- | --- | --- | --- | --- | --- |
| `ordinary_store_and_fp_store_interleave_with_unchanged_legacy_lr_sc` | legacy LR/SC interleaved with raw scalar/FP stores: SC still succeeds and overwrites | **Flip** | Same-domain committed overlapping write invalidates: SC fails `rd=1` (integer and FP variants) | C17, §11 item 3 | A7 fixture is the old column; T3 asserts the flip |
| `ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain` — visibility sub-row (raw store visible to the legacy domain; only the ordinary store uses the raw data port) | one migrated domain, both directions | **Keep** | Unchanged domain-sharing property (route expectations adapt to the envelope at T4, same observable) | §9 (keep list), C25 | `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection` remains the visibility fixture; ordinary/raw separation guarded by `lr_and_sc_amo_class_faults_keep_their_cause_mapping` |
| same test — AMO arithmetic sub-row (`amoadd_d` retires `rd = old = 7`, memory `7 + 3 = 10`) | `funct5=00001`/D executes AMOADD.D arithmetic via word helpers | **Flip** | `funct5=00001`/D executes AMOSWAP.D: `rd = old = 7`, memory = `rs2 = 3` (subsequent LR reads 3) | C2, C3 | `amoswap_encoding_executes_amoadd_arithmetic`, dispatch-matrix row `00001`/D |
| `rejected_ordinary_write_and_faulting_sc_preserve_characterized_reservation` — rejected-write sub-row | faulted ordinary store performs no write and keeps the reservation; later SC succeeds | **Keep** | Same (no commit → no invalidation) | C19 | `write_faulting_sc_retains_the_reservation_for_a_later_sc` (retain principle) |
| same test — faulting-SC sub-row (`sc` traps, later facade SC succeeds) | early-return accident retains the key | **Relabel** | Same observable; retain is the approved profile rule, not an accident | C15, §11 item 2 | `write_faulting_sc_retains_the_reservation_for_a_later_sc` |
| `legacy_lr_survives_reset_and_replacement_storage_with_global_key_behavior` | post-reset SC and second-storage SC both succeed against the global key | **Flip** | Reset/replacement clear the per-Hart reservation: both SCs fail `rd=1` | C16 | `global_reservation_is_shared_across_cores_and_survives_reset` (old column) |
| `public_flat_reload_replaces_storage_but_retains_legacy_lr_key` | flat reload replaces core+RAM; SC at the same guest address still succeeds | **Flip** | Reload constructs a fresh core ⇒ reservation cleared ⇒ `rd=1` | C16 | `global_reservation_survives_flat_image_replacement` (old column) |
| `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection` | legacy typed writes reach raw fetch, raw load, signature dump, and host inspection | **Keep** | Unchanged one-domain visibility | §9 (keep list) | A7 fixture remains the fixture; `ordinary_load_context_row_still_retires` guards the ordinary path |
| `successful_legacy_sc_width_debt_mmio_callback_is_retained` (`legacy_mmio_scenario`) | `SC.W@HTIF+4` with a live reservation retires `rd=0` and fires the dword callback | **Retire** | Replaced by D-c HTIF tests (LR/SC access fault, no callback; AMO callback-exactly-once) | C21, §11 item 4, §9 | `htif_sc_family_dword_callback_needs_alignment_and_live_reservation` (old column) |
| `legacy_lock_reentry_child` | store+LR+SC complete under one held domain lock without deadlock | **Keep** | Non-re-entrancy preserved on the envelope path (observable unchanged; harness re-targeted at T3) | §9 (keep list), §7.2 | not re-pinned; completion is re-proven by every stepping test in the baseline (each holds the data lock across typed helper calls) |

## 3. `tests/amo_test.rs` and in-crate atomic helper tests

| Fixture row | Old verified expectation | Class | New expected value (post-A8) | Approving row | T0 baseline |
| --- | --- | --- | --- | --- | --- |
| all `tests/amo_test.rs` tests (31) | direct helper calls (`exec_amoadd`, `exec_amoand`, …) keep their documented word-width semantics and results | **Keep** | Helpers keep their signatures and semantics as the labeled typed compatibility adapter (C24); dispatch-level flips do not change helper-level contracts | C24, §11 item 6 | helper semantics are the arithmetic used by the baseline's retire rows; no separate re-pin needed |
| `src/isa/rv64a/amo.rs` `#[cfg(test)]` module tests | same helper-level semantics in-crate | **Keep** | Same as above | C24 | same as above |
| `src/isa/rv64a/lr_sc.rs` `#[cfg(test)]` module tests (e.g. `test_lr_creates_reservation`, `test_sc_fail_after_conflict`) | helper-level LR/SC semantics against `GLOBAL_RESERVATION` / `clear_reservation` | **Keep (semantics) / migrate (surface)** | Helper-level behavior assertions are kept, but the process-global surface is removed (C23): these tests migrate to a fresh core or per-core helper at T3 per §11 item 5 | C23, §11 item 5 | reservation semantics old column pinned by the baseline's global rows and `cross_width_sc_rows_pin_the_width_less_exact_address_key` |

## 4. T0 new rows — behaviors with no A7 fixture (§9 "new coverage")

The A7 fixtures cover only the buggy `LR.W` dword read, `SC.W@base+4`, the
cross-core/reset/reload/global-key mechanics, and helper-level arithmetic.
Everything below was uncovered; each row is now pinned by the executable
baseline so the T3 flip is a diff against a running assertion.

| # | Verified old behavior (no A7 fixture) | Approving row | Baseline test |
| --- | --- | --- | --- |
| N1 | All 32 `funct5` values × W/D: dispatch outcomes, typed call sequence, `rd`, memory word, and pc for each | C1, C2, C4, C5, C6, C13 | `dispatch_matrix_pins_all_32_funct5_values` |
| N2 | Spec AMOMINU (`11000`) and AMOMAXU (`11100`) are unreachable: simulator failure (`UnsupportedLegalInstruction`) before any access; the repo encodings `01001`/`01011` execute AMOMINU/AMOMAXU instead | C4, C6 | dispatch matrix rows + `htif_unsupported_funct5_fails_before_any_access` |
| N3 | Spec AMOOR (`01000`) executes the AMOMIN helper; `00110`/`00111`/`01001`/`01010`/`01011` execute AMOOR/AMOAND/AMOMINU/AMOMAX/AMOMAXU; signed/unsigned helper identity proven by a two-pair discriminator | C5, C6 | `min_max_helper_mistargets_are_discriminated_by_two_operand_pairs` |
| N4 | AMOSWAP is missing: the real AMOSWAP encoding (`00001`) executes AMOADD arithmetic (memory `old+rs2`, never `rs2`) | C2 | `amoswap_encoding_executes_amoadd_arithmetic` |
| N5 | Width reversal with a live reservation: real SC.W writes a full dword; real SC.D writes a dword (already correct width via the same fallback); malformed `00010`+`rs2!=0` executes SC with reversed widths (`010`→dword, `011`→word) | C9, C10, C11 | `sc_width_reversal_on_ram_pins_dword_and_word_helpers` |
| N6 | C30 old column: `LR.W@p→SC.D@p` succeeds (`rd=0`); `LR.D@p→SC.W@(p+4)` fails (`rd=1`) under the width-less exact-address key | C30, §11 item 2 | `cross_width_sc_rows_pin_the_width_less_exact_address_key` |
| N7 | Faulting-SC retain observable in isolation: a write-faulting SC traps (cause 7, original `mtval`) and a later SC at the same address still succeeds | C15, §11 item 2 | `write_faulting_sc_retains_the_reservation_for_a_later_sc` |
| N8 | §3.4 HTIF: `LR.W` at 4-aligned endpoint addresses (`base`, `base+4`) retires via the buggy dword typed read, returns 0, fires no callback, and sets the reservation proven by a following aligned `SC.W` success | C21 | `htif_lr_w_retires_via_dword_read_bug_and_reserves` |
| N9 | §3.4 HTIF: real `LR.D@base` width-8 precheck passes, then the typed `read_word` has no HTIF branch → load access fault (cause 5) with the original guest address, and establishes no reservation (following `SC.W` fails `rd=1`, no callback) | C21 | `htif_real_lr_d_at_base_load_access_fault_and_no_reservation` |
| N10 | §3.4 HTIF: interior-offset doubleword encodings trap encoded misalignment before any access — `LR.D@base+4` → load-address-misaligned; `SC.D@base+4` → store-address-misaligned even with a live reservation at `base+4`; zero typed calls, no callback | C21, C26 | `htif_interior_doubleword_encodings_trap_before_any_access` |
| N11 | §3.4 HTIF: `SC.W@base` and `SC.D@base` with a live reservation retire `rd=0` through the dword callback (real `SC.D` at the endpoint was never covered); no-reservation `SC.W@base` retires `rd=1` with zero typed calls and no callback | C21, C13 | `htif_sc_family_dword_callback_needs_alignment_and_live_reservation` |
| N12 | §3.4 HTIF: dispatched AMO helper (`AMOADD.W@base`) performs the typed word read → `InvalidAddress` → store/AMO access fault (cause 7), no callback; malformed `00010`+`rs2!=0`/`011` attempts the reversed 32-bit write → cause 7; malformed `…`/`010` mirrors `SC.W@base` (dword callback success) | C21, C11 | `htif_dispatched_amo_helpers_fault_store_access_cause_7` |
| N13 | §3.4 HTIF: unsupported `funct5` AMOs (`00000`, `01100`, `10000`, `10100`, `11000`, `11100`) are simulator failures before any access at the endpoint (zero typed calls, no callback, pc unchanged) | C21, C1, C4 | `htif_unsupported_funct5_fails_before_any_access` |
| N14 | Hart precheck classes are load/store-split and precede any access: LR → load-address-misaligned, SC/AMO → store-address-misaligned at the same misaligned guest address; LR's backend fault stays load access fault (cause 5) | C26 | `lr_and_sc_amo_class_faults_keep_their_cause_mapping` |
| N15 | AMO encodings with `funct3` outside {010, 011} are illegal-instruction traps before any access (unchanged Hart rule the decode repair preserves) | §5.3 | `amo_funct3_outside_w_d_is_an_illegal_instruction_trap` |

## 5. Spike reference-evidence rows (documentation only)

These rows are **not tests of this repository** and certify nothing. They
record the reference implementation's observable behavior at the pinned
investigation commit — `riscv-isa-sim` (Spike) @
`02b1dc182164bb73b19b050676dd89f0834f8b2e` (`riscv/mmu.h`, `riscv/mmu.cc`,
`riscv/sim.cc`, `riscv/simif.h`), as recorded in the contract's §2 reference
investigation, §5.4, and §5.6 — and the contract rows they anchor. Spike is
an implementation reference, never specification.

| # | Spike observable (pinned `02b1dc1`) | Anchor |
| --- | --- | --- |
| S1 | A *trapping* SC retains the reservation: `yield_load_reservation` is reached only after a completed SC (and at MMU construction and the hart-interleave switch, `riscv/sim.cc:375`), so an SC that raises before completion consumes nothing | C15, §11 item 2 (faulting-SC retain) |
| S2 | Spike performs no same-hart write invalidation; the specification imposes no same-Hart requirement either | C17 (this contract's same-Hart invalidation is a deliberate, recorded divergence, stricter than both) |
| S3 | The reservation key is a single exact `paddr` equality with no recorded width (`riscv/mmu.h:286`), so `LR.W→SC.D` at the same address succeeds — matching this repository's current exact-address key | C30 (old column parity; span-containment remains a deliberate stricter deviation, legal under `lr_reservation_set_size`) |
| S4 | `reservable()` is `addr_to_mem` — RAM only: LR to an MMIO address raises a load access fault (`riscv/mmu.cc:247`) and SC to MMIO a store access fault (`riscv/mmu.h:288`) | C21/§11 item 4 (D-c surface: LR/SC rejected at the HTIF endpoint) |
| S5 | AMOs reach MMIO: Spike implements them as a load followed by a store, and the pinned Zaamo NOTE states that using AMOs to update MMIO registers is an intended use | C21/§11 item 4 (D-c adopts Spike's policy surface — AMO allowed — but not its two-step mechanism: this contract's AMO is one indivisible envelope per ADR-0002 §5.5) |

No differential/oracle run against Spike was executed or claimed for T0; any
new differential selection remains a separate explicit proposal (§8, §10).

## 6. Classification statistics and baseline coverage matrix

### 6.1 A7 atomic fixture rows (ledger sections 1–3)

| Class | Count | Rows |
| --- | --- | --- |
| Keep | 12 | `sc.no-reservation`, `lr@b0->sc@b8`, `lr->failed-sd->sc`, direct typed HTIF dword, RAM-first/UART/non-atomic context group (counted once), `legacy_write_is_visible…`, `legacy_lock_reentry_child`, `amo_test.rs` (helper suite), `amo.rs` unit tests, LR/SC unit-test semantics half, `ordinary_store_then_legacy_amo…` visibility half, rejected-write sub-row |
| Flip | 12 | `opcode_funct5…` W and D sub-cases, `amoadd.w`, `lr->other-core-sc`, `lr->reset->sc`, `lr->sd->sc`, `lr->fsd->sc`, `lr->host-write_mem->sc`, `ordinary_store_and_fp_store…`, `legacy_lr_survives_reset…`, `public_flat_reload…`, AMO arithmetic sub-row of `ordinary_store_then_legacy_amo…` |
| Retire | 3 | `sc.w@htif+4` transcript row, `successful_legacy_sc_width_debt_mmio_callback_is_retained`, isolated-child global-singleton mechanics (harness re-evaluated at T3 per §9) |
| Relabel | 2 | `sc.write-fault` (accident → approved retain), faulting-SC sub-row of `rejected_ordinary_write_and_faulting_sc…` |

No row is unclassified; dual rows (visibility/arithmetic, rejected-write/
faulting-SC, unit-test semantics/surface) are split into the sub-rows above
rather than left pending.

### 6.2 T0 executable baseline coverage matrix (`tests/a8_atomic_baseline.rs`, 18 tests)

| Contract area | Baseline tests |
| --- | --- |
| `funct5` dispatch table (all 32 values × W/D) | `dispatch_matrix_pins_all_32_funct5_values` |
| Wrong-op targets and AMOSWAP absence | `min_max_helper_mistargets_are_discriminated_by_two_operand_pairs`, `amoswap_encoding_executes_amoadd_arithmetic` |
| SC/LR width reversal (C9/C10/C11) | `sc_width_reversal_on_ram_pins_dword_and_word_helpers` |
| C30 cross-width key | `cross_width_sc_rows_pin_the_width_less_exact_address_key` |
| `GLOBAL_RESERVATION` global scope (C16/C20 old column) | `global_reservation_is_shared_across_cores_and_survives_reset`, `global_reservation_survives_flat_image_replacement`, `host_write_mem_does_not_invalidate_the_global_reservation` |
| Faulting-SC retain (C15 old observable) | `write_faulting_sc_retains_the_reservation_for_a_later_sc` |
| §3.4 HTIF per-encoding Hart outcomes (C21 old column) | `htif_lr_w_retires_via_dword_read_bug_and_reserves`, `htif_real_lr_d_at_base_load_access_fault_and_no_reservation`, `htif_interior_doubleword_encodings_trap_before_any_access`, `htif_sc_family_dword_callback_needs_alignment_and_live_reservation`, `htif_dispatched_amo_helpers_fault_store_access_cause_7`, `htif_unsupported_funct5_fails_before_any_access` |
| Precheck classes and unchanged Hart rules (C26, §5.3) | `lr_and_sc_amo_class_faults_keep_their_cause_mapping`, `amo_funct3_outside_w_d_is_an_illegal_instruction_trap` |
| Ordinary-path guard | `ordinary_load_context_row_still_retires` |

### 6.3 T0 disposition

* Ledger complete: no pending classification (§6.1).
* Baseline executable and green on unchanged code; existing A7 fixtures and
  `amo_test.rs` show zero regression (exit command above).
* No `src/` change at T0: the baseline asserts the then-current behavior;
  later approved changes and their exact-head evidence are separate records.

## 7. Spec-first SC correction follow-up

This addendum supersedes the earlier C13 expectation for the standard
physical-port route only; it does not rewrite the pinned A7/T0 historical
observations above, and the typed `RiscvCore::new` route remains explicitly
non-conforming (no target permission/capability checks).

The maintainer-authorized correction follows the normative/implementation
separation in the current [`docs/dev-plan.md`](../dev-plan.md) §§2, 5, 6, 7,
8, and 11. Normative Zalrsc anchors require SC permission checks before
retirement, failure outside the reservation set, permit failed SC to be
checked as a store, leave translation side effects unspecified, and require
nonzero failure/no write. The chosen A8 order is an implementation decision:
Hart legality/alignment → guest-to-port conversion → one existing SC envelope
with optional reservation context → target store-span and atomic-capability
validation → conditional failure if valid RAM but context is absent/uncovered.
Spike @ `02b1dc1` and Sail @ `8890da7` are implementation references, not
specification. A8's public MMU/PMP remains unwired.

| Case | Previous standard-route result | Corrected standard-route result |
| --- | --- | --- |
| Valid mapped RAM, no reservation or live disjoint/uncovered reservation (W/D) | No-reservation SC skipped conversion and the target; uncovered SC converted then skipped target; both retired `rd=1` without a physical envelope. | One SC envelope validates the complete RAM span/capability, then returns Failure; `rd=1`, no guest-byte read/write or bookkeeping bump, and a live reservation is consumed. |
| Flat guest address below base or odd storage offset, no reservation | Skipped address conversion and retired `rd=1`. | Store/AMO access fault (cause 7), original guest address in `mtval`, zero target envelopes, no retirement/`rd` write. |
| Same flat conversion failure with a live reservation | Conversion failed before Hart coverage check; cause-7 fault, staged state retained reservation. | Same fault, original guest `mtval`, zero envelope, no retirement/`rd` write; reservation retained. |
| HTIF D-c, UART, or unmapped span, no reservation or live-uncovered SC | Hart returned conditional Failure before target request (`rd=1` retirement), so unsupported/map-invalid targets were not checked. | One envelope reaches target validation; target rejects with store/AMO access fault (cause 7), no retirement/`rd` write, callback, device mutation, or exit. A live reservation is retained on fault. |
| Covered SC target fault | Store access fault, no retirement/`rd` write; live reservation retained. | Same approved faulting-SC retain; the rule now also applies when the submitted context is uncovered and the target rejects before conditional status. |

Focused updated coverage is in `tests/a8_atomic_contract.rs`,
`tests/a8_atomic_targets.rs`, `tests/a8_hart_atomic.rs`,
`tests/a8_atomic_baseline.rs`, and `tests/a8_public_atomic_equivalence.rs`.
These follow-up assertions do not claim RV64A certification or public
MMU/PMP integration.
