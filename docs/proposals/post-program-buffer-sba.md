# System Bus Access debug proposal

**Status:** Draft, awaiting owner approval. Approved selections move into the successor milestone contract on the rotation merge; this entry then becomes informational navigation like the program-buffer proposal.

**Authority:** Informational proposal. It is not a milestone contract, does not replace [`docs/dev-plan.md`](../dev-plan.md), and makes no implementation claim. The Program-buffer Debug Execution contract remains the sole Current contract until the rotation merge.

**Baseline:** `main` at merge commit `3085f9e` (PR #118), which completes program-buffer U1–U5. CI on that head is green (Quality and tests, Debug attach).

## 1. Objective and inherited profile

Complete the recorded Spike-style debug remainder by adding system bus access (SBA) to the single-hart debugger. OpenOCD must read and write physical target memory through `sbcs`/`sbaddress`/`sbdata` without executing an abstract command, while the hart is halted or running, with the pinned comparison ISS providing named-behavior parity. Existing abstract register access, physical abstract memory, program-buffer execution, and run control remain unchanged.

The inherited profile is RV64IMA/Zicsr plus Zifencei, M/S/U, IALIGN=32, the selected Stage 5 platform/interrupt/WFI rules, Stage 6 Debug Specification 1.0 behavior, and the program-buffer decisions D1–D9. This proposal adds no trigger module, second hart, Sdtrig, Quick Access, F/D/C/V extension, SBI, Linux target, or TLM/SystemC integration. Existing zero-entry PMP and Sv39 behavior remain unchanged.

The inherited Stage 3 performance-certification waiver continues. Comparison publication, noise calibration, thresholds, oracle sealing and new performance payloads are not taken on. `tools/a10/public-v1.json` and the suite's lack of an oracle pin remain unchanged.

## 2. Specification and decisions to approve

The normative Debug reference remains RISC-V Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. The ISA Manual reference remains document version 20250508. OpenOCD remains 0.12.0. The comparison ISS remains Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`, run with `--dm-sba=64` so its SBA block is enabled; comparison checks named behavior, not an output digest.

| Decision | Proposed selection |
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

The DM owns the SBA register block, `sbbusy`/sticky error state, and the autoincrement/read-on-access protocol. The Platform owns physical routing, raw-byte and width rules, target side effects, and target-fault reporting, exactly as for Hart-issued transactions. The Machine owns admission at coherent boundaries and reset draining. The Hart is not involved in an SBA transaction: no fetch, no translation, no PMP, no trap, no retirement, and no architectural register effect.

SBA shares the Platform's physical semantics with Hart-issued transactions as ADR-0002 §9 requires: all-or-nothing transactions, no partial RAM or device update on failure, and visibility of committed SBA writes to Hart reservation invalidation. An SBA transaction must not observe or produce an intermediate state of a Hart AMO or SC, and a failed SBA access causes no reservation invalidation, interrupt, or callback.

Interrupt, WFI, and halt/run scheduling rules are unchanged. Pending SBA work must not be mistaken for WFI or NoProgress and must not cause an idle-time jump. SBA does not create halt causes, does not touch `dcsr`/`dpc`, and does not interact with program-buffer or abstract-command state beyond sharing the DM's busy/error discipline where the specification requires.

## 4. Draft ADR-0002 amendment

The following text is the proposed ADR-0002 amendment, applied to the ADR only on the rotation merge after owner approval, in the same style as the 2026-10-28 program-buffer amendment. Coordinated small amendments to ADR-0001 (no Hart outcome or observation is fabricated for SBA), ADR-0003 (Machine admission and Platform ownership of the inbound path), and ADR-0004 (SBA delay consumption adds no guest or turn budget charge) are drafted with the successor contract and activate with it.

> **Amendment: system bus access inbound master.** The selected debug profile adds system bus access as the first inbound Platform master under §9. The DM originates physical read and write requests of 8, 16, 32, or 64 bits at an explicit physical address; the Platform routes them with the same raw-byte, width, routing, all-or-nothing, side-effect, and target-fault semantics as Hart-issued transactions. SBA performs no Hart translation, privilege, PMP, or architectural check, issues no atomic envelope, and never enters a guest trap or ISA execution path.
>
> A committed SBA write is a competing write for reservation visibility: the Platform reports it so an overlapping Hart reservation is invalidated under the existing Hart-owned architectural rule. A failed SBA transaction invalidates nothing. An SBA transaction must not expose an intermediate state of a Hart atomic operation.
>
> The normalized physical fault of §4 maps to the DM's SBA error taxonomy (bad address, alignment, unsupported size) at the DM boundary; simulator/internal failures retain their existing non-guest classification and unknown-completion quarantine. SBA requests and responses carry §7 delay metadata unchanged; ADR-0004 owns its consumption. The §9 DMI-range-invalidation rule is not applicable here: no direct-memory-interface mechanism exists in this profile, and the rule remains deferred with transport integration. This amendment selects no Rust API, transport encoding, arbitration, or multi-initiator ordering beyond the single-hart visibility rule above. DMA, coherence, watchpoints, and further masters remain deferred under §9.

## 5. Sequenced units and acceptance

| Unit | Required observable proof |
| --- | --- |
| U1. DM SBA register block | Discover `sbcs` version/size/width capabilities. Exercise every `sbaccess` width, `sbreadonaddr`, `sbreadondata`, `sbautoincrement` including the `sbaddress1` carry, `sbbusy`, busy-time access, `sbbusyerror` set/block/W1C, and each `sberror` case with sticky/W1C recovery, including that a non-zero `sberror` blocks new SBA accesses until cleared. |
| U2. Platform inbound path | Through the amended boundary: RAM and native MMIO reads/writes at every width, MMIO read/write side effects, all-or-nothing failure with no partial update, bad-address/alignment/unsupported-size mapping, and no Hart translation or trap involvement. Prove a committed SBA write invalidates an overlapping Hart reservation and a failed one does not. |
| U3. Machine integration | SBA while the hart runs and while halted; admission at coherent boundaries with idle transport; no lock held across target access; no interference with an active abstract command or program-buffer execution; reset/abort draining and DM-reset clearing; pending SBA never reported as WFI/NoProgress. |
| U4. Public tools | Force OpenOCD `riscv set_mem_access sysbus`; a fallback to progbuf or abstract memory does not pass. Transfer at least four adjacent distinct values, cover all four supported widths, and independently check addresses through abstract memory. Compare named results with pinned Spike run with `--dm-sba=64`. The existing progbuf and abstract sessions remain regression controls. |
| U5. Exit | CI runs the full Rust quality gate, fresh guest ELF suite, extended tool script, legacy regressions and unit-specific negative controls. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Required negative controls include `sbaccess=4` (128-bit) rejected, SBA access while `sbbusy`, a write that silently skips reservation invalidation, SBA routed through the Hart's `PhysicalAccess` port, autoincrement that skips the `sbaddress1` carry, and an SBA access accepted mid-instruction. Keep their small patches, commands and expected assertions as verification code or test inputs under the existing mutation-runner pattern.

## 6. Exclusions and retained direction

No DMA engine, co-simulation master, second hart, multi-initiator ordering or coherence, watchpoints, Sdtrig, Quick Access, F/D/C/V extension, new protection/virtualization profile, legal DRET, Linux/SBI, TLM/SystemC/DMI integration, performance certification, oracle sealing, or new public application result field.

Conditional transport integration remains a separate direction requiring a concrete consumer and fixture. The multi-master remainder of ADR-0002 §9 (arbitration, coherence, further masters) stays deferred, and its DMI-range-invalidation rule is not applicable to this profile and remains deferred with transport integration. Neither is silently carried into this proposal.

## 7. Rotation plan

On owner approval of D1–D10: one documentation-only rotation PR archives the entire Program-buffer Debug Execution contract to `docs/archive/milestones/` with its scope, completion date (2026-10-30, PR #118) and known limitations, applies this ADR-0002 amendment and the coordinated ADR-0001/0003/0004 amendments, and installs the successor contract as the sole `docs/dev-plan.md`. The rotation implements no SBA behavior; implementation proceeds in the U1–U5 sequence on its own reviewed commits.

## References

- [Debug Specification 1.0 SBA registers](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/xml/dm_registers.xml) (`sbcs`, `sbaddress0`–`2`, `sbdata0`–`3`) and [DM chapter](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc).
- [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md) §9 inbound-master deferral, [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [Pinned Spike](https://github.com/riscv-software-src/riscv-isa-sim/tree/5d99611782ef8dd81b65b842369bd351a9d4702a) (`--dm-sba` enables its SBA block) and [OpenOCD 0.12.0](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
- [Post-A7 roadmap](post-a7-roadmap.md) §9 remainder and [program-buffer proposal](post-stage6-program-buffer-debug.md).
