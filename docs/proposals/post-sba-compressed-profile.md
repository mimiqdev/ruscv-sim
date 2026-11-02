# Zca compressed-instruction profile proposal

**Status:** Current approved proposal entry. Approved 2026-11-03 under the owner's explicit delegation of full planning authority to the maintaining agent; the recommended D1–D10 scope was selected.

**Authority:** Informational navigation. This is not a second current contract or an implementation claim.

The approved scope is maintained in the [Selectable Zca Compressed-Instruction Profile contract](../dev-plan.md), activated on the System Bus Access Debug successor rotation merge. The [archived SBA contract](../archive/milestones/system-bus-access-debug.md) preserves the preceding scope.

## Selection rationale

With the program-buffer and SBA increments complete, the recorded debug direction of the [post-A7 roadmap](post-a7-roadmap.md) is finished. Its remaining stages (TLM/SystemC transport and multi-hart/DMA) each require a concrete consumer and fixture, and performance certification remains deferred with no consumer. The largest gap that ordinary guests hit is the instruction profile: fetch is fixed at IALIGN=32, the separate `src/isa/rv64c` component is not on the active path and has RV64 mapping defects, and `misa` advertises no C, so default toolchain output does not run. The pinned ACT4 revision already provides 32 RV64 Zca sources for external verification.

Compressed support is a selectable profile because the A10 `public-v1` witness `s4_csr_fields`, several debug fixtures and the frozen RV64I selection pin the IMA `misa` and IALIGN=32 masks. A default-unchanged `rv64ima` profile keeps all existing evidence valid without an A10 change.

Execution and review outcomes remain in CI/PR; this entry retains no result record.
