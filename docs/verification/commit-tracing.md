# Commit Tracing and Differential Testing

**Status:** Current behavior with known limitations

**Authority:** Informational; source and tests define the exact emitted format

**Last verified:** 2026-08-26

## Public interface

The CLI accepts:

```bash
cargo run -- run program.elf --log-commits commits.log
```

`src/core/commits.rs` emits one Spike-style line containing hart ID, privilege, PC, raw instruction, changed integer registers, and an optional memory-access suffix.

## Known limitation

The public ELF execution loop currently passes no memory-access record to the logger. The data type and formatter support memory accesses, but load/store information is not yet connected end to end. Documentation must not show memory suffixes as guaranteed current CLI output.

## Differential-testing role

Commit logs are an observation surface, not an alternate execution path. A comparison tool should normalize known formatting differences and report the first architectural divergence with enough context to reproduce it.

Historical format proposals and Spike observations are preserved in:

- [`../archive/designs/commit-log-format-plan.md`](../archive/designs/commit-log-format-plan.md)
- [`../archive/reference/spike-log-format-analysis.md`](../archive/reference/spike-log-format-analysis.md)
