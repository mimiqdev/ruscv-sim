# Public Behavior Defect and Gap Register

**Status:** Current evidence record

**Authority:** Informational; this register supports the [A1 public behavior matrix](public-behavior-matrix.md) and does not authorize production repair

**Last reviewed:** 2026-09-11; individual verification runs retain their recorded scope

**Committed evidence snapshot:** `b16a7872cf62e51dc204d9ea122b6c5223c15f96`
(first submitted PR14 test snapshot)

**Merged evidence:** the zero-fill and G-02 harness corrections were verified
on a working tree based on that snapshot, then committed as
`54bfdbf11491caf7e78565ddf62cc1e7e13c4e67` and merged in
`fae4759e09c6750e4105c8be7c287e841dabf0dc`. This is a finite register for the
first A1 inventory and second tests/reproductions batch, not uncommitted work.
The [A1 acceptance assessment](a1-acceptance-assessment.md) records fresh
2026-09-08 post-merge checks and links to accepted dispositions; historical observations
below retain their original verification scope. “Reproduced defect” is used only where a bounded run observed the
behavior. A source observation, contract comparison, or hypothesis is labelled
separately. No entry is a compatibility promise, a production-fix commitment, or
a second active plan.

## Disposition vocabulary

| Disposition | Meaning in this register |
| --- | --- |
| **Reproduced defect** | A bounded command or API run observed the result; the reproduction and observed output are recorded. |
| **Source-observed gap** | The implementation and contract/test comparison identifies a gap, but this batch did not run an end-to-end reproduction. |
| **Unverified** | Evidence is missing or a required dependency was unavailable; neither support nor a defect is asserted. |
| **Documentation discrepancy** | Non-authoritative documentation conflicts with source or test evidence; it is not a runtime compatibility claim. |

## Register

### G-01 — Flat-library ELF tohost address is not adapted to relative memory

- **Disposition:** **Repaired** in A2 T2. Historical A1 status was **Reproduced defect**.
- **Surface:** `RiscVSimulator::load_elf` / `run` versus CLI `load_and_run`.
- **Contract context:** [ADR-0003 §9](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product) keeps `RiscVSimulator` available and requires the CLI and flat-library configurations to remain explicit; it does not bless a wrapper that silently misses its guest exit.
- **Implementation evidence:** The A1 defect was that `load_elf` loaded the `LoadedElf.memory` buffer relative to `base_addr`, reset the core with `base_addr`, and stored `loaded.tohost` without converting it to a flat offset, so `run` polled `SimpleMemory` at the guest address. `load_elf` now converts declared tohost metadata with checked base subtraction and rejects a tohost the flat image cannot represent. `run` polls the resulting flat offset.
- **Reproduction:** The same transient ELF with entry/base `0x30000000` and `.tohost = 0x30001000` exited through the CLI at cycle 7. The library harness returned:

  ```text
  library entry=0x30000000 ... result={exit=1, cycles=20, timeout=true, error=Some("Timeout after 20 cycles")}
  flat_offset_exit={exit=0, cycles=3, timeout=false, error=None}
  ```

  The A1 persistent contrast test at base `0x80000000` observed `exit_code=1`, `cycles=20`, `timed_out=true`, `Timeout after 20 cycles` while the CLI exited at cycle 4. It is now `public_behavior::flat_library_elf_tohost_metadata_selects_the_ram_exit_signal`, which keeps the CLI contrast and requires the library to observe the same declared exit at cycle 4. The A1 manually configured flat-offset case still works through an explicit `set_tohost` flat offset.
- **Existing tests:** `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal`, `flat_library_reports_zero_and_nonzero_guest_exits`, `flat_library_reports_base_zero_placement`, `flat_library_manual_flat_tohost_overrides_image_metadata`, `flat_library_manual_tohost_before_load_is_superseded_by_image_metadata`, `flat_library_manual_tohost_survives_a_load_without_metadata`, `flat_library_image_without_metadata_does_not_reuse_the_previous_tohost`, and `flat_library_rejects_a_declared_tohost_the_flat_image_cannot_represent` are **strong** for correspondence, exit selection, precedence and rejected placements. Against them, `test_simulator_creation`, `test_simulator_setters`, `test_simulator_run_with_max_cycles`, and `test_simulator_run_default_cycles` remain **weak** because they do not assert an image exit.
- **Impact:** The wrapper now observes an image's declared RAM exit at zero and nonzero bases, and the accepted `set_tohost` precedence is documented on `RiscVSimulator`. This repair is T2 only; artifacts and integrated acceptance remain open.
- **Selected successor scope:** [A2](../archive/milestones/a2-closeout-record.md) T2 repaired this placement/completion gap; A2 is complete with documented limitations.

### G-02 — Flat-library `read_mem` can loop indefinitely on an out-of-range aligned read

- **Disposition:** **Repaired** in A2 T1. Historical A1 status was **Reproduced defect**.
- **Surface:** `RiscVSimulator::read_mem`.
- **Implementation evidence:** The A1 defect was that an aligned dword/word/half read error could reach the next loop iteration without advancing `current_addr` or `offset`, so an address outside `SimpleMemory` had no progress condition. `read_mem` now rejects empty-after-overflow and out-of-range nonempty ranges through `ExecutorError`, reads in-range bytes in address order, and returns an empty vector for `size == 0` without a memory access.
- **Reproduction:** A1 reproduced `RiscVSimulator::new(0x1000).read_mem(0x2000, 4)` hanging: a temporary harness printed `before` and returned status `124` under `timeout 2s`. The persistent child harness still constructs the simulator, writes a ready marker, then performs that exact read. After T1 the child must receive `Err` and exit successfully inside the hang window; a hang regression is still killed and reaped. Companion tests keep missing-ready and early-exit cleanup coverage.
- **Existing tests:** `public_behavior::out_of_range_flat_read_mem_returns_error_without_hanging` is **strong** for the original request returning an error without hanging. Handshake-failure and early-exit tests remain **strong** for harness cleanup. `flat_read_mem_returns_exact_bytes_for_aligned_unaligned_and_mixed_lengths`, `flat_read_mem_empty_requests_do_not_access_memory`, `flat_read_mem_rejects_out_of_range_crossing_and_overflow_without_wrapping`, and `flat_read_mem_does_not_execute_or_mutate_after_guest_step` are **strong** for exact bytes, empty/overflow/crossing errors, and state preservation. `test_simulator_read_mem_unaligned` now asserts exact unaligned bytes; `test_simulator_read_mem_empty_and_out_of_range` covers empty and error returns.
- **Impact:** The public inspection helper no longer hangs on the reproduced out-of-range aligned read. Closing G-02 does not complete A2; load/run/result/artifact work remains.
- **Selected successor scope:** [A2](../archive/milestones/a2-closeout-record.md) T1 repaired this inspection gap; A2 is complete with documented limitations.

### G-03 — Public commit log loses opcodes for nonzero ELF bases

- **Disposition:** **Reproduced defect**.
- **Surface:** `--log-commits` on the CLI/public ELF path.
- **Implementation evidence:** Before `core.step`, `load_and_run` re-fetches the opcode with `pc_before.wrapping_sub(base_addr)`. The CLI core is reset with base 0 and `SystemBus` routes RAM at the ELF base, so this diagnostic read is outside the bus RAM window for a nonzero base; `.unwrap_or(0)` then logs opcode zero.
- **Reproduction:** The same three-instruction transient program produced correct opcodes at base 0, but at base `0x80000000` produced:

  ```text
  core   0: 3 0x0000000080000000 (0x00000000) x5  0x0000000000000001
  core   0: 3 0x0000000080000004 (0x00000000) x4  0x0000000040008000
  core   0: 3 0x0000000080000008 (0x00000000)
  ```

- **Existing tests:** Persistent `public_behavior::public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` is **strong**: it reads all four actual public lines and asserts the zero-opcode observation. The current `test_log_commit_path_with_logger` is **strong** for successful execution, line count, and PC/privilege shape, but deliberately avoids asserting the known-bad opcode; it therefore does not weaken or replace this reproduction. Formatter tests such as `test_log_commit_format` are **strong** for supplied arguments but do not exercise the public re-fetch.
- **Impact:** Nonzero-base commit logs are not reliable instruction evidence. The zero opcode output must not be treated as a compatibility format.
- **Next decision:** A later observation/logging scope must choose a fix or an explicit limitation. No source change is made here.

### G-04 — Native UART routing window exceeds the UART-declared range

- **Disposition:** **Source-observed gap; end-to-end boundary unverified**.
- **Surface:** Native `SystemBus` UART map versus `Uart16550`/TLM map.
- **Implementation evidence:** `SystemBus::new` sets `uart_size = 0x100` and `is_uart` accepts `0x10000000..0x10000100`. `Uart16550::UART_SIZE` is 8, `read_reg`/`write_reg` accept any offset at the direct Rust API, and the TLM target rejects addresses at or above base + 8.
- **Existing tests:** `test_system_bus_routing` covers offset 1; multi-byte access tests cover width rejection at the base; `test_uart_address_out_of_bounds` covers the TLM target at offset 8. There is no public-path assertion for native byte offsets `0x08..0xff`.
- **Impact:** The public native bus can route addresses that the declared UART/TLM range does not contain, with direct register operations becoming no-op/zero behavior rather than a consistent invalid-address result.
- **Next decision:** Define whether the native map is intentionally 0x100 or must match `UART_SIZE`; add one focused boundary test only after the intended contract is approved.

### G-05 — Verbose tohost diagnostic is misleading

- **Disposition:** **Reproduced defect**.
- **Surface:** CLI `--verbose` input diagnostics.
- **Implementation evidence:** `src/main.rs` prints `tohost.unwrap_or(0)` before loading the ELF; it does not print the eventual ELF-discovered or fixed default selection.
- **Reproduction:** On the transient ELF with an ELF `.tohost` at `0x30001000`, stderr began with:

  ```text
  Tohost address: 0x0000000000000000
  ...
  [DEBUG] Starting execution: ... tohost=0x0000000030001000
  ```

- **Existing tests:** `test_cli_run_verbose_flag` remains a **weak** no-panic check. The current `test_load_and_run_verbose_output` is **strong** for the bounded successful result, but it does not assert the diagnostic text and therefore does not remove this gap.
- **Impact:** The diagnostic can mislead users about the actual input/configuration; it does not change the observed run result.
- **Next decision:** Correct the diagnostic or document its meaning in a separately authorized change. No repair is made here.

### G-06 — Signature read errors are suppressed in run results

- **Disposition:** **Repaired for the flat-library path** in A2 T3; the CLI `load_and_run` path remains a **source-observed gap** with an unverified error path.
- **Surface:** `ExecutionResult.signature_data` after a signature read failure.
- **Implementation evidence:** All `load_and_run` result paths still call `dump_signature(...).ok().flatten()`, so a CLI read error becomes `None` while `signature_addr` may remain `Some`. `RiscVSimulator::signature_artifact` no longer suppresses: a declared region the flat image cannot represent leaves `signature_data` as `None` **and** appends an explicit diagnostic to `ExecutionResult.error`, without disturbing exit, cycles, final PC or a primary failure. The helper's byte-read arm is defensive parity: once the checked mapping passes, the range is inside the flat buffer, so the observed artifact failures are mapping failures rather than backend read failures. A2's non-goals exclude redesigning the CLI signature-failure policy, so that path is unchanged and still cannot be advertised as successful capture.
- **Existing tests:** For the flat path, `flat_library_distinguishes_absent_empty_and_unreadable_signatures` is **strong** for the three distinguishable outcomes, and `flat_library_keeps_the_run_when_the_signature_is_unreadable` is **strong** for preserved accounting and for a primary timeout/execution error surviving alongside the artifact failure. For the CLI path, `test_dump_signature_none`, `test_dump_signature_zero_size`, and `test_load_and_run_with_signature` remain **strong** for successful cases; no public CLI test injects a signature-region read failure.
- **Impact:** The flat-library result now distinguishes “no metadata,” “empty artifact,” and “artifact read failed”. The CLI limitation must stay explicit.
- **Next decision:** If the CLI surface is repaired later, decide the result/error contract there and add a focused reproduction; that is not part of A2.

### G-07 — Host-only guest ELF commands require the project toolchain/container

- **Disposition:** **Host-environment limitation; guest verification supplied by Docker and post-merge main CI**, not a simulator defect.
- **Surface:** Project-authored bare-metal ELF suite and the real cross-assembled `test_add_program` path.
- **Evidence:** On the host, `riscv64-unknown-elf-as` and `riscv64-unknown-elf-ld` were absent; `cargo test --all-features --test test_add_direct -- --nocapture` skipped the test; `./scripts/compile_riscv_tests.sh` exited 1; and `./scripts/run_elf_tests.sh` exited 1 because no ELFs existed. The repository Docker image then compiled 46 ELFs, ran `test_add_program` successfully with exit code 0/cycles 53, and passed the ELF runner 46/46. This Docker run predates the final executor-test strengthening and was not rerun by the PR14 independent reviewer or by the follow-up correction run.
- **Post-merge evidence:** [Main CI run 34184525187](https://github.com/mimiqdev/ruscv-sim/actions/runs/34184525187) at `fae4759e09c6750e4105c8be7c287e841dabf0dc` compiled the guest programs and passed 46/46 on 2026-09-08. This is exact-merge evidence beyond the historical Docker run, not proof of host toolchain installation.
- **Impact:** Host-only assembly still requires a suitable toolchain; guest verification no longer blocks A1 acceptance because exact-main CI supplied it. Neither Docker nor CI establishes ISA-wide compliance. Workflow definitions and old reference logs alone are not successful-run evidence.
- **Next decision:** Use the Docker image or install a compatible host toolchain when repeating guest verification; preserve the actual command output.

### G-08 — Bare-metal README exit-code text conflicts with executable source

- **Disposition:** **Documentation discrepancy**, not a runtime defect.
- **Evidence:** `tests/bare-metal-riscv-test/README.md` lists `add.elf` as expected exit code 55. `tests/bare-metal-riscv-test/rv64i/add.S` writes zero on its pass path, and `scripts/run_elf_tests.sh` expects zero for discovered tests. The source and runner are authoritative for current behavior.
- **Impact:** The README is unsafe as compatibility evidence for the add test's exit code.
- **Next decision:** Reconcile the public test documentation in a documentation-only follow-up if desired; do not infer a behavior change from the stale text.

### G-09 — Public commit log omits memory-access suffixes

- **Disposition:** **Reproduced defect**.
- **Surface:** `--log-commits` memory-access annotations on the CLI/public ELF path.
- **Implementation evidence:** `CommitLogger::log_commit` supports optional `MemoryAccess` formatting, but `load_and_run` unconditionally sets `let mem_access = None` before logging each committed instruction.
- **Reproduction:** The transient store/exit ELF produced a log line ending after its register changes, with no `mem 0x...` suffix, even though the formatter tests can emit store/load suffixes when supplied with a `MemoryAccess`. Persistent `public_behavior::public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` repeats this against the public path and asserts that no line contains ` mem `.
- **Existing tests:** `test_log_commit_with_memory_load`, `test_log_commit_with_memory_store`, and `test_memory_access_helpers` are **strong** for formatter/helper inputs; the persistent public-path test is **strong** for the current omission; the current `test_log_commit_path_with_logger` is **strong** for result, line count, and PC/privilege shape, but deliberately does not inspect the known-bad opcode or missing suffix.
- **Impact:** Current public commit logs cannot be used as complete memory-side-effect traces. The formatter's capability is not current public-path behavior.
- **Next decision:** Decide whether public memory annotation is required and how it should be captured; no repair is made here.

### G-10 — Flat-library `run` loses a nonzero guest exit code when clearing the signal

- **Disposition:** **Repaired** in A2 T2. A2 recorded it as source-observed before the repair.
- **Surface:** `RiscVSimulator::run`, which delegated to a private `get_result` before the repair.
- **Implementation evidence:** `run` decoded an exit code from the RAM tohost value, cleared the signal, and then called `get_result`, which re-read the now-zero signal value. The reported `exit_code` was therefore `0` for any guest exit, including a nonzero failure code.
- **Reproduction:** A bounded harness loaded a `0x80000000`-base image, called `set_tohost(0x100)`, and ran a guest that stored the standard payload `3` (exit code `1`) at flat offset `0x100`. Before the repair the result was `{exit=0, cycles=4, pc=0x80000010, timed_out=false, error=None}`. After the repair the same run reports `exit=1`.
- **Existing tests:** `flat_library_retains_the_nonzero_exit_before_clearing_the_signal` is **strong**: it asserts the nonzero exit and that the flat signal bytes are cleared afterwards. `flat_library_reports_zero_and_nonzero_guest_exits` covers codes `0`, `1`, and `42`. `flat_library_distinguishes_guest_exit_timeout_and_execution_error` separates a retained guest exit from a timeout and from an execution error.
- **Impact:** A guard against a nonzero failure exit was silently reported as success. This is a result-surface repair; it does not by itself complete A2.
- **Selected successor scope:** [A2](../archive/milestones/a2-closeout-record.md) T2 repaired this result gap; A2 is complete with documented limitations.

### G-11 — A manual flat tohost near the top of the address space panics the exit poll

- **Disposition:** **Reproduced defect** (pre-existing, not repaired).
- **Surface:** `RiscVSimulator::set_tohost` followed by `RiscVSimulator::run`, and the CLI `--tohost` option followed by a run.
- **Implementation evidence:** `SimpleMemory::read_dword` computes `addr + 8 > self.size` without a checked add, and the wrapper's poll passes the configured flat offset through unchanged. An extreme manual offset therefore overflows in the bound check instead of failing the poll. The same unchecked pattern exists in `SystemBus::read_dword`, which the CLI reaches through `--tohost`; an A3 T1 differential probe with `--tohost 0xFFFFFFFFFFFFFFFF` panicked at `src/executor.rs` in `SystemBus::read_dword`, identically before and after that change. The CLI address parser accepts plain hexadecimal without digit separators.
- **Reproduction:** With a loaded one-instruction image, `set_tohost(0xFFFF_FFFF_FFFF_FFF8)` then `run(Some(4))` panicked at `src/memory/mod.rs:105` with `attempt to add with overflow` (debug build). The same panic occurs at the pre-T2 revision `2769f56`, so A2 T2 neither introduced nor changed it.
- **Existing tests:** None. The A2 T2 placement tests cover image-derived offsets, which are now range-checked and aligned at load, so they cannot reach this path. Only a manual flat offset at the top of the address space does.
- **Impact:** A public setter combined with `run` can panic instead of returning a bounded error. The exit poll for a selected offset should report an error or a distinguishable timeout shape.
- **Next decision:** Choose between a checked bound in the flat memory accessors and a validated manual offset in the wrapper, and record whether the memory-wide hardening is in scope. Neither is part of A2 T2.

### G-12 — Flat-library signature artifact was read at the guest address and suppressed

- **Disposition:** **Repaired** in A2 T3.
- **Surface:** `RiscVSimulator::run` result construction for `ExecutionResult.signature_data`.
- **Implementation evidence:** The wrapper passed the image's signature metadata `vaddr` straight to `dump_signature` against its flat `SimpleMemory`, and discarded the `ExecutorError` with `.ok().flatten()`. For any nonzero-base image the guest address is not a flat offset, so the read failed and the artifact was silently absent while `signature_addr` still reported the guest address.
- **Reproduction:** A `0x80000000`-base image with a declared `.signature` at `0x80002000`, whose guest stored `0x5a` at the first signature byte and then exited through its declared tohost, returned `{exit=0, cycles=8, signature_addr=Some(0x80002000), signature_data=None, error=None}` before the repair. The same run now returns `signature_data=Some([0x5a, 17, 34, 51, 68, 85, 102, 119])`. A declared-but-unmappable region at `0x80200000` returned `signature_data=None` with no diagnostic; it now returns an explicit `Signature artifact unavailable: …` message while preserving the run.
- **Existing tests:** `flat_library_returns_guest_written_signature_bytes_at_nonzero_base` and `flat_library_returns_signature_bytes_at_base_zero` are **strong** for guest-written bytes plus the guest metadata address; `flat_library_distinguishes_absent_empty_and_unreadable_signatures` and `flat_library_keeps_the_run_when_the_signature_is_unreadable` cover the artifact outcomes and preserved accounting; `flat_library_replaces_image_metadata_and_ram_on_a_second_load` covers replacement. At the pre-T3 revision the new suite reports `33 passed; 4 failed`.
- **Impact:** The flat library reported an address with no bytes and no explanation. This repair is T3 only; the integrated workflow remains open.
- **Selected successor scope:** [A2](../archive/milestones/a2-closeout-record.md) T3 repaired this artifact gap; A2 is complete with documented limitations.

### G-13 — RV64I conditional branch used a 12-bit sign extension for a 13-bit B-immediate

- **Disposition:** **Reproduced defect** and **Repaired** under A5 (in-scope base-I branch behavior; the milestone contract authorizes minimal base-I ISA fixes with reproduced-before-repair regressions).
- **Surface:** [`exec_branch`](../../src/isa/rv64i/branch.rs) taken-branch displacement, on the shared decode+execute path used by the public ELF `run`.
- **Implementation evidence:** The 32-bit decoder in [`src/decode/mod.rs`](../../src/decode/mod.rs) reconstructs the B-type immediate correctly as a raw 13-bit value (`imm[12|10:5|4:1|11]`, bit 0 always 0). `exec_branch` then sign-extended it with `((imm as i32) << 20 >> 20)`, a 12-bit sign extension from bit 11, dropping the true sign bit 12. The fix is the minimal width correction to `((imm as i32) << 19 >> 19)`, sign-extending from bit 12. No comparator predicate, `x0`, PC fall-through, retirement, or memory side effect was changed. This defect is distinct from the still-unimplemented `Opcode::MiscMem`/FENCE gap, which is untouched here.
- **Reproduction:** With the buggy width, raw decoded B-immediates mapped as `0x0800 -> -2048` (should be +2048), `0x0ffc -> -4` (should be +4092), `0x0ffe -> -2` (should be +4094, the spec maximum reachable positive offset), and `0x1000 -> 0` (should be -4096, the spec minimum). Unit reproduction: five decode+execute boundary tests in `src/isa/rv64i/branch.rs` observed PCs of `0x80001800`, `PC-4`, `PC-2`, and `PC` (no displacement) at base PC `0x80002000` before repair. Public-path reproduction: `public_behavior::public_taken_branch_uses_full_13_bit_forward_displacement` built an ELF whose taken forward `BEQ` (+2048) must skip a wrong exit to reach the success exit; before repair it returned `exit_code=1`, after repair `exit_code=0, cycles=6`.
- **Existing tests:** `src/isa/rv64i/branch.rs::tests::{test_bimm_positive_2048, test_bimm_positive_4092, test_bimm_positive_max_4094, test_bimm_negative_min_4096, test_bimm_unsigned_predicates_displacement, test_bimm_not_taken_no_displacement}` are **strong** for the boundary displacements across signed/unsigned predicates and taken/not-taken paths on the decode+execute path; `public_behavior::public_taken_branch_uses_full_13_bit_forward_displacement` is **strong** for the full public ELF `run` path. `test_negative_offset` was strengthened to feed the raw decoded immediate (`0x1FE0`) that the decoder actually produces rather than a pre-sign-extended `u32`; it did not previously catch the bug because `-32`'s raw form sets both bit 11 and bit 12, making 12-bit and 13-bit extension coincide.
- **Impact:** All taken conditional branches with a B-immediate whose bit 12 differs from bit 11 (roughly, positive offsets ≥ +2048 and negative offsets in `[-4096, -2049]`) reached the wrong target. This is the proved defect and its direct regressions.
- **ACT4 branch replay (bounded, informative):** The retained A5 feasibility artifact `10274071547` (workflow run `34624439277`, head `4e8a947300f6012d83b991017df4247cda06a449`, SHA-256 `08682429f62b8deaf656e1013dd1e11ad54795609353b48087f3810977cdbbd1`) holds the six generated branch ELFs and their recorded pre-fix results. Replaying each unchanged ELF (hashes verified against the manifest) through `target/release/ruscv-sim run <elf> --max-cycles 1000000` at this fix head flipped every one from FAILED to SUCCESS:

  | Case | ELF SHA-256 (prefix) | Before (head `4e8a947`) | After (this fix) |
  | --- | --- | --- | --- |
  | `I-beq-00`  | `2bfa8160` | exit 1, FAILED, PC `0x80009800`, Invalid memory address | exit 0, SUCCESS, 5008 cycles, PC `0x8001c038` |
  | `I-bge-00`  | `c068eaa9` | exit 1, FAILED, PC `0x80009800`, Invalid instruction `0x800097ec` | exit 0, SUCCESS, 5087 cycles, PC `0x8001c038` |
  | `I-bgeu-00` | `d5aa3a43` | exit 1, FAILED, PC `0x8000981c`, Invalid memory address | exit 0, SUCCESS, 5083 cycles, PC `0x8001c038` |
  | `I-blt-00`  | `414ed9b8` | exit 1, FAILED, PC `0x80009800`, Invalid instruction `0x800097ec` | exit 0, SUCCESS, 5096 cycles, PC `0x8001c038` |
  | `I-bltu-00` | `7093f339` | exit 1, guest-fail (self-check mismatch), PC `0x8001c050` | exit 0, SUCCESS, 5098 cycles, PC `0x8001c038` |
  | `I-bne-00`  | `9bc273ef` | exit 1, FAILED, PC `0x80009800`, Invalid memory address | exit 0, SUCCESS, 5169 cycles, PC `0x8001c038` |

  This replays only the six branch ELFs against a locally built release binary; it is not a full-selection A5 rerun or a freeze. The A5 51-source profile remains provisional and the misalignment/MXLEN decisions remain pending; this entry does not alter that status.

### G-15 — Overflow-prone storage bounds and native RAM routing

- **Disposition:** **Reproduced defect; bounded repair implemented**, awaiting
  independent review and merge. This is a separately authorized robustness
  repair, not A5 selection progress or an implicit profile change. G-14 is
  reserved for the independent FENCE repair in PR #35.
- **Surface and rule:** [`SimpleMemory`](../../src/memory/mod.rs) previously
  narrowed `u64` addresses with `as usize`, then tested unchecked `addr + width`.
  [`SystemBus`](../../src/executor.rs) likewise added both the access width and
  region size before comparing endpoints. All reads/writes of widths 1/2/4/8
  now use the crate-private `contains_range` rule; storage additionally uses
  `checked_index` before locking/indexing. Sign-/zero-extension methods continue
  to delegate to those checked reads.
- **Address-space policy:** A region extending beyond `u64::MAX` exposes only
  its addressable prefix; a complete access may include the byte at `u64::MAX`
  but may not wrap through zero. Subtraction-based containment and checked host
  conversion avoid requiring a representable exclusive endpoint. Storage errors
  retain the original `u64` offset. Bounds still precede alignment; in-bounds
  misalignment remains `Misaligned`, and out-of-bounds misalignment returns
  `InvalidAddress`. Invalid writes do not touch storage.
- **Reproduced before repair:** On `978cce0`, aarch64 macOS, Rust 1.98.0,
  `cargo test --test memory_bounds` failed four of the initial five regression
  tests with arithmetic overflow panics (storage, bus access endpoint, bus
  region endpoint, public execution). In release, storage `read_half(MAX-1)`
  with size zero reached an index-out-of-bounds panic; other near-MAX storage
  accesses and base-zero bus accesses returned `Misaligned` instead of
  `InvalidAddress`. The final valid byte of high-base RAM was rejected.
  No successful invalid access was observed. The narrowing defect on 32-bit
  hosts is latent: it was not reproduced on a 32-bit target.
- **Public-path evidence:** The hand-built LD/SD fixtures in
  [`tests/memory_bounds.rs`](../../tests/memory_bounds.rs) exercise both paths.
  The flat facade subtracts its nonzero loaded base with the intentional
  `MemoryAdapter::wrapping_sub`; a guest address one byte below that base
  becomes offset `u64::MAX`. Native CLI cores instead reset with base zero, so
  the bus fixtures explicitly construct guest address `u64::MAX`.
  Testing the four separately against an unmodified `978cce0` source archive
  reproduced debug panics for both loads and both stores. In release the flat
  pair returned the wrong `Misaligned` error; the native pair already returned
  a clean `InvalidAddress` after RAM-relative conversion. After repair all four
  return `InvalidAddress`, with the faulting instruction unretired and its PC
  unchanged. CLI subprocess tests require exit status 1 and an address-error
  diagnostic, not a panic.
- **Preserved behavior and coverage:** Eleven focused tests cover empty/small
  storage, MAX/MAX-1/MAX-7 and aligned near-end addresses, host-narrowing
  candidates, final-valid/first-invalid accesses, error ordering, invalid-write
  nonmutation, extension values/errors, high-base RAM and region overflow.
  Device tests retain RAM-first routing, UART byte-only access, HTIF dword
  callbacks and fallthrough when a complete RAM access does not fit.
  No public API, error variant, lock/storage model, MMU/PMP/trap/ISA behavior,
  address translation, or normal alignment semantics changes.
- **Verification:** The six-command Rust gate in `docs/dev-plan.md`,
  `cargo test --release --all-features`, the A5 Python unit tests (23), and
  `bash -n scripts/a5/experiment.sh` passed locally. The local RISC-V assembler
  is unavailable, so `test_add_program` uses its existing early-return path;
  Cargo success is not a separate 46-guest ELF run. That CI step runs only on
  push to main, not on this PR.
- **External limitations:** Artifact `10274071547` metadata was reachable and
  reported the expected 21,676,628-byte size and SHA-256
  `08682429f62b8deaf656e1013dd1e11ad54795609353b48087f3810977cdbbd1`,
  but full and bounded-range download attempts exceeded their 45-second
  wall limits. No complete archive was hash-verified or replayed, and no new
  ACT4 generation was attempted. The eight success-misalignment cases and
  MXLEN decisions remain pending; this repair changes neither the A5 contract,
  denominator nor profile approval status.

## A4 bounded stop-decision ownership gap

The initial A4 T1 helper shared accounting and RAM decode-before-clear but left
`load_and_run` and `RiscVSimulator::run` independently branching on step errors,
guest exits and exhaustion. This was an internal contract gap, not a new
reproduced public behavior defect or a reopening of G-10.

[`RunControl::start` / `after_step`](../../src/executor.rs) now own the
continue/guest-exit/timeout/execution-error selection and retirement accounting.
The configurations supply their ordered lazy observers, and the owner traverses
them with first-exit short-circuiting. The CLI supplies HTIF before selected RAM;
the flat library supplies only selected RAM. Existing RAM decode-before-clear
and configuration-specific diagnostics, address forms and artifact policies
remain unchanged.

Five focused decision tests and two new
[`a4_run_control`](../../tests/a4_run_control.rs) public regressions directly
exercise these rules; exact coverage and local verification are recorded in the
[matrix](public-behavior-matrix.md#a4-t1-shared-run-control-evidence).
The implementation closes the bounded ownership gap. The
[A4 closeout](../archive/milestones/a4-closeout-record.md) records T3 exact-head
independent approval, the full gate and final `e47a1b0` merge CI with separately
compiled guests at 46 total / 46 passed / 0 failed. All A4 criteria have evidence;
formal closeout and the [A5 proposal](../dev-plan.md) await closeout merge approval.
All previously retained public gaps keep their dispositions; no new public defect
was found by T3. No external ISA verification is implied.

## Persistent second-batch evidence

The committed PR14 snapshot's persistent command passed **14 tests**. The
follow-up working tree's command is:

```text
cargo test --all-features --test public_behavior -- --nocapture
```

It passed **17 tests**: the original 14, the prefilled-buffer BSS test, and two
G-02 harness cleanup-path tests. The fixture-backed tests assert segment bytes
and zero fill, nonzero entry/base execution, RAM effects, exit encodings and
process statuses, exact limits, tohost precedence, UART bytes, signature bytes,
the CLI/flat-library distinction, and actual public commit-log contents. The
G-02 child requires a ready marker before its two-second hang window; the parent
kills and waits for every spawned child, including failure paths. No test changes
the runtime loop or treats a known-defect observation as supported behavior.
The Docker 46/46 result remains prior evidence from before final executor-test
strengthening, not a follow-up rerun. The detailed zero-fill mutation outcomes
and the public/component evidence split are recorded in matrix E7.

## Out of scope for this register

This batch does not repair any entry, add a broad ISA campaign, migrate
Runner/Machine/Platform boundaries, define precise outcomes, change address maps,
or alter limit semantics. Component tests for MMU, TLM, peripherals, traps, or ISA
implementations remain component evidence only.
