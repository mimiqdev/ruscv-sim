# Code-Generation Component Status

**Status:** Current implementation note

**Authority:** Informational

**Last verified:** 2026-08-26

The repository contains two code-generation experiments:

- `src/codegen/template.rs` defines R-type and I-type encoding templates and tests their encodings.
- `ruscv-macros/` defines procedural-macro experiments for instruction structs.

These components are not the active decode/execute dispatcher. Some procedural-macro expansions reference APIs that are not part of the current `RiscvCore`, and the batch/set macros are placeholders. Their presence must not be interpreted as generated ISA coverage.

Any future use should begin with an architecture decision defining the generated source of truth, diagnostics, reviewability, and equivalence tests. The earlier roadmap is preserved in [`../archive/designs/code-generation-legacy.md`](../archive/designs/code-generation-legacy.md).
