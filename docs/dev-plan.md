# Development Plan

**Project:** `ruscv-sim`

**Current milestone:** A6 — Machine-Mode Synchronous Trap Entry and Return

**Status:** Current

**Authority:** Normative milestone contract approved by maintainer authorization on 2026-09-17.
In accordance with `docs/documentation-policy.md`, this target contract does not claim
implementation completeness; simulator implementation work begins under this contract.

**Updated:** 2026-09-17

---

## 1. Context and Purpose

Following the formal closeout of Milestone A5 (ACT4 RV64I External Compatibility Baseline, PR #37,
merged `d1834cc6566b342824bca30772cc829953a5c5ef`), the repository development plan maintained an
A5 forwarding record until a successor was approved. On 2026-09-17, maintainer authorization
approved Milestone A6 as the sole current milestone contract. The post-closeout forwarding record is
preserved in [`docs/archive/milestones/a5-forwarding-record.md`](archive/milestones/a5-forwarding-record.md).

During Milestones A0–A5, architectural synchronous exceptions (such as `ECALL`, `EBREAK`, illegal
instructions, misaligned memory addresses, and physical access faults) were treated as non-trapping
exclusions or converted into simulator host execution errors:
- `RiscvCore::step` in `src/core/mod.rs` halts execution on fetch, decode, or execution failures,
  returning an `anyhow::Result::Err` that outer runners convert into a fatal execution error
  (`RunDecision::ExecutionError`).
- The existing `TrapHandler` in `src/core/trap.rs` contains standalone component logic and tests, but
  is not integrated into the public fetch-decode-execute loop in `RiscvCore::step`.
- In `src/core/trap.rs`, the vectored trap calculation (`TrapHandler::vector_trap`) incorrectly
  offsets synchronous exceptions by `(cause & 0x7F) << 2`, whereas the RISC-V Privileged
  Architecture Specification §3.1.7 mandates that all synchronous exceptions vector to `BASE`
  regardless of whether `mtvec.MODE` is Direct (`0b00`) or Vectored (`0b01`).
- The `MRET` implementation in `src/isa/rv64i/system.rs` (`exec_mret`) lacks privilege validation
  (allowing execution outside Machine mode), omits setting `mstatus.MPIE` to 1, does not explicitly
  transition `mstatus.MPP` to User mode (`0b00`), and omits clearing `mstatus.MPRV` when returning to
  a privilege mode below Machine mode, as mandated by RISC-V Privileged Specification v1.12 §3.1.6.1 / §3.1.6.5.
- The public execution loop (`RiscvCore::step`) uses 32-bit instruction fetch (`read_word`) and advances
  PC by 4 bytes, without supporting 16-bit compressed instructions. The public `Executor` dispatches
  32-bit base integer instructions as well as M, A, F, and D opcodes (per `src/execute/mod.rs`), though
  as established in `AGENTS.md`, dispatch presence is distinguished from end-to-end verified ISA support.
  Inspection of `src/csr/mod.rs` shows that `misa` is initialized to `0x8000_0000_0010_0100` (MXL=2 for
  RV64, bit 8 for I, bit 20 for U; bit 2 for C is 0). The inline source comment `// RV64IMAC` is an
  erroneous, stale comment that contradicts the actual bit value. However, current `CsrFile::write` has
  a generic fallback that permits arbitrary writes to `misa`. For Milestone A6, the architectural alignment
  boundary is defined as 32-bit fetch / C-disabled / fixed IALIGN=32. Making `misa.C` WARL fixed to 0
  (rejecting attempts to set bit 2 via direct write or CSR write/set/clear) and hardwiring `mepc[1:0]`
  to zero are explicit implementation deliverables of Milestone A6.
- The simulator does not currently implement the architectural `minstret` CSR (`0xB02`) in `CsrFile`.
  Faulting instructions and trap entry do not currently adhere to the non-retirement semantics defined
  in accepted architecture decision record ADR-0001 (`docs/architecture/decisions/0001-hart-execution-outcome-and-observation.md`)
  or the started-slot budget versus completed-turn accounting and non-lossy control delivery in ADR-0003
  and ADR-0004 (`docs/architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md`).

This document establishes the normative milestone contract for **Milestone A6: Machine-Mode
Synchronous Trap Entry and Return**. It integrates full Machine-mode synchronous trap entry,
trap return (`MRET`), guest trap handler execution, and architectural `minstret` retirement counter
tracking into the primary execution loop.

---

## 2. Objective

Integrate full Machine-mode synchronous trap entry and return into the simulator's core execution loop,
strictly conforming to the RISC-V Privileged Architecture Specification (v1.12 / v20211203) and
accepted architecture decisions ADR-0001 through ADR-0004.

When a guest instruction encounters a synchronous exception:
1. The Hart completes trap entry into Machine mode atomically: architectural trap CSRs (`mepc`,
   `mcause`, `mtval`, `mstatus`) and current privilege update to their architecturally mandated states.
2. The program counter redirects to the base address configured in `mtvec` (`mtvec.BASE`) for both
   Direct and Vectored modes.
3. The faulting instruction does not retire, commits no destination register or memory write, and does
   not advance retirement counters (`minstret`), satisfying ADR-0001 §2.
4. Under the explicitly adopted `continue-to-guest-handler` continuation policy (ADR-0004 §10.3), the
   Hart delivers the `TrapEntered` control fact to the Runner at each completed trap boundary. The
   Runner consumes the fact (making it available for diagnostics and observation) before driving the
   next step into the guest trap handler starting from `mtvec.BASE`, ensuring no completed trap fact
   is overwritten unconsumed.
5. Counting and turn-budget contract (ADR-0003, ADR-0004):
   - Outer execution budget (`--max-cycles`) bounds **started turn slots** (instruction attempts, trap
     entries, and simulator failures).
   - Execution progress counter (`ExecutionResult.cycles`) reports **completed execution turns**.
   - A step ending in `SimulatorFailure` consumes 1 started turn slot from the budget, but does NOT
     advance completed turns (`ExecutionResult.cycles`) or retirement (`minstret`), and halts the
     runner with `RunDecision::ExecutionError` without fabricating `BudgetExhausted`.
   - Pure recursive unhandled traps consume 1 started turn slot per trap, advance `ExecutionResult.cycles`,
     leave `minstret` unchanged, and cleanly terminate when `--max-cycles` is exhausted via
     `RunDecision::Timeout` (`timed_out: true`).
6. Architectural Counter Delivery and Write Precedence: Provide single authoritative 64-bit retirement
   counter storage in the Hart's `CsrFile` under address `machine::MINSTRET` (`0xB02`), with verified
   coherence between direct CSR read/write access and core execution retirement updates. For instructions
   that do not write `minstret`, `minstret` increments by 1 upon `InstructionRetired`. For an instruction
   that explicitly writes `minstret`, the written value takes precedence over and suppresses the implicit
   +1 retirement increment for that instruction (Privileged Spec v1.12 §3.1.11), while retirement progress
   and turn budget account for the instruction normally. `minstret` remains unchanged on `TrapEntered`
   or `SimulatorFailure`. The unprivileged `instret` shadow (`0xC02`), counter access control registers
   (`mcounteren` / `scounteren`), and `mcycle` (`0xB00`) are explicitly deferred to successor milestones.
7. Execution of `MRET` in Machine mode restores privilege from `mstatus.MPP`, restores `mstatus.MIE` from
   `mstatus.MPIE`, sets `mstatus.MPIE` to 1, sets `mstatus.MPP` to User mode (`0b00`), clears `mstatus.MPRV`
   to 0 if returning to a mode below Machine (User or Supervisor) while preserving `mstatus.MPRV` if
   returning to Machine mode, returns execution to `mepc`, and retires as a normal instruction.
   Executing `MRET` outside Machine mode triggers an `IllegalInstruction` trap entry without applying
   MRET restoration side effects.
8. Public CLI runner facilities (`ruscv-sim run`), exit signaling (`tohost` / HTIF), UART MMIO, and
   cycle limits operate seamlessly during and after trap handling.

---

## 3. Scope Boundaries

### 3.1 In Scope

1. **Machine-Mode Synchronous Exception Mapping:**
   Classification and mapping of all guest synchronous exception causes defined by RISC-V Privileged
   Specification §3.1.15 for RV64I:
   - Cause 0: `InstructionAddressMisaligned` (misaligned target PC on jump or taken branch).
     - *Alignment Boundary:* The execution loop operates with 32-bit instruction fetch, compressed
       instructions disabled, and fixed IALIGN=32. Milestone A6 establishes `misa.C` as WARL fixed to 0.
     - *mepc WARL Rule:* Under fixed IALIGN=32, `mepc[1:0]` are hardwired to zero (Privileged Spec
       §3.1.14 / §3.1.6.5); writing any address to `mepc` (via direct write or CSR write/set/clear)
       masks bits `[1:0]`. Therefore, `MRET` restoring `PC <- mepc` restores a 4-byte aligned PC and
       cannot produce an instruction address misalignment exception.
     - *Architectural Trigger:* Cause 0 is strictly induced when an unconditional jump (`JAL`, `JALR`)
       or taken conditional branch evaluates to an unaligned target PC (bits `[1:0] != 0`).
     - *Effects:* `mepc` receives the address of the jump/branch instruction; `mtval` receives the
       misaligned target address. The instruction does not retire.
     - *Compatibility Boundary:* Preserves Milestone A5 ACT4 RV64I external compatibility and existing
       M/A/F/D dispatch without introducing compressed instruction execution into the public ELF path.
   - Cause 1: `InstructionAccessFault` (fetch access violation or unmapped physical memory).
     - *Architectural Trigger:* Instruction fetch targets an unmapped physical address or is rejected
       by the physical memory bus.
     - *Effects:* `mepc` receives the faulting fetch address; `mtval` receives the faulting fetch address.
   - Cause 2: `IllegalInstruction` (unrecognized opcodes, reserved fields, privileged instructions in
     insufficient privilege mode, or invalid CSR access).
     - *Effects:* `mepc` receives the instruction address; `mtval` receives the faulting instruction word
       (or 0 if unavailable).
   - Cause 3: `Breakpoint` (`EBREAK` instruction).
     - *Effects:* `mepc` receives the `EBREAK` instruction address; `mtval` is set to 0.
   - Cause 4: `LoadAddressMisaligned` (data load from an address not naturally aligned to access size).
     - *Architectural Trigger:* `LH`, `LW`, or `LD` with unaligned memory address.
     - *ADR-0002 Invariant:* Alignment is verified by the Hart before issuing a physical transaction;
       no physical read transaction is dispatched to the bus.
     - *Effects:* `mepc` receives the load instruction address; `mtval` receives the misaligned address.
       Destination register `rd` is not modified; the instruction does not retire.
   - Cause 5: `LoadAccessFault` (physical memory access failure or unmapped target on data load).
     - *Architectural Trigger:* Aligned load targeting unmapped physical memory or receiving a bus error.
     - *Effects:* `mepc` receives the load instruction address; `mtval` receives the faulting physical
       address. Destination register `rd` is not modified; the instruction does not retire.
   - Cause 6: `StoreAddressMisaligned` (data store or AMO to an unaligned address).
     - *Architectural Trigger:* `SH`, `SW`, `SD`, or AMO with unaligned memory address.
     - *ADR-0002 Invariant:* Alignment is verified before issuing a physical transaction; no physical
       write transaction is dispatched to the bus.
     - *Effects:* `mepc` receives the store instruction address; `mtval` receives the misaligned address.
       No memory byte is modified; the instruction does not retire.
   - Cause 7: `StoreAccessFault` (physical memory access failure or unmapped target on data store/AMO).
     - *Architectural Trigger:* Aligned store/AMO targeting unmapped physical memory or receiving a bus error.
     - *Effects:* `mepc` receives the store instruction address; `mtval` receives the faulting physical
       address. No partial store commits to memory; the instruction does not retire.
   - Cause 8: `EcallU` (`ECALL` executed while `privilege == PrivilegeMode::User`).
   - Cause 9: `EcallS` (`ECALL` executed while `privilege == PrivilegeMode::Supervisor`).
   - Cause 11: `EcallM` (`ECALL` executed while `privilege == PrivilegeMode::Machine`).

2. **Machine-Mode Trap CSR State Transitions:**
   Atomic state update on synchronous exception trap entry:
   - `mepc`: Written with the virtual address (PC) of the faulting instruction.
   - `mcause`: Bit 63 (Interrupt) set to 0; bits 62:0 set to the exception code.
   - `mtval`: Written with exception-specific diagnostic values per §3.1.1.
   - `mstatus`:
     - `mstatus.MPIE` (bit 7) receives the previous value of `mstatus.MIE` (bit 3).
     - `mstatus.MIE` (bit 3) is set to 0 (disabling Machine-mode interrupts).
     - `mstatus.MPP` (bits 12:11) receives the privilege mode of the Hart immediately prior to trap
       entry (`0b00` for User, `0b01` for Supervisor, `0b11` for Machine).
   - `privilege`: Hart privilege unconditionally transitions to `PrivilegeMode::Machine`.

3. **`mtvec` Target Calculation Conformance (Privileged Specification §3.1.7):**
   - Correct `TrapHandler::vector_trap` logic:
     - `MODE = mtvec[1:0]`: `0b00` = Direct, `0b01` = Vectored, `>= 0b10` = Reserved (handled as Direct).
     - `BASE = mtvec[63:2] << 2` (4-byte aligned).
     - For **all** synchronous exceptions (Causes 0–11), trap entry vectors strictly to `BASE`,
       regardless of whether `mtvec.MODE` is Direct (`0b00`) or Vectored (`0b01`).

4. **`MRET` Instruction Semantics (Privileged Specification §3.1.6.1 / §3.1.6.5):**
   - Strict privilege validation: `MRET` is legal only in Machine mode (`privilege == PrivilegeMode::Machine`).
   - Rejection semantics: Attempting `MRET` outside Machine mode raises an `IllegalInstruction`
     exception (Cause 2). No MRET return restoration side effects occur (neither `PC` nor `privilege`
     is restored from `mepc`/`MPP`, `mstatus.MIE` is not restored from `MPIE`, and `mstatus.MPRV` is not
     modified by MRET), and standard `IllegalInstruction` trap entry into Machine mode completes (saving
     faulting PC to `mepc`, setting `mstatus.MPIE <- mstatus.MIE`, `mstatus.MIE <- 0`, and
     `mstatus.MPP <- prior_privilege`).
   - Legal state restoration:
     - `mstatus.MIE` receives the value of `mstatus.MPIE`.
     - `mstatus.MPIE` is set to 1.
     - Privilege mode is set to the value in `mstatus.MPP`.
     - `mstatus.MPP` is set to User mode (`0b00`, the least-privileged supported mode in RV64).
     - **`mstatus.MPRV` (bit 17) clearance**: When the new privilege mode established by `mstatus.MPP`
       is less than Machine mode (User `0b00` or Supervisor `0b01`), `mstatus.MPRV` is set to 0. When
       the new privilege mode is Machine mode (`0b11`), `mstatus.MPRV` remains unchanged. (This conforms
       strictly to Privileged Spec v1.12 §3.1.6.5 without misattributing unconstrained Machine access to
       lower-privilege instructions).
     - Program counter is set to the value of `mepc` (4-byte aligned per WARL `mepc[1:0] == 00`).
   - Retirement: Legally executed `MRET` instructions retire normally, incrementing `minstret` by 1.

5. **Execution Loop Integration, Continuation Policy, and Non-Lossy Counting Contract (ADR-0001, ADR-0003, ADR-0004):**
   - Distinction between Hart semantic outcomes: `InstructionRetired`, `TrapEntered`, and `SimulatorFailure`.
     The Hart does not report a generic, undifferentiated `Ok(())` that conceals trap entry as retirement.
   - Selected continuation policy: The execution engine explicitly adopts `continue-to-guest-handler`
     (ADR-0004 §10.3).
   - **Per-Trap Control Delivery & Consumption**: At every architectural step where a synchronous
     exception occurs, the Hart establishes trap entry state and returns the `TrapEntered` outcome
     (carrying cause, faulting PC, vector target PC, and continuation policy) across the Hart boundary.
     The Runner consumes this control fact at that exact trap boundary (accounting for the started turn
     budget slot, updating run statistics, and presenting the fact to any attached diagnostic listener)
     before driving the next turn starting from `mtvec.BASE`.
   - **Non-Lossy Multi-Trap Invariant**: Under `continue-to-guest-handler`, consecutive, recursive, or
     heterogeneous traps deliver distinct `TrapEntered` facts at each trap boundary; each is consumed
     by the Runner in architectural order without being overwritten unconsumed.
   - **Always-Present Control Facts vs Subscriber-Gated Observations**: Always-present control facts
     drive outer execution without requiring heap allocation or unbounded retention. When an observer
     subscriber is attached, structured `TrapRecord` / `CommitRecord` data is materialized. When
     observation is disabled, no unbounded retention or per-step record allocation occurs.
   - **Counting and Bounded Execution Contract**:
     - `minstret` counts strictly retired instructions (`InstructionRetired`).
     - For instructions that complete `InstructionRetired` without explicitly writing to `minstret`,
       `minstret` increments by 1 upon retirement.
     - For instructions that complete `InstructionRetired` and explicitly write to `minstret`, the
       written value takes precedence over and suppresses the implicit +1 retirement increment for that
       instruction (Privileged Spec v1.12 §3.1.11), leaving the counter at the written value at the end
       of the step. The instruction produces `InstructionRetired`, advances completed turns (`ExecutionResult.cycles`),
       and consumes 1 started turn slot from the budget.
     - Faulting instructions (`TrapEntered`) and simulator failures (`SimulatorFailure`) do not retire
       and never increment `minstret`.
     - Turn budget (`--max-cycles`) limits **started turn slots** (instruction attempts, trap entries,
       and simulator failures).
     - Public execution counter (`ExecutionResult.cycles`) reports **completed execution turns**.
     - Five distinct observable test scenarios:
       1. **Zero budget (`--max-cycles 0`)**: consumes 0 started slots; executes 0 turns; immediately
          halts with `RunDecision::Timeout` (`timed_out: true`, `cycles == 0`, `minstret == 0`).
       2. **Normal retirement**: consumes 1 started turn slot; completes 1 turn; increments
          `ExecutionResult.cycles` by 1. For ordinary instructions, `minstret` increments by 1; for instructions
          explicitly writing `minstret`, the written value is committed without subsequent +1 increment.
       3. **Synchronous trap entry**: consumes 1 started turn slot; completes 1 turn; increments
          `ExecutionResult.cycles` by 1; `minstret` remains unchanged.
       4. **Pure recursive exception**: each trap entry consumes 1 started turn slot, completes 1 turn,
          increments `ExecutionResult.cycles` by 1, and leaves `minstret` unchanged; when `--max-cycles`
          is exhausted, cleanly halts with `RunDecision::Timeout` (`timed_out: true`, `cycles == max_cycles`).
       5. **SimulatorFailure**: a step that encounters a simulator/host failure consumes 1 started turn
          slot from the budget, but does NOT complete a turn: leaves `ExecutionResult.cycles` unchanged,
          leaves `minstret` unchanged, and halts with `RunDecision::ExecutionError` (`error.is_some()`,
          `timed_out: false`), without fabricating `BudgetExhausted` even if occurring in the final budget slot.
   - Exit signaling: Writes to `tohost` or HTIF MMIO within trap handlers retire normally, and exit
     detection terminates the run with `RunDecision::GuestExit(code)`.

6. **Architectural Counter Deliverable and Explicit Write Precedence (`minstret`):**
   - Provide single authoritative 64-bit storage in the Hart's `CsrFile` under address `machine::MINSTRET`
     (`0xB02`).
   - No duplicate sibling `CoreState.minstret` field is introduced, avoiding duplicate-authority and
     synchronization hazards.
   - Initialized to 0 on core reset.
   - Machine-mode read and write access.
   - **Explicit Counter Write Precedence (Privileged Spec v1.12 §3.1.11)**:
     - An explicit write to `minstret` takes precedence over and overrides the implicit +1 increment by
       the writing instruction itself; the written value is not subsequently incremented by the retirement
       of that instruction. The written value is observed by subsequent instructions.
     - **CSR Instruction Write Classification (RISC-V Zicsr Specification)**:
       - `CSRRW` / `CSRRWI`: Always performs an explicit write to the CSR (regardless of `rs1` or `zimm`
         value). The destination register `rd` (if `rd != x0`) receives the previous counter value, and
         the new value is written to `minstret`. The written value overrides the implicit increment
         (e.g., `CSRRW x0, minstret, x0` leaves `minstret == 0`, not 1).
       - `CSRRS` / `CSRRC`: Performs a write if and only if `rs1 != x0`. If `rs1 == x0`, the instruction
         is strictly read-only; no write occurs, `rd` receives the old value, and the instruction's normal
         retirement increment (+1) applies to `minstret`. If `rs1 != x0`, an explicit write occurs; the
         computed bit-set or bit-clear value is committed and overrides the implicit increment.
       - `CSRRSI` / `CSRRCI`: Performs a write if and only if `zimm != 0`. If `zimm == 0`, the instruction
         is strictly read-only; no write occurs, and normal retirement increment (+1) applies. If `zimm != 0`,
         an explicit write occurs and overrides the implicit increment.
     - The *subsequent* retired instruction (assuming it does not write `minstret`) increments `minstret`
       by 1 from the previously written value.
     - On `TrapEntered` or `SimulatorFailure`, no instruction retires, no implicit increment occurs, and
       no write by a faulting instruction commits.
   - Core execution retirement updates and direct CSR read/write access operate directly on this single
     authoritative value in `CsrFile`.

7. **Verification Suite:**
   - Unit tests covering trap vectoring, CSR transitions, privilege validation, `MRET` (with MPRV
     transitions), `minstret` CSR read/write, write-precedence vs read-only CSR classification,
     `misa.C` WARL non-writability, and `mepc[1:0]` WARL masking.
   - Integration tests in `tests/trap_test.rs` and `tests/executor.rs` verifying loop continuation,
     per-trap boundary fact consumption during multi-trap execution, induced-fault paths for Causes 0,
     1, 4, 5, 6, 7, five-scenario counter/budget separation, counter write-precedence during execution,
     timeout bounds, and exit detection.
   - Project-authored bare-metal test programs in `tests/bare-metal-riscv-test/rv64i/`.

### 3.2 Out of Scope (Non-Goals)

The following areas are explicitly excluded from Milestone A6 to keep delivery strictly bounded:

- **`mcycle` and Hardware Performance Counters:** The architectural cycle counter `mcycle` (`0xB00`)
  and arbitrary hardware performance counters (`mhpmcounter*`) are explicitly deferred to a successor
  milestone covering interrupt and timer architecture (ADR-0004); Milestone A6 delivers `minstret` for
  instruction retirement tracking and does not claim full ADR-0004 cycle counter or virtual time conformance.
- **Unprivileged Counter Shadows & Access Control Registers:** The unprivileged `instret` read shadow
  (`0xC02`) and counter privilege gating registers (`mcounteren` / `scounteren`) are explicitly deferred;
  Milestone A6 delivers strictly Machine-mode `minstret` (`0xB02`).
- **Compressed Instructions (C Extension) and Variable IALIGN:** The public execution path operates
  strictly with 32-bit instruction fetch and fixed IALIGN=32; compressed instruction execution and
  dynamic IALIGN=16 switching remain out of scope for A6. Standalone compressed instruction components
  in `src/isa/rv64c/` remain an unwired component boundary.
- **Whole-Machine Extension Certification:** Milestone A6 scopes trap entry and return; existing
  instruction dispatch for M, A, F, and D opcodes in `src/execute/mod.rs` is neither disabled nor
  claimed as end-to-end verified ISA support.
- **Asynchronous Interrupts & Interrupt Controllers:** Interrupt lines, sampling slots, CLINT timer/software
  interrupts, PLIC external interrupts, interrupt priority arbitration, and `WFI` state transitions
  (governed separately by ADR-0004).
- **Supervisor-Mode Traps & Delegation:** `SRET`, user-mode traps (`URET`), and trap delegation registers
  (`medeleg` / `mideleg`). All synchronous exceptions trap directly to Machine mode.
- **Virtual Memory & MMU / Sv39 Integration:** Page table walks, `satp` address translation during traps,
  and page faults (Causes 12, 13, 15). Memory accesses remain physical/flat.
- **External ACT4 Test Harness Expansion:** Extension of the external ACT4 / Sail compliance harness for
  trap-testing suites. A6 verification relies on project-authored bare-metal ELFs and focused Rust tests.
- **Multi-Hart Coordination:** Multi-Hart trap arbitration and inter-processor interrupts.
- **Debug Mode Traps:** RISC-V Debug Mode (`dcsr`, `dpc`, `dret`) and hardware triggers.

---

## 4. Architectural Constraints and ADR Alignment

Implementation of Milestone A6 must strictly comply with accepted architecture decision records:

1. **ADR-0001 (Hart Execution Outcome and Observation Records):**
   - One Hart step yields exactly one semantic outcome: `InstructionRetired`, `TrapEntered`, or
     `SimulatorFailure`.
   - A faulting instruction raising a synchronous exception produces `TrapEntered`. It does not retire,
     commits no destination register or memory write, does not advance PC as fall-through, and does not
     increment retirement counters (`minstret`).
   - `TrapEntered` requires completed architectural trap entry effects (`mepc`, `mcause`, `mtval`,
     `mstatus`, `privilege`, and `pc = mtvec.BASE`).
   - Host resource exhaustion, unmapped host memory, unsupported legal instructions, or simulator
     invariant failures produce `SimulatorFailure`, which halts the runner with execution error.
     Internal simulator errors must never be converted into guest trap entries.
   - Classification of illegal instructions: An encoding defined as illegal by the active RV64I profile
     raises an `IllegalInstruction` trap (Cause 2). An encoding that is architecturally legal but
     unsupported by the implementation is a `SimulatorFailure`.

2. **ADR-0002 (Physical-Access Transaction and Fault Contract):**
   - Architectural address misalignment (Causes 0, 4, 6) is detected by the Hart before issuing a
     physical transaction; no physical transaction is issued for misaligned accesses.
   - Memory access faults (Causes 1, 5, 7) originate from physical target rejections (e.g. unmapped
     address or target access denial) of valid aligned transactions and map to their respective causes
     based on the initial access category (fetch, load, or store/AMO).
   - Bus/device errors do not mutate destination registers or commit partial stores.
   - Transport/adapter protocol failures remain `SimulatorFailure`.

3. **ADR-0003 (Runner, Machine, and Platform Ownership):**
   - The Runner owns run-control decisions, limits (`--max-cycles`), observer delivery, and terminal exit
     presentation (`ExecutionResult`).
   - `ExecutionResult.cycles` preserves the public completed-cycle semantics; failed attempts ending in
     `SimulatorFailure` do not advance this counter.
   - The Hart owns architectural state, instruction semantics, traps, counter updates, and outcome facts.
     The Hart's `CsrFile` holds the single authoritative storage for the retirement counter (`machine::MINSTRET`),
     accessed coherently by core retirement and CSR operations without duplicate sibling synchronization.
   - Platform exit detection via `tohost` / HTIF MMIO write ordering is preserved: an instruction
     inside a trap handler that writes to `tohost` retires first (`InstructionRetired`), and the
     Runner detects the exit signal at the step boundary.

4. **ADR-0004 (Interrupt, Time, Scheduling, and Stop-Event Boundaries):**
   - §10.3 Continuation Policy: The minimal ISS baseline policy defaults to `stop-on-synchronous-trap`.
     Milestone A6 explicitly exercises the permitted alternative by adopting `continue-to-guest-handler`
     while delivering and consuming the trap fact at each trap boundary.
   - §1.1 & §2 Counting & Turn Budget: A Hart turn is one granted architectural transition slot.
     `--max-cycles` serves as the outer turn budget bounding started turn slots. Each started turn
     (retiring instruction, trap entry, or simulator failure) consumes 1 turn budget slot.
   - §10.3 Primary Reason Rank: `SimulatorFailure` (rank 1) outranks `BudgetExhausted` (rank 9). If the
     N-th budget slot fails, the terminal reason is `SimulatorFailure`, not `BudgetExhausted`.
   - Counter Ownership and Write Precedence: `minstret` is Hart-owned and tracks retired instructions.
     Explicit counter writes take precedence over same-instruction retirement increments (Privileged Spec
     v1.12 §3.1.11), while the writing instruction completes retirement accounting normally. `mcycle` is
     explicitly deferred; A6 does not define a cycle timing profile or claim full ADR-0004 counter conformance.

5. **Alignment Consistency and MISA.C WARL Policy:**
   - The public execution engine operates with 32-bit instruction fetch, compressed instructions disabled,
     and fixed IALIGN=32.
   - Initial `misa` value `0x8000_0000_0010_0100` (`MXL=2`, `I=1`, `U=1`, `C=0`) has `C=0`. Milestone A6
     delivers explicit WARL enforcement for `misa`: bit 2 (`C`) cannot be set to 1 by direct CSR writes
     or CSR write/set/clear instructions.
   - Under fixed IALIGN=32, `mepc[1:0]` is hardwired to zero (`mepc & !0b11`).
   - `MRET` restores PC from `mepc` with masked bits `[1:0] == 00` and cannot produce an `InstructionAddressMisaligned` exception.

---

## 5. Detailed Technical Specification

### 5.1 Machine-Mode Synchronous Trap Entry

When a synchronous exception occurs during instruction fetch, decode, or execution, the core executes
the following atomic sequence:

```text
mepc         <- PC (virtual address of faulting instruction)
mcause       <- (0 << 63) | exception_code
mtval        <- diagnostic_value (faulting address, instruction encoding, or 0)
mstatus.MPIE <- mstatus.MIE
mstatus.MIE  <- 0
mstatus.MPP  <- privilege (current privilege mode prior to trap)
privilege    <- PrivilegeMode::Machine
PC           <- mtvec.BASE
```

#### 5.1.1 Exception Cause and `mtval` Specification

| Cause Code | Exception Name | Trigger Condition | Architectural `mtval` Value |
| :--- | :--- | :--- | :--- |
| `0` | `InstructionAddressMisaligned` | Computed jump or branch target PC is not 4-byte aligned (target[1:0] != 0) under fixed IALIGN=32. | Faulting target address |
| `1` | `InstructionAccessFault` | Instruction fetch encounters unmapped address or physical access denial. | Faulting fetch address |
| `2` | `IllegalInstruction` | Unrecognized opcode, reserved field, privileged instruction in lower mode, or invalid CSR access. | Faulting instruction word (or 0 if unavailable) |
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
- Bits `[63:2]` (`BASE`): Base address of trap vector, 4-byte aligned (`mtvec[63:2] << 2`).

**Invariant:** For all synchronous exceptions (Causes 0–11), the next PC is strictly `BASE` in both
Direct and Vectored modes.

### 5.2 MRET Instruction Specification (§3.1.6.1 / §3.1.6.5)

The `MRET` instruction returns from a trap handled in Machine mode:

```text
// 1. Privilege Validation
if privilege != PrivilegeMode::Machine {
    raise IllegalInstruction (Cause 2)
    // No MRET state restoration occurs; standard trap entry into Machine mode applies:
    // mepc <- mret_pc, mcause <- 2, mtval <- mret_instr,
    // mstatus.MPIE <- mstatus.MIE, mstatus.MIE <- 0, mstatus.MPP <- prior_mode,
    // privilege <- Machine, PC <- mtvec.BASE
}

// 2. Architectural State Restoration (Machine mode only)
mstatus.MIE  <- mstatus.MPIE
mstatus.MPIE <- 1
new_priv     <- mstatus.MPP
privilege    <- new_priv
mstatus.MPP  <- PrivilegeMode::User (0b00)
if new_priv < PrivilegeMode::Machine {
    mstatus.MPRV <- 0
}
PC           <- mepc // mepc[1:0] hardwired to 00 under fixed IALIGN=32
// MRET instruction retires normally (minstret += 1, as MRET does not write minstret)
```

### 5.3 Execution Loop Integration and Invariants

1. **Step Transition and Non-Lossy Outcome Fact Delivery:**
   In `RiscvCore::step`, when an instruction encounters a synchronous exception:
   - Trap entry state is atomically applied to the core's CSRs, privilege, and PC (`mtvec.BASE`).
   - The Hart reports the `TrapEntered` outcome (not `InstructionRetired` and not `SimulatorFailure`).
   - The step leaves destination registers and memory unmodified.
   - The Runner receives the `TrapEntered` fact at the trap boundary, consumes it (making cause,
     faulting PC, vector target PC, and continuation policy observable), charges one started slot to
     the budget, and continues execution at `mtvec.BASE`.
   - Subsequent traps in the same run deliver their facts at their respective boundaries, ensuring no
     trap fact is lost unconsumed.

2. **Retirement, Counter Write Precedence, and Turn Budget Invariant:**
   - For an instruction that completes `InstructionRetired` without explicitly writing to `minstret`,
     `minstret` increments by 1.
   - For an instruction that completes `InstructionRetired` and explicitly writes to `minstret`, the
     written value is committed and overrides the implicit +1 increment for that instruction (Privileged
     Spec v1.12 §3.1.11). The instruction retires and counts normally in completed turns (`ExecutionResult.cycles`).
   - Trap entry (`TrapEntered`) and simulator failure (`SimulatorFailure`) never increment `minstret`.
   - `--max-cycles` limits total started turn slots (instruction attempts, trap entries, and simulator failures).
   - `ExecutionResult.cycles` reports completed turns.
   - Pure recursive exceptions consume 1 started slot and complete 1 turn per trap, leaving `minstret`
     unchanged, and cleanly stop when `--max-cycles` is exhausted with `RunDecision::Timeout` (`timed_out: true`).
   - Simulator failures consume 1 started slot, leave `ExecutionResult.cycles` and `minstret` unadvanced,
     and return `RunDecision::ExecutionError` (`timed_out: false`).

3. **Continuous Execution & Exit Signaling Invariant:**
   - The guest trap handler may read and write memory, manipulate CSRs, modify `mepc` (e.g. `mepc += 4`
     to skip a faulting instruction), and execute `MRET`.
   - A store instruction writing an exit code to `tohost` / HTIF MMIO within the trap handler retires
     normally (`InstructionRetired`), and the Runner detects exit immediately at the step boundary,
     returning `RunDecision::GuestExit(code)`.

---

## 6. Deliverables and Task Decomposition

The implementation of Milestone A6 is decomposed into four discrete, reviewable tasks:

### Task 1: Trap CSRs, Cause Mapping, Vector Calculation, and Counter Storage
- Align `src/core/trap.rs` with RISC-V Privileged Specification §3.1.7:
  - Update `TrapHandler::vector_trap` to guarantee that all synchronous exceptions vector to `BASE`
    in both Direct and Vectored modes.
  - Implement full 64-bit cause mapping and `mtval` diagnostics for Causes 0–11.
  - Ensure `mstatus` bits (`MPIE`, `MIE`, `MPP`) update accurately on trap entry.
- Implement single authoritative 64-bit storage in `CsrFile` under `machine::MINSTRET` (`0xB02`) with
  Machine-mode read/write access in `src/csr/mod.rs`.
- Enforce `misa.C` WARL policy in `src/csr/mod.rs`: bit 2 (`C`) remains fixed to 0 across direct CSR writes
  and CSR instructions (`CSRRW`, `CSRRS`, `CSRRC`). Correct the stale inline comment `// RV64IMAC` in `src/csr/mod.rs`.
- Enforce `mepc` WARL policy under fixed IALIGN=32: hardwire `mepc[1:0]` to zero (`mepc & !0b11`) on direct
  writes and CSR instructions.
- Implement CSR write classification for `minstret` access:
  - `CSRRW` / `CSRRWI`: always writes and activates write-precedence.
  - `CSRRS` / `CSRRC`: writes only if `rs1 != x0`; read-only if `rs1 == x0`.
  - `CSRRSI` / `CSRRCI`: writes only if `zimm != 0`; read-only if `zimm == 0`.
- Unit tests verifying CSR state transitions, vector address calculations, `minstret` CSR read/write,
  `minstret` write precedence and read-only gating, `misa.C` WARL non-writability, and `mepc[1:0]` masking.

### Task 2: Privilege Validation and MRET Conformance
- Align `src/isa/rv64i/system.rs` (`exec_mret`) with Specification §3.1.6.1 and §3.1.6.5:
  - Add strict privilege check: reject execution when `privilege != PrivilegeMode::Machine` with an
    `IllegalInstruction` exception.
  - Guarantee that lower-privilege invocation applies standard `IllegalInstruction` trap entry without
    MRET return restoration side effects.
  - Update legal `mstatus` restoration: `MIE <- MPIE`, `MPIE <- 1`, `privilege <- MPP`, `MPP <- User` (`0b00`).
  - Implement `mstatus.MPRV` clearance: set `mstatus.MPRV <- 0` when returning to User or Supervisor
    mode; preserve `mstatus.MPRV` when returning to Machine mode.
  - Restore PC from `mepc`.
- Unit tests validating legal `MRET` state restoration (including both lower-privilege MPRV clearance
  and Machine-mode MPRV preservation) and illegal lower-privilege rejection.

### Task 3: Core Execution Loop Trap Entry Integration, Continuation Policy, and Counter Updates
- Integrate `TrapHandler` and continuation policy into `RiscvCore::step` (`src/core/mod.rs`) and
  runners (`src/executor.rs`):
  - Catch synchronous exceptions raised during fetch, decode, and execution.
  - Apply trap entry state atomically (`mepc`, `mcause`, `mtval`, `mstatus`, `privilege`, and `pc`).
  - Report `TrapEntered` outcome fact distinct from `InstructionRetired` and `SimulatorFailure`.
  - Enforce ADR-0001 non-retirement: do not increment `minstret` on trap entry or simulator failure.
  - Apply retirement counter updates with explicit write precedence (Privileged Spec §3.1.11):
    - If the retired instruction explicitly wrote `minstret`, commit the written value and suppress
      the implicit +1 increment for that instruction.
    - If the retired instruction did not write `minstret`, increment `minstret` by 1.
  - Apply ADR-0003 / ADR-0004 counting: charge 1 started turn slot per attempt/trap entry/failure.
    `SimulatorFailure` consumes a started slot but does not advance `ExecutionResult.cycles` or `minstret`,
    reporting execution error without fabricating `BudgetExhausted`.
  - Enable seamless continuation into guest trap handlers under `continue-to-guest-handler` while
    delivering and consuming the bounded trap fact at each trap boundary.
- Integration tests in `tests/trap_test.rs` and `tests/executor.rs`.

### Task 4: Bare-Metal and Induced-Fault Integration Suite
- Develop project-authored bare-metal test programs in `tests/bare-metal-riscv-test/rv64i/`:
  - `trap_ecall.S`: Configures `mtvec`, executes `ECALL` from Machine mode, verifies `mcause == 11`,
    increments `mepc += 4`, executes `MRET`, verifies resumed execution, and exits via `tohost`.
  - `trap_illegal.S`: Triggers an illegal instruction, verifies `mcause == 2` and `mtval`, skips
    instruction via `mepc += 4`, returns via `MRET`, and exits cleanly.
  - `trap_ebreak.S`: Triggers `EBREAK`, verifies `mcause == 3`, resumes, and exits.
  - `trap_vectored.S`: Sets `mtvec.MODE = 1` (Vectored), triggers synchronous exception, proves
    execution vectors to `BASE` (not `BASE + 4 * cause`), and returns.
  - `trap_mret_priv.S`: Verifies that attempting `MRET` outside Machine mode triggers an
    `IllegalInstruction` trap without MRET restoration side effects.
- Develop focused integration tests in `tests/trap_test.rs` inducing Causes 0, 1, 4, 5, 6, 7 through
  genuine execution and physical bus rejection paths:
  - Induce Cause 0 via computed jump/branch to misaligned target PC; verify `mepc`, `mtval`, and non-retirement.
  - Induce Cause 1 via instruction fetch from unmapped physical memory.
  - Induce Cause 4 via misaligned load; verify no physical memory read dispatched and `rd` unmodified.
  - Induce Cause 5 via aligned load encountering physical bus rejection; verify `rd` unmodified.
  - Induce Cause 6 via misaligned store; verify no physical memory write dispatched and memory unmodified.
  - Induce Cause 7 via aligned store encountering physical bus rejection; verify memory unmodified.
- Record RISC-V toolchain gap and execution status in test runners.
- End-to-end integration tests in `tests/trap_test.rs` validating public CLI and library facade.

---

## 7. Verification Strategy

Verification for Milestone A6 combines Rust unit tests, simulator integration tests, and bare-metal ELF tests:

1. **Unit Testing (`cargo test --lib`):**
   - Test `TrapHandler::handle_trap` across all synchronous exception causes.
   - Verify `vector_trap` under both `MODE = 0` (Direct) and `MODE = 1` (Vectored) for exceptions
     (vectors to `BASE`) and interrupts (vectors to `BASE + 4 * cause`).
   - Test `exec_mret` in Machine mode for exact `mstatus` updates:
     - Case 1: Return to User or Supervisor mode sets `mstatus.MPRV = 0`.
     - Case 2: Return to Machine mode preserves initial `mstatus.MPRV`.
   - Test `exec_mret` in User and Supervisor modes for immediate exception rejection and standard
     `IllegalInstruction` trap entry.
   - Test `minstret` CSR read/write in Machine mode and reset value.
   - Test `minstret` explicit write precedence:
     - `CSRRW` / `CSRRWI`: verify that writing value `V` leaves `minstret == V` (not `V + 1`), even
       when writing 0 via `rs1 == x0` or `zimm == 0`.
     - `CSRRS` / `CSRRC`: verify that `rs1 == x0` is strictly read-only (`rd` receives old value,
       instruction retirement increments counter by +1); verify that `rs1 != x0` writes the new value
       and overrides the implicit +1 increment.
     - `CSRRSI` / `CSRRCI`: verify that `zimm == 0` is read-only (counter increments by +1); verify that
       `zimm != 0` writes the new value and overrides the implicit +1 increment.
     - Subsequent instruction: verify the instruction following a counter write increments `minstret` by +1.
   - Test `misa.C` WARL behavior: verify that writing, setting, or clearing bits on `misa` leaves bit 2
     (`C`) fixed to 0.
   - Test `mepc[1:0]` WARL masking: verify that direct write or CSR write/set/clear with non-zero low
     bits masks bits `[1:0]` to zero under fixed IALIGN=32.

2. **Integration Testing (`tests/trap_test.rs`, `tests/executor.rs`):**
   - Test stepping a core that encounters an exception: verify PC redirects to `mtvec.BASE` and
     subsequent steps execute the handler.
   - **Induced-Fault Integration Scenarios (Causes 0, 1, 4, 5, 6, 7)**:
     - Cause 0: execute computed jump/branch to misaligned target PC; verify `mcause == 0`, `mtval == target`,
       `mepc == jump_pc`, no instruction retirement.
     - Cause 1: fetch from unmapped physical address; verify `mcause == 1`, `mtval == fetch_addr`,
       `mepc == fetch_addr`, no instruction retirement.
     - Cause 4: misaligned load; verify `mcause == 4`, `mtval == load_addr`, no physical memory read
       issued, destination register `rd` unmodified, no retirement.
     - Cause 5: aligned load encountering physical bus rejection; verify `mcause == 5`, `mtval == load_addr`,
       destination register `rd` unmodified, no retirement.
     - Cause 6: misaligned store; verify `mcause == 6`, `mtval == store_addr`, no physical memory write
       issued, memory unmodified, no retirement.
     - Cause 7: aligned store encountering physical bus rejection; verify `mcause == 7`, `mtval == store_addr`,
       memory unmodified, no retirement.
   - **Multi-Trap Non-Lossy Delivery**: verify that consecutive/heterogeneous traps in a single execution
     run deliver their respective `TrapEntered` facts at each trap boundary to the Runner without unconsumed
     overwriting.
   - **Counter Coherence and Write Precedence in Execution**: verify that retirement counter updates via
     core execution and direct `CsrFile` read/write access operate on the same authoritative storage,
     and verify that an instruction executing `csrw minstret, rs1` sets `minstret` to `rs1` without subsequent
     increment, while the next instruction increments it to `rs1 + 1`.
   - **Five Counting Scenarios**:
     1. Zero budget (`--max-cycles 0`): 0 started slots, immediate timeout (`timed_out: true`, `cycles: 0`, `minstret: 0`).
     2. Normal retirement: advances `minstret` and `cycles` by 1 per retired instruction (unless explicitly written).
     3. Synchronous trap entry: advances `cycles` by 1, leaves `minstret` unchanged.
     4. Pure recursive exception: advances `cycles` until `--max-cycles` timeout with `timed_out: true`,
        leaving `minstret` unchanged.
     5. SimulatorFailure: consuming the final budget slot halts with `RunDecision::ExecutionError`
        (`error.is_some()`, `timed_out: false`), without fabricating `BudgetExhausted` and leaving
        `cycles` and `minstret` unadvanced.
   - Verify that the CLI runner (`load_and_run`) and library facade (`RiscVSimulator`) both execute
     trap handlers without error.

3. **Guest Bare-Metal ELF Testing & Toolchain Status:**
   - Toolchain requirement: GNU `riscv64-unknown-elf` toolchain (`as`, `ld`, or `gcc`).
   - Environment status: The local development host environment currently lacks `riscv64-unknown-elf-gcc`
     in PATH. The container image defined in [`docs/development-environment.md`](development-environment.md)
     and GitHub Actions CI (`.github/workflows/ci.yml`) supply the cross-toolchain.
   - Test runner strategy: Bare-metal tests dynamically verify toolchain availability before assembling,
     skipping assembly when the cross-compiler is absent, while CI compiles and runs the suite.
   - Verified execution via `ruscv-sim run <elf-file>` checks: exit code 0 via `tohost`, expected output,
     and completion within cycle budget.

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

1. **Synchronous Exception Trap Entry & Induced Fault Conformance:**
   When an instruction triggers a synchronous exception (`ECALL`, `EBREAK`, illegal instruction,
   misaligned load/store, or access fault), the core updates `mepc` to the faulting instruction's PC,
   `mcause` to the specification cause code (with bit 63 clear), `mtval` to the appropriate diagnostic
   value (or 0), `mstatus.MPIE` to previous `MIE`, `mstatus.MIE` to 0, `mstatus.MPP` to the prior
   privilege mode, and transitions privilege to Machine mode.
   Causes 0, 1, 4, 5, 6, 7 are verified via genuine execution paths inducing the fault:
   - Misaligned jumps/branches (Cause 0) evaluate computed target misalignment before fetch under fixed
     IALIGN=32 and do not retire. `MRET` restores `mepc` with masked bits `[1:0] == 00` without generating Cause 0.
   - Misaligned load/store (Causes 4, 6) issue no physical transaction and leave registers/memory unmodified.
   - Physical access faults (Causes 1, 5, 7) commit no partial register or memory state.
   - `misa.C` is verified as WARL fixed to 0: attempts to set bit 2 via direct CSR write or CSR instructions
     (`CSRRW`, `CSRRS`, `CSRRC`) leave `misa.C == 0`, preserving fixed IALIGN=32.
   - `mepc[1:0]` is verified as hardwired to zero across direct writes and CSR operations.

2. **`mtvec` Target Calculation Conformance:**
   For all synchronous exceptions, execution vectors strictly to `mtvec.BASE` (clearing bits [1:0]),
   regardless of whether `mtvec.MODE` is configured as Direct (`0b00`) or Vectored (`0b01`).

3. **ADR-0001 Non-Retirement, ADR-0004 Budget Accounting, and `minstret` Delivery with Write Precedence:**
   - Single authoritative 64-bit retirement counter storage in the Hart's `CsrFile` under `machine::MINSTRET`
     (`0xB02`), initialized to 0, with verified coherence between core execution retirement updates and
     direct CSR read/write access.
   - **Counter Write Precedence**: An instruction that explicitly writes to `minstret` commits its written
     value, overriding the implicit +1 increment for that instruction (Privileged Spec v1.12 §3.1.11); the
     instruction produces `InstructionRetired`, advances `ExecutionResult.cycles`, and consumes 1 started
     turn slot. The subsequent instruction increments `minstret` by 1 from the written value.
   - **CSR Instruction Classification**:
     - `CSRRW` / `CSRRWI`: Always performs a write, overriding the implicit increment (e.g. `csrw minstret, x0`
       leaves `minstret == 0`).
     - `CSRRS` / `CSRRC`: If `rs1 == x0`, strictly read-only; no write occurs, `rd` receives the old value,
       and the normal +1 retirement increment applies. If `rs1 != x0`, writes the computed value and overrides
       the implicit +1 increment.
     - `CSRRSI` / `CSRRCI`: If `zimm == 0`, strictly read-only (normal +1 applies). If `zimm != 0`, writes
       the computed value and overrides the implicit +1 increment.
   - Five distinct observable counting behaviors are verified:
     - Zero budget (`--max-cycles 0`): executes 0 turns, immediately halts with `timed_out: true`, `cycles: 0`, `minstret: 0`.
     - Normal retirement: advances `minstret` by 1 and `cycles` by 1 per retired instruction (unless explicitly written).
     - Synchronous trap entry: advances `cycles` by 1, leaving `minstret` unchanged.
     - Pure recursive exception: advances `cycles` until `--max-cycles` is exhausted, cleanly halting
       with `RunDecision::Timeout` (`timed_out: true`), with `cycles == max_cycles` and `minstret` unchanged.
     - SimulatorFailure: consumes 1 started slot from budget, leaves `cycles` and `minstret` unchanged,
       and halts with `RunDecision::ExecutionError` (`error.is_some()`, `timed_out: false`), without
       fabricating `BudgetExhausted` even if occurring on the final budget slot.

4. **Continuous Execution into Guest Handler & Non-Lossy Trap Delivery:**
   Trap entry does not abort the simulator run or return a host `ExecutionError`. Under the selected
   `continue-to-guest-handler` policy, the Hart delivers the `TrapEntered` control fact (cause, faulting
   PC, vector target PC, and continuation policy) to the Runner at each trap boundary; the Runner
   consumes the fact and continues execution at `mtvec.BASE`. Sequential or heterogeneous traps in the
   same run deliver their respective trap facts at each boundary without unconsumed overwriting, and
   without requiring per-instruction heap allocation when observation is disabled.

5. **`MRET` State Restoration and MPRV Clearance in Machine Mode:**
   Execution of `MRET` in Machine mode restores PC to `mepc`, restores `mstatus.MIE` from `mstatus.MPIE`,
   sets `mstatus.MPIE` to 1, sets current privilege to `mstatus.MPP`, sets `mstatus.MPP` to User mode
   (`0b00`), and retires as a normal instruction (`minstret += 1`).
   Furthermore, if returning to User mode (`0b00`) or Supervisor mode (`0b01`), `mstatus.MPRV` is set to 0;
   if returning to Machine mode (`0b11`), `mstatus.MPRV` is preserved.

6. **`MRET` Privilege Enforcement & Rejection Semantics:**
   Attempting to execute `MRET` while the core is in User or Supervisor mode raises an `IllegalInstruction`
   exception (Cause 2). MRET return restoration side effects do not occur (`PC` is not restored from `mepc`,
   `privilege` is not restored from `mstatus.MPP`, `mstatus.MIE` is not restored from `mstatus.MPIE`, and
   `mstatus.MPRV` is not modified by MRET).
   The core completes standard `IllegalInstruction` trap entry into Machine mode (`mepc <- mret_pc`,
   `mcause <- 2`, `mstatus.MPIE <- mstatus.MIE`, `mstatus.MIE <- 0`, `mstatus.MPP <- prior_privilege`,
   `privilege <- Machine`, `PC <- mtvec.BASE`).

7. **End-to-End Bare-Metal Verification:**
   All project-authored guest bare-metal trap test ELFs (`trap_ecall.elf`, `trap_illegal.elf`,
   `trap_ebreak.elf`, `trap_vectored.elf`, and `trap_mret_priv.elf`) run through the public CLI
   (`ruscv-sim run`) and terminate with exit code 0 via `tohost`.

8. **Runner Exit, Diagnostics, and Invariant Preservation:**
   Public CLI options (`--max-cycles`, `--tohost`, `--verbose`), UART character output, and cycle limit
   timeout detection operate identically during and after trap handling.

9. **Zero Regressions and Quality Gate Cleanliness:**
   All pre-existing tests (including A1–A5 integration tests and ACT4 compatibility baselines) and all
   quality gate checks (`fmt`, `check`, `clippy`, `test`, `doc`) pass without warnings or errors.

---

## 9. Next Steps and Closeout Sequence

1. **Implementation Tasks:** Implement Tasks 1 through 4 on dedicated branches according to the task
   decomposition.
2. **Milestone Closeout:** Upon satisfying all nine acceptance criteria with recorded repository
   verification evidence:
   - Verify every acceptance criterion with recorded repository evidence.
   - Author the Milestone A6 capability assessment and closeout record.
   - Archive the completed contract to `docs/archive/milestones/a6-machine-mode-trap-entry-return.md`.
   - Propose and obtain approval for the successor milestone contract before replacing `docs/dev-plan.md`.
