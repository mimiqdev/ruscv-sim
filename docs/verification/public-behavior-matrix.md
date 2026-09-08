# Public Behavior Compatibility Matrix

**Status:** Current evidence record

**Authority:** Informational; scope is defined by the [active A1 contract](../dev-plan.md), while compatibility requirements are stated in [ADR-0003 §9](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product)

**Last verified:** 2026-09-07

**Revision examined:** `64c6f976e27ca24167f32f70c99d2469c7928933` (branch HEAD)

This record combines the first A1 inventory with the second A1 tests and
reproductions batch. E1-E5 retain historical evidence from the `b20e531` source
baseline and the documentation-only warning repair at `64c6f976`; E6 records
persistent tests in the current uncommitted working tree. It is not an A1
closeout, a target-architecture implementation claim, or an approval to repair
production behavior. Source code and executed tests are the authority for current
behavior; accepted ADR text is the contract against which a mismatch is recorded.
An observed error is never a compatibility promise.

## Reading the matrix

- **Verified** means that the named behavior was exercised or asserted with a
  meaningful value, state, output, or artifact in the named configuration.
- **Reproduced defect** means that an incorrect, misleading, or unsafe result was
  observed in a bounded reproduction. It is explicitly excluded from the
  compatibility promise.
- **Unverified** means that evidence is missing, the existing assertion is too
  weak, or a required dependency was unavailable. A source observation alone is
  not an end-to-end verification.
- Test strength is marked **strong** when it checks a value/effect/artifact and
  **weak** when it only checks construction, no panic, a generic `Ok`/`Err`, or a
  manually constructed result.

The compatibility paths are kept separate throughout:

- **CLI/public ELF path:** `ruscv-sim run` → `load_and_run_file` → `load_and_run`
  → `SystemBus` + `RiscvCore`.
- **Flat-library path:** `RiscVSimulator` → `SimpleMemory` + `RiscvCore`.
- **Component path:** loader, memory, UART, logger, or other focused tests that
  do not prove public-path integration.

## Evidence index

### E1 — Repository tests

At the revision above:

| Command | Outcome | Evidence boundary |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Exit 0 | Rust formatting check passed. |
| `cargo check --all-features` | Exit 0 (**pre-warning-fix baseline**) | All-feature compilation passed before the documentation-only warning repair; not rerun in this pass. |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0 (**pre-warning-fix baseline**) | Strict Clippy check passed before the documentation-only warning repair; not rerun in this pass. |
| `cargo test --all-features` | Exit 0 (**pre-warning-fix baseline**); every reported suite passed, including **809** library tests | Repository Rust test evidence before the documentation-only warning repair; not rerun in this pass and not guest ELF evidence. |
| `cargo doc --all-features --no-deps` | Exit 0, with **77 rustdoc warnings** (**pre-warning-fix baseline**) | Initial warning inventory; superseded by the post-repair doc runs below. |
| `cargo doc --all-features --no-deps` | Exit 0 with **zero warnings** (**post-warning-fix**) | Current documentation-build verification after the comment/link fixes. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Exit 0 with **zero warnings** (**post-warning-fix**) | Strict rustdoc verification; no warning suppression was added. |
| `cargo test --all-features --doc` | **27 passed** (**post-warning-fix**) | Applicable doctests passed after the documentation example changes. |
| `cargo fmt --all -- --check` | Exit 0 (**post-warning-fix**) | Formatting check after the documentation-only source changes. |
| `cargo test --all-features --test cli_test --test executor --test test_elf_loader --test commits_test -- --nocapture` | **100 passed** (**pre-warning-fix baseline**) | Focused Rust tests before the documentation-only warning repair; not rerun in this pass. |
| `cargo test --all-features --test executor -- --nocapture` | **57 passed** (**current working tree**) | Strengthened public `load_and_run` result, zero-limit, file-path, no-logger, and logger-artifact assertions; no production source changed. |
| `cargo test --all-features` | Exit 0 (**current working tree after executor strengthening**); **809** library tests, **57** executor tests, **14** persistent public-behavior tests, **27** doctests, and all reported integration suites passed | The existing generated `add.elf` allowed `test_add_program` to pass in this run; this does not establish that the host cross-assembler/linker is installed. |
| `cargo test --all-features --lib` | **809 passed** (**pre-warning-fix baseline**) | Unit/component evidence before the documentation-only warning repair; not rerun in this pass. |
| `cargo test --all-features --test test_add_direct -- --nocapture` | Test process passed, but `test_add_program` **skipped** because `riscv64-unknown-elf-as` was unavailable (**host baseline**) | No project-authored guest ELF was executed by this host run. |
| `./scripts/compile_riscv_tests.sh` | Exit 1: `riscv64-unknown-elf-as` unavailable (**host baseline**) | Host-only guest fixture compilation was unavailable. |
| `./scripts/run_elf_tests.sh` | Exit 1: no generated `.elf` files (**host baseline**) | Host-only guest suite did not run. |
| `docker run --rm --init -v "$PWD:/workspace" -w /workspace ruscv-sim-dev bash -c 'export PATH=/opt/riscv/bin:$PATH; ./scripts/compile_riscv_tests.sh && cargo test --all-features --test test_add_direct -- --nocapture && ./scripts/run_elf_tests.sh'` | Exit 0: 46 ELF files compiled; `test_add_program` passed with exit code 0/cycles 53; ELF runner passed **46/46** (**pre-warning-fix guest evidence**) | Docker supplied the missing cross-toolchain and Spike; this guest run was not repeated during the warning-only repair. |
| `cargo fmt --all -- --check` | Exit 0 (**second-batch working tree**) | Formatting passed after adding the persistent fixture/test files. |
| `cargo check --all-features` | Exit 0 (**second-batch working tree**) | All-feature compilation passed with the persistent fixture/test files. |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0 (**second-batch working tree**) | Strict Clippy passed, including integration-test targets. |
| `cargo test --all-features` | Exit 0 (**second-batch working tree**); **809** library tests, **14** persistent public-behavior tests, and all reported integration suites passed; `test_add_program` returned from its host-toolchain skip path | Host has no RISC-V cross assembler/linker, so this is not host guest-ELF execution evidence. The dedicated persistent fixture suite is self-contained. |
| `cargo doc --all-features --no-deps` | Exit 0 with zero warnings (**second-batch working tree**) | Documentation remains warning-free after the persistent test additions. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Exit 0 with zero warnings (**second-batch working tree**) | Strict rustdoc remains clean. |
| `cargo test --all-features --doc` | **27 passed** (**second-batch working tree**) | All applicable doctests passed. |
| `docker run --rm --init -v "$PWD:/workspace" -w /workspace ruscv-sim-dev bash -c 'export PATH=/opt/riscv/bin:$PATH; ./scripts/compile_riscv_tests.sh && cargo test --all-features --test test_add_direct -- --nocapture && ./scripts/run_elf_tests.sh'` | Exit 0: 46 ELF files compiled; `test_add_program` passed with exit code 0/cycles 53; ELF runner passed **46/46** (**second-batch guest evidence**) | Repeated through `newgrp docker` because the host shell lacked direct Docker-socket permission. The guest run uses the project image and remains separate from the self-contained persistent fixture suite. |

The initial repository quality gate recorded 77 rustdoc warnings. The
warning-only repair passed both ordinary and `-D warnings` rustdoc builds, plus
27 applicable doctests and formatting. The current second-batch rows record the
same quality gate with the persistent fixtures/tests present; the focused
executor and full-test rows record the later assertion strengthening; and the
older pre-warning-fix rows remain historical baseline evidence. The current full
run used an existing generated `add.elf`, while the host-only compile limitation
and the second-batch Docker row remain recorded separately. The repository's old
CI definitions and checked-in reference logs are not substituted for an A1 run.

#### Rustdoc warning repair scope

The pre-fix 77-warning total consisted of 35 broken intra-doc links, 8 links
from public documentation to private TLM modules, 2 invalid HTML tags, 31
assembly examples incorrectly marked as Rust code blocks, and 1 redundant
explicit intra-doc link. The repair changed only documentation comments, link
markup, and code-fence language tags; it did not change runtime logic or public
API signatures. The post-fix plain and strict doc builds both reported zero
warnings.

### E2 — Bounded transient public-path ELF runs

A non-persistent hand-built ELF64/RISC-V fixture set was used only to exercise
specific public behaviors without changing `src/`, `tests/`, build dependencies,
or CI. The fixtures were removed after observation; they are not a second-batch
test suite. The programs used ordinary RV64I instructions and self-checked
results where applicable.

| Scenario and command shape | Observed result |
| --- | --- |
| `ruscv-sim run fixed.elf --max-cycles 10` | Exit code 0, cycles 3, final PC `0x8000000c`, process status 0. |
| `ruscv-sim run nonzero.elf --max-cycles 10` | Exit code 1, cycles 3, process status 1. |
| `ruscv-sim run alt42.elf --max-cycles 10` | Alternative `(1<<63) \| 42` exit: exit code 42, cycles 6, process status 42. |
| `ruscv-sim run fixed.elf` | No explicit limit: exit code 0, cycles 3. This exercises the default-limit path, not exhaustion of the 10,000,000 bound. |
| `ruscv-sim run limit.elf --max-cycles 0` | Exit code 1, cycles 0, final PC at entry, `TIMEOUT`, `Timeout after 0 cycles`. |
| `ruscv-sim run limit.elf --max-cycles 5` | Exit code 1, cycles 5, `TIMEOUT`; the exit-causing sixth instruction was not started. |
| `ruscv-sim run limit.elf --max-cycles 6` | Exit code 0, cycles 6; the exit-causing store in the final permitted slot was observed. |
| `ruscv-sim run entry.elf --max-cycles 10` | Entry at nonzero offset `0x20000100` executed; exit code 0, cycles 3, final PC `0x2000010c`. |
| `ruscv-sim run zero-fill.elf --max-cycles 100` | A PT_LOAD file byte and a separate zero-filled byte were checked by guest code; exit code 0, cycles 12. |
| `ruscv-sim run ram.elf --max-cycles 100` | Guest store/load round-trip check passed; exit code 0, cycles 9. |
| `ruscv-sim run precedence.elf --max-cycles 20` | ELF `.tohost` address selected; exit code 0 after 7 cycles. |
| `ruscv-sim run precedence.elf --tohost 0x30001008 --max-cycles 20` | CLI override selected the first RAM signal; exit code 0 after 4 cycles. |
| `ruscv-sim run precedence.elf --tohost 0x40008000 --max-cycles 20` | Fixed endpoint was selected while the guest only wrote the RAM addresses; timeout after 20 cycles. |
| `ruscv-sim run uart.elf --max-cycles 100` | stdout contained `A1!` followed by a newline and a successful result; exit code 0, cycles 12. |
| `ruscv-sim run signature.elf --max-cycles 100` | Exit code 0, cycles 4; CLI reported signature address `0x80002000` and 8 bytes. A library harness also read `[0,17,34,51,68,85,102,119]` through `load_and_run`. |

These runs are behavioral evidence for the exact transient fixtures only. They do
not establish ISA-wide support, a compliance profile, or support for arbitrary ELF
layouts.

### E3 — Flat-library and helper reproduction

A temporary Rust harness called the public `load_elf_file`, `load_and_run`, and
`RiscVSimulator` APIs. Its relevant output was:

```text
loaded entry=0x30000000 base=0x30000000 memory=65536 tohost=Some(805310464)
cli_signature={addr=Some(2147491840), data=Some([0, 17, 34, 51, 68, 85, 102, 119]), cycles=4, timeout=false, error=None}
library entry=0x30000000 relative_code=[17, 12, 00, 00] result={exit=1, cycles=20, timeout=true, error=Some("Timeout after 20 cycles")}
flat_offset_exit={exit=0, cycles=3, timeout=false, error=None}
library_signature={addr=Some(2147491840), data=None, timeout=false, error=Some("Execution error: Execution error: Invalid instruction encoding: 0x00000000")}
```

The CLI and library cases used the same ELF. The CLI wrote/read the guest
addresses through `SystemBus`; the library wrapper's relative `SimpleMemory` did
not observe the ELF guest-address `tohost` value. The final `flat_offset_exit`
case is the contrasting configuration where the program and `tohost` were
manually placed at flat offset `0x100`.

A bounded helper reproduction also ran:

```text
before
read_mem_direct_status=124
```

`RiscVSimulator::read_mem(0x2000, 4)` on a 0x1000-byte simulator did not return
within a two-second `timeout`; see [G-02](public-behavior-gaps.md#g-02--flat-library-read_mem-can-loop-indefinitely-on-an-out-of-range-aligned-read).

### E4 — Actual commit-log observations

The logger formatter can emit memory suffixes when given a `MemoryAccess`, but
the public `load_and_run` loop passes `None`. A base-zero transient ELF produced
real log lines such as:

```text
core   0: 3 0x0000000000000000 (0x00100293) x5  0x0000000000000001
core   0: 3 0x0000000000000004 (0x40008237) x4  0x0000000040008000
core   0: 3 0x0000000000000008 (0x00523023)
```

The same three-instruction program loaded at nonzero base `0x80000000` produced:

```text
core   0: 3 0x0000000080000000 (0x00000000) x5  0x0000000000000001
core   0: 3 0x0000000080000004 (0x00000000) x4  0x0000000040008000
core   0: 3 0x0000000080000008 (0x00000000)
```

The nonzero-base opcode loss and the missing store-memory suffix are recorded as
reproduced gaps, not compatibility guarantees. Formatter-only tests such as
`test_log_commit_with_memory_load`, `test_log_commit_with_memory_store`, and
`test_memory_access_helpers` do not prove that the public loop emits those
records; see [G-03](public-behavior-gaps.md#g-03--public-commit-log-loses-opcodes-for-nonzero-elf-bases)
and [G-09](public-behavior-gaps.md#g-09--public-commit-log-omits-memory-access-suffixes).

### E5 — CLI surface smoke output

`target/debug/ruscv-sim run --help` reported the `ELF_FILE` positional input and
`--max-cycles`, `--tohost`, `--verbose`, and `--log-commits` options. The binary
reported `ruscv-sim 0.1.0` for `--version`. With `--verbose`, a real run printed
`Tohost address: 0x0000000000000000` before later debug output showed the selected
ELF address; that misleading diagnostic is [G-05](public-behavior-gaps.md#g-05--verbose-tohost-diagnostic-is-misleading).

### E6 — Persistent second-batch fixtures and tests

The second batch adds `tests/common/public_elf.rs` and
`tests/public_behavior.rs` without changing production code, APIs, dependencies,
or CI. The fixture builder emits a minimal ELF64 little-endian `ET_EXEC`
`EM_RISCV` image with one `PT_LOAD`, a nonzero base (`0x80000000`), optional
`.tohost`/`.signature` metadata, a file-backed byte region, and a larger
zero-filled memory extent. Its instruction encoders are limited to the RV64I
instructions used by the named tests; they are not an ISA implementation.

The focused command was:

```text
cargo test --all-features --test public_behavior -- --nocapture
```

It passed **14 tests**. The persistent assertions are:

| Test | Strong observation |
| --- | --- |
| `elf_segments_preserve_file_bytes_and_zero_fill` | Loader entry/base, code bytes, a file-backed byte, signature metadata/data, zero-filled tail, and allocated memory size. |
| `public_zero_fill_is_observed_by_guest_execution` | Guest loads the zero-filled tail, converts the observed value to a standard exit payload, and exits code 0 in 6 cycles. |
| `public_entry_and_nonzero_base_execute_from_the_declared_entry` | Guest entry offset `0x100`, nonzero base, exit code, exact cycle count, final PC, and timeout/error state. |
| `public_ram_store_and_load_affect_the_same_image` | Guest stores and reloads an exit payload through RAM, then exits code 42 after 8 cycles. |
| `default_limit_path_returns_an_early_guest_exit` | `None` limit follows the default-limit path and observes an early exit; it does not claim the 10,000,000-cycle exhaustion boundary. |
| `exact_limits_include_zero_and_the_final_exit_slot` | Exact zero, pre-exit, and final-slot limits assert cycles, PC, timeout text, and successful exit. |
| `cli_propagates_zero_and_nonzero_guest_exit_codes` | CLI process statuses 0 and 42 plus printed result fields. |
| `alternative_exit_encoding_is_observed_on_the_public_path` | Public alternative high-bit exit encoding returns code 42 in 5 cycles. |
| `tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint` | ELF section selection, CLI RAM override, and fixed-endpoint timeout are distinguished by code/cycles/status. |
| `uart_bytes_are_emitted_by_the_cli_elf_path` | CLI stdout contains the exact `A1!` plus newline bytes and a successful result. |
| `signature_bytes_are_returned_after_public_execution` | Public result contains signature address `0x80002000` and exact bytes `[0, 17, 34, 51, 68, 85, 102, 119]`. |
| `flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` | Flat `write_mem`/`read_mem` bytes round-trip; the same ELF exits in the CLI but times out in `RiscVSimulator`, reproducing G-01. |
| `public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` | Actual public log has four lines with zero opcodes and no `mem` suffix, reproducing G-03/G-09. |
| `out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped` | A child process running the out-of-range aligned helper read is allowed a 2-second bound, then killed and waited, reproducing G-02 without an unbounded test process. |

These tests are stronger evidence than the older smoke tests because they assert
state, process status, output, exact bytes, exact cycle/PC values, or actual log
artifacts. The known-defect tests intentionally document incorrect observations
and are linked to the gap register; they do not convert those observations into
compatibility promises.

The current working tree also strengthens the public `load_and_run` coverage in
`tests/executor.rs`. The former `test_load_and_run_small_memory` was renamed to
`test_load_and_run_with_one_cycle_budget` because `load_and_run` does not expose
a caller-selected memory size. The following tests now use the persistent valid
nonzero-base fixture rather than the old zero-length ELF and assert actual
results:

- `test_load_and_run_zero_cycles` checks exit code, zero cycles, entry PC,
  timeout state, and exact timeout text.
- `test_load_and_run_without_logger` and `test_load_and_run_file_path` check
  successful exit, cycles, final PC, and error/timeout state.
- `test_log_commit_path_with_logger` checks the result and the three emitted
  lines' hart/privilege/PC shape. It intentionally does **not** assert the
  known-bad nonzero-base opcode field; G-03/G-09 remain explicit reproduction
  evidence in `public_behavior.rs`.
- `test_htif_exit_code_non_exit`, the tohost-selection tests, verbose output,
  signature extraction, and the large-limit test now assert their bounded
  result; `test_load_and_run_with_signature` also asserts the exact artifact.

The focused executor command passed **57 tests** in the current working tree:

```text
cargo test --all-features --test executor -- --nocapture
```

For the 14 persistent `public_behavior` tests, the first 11 rows in the table
above are compatibility characterization. The flat-library ELF contrast
(G-01), public commit-log observation (G-03/G-09), and bounded out-of-range
`read_mem` case (G-02) are defect reproductions, not compatibility passes.
Other legacy executor/component smoke tests remain outside this targeted
strengthening and are not upgraded by this record.

## Compatibility matrix

### CLI, API inputs, ELF loading, and limits

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| CLI command and options | Retain `run <ELF_FILE>`, `--max-cycles`, hexadecimal-or-decimal `--tohost`, `--verbose`, and optional `--log-commits`. | `src/main.rs`: `Commands::Run`, `parse_addr`, `run_elf`, and `print_result`; `src/executor.rs::load_and_run_file`. | `test_cli_help`, `test_cli_run_help`, `test_cli_run_hex_tohost`, `test_cli_run_decimal_tohost`, and flag-combination tests. Help/parser checks are **strong** for syntax; run-flag tests are mostly **weak** because they do not assert result/output. | Help output and real option runs completed. **Verified** for the named CLI surface; execution claims are covered by the rows below. |
| Input and error boundary | Invalid/missing input must remain distinguishable from a guest result; `load_and_run` exposes loader errors. | `ElfLoader::load` validates ELF identity; `load_and_run_file` maps file reads to `ExecutorError::ElfLoadError`; CLI exits 1 on `Err`. | `test_cli_run_missing_elf`, `test_cli_run_invalid_elf`, `test_load_and_run_invalid_elf`, `test_load_and_run_truncated_elf`: **strong** error-status assertions. | Focused tests passed. **Verified** for the exercised invalid inputs; malformed ELF coverage is not exhaustive. |
| `ExecutionResult` and terminal presentation | Preserve exit code, completed cycle count, final PC, timeout/error distinction, and optional signature artifact; CLI prints the result and exits with `exit_code`. | `src/executor.rs::ExecutionResult` and `load_and_run`; `src/main.rs::print_result` and `std::process::exit(result.exit_code as i32)`. | `test_execution_result_defaults`, `test_execution_result_custom`, `test_execution_result_timeout`, and `test_execution_result_signature` are **weak-to-medium** because most values are manually constructed. | E2 observed actual exit/cycle/PC/timeout values and process statuses. **Verified** for the listed paths; broad error taxonomy remains unverified. |
| Accepted ELF input | Current loader accepts ELF64, little-endian, `ET_EXEC`, machine `EM_RISCV`, and rejects other identity fields. | `src/elf.rs::ElfLoader::load`. | `src/elf.rs::test_elf_load`, `test_load_elf_file_function`: **strong** for hand-built valid ELF; invalid-input tests are **strong** for rejection. | E2 used valid ELF64/RISC-V inputs. **Verified** for this profile, not for arbitrary ELF variants. |
| PT_LOAD file bytes | Load each PT_LOAD's `p_filesz` bytes at its `p_vaddr`-relative location. | `ElfLoader::memory_footprint`, `load_into_memory`, and `load_elf_file` create a relative buffer and copy each segment. | `test_load_into_memory`, `test_load_elf_file_function`, and persistent `public_behavior::elf_segments_preserve_file_bytes_and_zero_fill`: **strong** for selected bytes; overlap/permission cases remain unverified. | E2 and E6 assert code and data bytes through the loader; E6 also executes the same fixture. **Verified** for the exercised segment layout; permissions and overlap behavior are unverified. |
| PT_LOAD zero-fill | Bytes from `p_filesz` through `p_memsz` must be represented as zero-filled memory. | `load_into_memory` zeroes the tail of each segment; the initial `Vec` is also zeroed. | Persistent `public_behavior::elf_segments_preserve_file_bytes_and_zero_fill` asserts the tail byte, and `public_zero_fill_is_observed_by_guest_execution` loads it and asserts exit/cycles: **strong**. | E2 and E6 loaded a file byte and a distinct zero-filled byte; E6 also exercised the zero-filled byte through the CLI execution path. **Verified** for that layout. |
| Entry point | Preserve ELF `e_entry` as the guest PC; do not replace it with the storage base. | `ElfLoader::entry_point`; CLI calls `core.reset(entry_point, 0)`, while the library calls `core.reset(entry_point, base_addr)`. | `test_elf_load`, `test_load_elf_file_function`, and persistent `public_behavior::public_entry_and_nonzero_base_execute_from_the_declared_entry`: **strong** entry, cycle, and final-PC assertions. | E2 and E6 entry-offset fixtures executed from a nonzero entry and asserted the final PC. **Verified** for a non-base entry. |
| Nonzero load base and address meaning | Preserve guest addresses while allowing the current flat image to be stored relative to its lowest load address. | CLI stores relative bytes in `SimpleMemory`, routes them through `SystemBus(ram_base=base_addr)`, and resets the core with base 0; `MemoryAdapter` therefore passes guest addresses unchanged on CLI. | `test_memory_adapter_va_to_pa`, `test_memory_adapter_different_base`, and persistent `public_behavior::public_entry_and_nonzero_base_execute_from_the_declared_entry`: **strong** component/public assertions; `test_add_program` is guest evidence. | E2 and E6 executed at `0x80000000` and asserted results. **Verified** for execution; commit logging has the reproduced nonzero-base opcode defect in E4/E6. |
| RAM effects | Guest RAM stores and loads must affect the same public RAM image. | `SystemBus` routes RAM addresses to `SimpleMemory`; `RiscvCore::step` uses the bus for instruction and data memory. | `test_system_bus_routing` and persistent `public_behavior::public_ram_store_and_load_affect_the_same_image`: **strong** typed/public round-trips. | E2 and E6 stored and loaded a value before exit; E6 asserted code 42 and 8 cycles. **Verified** for the exercised aligned dword path. |
| Default limit | Keep the default maximum at 10,000,000 completed loop iterations/cycles in the compatibility path; this is not `mcycle`/`minstret` or virtual time. | `DEFAULT_MAX_CYCLES = 10_000_000`; `Option::unwrap_or` in both `load_and_run` and `RiscVSimulator::run`. | `test_simulator_run_default_cycles` remains a weak empty-instruction smoke test; persistent `public_behavior::default_limit_path_returns_an_early_guest_exit` is **strong** for the no-option path but does not exhaust the bound. | E2 and E6 no-option runs exited before the bound. The numeric exhaustion boundary was not run. **Unverified** for default-limit exhaustion. |
| Zero limit | `--max-cycles 0` performs no instruction and returns a timeout result with zero cycles. | `while cycles < max_cycles` is skipped; timeout result is built with `cycles = 0`. | Persistent `public_behavior::exact_limits_include_zero_and_the_final_exit_slot`: **strong** exit code, cycles, final PC, timeout flag, and exact error text. | E2 and E6 observed `TIMEOUT`, cycles 0, and final PC at entry. **Verified**. |
| Exact finite limit | A nonzero finite limit permits at most that many successful loop iterations; an exit on the last permitted instruction remains observable. | `cycles` increments immediately after `core.step`; tohost is checked before the loop condition is evaluated again. | Persistent `public_behavior::exact_limits_include_zero_and_the_final_exit_slot`: **strong** assertions for max 5 and max 6, including the final-slot exit. | E2 and E6 max 5 timed out at 5; max 6 observed exit at 6. **Verified** for exact-limit and final-slot ordering. |
| Execution error count | A failed `core.step` returns an error result without incrementing `cycles`; timeout is separate from execution error. | `load_and_run` increments only in the `Ok(())` arm and returns an error-shaped `ExecutionResult` in the `Err` arm. | `test_execution_result_error` is a **weak** manually constructed result; invalid-instruction public execution was not asserted with exact fields. | E2 zero/limit cases verified timeout counts; exact public execution-error count remains **Unverified**. |

### Exit, tohost, UART, signature, and logs

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| Exit encodings | Recognize the supported standard HTIF payload and the project alternative high-bit marker; report the decoded guest code. | `try_extract_exit_code` handles `(payload >> 1)` for standard `device=0/cmd=0` and `(1<<63) \| code` alternative. | `executor::test_htif_exit_code_extraction`, `test_htif_exit_code_alternative_format`, `test_htif_exit_code_standard_format`, and persistent `public_behavior::alternative_exit_encoding_is_observed_on_the_public_path`: **strong** pure/public assertions. | E2 and E6 returned standard codes 0/1/42 and alternative code 42. **Verified** for supported encodings. |
| Nonzero exit | A guest nonzero exit is a guest result, not a loader error; CLI propagates the code as its process status. | `ExecutionResult.exit_code` is set from tohost; CLI calls `exit(result.exit_code as i32)`. | Persistent `public_behavior::cli_propagates_zero_and_nonzero_guest_exit_codes` asserts actual process statuses 0 and 42 and output; older tests remain smoke-only. | E2 and E6 code 1/42 programs produced matching statuses. **Verified** for the persistent programs; codes outside host process range are unverified. |
| To-host precedence | `tohost_addr` precedence is CLI override, then ELF `.tohost`/symbol, then fixed `0x40008000`. | `load_and_run`: `tohost_addr.or(elf_tohost).unwrap_or(DEFAULT_TOHOST)`; loader discovers `.tohost` then `tohost` symbol. | Persistent `public_behavior::tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint` asserts each branch with exact cycles/status; `test_tohost_symbol_from_elf` and `test_fib_tohost_address` still skip when guest ELFs are absent. | E2 and E6 selected ELF address with no override, selected the first CLI RAM override, and timed out when selecting fixed HTIF for a guest that did not write it. **Verified** for section metadata and CLI override. Symbol fallback remains **Unverified** in this environment. |
| HTIF endpoint and final observation | Fixed `0x40008000` dword writes invoke the callback; selected RAM tohost is polled after each successful instruction and a recognized value is cleared. | `SystemBus::write_dword/read_dword`, HTIF callback, `load_and_run` post-success polling, and `clear_tohost`. Byte/half/word fixed-HTIF access is rejected. | `test_system_bus_read_htif`, `test_system_bus_write_htif`: **medium** (read/value or `Ok`, but callback value is not asserted); persistent `public_behavior::exact_limits_include_zero_and_the_final_exit_slot` and `tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint`: **strong** public timing/selection assertions; UART width tests are strong for rejection. | E2 and E6 fixed-endpoint and RAM-endpoint exits were observed, including final-slot exit. Post-run clearing of a selected RAM signal is source-supported but has no public assertion. **Unverified** for retained memory state after return. |
| UART public output | Public ELF path retains byte MMIO at `0x10000000`, output callback bytes, and rejection of multi-byte UART register access. | `SystemBus` routes byte accesses to `Uart16550`; UART callback is installed in `load_and_run`; its native window is `0x100` while the UART/TLM model declares `UART_SIZE = 8`. | Persistent `public_behavior::uart_bytes_are_emitted_by_the_cli_elf_path` asserts exact CLI bytes/result; `test_system_bus_routing` is strong for byte register round-trip; `test_system_bus_read/write_uart_{half,word,dword}` is strong for rejection; UART unit tests are component evidence. | E2 and E6 emitted `A1!\n` through the public path. **Verified** for byte TX and unsupported multi-byte accesses. The `0x08..0xff` routing mismatch is [G-04](public-behavior-gaps.md#g-04--native-uart-routing-window-exceeds-the-uart-declared-range). |
| Signature metadata and artifact | `.signature` discovery is host-side; absent means no artifact, zero-size means an empty artifact, and bytes are returned post-run without guest execution. | Loader records `SignatureInfo`; `dump_signature` reads the region after exit/timeout and `ExecutionResult` carries address/data. Read errors are currently swallowed by `.ok().flatten()` in result construction. | `test_dump_signature_none` and `test_dump_signature_zero_size`: **strong** helper behavior; `test_load_and_run_with_signature` and persistent `public_behavior::signature_bytes_are_returned_after_public_execution`: **strong** address/byte assertions; `test_execution_result_signature` remains a weak manual-result test. | E2 and E6 public runs returned address/length and exact bytes. **Verified** for absent/zero helper cases and a nonzero public artifact. Error propagation remains [G-06](public-behavior-gaps.md#g-06--signature-read-errors-are-suppressed-in-run-results). |
| Commit-log format | `--log-commits` retains the Spike-shaped hart/privilege/PC/opcode/register format; memory suffixes must not be advertised as current public output unless emitted. | `CommitLogger::log_commit` formats register changes and optional `MemoryAccess`; public loop re-fetches an opcode and passes `mem_access = None`. | `test_log_commit_format`, register-change tests, and memory formatter tests are **strong** for supplied inputs; the current `test_log_commit_path_with_logger` is **strong** for the result and emitted line count/PC shape but deliberately does not assert the known-bad opcode or missing suffix; persistent `public_behavior::public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` inspects every actual public line. | E4 and E6 inspected actual files. Base-zero opcode/register lines match the format; nonzero-base opcodes were zero and stores had no memory suffix. **Reproduced defects**; see [G-03](public-behavior-gaps.md#g-03--public-commit-log-loses-opcodes-for-nonzero-elf-bases) and [G-09](public-behavior-gaps.md#g-09--public-commit-log-omits-memory-access-suffixes). |
| Verbose diagnostics | Verbose mode may expose load/run diagnostics without changing the result. | `src/main.rs` prints the user option value before `load_and_run`; executor prints selected ELF tohost later. | `test_cli_run_verbose_flag` remains a **weak** CLI no-panic check; current `test_load_and_run_verbose_output` is **strong** for the bounded successful result but does not assert diagnostic text. | E5 showed the initial address was `0x0` even though the selected ELF address was `0x30001000`. The execution result was still bounded. **Reproduced defect** for diagnostic accuracy; see G-05. |

### Flat-library state, memory helpers, and configuration differences

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| `RiscVSimulator` construction and lifecycle | Keep the public wrapper, load/step/run methods, configured/default limit concept, and helper names while configurations remain distinct. | `src/executor.rs::RiscVSimulator` owns a `RiscvCore`, `SimpleMemory`, `tohost`, limit, signature, and verbose flag; `load_elf` reconstructs the flat memory/core. | Persistent `public_behavior::flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` asserts entry, helper bytes, CLI success, and flat-run timeout; older lifecycle tests remain smoke/medium checks. | E3 and E6 exercised load/run on the same ELF as the CLI. The wrapper is available, but its ELF tohost behavior diverges. **Reproduced defect/configuration gap**, G-01. |
| State inspection and mutation | `state()` exposes current state and `state_mut()`/core helpers remain available to library callers. | `RiscVSimulator::state`, `state_mut`, `reset_core`, `step_once`, and `get_core_state`; `CoreState` exposes PC, registers, privilege, CSR/FPU fields. | `test_simulator_state_access`, `test_simulator_state_mut`, `test_core_initialization`, `test_core_reset`: **medium** for access/defaults; mutation effects are not asserted. | API access and defaults passed in E1/E3. **Verified** for surface availability and reset/default observations; cross-run state isolation and arbitrary mutation effects are **Unverified**. |
| Flat memory construction and loading | `SimpleMemory` provides thread-safe flat bytes, typed little-endian reads/writes, and `load_program` loads relative to offset 0; its base argument is compatibility-only. | `src/memory/mod.rs::SimpleMemory`; all typed methods enforce natural alignment except byte access; `load_program` ignores `_base_addr`. | `test_memory_read_write`, `test_memory_misaligned`, `test_load_program`, and executor typed sign/zero-extension tests: **strong** for tested values. | E1 and E3 read/write round-trips passed. **Verified** for tested aligned/byte/relative behavior; bounds and overflow edges are not exhaustive. |
| `read_mem` / `write_mem` helpers | Public byte-oriented helpers should return requested bytes or a bounded error and should permit flat-memory writes. | `write_mem` loops byte writes; `read_mem` opportunistically uses dword/word/half reads and falls back to bytes. | `test_simulator_read_write_mem` and `test_simulator_write_mem_large` are **strong** for in-range data; persistent `public_behavior::flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` asserts an in-range round-trip; persistent `out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped` is a bounded known-defect reproduction. | E3 and E6 in-range round-trips passed. The out-of-range aligned read failed to return within two seconds; the child was killed and reaped. **Reproduced defect**, G-02. |
| CLI versus flat-library configuration | The CLI may use native RAM/UART/HTIF mapping while the flat wrapper uses its own explicit configuration; evidence must not merge them. | CLI: `SystemBus`, RAM base at ELF lowest `p_vaddr`, UART `0x10000000`, fixed HTIF callback, core reset base 0. Library: relative `SimpleMemory`, core reset with ELF base, no `SystemBus` UART/HTIF device map. | `test_system_bus_configs` and `test_memory_adapter_*` are component evidence; persistent `public_behavior::flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` is **strong** for the CLI/library contrast. | E2/E3/E6 show the distinction directly: the same ELF succeeds via CLI but times out in the wrapper because its absolute tohost address is read against relative memory. **Verified configuration difference; reproduced library gap is G-01.** |
| ISA/path boundary | A passing component instruction test is not a claim that the public ELF path supports that extension end to end. | `RiscvCore::step` uses the active decoder/executor; RV64C/MMU/TLM/peripheral components are not all wired into this public loop. | The 809 library tests include many component tests; the host-only `test_add_direct` run was skipped, while the Docker run executed the add ELF and the 46-file runner set. | **Unverified** for extension-wide or external compliance support. The 46 guest programs are bounded project-authored evidence, not a compliance claim. |

## Known stale or non-authoritative inputs

- `tests/bare-metal-riscv-test/README.md` describes `rv64i/add.elf` as returning
  exit code 55, while `rv64i/add.S` writes zero on its pass path and
  `scripts/run_elf_tests.sh` expects zero. The source and runner script win for
  current behavior; the README text is not compatibility evidence.
- The checked-in `tests/reference-logs/*.log.ref` files are historical Spike-side
  comparison inputs. They are not current ruscv-sim output and do not repair the
  public-loop logging gaps above.

## Bounded follow-up proposal (not a new active plan)

The second batch has made the scoped transient evidence persistent. A subsequent
bounded decision or repair scope should:

1. Decide separately whether G-01, G-02, G-03, G-04, G-05, G-06, and G-09 are
   production-repair scope or documented compatibility limitations. The new tests
   intentionally do not make that decision.
2. If a repair is approved, retain the persistent tests as regressions and add
   only the minimum implementation change needed for the selected gap; do not
   silently alter address maps, limit semantics, APIs, or logging contracts.
3. Add focused evidence for the remaining unverified boundaries only when their
   intended contract is defined: default-limit exhaustion, tohost symbol fallback,
   signature read failures, the UART boundary, and broader error classification.
4. Re-run the guest suite with the project Docker image or a compatible host
   toolchain, and record actual ELF outputs rather than relying on CI definitions
   or old logs.

This proposal does not start a second milestone and does not claim that A1 is
complete.
