# Integration Boundaries

**Status:** Current target boundary

**Authority:** Normative

**Last reviewed:** 2026-08-26

## Principle

External integration must adapt to stable simulator ports. It must not inject SystemC, vendor APIs, or test-framework types into Hart semantics.

```mermaid
flowchart LR
    subgraph Rust["Rust product core"]
        H["Hart"]
        M["Machine / Platform"]
        P["Stable ports<br/>PhysicalAccess / Interrupts / Events"]
        H --> P
        M --> P
    end

    subgraph Boundary["Narrow integration boundary"]
        CABI["C ABI / FFI facade"]
        ADAPTER["Protocol adapter"]
    end

    subgraph External["External ecosystems"]
        TEST["C/C++ test harness"]
        SYSTEMC["C++ SystemC / TLM"]
        EDA["EDA / RTL co-simulation"]
    end

    P --> CABI --> ADAPTER
    ADAPTER --> TEST
    ADAPTER --> SYSTEMC --> EDA
```

## Integration categories

- Validation toolchains build guest ELFs and drive the public Runner.
- Native platform components implement Rust ports directly.
- SystemC/TLM integration translates physical transactions, interrupts, time, and events at an FFI boundary.
- Debuggers and automation use frontend APIs and observation events.

See [SystemC/TLM boundary](systemc-tlm.md).
