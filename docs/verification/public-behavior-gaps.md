# Public Behavior Defect and Gap Register

**Status:** Current evidence record

**Authority:** Informational; this register supports the [A1 public behavior matrix](public-behavior-matrix.md) and does not authorize production repair

**Last reviewed:** 2026-09-08; individual verification runs retain their recorded scope

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

- **Disposition:** **Reproduced defect**.
- **Surface:** `RiscVSimulator::load_elf` / `run` versus CLI `load_and_run`.
- **Contract context:** [ADR-0003 §9](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product) keeps `RiscVSimulator` available and requires the CLI and flat-library configurations to remain explicit; it does not bless a wrapper that silently misses its guest exit.
- **Implementation evidence:** `src/executor.rs::RiscVSimulator::load_elf` loads the `LoadedElf.memory` buffer relative to `base_addr`, resets the core with `base_addr`, and stores `loaded.tohost` without converting it to a flat offset. `RiscVSimulator::run` calls `SimpleMemory::read_dword(self.tohost)` directly.
- **Reproduction:** The same transient ELF with entry/base `0x30000000` and `.tohost = 0x30001000` exited through the CLI at cycle 7. The library harness returned:

  ```text
  library entry=0x30000000 ... result={exit=1, cycles=20, timeout=true, error=Some("Timeout after 20 cycles")}
  flat_offset_exit={exit=0, cycles=3, timeout=false, error=None}
  ```

  The persistent `public_behavior::flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` test repeats the contrast at base `0x80000000`: the CLI exits at cycle 4, while `RiscVSimulator::load_elf` followed by `run(Some(20))` returns `exit_code=1`, `cycles=20`, `timed_out=true`, and `Timeout after 20 cycles`. The manually configured flat-offset case from the first batch succeeded only after setting `tohost=0x100` and placing the program/exit value at that offset.
- **Existing tests:** The persistent test is **strong** for the configuration contrast. The targeted `load_and_run` tests in `tests/executor.rs` now use valid fixtures and assert their results; `test_simulator_creation`, `test_simulator_setters`, `test_simulator_run_with_max_cycles`, and `test_simulator_run_default_cycles` remain **weak** for this flat-library ELF behavior because they do not assert an ELF tohost exit.
- **Impact:** The wrapper's public ELF success/exit behavior is not equivalent to the CLI. Do not claim flat-library ELF/tohost compatibility from the CLI runs.
- **Next decision:** A later scope must decide whether to add an adapter/fix or retain this documented limitation. The persistent reproduction is retained; no repair is made here.

### G-02 — Flat-library `read_mem` can loop indefinitely on an out-of-range aligned read

- **Disposition:** **Reproduced defect**.
- **Surface:** `RiscVSimulator::read_mem`.
- **Implementation evidence:** In `src/executor.rs::RiscVSimulator::read_mem`, an aligned dword/word/half read error can reach the next loop iteration without advancing `current_addr` or `offset`. For an address outside `SimpleMemory`, the loop has no progress condition.
- **Reproduction:** A bounded temporary program called `RiscVSimulator::new(0x1000).read_mem(0x2000, 4)`. Running the built harness as `timeout 2s stdbuf -o0 ...` printed `before` and returned status `124`; it did not print a result. The follow-up `public_behavior` harness now launches the exact read in a recursively isolated `--exact` child, requires a ready marker after simulator construction and before the read, waits two seconds only after that marker, then kills it and calls `wait()` to reap it. Child stdout/stderr are retained for diagnostics, and `KillOnDrop` covers panic/error paths. Companion tests exercise missing-ready and early-exit paths; the parent passes the reproduction only when the child remained alive through the post-ready hang window.
- **Existing tests:** `test_simulator_read_write_mem` and `test_simulator_write_mem_large` are **strong** only for in-range accesses. The persistent reproduction plus its handshake-failure and early-exit cleanup tests are **strong** for the bounded harness behavior. `test_simulator_read_mem_unaligned` remains **weak** because it accepts either `Ok` or `Err`.
- **Impact:** A public memory inspection helper can hang instead of returning `ExecutorError`.
- **Selected successor scope:** [A2](../dev-plan.md) authorizes the bounded progress/error repair. This A1 evidence record does not claim the repair is implemented; retain the reproduction until the implementation is verified.

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

- **Disposition:** **Source-observed gap; error path unverified**.
- **Surface:** `ExecutionResult.signature_data` after a signature read failure.
- **Implementation evidence:** All `load_and_run` result paths call `dump_signature(...).ok().flatten()`, so a read error becomes `None` while `signature_addr` may remain `Some`.
- **Existing tests:** `test_dump_signature_none`, `test_dump_signature_zero_size`, and the current `test_load_and_run_with_signature` are **strong** for successful helper/public artifact cases; no public test injects a signature-region read failure.
- **Impact:** The result cannot distinguish “no data,” “empty signature,” and “signature read failed” through `signature_data` alone. The current behavior must not be advertised as successful signature capture on an unreadable range.
- **Next decision:** Decide the result/error contract and add a focused reproduction/regression if a repair is authorized.

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
