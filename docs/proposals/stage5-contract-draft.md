# Stage 5 platform interrupts, WFI, modeled time, and counters (proposed milestone contract)

**Status:** Decisions approved. **Not active.** This document records the owner's decisions of 2026-10-16 13:08 CST. Each row in §8 is owner approved, recommended option. It does not archive the Stage 4 contract and does not replace [`docs/dev-plan.md`](../dev-plan.md).

**Authority:** Informational until a later approval promotes it.

**Why this is a draft, not a rotation:** The Stage 4 contract was written first as a proposal with open decisions, and it became [`docs/dev-plan.md`](../dev-plan.md) only after a separate owner approval and a documentation rotation. The [archive index](../archive/milestones/README.md) says the next milestone is promoted into `docs/dev-plan.md` only after it is separately approved. The decisions in §8 are decided. This text is not the sole Current contract until a separate rotation. Stage 4 implementation and review are recorded at PR #88, `ee134b441f6e0504cec60cf2e652a6ff7782b396`, and in the [final review](../verification/stage4-final-review.md). The [Stage 4 closeout](../verification/stage4-closeout.md) leaves archival of that contract to a later, separately authorized rotation. That rotation is not this document.

**Authority if later approved:** would become the sole Current normative milestone contract, constrained by accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md). No new ADR is proposed. If review finds an ADR gap, that gap is surfaced before implementation.

**Normative ISA text:** RISC-V ISA Manual, document version 20250508, ratified. Privileged citations below use that edition's section numbers: §3.1.9 Machine Interrupt Registers (`mip` and `mie`), §3.1.10 Hardware Performance Monitor, §3.2.1 Machine Timer Registers (`mtime` and `mtimecmp`), and §3.3.3 Wait for Interrupt. The same edition's unprivileged §2.7 is the FENCE rule. The Platform-Level Interrupt Controller Specification, version 1.0.0, is the PLIC rule. The Advanced Core Local Interruptor specification, revision 1.0-rc4 (10 January 2022), is cited for MSWI, MTIMER, and the SiFive CLINT compatibility table. That document is still marked Draft, so it is not a ratified requirement of the privileged architecture. Where 20250508 leaves a platform choice unspecified, the decision names Spike and Sail as read from source. Spike citations are `riscv-isa-sim` commit `17e8a69f`. Sail citations are commit `6136f843`. Those trees were read and were not re-executed. D1 marks one Spike idle-time claim unverified.

**Planning baseline:** `ee134b441f6e0504cec60cf2e652a6ff7782b396` (PR #88), which is `main` when this draft was written. Follows [post-A7 roadmap Stage 5](post-a7-roadmap.md). The Stage 4 contract's D0 waiver, and its §9 statement that later stages inherit that waiver, are the input to D0 here. They are not re-decided by this draft.

**Relationship to the deferred certification layer:** the A10 performance framework exists and is tested. This draft does not add performance payloads, thresholds, comparison, scheduled publication, or noise calibration.

## 1. Proposed objective and architecture

The objective, if later approved, is to connect one single-hart platform interrupt and time profile to the public native path so a guest program can prove timer, software, and external interrupts, a WFI that waits and wakes, and counters that are not one number.

The recommended profile, pending §8, is the Stage 4 hart (RV64IMA plus Zicsr, M/S/U, traps taken in M, `mideleg` read-only 0) plus:

- a platform `mtime` and one `mtimecmp`, on the SiFive CLINT map Spike and QEMU virt use;
- a PLIC 1.0 with one hart, a machine context, and a supervisor context;
- machine software, timer, and external interrupt lines sampled before each fetch;
- a legal WFI that retires once and then waits until a locally enabled interrupt is pending;
- separate accounting for retired instructions, `mcycle` and `minstret`, framework ticks, and `mtime`.

Boundary, from roadmap Stage 5, as narrowed by §8. The Hart owns eligibility, the architectural priority among presented causes, trap entry, and WFI legality and wake. The Platform owns source assertion, controller pending, claim and complete, and `mtime` and `mtimecmp`. The Machine owns input admission, the pre-fetch grant, framework time, and a legal idle jump. Those four interrupt stages stay distinct: a source can be asserted, pending in the controller, presented to the hart, and accepted as a trap, and none of those steps is a PLIC claim or complete. ADR-0004 §6.1 already requires that split. Component CLINT and PLIC tests do not prove it.

This is the proposed **first bounded milestone** of platform interrupt and time work. One hart. No Linux or SBI boot, no SSWI, no Sstc, no writable delegation, no translation cache, and no debug-mode interaction. The timer and software-interrupt window is the one-hart SiFive CLINT layout. ACLINT 1.0-rc4 describes that layout as one MSWI device beside one MTIMER device. This milestone does not claim any other ACLINT base arrangement.

## 2. Entry evidence and existing controls (read at the baseline)

These are seams, not public-path support. Component presence is not integration. Paths below were read at `ee134b4`.

- **The public hart samples only the three supervisor causes, and it samples them before fetch.** `execute_turn` calls `accepted_supervisor_interrupt` before `execute_instruction` ([`src/core/mod.rs`](../../src/core/mod.rs)). The mask is `SUPERVISOR_INTERRUPTS` (`0x222`). A pending enabled cause is taken in U and in S with MIE clear, and in M only when MIE is 1. The order is SEI, then SSI, then STI. MEIP, MTIP, and MSIP read 0, and MEIE, MTIE, and MSIE are read-only 0, because Stage 4 D11 has no CLINT and no PLIC. `mideleg` is read-only 0, so `sip` and `sie` read 0. An accepted interrupt enters M, sets `mcause` to the interrupt bit plus the cause, sets `mepc` to the interrupted PC, sets `mtval` to 0, and does not retire an instruction. `enter_trap` increments `mcycle` and does not increment `minstret`.
- **Legal WFI retires as a NOP.** In M, and in S with TW = 0, WFI returns success and changes no architectural state beyond the normal retirement ([`src/isa/rv64i/system.rs`](../../src/isa/rv64i/system.rs)). U-mode WFI, and S-mode WFI with TW = 1, raise illegal-instruction immediately. That is the Stage 4 D10 choice. The [Stage 4 contract](../dev-plan.md) defers a WFI that stalls the hart to Stage 5. The `s4_tvm_tsr_tw` guest expects the legal cases to fall through.
- **`mcycle` and `minstret` count completed hart transitions, not platform time.** A retired instruction increments both, unless the instruction itself wrote that counter. A trap entry increments `mcycle` only. `time` (`0xC01`) is absent. The CSR comment says the profile has no `mtime` ([`src/csr/mod.rs`](../../src/csr/mod.rs)). `mcountinhibit` is not implemented, so both counters behave as though it were zero. `mcounteren` is read-only 0, so an S-mode or U-mode read of `cycle` or `instret` is illegal-instruction. Zicntr is not claimed, because Zicntr is `cycle`, `time`, and `instret` together.
- **`ExecutionResult.cycles` counts completed Hart turns.** The field comment says retirements and synchronous trap entries ([`src/executor.rs`](../../src/executor.rs)). The runner increments that count for every successful step, and a `TrapEntered` step succeeds, so a Stage 4 interrupt entry already counts. A started turn that ends in `SimulatorFailure` does not. Idle time cannot appear, because the public loop does not idle.
- **CLINT and PLIC exist as TLM components and are not on the public bus.** [`src/peripherals/clint.rs`](../../src/peripherals/clint.rs) uses the SiFive map: `msip` at offset 0, `mtimecmp` at `0x4000`, `mtime` at `0xBFF8`, window `0xC000`. The constructor example base is `0x0200_0000`. The default clock argument is 10 MHz, stored next to an `ScTime`. [`src/peripherals/plic.rs`](../../src/peripherals/plic.rs) uses the PLIC 1.0 map: priorities at 0, pending at `0x1000`, enables at `0x2000` with stride `0x80`, threshold at `0x200000`, claim/complete at `0x200004`, context stride `0x1000`, window `0x400000`. The example base is `0x0C00_0000`. `with_default_config` builds 32 sources and 2 contexts. Priority 0 is unused; the stored maximum is 7. Source 0 cannot be enabled. `claim_interrupt` returns the lowest ID among equal priorities, because a later source replaces the choice only when its priority is strictly greater. A claim clears the pending bit. A complete clears the claimed ID and does not by itself restore a level that is still asserted. A 4-byte read of the claim register returns 0 and does not claim. The native `Platform` builds RAM, UART at `0x1000_0000`, and HTIF at `0x4000_8000` ([`src/machine/platform.rs`](../../src/machine/platform.rs)). It does not construct a CLINT or a PLIC. The flat facade has RAM only.
- **The UART component can report a local interrupt and is not a public PLIC source.** `Uart16550` has an interrupt callback. The public platform uses that object for transmit bytes. It does not connect the callback to a hart line.
- **FENCE with `fm` other than 0 retires as a fence.** Base FENCE, FENCE.TSO (`0x8330_000f`), and other reserved `fm` values execute as an ordering no-op ([`src/isa/rv64i/fence.rs`](../../src/isa/rv64i/fence.rs)). FENCE.I traps, because Zifencei is not claimed. The [Stage 4 review](../verification/stage4-final-review.md) left non-zero `fm` open and called the encoding unspecified. Unprivileged 20250508 §2.7 is stricter than that note. See D6.
- **There is no translation cache on the public path.** Stage 4 M4 deferred TLB integration. Every translated access walks. SFENCE.VMA is an ordering no-op on that path. The component TLB remains a component.

## 3. Proposed scope and exclusions

In scope if the recommended options in §8 are accepted:

- One hart. Native bus only for the new devices (D9).
- CLINT window at `0x0200_0000`, size `0xC000`. Hart 0 `msip` is the word at offset 0; bit 0 is the software-interrupt level; other bits read 0 and ignore writes. Hart 0 `mtimecmp` is the aligned doubleword at offset `0x4000`. `mtime` is the aligned doubleword at offset `0xBFF8`. Both are read-write. Reset `mtime` is 0. Reset `mtimecmp` is `2^64-1`, so a timer interrupt is not pending at reset on this profile. ACLINT 1.0-rc4 resets MTIME to 0 (line 198), leaves MTIMECMP unknown (line 213), and resets MSIP to 0 (line 344). All-ones is a legal value for that unknown reset. Spike `17e8a69f` leaves `mtimecmp` as an uninitialized `std::map` default, so a read returns 0 and MTIP is pending at reset; that pending state is inferred from the zero compare against `mtime` 0. Sail `6136f843` clears `msip` only (`platform.sail` lines 72–79). Exit guests write `mtimecmp` before they set MTIE and do not depend on the reset value. This is the SiFive CLINT map used by Spike and QEMU virt. ACLINT 1.0-rc4's compatibility table describes the same window as one MSWI device at offsets `0x0` through `0x3fff` and one MTIMER device at offsets `0x4000` through `0xbfff` (D2). No SSWI device is added.
- PLIC window at `0x0C00_0000`, size `0x400000`, PLIC specification 1.0.0. One hart, two contexts: context 0 is machine, context 1 is supervisor. Priorities are 0 through 7. Priority 0 never interrupts. Source 0 does not exist. At least source 1 is a level gateway. Equal priorities take the lowest ID. A 4-byte read of a context's claim register claims. A 4-byte write of that register completes. Pending bits are read-only. The claim address's current read-returns-0 behavior is a defect to fix, not behavior to keep.
- One guest-writable level bit at `0x0C40_0000`, bit 0, wired only to PLIC source 1. That address is outside the PLIC window (`0x0C00_0000` through `0x0C3F_FFFF`) and below UART at `0x1000_0000`. It is a platform gateway, not a PLIC register and not a UART interrupt. Writing 1 asserts the source. Writing 0 deasserts it. While the level is high and the source is not in service, the PLIC pending bit is set. Claim clears the pending bit and marks the source in service. Complete ends that service. If the level is still high after complete, the pending bit is set again. Trap acceptance does not claim, and claim does not enter a trap.
- Machine lines follow privileged §3.1.9. MSIP is read-only in `mip` and follows the `msip` bit. MTIP is read-only in `mip` and is set while `mtime >= mtimecmp`, compared as unsigned values. MEIP is read-only in `mip` and follows the machine PLIC context's presented line. MSIE, MTIE, and MEIE become writable because those interrupts can become pending. They reset to 0. The single-hart sentence that allows MSIP and MSIE to be read-only 0 is not taken (D2).
- Supervisor lines stay the Stage 4 rule, plus the external OR. SSIP and STIP remain software-writable in `mip`, because Sstc and `stimecmp` are absent and no ACLINT SSWI is added. SEIP is the logical OR of the software-writable bit and the supervisor PLIC context's line. A CSR read returns that OR. CSRRS and CSRRC update only the software-writable bit. `mideleg` stays 0, so `sip` and `sie` stay read-only 0, and every taken interrupt is taken in M.
- The hart samples before fetch, and again after MRET or SRET and after a write to `mip`, `mie`, `mstatus`, or `mideleg`, as Stage 4 already requires for the supervisor causes. The same slot now includes the machine causes. The order is MEI, MSI, MTI, SEI, SSI, STI (privileged §3.1.9). An interrupt admitted during an instruction is taken at the next boundary. It does not preempt the instruction. A synchronous exception of the current instruction is that instruction's result; the interrupt waits until the following boundary, and trap entry clears MIE, so the handler is not re-entered until software sets MIE.
- A change to `mtime`, `mtimecmp`, `msip`, or the PLIC that changes a hart line is visible at the next instruction boundary. Privileged §3.2.1 (machine.adoc lines 2314–2316) says a compare change is reflected in MTIP eventually, but not necessarily immediately. Immediate reflection is permitted. Spike `17e8a69f` ticks the CLINT on every CLINT write (`clint.cc` lines 96–118). Sail `6136f843` calls `update_mip` at step start (`interrupt_regs.sail` lines 243–268). The next-boundary rule matches both.
- `mtime` advances by one tick per completed Hart turn, and by the idle-jump distance during a wait (D1). It does not advance on a `SimulatorFailure`, and it does not advance because host wall time passed. `mcycle` advances by one on each completed architectural transition, including a WFI retirement and an accepted interrupt entry, and it does not advance during an idle jump. `minstret` advances by one per retired instruction, including a retired WFI, and not on a trap, an idle jump, or an interrupt entry. `ExecutionResult.cycles` stays the completed-turn count (D4). Those four numbers are allowed to be equal on a run with no wait and no counter write. A wait or a CSR write must be able to make them differ.
- `time` is a read-only shadow of `mtime` (D7). Machine mode can read it. `mcounteren` stays read-only 0, so S-mode and U-mode reads of `time`, `cycle`, and `instret` still trap. Zicntr is claimed because all three counters then exist. `mcountinhibit` stays unimplemented.
- Legal WFI waits (D3). Illegal WFI stays the Stage 4 trap.
- The Stage 4 guest that expects a legal WFI to fall through changes with that behavior. The behavioral oracle is updated. The oracle file digest is not sealed (D8).

Non-goals: a second hart; an SSWI device; any ACLINT base arrangement other than that one SiFive window; Sstc and `stimecmp`; writable `medeleg` or `mideleg`; a translation cache or any other fold-in of Stage 4 M4; trapping a non-zero FENCE `fm`; `mcountinhibit`; a writable `mcounteren`; a device tree; an SBI; a Hz register; host-time `mtime`; UART IRQ wiring; Linux boot; debug mode; PMP enforcement; the F and D extensions; a new CLI flag; a new public field on `ExecutionResult`.

## 4. Sequenced units and the public guests

The units are one milestone in order. A later unit's guest is not evidence for an earlier unit. The stage exit is the whole set on the public native routes. A component CLINT or PLIC test is not that exit.

| Unit | What becomes true | Guest or driver that shows it |
| --- | --- | --- |
| U1. Clock and counters | The native CLINT window responds. `mtime` and `mtimecmp` are read-write. `time` in M reads `mtime`. A write to `mtime` does not write `mcycle` or `minstret`. A write to `mcycle` does not write `mtime`. | `s5_counters` |
| U2. Software and timer | A store to `msip` bit 0 sets MSIP. The guest writes `mtimecmp` before it sets MTIE, and the check does not depend on the reset value of `mtimecmp`. After that write, `mtime >= mtimecmp` sets MTIP. Both bits are read-only in `mip`. MSIE and MTIE are writable and reset to 0. The interrupt is taken before fetch, in M, only when the enable is set and (the hart is below M or MIE is 1). MSI is taken before MTI when both are pending and enabled. The interrupted instruction has not retired. | `s5_software`, `s5_timer` |
| U3. External | The gateway bit sets PLIC source 1 pending while the level is high and the source is not in service. The machine context presents MEIP. The hart takes MEI before any claim. The claim read returns 1 and clears pending. The complete write ends service. If the level is still 1, pending returns. Clearing the level and completing leaves MEIP clear. Trap entry alone leaves the claim register able to return 1. The supervisor context's line ORs into SEIP and does not by itself change the software-writable SEIP bit. | `s5_external` |
| U4. WFI | A legal WFI retires once. With a locally enabled interrupt already pending, it does not wait. Wake uses `mip` & `mie` only (privileged §3.3.3, machine.adoc lines 2464–2478). The following boundary takes that interrupt when the hart is below M or MIE is 1, and `mepc` is the WFI address plus 4. In M with MIE clear, execution continues at that address and the interrupt stays pending. In S and U, an M-level interrupt is not gated by MIE. With no locally enabled interrupt and a future `mtimecmp`, a continue/run jumps `mtime` to the compare, does not add that distance to `mcycle`, `minstret`, or `cycles`, then takes the timer interrupt when the hart is below M or MIE is 1. With no future platform event, the run stops with NoProgress and does not retire until `max-cycles`. U-mode WFI, and S-mode WFI with TW = 1, still trap immediately and do not wait. Spike `17e8a69f` retires WFI and then waits. Sail `6136f843` retires WFI after the wake. Both conform. | `s5_wfi_event`, `s5_wfi_masked`, `s5_wfi_none` |
| U5. Public exit | The guests above pass on the native CLI and the native library. The Stage 4 legal-WFI fall-through expectation is updated in the behavioral oracle. Flat runs of guests that do not touch these devices stay as they are. The local quality gate and CI Quality-and-tests are green at the implementation head. | The §7 list |

U4 also has two driver checks that are not guests. A single-step of a Waiting hart returns the waiting state and does not move `mtime` (ADR-0004 §8.3). A virtual-time deadline that falls before the next `mtimecmp` stops the idle jump at the deadline and does not retire an instruction. The public CLI does not grow a deadline flag. `max-cycles` remains a Hart-turn budget.

Each new guest records source, linker, and ELF SHA-256 as a description of the bytes that were current when the expectations were written. Those three fields are not a gate (D8). The oracle checks counts, effects, privilege, `mcause`, `mepc`, the memory image, and the run result.

## 5. Routes

The new guests use MMIO, so they run on the native CLI and the native library (D9). The flat facade stays RAM-only. A flat run is not evidence for U2, U3, or U4.

No new route and no new CLI flag. `satp` still resets to 0. Existing guests that do not execute a legal WFI and do not depend on the machine interrupt enables staying read-only 0 keep their results. The known behavioral changes are the legal WFI in `s4_tvm_tsr_tw`, the `time` CSR no longer trapping in M, and MSIE, MTIE, and MEIE becoming writable.

Observation stays on the existing path. An accepted interrupt is a `TrapEntered` with no `CommitRecord`. A WFI retirement is an `InstructionRetired`. An idle jump is neither.

## 6. Measurement and evidence policy

Correctness first. No ratios, no thresholds, and no new performance payload. The A10 certification deferral stays deferred (D0).

Do not seal or re-seal `tools/a10/public-v1.json`. `suite-v1.json` gains no `oracle_sha256` pin. `--check` and `--check-artifacts` still reject a change to counts, effects, placement, or exit behavior. They do not reject a change that exists only in `source_sha256`, `linker_sha256`, or `elf_sha256`. The producer pin selected by `RISCV_REQUIRE_A10_PINNED_TOOLS` stays. This is the 2026-10-13 rule, extended to this milestone (D8).

## 7. Verification and acceptance

These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.

1. `s5_counters`, `s5_software`, `s5_timer`, `s5_external`, `s5_wfi_event`, `s5_wfi_masked`, and `s5_wfi_none` pass on the native CLI and the native library. A negative mutation of each guest fails its check.
2. A driver shows the four stages on one external source: gateway assertion, PLIC pending, MEIP presented to the hart, and trap acceptance. The same driver shows that acceptance does not consume the claim, that the claim read then returns the source ID, and that complete with the level still high makes the source pending again.
3. A driver shows that a load or store which posts a line does not take the interrupt until the instruction has retired, and that the next boundary takes it before the following fetch.
4. The WFI idle case shows `mtime` moving forward while `mcycle`, `minstret`, and `ExecutionResult.cycles` do not move by that same distance. The no-event case stops with the NoProgress diagnostic, `timed_out` clear, and a cycle count equal to the turns actually completed.
5. Single-step of a Waiting hart does not move `mtime`. A deadline earlier than the next timer stops without a retirement.
6. With no CLINT or PLIC access and no legal WFI, the existing A6 through Stage 4 suites stay as they are, except the legal-WFI fall-through in the Stage 4 TW guest and the M-mode `time` access. Illegal WFI cases in that guest stay illegal-instruction.
7. `medeleg` and `mideleg` still read 0. A machine interrupt is taken in M. `sip` and `sie` still read 0.
8. The full local quality gate is green, and CI Quality-and-tests is green at the implementation head. The ELF suite includes the new guests.
9. An independent final PR-head review has a recorded outcome. Findings are addressed on the same branch and re-verified.

## 8. Decided options

Every row is DECIDED. Decider: owner (Tony). Date: 2026-10-16 13:08 CST. Each row is owner approved, recommended option. Unchosen options stay so the choice is visible.

Roadmap §11 items 1 through 4 were answered by A8, A9, and the Stage 4 contract. Item 5 was answered for virtual memory by Stage 4. Item 6 stays conditional. Item 7, the certification layer, stays deferred. None of those is reopened here except the Stage 3 scheduling gate, which D0 inherits rather than re-litigates.

**D0. Stage 3 exit and Stage 5 entry.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option B.

The roadmap Stage 3 exit requires retained revision and PR baseline comparison, scheduled CI publication, and noise calibration. Stage 4 D0 waived that gate for Stage 4 and said the waiver does not mean A10 delivered the work. Stage 4 §9 says Stage 5 and later inherit the waiver unless a later milestone picks the work up.

- Option A, not recommended. Treat the right-sized A10 as satisfying the gate while comparison, publication, and calibration remain deferred.
- Option B, recommended. Inherit the waiver in the same words. This milestone does not deliver that layer and does not claim A10 did.
- Option C, not recommended. Block Stage 5 until the certification layer exists. The gate is a scheduling choice. ADR-0004 does not depend on it.

Rationale: the interrupt work has no architectural need for noise calibration, and claiming the gate was met would record work that was not delivered.

**D1. Time unit and `mtime` frequency.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

Privileged §3.2.1 requires `mtime` to increment at a constant frequency, and it requires the platform to provide a mechanism for determining the period of one tick. It does not set the frequency, and it does not identify `mtime` with `mcycle`. The same section's note says a simple fixed-frequency system may use one clock for both, and it also says the timer exists so that wall time can diverge from a variable core clock. ADR-0004 §5.1 sets the default framework map at one virtual-time tick to one `mtime` tick, and it forbids the Machine from inventing an `mcycle` delta during an idle jump. Host wall time is not guest time (ADR-0004 §3).

- Option A, recommended. One completed Hart turn advances virtual time by one `iss_tick` and advances `mtime` by one. An idle jump advances `mtime` by the number of ticks jumped and advances neither `mcycle` nor `minstret` nor `cycles`. There is no Hz number and no guest-readable frequency register. The period mechanism is this profile statement: one `mtime` tick equals one `iss_tick`. Reset `mtime` is 0. Reset `mtimecmp` is `2^64-1`.
- Option B, not recommended. Option A, plus a read-only doubleword that publishes a Hz value. The privileged spec does not define that register. Nothing in the exit guests needs a Hz value.
- Option C, not recommended. Use the component's 10 MHz `clock_freq` as the guest frequency and convert `iss_tick` into that crystal. The 10 MHz value is a TLM constructor default next to `ScTime`. Publishing it implies a hardware clock this model does not have.
- Option D, rejected as non-conforming to ADR-0004. Drive `mtime` from host wall time.

Whether Spike fast-forwards `mtime` to the next timer when every hart is waiting, without advancing `mcycle`, was not verified against Spike `17e8a69f`. Option A follows ADR-0004's idle-jump rule on its own. QEMU virt publishes a timebase in the device tree, commonly 10 MHz, independent of retired instructions. Sail does not fix the frequency; the platform model supplies `mtime`. Option A does not invent QEMU's device tree.

Privileged §3.2.1 (machine.adoc lines 2314–2316) permits a compare to show up in MTIP immediately. "Eventually, but not necessarily immediately" allows a delay and also allows the next instruction boundary. Spike `17e8a69f` ticks the CLINT on every CLINT write (`clint.cc` lines 96–118). Sail `6136f843` calls `update_mip` at step start (`interrupt_regs.sail` lines 243–268). The next-boundary rule matches both. A model that delays MTIP past the next instruction would make the timer guest race its own handler, which this milestone does not need.

**D2. CLINT and PLIC memory map and profile.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

Privileged §3.2.1 does not assign an address to `mtime` or `mtimecmp`. A platform map cannot violate that section by choosing an address. ACLINT 1.0-rc4 (Draft, 10 January 2022) defines MSWI, MTIMER, and SSWI as separate devices, each with its own base. MSIP0 is at offset 0 of the MSWI base, the upper 31 bits are wired 0, and the register resets to 0. MTIME is at offset 0 of a separate MTIME base, is read-write, and resets to 0. MTIMECMP0 is at offset 0 of a separate MTIMECMP base, and its reset value is unknown. That revision does not place `mtime` at offset `0x7FF8`. Its compatibility table says one SiFive CLINT is an MSWI device covering offsets `0x0` through `0x3fff` and an MTIMER device covering offsets `0x4000` through `0xbfff`. Spike's `clint` and QEMU's virt machine use that SiFive window: `msip` at offset 0, `mtimecmp` at `0x4000`, and `mtime` at `0xBFF8`, in one `0xC000` region. The component already implements that window. Putting the MTIME base at window plus `0xBFF8` makes the ACLINT MTIME register, which lives at offset 0 of its own base, land on the SiFive address. The PLIC 1.0.0 memory map is the component's PLIC map, and QEMU virt places it at `0x0C00_0000`.

Privileged §3.1.9 allows a one-hart platform to hardwire MSIP and MSIE to 0. The same section makes MSIP read-only in `mip` when the memory-mapped bit exists, and it makes SEIP the OR of a software bit and the controller signal. PLIC 1.0.0 separates gateway pending, the hart line, claim, and complete. Priority 0 never interrupts. The lowest ID wins a tie.

- Option A, recommended. SiFive CLINT at `0x0200_0000` and PLIC 1.0 at `0x0C00_0000`, as §3 describes. One hart. Implement `msip` anyway. Two PLIC contexts. Source 1 level gateway at `0x0C40_0000`. Priorities 0 through 7. Read the window as ACLINT's one-MSWI-plus-one-MTIMER compatibility layout. Do not add SSWI or any other base arrangement. Do not add Sstc. `mideleg` stays 0. Reset `mtimecmp` to all-ones. ACLINT 1.0-rc4 leaves that reset unknown (line 213), resets MTIME to 0 (line 198), and resets MSIP to 0 (line 344), so all-ones is legal. Exit guests write `mtimecmp` before they set MTIE.
- Option B, not recommended. Give MTIME, MTIMECMP, and MSWI three unrelated bases, each with its register at offset 0, and do not pack them into the SiFive `0xC000` window. That is a legal ACLINT arrangement of separate bases, and it disagrees with Spike, QEMU virt, and the component. No guest in this milestone needs the split.
- Option C, not recommended. Machine PLIC context only, and MSIP hardwired 0 under the one-hart allowance. External interrupts would still be provable through MEI, but the software-interrupt guest would fall back to the Stage 4 CSR write of SSIP, which does not exercise a platform source. The SEIP OR would stay the software bit alone.

Rationale: option A is the map the component, Spike, and QEMU virt already share, and it is the one-hart reading of ACLINT 1.0-rc4's SiFive compatibility table. MTIME reset 0 and MSIP reset 0 match that draft. The privileged architecture does not require ACLINT, and the draft can still change, so the frozen address is the SiFive window rather than a claim that every future ACLINT base is implemented. The same option keeps source assertion distinct from claim, and it gives the three guests three different platform sources. UART is not the external source. The UART object can raise a callback, and the public platform does not connect that callback. A 16550 interrupt identification path is a different device contract.

Addresses that must not move under option A: UART stays `0x1000_0000`, HTIF stays `0x4000_8000`. The CLINT window ends at `0x0200_C000`. The PLIC window ends at `0x0C40_0000`. The gateway word is the first word after that window.

**D3. WFI semantics.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

Privileged §3.3.3 (machine.adoc lines 2464–2478) says WFI tells the hart it may stall until an interrupt might need service. Wake uses `mip` & `mie` only. The hart must resume when a locally enabled interrupt becomes pending even if MIE or SIE is clear and even if `mideleg` would delegate it, and implementations should not resume for a pending interrupt that is not individually enabled. Taking the trap is a separate condition, the same one U2 uses: the hart is below M, or it is in M and MIE is 1. "Take the trap if MIE is 1" is the M-mode case. In S and U, an M-level interrupt is not gated by MIE. When the trap is taken, it is taken on the following instruction, with `mepc` equal to the WFI address plus 4. If the resume does not take a trap, execution continues at the WFI address plus 4. The same section permits a resume for any reason, and it therefore permits implementing WFI as a NOP. TW and the U-mode rule are unchanged from Stage 4: this profile traps U-mode WFI, and S-mode WFI with TW = 1, immediately. Those encodings do not retire.

Spike `17e8a69f` retires WFI and then waits until a locally enabled interrupt is pending. Sail `6136f843` retires WFI after the wake. Both conform. §3.3.3 leaves the retirement point relative to the stall unspecified, and it permits a NOP. This milestone's exit requires the wait, in Spike's retire-then-wait order.

- Option A, recommended. A legal WFI retires once (`minstret` and `mcycle` each advance by one, PC moves to the next instruction). If a locally enabled interrupt is already pending, the hart does not wait. The following boundary takes it when the hart is below M or MIE is 1, and otherwise continues. If none is pending, the hart waits. The Machine may idle-jump `mtime` to the next timer, deadline, or other platform event. The jump adds no retirement, no `mcycle` tick, and no `cycles` tick. Wake is a locally enabled pending interrupt only (`mip` & `mie`). This profile does not take the spec's permission to wake for no reason. No future event produces NoProgress rather than a spin to `max-cycles`. Illegal WFI stays the Stage 4 trap.
- Option B, not recommended. Keep the Stage 4 NOP for every legal WFI. That conforms to the permission in §3.3.3 and fails the roadmap exit. A guest cannot tell a wait from a retired hint.
- Option C, not recommended. Wait, but also wake on a pending interrupt whose individual enable is clear, or require MIE before waking. The first resumes for a cause §3.3.3 says implementations should not resume for. The second contradicts the sentence that global enables do not gate the resume.

Rationale: option A is the behavior the exit guests have to show. It follows Spike's retire-then-wait order. Sail's retire-after-wake order also conforms, and this profile records that difference so an implementation does not treat it as a spec conflict. Illegal WFI stays the Stage 4 trap. The `s4_tvm_tsr_tw` legal-WFI fall-through is a recorded compatibility change.

**D4. `ExecutionResult.cycles`.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

The roadmap says a compatibility change to this field needs approval before activation. Today the field is the number of completed Hart turns. The runner increments it for every successful step, including a Stage 4 interrupt entry, and not for a simulator failure. The struct is public, every field is public, and it is not `non_exhaustive`. Adding a field is a source break of the same kind as Stage 4's `MemoryEffect.implicit`.

Under D1 and D3, a waited WFI retires once and then the idle jump adds `mtime` without adding turns. A guest that loops on the Stage 4 NOP will no longer burn one cycle per WFI. A no-event WFI will stop instead of timing out.

- Option A, recommended. Leave the meaning as completed Hart turns, including accepted interrupt entries and the single WFI retirement, and excluding the idle jump. Add no field. A no-event wait sets `error` to the stable diagnostic `NoProgress`, leaves `timed_out` false, and reports `cycles` as the turns actually completed. A turn-budget exhaustion stays `timed_out` with the existing timeout diagnostic. Correct the field comment that says "synchronous trap entries," because interrupt entries already complete a turn. The comment change does not change the counter.
- Option B, not recommended. Redefine `cycles` as `mtime` ticks or as `mcycle`. That collapses the quantities ADR-0004 §3 keeps apart, and it changes the number for every run that waits.
- Option C, not recommended. Add `no_progress: bool`. The fact would be typed, and every downstream struct literal would break. The diagnostic in option A is enough for the guest and the driver.

Rationale: the compatibility change reviewers must accept is the WFI run's cycle count and the NoProgress stop, not a new meaning for the field. Callers that treat a clear `timed_out` and an empty `error` as success will see NoProgress as a failed run, which is the intended signal.

**D5. Whether Stage 4 M4 (TLB integration) folds in.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

Stage 4 M4 put no translation cache on the public path and recorded the roadmap's TLB deliverable as deferred. The component TLB's permission and superpage defects were fixed so a later cache would not inherit them. Stage 5's guests do not need a cache. SFENCE.VMA stays an ordering no-op while nothing is cached.

- Option A, recommended. Do not fold M4 into this milestone. The public path keeps walking on every translated access.
- Option B, not recommended. Integrate the component TLB here and make SFENCE.VMA invalidate it. That is a second fault and ordering contract on top of the interrupt contract. The Stage 4 review already separated it.

Rationale: a TLB changes fetch and load results and the meaning of SFENCE.VMA. Those proofs are not the Stage 5 exit, and mixing them makes a failure in one look like a failure in the other.

**D6. FENCE `fm` other than 0.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A. Option B is not available if the milestone obeys the spec.

Unprivileged 20250508 §2.7 (`rv32.adoc`) defines `fm = 0000` as a normal fence and `fm = 1000` with predecessor and successor both RW as FENCE.TSO. FENCE.TSO is a base I instruction and may be implemented as FENCE RW,RW (lines 856–857). Every other `fm` value, and `fm = 1000` with any other predecessor or successor, is reserved. Lines 865–867 say: "Base implementations shall treat all such reserved configurations as FENCE instructions (with `fm=0000`)." This hart has one in-order observer, so both encodings order nothing. Ztso is not claimed. The Stage 4 review said the reserved encodings were unspecified and that retiring them was a legal choice. The 20250508 sentence is a shall. Sail `6136f843` matches any `fm` (`base_insts.sail` lines 436–488). Spike executes these encodings as fences.

- Option A, recommended. Keep retiring every MISC-MEM instruction with `funct3 = 0` as a fence, including FENCE.TSO and every reserved `fm`. Do not claim Ztso. FENCE.I continues to trap.
- Option B, not recommended and not conforming. Trap when `fm` is not 0. That contradicts the shall in §2.7 and breaks the existing fence tests, including the reserved-`fm` and FENCE.TSO words.

Rationale: the review question can be closed without a behavior change. The profile records the spec rule so a later change does not start trapping a reserved `fm`.

**D7. The `time` CSR and Zicntr.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

Privileged §3.1.10 (lines 1520–1540) requires `mcycle` and `minstret` to be implemented, and §3.2.1 says `time` is a read-only shadow of `mtime`. Zicntr is `cycle`, `time`, and `instret` together (`counters.adoc` lines 12–15). Claiming Zicntr is valid once `time` exists: `mcounteren` is WARL and may be read-only 0 (§3.1.11, lines 1602–1606), and an absent `mcountinhibit` behaves as 0 (§3.1.12, lines 1628–1629). Sail `6136f843` reads `mtime` for `time` (`zicntr_control.sail` line 26). Stage 4 implements `mcycle` and `minstret`, leaves `time` illegal, and does not claim Zicntr. `mcounteren` stays read-only 0, so an S-mode or U-mode read of an enabled-counter CSR still traps.

- Option A, recommended. Add `time` (`0xC01`) as a read-only shadow of `mtime`, visible in M at the next instruction boundary after `mtime` changes. Claim Zicntr. Leave `mcounteren` read-only 0, so S-mode and U-mode still trap on `time`, `cycle`, and `instret`. A write to `time` is illegal-instruction.
- Option B, not recommended. Leave `time` illegal after `mtime` exists. That remains conforming if Zicntr is not claimed, and it makes the platform timer invisible to the CSR the unprivileged code is supposed to read. The M-mode guest can still load `mtime`, so the milestone could exit without `time`. The shadow is the smaller and more honest surface once the register exists.
- Option C, not recommended. Also make `mcounteren.TM`, CY, and IR writable. That changes every S-mode and U-mode counter trap Stage 4 just fixed, and no exit guest needs an S-mode `rdtime`.

Rationale: option A completes the trio the extension names, without reopening the Stage 4 counter-enable policy. A read-only-0 `mcounteren` and an absent `mcountinhibit` are both conforming under the citations above. Next-boundary reflection of `time` is the same rule as D1, and §3.2.1 permits that immediate reflection.

**D8. Oracle digest seal.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

The owner decision of 2026-10-13, recorded as Stage 4 D12, is that `tools/a10/public-v1.json` is not sealed or re-sealed while features are unfinished. Counts, effects, placement, and exits stay the regression. Digest fields stay descriptive.

- Option A, recommended. Extend that rule through this milestone. Updating the legal-WFI expectation and adding the §4 guests changes behavioral fields and does not add an `oracle_sha256` pin.
- Option B, not recommended. Re-seal the oracle digest as part of Stage 5. A comment or a rebuilt ELF hash would then fail a guest that had not changed architecturally.

Rationale: the rule exists so unfinished ISA work does not pretend a hash is the specification. Stage 5 is unfinished until it is approved and implemented.

**D9. Which public routes run the new guests.** DECIDED, 2026-10-16 13:08 CST, owner approved, recommended option A.

The flat facade has no SystemBus devices. UART and HTIF guests are already native-route guests. Putting a CLINT and a PLIC on the flat facade would invent a second physical map for addresses that, on flat, are either RAM offsets or faults.

- Option A, recommended. Native CLI and native library only. Flat is unchanged and is not evidence for these guests.
- Option B, not recommended. Mirror the CLINT, PLIC, and gateway into the flat address space. Two maps would have to stay byte-identical, and the flat machine has no device model to host them.

Rationale: the exit is a public guest on the public native path. Flat parity for MMIO is not an interrupt-semantics question.

## 9. Risks and approval boundary

The risks that follow from the recommended options:

- Treating a PLIC claim as the hart's trap, or treating trap entry as a claim, would pass a component test and fail `s5_external`. U3's driver is the guard.
- Advancing `mcycle` or `cycles` across a WFI idle jump would make the counters one number again. U4's guest reads all three after the jump.
- Spike `17e8a69f` reads `mtimecmp` as 0 after reset, because the compare lives in an uninitialized `std::map` and the default value is 0. With `mtime` at 0, MTIP is pending at reset. That pending state is inferred from the compare, not from a Spike comment that names the reset. Sail `6136f843` clears `msip` only (`platform.sail` lines 72–79). ACLINT 1.0-rc4 resets MTIME to 0 (line 198), leaves MTIMECMP unknown (line 213), and resets MSIP to 0 (line 344). All-ones is legal. This profile uses all-ones so reset does not post the timer. Exit guests write `mtimecmp` before they set MTIE and do not depend on that reset.
- ACLINT 1.0-rc4 is still a draft. A later ratified text could move bases. The address this milestone freezes is the SiFive window, which Spike and QEMU virt already use and which that draft's compatibility table already names.
- Making MTIP visible only "eventually," past the next instruction, would let the timer guest return before the interrupt. D1 fixes the reflection at the next boundary.
- Leaving the claim register's read path returning 0 would make a correct guest see no source ID. That component read is in scope to replace.
- The Stage 4 legal-WFI guest falls through today. After this milestone the same encoding waits. The behavioral oracle has to change in the implementation, or that guest fails for a reason the implementation intended.
- Inheriting the Stage 3 waiver leaves comparison, publication, and calibration with no owner. This draft does not pick them up.

Approval boundary. This document is not the Current contract. It does not archive Stage 4, it does not replace `docs/dev-plan.md`, and it authorizes no implementation. Activation still requires a separate approval of the decided text, and then a separate documentation-only rotation. Until that rotation, Stage 4 remains the sole Current contract.
