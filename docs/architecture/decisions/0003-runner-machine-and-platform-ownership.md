# ADR-0003: Runner, Machine, and Platform ownership

| Field | Value |
| --- | --- |
| Status | Proposed |
| Authority | Draft contract; normative only after acceptance |
| Date | 2026-09-03 |
| Owner | Runtime and composition architecture |
| Tracking | [MMQ-6](https://linear.app/mrtoniliu/issue/MMQ-6/adr-runner-machine-and-platform-ownership) |
| Related decisions | [ADR-0001](0001-hart-execution-outcome-and-observation.md), [ADR-0002](0002-physical-access-transaction-and-fault.md), MMQ-10 (interrupt, time, and stop-event boundaries) |
| Supersedes | None |

> This is a documentation-only decision. It defines semantic ownership and dependency
> direction; it does not select Rust layouts, constructors, trait signatures, callback
> shapes, serialization, scheduling algorithms, address maps, transport APIs, or a
> migration sequence. It does not claim that the target boundaries are implemented.

## Context

The product must provide a standalone instruction set simulator (ISS) now and a
composable Virtual Platform (VP) later without implementing RISC-V architectural
semantics twice. The [target architecture](../README.md) and
[architecture principles](../principles.md) establish one Hart, Hart-owned
architectural semantics, Platform-owned physical routing, Machine-owned
composition, and Runner-owned application orchestration. The
[current implementation inventory](../current-state.md) records that the public
path does not yet have those boundaries.

Today, the public path in [`src/executor.rs`](../../../src/executor.rs) concentrates
ELF loading, RAM/UART/HTIF construction, the instruction loop, cycle limits,
commit-log delivery, signature extraction, tohost polling, and
`ExecutionResult` construction in `load_and_run`. `SystemBus` is a concrete
RAM/UART/HTIF switch in that same module. The public CLI in
[`src/main.rs`](../../../src/main.rs) calls `load_and_run_file` and formats the
result. The exported `RiscVSimulator` in `executor.rs` has a second, simpler
flat-memory loop with different address and device behavior. This is useful
current implementation evidence, but it must not become two architectural
execution engines.

The existing component boundaries are also not yet composition boundaries:

- [`src/core/mod.rs`](../../../src/core/mod.rs) provides `RiscvCore`, but its
  `step` currently returns a generic result, performs base-address adaptation, and
  does not integrate the trap handler or interrupt inputs.
- [`src/elf.rs`](../../../src/elf.rs) validates ELF64/RISC-V input, flattens
  loadable segments relative to their lowest virtual address, and discovers the
  entry point, signature section, and tohost section or symbol. It currently
  returns a memory buffer rather than a transport-neutral image-placement
  contract.
- [`src/memory/mod.rs`](../../../src/memory/mod.rs) combines typed storage
  operations with RISC-V sign/zero-extension helpers. That is current API
  evidence, not the target physical-access boundary.
- [`src/peripherals/uart16550.rs`](../../../src/peripherals/uart16550.rs),
  [`src/peripherals/clint.rs`](../../../src/peripherals/clint.rs), and
  [`src/peripherals/plic.rs`](../../../src/peripherals/plic.rs) implement device
  behavior and focused TLM tests, but the public `SystemBus` only wires a limited
  UART and embedded HTIF; CLINT and PLIC are not on that public path.
- [`src/debug/mod.rs`](../../../src/debug/mod.rs), `debug/cli.rs`, and
  `debug/gdb_server.rs` define protocol and mock-target-facing control surfaces,
  but no production debugger target is connected to the public execution loop.

ADR-0001 defines the semantic Hart outcomes and observation ownership consumed by
this record: a completed step yields `InstructionRetired`, `TrapEntered`, or
`SimulatorFailure`, with Hart-owned commit/trap facts. ADR-0002 defines the one
transport-neutral `PhysicalAccess` boundary consumed here: all physical accesses
use the same raw-byte transaction vocabulary, and physical target faults remain
distinct from simulator/adapter failures. MMQ-10 will define the detailed
interrupt, time, and stop-event arbitration that this record deliberately bounds.

## Decision

### 1. Vocabulary and dependency direction

The terms in this ADR have semantic meanings independent of any particular Rust
module or public API:

- **Hart** is the single architectural engine. It owns architectural state,
  instruction semantics, privilege, traps, address translation, retirement, and
  per-Hart architectural reservation state as defined by the approved Hart and
  PhysicalAccess decisions.
- **Platform** is the physical world visible to the Hart. It owns physical
  address routing, memory and device targets, host-facing device behavior,
  interrupt sources and wiring endpoints, and platform events. It never owns
  RISC-V instruction semantics or virtual-address translation.
- **Machine** is the composition and lifecycle boundary for one Hart and one
  Platform. It owns their association, port connections, coherent access to the
  composed state, and lifecycle transitions. It is not a user-interface or
  run-policy object.
- **Runner** is the application run-control boundary. It owns image-loading
  orchestration, run options, outer limits, observer delivery, stop handling,
  aggregation of Hart outcomes and Platform events, inspection/report assembly,
  and the decision to return a run result. It never implements an instruction.
- **Frontend** is the CLI, library-facing application layer, debugger protocol,
  or other presentation/control entry point. It translates user or automation
  requests and presents results; it does not reach through the Runner to mutate
  Hart or Platform internals.

The required direction is:

```text
Frontend  ->  Runner  ->  Machine  ->  { Hart, Platform }
                                  Hart  ->  PhysicalAccess (semantic port)
                                  Platform -> PhysicalAccess implementation
                                  Platform -> devices / host services / event sources
```

The arrow from Hart to `PhysicalAccess` means that Hart semantics issue physical
transactions through the contract; it does not mean that Hart depends on a
concrete bus or device. The Platform supplies the implementation behind that
port. Machine connects the two. Platform events flow back through Machine to the
Runner as semantic observations, never as a direct dependency from a device to a
CLI or Runner policy.

The composition boundary may connect additional approved ports—such as interrupt
lines, observation delivery, and time/deadline information—without changing this
direction. Machine owns the association of any time/scheduling service with the
Hart and Platform; Runner requests bounded execution and control, while MMQ-10
defines the service's detailed semantics. A port is a contract, not a commitment
to a Rust trait, callback, channel, or transport representation.

| Concern | Primary owner | Boundary rule |
| --- | --- | --- |
| CLI/API/debug protocol and presentation | Frontend | Converts requests/results; does not own machine state or instruction semantics. |
| Image parsing and load-image description | ELF/image loader, invoked by Runner | Parses and validates the file and describes segments/metadata; does not mutate a Machine or translate a guest virtual address. |
| Run configuration and outer control | Runner | Chooses the run configuration, limits, observer sinks, and requested control operation; does not inspect concrete devices directly. |
| Hart + Platform composition | Machine | Creates or obtains the configured parts, connects their ports, and exposes the composed lifecycle. |
| Architectural execution | Hart | Owns fetch/decode/execute, translation, traps, retirement, and Hart observations. |
| Physical map and targets | Platform | Routes physical requests to RAM/ROM/MMIO or an external target and reports platform events. |
| Device behavior and host interaction | Platform and its devices | Implements UART, HTIF/tohost, interrupt sources, and other target-local behavior; does not decide the application run result. |
| Observer facts | Hart for architectural facts; Platform for platform facts | Hart emits ADR-0001 records; Platform emits platform events. Runner delivers/aggregates them. |
| Run-level result | Runner | Combines distinct terminal/non-terminal facts and captures the requested inspection; Frontend formats it. |

No layer below the Runner may depend on `src/main.rs`, CLI flags, a concrete
`ExecutionResult`, or presentation text. No Hart implementation may depend on
ELF parsing, CLI policy, concrete UART/HTIF types, SystemC/TLM mechanics, or a
specific Runner. The existing modules are mapped to these roles in §10 without
claiming that the roles have already been separated in code.

### 2. Machine composition boundary

A Machine represents one composed execution context. For the baseline in this
ADR it contains one shared Hart and one Platform; it is not a second Hart or a
second instruction dispatcher. A future VP may use the same boundary with a
richer Platform and scheduler, but multi-Hart product behavior is outside this
record's scope.

At composition time, the Machine is responsible for:

1. establishing one Hart instance using the selected architectural profile;
2. establishing one Platform instance and its configured physical targets;
3. establishing the Hart's `PhysicalAccess` connection to that Platform;
4. connecting interrupt inputs, observation paths, and any approved time or
   deadline ports without making the Hart aware of their concrete producers;
5. connecting device outputs to Platform event collection and host services; and
6. making the composed state available through a coherent lifecycle/inspection
   boundary.

The Machine may be created or selected by a Runner configuration, but the Runner
does not construct a second core, reach into a device, or attach a special
memory path for one product form. The Machine owns composition wiring; the Runner
supplies external observer sinks and run policy at that boundary. Hart observers
receive the immutable `CommitRecord` and `TrapRecord` facts defined by ADR-0001;
Platform observers receive normalized platform events. Neither observer type may
execute a callback in the middle of an architectural transition.

The Machine is therefore the only owner of the association between this Hart and
this Platform. A library convenience wrapper, debugger target, native bus, or
future TLM adapter may be an adapter around the Machine, but it must not create an
independent architectural loop.

### 3. Lifecycle and ownership

The lifecycle is semantic rather than an API prescription. A normal run has the
following phases:

```text
construct/configure -> install image -> reset -> run/control -> inspect -> teardown
```

#### Construction and configuration

The Machine composition root establishes the Hart, Platform, device instances,
physical-access connection, interrupt/event connections, and observer wiring.
The Hart owns the meaning of its architectural reset state. The Platform owns the
meaning of reset for each target/device. The Machine invokes and orders those
resets so that the composition presents one coherent starting point. The Runner
owns run options and creates/configures observer sinks, but does not own device
internals.

Static configuration includes the selected architectural profile, platform
capabilities, target topology, and any address-map choice supplied by the
configuration. These are preserved across a reset. This ADR does not select any
concrete profile, topology, address map, backend, constructor, ownership model,
or lifetime representation.

#### Image installation and placement

Installing an image is separate from resetting a running Machine. The Runner
obtains a load-image description from the ELF/image loader and asks the Machine
to install it. The Machine owns the placement operation as part of its
composition lifecycle; the Platform owns the physical writes, target checks, and
routing that make placement visible. The Hart is not involved in host-side image
installation and must not execute synthetic stores for it.

An installed image establishes the entry metadata and the initial contents against
which a fresh run is defined. Replacing an image or changing composition is a
separate lifecycle operation, not an implicit side effect of a Hart step. The
concrete operation used to replace it—reconfigure, rebuild, or another mechanism—
is intentionally not selected here.

#### Reset

Machine reset is a coordinated semantic boundary. Unless a caller explicitly
requests a new composition or image installation, reset must:

- **preserve** static Machine configuration, the Hart/Platform topology, the
  installed image description and image-owned initial contents, and the
  observer connection topology;
- **restore** the Hart to the selected profile's reset state and configured entry
  point, including architectural privilege/CSR state and the architectural PC;
- **clear or restore** Hart transient state that must not leak between runs,
  including translation caches, reservations, pending trap/interrupt sampling
  state, and other profile-defined execution state;
- **clear or restore** Platform dynamic state, including device FIFOs/register
  state, pending/claimed interrupt state, pending host-exit state, and queued
  platform events, according to each device's reset semantics;
- **restore** image-owned mutable storage (including zero-filled portions of load
  segments) to the installed initial image when the lifecycle promises a fresh
  rerun; a backend that cannot do so must not silently advertise a fresh reset
  over stale state; and
- **leave** observer sinks connected while resetting per-run counters or
  sequence state in the Runner as an observation concern.

Reset does not change an address map, perform virtual-to-physical translation,
reparse an ELF file, or report a run result. Exact Hart reset values and exact
device reset behavior follow their approved architectural/device contracts; the
semantic requirement is that no prior run's dynamic state is accidentally used as
the initial state of the next run.

#### Run, stop, and teardown

The Runner starts and controls a run through the Machine. The Runner may request
single-step or a bounded run, but the Machine is the only composition boundary
that drives its Hart against its Platform. The Hart performs the architectural
step; the Platform services physical transactions and produces platform events;
the Machine returns the resulting facts to the Runner. A batch or accelerated
execution strategy must preserve the per-instruction outcome/observation
semantics of ADR-0001.

Stop requests are handled at an approved architectural boundary. A Runner stop
request, debugger request, limit, Platform exit, Hart trap, or simulator failure
must remain distinguishable in the run-level result. The Runner owns the policy
of whether a non-terminal Hart outcome (for example, a trap that can be resumed)
continues the run, while MMQ-10 owns the precise arbitration when multiple stop
conditions are eligible at the same boundary.

At teardown the Runner stops accepting control, completes or reports observer
handling according to its run policy, and releases its run-level sinks. The
Machine quiesces the composition and disconnects or releases the Hart, Platform,
device, host-service, and event resources it owns. The Platform must not emit new
run events after the Machine has completed teardown. Whether a host transport
needs an adapter-specific shutdown sequence is an implementation detail, not a
new ownership boundary.

### 4. ELF parsing, image placement, and address meaning

The ELF/image loader and the Machine/Platform have separate responsibilities:

| Operation | Owner | Semantic result |
| --- | --- | --- |
| Validate ELF identity and supported input | ELF/image loader | Accept/reject the input according to the selected loader profile. |
| Read PT_LOAD contents and represent zero-fill | ELF/image loader | Produce a load-image description containing guest segment address/permission metadata and bytes, without mutating runtime state. |
| Discover entry, `.signature`, and `.tohost`/`tohost` symbol metadata | ELF/image loader | Preserve metadata for composition and later inspection/event setup. |
| Select how an image is installed in the configured Machine | Runner configuration plus Machine | Apply an image-placement policy without changing Hart semantics. |
| Write bytes to physical targets and enforce the physical map | Platform through its physical-placement boundary | Make the installed image visible in the Platform address space or report a placement failure. |
| Translate a guest virtual address during execution | Hart/MMU | Apply the architectural translation rules and issue a physical transaction through ADR-0002's port. |

The loader's current flattened buffer is an implementation convenience: it stores
segments relative to the lowest load address and returns the original entry and
metadata. A future load-image description may preserve more information, but its
semantic boundary remains the same.

An image-base offset or other storage adaptation is **not** virtual-to-physical
translation. It may describe how file/image addresses correspond to storage
locations in a flat backend, and it may be needed to reproduce the current public
ELF flow. It must not modify the Hart's virtual address, bypass page-table or
profile translation, turn a host offset into a guest mapping, or make the Platform
perform Hart permission checks. If a configured profile uses identity mapping,
that is an architectural/platform configuration and not evidence that the loader
performed translation.

The image entry point remains guest architectural metadata. The Machine must
establish a starting Hart PC consistent with the configured image-placement and
addressing contract; an internal storage offset cannot be mistaken for a
replacement entry address. Segment permissions and any `p_vaddr`/`p_paddr`
interpretation are likewise image/platform policy, not a new Hart API decision.

### 5. Hart driving and observation ownership

The Hart boundary is the one defined by ADR-0001 and is consumed without
redefinition here:

- `InstructionRetired` means one instruction completed and produced one
  Hart-owned commit observation;
- `TrapEntered` means architectural trap entry completed and no instruction
  retired in that step; and
- `SimulatorFailure` means architectural completion was not possible and no
  fabricated commit or trap is emitted.

The Machine drives the Hart only as part of its composition. The Runner consumes
these outcomes and must not recreate them by comparing register snapshots,
re-fetching opcodes, interpreting generic memory errors, or polling a second
execution loop. In particular, the current `load_and_run` commit logger's
pre/post register snapshot and opcode re-fetch are compatibility-era behavior,
not the target observation ownership.

ADR-0002 is consumed at the Machine/Platform connection:

- all Hart physical fetches, data accesses, atomics, and page-table walks use the
  one `PhysicalAccess` contract;
- the Platform routes physical addresses and returns raw bytes or normalized
  physical/target failures; it does not sign-extend loads, perform virtual
  translation, or enter traps;
- the Hart maps a valid physical target fault to the architectural access-fault
  outcome using the original access kind, while malformed/host/adapter failures
  remain simulator failures; and
- atomic operation envelopes and reservation visibility remain below the
  composition boundary without moving Hart reservation ownership into a device.

A TLM/native bus, flat memory, or external model can implement the Platform side
of this connection. It is not an alternate Hart execution path.

### 6. Platform events and successful host exit

Platform events originate in Platform-owned targets and event sources. Examples
include:

- a successful write to an HTIF/`tohost` endpoint that represents a host-exit
  request;
- UART output or input availability and device-generated interrupt assertions;
- CLINT/PLIC or other device state transitions; and
- future device, external-model, or scheduler events.

A device owns its local side effects and reports a semantic event to its Platform.
The Platform normalizes and retains event provenance; the Machine associates
Platform events with the composed run; and the Runner decides whether an event is
a terminal run condition and how it is represented in the result. A device or
Platform must not call `std::process::exit`, construct a CLI result, or apply
Runner stop priority. A debugger request is an external control event, not a
Platform event.

A successful guest write to HTIF/`tohost` has an explicit ordering guarantee:

1. The Hart issues the physical write through `PhysicalAccess` and the Platform
   completes the target/device transaction.
2. The Platform may create a host-exit event as a consequence of that successful
   write, but the event is held as a run observation until the current Hart step
   reaches its boundary.
3. The Hart completes the instruction and emits `InstructionRetired` with its
   commit facts, as required by ADR-0001. A successful platform write does not
   turn the writing instruction into a trap or an unretired instruction.
4. The Machine forwards the completed Hart outcome and the causal Platform event
   to the Runner. Only then may the Runner report Platform exit as the outer
   terminal result.

If the physical write fails, no successful-exit event is reported. The Hart
receives the ADR-0002 result and produces the corresponding architectural trap
(or simulator failure for an adapter/host failure); the Runner must not infer an
exit from a failed write. This ordering also applies when a compatibility backend
uses a callback internally: callback delivery may be an implementation detail,
but outer exit visibility cannot precede completed Hart retirement.

The Platform may expose an exit event carrying a decoded exit code and raw/device
context. Whether decoding is implemented in the device, a Platform adapter, or
another lower layer is not selected here; the semantic owner of host interaction
is Platform, and the semantic owner of terminating the run is Runner.

### 7. Runner result, limits, and inspection

The Runner owns a run-level result assembled from facts supplied by the Machine.
The result is conceptually a record with distinct dimensions, not a requirement
to expose a particular enum or struct:

- terminal reason category, preserving architectural trap, Platform exit,
  debugger stop, execution limit, scheduler/time stop, observer/reporting
  failure, and simulator failure as distinguishable causes;
- guest/platform exit code where one exists;
- completed-step/instruction-cycle accounting and, when enabled by later
  contracts, consumed virtual time or deadline information;
- final coherent Machine inspection (Hart architectural state plus requested
  Platform/device inspection); and
- optional image metadata and post-run artifacts such as signature address/data,
  commit/trace diagnostics, and failure context.

The Runner decides when to take the final inspection and how to include partial
state on a failure. The Machine owns the coherent inspection boundary: a
register, memory, device, or Platform read requested by a Runner or debugger
must go through the appropriate composed control/inspection path rather than
reaching into a concrete `SimpleMemory`, UART, or bus. A debug register write,
memory write, continue, single-step, or stop request is similarly routed through
Machine control; it must not synthesize a retirement record or bypass Platform
routing. The debugger protocol and breakpoint/watchpoint managers remain
Frontend/control adapters, while Runner owns the resulting run decision.

Signature extraction is a host-side inspection operation. The ELF loader supplies
signature metadata; the Machine records the installed image correspondence; and
the Runner reads the post-run bytes through the composed inspection path and
places them in the run result. Signature extraction does not execute a guest
load, invoke Hart virtual translation, or increment the run count. If the image
has no signature metadata, the result has no signature artifact; a zero-length
region is an empty artifact. The byte order and address meaning required for the
current CLI are compatibility constraints in §9, not a new serialization or Rust
API decision.

Limits are Runner-owned outer controls. The Runner may stop after a configured
number of completed Hart steps, a deadline, or another bound and reports a limit
rather than a Hart trap. The exact meaning of a cycle versus an instruction,
physical delay consumption, virtual-time advancement, deadline sampling, and
whether a limit wins against another event are explicitly deferred to MMQ-10.
The current CLI instruction-count behavior is nevertheless preserved as a
compatibility constraint while these decisions are made.

### 8. Standalone ISS and future VP configurations

The standalone ISS and future VP are configurations of the same boundaries:

| Product form | Runner configuration | Machine/Hart/Platform composition |
| --- | --- | --- |
| Standalone ISS | ELF-oriented run control, deterministic single-Hart limit, optional commit log/debug inspection, and current host-exit compatibility | One Hart, one minimal native/flat Platform implementing PhysicalAccess, and only the platform services required by the current CLI. |
| Future VP around one Hart | Same Runner/Machine contracts with richer event/time control and an interchangeable Platform backend | The same one Hart implementation and architectural outcomes connected to a composed Platform containing additional devices, interrupt sources, and/or a native or TLM-adapted physical transport. |

A VP scheduler, virtual-time source, external model, or TLM adapter changes how
the Runner/Machine drives the Hart and how the Platform implements
`PhysicalAccess`; it does not create alternate instruction semantics. A future
multi-Hart Machine is not defined by this ADR, although the one-Hart ownership
boundary must not prevent a later explicit extension.

`RiscVSimulator` may remain a public convenience/library facade for a flat
single-Hart configuration. It must not acquire semantics that differ from the
shared Hart boundary merely because it is a library wrapper. `load_and_run` and
`RiscVSimulator::run` are current duplicated loops; the target has one Runner
ownership model, even if compatibility adapters temporarily expose more than one
entry point. The details of such adapters, deprecation, or replacement are not
selected here.

### 9. Compatibility constraints for the current product

These constraints preserve observable behavior for later implementation work;
they do not freeze the target address map, concrete bus, Rust API shape, or
transport. This ADR itself changes no behavior.

#### CLI and ELF

The existing `ruscv-sim run` interface remains the compatibility frontend:

- `run <ELF_FILE>` accepts the current ELF execution flow;
- `--max-cycles` remains an optional outer bound;
- `--tohost` remains a hexadecimal-or-decimal address override;
- `--verbose` remains presentation/debug output; and
- `--log-commits` remains an optional Spike-compatible commit-log sink.

The current loader accepts RV64 ELF64 little-endian executable RISC-V input,
loads PT_LOAD file bytes and zero-fills the memory remainder, preserves the entry
point, and discovers `.signature` and `.tohost` metadata or a `tohost` symbol.
Those are current compatibility expectations, not a prohibition on a future
loader profile.

For public `load_and_run`, tohost selection currently has this precedence:
command-line override, ELF-discovered address, then the fixed default
`0x4000_8000`. Later composition must preserve that observable precedence until
a separately approved product decision changes it. A command-line override is
configuration/observation policy; it is not an instruction-translation rule.

The CLI continues to format an `ExecutionResult`-equivalent report containing
exit code, completed cycle count, final PC, timeout status, error text, and
optional signature information, and continues to use the reported exit code for
its process status. Invalid ELF/input errors and execution-time result errors
remain distinguishable at the frontend boundary. Exact concrete result fields,
error enum variants, and serialization remain outside this ADR.

#### UART

The current public path provides a UART16550-compatible byte MMIO behavior at
`0x1000_0000`, sends transmitted bytes through the configured output callback
(which the CLI currently prints), and rejects multi-byte accesses to UART
registers. The model's declared `UART_SIZE` is eight bytes while `SystemBus`
currently routes a 0x100-byte window; this discrepancy is recorded as current
boundary debt rather than resolved or frozen by this ADR. Later Platform work
must preserve the current CLI's supported byte-register behavior and output
observability while an approved device/address-map contract is developed.

UART receive/FIFO and interrupt behavior covered by component tests remains
component evidence. It is not a claim that UART interrupts are integrated into
the public Hart path.

#### HTIF/tohost

The public path must continue to recognize the current host-exit conventions:

- the fixed HTIF endpoint at `0x4000_8000` accepts the current dword write/read
  behavior, including callback-based exit detection;
- selected ELF/CLI tohost storage is polled after each successfully completed
  instruction in the current flow;
- standard HTIF syscall/exit encoding and the supported high-bit alternative
  encoding continue to produce the same exit code interpretation;
- a recognized signal in selected ELF/CLI tohost storage is cleared after it is
  processed, preserving the current Spike-compatible intent; the fixed callback
  endpoint has no backing value to clear; and
- a successful writing instruction is observed as retired before the outer
  result reports Platform exit, as required by §6.

The fixed endpoint, selected ELF address, and their current coexistence are
compatibility behavior. This ADR does not choose a final HTIF device model or
address map. Their lower-level implementations must converge on one semantic
Platform event rather than making the Runner maintain two architectural loops.

#### Signatures and cycle limits

A discovered `.signature` region remains available as post-run bytes at its
reported address/metadata, with absent metadata represented as absent output and
a zero-sized region represented as empty output. Extraction remains host-side
inspection and must not alter Hart state.

The current default maximum is 10,000,000. In the public loop, one successfully
completed `RiscvCore::step` increments the exposed cycle count; a step that
returns an error does not, tohost is checked after successful execution, and a
zero bound performs no steps before producing a timeout result. The library
wrapper exposes the same configured/default limit concept. These are compatibility
constraints for current CLI/library behavior. MMQ-10 may later define richer
instruction-cost and virtual-time semantics, but no implementation under this
ADR may silently change the existing externally visible bound.

#### Library and bus surfaces

`RiscVSimulator`, `SystemBus`, `MemoryInterface`, `ExecutionResult`, and the
existing public helper functions remain available compatibility surfaces unless a
separate accepted decision authorizes a change. Their current differences are
not target semantics:

- `RiscVSimulator` uses a flat `SimpleMemory` path and does not wire the public
  UART/SystemBus;
- `SystemBus` is a concrete public-path RAM/UART/HTIF router in `executor.rs`;
- `MemoryInterface` exposes typed and sign/zero-extending operations; and
- commit logging currently derives register differences around `step` and passes
  no memory access from the active Runner.

Later adapters may place these surfaces over the shared Machine/Runner contracts,
but this ADR does not prescribe their constructors, trait signatures, lifetimes,
concrete layouts, or migration sequence. Existing focused tests remain evidence
to preserve, not proof of end-to-end VP integration.

### 10. Current source and test map

The following map records evidence for the ownership problem and compatibility
constraints. It does not relabel component tests as integrated behavior.

| Area | Current evidence |
| --- | --- |
| CLI and result presentation | [`src/main.rs`](../../../src/main.rs); [`tests/cli_test.rs`](../../../tests/cli_test.rs) covers help, parsing, flags, invalid input, and command processing. |
| Public Runner/orchestration | `load_and_run`, `load_and_run_file`, `dump_signature`, `clear_tohost`, `try_extract_exit_code`, and `ExecutionResult` in [`src/executor.rs`](../../../src/executor.rs); [`tests/executor.rs`](../../../tests/executor.rs) covers result/defaults, routing, error paths, exit encodings, signature helpers, and limits at the API level. |
| Current library loop | `RiscVSimulator::new`, `load_elf`, `step`, `run`, `read_mem`, and `write_mem` in [`src/executor.rs`](../../../src/executor.rs); [`tests/test_elf_loader.rs`](../../../tests/test_elf_loader.rs) covers creation, invalid input, setters, memory access, and result shapes. |
| ELF parse and flattening | [`src/elf.rs`](../../../src/elf.rs), including `ElfLoader`, `memory_footprint`, `load_into_memory`, `load_elf_file`, signature discovery, and tohost section/symbol discovery; ELF unit tests and [`tests/test_elf_loader.rs`](../../../tests/test_elf_loader.rs). |
| Hart and current address adaptation | [`src/core/mod.rs`](../../../src/core/mod.rs), including `RiscvCore::step`, `reset`, `run`, and `MemoryAdapter`; core unit tests cover reset and base-address adaptation. |
| Current memory contract | [`src/memory/mod.rs`](../../../src/memory/mod.rs) and its typed, alignment, endian, and sign/zero-extension tests. |
| Commit observation debt | [`src/core/commits.rs`](../../../src/core/commits.rs); [`tests/commits_test.rs`](../../../tests/commits_test.rs); public `load_and_run` snapshots/re-fetches around `core.step`. |
| Hart outcomes/traps consumed here | [ADR-0001](0001-hart-execution-outcome-and-observation.md), [`src/core/trap.rs`](../../../src/core/trap.rs), and [`tests/trap_test.rs`](../../../tests/trap_test.rs). |
| Physical-access contract consumed here | [ADR-0002](0002-physical-access-transaction-and-fault.md), [`src/mmu/physical.rs`](../../../src/mmu/physical.rs), [`src/tlm/traits.rs`](../../../src/tlm/traits.rs), and [`tests/tlm_tests.rs`](../../../tests/tlm_tests.rs). |
| Public native platform path | `SystemBus` in [`src/executor.rs`](../../../src/executor.rs), including RAM/UART/HTIF routing and fixed endpoint behavior; [`tests/executor.rs`](../../../tests/executor.rs) covers representative routing and access widths. |
| UART and platform components | [`src/peripherals/uart16550.rs`](../../../src/peripherals/uart16550.rs), [`src/peripherals/clint.rs`](../../../src/peripherals/clint.rs), [`src/peripherals/plic.rs`](../../../src/peripherals/plic.rs), [`src/peripherals/mod.rs`](../../../src/peripherals/mod.rs), [`tests/peripheral_tests.rs`](../../../tests/peripheral_tests.rs), and focused module tests. |
| TLM routing/time/DMI components | [`src/tlm/bus.rs`](../../../src/tlm/bus.rs), [`src/tlm/payload.rs`](../../../src/tlm/payload.rs), [`src/tlm/time.rs`](../../../src/tlm/time.rs), and [`tests/tlm_tests.rs`](../../../tests/tlm_tests.rs); these are adapters/components, not an integrated Hart path. |
| Debug inspection/control surfaces | [`src/debug/mod.rs`](../../../src/debug/mod.rs), [`src/debug/cli.rs`](../../../src/debug/cli.rs), [`src/debug/gdb_server.rs`](../../../src/debug/gdb_server.rs), [`src/debug/breakpoint.rs`](../../../src/debug/breakpoint.rs), and [`src/debug/watchpoint.rs`](../../../src/debug/watchpoint.rs); focused tests use mock targets and do not establish public-path integration. |

## Relationship to ADR-0001

ADR-0001 remains **Proposed** and is consumed here as the Hart outcome and
observation contract; this ADR neither accepts it nor reopens its architectural
retirement/trap decisions. In particular:

- Runner consumes `InstructionRetired`, `TrapEntered`, and `SimulatorFailure`
  rather than translating `Result<()>` or a generic memory error itself;
- Machine provides the one Hart/Platform composition in which those outcomes
  occur; and
- Platform exit, debugger stop, limits, scheduler/time events, and observer
  delivery remain outside the Hart outcome and are not converted into Hart
  traps.

The successful HTIF ordering in §6 is a direct composition consequence of
ADR-0001's rule that a successful MMIO instruction retires before the Runner
reports the resulting Platform event.

## Relationship to ADR-0002

ADR-0002 remains **Proposed** and is consumed here as the physical transaction
boundary; this ADR does not replace its raw-byte, fault, atomicity, or delay
semantics. Machine connects the Hart's single conceptual `PhysicalAccess` port
to a Platform implementation. The Platform owns target routing and target-local
faults; Hart owns virtual translation, architectural checks, load interpretation,
trap mapping, and retirement. Native memory, `SystemBus`, TLM, DMI, and external
models are possible Platform-side implementations or adapters, not alternate ISA
engines.

Image installation is host-side composition work and therefore does not use a
synthetic Hart load or claim a Hart `CommitRecord`. A page-table walk, in contrast,
is Hart/MMU architectural work and uses the same PhysicalAccess port as every
other physical transaction, exactly as ADR-0002 requires.

## Boundary with MMQ-10

MMQ-10 owns the detailed timing, interrupt, and stop arbitration contract. This
ADR fixes only the ownership and causal boundaries needed to compose the result:

| Concern deliberately left to MMQ-10 | Boundary fixed by this ADR |
| --- | --- |
| Interrupt eligibility, masking, priority, and exact Hart sampling point | Platform/Machine provide interrupt sources/lines; Hart consumes them at an approved boundary. |
| Instruction cost, physical delay units, delay consumption, and virtual-time advancement | Runner applies outer limits; Machine connects time/deadline information without choosing a scheduler. |
| Scheduler quantum, batching, temporal decoupling, and event-loop algorithm | Runner drives Machine; all strategies preserve precise Hart observations. |
| Tie-breaking among simultaneous trap, Platform exit, debugger, limit, time, and failure events | Runner aggregates distinct causes; MMQ-10 chooses precise priority/arbitration. |
| Asynchronous user/debug interruption and safe stop boundary | Debug control reaches Runner/Machine; it is not an architectural trap or device exit. |
| Whether a trap or device event is resumable in a particular run mode | Hart reports its architectural outcome; Runner applies run policy at the agreed boundary. |

MMQ-10 may refine these details without moving Hart semantics into Runner or
Platform, and without changing the requirement that a successful tohost-writing
instruction retires before Platform exit is reported. This is a bounded deferral,
not an unresolved ownership question.

## Alternatives considered

### A. Keep `load_and_run` as the machine, runner, and platform

This is the current public shape and is credible because it is small and already
runs ELF programs. It is rejected as the target because one function owns image
placement, device construction, Hart driving, stop policy, observers, and result
formatting; `RiscVSimulator` then duplicates related policy. It prevents a clear
Machine lifecycle, makes Platform events indistinguishable from run policy, and
encourages a second implementation when a VP is added.

### B. Make the Platform own the Hart and the run loop

A Platform-centric design is credible for system simulators because a scheduler
and devices often coordinate CPU execution. It is rejected because it couples
architectural semantics to concrete devices and transport timing, lets the
Platform perform Hart work, and makes standalone ISS behavior a special case.
The chosen design lets a Machine connect a Hart to a Platform while the Runner
controls the outer run; a future scheduler remains an input/strategy rather than a
second ISA owner.

### C. Maintain separate ISS and VP engines

Two engines could optimize each product form independently and minimize initial
composition work. It is rejected by the product invariant: traps, retirement,
MMU behavior, and device-visible memory effects would drift, and every compliance
or differential result would have to explain which engine produced it. The same
Hart and Machine boundaries are deliberately shared instead.

### D. Let the ELF loader write directly to a concrete bus

This is credible because the current loader returns a flattened buffer and the
executor immediately copies it into `SimpleMemory`. It is rejected because it
couples file parsing to one address map, conflates image/storage offsets with
virtual translation, and prevents a VP or external Platform from choosing its own
physical target. The loader describes; Machine installs; Platform routes.

### E. Use one undifferentiated event/status stream for all stops

A single stream could simplify plumbing by treating traps, tohost, breakpoints,
limits, and host errors as generic stop records. It is rejected because guest
architecture, platform behavior, debugger control, bounded execution, and
simulator failure have different ownership and recovery semantics. The Runner
may aggregate them in one run result, but it must preserve their categories and
causal facts.

### F. Make observers and debuggers direct peers of the Hart

Direct callbacks from instruction implementations or debugger code are credible
for small interpreters and can appear to reduce Runner overhead. It is rejected
because callbacks can observe partial architectural state, re-enter execution,
bypass Platform routing, and make failed transitions appear committed. ADR-0001's
completed-outcome boundary and Machine/Runner inspection path provide observation
without exposing an in-progress transition.

## Consequences

### Benefits

- One Hart implementation can serve a minimal ISS and a richer VP without
  duplicating architectural semantics.
- Machine gives construction, reset, placement, inspection, and teardown one
  explicit composition boundary.
- Runner can combine Hart facts, Platform events, debugger requests, limits, and
  later scheduler/time information without making them look like one kind of
  exception.
- Platform devices and transport adapters remain replaceable behind the
  PhysicalAccess contract, while the current native path and future TLM path can
  share semantics.
- ELF host concerns remain separate from Hart address translation, and signatures
  remain non-architectural inspection artifacts.
- The successful tohost ordering is precise enough for commit logs, differential
  testing, and VP event delivery without selecting a callback or scheduler API.

### Costs and risks

- A composition root and coherent lifecycle boundary are more explicit than the
  current monolithic executor and require coordination among Hart, Platform,
  Runner, and Frontend work.
- Platform implementations must preserve event provenance and distinguish target
  faults from host/adapter failures; a generic `MemoryError` cannot decide the
  outer result.
- Reset requires a meaningful definition of initial image state and dynamic
  device state. Backends that cannot restore mutable image-owned storage need a
  separate lifecycle guarantee rather than silently reusing state.
- Debug inspection and observer delivery need safe boundary/error handling and
  must not reintroduce snapshot/re-fetch reconstruction.
- Existing public wrappers and text logs need compatibility adapters while the
  target semantic boundaries are implemented; this ADR does not prescribe how.

## Compatibility and migration impact

This ADR is documentation-only. It changes no simulator behavior, public Rust
API, CLI output, test expectation, or serialization format. The current source
and focused tests remain the authority for implemented behavior; this record is a
Proposed target contract and does not claim that `Machine`, `Platform`, or the
Runner boundary already exists.

Future implementation work must preserve the compatibility constraints in §9,
including current CLI ELF operation, tohost precedence and encodings, UART byte
output, signature artifacts, cycle limits, `ExecutionResult`-equivalent reporting,
`RiscVSimulator` availability, and the public native bus behavior until an
accepted replacement contract says otherwise. The exact decomposition of
`load_and_run`, the relation of a library facade to Runner, concrete adapters,
public API evolution, and any migration order are intentionally deferred.

## Later verification when implemented

The following evidence is expected from later implementation work; it is not an
acceptance gate for this documentation ADR:

- **Ownership and one-engine parity:** prove that standalone ISS and VP
  configurations execute through one Hart implementation and produce equivalent
  Hart outcomes/architectural state for the same one-Hart workload.
- **Composition lifecycle:** verify construction wires one Hart, one Platform,
  PhysicalAccess, interrupt/event inputs, and observers; reset preserves static
  configuration/image metadata while clearing/restoring dynamic Hart and device
  state; teardown prevents late events.
- **Image placement:** verify ELF parsing/segment zero-fill and metadata are
  separate from physical placement; test that image-base storage adaptation does
  not bypass or impersonate Hart virtual translation.
- **Hart observation:** verify Runner consumes ADR-0001 records without snapshots
  or opcode re-fetch, with exactly one commit for each retired instruction and no
  fabricated record for a trap or simulator failure.
- **Physical-access parity:** verify native and TLM/external Platform adapters
  preserve ADR-0002 raw-byte, fault, atomicity, and delay semantics.
- **Platform event ordering:** verify successful HTIF/tohost writes produce a
  Platform event only after the writing instruction's `InstructionRetired` fact
  is complete, and failed writes produce no successful exit.
- **Result and inspection:** verify terminal categories remain distinct, final
  Machine inspection is coherent, debugger operations route through Machine,
  and signature extraction is host-side and post-run.
- **Compatibility regression:** run the focused CLI, ELF, executor, commit,
  peripheral, TLM, and debug tests plus project-authored ELF programs; record
  exact commands/revisions for any external compliance or differential suite.
- **MMQ-10 integration:** separately verify interrupt sampling, timing/delay,
  scheduler boundaries, and simultaneous-stop arbitration once MMQ-10 defines
  them; do not infer those results from this ADR.

## Bounded deferrals and open questions

Only the following are intentionally left to later contracts or implementation
design:

1. Concrete Rust layouts, constructors, traits, generics, callbacks, channels,
   lifetimes, ownership mechanics, and serialization/wire formats.
2. The final Platform address map, concrete device models, host-service API, and
   native/TLM/SystemC transport or FFI API.
3. MMQ-10's interrupt eligibility/sampling, time units and advancement, delay
   consumption, scheduler/batching strategy, safe asynchronous stop boundary,
   and simultaneous-event priority.
4. The selected ISA profile's exact reset values and any profile-specific device
   reset details not already owned by an accepted device contract.
5. The concrete representation of image placement, storage snapshots, and
   signature inspection, provided they preserve the address/translation boundary
   in §4.
6. The implementation relationship, deprecation policy, and migration sequence
   for `load_and_run`, `RiscVSimulator`, `SystemBus`, `MemoryInterface`, and
   existing debug/TLM components.
7. Future multi-Hart ordering, shared-device arbitration, DMA/coherence, and
   global observation ordering.

The ownership decisions, image-base-versus-translation distinction, successful
HTIF retirement ordering, one-Hart ISS/VP composition, and compatibility
constraints are decisions in this record rather than open questions. This ADR
remains **Proposed** pending normal review and acceptance; it has no superseding
record.
