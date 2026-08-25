# M1: ISA Foundation

**Status:** Completed (historical record)

**Sprint aliases:** 1–9, including 8.5

## Goal

Build the Rust simulator skeleton and implement the RV64IMAFDC instruction components.

## Recorded outcomes

- Rust project skeleton, build system, CI, and local hooks
- RV64I integer instruction components
- CSR register framework and CSR instructions
- RV64M multiplication and division components
- RV64A LR/SC and AMO components
- Trap and privilege-mode support components
- RV64F and RV64D floating-point components
- RV64C compressed instruction decoder and execution components
- ISA implementation split into `src/isa/` modules

## Current audit note

Completion referred to instruction/component implementation and unit coverage. It did not establish ACT4 conformance. In particular, compressed instructions are not yet integrated into the ELF core loop, which still fetches 32-bit words and advances the PC by four bytes.
