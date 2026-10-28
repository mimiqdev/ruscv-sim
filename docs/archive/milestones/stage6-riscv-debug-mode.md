# Stage 6: RISC-V Debug Mode contract

**Status:** Historical on the actual successor-contract rotation merge. Until that merge, Stage 6 remains the current contract on `main`.

**Authority:** Preserved approved contract, not an active successor plan or an execution report.

**Original approval:** 2026-10-19, owner Tony, decisions D0–D15.

**Completion date:** The UTC date of the actual rotation merge, recorded in Git and PR history. Preparation does not claim completion.

**Scope and limits:** One RV64IMA/Zicsr M/S/U hart, abstract register and physical-memory commands, and JTAG `remote_bitbang`. No program buffer, SBA, triggers, second hart, FENCE.I, or DRET in this milestone. The inherited performance-certification deferral is not completed by this rotation.

**Source identifier:** `5c1d031b2a66bc17d058b5a7355d88ebe80eb652` identifies the final Stage 6 implementation changes. Check and review outcomes remain in CI/PR history, not this archive.

The entire last Stage 6 contract is preserved below, with relative links relocated. Its baseline and activation wording describe its own context; the successor is the sole current contract after rotation.

## Preserved approved Stage 6 contract

# Development Plan

**Current milestone:** Stage 6 — RISC-V Debug Mode

**Status:** Current accepted target. Decisions D0–D15 were owner approved on 2026-10-19. D0–D14 are the recommended option. D15 is `cmderr=3`. Activation merged in [PR #96](https://github.com/mimiqdev/ruscv-sim/pull/96) as `a8851c4ac57500eb54dadc89919cd7314b626594` at 2026-10-19T13:49:34Z. That merge formally completed Stage 5 and [archived its entire approved contract](stage5-platform-interrupts-wfi-and-counters.md). Stage 6 is the sole Current contract. Activation does not claim implementation completeness.

**Authority:** Normative milestone contract, constrained by accepted ADR-0001–0004. The 2026-10-19 amendment in ADR-0004 replaces the Debug Mode deferral and the External Debug Support 0.13.2 citation for this profile. ADR-0001 already keeps a Debug Mode halt distinct from a protocol halt and from a guest trap. This contract does not add a second Current contract.

**Approved:** 2026-10-19 — owner (Tony). D0–D14 are the recommended option. D15 reports a misaligned Access Memory failure as `cmderr=3`.

**Planning/evidence baseline:** Stage 5 implementation head `fb8399b20ee1d6b44bc6da157c3cc3f18b784474` (PR #94). The [Stage 5 final review](../../verification/stage5-final-review.md) reads that head. No runtime, ACT4, or performance execution is attributed to this document SHA. The Debug specification cited below is RISC-V Debug Specification 1.0, ratified, tag `5695c0a` (release "Specification Ratified", 2025-02-21). The privileged and unprivileged rules are the RISC-V ISA Manual, document version 20250508.

**Decisions:** every row in §8 is DECIDED. Decider: owner (Tony). Date: 2026-10-19.

**Relationship to the deferred certification layer:** the A10 performance framework exists and is tested. This contract does not add performance payloads, thresholds, comparison, scheduled publication, or noise calibration. It does not seal or re-seal `tools/a10/public-v1.json`.

## 1. Approved objective and architecture

The objective is to connect one single-hart RISC-V Debug Mode to the public native path so OpenOCD and GDB can halt the hart, resume it, single-step it, and read and write its registers and physical memory.

The approved profile is the Stage 5 hart (RV64IMA plus Zicsr, M/S/U, traps taken in M, `mideleg` read-only 0, CLINT, PLIC source 1, legal WFI) plus:

- Sdext, the Debug Specification 1.0 ISA extension for a hart that can enter Debug Mode;
- a Debug Module that implements abstract commands and does not execute a program buffer;
- a JTAG debug transport module, reached through OpenOCD `remote_bitbang`.

The abstract-command shape is the first implementation, and it is temporary. `progbufsize` is 0 and `datacount` is 4. A later stage, recorded in the [post-A7 roadmap](../../proposals/post-a7-roadmap.md), adds the Spike-style remainder: a program buffer, system bus access, and execution of instructions while the hart is in Debug Mode. This milestone does not claim that remainder.

Boundary. The Hart owns Debug Mode, `dcsr`, `dpc`, `dscratch0`, `dscratch1`, the halt cause, single-step, the `ebreak` redirect, and the choice between `haltreq` and a takeable interrupt at one boundary. The Debug Module owns its registers, abstract commands, and `cmderr`. The Machine admits a Debug Module transaction only at a Hart boundary, applies `ndmreset`, and decides when a run with a debugger attached may stop. The JTAG transport moves bits. It is not a second Hart and it is not the GDB Remote Serial Protocol. Component code under `src/debug/` is not this milestone's debugger. U6 removes it after U5 has a real OpenOCD connection.

This is the **first bounded milestone** of Debug Mode. One hart. No trigger module. No program buffer. No system bus access. No second hart.

## 2. Entry evidence and existing controls (read at the baseline)

These are seams, not public-path support. Component presence is not integration. Paths below were read at `fb8399b`.

- **`src/debug/` is a GDB Remote Serial Protocol component with no production target.** `src/lib.rs` re-exports it. No `DebugTarget` is connected to `RiscvCore`, the Machine, or the public CLI. The server does not patch guest memory. There is no `dcsr`, `dpc`, `dscratch`, Debug Module register, or `dret` in `src/csr/`.
- **A Hart turn samples, then may wait, then may take an interrupt, then fetches.** `RiscvCore::execute_turn` in `src/core/mod.rs` handles an unresolved physical access, then `wfi_wait`, then `accepted_interrupt`, then `execute_instruction`. The interrupt order is MEI, MSI, MTI, SEI, SSI, STI. A take in M requires MIE. A take below M does not.
- **A legal WFI retires, then waits.** The PC is already the WFI address plus 4 when the wait begins. U-mode WFI, and S-mode WFI with TW set, trap and do not wait. Privileged §3.3.3 allows a WFI to complete for an implementation-defined reason.
- **`dret` is already illegal.** The SYSTEM whitelist in `src/core/mod.rs` accepts ECALL, EBREAK, SRET, MRET, WFI, and SFENCE.VMA. `0x7b200073` is not on that list. Debug Specification 1.0 moved `dret` out of the normative text. This profile keeps it illegal, because the hart does not execute instructions in Debug Mode.
- **An unimplemented CSR is already illegal.** `CsrError::InvalidAddress` becomes an illegal-instruction trap. `tselect` and the other Sdtrig addresses are in that set today. Sdext requires that result when Sdtrig is absent.
- **`Machine::fresh_reset` reloads the installed image.** `src/machine/mod.rs` restores the image and a fresh Hart and devices. The Debug Module itself is not part of that object yet.
- **A control edit drops an LR reservation.** `src/machine/mod.rs` clears `reservation` on that path. Sdext allows the reservation to be lost on entry to Debug Mode.
- **The public `run` command has no debug flag.** `src/main.rs` accepts the ELF path, `--max-cycles`, `--tohost`, `--verbose`, and `--log-commits`.
- **`misa` is `0x8000_0000_0014_1101`.** F and D are clear, so an abstract access to an FPR is unsupported.
- **ADR-0001 already separates three stops.** An external protocol halt is not a Hart trap. An `EBREAK` that traps is `TrapEntered`. A future Debug Mode halt is Hart-owned. ADR-0004 ranked that halt and, until the amendment in this rotation, deferred the feature and cited External Debug Support 0.13.2 §4.

## 3. Approved scope and exclusions

In scope under the approved options in §8:

- One hart. Debug Specification 1.0. `dmstatus.version` is 3. `dtmcs.version` is 1. `dtmcs.abits` is 7.
- Sdext CSRs `dcsr` (`0x7b0`), `dpc` (`0x7b1`), `dscratch0` (`0x7b2`), and `dscratch1` (`0x7b3`). Field values are D5. `dpc` uses the same IALIGN=32 WARL rule as `mepc` (privileged §3.1.14). A guest CSR instruction that names any of these addresses is illegal, because the hart never executes that instruction in Debug Mode. Abstract Access Register is the only writer and reader.
- Halt causes `haltreq`, `ebreak`, and `step`. `resethaltreq`, trigger, and a halt group are not causes this profile produces.
- A JTAG TAP with a 5-bit instruction register. Reset selects IDCODE. BYPASS is selected for IR `0x00` and `0x1f`. IDCODE is IR `0x01`. `dtmcs` is IR `0x10`. `dmi` is IR `0x11`. Any other IR selects BYPASS. The IDCODE value is `0xdeadbeef` (bit 0 is 1). It is a placeholder shared with Spike's JTAG DTM, not a JEDEC manufacturer code.
- `remote_bitbang` on TCP, the OpenOCD adapter. The character protocol is the one in OpenOCD `remote_bitbang.c`.
- Debug Module registers required by D2. `dmcontrol` implements `dmactive`, `ndmreset`, `haltreq`, `resumereq`, `ackhavereset`, and hart select for hart 0. `hasel` is 0. `hartreset` is fixed 0. `setresethaltreq` is not implemented. `dmstatus` reports version 3, `authenticated` 1, `hasresethaltreq` 0, `impebreak` 0, and the all/any halted, running, unavailable, nonexistent, resumeack, and havereset bits. `hartinfo.nscratch` is 2 and `dataaccess` is 0. `abstractcs.datacount` is 4, `progbufsize` is 0, and `busy` is 0 because a command finishes before the DMI access returns. `cmderr` is write-1-to-clear. `haltsum0` is implemented. `sbcs` reads 0, which means there is no system bus access.
- Abstract Access Register for GPR `regno` `0x1000` through `0x101f` and for every implemented CSR, including the Sdext CSRs. The PC is `dpc`. `aarsize` 2 and 3 are legal. `aarsize` 4 returns `cmderr=2`. `postexec` returns `cmderr=2`. An FPR `regno` returns `cmderr=3` because the register does not exist in the hart (Debug Spec §3.7.1.1). A write of an ordinary CSR has the same side effects as a guest write of that register. A write of `dcsr`, `dpc`, or either scratch register follows D5 and does update the register. A guest CSR instruction cannot perform that write.
- Abstract Access Memory of a physical address, and only while the hart is halted. A command while running returns `cmderr=4`. `aamvirtual=1` returns `cmderr=2`. The access uses the hart's own M-mode load and store path, so a PLIC claim has the same side effect as a guest claim (Debug Specification 1.0 §3.7.1.3). A misaligned access that would raise an exception for an M-mode load or store returns `cmderr=3` and does not complete the access (D15).
- `ndmreset` calls `Machine::fresh_reset`. The Debug Module is not reset. If `haltreq` is set when the reset is released, the hart halts before the first instruction. `dpc` is the reset entry and `minstret` is 0.
- Public CLI flags `--rbb-port` and `--halted` on `run` (D12).

Non-goals: a second hart; Sdtrig and any trigger CSR made legal; a program buffer, `impebreak`, abstract auto, or Quick Access in this milestone; system bus access; halt and resume groups; authentication (`authenticated` stays 1); `resethaltreq` and `hartreset`; a custom DMI socket; hypervisor debug fields (`v`, `ebreakvs`, and `ebreakvu` stay 0); `relaxedpriv`, `stickyunavail`, and `keepalive`; a device-tree debug description; an SBI; Linux boot; a new public field on `ExecutionResult`; sealing or re-sealing the A10 oracle. The program buffer and system bus access are non-goals of this milestone only. They are the later-stage item in §9.

## 4. Sequenced units and the proofs

The units are one milestone in order. U1, then U2, then U3, then U4, then U5, then U6. A later unit's test is not evidence for an earlier unit. U1 through U5 each name one negative mutation in §7. U6 is a deletion check, not a behavior mutation. The stage exit is the whole set. A component test of `src/debug/` is not that exit.

| Unit | What becomes true | Proof |
| --- | --- | --- |
| U1. Hart Debug Mode | `dcsr`, `dpc`, and both scratch registers exist. The Hart can enter Debug Mode for `haltreq`, `ebreak`, and `step`, and can resume. `StepOutcome` gains `DebugHalted`. | Rust tests and `s6_*` guests for the U1 rows in §7 item 1. |
| U2. Debug Module | The register set and abstract commands in §3 answer at a DMI-level API, with no JTAG. | The version-discovery sequence in Debug Specification 1.0 §3.13, plus the `cmderr` and memory rows in §7 item 2. |
| U3. Machine integration | The Machine owns the Debug Module. A transaction is admitted only at a Hart boundary. `ndmreset` reloads the image. A debugger changes when a run may stop. | The boundary, reset, and frozen-counter rows in §7 item 3. |
| U4. JTAG and `remote_bitbang` | A bitbang client can shift IR and DR, and the CLI grows `--rbb-port` and `--halted`. | The IDCODE, BYPASS, `dtmcs`, and `dmstatus.version` rows in §7 item 4. |
| U5. OpenOCD and GDB | A scripted OpenOCD 0.12.0 session and GDB perform the §7 item 5 operations. The same script against Spike is a comparison, not a hash. | CI job `debug-attach` and its execution logs/artifacts. |
| U6. Remove the old debugger | `src/debug/` and its crate re-exports are gone. | The quality gate, and a search that finds no remaining use. |

U2 depends on U1's halt and resume. U3 depends on U2's Debug Module. U4 depends on U2's DMI registers and adds the transport U5 uses. U5 depends on U3 and U4. U6 depends on U5's connection evidence and must not land before that evidence exists.

## 5. Routes

`--rbb-port` and `--halted` are long options of `run`. `--rbb-port` takes a TCP port. `--halted` is valid only together with `--rbb-port`. Without `--rbb-port`, the run loop is the Stage 5 loop, including `NoProgress` and `max-cycles`.

With `--rbb-port`, the process serves `remote_bitbang` until the guest exits or the transport closes. A WFI with no future platform event does not report `NoProgress`. The loop waits for a DMI command instead. `max-cycles` counts turns that are not spent halted. A halted hart does not consume the turn budget. While the hart is running, the loop polls the socket at every Hart boundary, before the next fetch. That is K=1. A `haltreq` observed there is D6.

`--halted` stops the hart before the first instruction after reset. `dpc` is the entry address and `minstret` is 0.

Native and flat both accept the flags. Flat has no CLINT and no PLIC. Its proof is a RAM access. A flat run is not evidence for a PLIC claim side effect.

`StepOutcome` stays exhaustive. A missed `DebugHalted` arm fails the in-tree build. `ExecutionResult` gains no field.

No Stage 6 guest is added to `tools/a10`. The 2026-10-13 standing rule still holds. There is no hash lock.

## 6. Measurement and evidence policy

Correctness first. No ratios, no thresholds, and no new performance payload. The A10 certification deferral stays deferred (D0).

Do not seal or re-seal `tools/a10/public-v1.json`. `suite-v1.json` gains no `oracle_sha256` pin. The U5 Spike comparison checks named outputs, such as a register value and a breakpoint PC. It is not an oracle digest.

## 7. Verification and acceptance

These checks apply to the implementation that follows this rotation. They are not acceptance criteria of this document.

1. U1 shows each halt cause's `dcsr.cause`, `dcsr.prv`, and `dpc`. With the `ebreak` bit clear, EBREAK still traps with cause 3. With the bit set, `mepc` and `mcause` stay unchanged and `mcycle` and `minstret` do not increase. A halt during a WFI wait sets `dpc` to the WFI address plus 4 and clears the wait. A single-step of WFI does not wait. A single-step with a takeable timer pending executes one instruction, increases `minstret` by one, and does not take the interrupt. A single-step into `ecall` stops at the handler. `haltreq` beside a takeable interrupt stops at the current PC, and a later resume takes the interrupt with `mepc` at that PC. Resume to U clears MPRV. A guest CSR access to `0x7b0` through `0x7b3`, and to `0x7a0`, is illegal. An SC after a halt fails. `mtime` does not move while the hart is halted. A negative mutation that stores `dpc` as the PC plus 4 on `haltreq` must fail.
2. U2 runs the §3.13 version discovery. Unsupported options return `cmderr=2`, including `aarsize=4`, `postexec`, and `aamvirtual=1`. An FPR `regno` returns `cmderr=3` because the register does not exist. A command while running returns `cmderr=4`. Writing 1 to `cmderr` clears it. GPR and CSR round-trips honor WARL, including `misa`. An Access Memory read of the PLIC claim register consumes the claim. A misaligned access that M-mode would reject returns `cmderr=3`. Selecting hart 1 sets `anynonexistent`. `havereset` sticks until `ackhavereset`. A negative mutation that reports `datacount` 2 must fail a 64-bit Access Memory address.
3. U3 halts a spinning hart from another thread at a consistent boundary. After `ndmreset` with `haltreq`, `dpc` is the reset entry and `minstret` is 0. While halted, `mtime`, `mcycle`, `minstret`, and `ExecutionResult.cycles` do not move. With no debugger attached, `scripts/run_elf_tests.sh` keeps its current results. Flat completes one RAM-only halt and memory round-trip. A negative mutation that accepts `haltreq` in the middle of an instruction must fail.
4. U4's in-process bitbang client reads IDCODE after Test-Logic-Reset, selects BYPASS for an unimplemented IR, and reads `dtmcs.abits` and `dtmcs.version`. A DMI read returns `dmstatus.version` 3. `dmireset` clears a sticky DMI error. `--halted` without `--rbb-port` fails before the guest runs. A negative mutation that leaves the instruction register at a value other than IDCODE after reset must fail.
5. U5's GDB script connects, reads PC, a GPR, `mstatus`, and `dcsr`, single-steps, hits a software breakpoint, writes a register and single-steps to observe it, writes RAM and reads it back, `monitor resume` then `monitor halt`, and `monitor reset halt` with PC equal to the entry. Continue to guest exit ends the process, and OpenOCD sees the target go away. The same script against Spike, with `--dm-progsize=0`, `--dm-no-impebreak`, `--dm-datacount=4`, and `--rbb-port`, matches those named outputs. Changing `dmstatus.version` to 4 must make OpenOCD reject the module. Changing `datacount` to 2 must make abstract memory fail.
6. U6 removes `src/debug/` and the re-exports. The quality gate passes. A search finds no remaining reference that the build would need. There is no behavior mutation for a deletion. The failed check is a remaining reference or a failed build.
7. `tools/a10/public-v1.json` is unchanged. No `oracle_sha256` pin is added.
8. The full local quality gate is green, and CI Quality-and-tests is green at the implementation head. From U5 onward, CI also runs `debug-attach`.
9. An independent final PR-head review has a recorded outcome. Findings are addressed on the same branch and re-verified.

## 8. Decided options

Every row is DECIDED. Decider: owner (Tony). Date: 2026-10-19. D0–D14 are the recommended option. D15 is the owner's choice of `cmderr=3`.

The 2026-10-19 spec review is part of the rationale below. It used Debug Specification 1.0 at tag `5695c0a` and the privileged specification 20250508. One correction is normative for citations in this contract. `mcycle` and `minstret` are privileged §3.1.10. An earlier draft of this plan cited §3.1.11. That citation was wrong.

**D0. Stage 3 exit and Stage 6 entry.** DECIDED, 2026-10-19, owner approved, recommended option B.

Stage 5 D0 already inherited the Stage 3 certification waiver. This milestone does not pick up comparison, publication, or calibration.

- Option A, not recommended. Block Debug Mode until the certification layer has an owner.
- Option B, recommended. Inherit the waiver. Debug Mode does not depend on that layer.

Rationale: the same waiver Stage 5 recorded. A debugger does not make a hash into the specification.

**D1. Debug specification version.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. Debug Specification 1.0, tag `5695c0a`. `dmstatus.version` is 3.
- Option B, not recommended. External Debug Support 0.13.2. `dmstatus.version` is 2. That matches today's Spike and the previous ADR-0004 sentence. It is not the ratified text.

Rationale: 1.0 is ratified. OpenOCD 0.12.0 and current OpenOCD accept version 2 and version 3 in `riscv-013.c`. The extra obligations used here are the MPRV clear on resume and the illegal-instruction result for an unimplemented core debug CSR. The cost is that Spike still reports version 2, so a Spike comparison is not a version-number comparison. ADR-0004's 0.13.2 sentence is amended in this rotation.

**D2. Implementation shape.** DECIDED, 2026-10-19, owner approved, recommended option A. This option is a temporary first implementation.

- Option A, recommended for this milestone only. Abstract commands. `progbufsize` is 0. `datacount` is 4. Abstract access covers every software-visible register this hart implements, and abstract Access Memory covers physical memory. The hart does not execute instructions in Debug Mode.
- Option B, not recommended for this milestone. An execution-based module, with a program buffer and the hart running instructions in Debug Mode. That is the Spike default. It needs a debug ROM or a RAM window, and it pulls in the Sdext rules for exceptions inside a program buffer, WFI in Debug Mode, and an undefined `mret`.

Rationale: option A meets the Debug Module requirement to provide at least one of a program buffer, system bus access, or abstract Access Memory, and it meets the further requirement to abstract-access every software-visible register when the program buffer is absent. The 2026-10-19 spec review found that shape conforming. `datacount` is 4 because a 64-bit Access Memory address is `data2` and `data3`. A guest CSR instruction still cannot read `dcsr` or `dpc`. Those addresses are illegal outside Debug Mode, and this hart never executes a CSR instruction in Debug Mode. The cost is an OpenOCD warning that it cannot execute a fence, and no program buffer for a later extension. §9 records the later stage that replaces this shape with the Spike-style module.

**D3. Memory access.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. Abstract Access Memory, physical addresses, halted hart only.
- Option B, not recommended here. System bus access.
- Option C, not recommended here. Both.

Rationale: the hart's own path stays the only master. The roadmap's later multi-master work is not pulled forward. Spike leaves system bus access off by default. System bus access is part of the later-stage item, not of this milestone.

**D4. Transport.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. JTAG DTM plus `remote_bitbang` TCP. IDCODE is `0xdeadbeef`.
- Option B, not recommended. `jtag_vpi` or `jtag_dpi`.
- Option C, not recommended. A custom DMI socket. OpenOCD does not speak it.

Rationale: option A is the transport Spike and Debian's OpenOCD 0.12.0 already share. The IDCODE is the Spike placeholder. Bit 0 is 1, as JTAG requires. This milestone does not request a JEDEC code.

**D5. `dcsr` WARL.** DECIDED, 2026-10-19, owner approved, recommended option.

`debugver` reads 4. `ebreakm`, `ebreaks`, and `ebreaku` are writable, because M, S, and U exist. `ebreakvs`, `ebreakvu`, `v`, and `nmip` are fixed 0. `stepie` is fixed 0. `stopcount` and `stoptime` are fixed 1. `mprven` is fixed 1. `prv` stores 0, 1, or 3. A write of 2 stores a legal value instead. `step` is writable. `cause` is read-only. Both scratch registers exist.

The alternative, a writable `stepie`, needs a defined rule for taking an interrupt during a step. This profile does not take that alternative.

Rationale: `stepie` 0 is the value the specification says should be supported. With one hart, `stopcount` 1 stops `mcycle` (privileged §3.1.10). `stoptime` 1 matches a clock that advances only on a completed Hart turn. There is no turn while halted, so `mtime` does not move, and the profile does not grow a host-time `mtime`. On resume to a privilege below M, MPRV is cleared. The 2026-10-19 spec review confirmed `stopcount` 1 and the WFI step and halt rules. It also inferred, at medium confidence, that an Access Register write of `mip`, `mie`, or `mstatus` is sampled at the first instruction boundary after resume, using `dcsr.prv`, and that `step` with `stepie` 0 does not take that interrupt. This profile implements that inference.

**D6. `haltreq` beside a takeable interrupt.** DECIDED, 2026-10-19, owner approved, recommended option A.

The specification does not order a `haltreq` and a takeable interrupt that are both visible at the same boundary. The 2026-10-19 spec review said both orders conform. Spike takes the halt first.

- Option A, recommended. `haltreq` wins. `dpc` is the current PC. The interrupt stays pending. After resume, privileged §3.1.9 takes it before the instruction at `dpc`.
- Option B, not recommended. Take the interrupt, then halt at the first instruction of the handler.

Rationale: option A does not retire the interrupted instruction and does not write `mepc` until a later resume. It matches Spike.

**D7. LR reservation on Debug Mode entry.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. Clear the reservation on every entry.
- Option B, not recommended. Keep it.

Rationale: Sdext allows the reservation to be lost. The Machine already drops it on a control edit.

**D8. EBREAK redirect.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. EBREAK enters Debug Mode only when the `ebreak` bit for the current privilege is 1. The reset value of each bit is 0, so an existing EBREAK test is unchanged until a debugger sets the bit.
- Option B, not recommended. Always enter Debug Mode on EBREAK.

Rationale: reset behavior stays the Stage 5 trap. OpenOCD sets the bits by default, so a session attached through OpenOCD will catch a guest EBREAK. The U5 configuration may turn the bits off with OpenOCD's `riscv set_ebreakm`, `set_ebreaks`, and `set_ebreaku` when a test needs the guest handler. The contract does not change the reset value to hide that tool default.

**D9. Counters on the entry turn.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. A `haltreq` entry and an `ebreak` entry are not completed Hart turns. `mtime`, `mcycle`, `minstret`, and `ExecutionResult.cycles` do not move.
- Option B, not recommended. Count a turn, add one to `mtime`, and leave `mcycle` unchanged.

Rationale: option A matches `stopcount` 1 and `stoptime` 1. Privileged §3.1.10 is the counter rule. The Stage 5 rule that a trap entry increments `mcycle` does not apply to a `haltreq` or `ebreak` entry, because that entry is not a trap. A single-step retires the stepped instruction, and that retirement counts. The Debug Mode entry after it does not add a second tick. ADR-0004's accounting of a completed Hart turn gains this exception in the same amendment.

**D10. Where Debug Mode lives.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. A Hart-owned state, reported as `StepOutcome::DebugHalted`.
- Option B, not recommended. A Machine stop flag that the Hart does not see. That is an external protocol halt, which is the rejected GDB stub approach.

Rationale: ADR-0001 and ADR-0004 assign the architectural transition to the Hart. `dcsr` and `dpc` are Hart CSRs. Adding a `StepOutcome` variant is a source break. Every match on `StepOutcome` must gain the arm in the same implementation that introduces it.

**D11. `ndmreset`.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. Call `Machine::fresh_reset`, which reloads the installed image.
- Option B, not recommended. Reset the Hart and the devices and leave RAM as the guest left it.

Rationale: the specification requires the hart and the platform to reset, and it requires the Debug Module not to reset. It does not specify RAM contents. Option A reuses the tested reload. `hartreset` stays 0. `resethaltreq` stays unimplemented. `monitor reset halt` is `haltreq` together with `ndmreset`.

**D12. Run control while a debugger is attached.** DECIDED, 2026-10-19, owner approved, recommended option, including the new CLI flags.

While `--rbb-port` is set, a halted hart blocks for the next DMI command. A running hart polls that socket at every Hart boundary. A WFI with no future platform event waits for DMI instead of reporting `NoProgress`. `max-cycles` ignores halted time. On guest exit the process exits, and OpenOCD sees the connection drop. The alternative, staying halted after guest exit, is not taken.

The flags are `--rbb-port` and `--halted`. The owner approved both on 2026-10-19.

Rationale: the Stage 5 stop reasons are for a run with no debugger. Leaving them in force would end a debug session at the first WFI. Polling every boundary makes the stop PC independent of a skipped-turn interval.

**D13. The existing `src/debug/` crate.** DECIDED, 2026-10-19, owner approved, recommended option A.

- Option A, recommended. Delete it in U6, after U5's OpenOCD evidence exists.
- Option B, not recommended. Keep it and mark it deprecated.

Rationale: OpenOCD is the GDB server. Two stop paths would contradict ADR-0003's split between a protocol halt, a guest breakpoint trap, and a Debug Mode halt. The deletion is a public API break of the re-exports in `src/lib.rs`. U6 is the unit that breaks it, not this document.

**D14. CI and the development image.** DECIDED, 2026-10-19, owner approved, recommended option.

U5 adds a CI job that installs `openocd` and `gdb-multiarch` and runs the attach script. The development image gains the same two packages and moves its Spike pin to a revision that implements `--dm-datacount`, `--halted`, and Access Memory. The Dockerfile change lands with U5, not in this rotation.

The alternative is to skip the Spike comparison and keep the current Spike pin. That alternative is not taken. The comparison is still not an oracle hash.

**D15. Misaligned Access Memory.** DECIDED, 2026-10-19, owner approved, `cmderr=3`.

A misaligned abstract memory access fails only when an M-mode load or store of that address and size would raise an exception. The failure is reported as `cmderr=3` (exception). The access does not retire.

The other candidate was `cmderr=5` (bus). The specification's bus code names alignment, and Spike reports the exception code. The owner chose Spike's code.

Rationale: the 2026-10-19 spec review required the failure condition to match M-mode and left the `cmderr` code to the owner. D15 is that choice.

## 9. Risks and rotation boundary

The risks that follow from the decisions:

- Abstract commands are a temporary subset. A reader who treats `progbufsize=0` as the project's debugger has the wrong end state. The later stage adds a program buffer, system bus access, and instruction execution in Debug Mode, in the Spike style. That stage is not this milestone and it is not a reason to widen U1 through U6.
- `StepOutcome::DebugHalted` breaks every match. Landing the variant without the new arms will not build.
- OpenOCD 0.12.0 accepts `dmstatus.version` 3 in source. The drafting probe that checked abstract memory ran Spike, which reports version 2. This repository does not contain that probe. An end-to-end failure against version 3 is a U5 defect, and the fallback is not silent. It would be a new decision to leave 1.0.
- OpenOCD will warn that it cannot execute a fence. This hart has no instruction cache, and FENCE.I already traps. The warning is expected. It is not a failed attach.
- An OpenOCD session sets the `ebreak` bits. Guest EBREAK tests change meaning inside that session. They do not change at reset.
- There is no trigger module. A GDB watchpoint is a software watchpoint.
- Access Memory must use the hart path and the Machine admission boundary. A read through the host's volatile memory view would skip a PLIC claim and would violate ADR-0003.
- `ndmreset` reloads the image, so a guest that stored secrets in RAM loses them. That is option A of D11, not an accident.
- Inheriting the Stage 3 waiver leaves comparison, publication, and calibration with no owner. This contract does not pick them up.

Rotation boundary. Stage 6 became the sole Current contract on the activation merge `a8851c4ac57500eb54dadc89919cd7314b626594` at 2026-10-19T13:49:34Z (PR #96). That documentation-only rotation implemented none of §7. CI runs the implementation checks; PR history records reviews and delivery. The [program-buffer successor proposal](../../proposals/post-stage6-program-buffer-debug.md) is Draft and informational. It neither completes Stage 6 nor activates a successor. Formal completion and a later rotation require their own owner decision after acceptance evidence and successor approval.
