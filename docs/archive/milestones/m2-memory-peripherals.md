# M2: Memory and Peripherals

**Status:** Completed (historical record)

## Goal

Implement the Sv39 memory-management components and a Rust TLM-style peripheral framework.

## Recorded outcomes

- Sv39 page-table walking
- 64-entry, four-way set-associative TLB with LRU-style replacement
- Accessed and Dirty bit handling
- TLM payload, timing, initiator, target, bus, bridge, and memory abstractions
- CLINT model
- PLIC model
- UART 16550 model
- Platform configuration helpers

## Current audit note

These components are implemented and tested independently. The active ELF execution path does not yet route instruction fetches and data accesses through the MMU, and the optional TLM interface is not used by `RiscvCore::step`.
