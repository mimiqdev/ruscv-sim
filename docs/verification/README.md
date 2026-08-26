# Verification Architecture

**Status:** Current

**Authority:** Normative for test-layer ownership; individual tool integrations require their own verified setup

**Last reviewed:** 2026-08-26

## Test layers

```mermaid
flowchart TB
    U["Rust unit tests<br/>Decode / Execute / CSR / MMU / Devices"]
    C["Rust component tests<br/>Subsystem contracts in isolation"]
    I["Rust integration tests<br/>ELF / Runner / Bus / Platform"]
    G["Guest tests<br/>Assembly / C / C++ → RISC-V ELF"]
    A["Architecture and differential tests<br/>External suites / Spike / Sail"]
    S["System tests<br/>Firmware / OS / SystemC / Co-simulation"]

    U --> C --> I --> G --> A --> S
```

Passing a lower layer does not imply a higher layer is integrated. In particular, a component test for an ISA extension, MMU, peripheral, or TLM object is not an end-to-end product-support claim.

## Language policy

Test language follows the layer rather than the simulator implementation language:

| Layer | Expected languages |
| --- | --- |
| Simulator unit/component/integration | Rust, plus property-test and benchmark crates |
| Guest program | RISC-V assembly, C, or C++ |
| External architecture suite | Whatever the upstream suite and toolchain require |
| Orchestration and comparison | Rust, Python, shell, or upstream tooling |
| SystemC/TLM and EDA integration | C++ plus any vendor-required languages |

## Current repository inventory

- Rust tests live under `src/**` and `tests/*.rs`.
- Project-authored guest programs currently live under `tests/bare-metal-riscv-test/` and are assembly sources.
- CI installs a RISC-V GNU assembler/linker, runs all Rust tests, and on pushes to `main` builds and runs the project-authored ELF programs.
- Commit-log comparison helpers exist, but the log currently omits memory-access records in the public ELF run loop.
- No external RISC-V architecture suite is treated as integrated merely because documentation mentions it.

## Verification documents

- [Project-authored bare-metal tests](bare-metal-tests.md)
- [External RISC-V test integration contract](external-riscv-tests.md)
- [Commit tracing and differential testing](commit-tracing.md)

Historical test proposals are retained under [`../archive/designs/`](../archive/designs/).
