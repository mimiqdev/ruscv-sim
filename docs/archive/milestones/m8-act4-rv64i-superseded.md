# Superseded Milestone Plan — M8 ACT4 RV64I Baseline

> **Status:** Superseded without completion on 2026-08-26.
>
> **Reason:** Product planning was reset to define the ISS-to-Virtual-Platform architecture before selecting further implementation milestones.
>
> The unchecked items below are historical proposals, not completed work or current commitments.

**Project:** RISC-V ISS Simulator (`ruscv-sim`)

**Active milestone:** M8 — ACT4 RV64I Baseline

**Status:** Active — planning and environment bootstrap

**Started:** 2026-08-26

**Planning model:** Rolling milestone

This file was once the single source of truth for project work. It is now archived in [`milestones/`](./); its unchecked items and future goals are not active tasks.

## M8 objective

Integrate the latest RISC-V Architectural Certification Test framework (ACT4) and pass the complete non-privileged RV64I suite with self-checking ELF programs.

M8 deliberately starts with RV64I only. Existing M/A/F/D/C, CSR, trap, MMU, TLM, and peripheral components must not be advertised to ACT4 until their end-to-end execution paths are separately verified.

## Upstream baseline

ACT4 4.0 replaced the deprecated RISCOF flow. ACT4 uses a UDB description of the DUT, runs the Sail reference model to generate expected results, and compiles those results into self-checking ELF files.

Before implementation, record reproducible pins for:

- [ ] `riscv-arch-test` ACT4 commit
- [ ] RISC-V Sail version supported by that ACT4 commit
- [ ] RISC-V compiler and binutils versions
- [ ] Python, `uv`, Ruby, Bundler, and UDB environment

Official references:

- <https://github.com/riscv/riscv-arch-test/tree/act4>
- <https://github.com/riscv/sail-riscv>

## Scope

### 1. Reproducible ACT4 environment

- [ ] Choose the supported execution environment for local development and CI
- [ ] Install or provision ACT4 dependencies
- [ ] Generate a self-checking RV64I ELF using an upstream reference configuration
- [ ] Document exact setup and reproduction commands
- [ ] Keep the upstream test suite outside this repository or pin it explicitly; do not copy generated upstream tests into the source tree

### 2. Minimal ruscv-sim DUT configuration

Create an ACT4 configuration for the capabilities that are true in the ELF execution path:

- [ ] `test_config.yaml`
- [ ] Minimal RV64I UDB configuration
- [ ] `rvmodel_macros.h`
- [ ] ACT4-compatible linker script
- [ ] `sail.json`
- [ ] `rvtest_config.h`
- [ ] `rvtest_config.svh`
- [ ] `run_cmd.txt`

Initial configuration constraints:

- XLEN is 64
- Little-endian
- One hart
- RV64I only
- Privileged tests disabled
- Compressed instructions disabled
- Paging and MMU behavior not advertised
- Interrupts, PMP, Supervisor mode, and optional extensions not advertised

### 3. DUT runtime contract

- [ ] Place test code and data in RAM beginning at `0x8000_0000`
- [ ] Place `.tohost` in RAM and keep it eight-byte aligned
- [ ] Implement ACT4 `RVMODEL_HALT_PASS` using `tohost = 1`
- [ ] Implement ACT4 `RVMODEL_HALT_FAIL` using `tohost = 3`
- [ ] Implement ACT4 console macros for the UART 16550 at `0x1000_0000`
- [ ] Verify UART output contains an intact `RVCP-SUMMARY` line
- [ ] Distinguish test failure, simulator error, and timeout in process exit behavior and logs
- [ ] Select and document a safe per-test instruction limit
- [ ] Support ACT4 `run_cmd.txt` debug placeholders with `--log-commits`

### 4. First self-checking ELF

- [ ] Build the simulator in release mode
- [ ] Generate one simple RV64I ACT4 ELF
- [ ] Load and execute it through the public CLI
- [ ] Observe `RVCP-SUMMARY: TEST PASSED`
- [ ] Return process exit code zero
- [ ] Confirm `run_tests.py` recognizes the result
- [ ] Preserve the command and log as a smoke-test fixture or reproducible CI artifact

### 5. Complete non-privileged RV64I suite

- [ ] Run the complete ACT4 RV64I non-privileged selection
- [ ] Implement FENCE/FENCE.I in the real decode/execute path where required
- [ ] Fix instruction decoding and execution failures exposed by ACT4
- [ ] Fix load/store, alignment, and integer edge cases exposed by ACT4
- [ ] Add a focused local regression test for every simulator defect found by ACT4
- [ ] Keep DUT UDB and Sail configuration synchronized with actual behavior
- [ ] Produce a machine-readable and human-readable test summary

### 6. CI integration

- [ ] Add a dedicated ACT4 job without slowing ordinary unit-test iteration unnecessarily
- [ ] Cache or provision the pinned ACT4 tool environment reproducibly
- [ ] Upload summary, failing simulator logs, objdump files, and debug traces
- [ ] Fail CI on ACT failure, simulator error, timeout, or missing summary
- [ ] Document how to rerun a single failing ELF locally

## Known starting gaps

- `Opcode::MiscMem` currently returns `UnimplementedInstruction`, so FENCE/FENCE.I are not available through the ELF core loop.
- RV64C components are not integrated into fetch; the core fetches 32-bit words and advances the PC by four bytes.
- MMU and TLM components are not used by `RiscvCore::step`.
- Trap components are not yet a unified error-to-architectural-trap path for ELF execution.
- Commit logging does not yet receive memory-access information from the executor.
- The current local environment does not have the ACT4 toolchain, Sail, or a RISC-V cross compiler installed.

Only gaps that block the RV64I ACT4 acceptance criteria belong in M8. Other gaps must be evaluated when planning the next milestone.

## Execution order

1. Pin and validate the ACT4 build environment.
2. Add the minimal RV64I DUT configuration.
3. Run one self-checking ELF manually.
4. Make the same ELF pass through ACT4 `run_tests.py`.
5. Run the full RV64I selection and triage failures.
6. Add regressions and fix simulator behavior.
7. Add the stable suite to CI.
8. Verify every acceptance criterion and archive M8.

## Acceptance criteria

M8 is complete only when all of the following are true:

- [ ] Tool and upstream versions are pinned and reproducible
- [ ] The ruscv-sim ACT4 configuration validates and generates self-checking ELF files
- [ ] ACT4 uses a truthful RV64I-only DUT configuration
- [ ] Every selected non-privileged RV64I ACT4 test passes through `run_tests.py`
- [ ] No selected test times out or exits because of a simulator-internal error
- [ ] ACT4 failures discovered during development have focused local regression tests
- [ ] CI runs the same pinned suite and retains useful failure artifacts
- [ ] Setup, single-test debugging, and full-suite commands are documented

Passing project-authored bare-metal programs or Rust unit tests does not satisfy these criteria.

## Milestone closeout

When every acceptance criterion is verified:

1. Move this complete plan to `docs/archive/milestones/m8-act4-rv64i.md`.
2. Add completion date, ACT4 commit, tool versions, test counts, CI evidence, known limitations, and relevant commit/tag identifiers.
3. Re-evaluate—not automatically inherit—the remaining M/A/F/D/C, privileged, MMU, RV32, and Sv48/Sv57 work.
4. Replace this file with exactly one newly approved active milestone.
