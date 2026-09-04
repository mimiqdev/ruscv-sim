# Architecture Principles

**Status:** Current

**Authority:** Normative

**Last reviewed:** 2026-09-04

## Product invariant

The standalone ISS and the Virtual Platform use one RISC-V Hart implementation. Platform integration, timing, tracing, and acceleration may change how that Hart is driven, but must not create a second implementation of architectural semantics.

## Ownership boundaries

| Area | Owns | Must not own |
| --- | --- | --- |
| Hart | Architectural state, fetch/decode/execute, privilege, traps, address translation, retirement | ELF loading, concrete devices, CLI, SystemC |
| Physical access port | Physical transactions and access faults | RISC-V load sign extension, virtual translation, platform policy |
| Platform | Physical address map, RAM/ROM/MMIO, devices, interrupt wiring, platform events | ISA semantics |
| Machine | One Platform plus one or more Harts, composition, and lifecycle | User-interface policy and terminal-result classification |
| Runner | Image loading, limits, ruscv-sim stop taxonomy, result production, observers | Instruction semantics; need not own every outer execution thread |
| Frontend | CLI/API/debug protocol and presentation | Machine-internal behavior |

## Error and event boundaries

The following are distinct and must remain distinguishable:

- Architectural exceptions and interrupts taken by the guest.
- Guest-visible platform events such as MMIO completion or interrupt assertion.
- Platform stop events such as `tohost` termination.
- External debugger or protocol halt and user interruption, distinct from a guest architectural breakpoint trap and from future RISC-V Debug Mode halt.
- Simulator faults caused by invalid internal state or host failures.
- Execution limits used to bound a run.

## Address ownership

Virtual-to-physical translation is part of Hart behavior. Physical address routing is part of the Platform. ELF segment placement is a loader responsibility. An ELF base-address offset is not an architectural translation mechanism.

## Language boundaries

Rust is the implementation language of the current simulator core and native platform components. This is not a repository-wide ban on other languages.

- Assembly, C, and C++ are valid for guest tests and external RISC-V test environments.
- C++ is expected at a SystemC/TLM or EDA integration boundary.
- Python and shell are valid for orchestration, generation, and result analysis.
- A C ABI or another deliberately narrow FFI boundary should separate Rust architectural code from C++ integration code.

No architecture rule may require the Hart semantics to be implemented independently in both Rust and C++.

## Future performance work

Block execution, code translation, DMI, and temporal decoupling are intended future strategies. Current interfaces should not prohibit them, but their implementation is outside the architecture-baseline milestone.
