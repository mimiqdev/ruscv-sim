# Public Behavior Compatibility Matrix

**Status:** Current evidence record

**Authority:** Informational; scope is defined by the [active A1 contract](../dev-plan.md), while compatibility requirements are stated in [ADR-0003 §9](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product)

**Last verified:** 2026-09-07

**Revision examined:** `b20e531af7abd5fa89d1b11d224b100709a95869`

This is the first A1 delivery batch: an as-is public behavior inventory and
 evidence record. It is not an A1 closeout, a target-architecture implementation
claim, or an approval to repair production behavior. Source code and executed
 tests are the authority for current behavior; accepted ADR text is the contract
 against which a mismatch is recorded. An observed error is never a compatibility
 promise.

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
| `cargo test --all-features --lib` | **809 passed** (**pre-warning-fix baseline**) | Unit/component evidence before the documentation-only warning repair; not rerun in this pass. |
| `cargo test --all-features --test test_add_direct -- --nocapture` | Test process passed, but `test_add_program` **skipped** because `riscv64-unknown-elf-as` was unavailable (**host baseline**) | No project-authored guest ELF was executed by this host run. |
| `./scripts/compile_riscv_tests.sh` | Exit 1: `riscv64-unknown-elf-as` unavailable (**host baseline**) | Host-only guest fixture compilation was unavailable. |
| `./scripts/run_elf_tests.sh` | Exit 1: no generated `.elf` files (**host baseline**) | Host-only guest suite did not run. |
| `docker run --rm --init -v "$PWD:/workspace" -w /workspace ruscv-sim-dev bash -c 'export PATH=/opt/riscv/bin:$PATH; ./scripts/compile_riscv_tests.sh && cargo test --all-features --test test_add_direct -- --nocapture && ./scripts/run_elf_tests.sh'` | Exit 0: 46 ELF files compiled; `test_add_program` passed with exit code 0/cycles 53; ELF runner passed **46/46** (**pre-warning-fix guest evidence**) | Docker supplied the missing cross-toolchain and Spike; this guest run was not repeated during the warning-only repair. |

The initial repository quality gate recorded 77 rustdoc warnings. The current
warning-only repair passed both ordinary and `-D warnings` rustdoc builds, plus
27 applicable doctests and formatting. The check, Clippy, full Rust test, focused
test, and 809-library-test rows marked pre-warning-fix are retained as prior
baseline evidence, not presented as reruns. Likewise, the Docker guest row is
prior guest evidence and was not repeated during this documentation pass. The
repository's old CI definitions and checked-in reference logs are not substituted
for an A1 run.

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

## Compatibility matrix

### CLI, API inputs, ELF loading, and limits

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| CLI command and options | Retain `run <ELF_FILE>`, `--max-cycles`, hexadecimal-or-decimal `--tohost`, `--verbose`, and optional `--log-commits`. | `src/main.rs`: `Commands::Run`, `parse_addr`, `run_elf`, and `print_result`; `src/executor.rs::load_and_run_file`. | `test_cli_help`, `test_cli_run_help`, `test_cli_run_hex_tohost`, `test_cli_run_decimal_tohost`, and flag-combination tests. Help/parser checks are **strong** for syntax; run-flag tests are mostly **weak** because they do not assert result/output. | Help output and real option runs completed. **Verified** for the named CLI surface; execution claims are covered by the rows below. |
| Input and error boundary | Invalid/missing input must remain distinguishable from a guest result; `load_and_run` exposes loader errors. | `ElfLoader::load` validates ELF identity; `load_and_run_file` maps file reads to `ExecutorError::ElfLoadError`; CLI exits 1 on `Err`. | `test_cli_run_missing_elf`, `test_cli_run_invalid_elf`, `test_load_and_run_invalid_elf`, `test_load_and_run_truncated_elf`: **strong** error-status assertions. | Focused tests passed. **Verified** for the exercised invalid inputs; malformed ELF coverage is not exhaustive. |
| `ExecutionResult` and terminal presentation | Preserve exit code, completed cycle count, final PC, timeout/error distinction, and optional signature artifact; CLI prints the result and exits with `exit_code`. | `src/executor.rs::ExecutionResult` and `load_and_run`; `src/main.rs::print_result` and `std::process::exit(result.exit_code as i32)`. | `test_execution_result_defaults`, `test_execution_result_custom`, `test_execution_result_timeout`, and `test_execution_result_signature` are **weak-to-medium** because most values are manually constructed. | E2 observed actual exit/cycle/PC/timeout values and process statuses. **Verified** for the listed paths; broad error taxonomy remains unverified. |
| Accepted ELF input | Current loader accepts ELF64, little-endian, `ET_EXEC`, machine `EM_RISCV`, and rejects other identity fields. | `src/elf.rs::ElfLoader::load`. | `src/elf.rs::test_elf_load`, `test_load_elf_file_function`: **strong** for hand-built valid ELF; invalid-input tests are **strong** for rejection. | E2 used valid ELF64/RISC-V inputs. **Verified** for this profile, not for arbitrary ELF variants. |
| PT_LOAD file bytes | Load each PT_LOAD's `p_filesz` bytes at its `p_vaddr`-relative location. | `ElfLoader::memory_footprint`, `load_into_memory`, and `load_elf_file` create a relative buffer and copy each segment. | `test_load_into_memory` and `test_load_elf_file_function`: **strong** for two segments and selected bytes, but not all overlap/permission cases. | E2 guest file-byte check passed. **Verified** for the exercised segment layout; permissions and overlap behavior are unverified. |
| PT_LOAD zero-fill | Bytes from `p_filesz` through `p_memsz` must be represented as zero-filled memory. | `load_into_memory` zeroes the tail of each segment; the initial `Vec` is also zeroed. | Existing loader test checks initial values but does not assert a `p_filesz < p_memsz` byte directly (**weak** for this requirement). | E2 `zero-fill.elf` loaded a file byte and a distinct zero-filled byte and exited 0. **Verified** for that layout. |
| Entry point | Preserve ELF `e_entry` as the guest PC; do not replace it with the storage base. | `ElfLoader::entry_point`; CLI calls `core.reset(entry_point, 0)`, while the library calls `core.reset(entry_point, base_addr)`. | `test_elf_load` and `test_load_elf_file_function`: **strong** entry assertions; generic execution tests are weaker. | E2 entry-offset fixture executed from `0x20000100` and returned final PC `0x2000010c`. **Verified** for a non-base entry. |
| Nonzero load base and address meaning | Preserve guest addresses while allowing the current flat image to be stored relative to its lowest load address. | CLI stores relative bytes in `SimpleMemory`, routes them through `SystemBus(ram_base=base_addr)`, and resets the core with base 0; `MemoryAdapter` therefore passes guest addresses unchanged on CLI. | `test_memory_adapter_va_to_pa` and `test_memory_adapter_different_base`: **strong** component assertions; `test_add_program` would be public-path evidence but was skipped. | E2 executed at `0x80000000` and passed. **Verified** for execution; commit logging has the reproduced nonzero-base opcode defect in E4. |
| RAM effects | Guest RAM stores and loads must affect the same public RAM image. | `SystemBus` routes RAM addresses to `SimpleMemory`; `RiscvCore::step` uses the bus for instruction and data memory. | `test_system_bus_routing`: **strong** typed RAM round-trip; most `load_and_run` tests are **weak**. | E2 `ram.elf` stored and loaded a value before exit. **Verified** for the exercised aligned dword path. |
| Default limit | Keep the default maximum at 10,000,000 completed loop iterations/cycles in the compatibility path; this is not `mcycle`/`minstret` or virtual time. | `DEFAULT_MAX_CYCLES = 10_000_000`; `Option::unwrap_or` in both `load_and_run` and `RiscVSimulator::run`. | `test_simulator_run_default_cycles`: **weak**; it fails quickly on an empty instruction and does not reach the default bound. | E2 no-option run exited before the bound. The numeric exhaustion boundary was not run. **Unverified** for default-limit exhaustion. |
| Zero limit | `--max-cycles 0` performs no instruction and returns a timeout result with zero cycles. | `while cycles < max_cycles` is skipped; timeout result is built with `cycles = 0`. | `test_load_and_run_zero_cycles`: **weak** (`Ok` or `Err`); no strong existing assertion. | E2 observed `TIMEOUT`, cycles 0, final PC at entry. **Verified**. |
| Exact finite limit | A nonzero finite limit permits at most that many successful loop iterations; an exit on the last permitted instruction remains observable. | `cycles` increments immediately after `core.step`; tohost is checked before the loop condition is evaluated again. | `test_simulator_run_with_max_cycles` and large-limit tests are **weak**; they do not assert exact count. | E2 max 5 timed out at 5; max 6 observed exit at 6. **Verified** for exact-limit and final-slot ordering. |
| Execution error count | A failed `core.step` returns an error result without incrementing `cycles`; timeout is separate from execution error. | `load_and_run` increments only in the `Ok(())` arm and returns an error-shaped `ExecutionResult` in the `Err` arm. | `test_execution_result_error` is a **weak** manually constructed result; invalid-instruction public execution was not asserted with exact fields. | E2 zero/limit cases verified timeout counts; exact public execution-error count remains **Unverified**. |

### Exit, tohost, UART, signature, and logs

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| Exit encodings | Recognize the supported standard HTIF payload and the project alternative high-bit marker; report the decoded guest code. | `try_extract_exit_code` handles `(payload >> 1)` for standard `device=0/cmd=0` and `(1<<63) \| code` alternative. | `executor::test_htif_exit_code_extraction`, `test_htif_exit_code_alternative_format`, `test_htif_exit_code_standard_format`: **strong** for pure extraction; indirect wrapper tests are weaker. | E2 returned 0, 1, and 42 with matching process statuses. **Verified** for supported encodings. |
| Nonzero exit | A guest nonzero exit is a guest result, not a loader error; CLI propagates the code as its process status. | `ExecutionResult.exit_code` is set from tohost; CLI calls `exit(result.exit_code as i32)`. | Existing tests do not assert an actual CLI nonzero guest status (**weak/missing**). | E2 code 1 and code 42 produced statuses 1 and 42. **Verified** for the transient programs; codes outside host process range are unverified. |
| To-host precedence | `tohost_addr` precedence is CLI override, then ELF `.tohost`/symbol, then fixed `0x40008000`. | `load_and_run`: `tohost_addr.or(elf_tohost).unwrap_or(DEFAULT_TOHOST)`; loader discovers `.tohost` then `tohost` symbol. | `test_load_and_run_tohost_override`, `test_load_and_run_default_tohost`: **weak**; `test_tohost_symbol_from_elf` and `test_fib_tohost_address` skip when guest ELFs are absent. | E2 selected ELF address with no override, selected the first CLI RAM override, and timed out when explicitly selecting fixed HTIF for a guest that did not write it. **Verified** for section metadata and CLI override. Symbol fallback remains **Unverified** in this environment. |
| HTIF endpoint and final observation | Fixed `0x40008000` dword writes invoke the callback; selected RAM tohost is polled after each successful instruction and a recognized value is cleared. | `SystemBus::write_dword/read_dword`, HTIF callback, `load_and_run` post-success polling, and `clear_tohost`. Byte/half/word fixed-HTIF access is rejected. | `test_system_bus_read_htif`, `test_system_bus_write_htif`: **medium** (read/value or `Ok`, but callback value is not asserted); `test_system_bus_*_uart_*` are strong for width rejection. | E2 fixed-endpoint and RAM-endpoint exits were observed, including final-slot exit. Post-run clearing of a selected RAM signal is source-supported but has no public assertion. **Unverified** for retained memory state after return. |
| UART public output | Public ELF path retains byte MMIO at `0x10000000`, output callback bytes, and rejection of multi-byte UART register access. | `SystemBus` routes byte accesses to `Uart16550`; UART callback is installed in `load_and_run`; its native window is `0x100` while the UART/TLM model declares `UART_SIZE = 8`. | `test_system_bus_routing`: **strong** byte register round-trip; `test_system_bus_read/write_uart_{half,word,dword}`: **strong** rejection; UART unit tests are component evidence. | E2 emitted `A1!\n` through the public path. **Verified** for byte TX and unsupported multi-byte accesses. The `0x08..0xff` routing mismatch is [G-04](public-behavior-gaps.md#g-04--native-uart-routing-window-exceeds-the-uart-declared-range). |
| Signature metadata and artifact | `.signature` discovery is host-side; absent means no artifact, zero-size means an empty artifact, and bytes are returned post-run without guest execution. | Loader records `SignatureInfo`; `dump_signature` reads the region after exit/timeout and `ExecutionResult` carries address/data. Read errors are currently swallowed by `.ok().flatten()` in result construction. | `test_dump_signature_none` and `test_dump_signature_zero_size`: **strong** helper behavior; `test_execution_result_signature`: **weak** (manual result); no existing public nonzero-byte assertion. | E2 public run reported address/length and the temporary API harness returned the exact eight bytes. **Verified** for absent/zero helper cases and a nonzero public artifact. Error propagation remains [G-06](public-behavior-gaps.md#g-06--signature-read-errors-are-suppressed-in-run-results). |
| Commit-log format | `--log-commits` retains the Spike-shaped hart/privilege/PC/opcode/register format; memory suffixes must not be advertised as current public output unless emitted. | `CommitLogger::log_commit` formats register changes and optional `MemoryAccess`; public loop re-fetches an opcode and passes `mem_access = None`. | `test_log_commit_format`, register-change tests, memory formatter tests, and `test_log_commit_path_with_logger`: formatter tests are **strong** for supplied inputs; public path test is **weak** because it does not inspect the file. | E4 inspected actual files. Base-zero opcode/register lines match the format; nonzero-base opcodes were zero and stores had no memory suffix. **Reproduced defects**; see [G-03](public-behavior-gaps.md#g-03--public-commit-log-loses-opcodes-for-nonzero-elf-bases) and [G-09](public-behavior-gaps.md#g-09--public-commit-log-omits-memory-access-suffixes). |
| Verbose diagnostics | Verbose mode may expose load/run diagnostics without changing the result. | `src/main.rs` prints the user option value before `load_and_run`; executor prints selected ELF tohost later. | `test_cli_run_verbose_flag`, `test_load_and_run_verbose_output`: **weak** no-panic tests. | E5 showed the initial address was `0x0` even though the selected ELF address was `0x30001000`. The execution result was still bounded. **Reproduced defect** for diagnostic accuracy; see G-05. |

### Flat-library state, memory helpers, and configuration differences

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| `RiscVSimulator` construction and lifecycle | Keep the public wrapper, load/step/run methods, configured/default limit concept, and helper names while configurations remain distinct. | `src/executor.rs::RiscVSimulator` owns a `RiscvCore`, `SimpleMemory`, `tohost`, limit, signature, and verbose flag; `load_elf` reconstructs the flat memory/core. | `test_simulator_creation`, `test_simulator_invalid_elf`, `test_simulator_load_elf_file`, `test_simulator_step`, `test_simulator_run_until_exit`: mostly **weak** (no panic or generic `Ok`). | E3 exercised load/run on the same ELF as the CLI and a manually flat-offset program. The wrapper is available, but its ELF tohost behavior diverges. **Reproduced defect/configuration gap**, G-01. |
| State inspection and mutation | `state()` exposes current state and `state_mut()`/core helpers remain available to library callers. | `RiscVSimulator::state`, `state_mut`, `reset_core`, `step_once`, and `get_core_state`; `CoreState` exposes PC, registers, privilege, CSR/FPU fields. | `test_simulator_state_access`, `test_simulator_state_mut`, `test_core_initialization`, `test_core_reset`: **medium** for access/defaults; mutation effects are not asserted. | API access and defaults passed in E1/E3. **Verified** for surface availability and reset/default observations; cross-run state isolation and arbitrary mutation effects are **Unverified**. |
| Flat memory construction and loading | `SimpleMemory` provides thread-safe flat bytes, typed little-endian reads/writes, and `load_program` loads relative to offset 0; its base argument is compatibility-only. | `src/memory/mod.rs::SimpleMemory`; all typed methods enforce natural alignment except byte access; `load_program` ignores `_base_addr`. | `test_memory_read_write`, `test_memory_misaligned`, `test_load_program`, and executor typed sign/zero-extension tests: **strong** for tested values. | E1 and E3 read/write round-trips passed. **Verified** for tested aligned/byte/relative behavior; bounds and overflow edges are not exhaustive. |
| `read_mem` / `write_mem` helpers | Public byte-oriented helpers should return requested bytes or a bounded error and should permit flat-memory writes. | `write_mem` loops byte writes; `read_mem` opportunistically uses dword/word/half reads and falls back to bytes. | `test_simulator_read_write_mem` and `test_simulator_write_mem_large`: **strong** for in-range data; `test_simulator_read_mem_unaligned`: **weak** (`Ok` or `Err`). | In-range round-trip passed. Out-of-range `read_mem` failed to return under a two-second bound. **Reproduced defect**, G-02. |
| CLI versus flat-library configuration | The CLI may use native RAM/UART/HTIF mapping while the flat wrapper uses its own explicit configuration; evidence must not merge them. | CLI: `SystemBus`, RAM base at ELF lowest `p_vaddr`, UART `0x10000000`, fixed HTIF callback, core reset base 0. Library: relative `SimpleMemory`, core reset with ELF base, no `SystemBus` UART/HTIF device map. | `test_system_bus_configs`, `test_memory_adapter_*`, and wrapper tests are **component/weak** for the distinction. | E2/E3 show the distinction directly: same ELF succeeds via CLI but times out/errors via wrapper until a flat offset tohost is configured. **Verified configuration difference; reproduced library gap is G-01.** |
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

The next delivery batch should remain limited to evidence quality:

1. Add a small, persistent fixture/test set for the E2 scenarios whose current
   proof is transient: zero-fill, non-base entry, exact/final-slot limits,
   precedence, UART bytes, and signature bytes.
2. Turn the E3/G-02 reproductions into focused regression tests only after the
   intended disposition is approved; do not silently repair production behavior
   in the evidence batch.
3. Decide separately whether G-01, G-03, G-04, G-05, G-06, and G-09 are repair
   scope or documented compatibility gaps. Until then, none is a supported
   behavior.
4. Re-run the guest suite only when the RISC-V cross-toolchain is available, and
   record actual ELF outputs rather than relying on CI definitions or old logs.

This proposal does not start a second milestone and does not claim that A1 is
complete.
