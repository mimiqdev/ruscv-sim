# Execution Performance Directions

**Status:** Research

**Authority:** Informational; deferred until the architectural execution boundary is stable

## Candidate progression

```text
Instruction interpreter
    → decoded basic-block cache
    → block execution
    → host code translation
```

Independent data-path optimizations include direct RAM access, DMI, software TLBs, reduced observer overhead, and temporal decoupling from the platform scheduler.

## Constraints worth preserving now

- The Runner should eventually permit execution to a budget or deadline, not require an external callback after every instruction.
- Debug and trace must be optional observation paths rather than mandatory per-instruction allocation.
- Physical access must distinguish ordinary RAM from MMIO without exposing platform implementation types to the Hart.
- `FENCE.I`, self-modifying code, address-space changes, breakpoints, and privilege changes must be able to invalidate or terminate translated blocks.
- Optimization must preserve the same architectural semantics and verification path as the interpreter.

No acceleration mechanism is selected by the current milestone.
