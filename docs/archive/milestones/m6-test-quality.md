# M6: Regression Test Quality

**Status:** Completed (2026-02-05)

**Includes:** M6-extra instruction coverage

## Goal

Strengthen local regression testing around the ELF execution path and implemented instruction components.

## Recorded outcomes

- Reorganized bare-metal programs under `tests/bare-metal-riscv-test/`
- Added RV64I arithmetic, logical, shift, load/store, branch, jump, and CSR programs
- Added RV64M multiplication and division programs
- Recorded 46 passing bare-metal programs: 38 RV64I and 8 RV64M
- Added `cargo-llvm-cov` and Codecov CI integration
- Added `proptest` as a development dependency

## Current audit note

These are project-authored regression programs, not official RISC-V Architectural Certification Tests. Their success does not imply ACT4 conformance.
