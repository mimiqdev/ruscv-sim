# A11 — Sv39 integration: approved informational drafting snapshot

**Status:** Current approved snapshot — functional advancement selected by the user on **2026-10-10 UTC**.

**Authority:** Informational drafting record, not a second milestone contract. [docs/dev-plan.md](../dev-plan.md) is the sole normative A11 target, merge-effective on the A10→A11 documentation rotation. Until that merge `main` retains A10. This snapshot does not claim implementation completeness or authorize merge/release.

**Source baseline:** `5d23d337a6732d178a2e14445be9f5c28154f270`, squash-merged A10 PR #72. [A10's complete archive](../archive/milestones/a10-performance-baseline-infrastructure.md) preserves its contract, P3′/P4′ amendment and bounded completion evidence. A10 delivered performance-test infrastructure and an informational single-session baseline, not Sv39 integration or cohort/comparison certification. Deferred performance certification is not inherited A11 work.

## Selected functional direction

Integrate the existing `src/mmu/` Sv39/TLB components into the single Hart execution path, connected by Machine to the current native or flat physical world. The selection deliberately advances functionality rather than performance work. The normative scope, profile choices, staged tests and activation boundary are in the sole contract above; this snapshot preserves the drafting rationale without another acceptance checklist.

- Hart-owned `satp` mode/effective privilege translation for fetch and all admitted data accesses; PTE reads and hardware A/D updates through `PhysicalAccess`, not a separate MMU memory copy.
- ADR-0001/0002 original-access fault mapping: invalid/noncanonical/permission translation conditions are page faults; physical PTE/final-access target failures are access faults. Architectural `mtval` is the original VA; physical/PTE context is diagnostic. Host/protocol/unknown failures remain SimulatorFailure.
- A11's selected hardware A/D profile retains a successful PTE update if the subsequent access faults, without claiming it as a retired instruction effect. Per-Hart TLB and guest `sfence.vma` must satisfy the declared spec-minimal ASID profile; warm-hit permissions, D upgrades and superpage/fence units need tests, not assumptions.
- Opt-in, backward-compatible Machine capability selection; bare remains default with unchanged existing guest/test bytes, expectations and APIs. ELF physical placement and host signature inspection are not virtual translation. Public VM evidence must come from page-table-bearing guests through Machine, not direct component calls or an alternate engine.

Accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md) and [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) retain their ownership/fault/retirement contracts. ADR-0004 implementation (interrupts, WFI, time scheduler) is explicitly out of scope. Also excluded: multi-Hart/DMA, ASIDs beyond spec-minimal, PMP, Sv48/Sv57, new boards/OS, perf certification/optimization and API breaks.

## Entry seams, not delivered VM capability

At the baseline, [`RiscvCore::step_outcome` / `step_transition`](../../src/core/mod.rs) provide architectural outcomes and optional observations, and [`PhysicalAccess`](../../src/physical.rs) provides validated transactions. [`Mmu`](../../src/mmu/mod.rs), [`PageTableWalker`](../../src/mmu/sv39.rs), [`Tlb`](../../src/mmu/tlb.rs) and [Sv39](../../tests/sv39_test.rs)/[TLB](../../tests/tlb_test.rs)/[A-D](../../tests/ad_bits_test.rs) tests are implemented components with simplified/error-collapsing paths. They are **not yet integrated** into public Hart execution. [`MachineConfig`](../../src/machine/mod.rs) currently has no translation seam; CSR SATP storage alone is not VM support.

Repository-only component reproduction:

```bash
cargo test --test sv39_test
cargo test --test tlb_test
cargo test --test ad_bits_test
```

These commands are not new passes or end-to-end claims. Subsequent implementation stages progress from frozen bare inventory → Hart translation/TLB → fault/A-D matrix → opt-in public ELF fixtures → exact-head closeout evidence. Future public tests should use independently derived page tables/signatures, fault-injection spies, bare/identity useful-work equivalence, sfence remapping, A/D combinations and A10-style own-route negative oracles. The full Rust gate and required-toolchain guest exits live only in the normative contract.

This documentation rotation implements none of those stages. Git/PR history records drafting and delivery provenance; approval of functional direction does not imply completed implementation, review of a later implementation head, external compliance or permission to merge.
