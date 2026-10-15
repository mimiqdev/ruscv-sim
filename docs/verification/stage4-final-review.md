# Stage 4 final review

**Status:** Recorded outcome of the §7 item 10 review.

**Reviewed tree:** the Stage 4 implementation on `main` at `2066047`, plus the oracle-digest and CI changes in this pull request. The digest and CI edits do not change instruction behavior. F1–F5 were fixed on this branch after that review. The regression tests named in the findings table cover those fixes.

**Authority:** This is the review record. It is not a substitute for the [Stage 4 contract](../dev-plan.md). Formal completion of the milestone is the merge, not this document.

**Method:** Read-only comparison of the public Hart path with the contract and with the RISC-V privileged specification (the 20250508 text the contract cites). The paths read were `src/core/translation.rs`, `src/core/mod.rs` (fetch, alignment, illegal-encoding check, supervisor-interrupt sampling, trap entry), `src/core/trap.rs`, `src/isa/rv64i/system.rs`, `src/csr/mod.rs`, `src/mmu/pte.rs`, `src/mmu/sv39.rs`, and `src/mmu/translator.rs`. Guest evidence is the existing `tests/s4_*.rs` suites and `tests/bare-metal-riscv-test/rv64i/s4_*.S`. No Spike or Sail lock-step was run in this review. Where the specification leaves the behavior unspecified, this record does not invent a requirement.

## Verdict

No public-path defect was found that violates the privileged specification or the Stage 4 contract. Findings F1–F5 were component-path follow-ups. Each one now has a fix and a regression test in this pull request. The reserved FENCE `fm` question stays open.

## What was checked

| Area | Result |
| --- | --- |
| Sv39 canonicity (VA bits 63:39 equal bit 38), checked before any walk read | Matches D3. A failure is a page fault of the original access kind. |
| Reserved PTE bits 63:54, W=1 with R=0, non-leaf D/A/U, level-0 non-leaf, misaligned superpage | `PageTableEntry::valid_sv39_at_level` rejects each of these. Both the Hart walk and the component walker use it. |
| A/D update | Hardware sets A on a completed translation and D on a store-class translation, including SC and AMO, and not on LR. The update is a walk-private load-reserved/store-conditional. A failed conditional write writes nothing and the walk re-reads. A rejected walk read or A/D write is an access fault of the original kind. Host, protocol, and unknown completions are `SimulatorFailure`. |
| Permissions | U, SUM, and MXR are applied on the Hart walk. An S-mode fetch of a U=1 page faults regardless of SUM. MXR widens loads only. |
| MPRV | Only an M-mode load or store adopts MPP. Fetch keeps the true privilege. `mret` clears MPRV only when the new privilege is below M. `sret` clears MPRV because SPP is always below M. |
| Privilege and returns | `mret` is legal only in M. `sret` is legal in M and in S, illegal in U, and illegal in S when TSR=1. SPIE is set to 1. SEPC and MEPC bits 1:0 are cleared. MPP=2 cannot be written; the WARL rule keeps the previous MPP. |
| TVM, TSR, TW, WFI | S-mode `satp` and SFENCE.VMA trap when TVM=1. S-mode SRET traps when TSR=1. U-mode WFI traps at either TW value. S-mode WFI traps when TW=1 and retires as a NOP when TW=0. M-mode WFI retires as a NOP. |
| Delegation | `medeleg` and `mideleg` legalize to the value already stored, which reset leaves at 0. `sip` and `sie` are masked by `mideleg`, so both read 0 and ignore writes. The public `TrapHandler` is constructed with an empty `TrapDelegation`, so every trap is taken in M. |
| Supervisor interrupts | SSIP, STIP, and SEIP are taken from `mip` and `mie` bits `0x222`. They are taken in U and in S with MIE clear, and in M only when MIE is 1. SEI is taken before SSI, and SSI before STI. Vectored `mtvec` adds `4 * cause` only for interrupts. |
| `satp` | MODE 0 and 8 are stored with the ASID field cleared. Any other MODE leaves the register unchanged. |
| CSR field policy | `mstatus` writable bits are SIE, MIE, SPIE, MPIE, SPP, MPP, MPRV, SUM, MXR, TVM, TW, and TSR. FS, XS, VS, SD, UBE, SXL, and UXL are not writable. SXL and UXL read 2. Reset `mstatus` is `0x0000_000A_0000_0000`. Reset `sstatus` is that value masked to the S view, `0x0000_0002_0000_0000`. `misa` is fixed at `0x8000_0000_0014_1101`. `menvcfg` and `senvcfg` keep only FIOM. |
| F, D, FENCE.I | LOAD-FP, STORE-FP, and OP-FP are rejected in `is_illegal_encoding` before alignment, translation, and any data request. MADD, MSUB, NMSUB, and NMADD are not decoder opcodes, so they take the same illegal-instruction path at decode. FENCE.I is illegal. The fetch of the instruction itself still walks when Sv39 is on. |
| SFENCE.VMA | `raw & 0xFE00_7FFF == 0x1200_0073` accepts every rs1 and rs2 with rd=0 and funct3=0. It is legal in M and in S, illegal in U, and illegal in S when TVM=1. With no public translation cache it is an ordering no-op. A non-canonical rs1 raises no exception. rd≠0 and the H fences stay illegal-instruction. |
| PMP | Even `pmpcfg` and `pmpaddr0` through `pmpaddr63` read 0 and ignore writes. Odd `pmpcfg` addresses do not exist. There is no PMP enforcement on the public path. |
| §4 families | Each family has a guest under `tests/bare-metal-riscv-test/rv64i/s4_*.S`, run on the CLI, `load_and_run`, and `RiscVSimulator` by `tests/s4_sv39_guests.rs` and `tests/s4_profile_guests.rs`. The ELF suite now requires those same names. |
| §7 items 1–8 | Covered by those guests, by `tests/s4_sv39_translation.rs` (item 2), by the A6–A10 suites (item 3, with the D7 exceptions), and by `tests/s4_instruction_profile.rs`, `tests/s4_d10_counters.rs`, and `tests/s4_d11_supervisor_interrupts.rs`. Item 4 is the `s4_sfence_vma` guest, not a TLB unit test. |

## Findings

| Id | Severity | Location | Finding | Disposition |
| --- | --- | --- | --- | --- |
| F1 | fixed | `src/mmu/pte.rs` `leaf_permits` | The walker (`check_leaf_permissions`) and the TLB hit path (`check_tlb_permissions`) each had their own permission check. Both allowed an S-mode fetch of a U=1 page and ignored MXR. | Both call `leaf_permits`. An S-mode load of a U=1 page faults unless SUM is set, an S-mode fetch of a U=1 page faults even when SUM is set, and MXR widens loads only. The same rules apply on the walk and on a later TLB hit. Regression: `sum_mxr_and_supervisor_fetch_use_leaf_permits_on_walk_and_tlb_hit`. |
| F2 | fixed | `src/mmu/mod.rs` `Satp::asid` | The helper returned bits 59:44. ASIDLEN is 0, and a public `satp` write already clears that field. | `Satp::asid` returns 0. A translator fill is tagged with that value, so two `satp` values that differ only in bits 59:44 share one TLB entry. A direct `Tlb::insert` still stores the ASID on the entry. Regression: `raw_asid_bits_do_not_change_satp_asid_or_tlb_identity`. |
| F3 | fixed | `src/core/trap.rs` `TrapDelegation` | `delegate_exception`, `delegate_interrupt`, and `delegation_mut` could store a non-zero delegation. The public handler is constructed at zero, and `medeleg` / `mideleg` cannot be written nonzero. | The setters are gone. `medeleg` and `mideleg` are private and stay 0, so `should_delegate_exception` and `should_delegate_interrupt` are false for every cause a caller can construct. A trap taken from S stays in M. Regression: `test_trap_delegation_stays_clear`, `test_supervisor_source_exception_stays_in_machine`, `test_supervisor_source_interrupt_stays_in_machine`. |
| F4 | fixed | `src/core/translation.rs` and `src/mmu/sv39.rs` | A lost A/D race past 64 attempts became `SimulatorFailure::Invariant`. The specification says to restart the walk. | The retry has no budget on the Hart walk and on the component walker. `WalkResult::AdUpdateRetryLimit` and `MmuError::WalkAdRetryLimit` are gone. A host that never lets the store-conditional succeed can livelock; that is the specification. Host, protocol, and unknown completions stay `SimulatorFailure`. Regression: `ad_race_past_the_old_retry_budget_still_retires` and `ad_race_longer_than_the_old_budget_still_commits`. |
| F5 | fixed | `src/core/trap.rs` `sstatus_on_trap_entry` | The Supervisor arm cleared bits with `!0x222`. That mask clears bits 1, 5, and 9 and leaves SPP (bit 8) unchanged. | The clear mask is `!(SIE \| SPIE \| SPP)`, which is `!0x122`. SPIE receives the old SIE. SPP is clear when the source is U and set when the source is S. Bit 9 survives the mask. The public path still takes every trap in M, because delegation cannot be set. The Supervisor arm is what an in-module test reaches by writing the private field. Regression: `sstatus_trap_entry_clears_spp_and_keeps_bit_9` and `supervisor_trap_arm_uses_the_spp_mask`. |

F1–F5 are fixed in this pull request.

## Open questions

One encoding is unspecified, and this profile already chooses a behavior for it. It does not need a code change for this contract.

1. A base FENCE whose `fm` field is not 0, including FENCE.TSO when Ztso is absent, is a reserved MISC-MEM encoding. `is_illegal_encoding` accepts every MISC-MEM instruction with `funct3 = 0`, and the Hart retires it as a fence. The unprivileged specification leaves the reserved `fm` values unspecified. Retiring them as a fence is a legal choice for unspecified behavior. The Stage 4 contract requires FENCE.I to trap, which this profile does. Whether a later profile should trap a non-zero `fm` stays open.

These two are already decided and are restated so a later reader does not reopen them as defects:

1. SFENCE.VMA with rd≠0, and HFENCE.VVMA or HFENCE.GVMA with H absent, are reserved encodings. The privileged specification does not require a trap. This profile traps, matching Spike and Sail (D8).
2. U-mode WFI, and S-mode WFI with TW=1, trap immediately. The specification also allows a completed WFI inside an implementation-specific time bound. This profile does not take that allowance (D10).

## §7 items 9 and 10

Item 9 is the quality gate plus CI. This pull request makes the Quality-and-tests job run `cargo check`, the release ELF suite, and the required Stage 4 guests on the pull request, so a green check at that head is the gate. The local gate result is in [the closeout](stage4-closeout.md).

Item 10 is this document. Findings F1–F5 are fixed in this pull request, each with a regression test named in the table above. The reserved FENCE `fm` question in the next section stays open.
