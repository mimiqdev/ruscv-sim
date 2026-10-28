# Development Plan

**Current milestone on rotation merge:** Program-buffer Debug Execution

**Status:** Current approved target, activated by this contract-rotation merge. Until that merge, `main` retains Stage 6 as Current. On merge this is the sole Current contract and the [entire Stage 6 contract](archive/milestones/stage6-riscv-debug-mode.md) becomes historical. Activation changes no simulator behavior and makes no implementation-completeness claim.

**Authority:** Normative milestone contract, constrained by accepted ADR-0001–0004 and their 2026-10-28 program-buffer amendments. Source code and behavioral tests establish implemented behavior; CI and PR history retain check and review outcomes.

**Approved:** 2026-10-28, owner Tony. Decisions D1–D9 are DECIDED, all recommended choices. The [proposal entry](proposals/post-stage6-program-buffer-debug.md) is informational, not another current contract.

## 1. Objective and inherited profile

Extend the single-hart Stage 6 debugger with program-buffer execution through the existing Hart. OpenOCD must perform ordinary multi-element physical-memory transfers using the program buffer, execute fences, and retain existing abstract access and run control. The Hart stays in Debug Mode while executing the buffer.

The inherited profile is RV64IMA/Zicsr, M/S/U, IALIGN=32, the selected Stage 5 platform/interrupt/WFI rules, and Stage 6 Debug Specification 1.0 behavior. This contract adds Zifencei to both ordinary guest and debug execution. It adds no F/D/C/V extension, trigger module, second hart, SBI, or Linux target. Existing zero-entry PMP and Sv39 behavior remain unchanged.

This is the next bounded debug increment. The later recorded goal still includes system bus access (SBA). This contract delivers the program-buffer portion first; it does not call that subset full Spike-style support or introduce a second physical initiator.

The inherited Stage 3 performance-certification waiver continues. Comparison publication, noise calibration, thresholds, oracle sealing and new performance payloads are not taken on. `tools/a10/public-v1.json` and the suite's lack of an oracle pin remain unchanged.

## 2. Specification and selected decisions

The normative Debug reference is RISC-V Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. The ISA Manual reference is document version 20250508. OpenOCD is 0.12.0. The comparison ISS is Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`; comparison checks named behavior, not an output digest.

Every decision below was owner approved on 2026-10-28.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery order | Program-buffer debug execution before SBA or TLM/SystemC. |
| D2. Capacity | Two writable 32-bit words, implicit 32-bit EBREAK, `progbufsize=2`, `impebreak=1`, and `datacount=4`. |
| D3. Instruction source | A non-addressable DM-owned program buffer supplies opcodes to the existing Hart through the Machine's debug grant. No debug ROM or park loop. The ADR-0002 exception applies only to this instruction source; ordinary guest fetch and every data/atomic/page-table transaction retain `PhysicalAccess`. |
| D4. ISA | Zifencei is legal on the ordinary guest path and in the buffer. FENCE.I must not be secretly legal only for the debugger. |
| D5. Saved state | Buffer sequencing does not implicitly change `dpc`. Intended explicit GPR, CSR and memory writes retain their effects; an explicit `dpc` write changes the later resume PC. |
| D6. Bounded execution | In the buffer, reject all control transfers, all PC-dependent instructions including AUIPC, and ECALL/MRET/SRET/DRET with `cmderr=3`. Service DMI between debug instructions. At most two writable words and the implicit EBREAK are attempted; no new CLI work budget is added. |
| D7. SBA | Retain SBA as a separately scoped subsequent increment. Its independent Platform-master contract requires an accepted ADR-0002 amendment before implementation. |
| D8. Automatic execution | Implement `abstractauto.autoexecdata` for data0–data3. Other data bits and all `autoexecprogbuf` bits read zero and ignore writes. Reads capture their returned value before triggering execution; writes stage their value before execution. |
| D9. Rust component API | The `DebugModule` DMI read entry may require mutable DM/Hart access for read side effects. This specific component-signature break is permitted; migrate all in-tree callers in the same unit. Public CLI commands, transport protocol and `ExecutionResult` retain their shape. |

OpenOCD's privilege helper requires five effective program-buffer words, counting implicit EBREAK. Four writable words plus an implicit EBREAK qualify; this two-word profile does not. Tool-driven memory acceptance is physical. A direct Hart/DMI fixture must prove MPRV/Sv39 behavior; this contract does not claim OpenOCD virtual-memory access for the two-word configuration.

## 3. Ownership and architectural semantics

### Hart and instruction source

The Hart alone decodes and executes instructions, owns privilege/CSR legality, translation, reservations, exceptions, and architectural effects. The DM owns the two words, command/data/autoexec state, `busy`, and sticky errors. The Machine admits DMI and grants debug work at coherent Hart boundaries. The transport moves bits.

A debug grant may supply an opcode from the DM buffer rather than perform a physical instruction fetch. The source is not RAM, has no guest-visible address, and cannot be read or self-modified through load/store. It cannot inject instructions into normal guest execution. Reuse the same decoder/executor; neither the DM nor transport may implement another ISA loop.

Every buffer data load/store, AMO, LR/SC operation, and page-table access uses the Hart's existing physical boundary. The instruction-source exception does not permit host-memory shortcuts, an alternate data backend, or SBA. Target faults, host/protocol errors and unknown completion retain their existing distinctions.

### Execution and exceptions

The Hart remains in Debug Mode. Implemented instructions operate with Debug Mode's M privilege, except the selected MPRV/MPP, translation and permission rules for data access. `mprven=1` and `relaxedpriv=0` remain selected. This differs from physical Abstract Access Memory, which retains its Stage 6 semantics.

Interrupts are masked. WFI completes as a NOP. Explicit EBREAK ends the command successfully; falling through the second word reaches the implicit EBREAK. Compressed instructions remain outside the profile. D6 applies uniformly to branch/JAL/JALR targets inside and outside the buffer, including backward targets and not-taken branches; AUIPC and the selected privilege-changing instructions are illegal in this execution context. Ordinary guest control flow is unchanged.

A buffer exception stops before any following word, leaves the Hart halted, and sets `cmderr=3`. It does not enter a guest handler or rewrite guest trap CSRs. Earlier completed effects remain committed. An unsupported/FPR access, known simulator fault, or unresolved completion must use the selected abstract-command error semantics; unknown completion also leaves the Machine non-resumable and cannot be cleared by W1C, DM reset or `ndmreset`.

`dcsr.cause` continues to describe entry into Debug Mode; terminating a buffer does not create a new halt cause. `dcsr.prv` and `dpc` retain their saved-state roles. Explicit legal writes to debug CSRs remain visible, while guest instructions outside Debug Mode still cannot access those CSRs. A guest step request does not single-step or interrupt an internal buffer command.

### Command and run control

Access Register completes `transfer` before `postexec`; `transfer=0` still permits execution. A command is executed once. `busy` reflects an active command; a busy/error-blocked command write does not replace the command being executed. Automatic data access repeats the last COMMAND write accepted while idle with `cmderr=0`, with the specified read/write ordering. Sticky errors suppress new execution until cleared. Implemented and unimplemented autoexec bits follow D8.

Program-buffer writes cannot alter an active command's words. Apply the specification's busy-error behavior, and test that ignored writes have no architectural effect. The command can advance without further socket input; DMI is serviced between instruction grants. No DM lock is held while the Hart executes or accesses a physical target.

Resume cannot preempt an unfinished command. Completion/error facts become visible before any permitted guest resume, using the existing selected-hart, halt-request and resume-ack rules. A reset request stops further debug grants, drains already-started work, and follows the existing reset policy. Aborting an active command through non-DM reset reports `cmderr=4`; DM-owned buffer, data, autoexec and request state otherwise survive that reset. DM reset clears its own command state and does not automatically resume the Hart. Neither operation supplies proof for unknown completion.

### Accounting and observable effects

Debug instruction completion, buffer exception and abstract-command completion are distinct from guest retirement and guest trap entry. They do not fabricate guest `CommitRecord`, `TrapRecord`, instruction-attempt or retirement facts. Any optional debug observations originate from the same Hart effects; disabled observation does not remove required control facts.

With the selected `stopcount=1` and `stoptime=1`, debug work adds no automatic `mcycle`, `minstret`, `mtime`, guest-turn budget, `iss_tick`, or `ExecutionResult.cycles` charge. Explicit counter/CSR/MMIO writes retain defined effects. This milestone introduces no new nonzero transaction-delay or external-kernel timing model. The existing physical-delay ownership rules are not weakened.

Committed MMIO effects remain Platform effects with debug-operation provenance. The Machine returns them at the issuing instruction boundary without inventing a guest retirement; the Runner retains terminal classification. Pending debug work must not be mistaken for WFI or NoProgress, and must not cause an idle-time jump.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. Hart execution | Reuse the instruction engine for arithmetic, normal/debug CSR, memory/atomic and Zifencei classes, subject to D6. Prove saved `dpc`, interrupt masking, WFI NOP, explicit/implicit EBREAK, nonzero trap-CSR preservation on exception, earlier effects retained, and no following instruction after failure. Prove Zifencei with self-modifying guest code on the ordinary public path and in Debug Mode. |
| U2. DM protocol | Read/write both words and discover the exact capacity. Cover transfer-before-postexec, transfer=0, exactly-once execution, dynamic busy, busy-time access, sticky errors/W1C and recovery. Exercise each of the four autoexec data bits, read-before-trigger/write-before-trigger ordering, last-command handling, masked unimplemented bits, and stopping repeats when autoexec is cleared. |
| U3. Machine integration | Prove progress with idle transport, DMI service between bounded grants, no lock-held execution, and proper completion before resume. Cover reset/abort/DM-state preservation, explicit request clearing, and existing unknown-completion quarantine. Exercise RAM, native PLIC claim, AMO/LR/SC, MPRV/MPP/Sv39 and faults through the Hart path. Explicit time writes and debug-originated Platform events must not be converted into guest retirement. |
| U4. Public tools | Force OpenOCD program-buffer physical-memory access; a fallback to abstract memory does not pass. Transfer at least four adjacent distinct values, cover supported widths, and independently check addresses through abstract memory. Exercise registers, fences, exception recovery, reset halt, resume and exit. Compare named results with fixed Spike using matching buffer capacity, implicit EBREAK and datacount; handle only explicitly justified profile differences in test expectations. Direct Hart/DMI tests, not a two-word OpenOCD claim, cover virtual-memory behavior. |
| U5. Exit | CI runs the full Rust quality gate, fresh guest ELF suite, tool script, legacy regressions and unit-specific negative controls. Obtain an independent review of the final committed PR head. CI and PR history retain execution/review outcomes; the repository retains tests, replay inputs and requirements. The A10 oracle and certification deferral remain unchanged. |

Required negative controls include wrong `progbufsize`, omitted implicit EBREAK, a repeated `postexec`, disabled autoexec during a distinct-value bulk transfer, a guest trap on buffer exception, and an incorrectly accepted control transfer. Keep their small patches, commands and expected assertions as verification code or test inputs. Actual failure/pass diagnostics remain in CI, not a result document or snapshot archive.

The standard gate remains `cargo fmt --all -- --check`, `cargo check --all-features`, strict all-target Clippy, `cargo test --all-features`, `cargo doc --all-features --no-deps`, the fresh guest suite and the required live debug-attach CI. Extend the existing tool script for the forced program-buffer session and retain the abstract-memory session as a regression. Do not add an execution ledger, fixed successful-output inventory or hash gate to certify these requirements.

## 5. Exclusions and retained direction

No SBA or inbound Platform master, second hart, Sdtrig, `autoexecprogbuf`, Quick Access, F/D/C/V extension, new protection/virtualization profile, ROM/park loop, legal DRET, Linux/SBI, TLM/SystemC/DMI integration, performance certification, oracle sealing or new public application result field.

SBA remains a subsequent debug goal. It requires its own physical request/error, write/reservation visibility, lifecycle and future DMI-invalidation contract; it is not disguised Abstract Access Memory. Conditional transport integration remains a separate direction requiring a concrete consumer and fixture. Neither is silently carried into this contract.

## 6. Activation boundary

This documentation-only rotation preserves the entire Stage 6 contract, accepts the bounded ADR amendments and activates this target atomically on merge. CI/PR history records delivery and review. It does not assert runtime support, start implementation work, publish a release, or seal an oracle. Later implementation must meet the units above on its own reviewed commits.

## References

- [Debug Specification 1.0 DM](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc), [Sdext](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/Sdext.adoc), and [core registers](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/xml/core_registers.xml).
- [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [Pinned Spike](https://github.com/riscv-software-src/riscv-isa-sim/tree/5d99611782ef8dd81b65b842369bd351a9d4702a) and [OpenOCD 0.12.0](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
