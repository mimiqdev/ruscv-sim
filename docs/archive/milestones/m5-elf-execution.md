# M5: ELF Execution Loop

**Status:** Completed (2026-02-03)

## Goal

Create the minimum load-run-stop loop needed to execute bare-metal RISC-V ELF programs.

## Recorded outcomes

- ELF64 loader with entry-point and load-segment handling
- Core reset and instruction execution loop
- Signature-section discovery and in-memory extraction
- `tohost` section and symbol discovery
- HTIF-style exit detection
- System bus with RAM and UART routing
- RAM at the ELF-defined base and UART at `0x1000_0000`
- `dyn MemoryInterface` support in the core

## Current audit note

This milestone established the prerequisite execution path for architecture testing, not architecture-test integration itself. The latest ACT4 framework uses self-checking ELFs, so the signature extraction API is now mainly useful for diagnostics and compatibility with older flows.
