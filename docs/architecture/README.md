# ruscv-sim: ISS → Virtual Platform Architecture

**Status:** Current target architecture

**Authority:** Normative for product boundaries; not an implementation-status claim

**Established:** 2026-08-26

**Scope:** Product direction, ownership boundaries, dependency direction, and capability evolution

This document describes the intended product architecture. It does not claim that every depicted component is implemented or integrated. Current capability must be established from the source code and verified tests.

For the corresponding as-is execution path, component wiring, and Current → Target gaps, see [Current Implementation Architecture](current-state.md).

## 1. Product evolution

```mermaid
flowchart LR
    A["Stage A<br/>Architecture Engine"] --> B["Stage B<br/>Verifiable ISS"]
    B --> C["Stage C<br/>Full-System ISS"]
    C --> D["Stage D<br/>Virtual Platform"]
    D --> E["Stage E<br/>High-Performance VP"]

    A --- A1["RV64 ISA<br/>State / Decode / Execute<br/>Trap semantics"]
    B --- B1["ELF / Runner<br/>Compliance / Differential tests<br/>Commit trace / GDB"]
    C --- C1["Privilege / CSR<br/>MMU / PMP / Interrupts<br/>Firmware / OS"]
    D --- D1["Platform composition<br/>TLM / SystemC<br/>Devices / Multi-Hart / Time"]
    E --- E1["Block execution<br/>Code translation<br/>DMI / Temporal decoupling"]
```

## 2. Product system context

```mermaid
flowchart TB
    subgraph Users["Users and automation"]
        DEV["Firmware / OS developers"]
        ARCH["Architecture / model developers"]
        CI["CI / Compliance"]
        TOOL["IDE / Debugger / Scripts"]
    end

    subgraph Product["ruscv-sim product"]
        FRONT["Frontend Layer<br/>CLI / API / GDB / Python"]
        RUNNER["Simulation Runner<br/>Load / Control / Stop reasons"]
        MACHINE["Machine / Platform<br/>one or more Harts + Address space + Devices + Time"]
        OBS["Observability<br/>Commit / Trace / Profile / Events"]
    end

    subgraph Backends["Execution and integration backends"]
        FLAT["Standalone ISS"]
        NATIVE["Native Virtual Platform"]
        TLM["SystemC / TLM Adapter"]
        COSIM["RTL / Emulator Co-simulation"]
    end

    DEV --> FRONT
    ARCH --> FRONT
    CI --> FRONT
    TOOL --> FRONT
    FRONT --> RUNNER --> MACHINE --> OBS
    MACHINE --> FLAT
    MACHINE --> NATIVE
    MACHINE --> TLM --> COSIM
    OBS --> FRONT
```

## 3. Logical layers and dependency direction

```mermaid
flowchart TB
    subgraph L5["L5 — Product interfaces"]
        CLI["CLI"]
        API["Library API"]
        GDB["GDB RSP"]
        AUTO["Automation / Compliance"]
    end

    subgraph L4["L4 — Application orchestration"]
        LOAD["Image / ELF Loader"]
        RUN["Runner"]
        CONTROL["Run Control"]
        REPORT["Result / Report"]
    end

    subgraph L3["L3 — Platform model"]
        MACHINE["Machine"]
        MAP["Address Map / Bus"]
        DEVICES["Devices"]
        IRQ["Interrupt Wiring"]
        TIME["Simulation Time / Scheduler"]
    end

    subgraph L2["L2 — Hart architecture"]
        HART["RISC-V Hart"]
        STATE["Architectural State"]
        ISA["Decode / Execute"]
        TRAP["Trap / Interrupt"]
        MMU["MMU / TLB / PMP"]
        RETIRE["Retirement"]
    end

    subgraph L1["L1 — Stable ports and contracts"]
        PA["PhysicalAccess"]
        IL["InterruptLines"]
        CLOCK["Time / Deadline"]
        EVENTS["Events / Observers"]
    end

    subgraph L0["L0 — Infrastructure and adapters"]
        RAM["Flat RAM"]
        MMIO["Native MMIO Bus"]
        TLM["TLM Initiator Adapter"]
        HOST["Host Services"]
    end

    CLI --> RUN
    API --> RUN
    GDB --> CONTROL
    AUTO --> RUN
    LOAD --> MACHINE
    RUN --> MACHINE
    CONTROL --> MACHINE
    MACHINE --> REPORT
    MACHINE --> HART
    MACHINE --> MAP
    MACHINE --> DEVICES
    MACHINE --> IRQ
    MACHINE --> TIME
    HART --> STATE
    HART --> ISA
    HART --> TRAP
    HART --> MMU
    HART --> RETIRE
    HART --> PA
    IRQ --> IL
    TIME --> CLOCK
    RETIRE --> EVENTS
    DEVICES --> EVENTS
    PA --> RAM
    PA --> MMIO
    PA --> TLM
    DEVICES --> HOST
```

## 4. Hart internal architecture

```mermaid
flowchart TB
    ENTRY["step / run"] --> SAMPLE["Sample InterruptLines"]
    SAMPLE --> PENDING{"Eligible interrupt?"}
    PENDING -- Yes --> INTR["Build interrupt trap"]
    INTR --> TRAP["Trap entry<br/>CSR / Privilege / Target PC"]

    PENDING -- No --> FETCHVA["Instruction virtual address<br/>PC"]
    FETCHVA --> IMMU["Instruction translation<br/>MMU / TLB / PMP"]
    IMMU --> FETCHPA["Physical instruction fetch"]
    FETCHPA --> DECODE["Decode"]
    DECODE --> CHECK["Legality / Extension check"]
    CHECK --> EXEC["Execute semantics"]

    EXEC --> REG["Register / CSR effects"]
    EXEC --> MEMVA["Data virtual address"]
    MEMVA --> DMMU["Data translation<br/>MMU / TLB / PMP"]
    DMMU --> MEMPA["Physical load / store / atomic"]
    MEMPA --> REG

    EXEC --> EXCEPTION{"Synchronous exception?"}
    MEMPA --> EXCEPTION
    EXCEPTION -- Yes --> TRAP
    EXCEPTION -- No --> RETIRE["Retire<br/>x0 / Next PC / Counters"]
    REG --> RETIRE
    RETIRE --> CTRL["Control facts"]
    TRAP --> CTRL
    RETIRE -. optional observation .-> COMMIT["CommitRecord"]
    TRAP -. optional observation .-> TRAPREC["TrapRecord"]
    CTRL --> OUT["Step / quantum control result"]
    COMMIT --> OUT
    TRAPREC --> OUT
```

## 5. Address, memory, and TLM boundary

```mermaid
flowchart LR
    subgraph Hart["Hart: architectural semantics"]
        VA["Virtual address"]
        ALIGN["Alignment / Access rules"]
        XLATE["MMU / TLB / PMP"]
        PA["Physical address"]
        REQ["AccessRequest<br/>Fetch / Read / Write / Atomic"]
    end

    subgraph Contract["Stable port"]
        PORT["PhysicalAccess"]
        RESP["AccessResponse<br/>Data / Fault / Delay"]
    end

    subgraph Platform["Platform address space"]
        ROUTER["Mapped bus / Router"]
        RAM["RAM"]
        ROM["ROM / Flash"]
        UART["UART"]
        INTC["PLIC / CLINT"]
        HOSTDEV["HTIF / VirtIO / Host device"]
    end

    subgraph Future["Future transport implementation"]
        TLMAD["TLM Adapter"]
        BTRAN["b_transport"]
        DMI["DMI fast path"]
        SYSC["SystemC Platform"]
    end

    VA --> ALIGN --> XLATE --> PA --> REQ --> PORT
    PORT --> ROUTER
    ROUTER --> RAM
    ROUTER --> ROM
    ROUTER --> UART
    ROUTER --> INTC
    ROUTER --> HOSTDEV
    PORT -. Replaceable backend .-> TLMAD
    TLMAD --> BTRAN --> SYSC
    TLMAD --> DMI --> SYSC
    RAM --> RESP
    ROM --> RESP
    UART --> RESP
    INTC --> RESP
    HOSTDEV --> RESP
    SYSC --> RESP
    RESP --> PORT
```

## 6. One execution engine, multiple product forms

```mermaid
flowchart TB
    subgraph Shared["Single shared architectural implementation"]
        SEM["Hart semantics"]
        STATE["Architectural state"]
        TRAP["Trap / Privilege / MMU"]
        CONTRACT["PhysicalAccess contract"]
    end

    subgraph ISS["Standalone ISS"]
        ISSRUN["Single-Hart Runner"]
        FLATBUS["Flat / Native Bus"]
        ELF["ELF + Compliance"]
        DEBUG["Commit Trace / GDB"]
    end

    subgraph VP["Virtual Platform"]
        VPSCHED["Platform Scheduler"]
        MULTI["Multi-Hart Machine"]
        VPBUS["TLM / Native Platform Bus"]
        PERIPH["Peripheral Models"]
        VPTIME["Virtual Time"]
    end

    subgraph Integration["System integration"]
        SYSTEMC["SystemC"]
        RTL["RTL Emulator"]
        EXT["External IP Models"]
    end

    ELF --> ISSRUN
    DEBUG --> ISSRUN
    ISSRUN --> SEM
    SEM --> STATE
    SEM --> TRAP
    SEM --> CONTRACT
    CONTRACT --> FLATBUS
    VPSCHED --> MULTI
    MULTI --> SEM
    CONTRACT --> VPBUS
    VPSCHED --> VPTIME
    VPBUS --> PERIPH
    VPBUS --> SYSTEMC
    SYSTEMC --> RTL
    SYSTEMC --> EXT
```

A Machine is one Platform plus one or more Harts; N=1 is the ISS baseline. Native VP scheduling is Machine-associated. SystemC, HDL, or other co-simulation may own the outer execution thread while the same Hart/Platform semantics and ruscv-sim result taxonomy remain in force.

## 7. Runtime control, time, and events

```mermaid
sequenceDiagram
    participant F as Frontend
    participant R as Runner
    participant S as Scheduler
    participant H as Hart
    participant B as PhysicalAccess
    participant D as Device
    participant O as Observer

    F->>R: run(image, limits, options)
    R->>S: grant(machine, budget, deadline, control)

    loop Until a stop condition
        S->>H: exchange(grant: budget, deadline, control, observations)
        H->>B: fetch / load / store
        B->>D: MMIO transaction
        D-->>B: data / fault / delay / event
        B-->>H: AccessResponse
        H-->>O: optional Commit / Trap records
        H-->>S: ordered facts + progress + consumed time
        S->>D: advance_to(next boundary)
        D-->>S: interrupt / timer / platform event
    end

    S-->>R: unclassified co-incident facts
    R-->>F: classified ExecutionResult
```

This sequence is the Runner-driven ISS/native path. [ADR-0004](decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md) defines the exchange, pre-fetch interrupt sample, modeled-time and delay accounting, WFI/idle boundary, and fact ordering shown here. In external-kernel hosting the kernel grants time into the Machine; the Runner still classifies non-lossy facts and does not have to own that outer thread. Observation records are subscriber-gated; control facts are always returned.

## 8. Capability accumulation and architecture gates

```mermaid
flowchart TB
    P0["P0 — Architecture skeleton"]
    P1["P1 — Verifiable RV64 ISS"]
    P2["P2 — Complete Hart"]
    P3["P3 — Full-System Machine"]
    P4["P4 — Virtual Platform"]
    P5["P5 — High-performance execution"]

    P0 --> P1 --> P2 --> P3 --> P4 --> P5

    P0 --- C0["Stable boundaries<br/>Hart / Platform / Runner / Ports"]
    P1 --- C1["ISA correctness<br/>ELF / Compliance / Differential / Trace"]
    P2 --- C2["Privilege / CSR / Trap<br/>MMU / PMP / Interrupt"]
    P3 --- C3["Firmware / OS boot<br/>PLIC / CLINT / UART / VirtIO<br/>Multi-Hart"]
    P4 --- C4["SystemC / TLM<br/>Platform composition / External IP<br/>Virtual time / Co-simulation"]
    P5 --- C5["Decoded block cache<br/>Code translation<br/>DMI / Quantum"]

    G0{"Architecture boundaries approved"} --> P1
    P1 --> G1{"ISA verification gate"}
    G1 --> P2
    P2 --> G2{"Architectural-state closure"}
    G2 --> P3
    P3 --> G3{"OS boot and platform observability"}
    G3 --> P4
    P4 --> G4{"Stable TLM integration semantics"}
    G4 --> P5
```
