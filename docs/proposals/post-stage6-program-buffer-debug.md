# Successor contract draft: program-buffer debug execution

**Status:** Draft. This proposal is not approved, active, or an implementation claim.

**Authority:** Informational successor proposal, constrained by the current [Stage 6 contract](../dev-plan.md) and accepted ADR-0001–0004. `docs/dev-plan.md` remains the only current contract.

**Drafted:** 2026-10-28.

## 1. Proposed objective and entry

Complete the next bounded debug increment after Stage 6: OpenOCD executes a program buffer through the existing Hart, including memory operations and fences, while the target remains in Debug Mode. The Debug Module owns the command and buffer. The Machine schedules the Hart's debug work at its existing coherent boundary. There is one Hart and one instruction implementation.

This is a proposed sequencing choice. The [recorded later debug direction](post-a7-roadmap.md#stage-6--risc-v-debug-mode) includes a program buffer, system bus access, and instruction execution in Debug Mode. This proposal delivers the program-buffer portion first and retains SBA as a separate subsequent contract. It does not call that subset full Spike-style support.

Entry requires all Stage 6 acceptance criteria and its cumulative final review to be recorded, any findings to be fixed, and the applicable final-head checks to pass. The initial review at `4332841a565fb845952cd52ccf84283959f373bf` reproduced an unresolved-completion reset bypass and loss of a halt request across reset and selection changes. Both are corrected at `5c1d031b2a66bc17d058b5a7355d88ebe80eb652`, with an independent cumulative code APPROVE and successful CI. The [Stage 6 acceptance overview](../verification/stage6-closeout/README.md) records the separate local-gate and delivery boundaries. Those results do not approve this proposal or complete Stage 6.

The inherited performance-certification waiver continues. It does not claim comparison publication, noise calibration, or a sealed A10 oracle. The Stage 4 archive records inheritance by Stage 5 and later milestones unless a later contract explicitly takes that work on. This proposal does not take it on.

## 2. Recommended scope

The specification remains RISC-V Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. The privileged/unprivileged reference remains the ISA Manual document version 20250508. The reference ISS remains Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`; named behavioral comparisons are evidence, not an oracle hash.

Recommended product choices, all pending owner approval:

| Decision | Recommended choice | Consequence |
| --- | --- | --- |
| D1. Next delivery | Program-buffer debug execution before SBA or TLM/SystemC. | Continues the recorded debug objective without introducing a second physical initiator in this contract. |
| D2. Buffer shape | Two 32-bit writable program-buffer words, implicit EBREAK, `datacount=4`. | `progbufsize=2`, `impebreak=1`; preserve existing abstract register and memory operations. Tool acceptance includes distinct-value, multi-element physical-memory transfers. |
| D3. Execution model | A non-addressable program-buffer instruction source supplied by the DM to the existing Hart; no ROM or park loop. | The Hart reuses its decoder/executor. An explicit, narrow ADR-0002 amendment permits this instruction source; all guest fetches and all debug data, atomic, and page-table accesses retain `PhysicalAccess`. |
| D4. ISA profile | Add Zifencei to the current RV64IMA/Zicsr profile, including the guest path. | OpenOCD's program-buffer fences can execute. FENCE.I must not become legal only in a hidden debugger path. |
| D5. Saved state | No implicit change to `dpc`; preserve intended explicit GPR, CSR, and memory writes. | Buffer sequencing has its own cursor. An explicit write to `dpc` still changes the later resume PC. The contract chooses preservation where the Debug specification permits unspecified `dpc`. |
| D6. Bounded buffer profile | In Debug Mode, reject all control transfers, all PC-dependent instructions including AUIPC, and ECALL/MRET/SRET/DRET with `cmderr=3`; service DMI between debug instructions. | These uniform optional restrictions are allowed by Sdext. At most two buffer words and the implicit EBREAK are attempted; no arbitrary timeout or new CLI budget is needed. Guest execution retains its normal instruction semantics. |
| D7. SBA sequencing | Keep SBA as a separately scoped subsequent debug increment. | Its independent Platform-master port, reservation visibility, and lifecycle rules need an accepted ADR-0002 amendment before implementation. |
| D8. Automatic execution | Implement `abstractauto.autoexecdata` for the four implemented data registers; unimplemented data bits and `autoexecprogbuf` remain zero. | OpenOCD can repeat the last accepted abstract command for bulk transfers. Reading a data register captures its value before triggering execution; writes stage their value before execution. Busy and sticky-error rules still apply. |
| D9. Component API | Permit the `DebugModule` DMI read entry to require mutable DM/Hart access for automatic-execution side effects; migrate all in-tree callers in the same unit. | This is a proposed breaking change to that component method's Rust signature. Public CLI commands, debugger transport protocol, and `ExecutionResult` keep their existing shape. |

D4 is an explicit extension proposal. Stage 6's existing FENCE.I-illegal behavior remains authoritative until a successor contract accepting this change activates. If the owner chooses to retain that behavior, this draft's fence-success objective and tool acceptance must be revised before activation.

D8 is required for the proposed ordinary bulk program-buffer workflow with OpenOCD 0.12.0. Its read path enables automatic execution after the first element, and its write path depends on it for subsequent elements. A single-element-only alternative would exclude ordinary bulk program-buffer transfers and must be presented as a narrower product choice, not inferred from a successful one-element demo.

OpenOCD 0.12.0 only enables its MPRV/MPP helper with at least five effective program-buffer words, counting the implicit EBREAK. Four writable words plus an implicit EBREAK satisfy that condition; the selected two-word profile does not. This profile therefore proves tool-driven physical memory and proves effective-privilege/Sv39 behavior through a direct Hart/DMI fixture. It does not claim OpenOCD virtual-memory access.

## 3. Architectural constraints

The Hart owns Debug Mode instruction semantics, privilege, CSR legality, translation, exceptions, and the architectural effects of completed instructions. The Debug Module owns buffer contents, the pending command, `busy`, and sticky `cmderr`. The Machine owns grants, input admission, lifecycle, and command progress. The transport moves DMI bits and never executes an instruction.

Program-buffer instructions use the existing instruction implementations and Hart-issued physical data access. A load, store, AMO, LR/SC, or page-table access is not implemented as an unrestricted host-memory operation. There is no DM-originated Platform access in this contract. The buffer is not RAM, has no guest-visible address, and cannot be read or self-modified with loads/stores. An accepted ADR-0002 amendment must define its DM-supplied instruction-source exception without changing ordinary guest fetch or any data-access rule.

Debug instruction completion is distinct from guest retirement and from completion of an abstract command. A program-buffer exception terminates the command with `cmderr=3`, without entering a guest handler or rewriting guest trap CSRs. Already completed instruction effects remain committed. Unknown completion retains the existing terminal uncertainty rules and blocks reset/retry/reuse without adapter proof.

While executing the buffer, the Hart remains architecturally halted in Debug Mode and masks interrupts. WFI completes without waiting. EBREAK ends buffer execution. Control transfers, PC-dependent instructions, and the selected unspecified privilege-changing instructions terminate with `cmderr=3`. Falling through the second word executes the implicit EBREAK. Ordinary debug CSR accesses become legal in this execution context, while guest accesses remain illegal. `mprven=1` applies the specification's selected effective-privilege behavior to debug loads/stores; it is not the unconditional physical-access rule of Abstract Access Memory.

With the selected fixed `stopcount=1` and `stoptime=1`, debug execution adds no automatic guest `mcycle`, `minstret`, `mtime`, or `ExecutionResult.cycles` charge. Explicit writes to writable counter CSRs retain their normal defined effects. Debug command progress is recorded separately. No new public field on `ExecutionResult` is proposed.

Before implementation, accepted amendments to ADR-0001/0002/0003/0004 must define debug-work facts, the program-buffer instruction source, the completion/error distinction, scheduling, observation, accounting, and drain. They must retain the single-Hart engine and non-lossy failure semantics. `dret` remains illegal under the recommended non-ROM design. A ROM-based alternative requires a separate explicit change to that selected rule.

## 4. Sequenced units and acceptance

These are proposed future deliverables, not current test or implementation claims.

| Unit | Behavior and required proof |
| --- | --- |
| U1. Hart debug execution | Execute the existing RV64IMA/Zicsr instruction classes in Debug Mode, plus Zifencei if D4 is accepted. Prove arithmetic, ordinary/debug CSR access, WFI completion, EBREAK termination, interrupt masking, saved `dpc`, and no automatic guest counter changes. Exceptions preserve nonzero `mepc/mcause/mtval` and stop before a following buffer instruction. |
| U2. Buffer and abstract-command protocol | Read/write both words and report capacity. Access Register performs `transfer` before `postexec`, supports `transfer=0`, and executes the command once. Implement the four `autoexecdata` bits and repeat the last accepted command after the specified data read/write ordering. Prove every implemented trigger bit, masked unimplemented bits, `busy`, accesses while busy, sticky error and W1C, implicit EBREAK, and recovery after error clear. Clearing automatic execution must stop repeats. |
| U3. Machine and physical effects | Grant debug work without holding the DM lock across execution. Prove that a control-transfer buffer reports `cmderr=3` and DMI remains responsive; no buffer can loop. Exercise RAM, PLIC claim, AMO, LR/SC, effective privilege, Sv39, and rejected accesses through the Hart path. Preserve unknown-completion fencing and DM request state across reset. |
| U4. Public tools | Extend the attach script to force OpenOCD program-buffer physical-memory access and prove it did not fall back to abstract memory. Write and read at least four adjacent elements with different values, and independently verify all addresses through abstract memory. Exercise supported widths, register access, fences, buffer-exception recovery, reset halt, resume, and exit. A direct Hart/DMI fixture covers MPRV/Sv39; the tool test does not claim that capability with the two-word profile. Run named comparisons against the fixed Spike with matching buffer settings and document allowed differences. |
| U5. Exit | Run the full Rust gate, existing fresh guest suite, a new guest proving Zifencei on the ordinary path if selected, and unit-specific negative controls. Record a cumulative independent final PR-head review, accurate CI identity, known limits, and the unchanged A10 oracle. |

Every unit needs a positive observable result and a negative control that breaks that result. Required controls are wrong `progbufsize`, omitted implicit EBREAK, re-executed `postexec`, disabled `autoexecdata` during a multi-element transfer, a guest trap on buffer exception, and an incorrectly accepted control transfer. Each control's exact patch, focused test command, failure assertion, and reachable source commit must be recorded before exit.

The D6 restrictions apply uniformly to branch/JAL/JALR targets both inside and outside the buffer, including a backward target and a not-taken branch. AUIPC is illegal in the buffer. ECALL/MRET/SRET/DRET produce the selected exception result. Explicit EBREAK terminates successfully, and fall-through reaches the implicit EBREAK. Self-modification is unavailable because the instruction source is not addressable RAM. These are proposed profile choices permitted by Debug Specification 1.0 Sdext; they are not copied from Spike behavior.

## 5. Exclusions and retained work

This proposed increment excludes SBA and any inbound Platform master; a second hart; Sdtrig; `autoexecprogbuf`; Quick Access; F/D/V additions; new virtualization or protection profiles; SystemC/TLM/DMI integration; Linux/SBI; and performance certification or oracle sealing. Existing Stage 6 abstract access and public non-debug execution remain regression requirements.

SBA remains a subsequent target. Its contract must define DM-originated physical transactions, target side effects, write visibility to reservations, future DMI invalidation, `sberror`, unknown completion, and lifecycle drain. Halting the only Hart does not make SBA a Hart-initiated access or remove that architectural obligation.

The conditional Stage 7 transport direction remains available when there is a concrete external consumer and a reproducible fixture. Its minimal sequence is blocking Rust TLM parity, then a pinned SystemC FFI and external-kernel exchange, then DMI for a qualified RAM region. Existing component presence is not integration evidence. Neither that direction nor this draft is activated by the current request to finish Stage 6.

## 6. Approval and rotation boundary

The owner must select the successor direction and approve the resulting contract and its ADR amendments. D1–D9 above are recommended choices, all currently undecided. This draft is a concrete recommendation for that decision, not a second current contract.

Only after Stage 6 acceptance is complete and a successor contract is approved may the rotation preserve the entire Stage 6 contract under `docs/archive/milestones/` and replace `docs/dev-plan.md`. The rotation audit must verify whole-body preservation and the single-current-contract invariant. Completion date, exact implementation/gate/review evidence, and known limits belong in the Stage 6 record. Git and PR history record the delivery identity.

## Sources

- [Debug Specification 1.0, Program Buffer and SBA](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc).
- [Sdext Debug Mode execution rules](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/Sdext.adoc).
- [Debug core registers and `dpc`](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/xml/core_registers.xml).
- [Fixed Spike Debug Module](https://github.com/riscv-software-src/riscv-isa-sim/blob/5d99611782ef8dd81b65b842369bd351a9d4702a/riscv/debug_module.cc) and [Hart implementation](https://github.com/riscv-software-src/riscv-isa-sim/blob/5d99611782ef8dd81b65b842369bd351a9d4702a/riscv/processor.cc).
- [Fixed OpenOCD 0.12.0 program-buffer transfers and privilege helper](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
- [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
