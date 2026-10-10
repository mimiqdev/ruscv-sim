# Stage 4 virtual memory and protection (proposed milestone contract draft)

**Status:** Proposed draft. **Not approved, not active**. This document is a bounded approval candidate modeled on the structure of [docs/dev-plan.md](../dev-plan.md). Activation requires (1) explicit user answers to every open decision in §8, (2) a revision of this draft into the accepted contract, and (3) a separate documentation-only rotation that archives the A10 contract, mirroring the A9→A10 transition (PRs #70/#71). Nothing here is implemented, scheduled or claimed.

**Authority if approved:** would become the sole Current normative milestone contract, constrained by accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md) (Hart execution outcome and observation), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md) (physical access transaction and fault), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) (runner, Machine and platform ownership) and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md) (interrupt-time scheduling and stop boundaries). Translation and protection are Hart-owned work feeding the existing physical port taxonomy. No new ADR is proposed by this draft; if review finds an ADR gap (e.g. TLB/invalidation ownership), that gap is surfaced before implementation, not patched mid-milestone.

**Planning baseline:** merged A10 head `5d23d337a6732d178a2e14445be9f5c28154f270` (PR #72, 2026-10-10). Follows [post-A7 roadmap Stage 4](post-a7-roadmap.md); the roadmap's Stage-3 entry gate was satisfied by the delivered A10 baseline framework, which is a scheduling gate, not an architectural dependency.

**Relationship to the deferred certification layer:** the A10 performance framework (baseline profile, smoke profile, schema/bundle machinery) exists and is tested; this draft does **not** add performance payloads or thresholds, and does not require the deferred certification layer.

## 1. Proposed objective and architecture

Objective, if later approved: connect one privilege and address-translation profile to the common physical port so a public ELF guest can run with translation on. Which profile, which page-table mode, and whether PMP is included are open in §8. Fetch, load, and store would be translated, a permission violation would become an architectural page fault that keeps the original virtual access kind and address, A/D effects would stay visible, and the PMP choice would be explicit. No A8, A9, or A10 guarantee would be weakened.

Boundary (roadmap Stage 4 wording, retained): page-table reads and A/D writeback flow through the same physical result taxonomy; the original virtual access kind and address are retained when mapping physical/page faults; successful A/D effects remain separate and visible; Hart-owned translation/TLB integration; explicit PMP decision; small public ELF guests for bare, mapped, permission-fault, page-walk-fault, A/D, TLB-flush and protection cases; exact compatibility decision for image-base adaptation versus identity/bare configuration.

This is the proposed **first bounded milestone** of virtual-memory work: one selected profile, single hart, no OS/Linux boot, no hypervisor, no multi-hart TLB coherence, no interrupt virtualization. Sv39-only, which privilege levels, and whether PMP is in or out are OPEN DECISIONS in §8. This draft delivers none of them. A later milestone may extend whatever profile is chosen.

## 2. Entry evidence and existing controls (verified at the baseline)

Actual available seams, not capability claims (component presence ≠ public-path support):

- **Privilege state exists and is Hart-owned but barely exercised publicly.** `PrivilegeMode { User, Supervisor, Machine }` and staged trap transitions live in [`src/core/mod.rs`](../../src/core/mod.rs); `mret`/`sret`/`uret` and `ecall`/`ebreak` in [`src/isa/rv64i/system.rs`](../../src/isa/rv64i/system.rs); CSRs including `satp` (0x180) in [`src/csr/`](../../src/csr/). Verified by [`tests/privilege_transition_test.rs`](../../tests/privilege_transition_test.rs) (19 tests), [`tests/csr_access_test.rs`](../../tests/csr_access_test.rs) (27), and the A6 machine-mode trap entry/return suites. No public ELF guest today transitions privilege modes or enables translation.
- **Sv39/MMU components are implemented and component-tested, not integrated.** [`src/mmu/`](../../src/mmu/): `Satp` parsing (modes Bare=0/Sv39=8/Sv48=9, others rejected), `Mmu::translate` over `PhysicalMemoryInterface`, `MmuError { PageFault(reason), AccessFault, PmpViolation, UnsupportedMode, InvalidSatpMode }`, A/D bit set/clear in `Pte`, and `Mmu::flush_tlb` labeled "SFENCE.VMA implementation". Verified by [`tests/translation_test.rs`](../../tests/translation_test.rs) (23 tests) and [`tests/ad_bits_test.rs`](../../tests/ad_bits_test.rs) (10). **The public core path never calls these**: fetch/load/store use image-base/storage adaptation, page-table accesses use the MMU-specific `PhysicalMemoryInterface`, not the A7/A8 `PhysicalAccess` route, and no public instruction decodes or executes SFENCE.VMA.
- **PMP is nominal only.** `MmuConfig::pmp_entries` (default 16) and `MmuError::PmpViolation` exist; no `pmpcfg`/`pmpaddr` CSRs, no enforcement anywhere, no tests. Treat as absent.
- **The integration seam is clean.** A8/A9 put every fetch/load/store/AMO through the validated physical data port with a complete fault taxonomy (target/host/protocol/unknown) and original-address `mtval` preservation, orchestrated by the Hart behind `Machine::step`. This is exactly the seam a translator adapter plugs into; the roadmap's "connect to the common physical port" phrasing maps to an adapter between Hart virtual accesses and the existing `PhysicalAccess` route, with page-table walks themselves issued as physical accesses.

What this means for planning: the milestone is an **integration + completion** milestone (wire existing components through the public path, add SFENCE.VMA decode/dispatch, add PMP for real or exclude it explicitly, decide A/D policy), not a from-scratch MMU build. That is why the scope can stay bounded.

## 3. Proposed scope and exclusions (none of §8 is decided)

The list below is the candidate work **if** the matching §8 option is later chosen. It is not selected scope. Each bullet names the open decision that still owns it.

- One privilege/translation profile, only after D1 and D2. No mode set is in scope until those answers exist.
- Hart-side translation of instruction fetch, load/store, and AMO effective addresses, only for the `satp` modes D2 accepts. Walk behavior is D3.
- Architectural page faults that keep the original virtual access kind and address, only under the fault rule D6 chooses. No new trap channel is proposed; delivery would use the existing A6 trap entry.
- A/D bit updates only under D4. Hardware-set, trap-on-unset, and a per-Machine flag are all still open.
- SFENCE.VMA decode and dispatch only if D2/D3 include a TLB. Component `flush_tlb` is not public-path support.
- PMP only under D5. Exclusion, 8 entries, and 16 entries are all still open. `MmuConfig::pmp_entries` (default 16) and `MmuError::PmpViolation` exist with no `pmpcfg`/`pmpaddr` CSRs and no enforcement.
- Misaligned access under translation only under D6.
- Public compatibility only under D7. Image-base adaptation versus an identity/bare configuration is part of that decision, matching roadmap Stage 4.
- A public ELF fixture family sized to the chosen options: bare sanity, mapped read/write, permission fault, walk fault, A/D, TLB flush, and a protection case only if D5 includes PMP. Each fixture would get an independently derived oracle (A10 P0 pattern). The fixture list is not approved.

Non-goals that do not depend on §8: hypervisor/VS/`hgatp`, Linux/SBI boot, multi-hart TLB coherence, instruction-cache or data-cache modeling, performance optimization of the walk path, debug-mode interaction with translation, and any change to A8 atomic semantics. Sv48/Sv57 stay out unless D2 option C is explicitly chosen, which this draft does not do. Public API breaks stay out unless D7 option B or C is explicitly chosen, which this draft does not do.

## 4. Initial workload and oracle contract (proposed, gated on §8)

No fixture below is in the milestone until the named decision is answered. If the chosen option drops a family, that family is out. The oracle shape, if a family is in, follows A10: source/linker/ELF SHA-256, producer identities, exact instruction/retirement/trap counts, and complete effect expectations. Trap-inclusive fixtures report attempts, completed turns, traps, and retirements independently. `ExecutionResult.cycles` includes completed trap turns. No guest writes MINSTRET.

| Family | In only if | Representative fixture (candidate) | Oracle shape |
| --- | --- | --- | --- |
| Bare sanity | D7 keeps a bare default | existing `rv64i/fib.S`, translation off | byte-identical to today's public routes |
| Mapped read/write | D1 and D2 select a translated mode | guest builds page tables, turns translation on, loads and stores | exact translated bytes, neighbors, final PC, retirements |
| Permission fault | D6 selects a fault rule | store to a read-only PTE; fetch from a non-executable PTE | exact cause, original virtual address in `mtval`, no partial store |
| Walk fault | D3 selects a walk rule | invalid PTE, reserved bits, or a bad non-leaf | exact cause and `mtval`, no partial RAM effect |
| A/D observation | D4 selects a policy | fresh page, read then write | the effects that policy names, including a failed write-back |
| TLB flush | D8 selects SFENCE.VMA | mutate a PTE, use the old translation, flush, use the new one | old translation before the flush, new translation after |
| Protection | D5 includes PMP | store that crosses a deny region | fault kind and address from the chosen PMP model |

Whatever is selected has to run on the public routes (real CLI child, native library, flat facade where the fixture is device-independent). Component MMU tests do not prove this milestone. Component presence is not end-to-end support.

## 5. Route and phase matrix (proposed)

Reuse the A10 route matrix. A second configuration axis (`bare` versus the mode D2 selects) exists only after D2 is answered. Until then there is no `sv39` cell. A10 baseline and smoke commands gain no mandatory cells from this draft. Timing of a translated path is not an acceptance criterion. If D4 or D7 later requires PTE writes to be visible, those writes are ordinary commit effects on the existing observation path. No new observation plane.

## 6. Measurement and evidence policy (proposed)

Correctness first, same rule as A10. Each selected fixture, route, and mode cell is checked on every repetition. `perf-test.sh run --profile smoke` grows only by manifest entries for fixtures §8 actually includes. No ratios, no thresholds, no cohort. The A10 certification deferral stays deferred. Exact-head evidence for a future implementation PR would be the full Rust gate, focused new suites, negative controls, and fresh guest builds with the pinned producer audit. This draft has no implementation PR.

## 7. Verification and acceptance (falsifiable, proposed)

These checks apply only after §8 is answered and this draft is revised into a contract. They are not acceptance criteria of the draft.

1. Every fixture the chosen options include passes its oracle on every applicable public route. Each negative mutation fails its validator.
2. If D3 option A is chosen, a driver test shows page-table walk reads on the existing physical port, distinct from guest data accesses. If another D3 option is chosen, the contract states the proof that option requires instead.
3. If D7 keeps a bare default, existing A6-A10 tests and guests stay byte-identical in that configuration, including the A10 baseline floor.
4. If D8 includes SFENCE.VMA, the TLB fixture proves flush behavior on a public route. `Mmu::flush_tlb` unit tests do not.
5. Every fixture-induced fault carries the original virtual access kind and address. No fault is classified `unknown` by omission. A9 quarantine and safety regressions stay green.
6. The compatibility test matches the chosen D7 option.
7. The full local quality gate is green (fmt, check, clippy `-D warnings`, test, doc), and CI Quality-and-tests is green at the exact head.
8. An independent final PR-head review has a recorded outcome. Findings are addressed on the same branch and re-verified.

## 8. Open decisions (none decided)

Every item below is an OPEN DECISION. This draft does not select an option. The line marked **recommended default** is a suggestion for the approval discussion. It is not a decision, and §3 does not implement it. Numbering is D1 through D10.

Roadmap §11 items 1, 2, 3, 4, 6, and 7 were answered by later milestones (A8 atomic profile and host-writer policy, A9 Hart/Machine lifecycle, A10 performance facility, and the still-conditional Stage 7 external-integration gate). They are not reopened here. Roadmap §11 item 5 (Hart/profile boundary: privilege, MMU/PMP, interrupt/WFI, counters) is still open for the virtual-memory part. Interrupt/WFI and counters stay with Stage 5 (D10). The other rows are decisions Stage 4 needs that §11 does not list.

**D1. Privilege levels in v1.** OPEN DECISION. Which privilege modes can a public guest run in and transition between? Roadmap §11 item 5.

- Option A. **Recommended default:** M + S + U, with M as boot/handler mode. Matches existing `PrivilegeMode` and the A6 trap machinery. No new privilege state.
- Option B. M + U only, no S. Smaller trap story. `satp` is an S-mode CSR, so mapped fixtures would have to write it from M.
- Option C. M only, with a test harness writing `satp`. Smallest. Exercises almost no privilege interaction.

**D2. `satp` modes.** OPEN DECISION. Which modes must the public path accept, and when may translation be on?

- Option A. **Recommended default:** Bare (0) and Sv39 (8) only. Any other mode written to `satp` is rejected. Translation changes apply at the next fetch boundary. `Satp::from_satp` already accepts Bare, Sv39, and Sv48 and rejects other modes. Accepting Sv48 in the component is not a public-path commitment.
- Option B. Bare + Sv39, and Sv48 writes are stored but behave as Bare. That reports a mode the core does not implement.
- Option C. Bare + Sv39 + Sv48, with Sv48 implemented. Larger than this milestone. Listed so leaving it out is a choice, not an omission.

**D3. Sv39 page-table walk.** OPEN DECISION. How are walks issued, and how are walk faults classified?

- Option A. **Recommended default:** each walk read is a physical data read on the existing A7/A8 port. A failed walk read is the guest's page fault, with the walk level kept in diagnostics, never a bare `unknown`. Support the Sv39 superpage sizes the encoding allows. A misaligned superpage PTE is a walk fault.
- Option B. Walks stay on the MMU-private `PhysicalMemoryInterface`, which is what the component tests use today. That avoids an adapter and leaves page-table reads off the A7/A8 fault taxonomy. The roadmap says no page-table access bypasses the target fault/unknown distinction.
- Option C. Option A, plus a software-managed single-entry cache of the current walk. That is an optimization. Do not add it unless a fixture needs it.

**D4. A/D bit policy.** OPEN DECISION. Hardware-set, trap-based, or a flag?

- Option A. **Recommended default:** hardware sets Accessed and Dirty on first use. The PTE write-back is a visible physical write. A failed write-back is a physical fault reported to the guest. It is never dropped. This matches `set_accessed` / `set_dirty` and `tests/ad_bits_test.rs`, which are component tests, not public-path proof.
- Option B. Trap when A or D is clear (`PageFaultReason::NotAccessed` / `NotDirty`). The M-mode handler sets the bit. More trap surface for v1.
- Option C. A per-Machine flag choosing A or B. More configurations to test. Add it only if a consumer needs both.

**D5. PMP entries and granularity.** OPEN DECISION. Roadmap Stage 4 requires an explicit PMP decision. Exclusion is one possible answer. It is not the answer yet.

- Option A. **Recommended default:** no PMP enforcement in this milestone. Leave `MmuError::PmpViolation` and `pmp_entries` off the public path. Record the exclusion in the contract non-goals. PMP has no CSRs, no enforcement, and no tests today. A follow-up milestone can add it.
- Option B. Eight entries, NAPOT with 4-byte granularity, configured in M, checked in U and S, not checked in M. Requires `pmpcfg`/`pmpaddr` CSRs, priority and lock rules, and a deny/allow fixture pair.
- Option C. Sixteen entries (the current `MmuConfig` default), every NAPOT form, plus `mseccfg`. Larger than v1.

**D6. Fault and trap behavior, including misaligned access.** OPEN DECISION. How a translated access faults, and what a misaligned access does.

- Option A. **Recommended default:** keep today's alignment check, applied to the translated physical span. An access that splits a page boundary translates each page and faults with the original virtual address. No access completes partially. Page faults use the existing cause codes and `mtval` path. Do not add a new trap channel.
- Option B. Trap on every misaligned access, including accesses today's RAM path accepts. That changes bare guests unless it is limited to translated accesses.
- Option C. Option B for translated accesses only. Bare guests stay on option A. Two policies to test. The bare anchor stays intact.

**D7. Public compatibility.** OPEN DECISION. Roadmap Stage 4 asks for an exact choice between image-base adaptation and an identity/bare configuration. Roadmap §11 item 4 asked the same kind of question for earlier APIs. Those earlier answers do not decide this one.

- Option A. **Recommended default:** no public API change. Translation is opt-in on a new Machine or guest configuration input. Existing constructors and the CLI default stay bare and byte-identical. Image placement stays on the current image-base path until a fixture needs otherwise.
- Option B. A CLI flag such as `--satp-mode sv39`. More public surface. Fixtures can drive the library without it.
- Option C. A new public facade type for translated Machines. More API change than one milestone needs unless option A cannot express the guest configuration.

**D8. TLB invalidation.** OPEN DECISION. Separate from the walk rule in D3 because the component already has `Mmu::flush_tlb` and no public SFENCE.VMA.

- Option A. **Recommended default:** decode SFENCE.VMA and dispatch the existing flush, including the full, per-ASID, and per-VA forms the component already has. One public-route fixture: mutate a PTE, observe the old translation, flush, observe the new one.
- Option B. No SFENCE.VMA in this milestone. Translation tests use a fresh Machine per mapping. Smaller, and it does not prove invalidation.
- Option C. Flush the whole TLB on every `satp` write and skip per-address SFENCE.VMA. Enough for single-guest fixtures, not the spec form.

**D9. SUM, MXR, and bare `mstatus` bits that change translation.** OPEN DECISION. Not required to pick D1, but a permission-fault fixture is ambiguous without it.

- Option A. **Recommended default:** SUM and MXR stay 0. Permission checks use the PTE bits only. Fixtures do not set either bit.
- Option B. Implement SUM and MXR as specified and add one fixture each.
- Option C. Ignore the bits if set. That is a silent behavior change. Listed so it can be rejected explicitly.

**D10. Interrupt, WFI, and counter interaction.** OPEN DECISION. Roadmap §11 item 5 also names interrupt/WFI and counters. This row keeps them out of the virtual-memory milestone unless the user pulls them in.

- Option A. **Recommended default:** no interrupt, WFI, or counter work here. Traps are synchronous page and protection faults only. Stage 5 owns asynchronous events.
- Option B. Deliver timer interrupts while in S-mode. That pulls in Stage 5.
- Option C. Define `mcycle`/`minstret` behavior across a page fault only, with no interrupt delivery. Narrower than B, still a counter decision that A10 left alone.

## 9. Risks and activation boundary

Principal risks, if the milestone is later approved: a page-walk fault classified as `unknown`, a stale TLB after an untested flush, bare-guest behavior drifting, and PMP, Sv48, or interrupts entering without a §8 answer. Any A/D policy that can drop a failed PTE write-back is rejected no matter which D4 option is chosen. That rejection is a constraint on the options, not a choice of option.

Activation boundary: this draft becomes a contract only through the rolling workflow. The user answers D1 through D10, the draft is revised to match those answers, and a separate approval lands. A documentation-only rotation PR then archives A10. A10's own closeout still has a pending independent review. This document authorizes no implementation.
