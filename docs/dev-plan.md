# Development Plan

**Current milestone on rotation merge:** System Bus Access Debug

**Status:** Current approved target, activated by this contract-rotation merge. Until that merge, `main` retains Program-buffer Debug Execution as Current. On merge this is the sole Current contract and the [entire Program-buffer Debug Execution contract](archive/milestones/program-buffer-debug-execution.md) becomes historical. Activation changes no simulator behavior and makes no implementation-completeness claim.

**Authority:** Normative milestone contract, constrained by accepted ADR-0001–0004, their 2026-10-28 program-buffer amendments, and their 2026-10-30 system-bus-access amendments. Source code and behavioral tests establish implemented behavior; CI and PR history retain check and review outcomes.

**Approved:** 2026-10-30, owner Tony. Decisions D1–D10 are DECIDED, all recommended choices. The [proposal entry](proposals/post-program-buffer-sba.md) is informational, not another current contract.

## 1. Objective and inherited profile

Complete the recorded Spike-style debug remainder by adding system bus access (SBA) to the single-hart debugger. OpenOCD must read and write physical target memory through `sbcs`/`sbaddress`/`sbdata` without executing an abstract command, while the hart is halted or running, with the pinned comparison ISS providing named-behavior parity. Existing abstract register access, physical abstract memory, program-buffer execution, and run control remain unchanged.

The inherited profile is RV64IMA/Zicsr plus Zifencei, M/S/U, IALIGN=32, the selected Stage 5 platform/interrupt/WFI rules, Stage 6 Debug Specification 1.0 behavior, and the completed Program-buffer Debug Execution decisions D1–D9. This contract adds no trigger module, second hart, Sdtrig, Quick Access, F/D/C/V extension, SBI, Linux target, or TLM/SystemC integration. Existing zero-entry PMP and Sv39 behavior remain unchanged.

This is the next bounded debug increment and the first inbound Platform master. SBA is a DM-owned physical requester admitted by the Machine; it is not disguised Abstract Access Memory and introduces no arbitration, coherence, or further masters.

The inherited Stage 3 performance-certification waiver continues. Comparison publication, noise calibration, thresholds, oracle sealing and new performance payloads are not taken on. `tools/a10/public-v1.json` and the suite's lack of an oracle pin remain unchanged.

## 2. Specification and selected decisions

The normative Debug reference is RISC-V Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. The ISA Manual reference is document version 20250508. OpenOCD is 0.12.0. The comparison ISS is Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`, run with `--dm-sba=64` so its SBA block is enabled; comparison checks named behavior, not an output digest.

Every decision below was owner approved on 2026-10-30.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery | SBA as the next single bounded milestone, before any TLM/SystemC or multi-hart work. |
| D2. Register block | `sbcs` version 1, `sbasize=64`, `sbaddress0`/`sbaddress1`, `sbdata0`/`sbdata1`. No `sbaddress2/3`, no `sbdata2/3`. `sbaccess` values 0–3 (8/16/32/64-bit) supported and advertised in `sbaccess8/16/32/64`; `sbaccess128` reads 0. |
| D3. Initiator kind | SBA is the first inbound Platform master under the amended ADR-0002 §9: a DM-owned physical requester admitted by the Machine. It is not the Hart's `PhysicalAccess` port, performs no translation, PMP, or architectural check, and is not disguised Abstract Access Memory. |
| D4. Availability | SBA operates while the hart is halted or running. A running-hart SBA transaction is admitted at a coherent Hart boundary, like DMI service; it never preempts an in-flight instruction or abstract command. |
| D5. Error semantics | `sberror`: 0 none, 2 bad address (unmapped/denied physical target), 3 alignment, 4 unsupported size. Sticky until W1C; while `sberror` is non-zero the DM initiates no further system bus accesses. `sbbusy`/`sbbusyerror` follow the specification, including setting `sbbusyerror` on access while busy and blocking further SBA until cleared. No timeout or "other" error is fabricated. |
| D6. Side-effect protocol | `sbreadonaddr`, `sbreadondata`, and `sbautoincrement` follow the specification exactly. An SBA read of MMIO performs that device's defined successful-read side effects; there is no side-effect-free peek. |
| D7. Reservation visibility | A committed SBA write is a competing write for ADR-0002 reservation visibility: on this single-hart Machine it invalidates the hart's LR/SC reservation on overlap. SBA issues no atomic envelope and performs no AMO/LR/SC. |
| D8. Accounting | SBA transactions add no guest retirement, `mcycle`/`minstret`/`mtime` charge, turn budget, or `ExecutionResult.cycles` charge. Physical delay metadata follows the existing ADR-0004 ownership. Committed MMIO effects remain Platform effects with debug-operation provenance. |
| D9. Reset and lifecycle | A reset request drains or rejects in-flight SBA work under the existing reset policy; sticky SBA errors survive non-DM reset like other DM error state, and DM reset clears SBA command/busy state. Unknown completion retains the existing quarantine semantics. |
| D10. Tool verification | OpenOCD is forced to `riscv set_mem_access sysbus`; a fallback to program-buffer or abstract memory does not pass. The pinned Spike comparison runs with `--dm-sba=64` and matching `sbasize`/width profile; only explicitly justified profile differences appear in test expectations. Two pinned-Spike deviations are pre-identified as justified differences: Spike reports `sberror=3` (not the specification's 4) for an unsupported `sbaccess` size, and performs no SBA alignment check. This contract follows the specification on both; test expectations name Spike's behavior where they differ. |

## 3. Ownership and architectural semantics

### Inbound initiator and ownership

The DM owns the SBA register block, `sbbusy`/sticky error state, and the autoincrement/read-on-access protocol. The Platform owns physical routing, raw-byte and width rules, target side effects, and target-fault reporting, exactly as for Hart-issued transactions. The Machine owns admission at coherent boundaries and reset draining. The Hart is not involved in an SBA transaction: no fetch, no translation, no PMP, no trap, no retirement, and no architectural register effect. The transport only carries DMI.

### Transactions, errors, and reservations

SBA shares the Platform's physical semantics with Hart-issued transactions as amended ADR-0002 §9 requires: all-or-nothing transactions, no partial RAM or device update on failure, and visibility of committed SBA writes to Hart reservation invalidation. An SBA transaction must not observe or produce an intermediate state of a Hart AMO or SC, and a failed SBA access causes no reservation invalidation, interrupt, or callback. The normalized physical fault of ADR-0002 §4 maps to the DM's SBA error taxonomy (bad address, alignment, unsupported size) at the DM boundary; simulator/internal failures retain their existing non-guest classification and unknown-completion quarantine.

### Scheduling, accounting, and lifecycle

Interrupt, WFI, and halt/run scheduling rules are unchanged. Pending SBA work must not be mistaken for WFI or NoProgress and must not cause an idle-time jump. SBA does not create halt causes, does not touch `dcsr`/`dpc`, and does not interact with program-buffer or abstract-command state beyond sharing the DM's busy/error discipline where the specification requires. SBA transactions add no guest retirement, counter, turn-budget, or `ExecutionResult.cycles` charge; delay metadata consumption follows ADR-0004's existing ownership. A reset request drains or rejects in-flight SBA work, sticky SBA errors survive non-DM reset, and DM reset clears SBA command/busy state. Unknown completion remains non-resumable without adapter proof.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. DM SBA register block | Discover `sbcs` version/size/width capabilities. Exercise every `sbaccess` width, `sbreadonaddr`, `sbreadondata`, `sbautoincrement` including the `sbaddress1` carry, `sbbusy`, busy-time access, `sbbusyerror` set/block/W1C, and each `sberror` case with sticky/W1C recovery, including that a non-zero `sberror` blocks new SBA accesses until cleared. |
| U2. Platform inbound path | Through the amended boundary: RAM and native MMIO reads/writes at every width, MMIO read/write side effects, all-or-nothing failure with no partial update, bad-address/alignment/unsupported-size mapping, and no Hart translation or trap involvement. Prove a committed SBA write invalidates an overlapping Hart reservation and a failed one does not. |
| U3. Machine integration | SBA while the hart runs and while halted; admission at coherent boundaries with idle transport; no lock held across target access; no interference with an active abstract command or program-buffer execution; reset/abort draining and DM-reset clearing; pending SBA never reported as WFI/NoProgress. |
| U4. Public tools | Force OpenOCD `riscv set_mem_access sysbus`; a fallback to progbuf or abstract memory does not pass. Transfer at least four adjacent distinct values, cover all four supported widths, and independently check addresses through abstract memory. Compare named results with pinned Spike run with `--dm-sba=64`. The existing progbuf and abstract sessions remain regression controls. |
| U5. Exit | CI runs the full Rust quality gate, fresh guest ELF suite, extended tool script, legacy regressions and unit-specific negative controls. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Required negative controls include `sbaccess=4` (128-bit) rejected, SBA access while `sbbusy`, a write that silently skips reservation invalidation, SBA routed through the Hart's `PhysicalAccess` port, autoincrement that skips the `sbaddress1` carry, and an SBA access accepted mid-instruction. Keep their small patches, commands and expected assertions as verification code or test inputs under the existing mutation-runner pattern.

The standard gate remains `cargo fmt --all -- --check`, `cargo check --all-features`, strict all-target Clippy, `cargo test --all-features`, `cargo doc --all-features --no-deps`, the fresh guest ELF suite and the required live debug-attach CI. Extend the existing tool script for the forced sysbus session and retain the program-buffer and abstract-memory sessions as regressions. Do not add an execution ledger, fixed successful-output inventory or hash gate to certify these requirements.

## 5. Exclusions and retained direction

No DMA engine, co-simulation master, second hart, multi-initiator ordering or coherence, watchpoints, Sdtrig, Quick Access, F/D/C/V extension, new protection/virtualization profile, legal DRET, Linux/SBI, TLM/SystemC/DMI integration, performance certification, oracle sealing, or new public application result field.

Conditional transport integration remains a separate direction requiring a concrete consumer and fixture. The multi-master remainder of ADR-0002 §9 (arbitration, coherence, further masters) stays deferred, and its DMI-range-invalidation rule is not applicable to this profile and remains deferred with transport integration. Neither is silently carried into this contract.

## 6. Activation boundary

This documentation-only rotation preserves the entire Program-buffer Debug Execution contract, applies the bounded ADR-0001–0004 system-bus-access amendments and activates this target atomically on merge. CI/PR history records delivery and review. It does not assert runtime support, start implementation work, publish a release, or seal an oracle. Later implementation must meet the units above on its own reviewed commits.

## References

- [Debug Specification 1.0 SBA registers](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/xml/dm_registers.xml) (`sbcs`, `sbaddress0`–`2`, `sbdata0`–`3`) and [DM chapter](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc).
- [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [Pinned Spike](https://github.com/riscv-software-src/riscv-isa-sim/tree/5d99611782ef8dd81b65b842369bd351a9d4702a) (`--dm-sba` enables its SBA block) and [OpenOCD 0.12.0](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
- [Post-A7 roadmap](proposals/post-a7-roadmap.md) §9 remainder and [SBA proposal entry](proposals/post-program-buffer-sba.md).
