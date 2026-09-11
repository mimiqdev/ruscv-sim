# Archive context and final evidence

**Status:** Historical assessment; formal closeout proposed

**Authority:** Informational; not the current milestone contract

**Implementation completion / record preparation date:** 2026-09-11

All five A4 capability criteria have implementation and verification evidence.
The [closeout record](a4-closeout-record.md) supplies the final dispositions.
Formal acceptance and the detailed [A5 contract](../../dev-plan.md) remain
pending through the closeout PR merge; no closeout merge is claimed here.

T3 was independently reviewed at exact PR #30 head
`810bf83968226e5390b15b6c73890382bbc895e3`: approved, no defects, with the full
six-command gate below (including strict rustdoc) passed. No mutation checks
or runtime differential were run. The reviewer also independently verified the
older CI records and the pinned ACT4 4.0.0 README, superseding the earlier
assessment's API-rate-limit verification limitation.

[PR CI 34608196803](https://github.com/mimiqdev/ruscv-sim/actions/runs/34608196803)
succeeded. [PR #30](https://github.com/mimiqdev/ruscv-sim/pull/30) merged as
`e47a1b0b2c8de25ad3dae17a1b9f61d7c42207d5`;
[push CI 34608867200](https://github.com/mimiqdev/ruscv-sim/actions/runs/34608867200)
succeeded, including release build/smoke and separate guest ELF compilation
and execution: **46 total / 46 passed / 0 failed**. The push logs were read
directly for this closeout evidence. This is T3 merge evidence, not a local
guest run or an external ACT4 result.

The original pre-closeout assessment below is preserved with relocated links.
Its pending/uncommitted statements and proposed successor describe that earlier
state; final evidence above supersedes them, not its recorded test results.

---

# A4 Capability Assessment

**Status:** Current assessment; capability acceptance pending

**Authority:** Informational; not the milestone contract or a closeout

**Assessed base:** `b461744b216f0c7f90f63b56b8e82791ef8ef831`

**Local verification date:** 2026-09-11

## Purpose and disposition

[A4](a4-shared-run-control-and-image-installation.md) requires one internal run-control decision and one image
installation sequence for **two public entry loops**: `load_and_run` and
`RiscVSimulator::run`. The merged base implements those owners. The T3 addition
supplies integrated public equivalence tests and this assessment, locally
verified but **not yet committed, independently reviewed, or merged**.
Acceptance criteria 4 and 5 still require T3 committed-head review and
merge-time evidence. A4 is not complete; the active contract is unchanged in
scope and must not be archived on the strength of this assessment.

The claim does not include `RiscvCore::run`, merge the public loops, or integrate
the target Runner/Machine/Platform composition. It is bounded by
[ADR-0003 §§4, 6, 9](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md):
host installation is separate from Hart execution, successful exit follows
retirement, and current public configuration differences remain explicit.
The precise outcomes, physical ports and scheduling contracts of
[ADRs 0001–0004](../../architecture/decisions/README.md) remain accepted targets,
not newly implemented capabilities.

## Implementation and evidence layers

| Contribution | Concrete owner or evidence | State |
| --- | --- | --- |
| Run-control accounting and RAM retention | [`RunControl`](../../../src/executor.rs#L605), including `take_ram_exit` | Initial T1 merged through PR #27; accounting alone was insufficient for the contract. |
| Installation | [`install_image`](../../../src/executor.rs#L832), called by [`load_and_run`](../../../src/executor.rs#L915) and [`load_elf`](../../../src/executor.rs#L1328) | T2 merged through PR #28. Creates RAM, loads the program, wraps the supplied backend, constructs the core and resets its entry/address form. |
| Complete bounded decision ownership | [`start` / `after_step`](../../../src/executor.rs#L638) and [`RunDecision`](../../../src/executor.rs#L611) | Follow-up PR #29 merged in the assessed base. The owner selects continue, guest exit, timeout or execution error and traverses ordered lazy observers. |
| Integrated capability | [`a4_integrated_equivalence`](../../../tests/a4_integrated_equivalence.rs#L1) | Two T3 tests locally pass; commitment and independent review remain pending. No production code changes. |

The CLI supplies HTIF before selected RAM; the flat facade supplies only its
selected RAM offset. Failure does not retire or observe; success retires once,
and the first observed exit wins over exhaustion. Each loop retains its own
stepping, signal sources, diagnostic formatting and artifact policy.
Installation shares the sequence, not devices: the native backend retains
RAM/UART/HTIF and the library backend stays bare flat memory.

## Acceptance criteria mapping

### 1. One run-control decision

The two public loops call the shared owner rather than selecting an independent
stop reason. The following committed base tests in
[`src/executor.rs`](../../../src/executor.rs#L1969) cover the rule directly:

| Required scenario | Test evidence |
| --- | --- |
| Zero budget; no instruction or observation | `test_run_decision_zero_budget_never_steps_or_observes`; `test_run_control_zero_budget_executes_nothing` |
| Continue, exact count and exhaustion | `test_run_decision_continue_then_exhaustion`; `test_run_control_counts_only_retired_instructions` |
| Failed step does not retire or consume signals | `test_run_decision_error_does_not_retire_or_observe` |
| Final-slot exit wins and lower-priority observer is lazy | `test_run_decision_first_exit_skips_lower_priority_observer_on_final_slot` |
| Signal ordering, retention before clearing, both encodings | `test_run_decision_observes_in_order_and_retains_ram_exit_on_final_slot`; `test_run_control_retains_the_exit_before_clearing_the_signal` |

The committed [`a4_run_control`](../../../tests/a4_run_control.rs#L9) regressions
add public outcomes: both configurations preserve a pending RAM exit on a failed
first instruction; a CLI HTIF exit at slot 1000 suppresses the lower-priority
unmapped RAM read and timeout/periodic diagnostics. T3 adds the integrated
data-dependent boundaries below. **Disposition:** implemented and verified in
the merged base; additive T3 evidence locally verified, not formally reviewed.

### 2. One installation path

The three committed
[`test_install_image_*` tests](../../../src/executor.rs#L2121) prove that the
backend receives loaded RAM, that reset establishes the entry, and that both
flat-subtracting and bus-pass-through forms fetch and retire an instruction.
Those tests alone use entry offset zero. T3 deliberately combines **nonzero
entry offsets** with actual guest data access in both public configurations.

| T3 test | Independent expected outcomes in both configurations |
| --- | --- |
| [`nonzero_entry_data_workflow_agrees_at_run_control_boundaries`](../../../tests/a4_integrated_equivalence.rs#L38) | One identical ELF per row starts at `0x80000180`, loads file-backed seed 17, adds 25, stores and reloads 42, then forms exit payload 85 from that reload. Both results return the computed eight-byte artifact at guest address `0x80002000`, or the original file bytes when zero budget prevents execution. |
| [`replacement_load_restores_data_and_entry_before_the_same_public_workflow`](../../../tests/a4_integrated_equivalence.rs#L107) | After a prior exit 42, dirty BSS, a pending signal and a manual override, loading a replacement restores the seed, clears old code/signal/BSS and resets all GPRs and PC to `0x80000240`. Running the replacement agrees with a fresh CLI installation: exit 43, 11 retirements, PC `0x8000026c`, artifact `43u64.to_le_bytes()`. |

The first test's rows independently assert:

| Budget / variant | Exit / retired / final PC | Result and data |
| --- | --- | --- |
| 0 | 1 / 0 / `0x80000180` | Timeout, exact `Timeout after 0 cycles`, original file data |
| 10 | 1 / 10 / `0x800001a8` | Timeout before exit store, exact `Timeout after 10 cycles`, computed artifact |
| 11 | 42 / 11 / `0x800001ac` | Final-slot guest exit, no timeout/error, computed artifact |
| 12 | 42 / 11 / `0x800001ac` | Early exit; invalid instruction after exit is never attempted |
| 11, invalid exit instruction | 1 / 10 / `0x800001a8` | Execution error, not timeout; prior data effects/artifact retained; documented error wording differs |

Flat inspection additionally checks `x0`, the reload-derived exit register,
artifact RAM and cleared signal bytes. Segment-base bytes are invalid
instructions, so resetting to the base instead of the declared entry cannot
accidentally pass. Replacement tests `load_elf`'s fresh installation semantics,
not a new reset API or general Machine/device reset contract.
**Disposition:** shared implementation verified in the merged base; integrated
T3 evidence locally verified, pending committed-head review.

### 3. Behavior preserved

All pre-existing tests and fixtures are unchanged. The T3 test file is additive;
`git diff b461744 -- src tests` contains no modifications to existing files.
The full Cargo run passed the unchanged `public_behavior` (44), `executor` (58),
`cli_test` (15) and `a4_run_control` (2) suites, as well as all other Rust suites.
The [A3 retained differences](a3-capability-assessment.md#retained-differences-between-the-configurations)
still apply:

| Difference | Preserved evidence in [`public_behavior.rs`](../../../tests/public_behavior.rs#L1630) |
| --- | --- |
| CLI bus addresses versus flat storage offsets; explicit override precedence | `shared_placement_selection_rule_holds_in_both_entry_points` |
| CLI UART/HTIF device composition versus bare flat RAM | `integrated_workflow_records_the_retained_cli_device_difference` |
| CLI suppresses unreadable signature artifacts; flat facade reports a diagnostic | `shared_result_keeps_each_configurations_artifact_policy` |
| CLI instruction-error text names the PC; flat facade exposes it in `final_pc` | `shared_result_shapes_hold_for_timeout_and_instruction_error` |
| Shared exit, count, PC, artifact bytes and guest metadata address where configurations agree | `shared_result_returns_the_same_artifact_bytes_in_both_entry_points`, plus the two T3 tests |

### 4. Evidence is current

#### Local T3 verification

Host: Darwin arm64; `rustc 1.98.0 (88d9e12ae 2026-08-18)`,
`cargo 1.98.0 (797e8a9bc 2026-08-05)`. All six commands exited zero on the
uncommitted T3 test addition over the assessed base:

| Command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo check --all-features` | Passed |
| `cargo clippy --all-features --all-targets -- -D warnings` | Passed |
| `cargo test --all-features` | Passed: 830 library unit tests, all integration suites including the two new T3 tests, and 27 doctests; zero reported failures |
| `cargo doc --all-features --no-deps` | Passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps` | Passed |

The focused `cargo test --all-features --test a4_integrated_equivalence` also
passed 2/2. An initial one-second compile invocation was interrupted, and the
first formatting check reported formatting differences in the new file; both
were superseded by the successful complete gate above.

No RISC-V GNU compiler, assembler/linker or Docker was found on `PATH`.
`cargo test --all-features --test test_add_direct -- --nocapture` confirmed
`skipping test_add_program: missing assembler riscv64-unknown-elf-as`.
Cargo reports this early return as a pass, not an ignored test. Thus the Cargo
result is **not** evidence of locally assembling that guest, much less of
separately compiling/executing all 46 project-authored ELFs. The
[guest commands](../../verification/bare-metal-tests.md) were not run locally. No tools were
installed and no ACT4 test was generated or executed.

#### Recorded merged-base evidence and independent review

These are prior merge records, not prospective T3 results:

| Merged change | Recorded review / merge evidence |
| --- | --- |
| Initial T1, `f501593`, [PR #27](https://github.com/mimiqdev/ruscv-sim/pull/27) | [Main CI 34601046791](https://github.com/mimiqdev/ruscv-sim/actions/runs/34601046791): successful merge run, separately compiled/executed guest suite 46/46. This did not by itself establish full decision ownership. |
| T2, `dd45c86`, [PR #28](https://github.com/mimiqdev/ruscv-sim/pull/28) | Independent review had no blockers; [main CI 34602623737](https://github.com/mimiqdev/ruscv-sim/actions/runs/34602623737) succeeded with separate guest compilation/execution 46/46. |
| Bounded decision completion, `b461744`, [PR #29](https://github.com/mimiqdev/ruscv-sim/pull/29) | Independent review of exact head `b788114` approved with no defects; full local gate passed. No base/head runtime differential was run; behavior preservation was assessed through source comparison and regressions. [Main CI 34605659612](https://github.com/mimiqdev/ruscv-sim/actions/runs/34605659612) succeeded with separate guest compilation/execution 46/46. |
| T3 addition | Local evidence above only. No committed-head independent review, PR CI or merge-time guest evidence yet. |

The base records were supplied as verified prior review/merge evidence; attempts
to re-query those GitHub run APIs during this assessment returned HTTP 403 rate
limit errors. They were not independently rerun here. The
[CI workflow](../../../.github/workflows/ci.yml#L63) compiles and executes the full
guest set on pushes, not on PR runs. A successful PR Cargo run cannot replace
the required T3 merge-time record.

### 5. Capability acceptance, not task counting

This assessment maps all five criteria, but cannot mark the new tests committed
or supply future review/merge results. After authorization to commit/publish,
formal independent review must target the exact committed head and its gate
evidence; findings require correction and re-review. Record the actual merge
revision and successful separately compiled guest run before asking for A4
acceptance. Closeout and a replacement active contract require a separate
maintainer decision. **Disposition:** pending, not waived.

## Limitations and retained gaps

- One Hart, synchronous flat/bus RAM, bounded little-endian ELF64 fixtures using
  already exercised instructions only. No ISA extension-wide claim.
- The two public loops remain separate; `RiscvCore::run` is not consolidated.
  No precise Hart outcome/observation, Machine/Platform composition, scheduler,
  interrupt, MMU/PMP, multi-Hart, SystemC/TLM or wrapper device integration.
- Default-limit exhaustion (10,000,000) and the `tohost` symbol fallback remain
  unverified; `.tohost` section fixtures do not prove symbol fallback.
- [G-03, G-04, G-05, CLI G-06, G-07, G-08, G-09 and G-11](../../verification/public-behavior-gaps.md)
  retain their dispositions. No logging, UART, artifact-policy, extreme-address,
  allocation, transactional-write or poisoned-lock/blocking-backend repair.
- No current external architecture-suite result or certification claim. The
  shared host path is not evidence that all RV64I instructions work:
  [`Opcode::MiscMem` is rejected](../../../src/decode/mod.rs#L248), so FENCE is a
  known public-path gap likely to matter to external RV64I validation.
  FENCE.I belongs to **Zifencei**, not RV64I.

## One successor proposal: ACT4 RV64I external compatibility validation

**Recommended direction:** pivot from internal consolidation to reproducible,
externally authored RV64I compatibility evidence through the public CLI.
The direction has maintainer approval; the detailed next contract below is a
proposal requiring approval at A4 closeout, not active A4 scope.

### Proposed objective and boundary

Generate and execute a pinned, explicit selection of ACT4 self-checking RV64I
ELFs through `ruscv-sim run`, using Sail-derived expected values. Establish
exactly which selected unprivileged base-integer scenarios pass on the current
single-Hart native configuration and repair bounded defects they expose with
focused public regressions. This is more valuable now than another purely
internal extraction: A4 has removed the specific host-side duplication, while
the public ISA path still lacks any selected external baseline.

Do not implement a RISCOF plugin or compare legacy DUT signature dumps.
Do not claim RV64G, Zifencei, compressed execution, privilege/trap/interrupt
integration, devices beyond the existing native exit/console path, or full
RV64I certification. Tests requiring unavailable architectural traps or other
extensions need explicit exclusions, not a false capability declaration.
FENCE must be investigated as a base-I gap rather than mislabeled as Zifencei;
any repair must define its bounded synchronous-memory behavior without inventing
a concurrent platform ordering model.

### Upstream research and toolchain dependency

ACT4 4.0.0 resolves to
[`a7c99303516f4e668f7488f172043392e23b9dfd`](https://github.com/riscv/riscv-arch-test/tree/a7c99303516f4e668f7488f172043392e23b9dfd)
(`git ls-remote` checked during this assessment). Its
[README](https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/README.md)
describes Make/Python, UDB configuration, DUT macros and linker script,
Sail-generated expected values compiled into self-checking ELFs, and a
user-owned DUT execution harness. It names Sail **0.10** and compiler baselines
GCC 15/Binutils 2.44 or LLVM/Clang 21. Consequently the research candidate
Sail **0.13.1** is not an established compatible partner for that revision.
No approximate source-test count is an acceptance denominator.

The next contract must select and experimentally verify one exact ACT4/Sail/
compiler combination, including submodules, UDB/Ruby/Bundler and Python
dependencies. The current [development image](../../development-environment.md)
does not establish that combination: Spike availability is not Sail availability,
and the existing GNU guest-toolchain CI job is not an ACT4 setup. Choose a
reviewed workspace/container setup with immutable pins; record image digest,
compiler and model versions. Do not assume this host can build it.

### Proposed deliverables and acceptance

1. **Reproducible generation:** checked-in DUT configuration, linker script and
   pass/fail macros mapped to the existing exit path; pinned tool manifest and
   one documented build command that generates self-checking ELFs with Sail
   expected values. Prove generated startup/code requirements match the declared
   public profile; never enable unsupported extensions to make filtering pass.
2. **Explicit selection:** machine-readable source selection, configuration
   parameters and justified exclusions, plus an enumerated generated-ELF
   manifest with hashes. Derive counts from that exact build, distinguishing
   sources, generated variants and executed ELFs. No silent skip, missing file
   or generation failure may count as pass.
3. **Public execution and diagnostics:** a bounded runner invokes the public CLI
   for every selected ELF, retaining commands, limits, stdout/stderr and
   per-test machine-readable outcomes. Distinguish guest pass, guest failure,
   timeout, unsupported execution and simulator/harness failure. Negative
   controls must prove a failing guest, timeout and execution error cannot be
   reported as success.
4. **Compatibility result:** every approved selected ELF must pass before that
   selection is accepted. Each simulator defect receives a reproducing local
   regression and the minimal scoped repair; out-of-boundary discoveries are
   reported for an explicit scope decision, not silently dropped from the
   denominator. Keep all A1–A4 regressions unchanged unless a separately approved
   behavior change requires an explicit contract update.
5. **CI and final evidence:** a reproducible CI job must actually provision the
   pinned environment, generate with Sail and run the selected ELFs, retaining
   build/configuration/failure artifacts and totals. An independent committed-head
   review, full Rust gate, current project-authored guest results and exact
   reviewed-head or verified-merge ACT4 evidence are required. If tool setup or
   Sail generation is unavailable, report the blocker; a cached unexplained ELF
   bundle or host-only Cargo pass cannot satisfy this acceptance criterion.

No second successor is proposed. This proposal does not schedule all remaining
architecture debt, replace the active plan, or pre-approve changes to the ISA.
