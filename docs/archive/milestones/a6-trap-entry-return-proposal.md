# Milestone Proposal: A6 — Machine-Mode Synchronous Trap Entry and Return

> **Archived Proposal Snapshot:** Preserved historical proposal text as authored on 2026-09-16 (PR #39, commit `9429e6ab2d324bb297ecee78fa1c9b1b52d6494f`).
> This proposal was reviewed, reconciled with repository architecture decisions (ADR-0001 through ADR-0004), and activated into [`docs/dev-plan.md`](../../dev-plan.md) on 2026-09-17 as the sole current milestone contract.
> The historical text below is preserved without retrospective edits; consult [`docs/dev-plan.md`](../../dev-plan.md) for current authoritative requirements.

**Status:** Historical

**Authority:** Informational; archived proposal record. Current milestone contract is [`docs/dev-plan.md`](../../dev-plan.md).

**Date:** 2026-09-16 (Archived: 2026-09-17)

---

## 1. Context and Purpose

Following the formal closeout of Milestone A5 (ACT4 RV64I External Compatibility Baseline, PR #37, merged `d1834cc6566b342824bca30772cc829953a5c5ef`), the repository development plan (`docs/dev-plan.md`) serves as the sole milestone forwarding record until a successor is approved.

During Milestones A0–A5, architectural synchronous exceptions (such as `ECALL`, `EBREAK`, illegal instructions, misaligned memory addresses, and physical access faults) were treated as non-trapping exclusions or simulator execution errors. In the current simulator codebase:
- `RiscvCore::step` in `src/core/mod.rs` halts execution on fetch, decode, or execution failures, returning an `anyhow::Result::Err` that the runner converts into an execution error (`RunDecision::ExecutionError`).
- The existing `TrapHandler` in `src/core/trap.rs` contains standalone component logic and tests, but is not wired into the public fetch-decode-execute loop in `RiscvCore::step`.
- In `src/core/trap.rs`, the vectored trap calculation (`TrapHandler::vector_trap`) incorrectly offsets synchronous exceptions by `(cause & 0x7F) << 2`, whereas the RISC-V Privileged Architecture Specification §3.1.7 mandates that all synchronous exceptions vector to `BASE` regardless of whether `mtvec.MODE` is Direct or Vectored.
- The `MRET` implementation in `src/isa/rv64i/system.rs` (`exec_mret`) lacks privilege validation (allowing illegal execution from User or Supervisor modes), omits setting `mstatus.MPIE` to 1, and does not explicitly transition `mstatus.MPP` to User mode (`0b00`).
- Faulting instructions and trap entry do not currently adhere to the non-retirement semantics defined in accepted architecture decision record ADR-0001 (`docs/architecture/decisions/0001-hart-execution-outcome-and-observation.md`).

This document proposes a normative milestone contract for **Milestone A6: Machine-Mode Synchronous Trap Entry and Return**. It establishes the complete functional and architectural requirements for integrating Machine-mode synchronous trap entry, trap return (`MRET`), and continuous guest trap handler execution into the primary execution loop.

---

## 2. Objective

Integrate full Machine-mode synchronous trap entry and return into the simulator's core execution loop, conforming strictly to the RISC-V Privileged Architecture Specification (v1.12 / v20211203) and ADR-0001.

When a guest instruction encounters a synchronous exception:
1. The simulator completes trap entry into Machine mode without terminating the process or raising a simulator execution error.
2. Architectural trap CSRs (`mepc`, `mcause`, `mtval`, and `mstatus`) and current privilege are updated atomically.
3. The next program counter redirects to the base address configured in `mtvec` (for both Direct and Vectored modes).
4. The faulting instruction does not retire and does not advance retirement counters (`minstret`), satisfying the non-retirement invariant of ADR-0001 §2.
5. Execution seamlessly continues into the guest-installed trap handler.
6. Execution of `MRET` in Machine mode restores privilege, restores interrupt-enable state, sets `mstatus.MPIE` to 1, sets `mstatus.MPP` to User mode, and returns execution to the address stored in `mepc`.
7. Public CLI runner facilities (`ruscv-sim run`), exit signaling (`tohost` / HTIF), UART output, and cycle limits continue to function accurately during and after trap handling.

---

## 3. Scope Boundaries

### 3.1 In Scope

1. **Machine-Mode Synchronous Exception Mapping:**
   - Classification and mapping of all guest synchronous exception causes defined by the RISC-V Privileged Specification §3.1.15:
     - Cause 0: `InstructionAddressMisaligned` (misaligned fetch target address).
     - Cause 1: `InstructionAccessFault` (fetch access violation or unmapped physical memory).
     - Cause 2: `IllegalInstruction` (unrecognized opcodes, invalid format fields, executing privileged instructions without sufficient privilege, or invalid CSR access).
     - Cause 3: `Breakpoint` (`EBREAK` instruction).
     - Cause 4: `LoadAddressMisaligned` (data load from an unaligned address in an environment enforcing natural alignment).
     - Cause 5: `LoadAccessFault` (physical memory access failure or unmapped target on data load).
     - Cause 6: `StoreAddressMisaligned` (data store or AMO to an unaligned address in an environment enforcing natural alignment).
     - Cause 7: `StoreAccessFault` (physical memory access failure or unmapped target on data store or AMO).
     - Cause 8: `EcallU` (Environment call from User mode).
     - Cause 9: `EcallS` (Environment call from Supervisor mode).
     - Cause 11: `EcallM` (Environment call from Machine mode).

2. **Machine-Mode Trap CSR State Transitions:**
   - `mepc`: Written with the virtual address (PC) of the faulting instruction.
   - `mcause`: Bit 63 (Interrupt flag) set to 0; bits 62:0 set to the exception code.
   - `mtval`: Written with exception-specific diagnostic values:
     - Faulting address for misaligned address and access fault exceptions (Causes 0, 1, 4, 5, 6, 7).
     - Faulting instruction raw encoding for illegal instructions (Cause 2).
     - Zero for environment calls (Causes 8, 9, 11) and breakpoints (Cause 3).
   - `mstatus`:
     - `mstatus.MPIE` (bit 7) receives the previous value of `mstatus.MIE` (bit 3).
     - `mstatus.MIE` (bit 3) is set to 0 (disabling Machine-mode interrupts).
     - `mstatus.MPP` (bits 12:11) receives the privilege mode of the Hart immediately prior to trap entry (`0b00` for User, `0b01` for Supervisor, `0b11` for Machine).
   - `privilege`: Hart privilege unconditionally transitions to `PrivilegeMode::Machine`.

3. **`mtvec` Target Calculation Conformance (Privileged Specification §3.1.7):**
   - Correct `TrapHandler::vector_trap` logic:
     - `MODE = mtvec[1:0]`: `0b00` = Direct, `0b01` = Vectored, `>= 0b10` = Reserved (handled as Direct).
     - `BASE = mtvec[63:2] << 2` (4-byte aligned).
     - For **all** synchronous exceptions, trap entry vectors strictly to `BASE`, regardless of whether `mtvec.MODE` is Direct (0) or Vectored (1).

4. **`MRET` Instruction Semantics (Privileged Specification §3.1.6.5):**
   - Privilege validation: `MRET` is legal only in Machine mode (`privilege == PrivilegeMode::Machine`). Executing `MRET` in User or Supervisor mode raises an `IllegalInstruction` exception (Cause 2).
   - State restoration:
     - `mstatus.MIE` receives the value of `mstatus.MPIE`.
     - `mstatus.MPIE` is set to 1.
     - Privilege mode is set to the value in `mstatus.MPP`.
     - `mstatus.MPP` is set to User mode (`0b00`, the least-privileged supported mode in the RV64 profile).
     - Program counter is set to the value of `mepc`.
   - Retirement: Legally executed `MRET` instructions retire normally, incrementing retirement counters.

5. **Execution Loop Integration (ADR-0001):**
   - Integration of trap handling into `RiscvCore::step`.
   - Distinction between architectural `TrapEntered` and host `SimulatorFailure`.
   - Enforcement of the non-retirement rule (ADR-0001 §2): faulting instructions that take a synchronous trap do not increment `minstret` or cycle counts.
   - Continuous guest execution into the guest trap handler.
   - Unbroken runner exit detection (`tohost` / HTIF), UART MMIO handling, and cycle limits (`--max-cycles`).

6. **Verification Suite:**
   - Unit tests covering trap vectoring, CSR updates, privilege checks, and `MRET`.
   - Regression tests in `tests/trap_test.rs` and `tests/executor.rs`.
   - Project-authored guest bare-metal ELF tests exercising end-to-end trap entry, CSR validation, handler execution, and `MRET` return.

### 3.2 Out of Scope (Non-Goals)

The following areas are explicitly excluded from Milestone A6 to keep delivery strictly bounded:

- **Modifying `docs/dev-plan.md`:** The development plan forwarding record remains unchanged until formal maintainer review and approval.
- **Asynchronous Interrupts & Interrupt Controllers:** Interrupt lines, sampling slots, CLINT timer/software interrupts, PLIC external interrupts, interrupt priority arbitration, and `WFI` state transitions (governed separately by ADR-0004).
- **Supervisor-Mode Traps & Delegation:** `SRET`, user-mode traps (`URET`), and trap delegation registers (`medeleg` / `mideleg`). All synchronous exceptions trap directly to Machine mode.
- **Virtual Memory & MMU / Sv39 Integration:** Page table walks, `satp` address translation during traps, and page faults (Causes 12, 13, 15). Memory accesses remain physical/flat.
- **External ACT4 Test Harness Expansion:** Extension of the external ACT4 / Sail compliance harness for trap-testing suites. A6 verification relies on project-authored bare-metal ELFs and focused Rust tests.
- **Multi-Hart Coordination:** Multi-Hart trap arbitration and inter-processor interrupts.
- **Debug Mode Traps:** RISC-V Debug Mode (`dcsr`, `dpc`, `dret`) and hardware triggers.

---

## 4. Architectural Constraints and ADR Alignment

The implementation of Milestone A6 must strictly comply with the accepted architecture decision records:

1. **ADR-0001 (Hart Execution Outcome and Observation Records):**
   - A Hart step yields exactly one semantic outcome: `InstructionRetired`, `TrapEntered`, or `SimulatorFailure`.
   - A faulting instruction raising a synchronous exception produces `TrapEntered`. It does not retire, commits no destination register or memory write, and does not increment retirement counters (`minstret`).
   - `TrapEntered` requires completed architectural trap entry effects (`mepc`, `mcause`, `mtval`, `mstatus`, `privilege`, and `pc = mtvec.BASE`).
   - Host resource exhaustion, unmapped host memory, or simulator invariant failures produce `SimulatorFailure`, which halts the runner. Architectural guest exceptions must never be converted into `SimulatorFailure`.

2. **ADR-0002 (Physical-Access Transaction and Fault Contract):**
   - Memory access faults (Causes 1, 5, 7) originating from physical target rejections are mapped to their respective architectural causes based on the initial access category (fetch, load, or store/AMO).
   - Architectural address misalignment is detected before issuing a physical transaction and mapped to misalignment causes (Causes 0, 4, 6).
   - Bus/device errors do not mutate destination registers or commit partial stores.

3. **ADR-0003 (Runner, Machine, and Platform Ownership):**
   - The Runner owns run-control decisions, limits, and terminal exit presentation (`ExecutionResult`).
   - Platform exit detection via `tohost` / HTIF MMIO write ordering is preserved: an instruction inside a trap handler that writes to `tohost` retires first (`InstructionRetired`), and the Runner detects the exit signal at the step boundary.
   - Cycle budget exhaustion during trap handler execution reports `timed_out: true` without panicking.

---

## 5. Detailed Technical Specification

### 5.1 Machine-Mode Synchronous Trap Entry

When an exception occurs during instruction fetch, decode, or execution, the core executes the following atomic sequence:

```text
mepc    <- PC (virtual address of the faulting instruction)
mcause  <- (0 << 63) | exception_code
mtval   <- diagnostic_value (faulting address, instruction encoding, or 0)
mstatus.MPIE <- mstatus.MIE
mstatus.MIE  <- 0
mstatus.MPP  <- privilege (current privilege mode prior to trap)
privilege    <- PrivilegeMode::Machine
PC           <- mtvec.BASE
```

#### 5.1.1 Exception Cause and `mtval` Specification

| Cause Code | Exception Name | Trigger Condition | Architectural `mtval` Value |
| :--- | :--- | :--- | :--- |
| `0` | `InstructionAddressMisaligned` | Jump, branch, or return target PC is not aligned (PC[0] or PC[1] != 0 when uncompressed). | Faulting target address |
| `1` | `InstructionAccessFault` | Instruction fetch encounters an unmapped address or access denial. | Faulting fetch address |
| `2` | `IllegalInstruction` | Unimplemented opcode, reserved field, privileged instruction in lower mode, or invalid CSR access. | Faulting instruction word (or 0 if unavailable) |
| `3` | `Breakpoint` | Execution of `EBREAK` instruction. | 0 |
| `4` | `LoadAddressMisaligned` | Data load address not naturally aligned to access size (2, 4, or 8 bytes). | Faulting memory address |
| `5` | `LoadAccessFault` | Physical bus error, unmapped target, or permission violation on load. | Faulting memory address |
| `6` | `StoreAddressMisaligned` | Data store/AMO address not naturally aligned to access size. | Faulting memory address |
| `7` | `StoreAccessFault` | Physical bus error, unmapped target, or permission violation on store. | Faulting memory address |
| `8` | `EcallU` | `ECALL` executed while `privilege == PrivilegeMode::User`. | 0 |
| `9` | `EcallS` | `ECALL` executed while `privilege == PrivilegeMode::Supervisor`. | 0 |
| `11` | `EcallM` | `ECALL` executed while `privilege == PrivilegeMode::Machine`. | 0 |

#### 5.1.2 Vector Base Address Calculation (§3.1.7)

The `mtvec` register layout:
- Bits `[1:0]` (`MODE`):
  - `0b00`: **Direct**. All traps set `PC = BASE`.
  - `0b01`: **Vectored**. Asynchronous interrupts set `PC = BASE + 4 * cause`. **All synchronous exceptions set `PC = BASE`**.
  - `0b10` / `0b11`: Reserved. Treated as Direct mode (`PC = BASE`).
- Bits `[63:2]` (`BASE`): Base address of the trap vector, 4-byte aligned (`BASE << 2`).

**Invariant:** For all synchronous exceptions (Cause codes 0–11 above), the next PC is strictly `BASE` in both Direct and Vectored modes.

### 5.2 MRET Instruction Specification (§3.1.6.5)

The `MRET` instruction returns from a trap handled in Machine mode:

```text
// 1. Privilege Validation
if privilege != PrivilegeMode::Machine {
    raise IllegalInstruction (Cause 2)
}

// 2. Architectural State Restoration
mstatus.MIE  <- mstatus.MPIE
mstatus.MPIE <- 1
privilege    <- mstatus.MPP
mstatus.MPP  <- PrivilegeMode::User (0b00)
PC           <- mepc
```

- If `MRET` is executed in User or Supervisor mode, it raises an `IllegalInstruction` exception into Machine mode; neither `mstatus` nor the PC is updated from `mepc`.
- If executed in Machine mode, `MRET` crosses the retirement boundary, increments `minstret`, and transfers execution to `mepc`.

### 5.3 Execution Loop Integration & Invariants

1. **Step Transition Invariant:**
   In `RiscvCore::step`, when an instruction encounters a synchronous exception:
   - The trap state is written to the core's CSRs and privilege state.
   - `core.state.pc` is updated to `mtvec.BASE`.
   - The step returns `Ok(())` (indicating an architectural step completed), but registers that no instruction retired.
   - The execution loop does not break; the next iteration fetches and executes the instruction located at `mtvec.BASE`.

2. **Retirement & Counter Invariant (ADR-0001 §2):**
   - Faulting instructions are never counted as retired. `minstret` and runner instruction/cycle counters must only increment when an instruction successfully completes all operations and crosses the retirement boundary (`InstructionRetired`).
   - Trap entry (`TrapEntered`) increments no retirement counter.
   - Instructions inside the trap handler (including the concluding `MRET`) retire normally and increment retirement counters.

3. **Continuous Execution & Exit Signaling Invariant:**
   - The guest trap handler may read and write memory, manipulate CSRs, modify `mepc` (e.g. to skip past a faulting instruction by adding 4), and execute `MRET`.
   - The trap handler may signal program completion or failure by writing to `tohost` or HTIF MMIO (`0x4000_8000`). The runner detects this write immediately upon retirement of the store instruction and terminates with `RunDecision::GuestExit(code)`.
   - If a trap handler enters an infinite loop or encounters recursive unhandled traps, the runner's cycle limit terminates execution cleanly with `RunDecision::Timeout`.

---

## 6. Deliverables and Task Decomposition

The implementation of Milestone A6 is decomposed into four discrete, reviewable tasks:

### Task 1: Trap CSRs, Cause Mapping, and Vector Calculation Conformance
- Align `src/core/trap.rs` with RISC-V Privileged Specification §3.1.7:
  - Update `TrapHandler::vector_trap` to guarantee that synchronous exceptions vector to `BASE` in both Direct and Vectored modes.
  - Implement full 64-bit cause mapping and `mtval` diagnostics for Causes 0–11.
  - Ensure `mstatus` bits (`MPIE`, `MIE`, `MPP`) update accurately on trap entry.
- Unit tests verifying CSR state transitions and vector address calculations.

### Task 2: Privilege Validation and MRET Conformance
- Align `src/isa/rv64i/system.rs` (`exec_mret`) with Specification §3.1.6.5:
  - Add strict privilege check: reject execution when `privilege != PrivilegeMode::Machine` with an `IllegalInstruction` exception.
  - Update `mstatus` restoration: `MIE <- MPIE`, `MPIE <- 1`, `privilege <- MPP`, `MPP <- User` (`0b00`).
  - Restore PC from `mepc`.
- Unit tests validating legal `MRET` state restoration and illegal lower-privilege invocation rejection.

### Task 3: Core Execution Loop Trap Entry Integration
- Integrate `TrapHandler` into `RiscvCore::step` (`src/core/mod.rs`):
  - Catch synchronous exceptions raised during fetch, decode, and execution.
  - Apply trap entry state atomically (`mepc`, `mcause`, `mtval`, `mstatus`, `privilege`, and `pc`).
  - Enforce ADR-0001 non-retirement invariant: do not increment `minstret` on trap entry.
  - Enable seamless continuation of the execution loop into the guest trap handler.
- Verify that `load_and_run` and `RiscVSimulator::run` execute guest trap handlers to completion.

### Task 4: Bare-Metal ELF Test Suite and Integrated Verification
- Develop project-authored bare-metal test programs in `tests/bare-metal-riscv-test/rv64i/`:
  - `trap_ecall.S`: Configures `mtvec`, executes `ECALL` from Machine mode, verifies `mcause == 11`, increments `mepc += 4`, executes `MRET`, verifies resumed execution, and exits via `tohost`.
  - `trap_illegal.S`: Triggers an illegal instruction, verifies `mcause == 2` and `mtval`, skips instruction via `mepc += 4`, returns via `MRET`, and exits cleanly.
  - `trap_ebreak.S`: Triggers `EBREAK`, verifies `mcause == 3`, resumes, and exits.
  - `trap_vectored.S`: Sets `mtvec.MODE = 1` (Vectored), triggers synchronous exception, proves execution vectors to `BASE` (not `BASE + 4 * cause`), and returns.
  - `trap_mret_priv.S`: Verifies that attempting `MRET` outside Machine mode triggers an `IllegalInstruction` trap.
- End-to-end integration tests in `tests/trap_test.rs` validating public CLI and library facade execution of the bare-metal test suite.

---

## 7. Verification Strategy

Verification for Milestone A6 combines Rust unit tests, simulator integration tests, and compiled guest bare-metal ELF tests:

1. **Unit Testing (`cargo test --lib`):**
   - Test `TrapHandler::handle_trap` across all synchronous exception causes.
   - Verify `vector_trap` under both `MODE = 0` (Direct) and `MODE = 1` (Vectored) for exceptions (vectors to `BASE`) and interrupts (vectors to `BASE + 4 * cause`).
   - Test `exec_mret` in Machine mode for exact `mstatus` and PC updates.
   - Test `exec_mret` in User and Supervisor modes for immediate exception rejection.

2. **Integration Testing (`tests/trap_test.rs`, `tests/executor.rs`):**
   - Test stepping a core that encounters an exception: verify PC redirects to `mtvec.BASE` and subsequent step executes the handler.
   - Verify that instructions faulting into traps do not increment the cycle or retirement counts in `ExecutionResult`.
   - Verify that the CLI runner (`load_and_run`) and library facade (`RiscVSimulator`) both execute trap handlers without error.

3. **Guest Bare-Metal ELF Testing (`tests/bare-metal-riscv-test/`):**
   - Build test ELFs using the standard RISC-V GNU toolchain (`riscv64-unknown-elf-gcc`).
   - Execute all test ELFs via `ruscv-sim run <elf-file>` and verify:
     - Exit code 0 is returned via `tohost`.
     - Output matches expected values.
     - Execution completes within a bounded cycle limit.

4. **Quality Gates:**
   Every commit and pull request must satisfy the full local quality gate:
   ```bash
   cargo fmt --all -- --check
   cargo check --all-features
   cargo clippy --all-features --all-targets -- -D warnings
   cargo test --all-features
   cargo doc --all-features --no-deps
   ```

---

## 8. Testable Acceptance Criteria

Completion and acceptance of Milestone A6 require satisfying all of the following observable criteria:

1. **Synchronous Exception Trap Entry:**
   When an instruction triggers a synchronous exception (`ECALL`, `EBREAK`, illegal instruction, misaligned load/store, or access fault), the core updates `mepc` to the faulting instruction's PC, `mcause` to the specification cause code (with bit 63 clear), `mtval` to the appropriate diagnostic value (or 0), `mstatus.MPIE` to previous `MIE`, `mstatus.MIE` to 0, `mstatus.MPP` to the prior privilege mode, and transitions privilege to Machine mode.

2. **`mtvec` Target Calculation Conformance:**
   For all synchronous exceptions, execution vectors strictly to `mtvec.BASE` (clearing bits [1:0]), regardless of whether `mtvec.MODE` is configured as Direct (`0b00`) or Vectored (`0b01`).

3. **ADR-0001 Non-Retirement Invariant:**
   A step that enters a synchronous trap does not increment retirement counters (`minstret` or runner completed instruction counts). Only instructions that complete execution without trapping retire.

4. **Continuous Execution into Guest Handler:**
   Trap entry does not abort the simulator run or return a host `ExecutionError`. The core continues instruction fetching and execution starting from `mtvec.BASE`.

5. **`MRET` State Restoration:**
   Execution of `MRET` in Machine mode restores PC to `mepc`, restores `mstatus.MIE` from `mstatus.MPIE`, sets `mstatus.MPIE` to 1, sets current privilege to `mstatus.MPP`, sets `mstatus.MPP` to User mode (`0b00`), and retires as a normal instruction.

6. **`MRET` Privilege Enforcement:**
   Attempting to execute `MRET` while the core is in User or Supervisor mode raises an `IllegalInstruction` exception (Cause 2) and does not update the PC to `mepc` or alter `mstatus.MIE`.

7. **End-to-End Bare-Metal Verification:**
   All project-authored guest bare-metal trap test ELFs (`trap_ecall.elf`, `trap_illegal.elf`, `trap_ebreak.elf`, `trap_vectored.elf`, and `trap_mret_priv.elf`) run through the public CLI (`ruscv-sim run`) and terminate with exit code 0 via `tohost`.

8. **Runner Exit & Invariant Preservation:**
   Public CLI options (`--max-cycles`, `--tohost`, `--verbose`), UART character output, and cycle limit timeout detection operate identically during and after trap handling.

9. **Zero Regressions and Quality Gate Cleanliness:**
   All pre-existing tests (including A1–A5 integration tests and ACT4 compatibility baselines) and all quality gate checks (`fmt`, `check`, `clippy`, `test`, `doc`) pass without warnings or errors.

---

## 9. Next Steps and Closeout Sequence

1. **Review and Approval:** Submit this proposal for maintainer review. Upon approval, this contract replaces the forwarding record in `docs/dev-plan.md` as the sole active milestone contract.
2. **Implementation PRs:** Implement Tasks 1 through 4 on dedicated branches according to the task decomposition.
3. **Milestone Closeout:** Upon satisfying all nine acceptance criteria with recorded repository verification evidence, author the Milestone A6 capability assessment and closeout record, archive the contract to `docs/archive/milestones/`, and propose the successor contract.
