# M4: Debug Support

**Status:** Completed (2026-02-02)

## Goal

Implement reusable debugging components for the simulator.

## Recorded outcomes

- GDB Remote Serial Protocol server components
- Interactive CLI debugger components
- Software and hardware breakpoint management
- Read, write, and access watchpoint management
- Version-controlled Git hooks

## Current audit note

The debugging components are exported by the library, but the main `ruscv-sim` CLI currently exposes only the ELF `run` command. End-to-end CLI/GDB integration was not verified as part of the later ACT4 work.
