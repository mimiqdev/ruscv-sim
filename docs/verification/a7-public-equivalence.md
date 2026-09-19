# A7 T4 public-facade equivalence

**Status:** T4 implementation and verification record

**Scope:** public CLI, `load_and_run`, and `RiscVSimulator` behavior at the
A7 physical-access migration boundary. This record is not an ISA-extension
support claim and does not replace the T0 characterization or T1--T3
component records.

**Fixture policy:** `tests/a7_public_equivalence.rs` creates fresh ELF bytes in
memory for every case. The principal workflow uses RAM base `0x8000_0000`, an
entry at `base + 0x100`, a trap handler at `base + 0x300`, a signature at
`base + 0x2000`, and an ELF-declared tohost at `base + 0x1000`. The CLI cases
execute the Cargo-provided `CARGO_BIN_EXE_ruscv-sim` path, never a guessed
stale `target/debug` binary.

## Coverage matrix

| Contract | Fixture and observable evidence | Public surfaces |
| --- | --- | --- |
| Shared placement and result facts | The workflow has a nonzero base and entry. Exact exit code, completed-turn count, final PC, signature address/bytes, and non-timeout result are asserted. The flat facade additionally checks PC, privilege, GPRs, CSR state, FPR state, and RAM bytes. | CLI process, `load_and_run`, `RiscVSimulator` |
| Ordinary integer memory | `SD` followed by `LD` at the nonzero guest address; the loaded value and final little-endian bytes are asserted. | All three workflow surfaces; raw non-atomic data port in the two standard facades |
| Ordinary FP memory | `FLW`/`FSW` and `FLD`/`FSD` use distinct widths. The ELF seeds asymmetric nonzero normal bit patterns `0x3fc01234` and `0x400c000000000001`, including both double halves, plus nonzero destination sentinels. Guest integer checks make the complete FP results part of the native/CLI exit oracle; flat state and bytes assert the exact values. A one-low-bit ELF mutation reaches the distinct failure exit/signature. | All three workflow surfaces; flat state and storage inspection |
| Retained legacy atomics | `AMOADD.W`, the characterized LR path, and SC use the same aligned address after ordinary integer accesses. Guest branches validate old-result registers, SC status, and final value before selecting success; the old typed backend is separately called with exact `read/write` widths, addresses, and order. | All three workflow surfaces; public `RiscvCore::new` with a third-party `MemoryInterface` |
| Trap, handler, and MRET | The workflow takes ECALL, advances MEPC in a real handler, records handler state, and returns with MRET. A separate image repeatedly traps on an illegal handler instruction. `cycles` counts completed trap turns while `minstret` excludes them. | CLI, `load_and_run`, `RiscVSimulator`; typed public core boundary |
| Budget and exit boundary | Zero budget starts no turn. One slot before the final exit store times out. The exact budget exits successfully, including when the exit store is in the final permitted slot. | CLI, `load_and_run`, `RiscVSimulator` |
| Host failure boundary | A reachable public flat memory handle is poisoned in a child thread. The one started slot reports a host failure with zero completed turns and no timeout; `minstret` and PC remain unchanged. | `RiscVSimulator` only; no unsafe backend injection was added |
| Unknown completion | A public `RiscvCore::new_with_physical_ports` test injects one unknown fetch completion. Repeated stepping returns the retained terminal failure without another backend call; only explicit `clear_unresolved_physical_access` permits the next call. | Public typed core injection seam; not the higher-level facades |
| Logging and fetch/commit distinction | Logging on/off produces the same result. Commit-log lines equal retired instructions, contain the final store, omit ECALL, and are produced by the already-returned retired fact. A fetch trace separately proves one fetch per started trap attempt and no trap retirement. | `load_and_run`, real CLI binary, public typed core trace |
| Signature artifacts | Absent, empty, readable, and unreadable declarations are checked. Native unreadable signatures are suppressed; flat unreadable signatures remain a reported artifact failure while the primary exit facts survive. | `load_and_run`, `RiscVSimulator` |
| Reload and image ownership | Rejected below-base replacement leaves old PC, metadata, memory, and image usable. Successful replacement resets state and binds the new raw and typed views to new shared storage; mixed ordinary/legacy guest accesses and post-run bytes prove the view is one image. | `RiscVSimulator` |
| Configuration and route differences | Native UART/fixed HTIF succeeds where flat RAM has no device. Native CLI override > ELF tohost > fixed default and flat post-load manual offset behavior are asserted independently. | `load_and_run`, `RiscVSimulator`; CLI device path is covered by the existing public-behavior suite |

The existing matrix in [`public-behavior-matrix.md`](public-behavior-matrix.md)
and the A4 run-control tests remain complementary evidence for HTIF-before-RAM
observer priority, decode-before-clear behavior, diagnostics, and the full
historical CLI/library compatibility inventory. T4 does not silently duplicate
those configuration-specific claims as cross-facade equivalence.

## Route audit

The audit was performed against the implementation at the final T4 HEAD.
The relevant call points are named rather than line-numbered so later formatting
changes do not make the record stale.

| Route | Actual implementation | Boundary retained intentionally |
| --- | --- | --- |
| Image installation | `src/executor.rs::install_image_with_physical_ports` creates the typed memory view and separate raw fetch/data ports over the same backing image, then resets the Hart with the address form's base. Native uses `SystemBus`; flat uses `NativeRamBackend` over flat RAM. | The two raw ports remain distinct handles even when they share storage. |
| Fetch | `RiscvCore::step_outcome` uses `instruction_access` for one raw fetch when installed; `RiscvCore::new` without ports keeps the typed instruction-memory compatibility route. | An old caller is not forced to implement `PhysicalBackend`. |
| Ordinary guest data | `uses_non_atomic_access` routes `Load`, `Store`, `LoadFp`, and `StoreFp` through `data_access` and the checked physical adapter. Alignment and target failures are classified before architectural commit. | The guest ordinary path is migrated; host inspection is not treated as a guest transaction. |
| AMO/LR/SC | `uses_non_atomic_access` deliberately excludes `Opcode::Amo`; the executor receives `LegacyTypedMemoryAdapter` over `data_mem`. | T0's typed widths, global reservation behavior, exact-address key, and fault/reset defects are compatibility targets, not repaired by T4. |
| Native runner | `load_and_run` installs two `SystemBus` raw ports. After the Hart outcome, HTIF callback observation is traversed before the RAM tohost observer. A retired instruction is logged before observers run. | Native UART and fixed HTIF are devices, not flat RAM aliases. |
| Flat runner | `RiscVSimulator::load_elf` installs two raw RAM ports over one new `SimpleMemory`; `run` polls the selected flat offset after each completed turn. | `read_mem`, `write_mem`, `set_tohost`, tohost clear, and artifact reads remain typed host APIs. |
| Host artifact/clear calls | Native `dump_signature` and `clear_tohost` use the typed bus/memory interface; flat artifact and signal helpers use the typed flat handle. | These calls are observation or host mutation, not evidence that ordinary guest accesses bypass raw ports. |
| Compatibility injection | `RiscvCore::new_with_physical_ports` is the existing public test/integration seam. The T4 unknown-completion test uses it and calls `clear_unresolved_physical_access` only as an explicit host resolution step. | No new public fault injector or backend setter was added solely for testing. |

### Legacy bridge exceptions

The following typed calls are expected and are not migration failures:

1. AMO/LR/SC guest operations, including their exact legacy reservation and
   width behavior.
2. `RiscvCore::new` callers that do not install physical ports.
3. Native/flat host-side tohost polling and clearing, signature reads, and the
   public flat `read_mem`/`write_mem` inspection/mutation APIs.
4. The existing typed `SystemBus` compatibility methods. The raw SystemBus
   adapter has stricter full-span routing; it does not retroactively change the
   old typed start-address behavior.

The custom backend test implements only `MemoryInterface`, uses independent
instruction/data `Arc<Mutex<...>>` handles, and verifies actual calls. It is
therefore compile/API compatibility evidence, not just a test double that
implements the new raw interface.

## Evidence boundaries and limitations

* An unknown physical completion cannot be injected through `load_and_run` or
  `RiscVSimulator` without exposing a new backend-injection API. The narrow
  reachable evidence is the existing public `RiscvCore::new_with_physical_ports`
  seam; the higher-level APIs are not claimed to expose recovery controls.
* A host-failure injection at the flat public boundary is reachable only by
  poisoning the already-public memory mutex. Native `load_and_run` has no
  equivalent safe public fault injection, so no native host-failure result is
  claimed from T4.
* The workflow intentionally uses the characterized LR/SC encodings and
  aligned addresses. T4 preserves, but does not upgrade, the T0/T3 legacy
  atomic semantics. The in-process workflow cases hold one test-only mutex
  across their complete runs because the retained reservation is process-global.
  See [`a7-migration-characterization.md`](a7-migration-characterization.md)
  and [`a7-hart-physical.md`](a7-hart-physical.md).
* Equality is asserted only for shared architectural/result facts. UART,
  fixed HTIF, flat offsets, artifact diagnostics, and native-vs-flat failure
  policy remain explicit configuration differences.
* This is Rust/public-facade evidence. It does not claim MMU/PMP/page-fault
  integration, asynchronous interrupt delivery, ACT4/Sail equivalence, or a
  future platform/result taxonomy.

## Verification commands

The focused T4 command is:

```bash
cargo test --test a7_public_equivalence
```

The retained T0--T3 and A6 regression set is:

```bash
cargo test --test a7_migration_characterization \
  --test a7_physical_contract \
  --test a7_native_targets \
  --test a7_hart_physical \
  --test a7_legacy_atomic_compat \
  --test a4_integrated_equivalence \
  --test a4_run_control \
  --test a6_task3_core_trap_test \
  --test a6_trap_elf_integration
```

The repository quality gate remains:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

The final evidence record must include the exact committed HEAD, focused and
regression counts, any unavailable cross-toolchain/ACT4 checks, and
`git diff --check a75f8b290a3c359793f1f834476b44c2a485f952..HEAD`. Check IDs and
validation status are supplied by the CodingTask handoff system; prose or a
command copied from this document is not a substitute for that captured
validation contract.

## T4/T5 handoff checklist

The completed T4 audit rows below are bound to the code/test evidence revision
`88b2c7f80236ead76c6b2b47b0af3a56f7404b86`; the final documentation and quality
checks are recorded against the later exact HEAD by the CodingTask handoff.

| Status | Audit item | Evidence and path | Boundary or next action |
| --- | --- | --- | --- |
| Resolved | Reservation-sensitive workflow scheduling | `tests/a7_public_equivalence.rs::workflow_reservation_guard`; the three workflow tests hold one mutex across all simulator/CLI runs. | Test-only serialization preserves the approved process-global legacy key; production semantics unchanged. |
| Resolved | Shared integer/atomic/FP result oracle | `workflow_fixture`, `double_low_half_mutation_reaches_the_shared_failure_oracle`, and the public workflow test. Native/library/CLI exit and signature results depend on checks; flat state/bytes provide intermediate oracles. | The mutation changes only fixture bytes and must select failure code/signature; no production fault injection is used. |
| Resolved | Final-slot/retirement/logging/signature/reload evidence | `final_slot_host_failure...`, `public_fetch_trace...`, workflow commit-log assertions, `signature_artifacts...`, and `reload_and_configuration...`; complementary A4/A6/public-behavior tests are linked above. | Native/flat observer differences remain explicit; retired stores are logged before exit observation. |
| Resolved with bounded seam | Unknown completion and host failure | Public `RiscvCore::new_with_physical_ports` proves terminal unknown/no-retry and explicit clear; flat `RiscVSimulator::memory()` poisoning proves final-slot host failure. | Higher-level native host-failure/unknown injection has no safe public API and is not claimed; adding a dangerous injector is out of scope. |
| Resolved | Standard route audit and legacy exceptions | This record's route-audit table plus `src/core/mod.rs::step_outcome`, `src/executor.rs::install_image_with_physical_ports`, `tests/a7_hart_physical.rs::real_fetch_integer_and_fp_accesses_use_raw_ports_and_one_ram`, `tests/a7_legacy_atomic_compat.rs::ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain`, `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection`, and `legacy_lock_reentry_child`. | Ordinary standard fetch/integer/FP accesses use raw ports; shared RAM handles and lock domains are asserted. AMO/LR/SC, old constructor, host inspection, typed SystemBus methods, and the exact legacy reservation address key remain explicit exceptions. |
| Approved deferred debt | Legacy atomic width/global-reservation/fault/reset behavior | `docs/verification/a7-migration-characterization.md`, `docs/verification/a7-hart-physical.md`, `tests/a7_legacy_atomic_compat.rs`. | This is preserved T0/T3 compatibility evidence, not a T4 failure or an A7 completion claim. |
| Evidence insufficient / deferred | Native high-level injectable host-failure and unknown-completion cases | No safe public injection route exists; the narrow core/flat seams are recorded above. | Do not infer full facade failure coverage. Revisit only with an approved API/scope decision. |
| Deferred to T5 | Fresh project-authored and pinned ACT4 suites | `docs/dev-plan.md` §7 and the commands below; no fresh T5 run is claimed here. | Run both distinct 51-case suites at the final frozen reviewed HEAD with selection/configuration/artifact hashes. |
| Deferred to T5 | Performance observation | No runtime code changed in T4 and no benchmark comparison was run. | If useful, compare existing benchmarks before/after at T5; no no-overhead claim or new gate is introduced. |

## T5 handoff

T4 does not claim the two separately scoped 51-case suites or A7 completion. T5
must rerun the 51 project-authored ELF guests and the separately pinned 51
nontrapping ACT4 RV64I cases at the final reviewed head, retaining fresh
selection/configuration and artifact hashes. The historical ACT4 replay remains
historical evidence; it is not a fresh T4 pass. T5 also owns any optional
lightweight benchmark comparison and the final residual-debt inventory.
