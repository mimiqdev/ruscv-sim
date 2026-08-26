# SystemC/TLM Boundary

**Status:** Target integration design

**Authority:** Architectural constraint, not an implementation claim

**Last reviewed:** 2026-08-26

## Intended mapping

| ruscv-sim concept | SystemC/TLM concept |
| --- | --- |
| Physical access request | Generic payload transaction |
| Blocking functional access | `b_transport` |
| Access delay | Annotated `sc_time` |
| Direct RAM fast path | DMI region and invalidation |
| Interrupt line state | Signal, callback, or explicit bridge API |
| Platform deadline | Scheduler synchronization point |

The initial integration should use blocking transport. Non-blocking transport requires suspended instruction state, payload lifetime management, and scheduler callbacks; it is a later timing-model decision.

## Language and ownership

SystemC and standard TLM sockets remain in C++. The Rust side exposes a narrow lifecycle and transaction facade. Neither `tlm::tlm_generic_payload` nor `sc_time` belongs in ISA instruction functions.

## Required future decisions

- FFI ownership and thread model.
- Error and exception mapping across the boundary.
- Time unit, quantum, and synchronization semantics.
- DMI lifetime and invalidation.
- Callback reentrancy and simulator stop behavior.
- Build, packaging, and supported SystemC versions.
