# Public Behavior Compatibility Matrix

**Status:** Current evidence record

**Authority:** Informational as-of A1 evidence; original scope is preserved in the [archived A1 contract](../archive/milestones/a1-public-behavior-baseline.md), while compatibility requirements are stated in [ADR-0003 §9](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product)

**Last reviewed:** 2026-09-11; A1 rows retain their 2026-09-08 recorded scope. A2 T1 and T2 update the `read_mem` helper row, the flat-library lifecycle/config rows and the G-01/G-02 test names; A2 T3 updates the signature-artifact rows.

**Committed evidence snapshot:** `b16a7872cf62e51dc204d9ea122b6c5223c15f96`
(first submitted PR14 test snapshot)

**Merged evidence:** E7's corrections were committed at
`54bfdbf11491caf7e78565ddf62cc1e7e13c4e67` and merged in
`fae4759e09c6750e4105c8be7c287e841dabf0dc`. E1–E5 retain historical evidence
from the `b20e531` source baseline and the documentation-only warning repair at
`64c6f976`; E6 records the initial PR14 snapshot; E7 records checks collected on
the follow-up working tree before its commit. The
[A1 acceptance assessment](a1-acceptance-assessment.md) records the 2026-09-08
post-merge verification and links to the final accepted residual-gap dispositions. This is not an A1 closeout, a target-architecture
implementation claim, or an approval to repair production behavior. Source code
and executed tests are the authority for current behavior; accepted ADR text is
the contract against which a mismatch is recorded. An observed error is never a
compatibility promise.

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

At the committed evidence snapshot above; follow-up commands are recorded in E7:

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
| `cargo test --all-features --test executor -- --nocapture` | **57 passed** (**committed PR14 snapshot `b16a787`**) | Strengthened public `load_and_run` result, zero-limit, file-path, no-logger, and logger-artifact assertions; no production source changed. |
| `cargo test --all-features` | Exit 0 (**committed PR14 snapshot `b16a787`, before E7**); **809** library tests, **57** executor tests, **14** persistent public-behavior tests, **27** doctests, and all reported integration suites passed | The existing generated `add.elf` allowed `test_add_program` to pass in this run; this does not establish that the host cross-assembler/linker is installed. |
| `cargo test --all-features --lib` | **809 passed** (**pre-warning-fix baseline**) | Unit/component evidence before the documentation-only warning repair; not rerun in this pass. |
| `cargo test --all-features --test test_add_direct -- --nocapture` | Test process passed, but `test_add_program` **skipped** because `riscv64-unknown-elf-as` was unavailable (**host baseline**) | No project-authored guest ELF was executed by this host run. |
| `./scripts/compile_riscv_tests.sh` | Exit 1: `riscv64-unknown-elf-as` unavailable (**host baseline**) | Host-only guest fixture compilation was unavailable. |
| `./scripts/run_elf_tests.sh` | Exit 1: no generated `.elf` files (**host baseline**) | Host-only guest suite did not run. |
| `docker run --rm --init -v "$PWD:/workspace" -w /workspace ruscv-sim-dev bash -c 'export PATH=/opt/riscv/bin:$PATH; ./scripts/compile_riscv_tests.sh && cargo test --all-features --test test_add_direct -- --nocapture && ./scripts/run_elf_tests.sh'` | Exit 0: 46 ELF files compiled; `test_add_program` passed with exit code 0/cycles 53; ELF runner passed **46/46** (**pre-warning-fix guest evidence**) | Docker supplied the missing cross-toolchain and Spike; this guest run was not repeated during the warning-only repair. |
| `cargo fmt --all -- --check` | Exit 0 (**committed PR14 snapshot `b16a787`, pre-E7**) | Formatting passed after adding the persistent fixture/test files. |
| `cargo check --all-features` | Exit 0 (**committed PR14 snapshot `b16a787`, pre-E7**) | All-feature compilation passed with the persistent fixture/test files. |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0 (**committed PR14 snapshot `b16a787`, pre-E7**) | Strict Clippy passed, including integration-test targets. |
| `cargo test --all-features` | Exit 0 (**committed PR14 snapshot `b16a787`, pre-E7**); **809** library tests, **14** persistent public-behavior tests, and all reported integration suites passed; `test_add_program` returned from its host-toolchain skip path | Host has no RISC-V cross assembler/linker, so this is not host guest-ELF execution evidence. The dedicated persistent fixture suite is self-contained. |
| `cargo doc --all-features --no-deps` | Exit 0 with zero warnings (**committed PR14 snapshot `b16a787`, pre-E7**) | Documentation remains warning-free after the persistent test additions. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Exit 0 with zero warnings (**committed PR14 snapshot `b16a787`, pre-E7**) | Strict rustdoc remains clean. |
| `cargo test --all-features --doc` | **27 passed** (**committed PR14 snapshot `b16a787`, pre-E7**) | All applicable doctests passed. |
| `docker run --rm --init -v "$PWD:/workspace" -w /workspace ruscv-sim-dev bash -c 'export PATH=/opt/riscv/bin:$PATH; ./scripts/compile_riscv_tests.sh && cargo test --all-features --test test_add_direct -- --nocapture && ./scripts/run_elf_tests.sh'` | Exit 0: 46 ELF files compiled; `test_add_program` passed with exit code 0/cycles 53; ELF runner passed **46/46** (**guest evidence before final executor-test strengthening**) | The Docker run predates the final executor assertion strengthening and was not rerun by the independent PR14 reviewer or in the E7 follow-up. Production code was unchanged; retain this as prior guest evidence, not exact-head reviewer evidence. It was repeated through `newgrp docker` because the host shell lacked direct Docker-socket permission. |

The initial repository quality gate recorded 77 rustdoc warnings. The
warning-only repair passed both ordinary and `-D warnings` rustdoc builds, plus
27 applicable doctests and formatting. The current second-batch rows record the
same quality gate with the persistent fixtures/tests present; the focused
executor and full-test rows record the committed PR14 snapshot; and the older
pre-warning-fix rows remain historical baseline evidence. The committed full run
used an existing generated `add.elf`, while the host-only compile limitation and
the prior Docker row remain recorded separately. The E7 follow-up quality gate
is recorded below. The repository's old CI definitions and checked-in reference
logs are not substituted for an A1 run.

During the stacked PR14 reviews, its base was `a1-public-behavior-inventory`,
not `main`. The workflow's `pull_request.branches: [main]` filter meant no checks,
not pending/passing CI. PR14 subsequently merged into `main`; the acceptance
assessment records the successful exact-merge push CI, including guest tests.
That later result does not rewrite these historical review-stage observations.

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

The committed PR14 snapshot adds `tests/common/public_elf.rs` and
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

| Test | Assertion in the committed snapshot |
| --- | --- |
| `elf_segments_preserve_file_bytes_and_zero_fill` | Loader entry/base, code bytes, a file-backed byte, signature metadata/data, a zero byte at `0x2fff`, and allocated memory size; the probe was inside the minimum 64 KiB allocation and did not prove `p_memsz`. Corrected in E7. |
| `public_zero_fill_is_observed_by_guest_execution` | Guest loaded the zero-filled probe at `0x80002fff` and exited code 0 in 6 cycles, but the probe was inside the minimum allocation and could not detect an ignored `p_memsz`. Corrected in E7. |
| `public_entry_and_nonzero_base_execute_from_the_declared_entry` | Guest entry offset `0x100`, nonzero base, exit code, exact cycle count, final PC, and timeout/error state. |
| `public_ram_store_and_load_affect_the_same_image` | Guest stores and reloads an exit payload through RAM, then exits code 42 after 8 cycles. |
| `default_limit_path_returns_an_early_guest_exit` | `None` limit follows the default-limit path and observes an early exit; it does not claim the 10,000,000-cycle exhaustion boundary. |
| `exact_limits_include_zero_and_the_final_exit_slot` | Exact zero, pre-exit, and final-slot limits assert cycles, PC, timeout text, and successful exit. |
| `cli_propagates_zero_and_nonzero_guest_exit_codes` | CLI process statuses 0 and 42 plus printed result fields. |
| `alternative_exit_encoding_is_observed_on_the_public_path` | Public alternative high-bit exit encoding returns code 42 in 5 cycles. |
| `tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint` | ELF section selection, CLI RAM override, and fixed-endpoint timeout are distinguished by code/cycles/status. |
| `uart_bytes_are_emitted_by_the_cli_elf_path` | CLI stdout contains the exact `A1!` plus newline bytes and a successful result. |
| `signature_bytes_are_returned_after_public_execution` | Public result contains signature address `0x80002000` and exact bytes `[0, 17, 34, 51, 68, 85, 102, 119]`. |
| `flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted` | A1 reproduction of G-01: flat `write_mem`/`read_mem` bytes round-trip; the same ELF exits in the CLI but timed out in `RiscVSimulator`. Replaced in A2 T2 by `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal`. |
| `public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` | Actual public log has four lines with zero opcodes and no `mem` suffix, reproducing G-03/G-09. |
| `out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped` | A1 child-process hang reproduction for G-02; replaced in A2 T1 by `out_of_range_flat_read_mem_returns_error_without_hanging`. |

Most of these tests are stronger evidence than the older smoke tests because
they assert state, process status, output, exact bytes, exact cycle/PC values, or
actual log artifacts. The two committed zero-fill assertions had a coverage
blind spot: their probe was inside the allocator's minimum window. E7 adds a
public extent probe and a component prefilled-buffer test to separate `p_memsz`
coverage from explicit BSS clearing. The known-defect tests intentionally
document incorrect observations and are linked to the gap register; they do not
convert those observations into compatibility promises.

The committed PR14 snapshot also strengthens the public `load_and_run` coverage
in `tests/executor.rs`. The former `test_load_and_run_small_memory` was renamed
to `test_load_and_run_with_one_cycle_budget` because `load_and_run` does not expose
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

The focused executor command passed **57 tests** in the committed PR14 snapshot:

```text
cargo test --all-features --test executor -- --nocapture
```

In the committed 14-test snapshot, the first 11 rows above were intended as
compatibility characterization, but the two zero-fill rows had the limitation
noted above. E7 corrects that limitation. The public commit-log observation
(G-03/G-09) remains a defect reproduction, not a compatibility pass. The A1
flat-library ELF contrast (G-01) and the A1 G-02 hang row were repaired in A2
T1/T2 by the error-return and correct-exit regressions named below. E7's handshake-failure and
early-exit cases still validate harness cleanup, not CLI/library equivalence.
Other legacy executor/component smoke tests remain outside this targeted
strengthening.

### E7 — Follow-up evidence corrections based on `b16a787`

These checks were collected before committing the follow-up working tree based
on `b16a7872cf62e51dc204d9ea122b6c5223c15f96`; the corrections were subsequently
committed as `54bfdbf11491caf7e78565ddf62cc1e7e13c4e67`. They changed only test
fixtures, test harness code and evidence wording; no production code, public API,
dependency, CI or address/limit behavior changed. The results below describe that
run, not a fresh verification of every later documentation edit.

#### Zero-fill evidence

The fixture now uses `p_memsz = 0x18000`, which makes the loader allocation
`0x20000` when `p_memsz` is honored, rather than the `0x10000` minimum that also
masked the earlier test. The public path test loads `0x80017fff`, beyond that
minimum window, and therefore fails if the loader ignores the segment extent.
The separate component test
`elf_loader_clears_bss_in_prefilled_memory` initializes the destination buffer
to `0x5a` before calling `ElfLoader::load_into_memory` and asserts that the same
BSS probe becomes zero. This distinguishes allocation/extent evidence from
explicit BSS clearing; neither test is evidence for the other layer.

Controlled mutations were performed only in temporary copies and removed after
the run:

| Temporary mutation | Focused command outcome |
| --- | --- |
| `memory_footprint` changed from using `p_memsz` to `p_filesz` | `public_zero_fill_is_observed_by_guest_execution` exited **101** with a test failure. |
| `load_into_memory` BSS loop changed from `0..mem_size` to `0..file_size` | `elf_loader_clears_bss_in_prefilled_memory` exited **101** with a test failure. |

#### G-02 harness safety

The child now writes a `ready` marker, through the `RUSCV_SIM_READ_MEM_READY`
path passed in the environment, after constructing `RiscVSimulator` and before
calling `read_mem`. The parent polls that marker with a deadline and starts the
two-second hang window only after the marker is observed. Child stdout/stderr are
piped and included in failure diagnostics. `KillOnDrop` owns every spawned child;
its normal timeout path performs `kill()` followed by `wait()`, and its `Drop`
implementation performs the same best-effort cleanup for panic/error paths. The
recursive `RUSCV_SIM_READ_MEM_CHILD` environment guard and `--exact` invocation
remain in place.

The three harness paths were each run exactly:

| Test | Result |
| --- | --- |
| `out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped` | 1 passed in about 2.01 s; ready observed before the two-second hang window, then child killed and reaped. |
| `out_of_range_flat_read_mem_handshake_failure_is_reaped` | 1 passed in about 0.25 s; missing ready was bounded and child was cleaned up. |
| `out_of_range_flat_read_mem_early_exit_is_reaped` | 1 passed in about 0.01 s; child exit before ready was detected, diagnostics retained, and child was reaped. |

The post-run process check found no matching `public_behavior` read-mem child
processes. The full follow-up public-behavior run passed **17 tests** (the
original 14 plus the prefilled-BSS test and two harness-path tests):

```text
cargo test --all-features --test public_behavior -- --nocapture
```

The follow-up quality checks were:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0. |
| `cargo check --all-features` | Exit 0. |
| `cargo clippy --all-features --all-targets -- -D warnings` | Exit 0. |
| `cargo test --all-features --test executor -- --nocapture` | **57 passed**. |
| `cargo test --all-features` | Exit 0; **809** library tests, **57** executor tests, **17** public-behavior tests, **27** doctests, and all reported integration suites passed. The existing generated `add.elf` allowed `test_add_program` to pass; this is not host cross-toolchain evidence. |
| `cargo doc --all-features --no-deps` | Exit 0 with zero warnings. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Exit 0 with zero warnings. |

The follow-up did not rerun Docker. The 46/46 guest result in E1 predates the
final executor-test strengthening and was not independently rerun by the PR14
reviewer; production code was unchanged, so it remains prior guest evidence,
not exact-follow-up-head evidence.

## Compatibility matrix

### CLI, API inputs, ELF loading, and limits

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| CLI command and options | Retain `run <ELF_FILE>`, `--max-cycles`, hexadecimal-or-decimal `--tohost`, `--verbose`, and optional `--log-commits`. | `src/main.rs`: `Commands::Run`, `parse_addr`, `run_elf`, and `print_result`; `src/executor.rs::load_and_run_file`. | `test_cli_help`, `test_cli_run_help`, `test_cli_run_hex_tohost`, `test_cli_run_decimal_tohost`, and flag-combination tests. Help/parser checks are **strong** for syntax; run-flag tests are mostly **weak** because they do not assert result/output. | Help output and real option runs completed. **Verified** for the named CLI surface; execution claims are covered by the rows below. |
| Input and error boundary | Invalid/missing input must remain distinguishable from a guest result; `load_and_run` exposes loader errors. | `ElfLoader::load` validates ELF identity; `load_and_run_file` maps file reads to `ExecutorError::ElfLoadError`; CLI exits 1 on `Err`. | `test_cli_run_missing_elf`, `test_cli_run_invalid_elf`, `test_load_and_run_invalid_elf`, `test_load_and_run_truncated_elf`: **strong** error-status assertions. | Focused tests passed. **Verified** for the exercised invalid inputs; malformed ELF coverage is not exhaustive. |
| `ExecutionResult` and terminal presentation | Preserve exit code, completed cycle count, final PC, timeout/error distinction, and optional signature artifact; CLI prints the result and exits with `exit_code`. | `src/executor.rs::ExecutionResult` and `load_and_run`; `src/main.rs::print_result` and `std::process::exit(result.exit_code as i32)`. | `test_execution_result_defaults`, `test_execution_result_custom`, `test_execution_result_timeout`, and `test_execution_result_signature` are **weak-to-medium** because most values are manually constructed. | E2 observed actual exit/cycle/PC/timeout values and process statuses. **Verified** for the listed paths; broad error taxonomy remains unverified. |
| Accepted ELF input | Current loader accepts ELF64, little-endian, `ET_EXEC`, machine `EM_RISCV`, and rejects other identity fields. | `src/elf.rs::ElfLoader::load`. | `src/elf.rs::test_elf_load`, `test_load_elf_file_function`: **strong** for hand-built valid ELF; invalid-input tests are **strong** for rejection. | E2 used valid ELF64/RISC-V inputs. **Verified** for this profile, not for arbitrary ELF variants. |
| PT_LOAD file bytes | Load each PT_LOAD's `p_filesz` bytes at its `p_vaddr`-relative location. | `ElfLoader::memory_footprint`, `load_into_memory`, and `load_elf_file` create a relative buffer and copy each segment. | `test_load_into_memory`, `test_load_elf_file_function`, and persistent `public_behavior::elf_segments_preserve_file_bytes_and_zero_fill`: **strong** for selected bytes; overlap/permission cases remain unverified. | E2 and E6 assert code and data bytes through the loader; E6 also executes the same fixture. **Verified** for the exercised segment layout; permissions and overlap behavior are unverified. |
| PT_LOAD zero-fill | Bytes from `p_filesz` through `p_memsz` must be represented as zero-filled memory. | `load_into_memory` zeroes the tail of each segment; the initial `Vec` is also zeroed. | E7 `public_behavior::elf_segments_preserve_file_bytes_and_zero_fill` and `public_zero_fill_is_observed_by_guest_execution` use a probe beyond the 64 KiB minimum allocation; E7 `elf_loader_clears_bss_in_prefilled_memory` supplies a nonzero buffer: **strong**, with public and component evidence separated. | E7's public probe detects an ignored `p_memsz`; its component prefill test detects removal of explicit BSS clearing. **Verified** for the exercised layout and explicit clear operation; other segment/overlap cases remain unverified. |
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
| Exit encodings | Recognize the supported standard HTIF payload and the project alternative high-bit marker; report the decoded guest code. | `try_extract_exit_code` handles `(payload >> 1)` for standard `device=0/cmd=0` and `(1<<63) \| code` alternative. | `executor::test_htif_exit_code_extraction` and persistent `public_behavior::alternative_exit_encoding_is_observed_on_the_public_path`: **strong** pure/public assertions. The legacy `test_htif_exit_code_alternative_format` and `test_htif_exit_code_standard_format` are **weak** wrapper tests: they ignore out-of-range writes and only assert `is_ok()`, so they are excluded from behavior proof. | E2 and E6 returned standard codes 0/1/42 and alternative code 42. **Verified** for the exercised public encodings; the two legacy wrapper tests are not supporting evidence. |
| Nonzero exit | A guest nonzero exit is a guest result, not a loader error; CLI propagates the code as its process status. | `ExecutionResult.exit_code` is set from tohost; CLI calls `exit(result.exit_code as i32)`. | Persistent `public_behavior::cli_propagates_zero_and_nonzero_guest_exit_codes` asserts actual process statuses 0 and 42 and output; older tests remain smoke-only. | E2 and E6 code 1/42 programs produced matching statuses. **Verified** for the persistent programs; codes outside host process range are unverified. |
| To-host precedence | `tohost_addr` precedence is CLI override, then ELF `.tohost`/symbol, then fixed `0x40008000`. | `load_and_run`: `tohost_addr.or(elf_tohost).unwrap_or(DEFAULT_TOHOST)`; loader discovers `.tohost` then `tohost` symbol. | Persistent `public_behavior::tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint` asserts each branch with exact cycles/status; `test_tohost_symbol_from_elf` and `test_fib_tohost_address` still skip when guest ELFs are absent. | E2 and E6 selected ELF address with no override, selected the first CLI RAM override, and timed out when selecting fixed HTIF for a guest that did not write it. **Verified** for section metadata and CLI override. Symbol fallback remains **Unverified** in this environment. |
| HTIF endpoint and final observation | Fixed `0x40008000` dword writes invoke the callback; selected RAM tohost is polled after each successful instruction and a recognized value is cleared. | `SystemBus::write_dword/read_dword`, HTIF callback, `load_and_run` post-success polling, and `clear_tohost`. Byte/half/word fixed-HTIF access is rejected. | `test_system_bus_read_htif`, `test_system_bus_write_htif`: **medium** (read/value or `Ok`, but callback value is not asserted); persistent `public_behavior::exact_limits_include_zero_and_the_final_exit_slot` and `tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint`: **strong** public timing/selection assertions; UART width tests are strong for rejection. | E2 and E6 fixed-endpoint and RAM-endpoint exits were observed, including final-slot exit. Post-run clearing of a selected RAM signal is source-supported but has no public assertion. **Unverified** for retained memory state after return. |
| UART public output | Public ELF path retains byte MMIO at `0x10000000`, output callback bytes, and rejection of multi-byte UART register access. | `SystemBus` routes byte accesses to `Uart16550`; UART callback is installed in `load_and_run`; its native window is `0x100` while the UART/TLM model declares `UART_SIZE = 8`. | Persistent `public_behavior::uart_bytes_are_emitted_by_the_cli_elf_path` asserts exact CLI bytes/result; `test_system_bus_routing` is strong for byte register round-trip; `test_system_bus_read/write_uart_{half,word,dword}` is strong for rejection; UART unit tests are component evidence. | E2 and E6 emitted `A1!\n` through the public path. **Verified** for byte TX and unsupported multi-byte accesses. The `0x08..0xff` routing mismatch is [G-04](public-behavior-gaps.md#g-04--native-uart-routing-window-exceeds-the-uart-declared-range). |
| Signature metadata and artifact | `.signature` discovery is host-side; absent means no artifact, zero-size means an empty artifact, and bytes are returned post-run without guest execution. | Loader records `SignatureInfo`. The CLI `load_and_run` still reads the region through `dump_signature` and swallows read errors with `.ok().flatten()`. The flat wrapper resolves the guest metadata address to a checked flat offset and reads it through the bounded helper; an unusable region yields an explicit diagnostic. | `test_dump_signature_none` and `test_dump_signature_zero_size`: **strong** helper behavior; `test_load_and_run_with_signature` and persistent `public_behavior::signature_bytes_are_returned_after_public_execution`: **strong** address/byte assertions for the CLI; `test_execution_result_signature` remains a weak manual-result test. The A2 T3 flat-library artifact tests assert bytes, address, absent/empty/unreadable outcomes and preserved accounting. | E2 and E6 public runs returned address/length and exact bytes for the CLI. **Verified** for absent/zero helper cases, a nonzero CLI artifact, and the flat-library artifact path after A2 T3. The CLI error path remains [G-06](public-behavior-gaps.md#g-06--signature-read-errors-are-suppressed-in-run-results); the repaired flat-library path was [G-12](public-behavior-gaps.md#g-12--flat-library-signature-artifact-was-read-at-the-guest-address-and-suppressed). |
| Commit-log format | `--log-commits` retains the Spike-shaped hart/privilege/PC/opcode/register format; memory suffixes must not be advertised as current public output unless emitted. | `CommitLogger::log_commit` formats register changes and optional `MemoryAccess`; public loop re-fetches an opcode and passes `mem_access = None`. | `test_log_commit_format`, register-change tests, and memory formatter tests are **strong** for supplied inputs; the current `test_log_commit_path_with_logger` is **strong** for the result and emitted line count/PC shape but deliberately does not assert the known-bad opcode or missing suffix; persistent `public_behavior::public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` inspects every actual public line. | E4 and E6 inspected actual files. Base-zero opcode/register lines match the format; nonzero-base opcodes were zero and stores had no memory suffix. **Reproduced defects**; see [G-03](public-behavior-gaps.md#g-03--public-commit-log-loses-opcodes-for-nonzero-elf-bases) and [G-09](public-behavior-gaps.md#g-09--public-commit-log-omits-memory-access-suffixes). |
| Verbose diagnostics | Verbose mode may expose load/run diagnostics without changing the result. | `src/main.rs` prints the user option value before `load_and_run`; executor prints selected ELF tohost later. | `test_cli_run_verbose_flag` remains a **weak** CLI no-panic check; current `test_load_and_run_verbose_output` is **strong** for the bounded successful result but does not assert diagnostic text. | E5 showed the initial address was `0x0` even though the selected ELF address was `0x30001000`. The execution result was still bounded. **Reproduced defect** for diagnostic accuracy; see G-05. |

### Flat-library state, memory helpers, and configuration differences

| Surface | Contract requirement | Current implementation | Existing test / assertion strength | A1 evidence and disposition |
| --- | --- | --- | --- | --- |
| `RiscVSimulator` construction and lifecycle | Keep the public wrapper, load/step/run methods, configured/default limit concept, and helper names while configurations remain distinct. | `src/executor.rs::RiscVSimulator` owns a `RiscvCore`, `SimpleMemory`, manual/image tohost selection, image base, limit, signature, and verbose flag; `load_elf` reconstructs the flat memory/core and resolves and validates the image's flat exit offset; the signature artifact placement is resolved when the result is built. | `public_behavior::flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` asserts entry, helper bytes, CLI parity, and the library exit; the A2 T2 exit/limit/precedence tests assert codes, cycles, PC and timeout shapes. Older lifecycle tests remain smoke/medium checks. | E3 and E6 exercised load/run on the same ELF as the CLI. A2 T2 made the wrapper observe the image's declared RAM exit. **Repaired**, G-01; the remaining divergence is documented in the CLI-versus-library row. |
| State inspection and mutation | `state()` exposes current state and `state_mut()`/core helpers remain available to library callers. | `RiscVSimulator::state`, `state_mut`, `reset_core`, `step_once`, and `get_core_state`; `CoreState` exposes PC, registers, privilege, CSR/FPU fields. | `test_simulator_state_access`, `test_simulator_state_mut`, `test_core_initialization`, `test_core_reset`: **medium** for access/defaults; mutation effects are not asserted. | API access and defaults passed in E1/E3. **Verified** for surface availability and reset/default observations; cross-run state isolation and arbitrary mutation effects are **Unverified**. |
| Flat memory construction and loading | `SimpleMemory` provides thread-safe flat bytes, typed little-endian reads/writes, and `load_program` loads relative to offset 0; its base argument is compatibility-only. | `src/memory/mod.rs::SimpleMemory`; all typed methods enforce natural alignment except byte access; `load_program` ignores `_base_addr`. | `test_memory_read_write`, `test_memory_misaligned`, `test_load_program`, and executor typed sign/zero-extension tests: **strong** for tested values. | E1 and E3 read/write round-trips passed. **Verified** for tested aligned/byte/relative behavior; bounds and overflow edges are not exhaustive. |
| `read_mem` / `write_mem` helpers | Public byte-oriented helpers should return requested bytes or a bounded error and should permit flat-memory writes. | `write_mem` loops byte writes; `read_mem` returns empty for `size == 0`, rejects overflowing nonempty ranges, and otherwise reads bytes in address order or returns `ExecutorError`. | `test_simulator_read_write_mem` is **strong** for in-range data; `test_simulator_read_mem_unaligned` asserts exact unaligned bytes; `test_simulator_read_mem_empty_and_out_of_range` covers empty/error returns. Persistent A2 T1 tests assert exact bytes, empty/overflow/crossing errors, state preservation, and that `new(0x1000).read_mem(0x2000, 4)` returns `Err` inside the bounded child harness. | A1 E3/E6 in-range round-trips passed and the aligned out-of-range read hung. A2 T1 repaired G-02; closing G-02 does not complete A2. |
| CLI versus flat-library configuration | The CLI may use native RAM/UART/HTIF mapping while the flat wrapper uses its own explicit configuration; evidence must not merge them. | CLI: `SystemBus`, RAM base at ELF lowest `p_vaddr`, UART `0x10000000`, fixed HTIF callback, core reset base 0. Library: relative `SimpleMemory`, core reset with ELF base, no `SystemBus` UART/HTIF device map. | `test_system_bus_configs` and `test_memory_adapter_*` are component evidence; `public_behavior::flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` is **strong** for the CLI/library parity, and the A2 T2 placement tests cover the library-only tohost rules. | E2/E3/E6 observed the A1 divergence. A2 T2 made the same ELF exit identically in both configurations for a declared RAM tohost, while the CLI retains UART/HTIF device ability the flat wrapper does not claim. **Verified configuration difference; G-01 repaired.** |
| ISA/path boundary | A passing component instruction test is not a claim that the public ELF path supports that extension end to end. | `RiscvCore::step` uses the active decoder/executor; RV64C/MMU/TLM/peripheral components are not all wired into this public loop. | The 809 library tests include many component tests; the host-only `test_add_direct` run was skipped, while the Docker run executed the add ELF and the 46-file runner set. | **Unverified** for extension-wide or external compliance support. The 46 guest programs are bounded project-authored evidence, not a compliance claim. |

## A2 T1 inspection evidence

A2 T1 repaired G-02 in `RiscVSimulator::read_mem`. The A1 hang observations above
remain historical. Current assertions live in `tests/public_behavior.rs` and
`tests/executor.rs`:

| Test | Assertion |
| --- | --- |
| `out_of_range_flat_read_mem_returns_error_without_hanging` | The original `new(0x1000).read_mem(0x2000, 4)` request returns `Err` in the bounded child after the ready marker; a hang is still killed and reaped. |
| `out_of_range_flat_read_mem_handshake_failure_is_reaped` | Missing ready times out against a live child, then the child is killed and reaped. This is harness cleanup, not the G-02 hang regression. |
| `out_of_range_flat_read_mem_early_exit_is_reaped` | Child exit before ready is detected, diagnostics retained, and the child is reaped. |
| `flat_read_mem_returns_exact_bytes_for_aligned_unaligned_and_mixed_lengths` | Exact little-endian bytes for aligned/unaligned and mixed lengths, including the final valid byte; PC/registers/privilege unchanged. |
| `flat_read_mem_empty_requests_do_not_access_memory` | `size == 0` returns an empty vector at in-range, out-of-range and `u64::MAX` addresses without changing state. |
| `flat_read_mem_rejects_out_of_range_crossing_and_overflow_without_wrapping` | Out-of-range starts, range crossing, and address overflow return errors without wrapping or mutating state. |
| `flat_read_mem_does_not_execute_or_mutate_after_guest_step` | Successful and failing inspection after a retired NOP leave PC, `x5` and RAM unchanged. |
| `test_simulator_read_mem_unaligned` | Unaligned in-range bytes are exact. |
| `test_simulator_read_mem_empty_and_out_of_range` | Empty out-of-range reads succeed; nonempty out-of-range and overflow reads error. |

These rows cover T1 only.

## A2 T2 placement and completion evidence

A2 T2 repaired G-01 and G-10 in `RiscVSimulator::load_elf`/`run`. Current
assertions live in `tests/public_behavior.rs`:

| Test | Assertion |
| --- | --- |
| `flat_library_elf_tohost_metadata_selects_the_ram_exit_signal` | The CLI and library observe the same declared exit at cycle 4; helper bytes still round-trip. |
| `flat_library_reports_zero_and_nonzero_guest_exits` | Guest codes `0`, `1`, and `42` are reported with the writing instruction's cycle and PC. |
| `flat_library_reports_base_zero_placement` | Base-zero image entry, exit and PC use raw offsets. |
| `flat_library_retains_the_nonzero_exit_before_clearing_the_signal` | A nonzero exit survives the RAM signal clear, which leaves eight zero bytes. |
| `flat_library_manual_flat_tohost_overrides_image_metadata` | A declared signal the guest never writes times out; `set_tohost` after loading selects the guest's signal. |
| `flat_library_manual_tohost_before_load_is_superseded_by_image_metadata` | The documented before-load precedence: the image's declared signal wins. |
| `flat_library_manual_tohost_survives_a_load_without_metadata` | A manual offset is kept when the image declares none. |
| `flat_library_image_without_metadata_does_not_reuse_the_previous_tohost` | A second image without metadata times out instead of inheriting the first image's offset. |
| `flat_library_rejects_a_declared_tohost_the_flat_image_cannot_represent` | Below-base, beyond-memory, overflowing and misaligned declared tohost offsets fail the load; a representable one loads. |
| `flat_library_rejected_placement_leaves_the_previous_image_runnable` | A rejected placement propagates the error before wrapper state changes, so the previous image still exits correctly. |
| `flat_library_bounds_zero_budget_and_final_slot_exits` | Zero budget executes nothing and times out; the final permitted slot exits without a timeout. |
| `flat_library_distinguishes_guest_exit_timeout_and_execution_error` | Retained exit, timeout, and execution error are distinct result shapes with correct accounting. |

These rows cover T2 only.

## A2 T3 artifact and image-replacement evidence

A2 T3 repaired G-12 and the flat-library half of G-06 in
`RiscVSimulator::signature_artifact`. Current assertions live in
`tests/public_behavior.rs`:

| Test | Assertion |
| --- | --- |
| `flat_library_returns_guest_written_signature_bytes_at_nonzero_base` | Guest-written byte and image bytes are returned together with the guest metadata address, and match the flat bytes. |
| `flat_library_returns_signature_bytes_at_base_zero` | Base-zero placement returns the same bytes with raw offsets. |
| `flat_library_distinguishes_absent_empty_and_unreadable_signatures` | Absent yields no artifact, zero length yields an empty artifact, and an unmappable region yields an explicit diagnostic rather than silent absence. |
| `flat_library_keeps_the_run_when_the_signature_is_unreadable` | Exit, cycles, final PC and guest RAM are identical with a readable and an unreadable declared region; a primary timeout or execution error survives alongside the artifact diagnostic. |
| `flat_library_replaces_image_metadata_and_ram_on_a_second_load` | A second image replaces RAM and metadata, the first image's artifact does not leak, and reloading the first image restores it. |

A zero-length declared region yields an empty artifact without a mapping check,
matching the bounded `read_mem` rule and the CLI helper, so an empty region at an
otherwise unmappable address is reported as an empty artifact rather than a
failure. Empty is distinguished from absent by `signature_addr`, and from
unreadable by the presence of a diagnostic.

These rows cover T3 only.

## A2 T4 integrated acceptance evidence

A2 T4 proves the capability as one workflow and records the retained CLI
differences; the criterion-by-criterion mapping is in the
[archived A2 capability assessment](../archive/milestones/a2-capability-assessment.md).

| Test | Assertion |
| --- | --- |
| `integrated_load_run_result_inspect_workflow` | One nonzero-base image runs real work (store, load, modify), writes its declared signature byte, exits through its declared tohost with code 42 at cycle 16, and exposes exact registers, RAM bytes and artifact; inspection leaves PC, registers, privilege and RAM unchanged. |
| `integrated_workflow_records_the_retained_cli_device_difference` | A declared RAM tohost exits identically in both configurations, while a UART store is served by the CLI device map and fails as an execution error in the flat wrapper, which has no device mapping. |
| `flat_library_executes_from_a_nonzero_entry_offset` | Entry `0x8000_0100` executes from the declared offset, exits at cycle 4 with final PC `0x8000_0110`, and the image's file bytes are readable at flat `0x100`. |

The workflow test also covers the artifact half of criterion 4 end to end; the
T1–T3 sections above cover the individual boundaries.

## A3 T1 shared placement evidence

A3 T1 replaced the wrapper's private guest-to-offset conversion with one internal
placement owner (`AddressForm` and `ImagePlacement` in `src/executor.rs`) that
both public entry points use. The bus configuration passes image/guest addresses
through, because its RAM mapping already places the image at the base; the flat
configuration converts them with checked arithmetic. Per-use rules stay at the
call site: the exit poll still requires an eight-byte aligned offset, and the
library's zero-length artifact rule still short-circuits before conversion.

The A1/A2 CLI and flat-library suites pass unchanged, including every placement,
exit, artifact and replacement assertion. New unit tests cover the shared owner:

| Test | Assertion |
| --- | --- |
| `test_placement_resolves_both_address_forms` | The same declared `tohost` and signature resolve to the guest address in the bus form and to the offset in the flat form, while the reported signature address stays the guest metadata address. |
| `test_placement_base_zero_uses_raw_offsets` | Both forms coincide at base zero. |
| `test_placement_rejects_unaddressable_flat_ranges` | Below-base, beyond-image and address-space-overflow placements are errors in the flat form and pass through unchanged in the bus form. |
| `test_placement_absent_metadata_is_none` | An image that declares nothing yields no tohost and no artifact in both forms. |
| `test_placement_signature_range_boundaries` | The last addressable byte resolves, one past it fails, and an empty range needs no bytes while an offset outside the image memory is still refused. |

Exit retention and artifact composition were still constructed separately in
each entry point at that point; A3 T2 shared that construction, below.

## A3 T2 shared result evidence

A3 T2 replaced the five separate `ExecutionResult` constructions (four in
`load_and_run`, one in the wrapper) with one internal owner, `ResultInputs` in
`src/executor.rs`. It assembles the observed exit, cycle count, final PC,
timeout flag, primary failure and signature artifact, and merges an artifact
failure into the existing error surface. The artifact-failure policy is an
explicit input — `ArtifactPolicy::Suppress` for the CLI's documented behavior,
`ArtifactPolicy::Report` for the flat library — instead of an implicit property
of whichever read happened to run. Because the exit code is an input, no path
can re-read a RAM signal it already cleared.

The A1/A2 suites pass unchanged, including every exit, limit, artifact,
replacement and retained-difference assertion. New unit tests cover the owner:

| Test | Assertion |
| --- | --- |
| `test_result_inputs_assembles_observed_facts` | Exit, cycles, final PC, timeout flag, artifact bytes and the guest metadata address are carried through unchanged. |
| `test_result_inputs_reports_or_suppresses_artifact_failures` | The same failed read yields an explicit diagnostic under `Report` and silent absence under `Suppress`, with the exit and metadata address intact either way. |
| `test_result_inputs_preserves_a_primary_failure` | A timeout or execution error survives alongside an artifact diagnostic, primary first, and is unchanged when the policy suppresses. |
| `test_result_inputs_absent_and_empty_artifacts` | An absent region yields no artifact, and an empty one yields an empty artifact rather than a failure. |
| `test_result_inputs_timeout_shape_is_caller_supplied` | A caller-supplied failure keeps its distinct timeout shape, code and accounting. |

The two loops still own their own stepping, exit detection, signal clearing and
limit policy; sharing the result construction does not merge them.

## A3 T3 equivalence evidence

A3 T3 pins what the shared path guarantees across both entry points, so the
equivalence is committed evidence rather than an inference from two separate
suites. The tests run the same image through `load_and_run` and
`RiscVSimulator`:

| Test | Assertion |
| --- | --- |
| `shared_result_returns_the_same_artifact_bytes_in_both_entry_points` | Exit, cycles, final PC, signature metadata address and artifact bytes are identical where the configurations agree. |
| `shared_result_keeps_each_configurations_artifact_policy` | For one image whose declared region cannot be read, both report the same exit, cycles, final PC and metadata address, while the CLI keeps its documented silent absence and the flat library reports the explicit diagnostic. |
| `shared_result_shapes_hold_for_timeout_and_instruction_error` | Both exhaust the same budget with identical cycles, PC and timeout text, and both report an instruction error at the same boundary without claiming a guest exit. The differing message shape is pinned: the CLI names the PC, the flat library reports the boundary through `final_pc`. |
| `shared_placement_selection_rule_holds_in_both_entry_points` | The same selection rule holds in both configurations — an explicit override wins over the image's declaration — in each configuration's own address form: a bus address for the CLI, a flat storage offset for the library. |

## A4 T1 shared run-control evidence

A4 T1's initial helpers shared the instruction budget, retirement count, timeout
diagnostic and decode-before-clear RAM rule, but left both loops independently
selecting their stop reasons. Bounded T1 completion makes
[`RunControl::start` and `RunControl::after_step`](../../src/executor.rs) the
single decision owner: zero budget selects timeout before stepping; failure
selects execution error without retirement or observation; success retires once,
traverses configured observers lazily in order, and selects the first guest exit
before deciding exhaustion or continuation.

The configurations supply ordered observer lists (CLI HTIF then selected RAM;
flat selected RAM only), address forms, stepping, commit logging and diagnostic
wording. The loops format the returned `RunDecision`; they do not select their
own stop reason. RAM observers use the same decode-before-clear helper. No loops
or backend compositions were merged.

The A1–A3 CLI and flat-library suites pass unchanged, including every budget,
final-slot, exit-retention, artifact and equivalence assertion. New unit tests
cover the owner:

| Test | Assertion |
| --- | --- |
| `test_run_control_counts_only_retired_instructions` | The budget admits exactly the permitted instructions, the count tracks retirement, and the timeout text names the budget in force. |
| `test_run_control_zero_budget_executes_nothing` | A zero budget admits no instruction and reports a zero-cycle timeout. |
| `test_run_control_retains_the_exit_before_clearing_the_signal` | A standard or alternative payload is decoded and retained before the signal is cleared, and a value without an exit command leaves the signal untouched. |

The earlier claim that signal ordering was not separately testable was too
strong. Lazy observation can be tested directly, and skipping a lower-priority
read can also be distinguished through public verbose diagnostics.

| Added test | Assertion |
| --- | --- |
| `test_run_decision_zero_budget_never_steps_or_observes` | The initial decision prevents both stepping and observation at zero budget. |
| `test_run_decision_continue_then_exhaustion` | Successful steps retire once, observe the updated count, continue while budget remains, then select timeout. |
| `test_run_decision_error_does_not_retire_or_observe` | A failing step after one retirement returns the original error, leaves the count unchanged and never invokes a pending-exit observer. |
| `test_run_decision_first_exit_skips_lower_priority_observer_on_final_slot` | The first exit wins over exhaustion and the lower-priority observer is never invoked. |
| `test_run_decision_observes_in_order_and_retains_ram_exit_on_final_slot` | A silent primary observer precedes RAM observation; clearing occurs after decode and the nonzero code survives the clear. |
| [`failed_first_instruction_does_not_consume_a_pending_ram_exit`](../../tests/a4_run_control.rs) | Both public configurations report an execution error, not the already-present exit or a timeout; flat RAM retains its unconsumed signal. |
| [`cli_final_slot_htif_exit_skips_ram_poll_and_timeout_diagnostics`](../../tests/a4_run_control.rs) | A CLI ELF exits via HTIF in slot 1000; the lower-priority unmapped RAM read and periodic/timeout diagnostics do not occur. |

Local implementation verification: the full quality gate and strict rustdoc in
the active plan passed. `cargo test --all-features` passed 830 library unit tests,
the unchanged 44-test `public_behavior` suite, the unchanged 58-test `executor`
suite, the new two-test `a4_run_control` suite, all other Rust integration
suites, and 27 doctests. Focused runs of the five `run_decision` unit tests and
both public suites also passed. No A1–A3 test was changed or weakened.

This is uncommitted implementation evidence, not exact-head formal review or
merge evidence. The separately compiled 46 guest ELF tests were not rebuilt or
run for this change; Cargo tests do not establish that result. A4 remains active:
T3's full capability assessment, independent review, required guest evidence and
separately approved successor decision remain subsequent work. No ACT4
verification or successor approval is claimed.

## A4 T2 shared image installation evidence

A4 T2 moved the installation sequence — create RAM sized to the loaded image,
load the program bytes, construct the core over the configuration's memory
backend and reset it to the entry point — into one internal owner,
`install_image` in `src/executor.rs`, used by both entry points. The backend
stays with the caller: the CLI composes the loaded RAM at the image base with
its UART and HTIF devices on the system bus, the flat library uses the bare
RAM. The core's reset translation base is derived from the configuration's
address form (`AddressForm::core_translation_base`): pass-through for the bus
form, image-base subtraction for the flat form — the same two forms A3
introduced for metadata resolution.

Retained per configuration, unchanged: the CLI's empty-image
`MemoryAllocationFailed` guard stays in the CLI caller, the device composition
and HTIF exit callback stay with the bus backend, and the wrapper keeps its
load-time tohost validation ahead of the shared sequence and its metadata
replacement rules after successful installation.

The A1–A3 CLI and flat-library suites pass unchanged. New unit tests cover the
owner:

| Test | Assertion |
| --- | --- |
| `test_install_image_loads_the_program_and_resets_to_the_entry_point` | The backend receives the RAM with the program already loaded, and the returned core starts at the entry point over the backend the caller supplied. |
| `test_install_image_flat_form_subtracts_the_image_base` | Under the flat form, the first fetch at a nonzero guest entry point resolves to storage offset zero and the instruction retires. |
| `test_install_image_bus_form_passes_guest_addresses_through` | Under the bus form, the core passes the guest address through and the composed bus maps it to the loaded RAM, and the instruction retires. |

## Known stale or non-authoritative inputs

- `tests/bare-metal-riscv-test/README.md` describes `rv64i/add.elf` as returning
  exit code 55, while `rv64i/add.S` writes zero on its pass path and
  `scripts/run_elf_tests.sh` expects zero. The source and runner script win for
  current behavior; the README text is not compatibility evidence.
- The checked-in `tests/reference-logs/*.log.ref` files are historical Spike-side
  comparison inputs. They are not current ruscv-sim output and do not repair the
  public-loop logging gaps above.

## Bounded follow-up proposal (not a new active plan)

A1, A2 and A3 are complete with documented limitations;
[the A2 closeout](../archive/milestones/a2-closeout-record.md) and
[the A3 closeout](../archive/milestones/a3-closeout-record.md) hold their
dispositions. G-02 was repaired by A2 T1, G-01/G-10 by A2 T2, and G-12 with the
flat half of G-06 by A2 T3; A2 T4 added the integrated workflow and its
[archived capability assessment](../archive/milestones/a2-capability-assessment.md),
and A3 shared image placement and result construction between the entry points.
[The active plan](../dev-plan.md) is A4, one host-side path for run control and
image installation. These remaining options are not scheduled by A4 and must be
re-evaluated before any of them becomes work:

1. Decide separately whether G-03, G-04, G-05, the CLI portion of G-06, and G-09
   are production-repair scope or documented compatibility limitations. The A1
   tests intentionally do not make that decision.
2. If a repair is approved, retain the persistent tests as regressions and add
   only the minimum implementation change needed for the selected gap; do not
   silently alter address maps, limit semantics, APIs, or logging contracts.
3. Add focused evidence for the remaining unverified boundaries only when their
   intended contract is defined: default-limit exhaustion, tohost symbol fallback,
   the CLI signature read failure, the UART boundary, and broader error
   classification.
4. Re-run the guest suite with the project Docker image or a compatible host
   toolchain, and record actual ELF outputs rather than relying on CI definitions
   or old logs.

These options do not expand A4 and must be re-evaluated before any becomes
work. A1's completion and limitations are recorded in
its [closeout record](../archive/milestones/a1-closeout-record.md), A2's in
[the A2 closeout](../archive/milestones/a2-closeout-record.md), and A3's in
[the A3 closeout](../archive/milestones/a3-closeout-record.md).
