# Stage 4 final review

**Status:** Recorded outcome of the §7 item 10 review.

**Reviewed tree:** the Stage 4 implementation on `main` at `2066047`, plus the oracle-digest and CI changes in this pull request. The digest and CI edits do not change instruction behavior.

**Authority:** This is the review record. It is not a substitute for the [Stage 4 contract](../dev-plan.md). Formal completion of the milestone is the merge, not this document.

**Method:** Read-only comparison of the public Hart path with the contract and with the RISC-V privileged specification (the 20250508 text the contract cites). The paths read were `src/core/translation.rs`, `src/core/mod.rs` (fetch, alignment, illegal-encoding check, supervisor-interrupt sampling, trap entry), `src/core/trap.rs`, `src/isa/rv64i/system.rs`, `src/csr/mod.rs`, `src/mmu/pte.rs`, `src/mmu/sv39.rs`, and `src/mmu/translator.rs`. Guest evidence is the existing `tests/s4_*.rs` suites and `tests/bare-metal-riscv-test/rv64i/s4_*.S`. No Spike or Sail lock-step was run in this review. Where the specification leaves the behavior unspecified, this record does not invent a requirement.

## Verdict

No public-path defect was found that violates the privileged specification or the Stage 4 contract. The findings below are follow-ups. None of them is a reason to change the public Hart in this pull request.

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
| F1 | follow-up | `src/mmu/translator.rs` `check_tlb_permissions` | A component TLB hit does not apply SUM or MXR. An S-mode load of a U=1 page is allowed, and an execute-only load is refused even when MXR=1. The public Hart does not use this TLB (M4). The two §2 TLB defects, the missing D update and the dropped superpage VPN bits, are fixed on this component: a clean write drops the entry and walks, and the stored PPN is the resolved 4 KiB page. | Do not treat this TLB as the future public cache without adding SUM and MXR. Not a public-path defect. |
| F2 | follow-up | `src/mmu/mod.rs` `Satp::asid` | The component still returns a 16-bit ASID. The public `satp` write clears that field (ASIDLEN 0). The contract already records the component width as not the public width. | Leave it. A later TLB that keys on this helper would be wrong for the public profile. |
| F3 | follow-up | `src/core/trap.rs` `TrapDelegation` | The component handler can still delegate when a caller sets `TrapDelegation`. The public core constructs the handler at zero, and `medeleg` / `mideleg` cannot be written to a non-zero value. | Not a public-path control, as M1 says. |
| F4 | follow-up | `src/core/translation.rs` `MAX_AD_UPDATE_ATTEMPTS` | A walk that loses the A/D race more than the retry budget becomes `SimulatorFailure`, not another guest fault. The specification tells the implementation to retry. The budget is an implementation limit so a livelock is not silent. | Keep. A guest cannot force the budget without a concurrent host writer. |

No finding in this table was changed in this pull request.

## Open questions

No new specification question came out of this review.

These two are already decided and are restated so a later reader does not reopen them as defects:

1. SFENCE.VMA with rd≠0, and HFENCE.VVMA or HFENCE.GVMA with H absent, are reserved encodings. The privileged specification does not require a trap. This profile traps, matching Spike and Sail (D8).
2. U-mode WFI, and S-mode WFI with TW=1, trap immediately. The specification also allows a completed WFI inside an implementation-specific time bound. This profile does not take that allowance (D10).

## §7 items 9 and 10

Item 9 is the quality gate plus CI. This pull request makes the Quality-and-tests job run `cargo check`, the release ELF suite, and the required Stage 4 guests on the pull request, so a green check at that head is the gate. The local gate result is in [the closeout](stage4-closeout.md).

Item 10 is this document. Findings F1–F4 stay open as follow-ups. There is no defect fix to re-verify on the Hart.
