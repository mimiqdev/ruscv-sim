# ADR-0003: Runner, Machine, and Platform ownership

| Field | Value |
| --- | --- |
| Status | Proposed |
| Authority | Draft contract; normative only after acceptance |
| Date | 2026-09-03 |
| Owner | Runtime and composition architecture |
| Related decisions | [ADR-0001](0001-hart-execution-outcome-and-observation.md), [ADR-0002](0002-physical-access-transaction-and-fault.md), the interrupt, time, and stop-event decision |
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

Today, [`src/executor.rs`](../../../src/executor.rs) concentrates ELF loading,
RAM/UART/HTIF construction, the instruction loop, limits, observations,
signature extraction, tohost polling, and `ExecutionResult` construction in
`load_and_run`; its `SystemBus` is a concrete RAM/UART/HTIF switch. The CLI in
[`src/main.rs`](../../../src/main.rs) calls `load_and_run_file`, while
`RiscVSimulator` exposes a second flat-memory loop with different address/device
behavior. See the [current implementation inventory](../current-state.md) for
wiring and test status; these are evidence for this decision, not target
execution engines.

`RiscvCore`, `ElfLoader`, memory, peripheral, TLM, and debug modules remain
component boundaries rather than the target composition. The current loader
returns a flattened buffer and the public core step a generic result, so this ADR
defines target contracts without claiming they are separated in code.

ADR-0001 defines the semantic Hart outcomes and observation ownership consumed by
this record: a completed step yields `InstructionRetired`, `TrapEntered`, or
`SimulatorFailure`, with Hart-owned commit/trap facts. ADR-0002 defines the one
transport-neutral `PhysicalAccess` boundary consumed here: all physical accesses
use the same raw-byte transaction vocabulary, and physical target faults remain
distinct from simulator/adapter failures. The interrupt, time, and stop-event
decision will define the detailed interrupt, time, and stop-event arbitration that
this record deliberately bounds.

## Decision

### 1. Vocabulary and dependency direction

The terms in this ADR have semantic meanings independent of any particular Rust
module or public API:

- **Hart** is the single architectural engine. It owns architectural state,
  instruction semantics, privilege, traps, address translation, retirement, and
  per-Hart architectural reservation state under the consumed Proposed working
  contracts ADR-0001 and ADR-0002.
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

The composition boundary may connect additional defined ports—such as interrupt
lines, observation delivery, and time/deadline information—without changing this
direction. Machine owns the association of any time/scheduling service with the
Hart and Platform. A scheduler, when present, is Machine-associated and cannot
bypass the Runner's outer loop or terminal policy; the complete invocation and
return rule is in §3. The interrupt, time, and stop-event decision defines only
detailed timing, interrupt, and stop-arbitration semantics. A port is a contract,
not a commitment to a Rust
trait, callback, channel, or transport representation.

| Concern | Primary owner | Boundary rule |
| --- | --- | --- |
| CLI/API/debug protocol and presentation | Frontend | Converts requests/results; does not own machine state or instruction semantics. |
| Image parsing and load-image description | ELF/image loader, invoked by Runner | Parses and validates the file and describes segments/metadata; does not mutate a Machine or translate a guest virtual address. |
| Image installation and placement | Machine coordinates; Platform writes | Machine installs the loader's `LoadImage` metadata; Platform performs physical writes, target checks, and routing; Hart is not involved. |
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
specific Runner. Current source and test evidence is pointed to in §10 without
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
4. connecting interrupt inputs, observation paths, and any defined time or
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
device reset behavior follow the applicable architectural/device contracts; the
semantic requirement is that no prior run's dynamic state is accidentally used as
the initial state of the next run.

#### Run, stop, and teardown

**Execution-loop rule.** The Runner owns the outer execution loop and all
terminal policy. For each iteration/request, it asks the Machine for one Hart
step or a budgeted quantum. The Machine alone invokes the Hart, or invokes its
Machine-associated scheduler to operate the Hart against that Machine's Platform,
and returns per-step Hart outcomes plus causal Platform events (and any
budget/time facts). The Machine does not apply Platform-exit, execution-limit,
debugger-stop, or other Runner terminal policy and does not decide whether
returned facts end the run. The Runner consumes those facts/events and decides
continue versus stop, including non-terminal Hart outcomes and result
aggregation. A quantum may contain multiple steps only when the Machine returns
each step's facts/events and preserves ADR-0001 observation semantics; it is not
a Machine-owned stop-policy operation. The interrupt, time, and stop-event
decision defines timing, interrupt eligibility/sampling, and simultaneous-stop
arbitration only; it does not own
this loop.

A Runner stop request, debugger request, limit, Platform exit, Hart trap, or
simulator failure must remain distinguishable in the run-level result. Stop
requests are handled at the defined architectural boundary; the interrupt, time,
and stop-event decision arbitrates only when multiple conditions are eligible at
one boundary.

At teardown the Runner stops accepting control, completes or reports observer
handling according to its run policy, and releases its run-level sinks. The
Machine quiesces the composition and disconnects or releases the Hart, Platform,
device, host-service, and event resources it owns. The Platform must not emit new
run events after the Machine has completed teardown. Whether a host transport
needs an adapter-specific shutdown sequence is an implementation detail, not a
new ownership boundary.

### 4. ELF parsing, image placement, and address meaning

The ELF/image loader and the Machine/Platform have separate responsibilities.
This Proposed ADR explicitly refines the wording in
[`principles.md`](../principles.md#address-ownership) that `ELF segment placement
is a loader responsibility`: here that loader responsibility means producing a
`LoadImage` metadata description containing segments, zero-fill, entry, and
signature/tohost metadata; Machine coordinates installation; Platform performs
physical writes and routing. The `Runner` `Image loading` row in the same
principles document is refined to this metadata/orchestration split. Because
ADR-0003 remains Proposed, `principles.md` remains the current normative
authority and is neither marked accepted nor rewritten here; it must be aligned
when this ADR is accepted.

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

### 5. Hart outcomes and observation ownership

The Hart boundary is the one defined by ADR-0001 and is consumed without
redefinition here:

- `InstructionRetired` means one instruction completed and produced one
  Hart-owned commit observation;
- `TrapEntered` means architectural trap entry completed and no instruction
  retired in that step; and
- `SimulatorFailure` means architectural completion was not possible and no
  fabricated commit or trap is emitted.

Under the §3 execution-loop rule, the Machine invokes the Hart only against its
composed Platform and returns these outcomes to the Runner. The Runner consumes
them and must not recreate them by comparing register snapshots, re-fetching
opcodes, interpreting generic memory errors, or polling a second execution loop;
current `load_and_run` snapshot/re-fetch logging is compatibility-era behavior,
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
whether a limit wins against another event are explicitly deferred to the
interrupt, time, and stop-event decision.
The current CLI instruction-count behavior is nevertheless preserved as a
compatibility constraint while these decisions are made.

### 8. Standalone ISS and future VP configurations

The standalone ISS and future VP are configurations of the same boundaries:

| Product form | Runner configuration | Machine/Hart/Platform composition |
| --- | --- | --- |
| Standalone ISS | ELF-oriented run control, deterministic single-Hart limit, optional commit log/debug inspection, and current host-exit compatibility | One Hart, one minimal native/flat Platform implementing PhysicalAccess, and only the platform services required by the current CLI. |
| Future VP around one Hart | Same Runner/Machine contracts with richer event/time control and an interchangeable Platform backend | The same one Hart implementation and architectural outcomes connected to a composed Platform containing additional devices, interrupt sources, and/or a native or TLM-adapted physical transport. |

A VP scheduler, virtual-time source, external model, or TLM adapter changes the
scheduling/budget strategy used by the Machine under §3 and how the Platform
implements `PhysicalAccess`; it does not create alternate instruction semantics.
A scheduler is Machine-associated, not a second Hart owner, and cannot bypass
the Runner's outer loop or Platform-exit, limit, or debugger terminal policy. A
future multi-Hart Machine is not defined by this ADR, although the one-Hart
ownership boundary must not prevent a later explicit extension.

`RiscVSimulator` may remain a public convenience/library facade for a flat
single-Hart configuration, but it must use the same Runner/Machine/Hart semantics
and must not become an independent execution loop. Compatibility adapters,
deprecation, or replacement are not selected here.

### 9. Compatibility constraints for the current product

These are the observable invariants required by this ADR for later
implementation; they do not freeze target internals, address maps, buses, Rust
APIs, or transport.
This ADR changes no behavior; see the [current implementation inventory](../current-state.md)
for current wiring and test status.

#### CLI and ELF

`ruscv-sim run <ELF_FILE>` retains `--max-cycles`, hexadecimal-or-decimal
`--tohost`, `--verbose`, and optional Spike-compatible `--log-commits`. The
current loader accepts RV64 ELF64 little-endian input, loads PT_LOAD bytes and
zero-fill, preserves the entry point, and discovers `.signature` and `.tohost`
metadata or a `tohost` symbol. Public `load_and_run` keeps tohost precedence as
CLI override, ELF-discovered address, then fixed `0x4000_8000`; the frontend
continues to report exit code, completed cycles, final PC, timeout/error status,
and optional signature data, with input and execution errors distinguishable.

#### UART, HTIF/tohost, and exit ordering

The public path retains UART16550-compatible byte MMIO at `0x1000_0000`,
transmitted-byte callback output, and rejection of multi-byte register accesses.
The fixed HTIF endpoint at `0x4000_8000` retains dword/callback exit behavior;
selected ELF/CLI tohost storage is polled after each successful instruction,
supports standard and supported high-bit encodings, and clears a recognized
selected-address signal (the fixed callback endpoint has no backing value to
clear). A successful HTIF/tohost write retires before the outer result reports
Platform exit, as required by §6.

#### Signatures, limits, and public surfaces

A discovered `.signature` region remains a host-side post-run artifact at its
reported address/metadata: absent is absent output and zero-sized is empty,
without a guest load or Hart-state change. The default maximum remains
`10_000_000`; a successful `RiscvCore::step` increments the count, an error does
not, tohost is checked after success, and zero performs no steps before timeout.
The library wrapper retains this configured/default limit concept. `load_and_run`,
`RiscVSimulator`, `SystemBus`, `ExecutionResult`, and existing helper names remain
available until an accepted replacement contract says otherwise; concrete layouts,
constructors, and migration remain outside this ADR.

### 10. Current source and test pointer

For as-is wiring and the full source/test inventory, see the [current
implementation inventory](../current-state.md). The ownership split is evidenced
by [`src/executor.rs`](../../../src/executor.rs), [`src/main.rs`](../../../src/main.rs),
[`src/elf.rs`](../../../src/elf.rs), and [`src/core/mod.rs`](../../../src/core/mod.rs);
focused tests cited there remain regression evidence, not proof of end-to-end VP
integration.

## Relationship to ADR-0001

ADR-0001 remains **Proposed** and is consumed here as the Hart outcome and
observation contract; this ADR neither accepts nor reopens its retirement/trap
decisions. Runner consumes `InstructionRetired`, `TrapEntered`, and
`SimulatorFailure`; Machine supplies their one Hart/Platform composition; and
Platform exit, debugger stop, limits, and scheduler/time events remain outside
Hart outcomes. The §6 HTIF ordering follows ADR-0001's retire-before-event rule.

## Relationship to ADR-0002

ADR-0002 remains **Proposed** and is consumed here as the physical transaction
boundary; this ADR does not replace its raw-byte, fault, atomicity, or delay
semantics. Machine connects Hart `PhysicalAccess` to a Platform: Platform owns
routing/target faults, while Hart owns translation, architectural checks, load
interpretation, trap mapping, and retirement. Native, `SystemBus`, TLM, DMI, and
external models are Platform implementations, not alternate ISA engines. Image
installation is host-side Machine work without a Hart load/commit; page walks use
the same port.

## Boundary with the interrupt, time, and stop-event decision

The interrupt, time, and stop-event decision owns detailed timing, interrupt, and
stop-arbitration semantics only. This ADR fixes ownership and causal boundaries;
it does not assign the outer loop or terminal policy to that decision:

| Concern left to the interrupt, time, and stop-event decision | Boundary fixed by this ADR |
| --- | --- |
| Interrupt eligibility, masking, priority, sampling point | Platform/Machine provide lines; Hart samples at the defined boundary. |
| Cost, delay, time units, advancement, and budget facts | Runner owns limits; the Machine-associated scheduler reports facts; the interrupt, time, and stop-event decision defines timing. |
| Quantum, batching, and event-loop strategy | Machine associates/invokes the scheduler under §3; it returns per-step facts/events and cannot bypass Runner terminal policy. |
| Simultaneous trap/exit/debug/limit/time/failure priority | Runner aggregates causes; the interrupt, time, and stop-event decision chooses arbitration. |
| Asynchronous interruption and resumability | Debug control reaches Runner/Machine; it is not a Hart trap or device exit. |

The interrupt, time, and stop-event decision may refine timing, interrupt, and
arbitration without moving Hart semantics or §3 loop/terminal ownership, and
without changing successful tohost retirement
ordering. This is a bounded deferral, not an unresolved ownership question.

## Alternatives considered

| Alternative | Decision |
| --- | --- |
| Keep `load_and_run` as machine, runner, and platform | Rejected: it fuses placement, device construction, Hart invocation, stop policy, observers, and result formatting, with no clear Machine lifecycle. |
| Let Platform own Hart and the run loop | Rejected: it couples ISA semantics to devices/timing; Machine composes Hart/Platform while Runner owns the outer run. |
| Maintain separate ISS and VP engines | Rejected: traps, retirement, MMU, and device effects would drift; one Hart/Machine boundary is required. |
| Let the loader write directly to a concrete bus | Rejected: it couples parsing to an address map and confuses storage offsets with translation; loader → Machine → Platform is retained. |
| Use one undifferentiated status stream | Rejected: causes have different ownership/recovery; Runner aggregates while preserving categories and causal facts. |
| Make observers/debuggers direct Hart peers | Rejected: callbacks risk partial state, re-entry, bypassed routing, and false commits; completed outcomes remain the boundary. |

## Consequences

### Benefits

- One Hart serves ISS and VP; Machine owns composition/lifecycle and Runner aggregates distinct facts without conflating terminal causes.
- Replaceable Platform backends preserve PhysicalAccess while image storage, translation, signatures, and successful tohost retirement remain distinct.

### Costs and risks

- The explicit composition root needs coordination beyond the monolithic executor and compatibility adapters for existing wrappers/logs.
- Platform reset/event implementations must preserve provenance, distinguish target from host failures, and restore initial image/device state rather than stale state.
- Inspection and observers need safe boundaries and must not reintroduce snapshots or opcode re-fetch.

## Compatibility and migration impact

This documentation-only ADR changes no simulator behavior, public API, CLI output,
tests, or serialization; source/tests remain authoritative and the target Machine,
Platform, and Runner boundaries are not claimed to be integrated. Later work must
preserve §9's CLI/ELF, tohost, UART, signature, cycle-limit, result, and named
surface invariants; decomposition, adapters, API evolution, and migration remain
implementation work.

## Later verification when implemented

These are implementation evidence for a later implementation:

- **Engine/lifecycle:** verify one Hart serves ISS/VP and §2–§4 construction, reset,
  image, observation, and teardown semantics.
- **Placement/observation:** verify metadata installation through Machine/Platform
  without translation and ADR-0001 records without snapshots or re-fetch.
- **Physical/events:** verify ADR-0002 parity and successful tohost exit only after
  retirement, with failed writes producing no successful exit.
- **Result/compatibility:** verify distinct terminal categories, coherent inspection,
  host-side signatures, current CLI behavior, and project ELF tests.
- **Interrupt, time, and stop-event decision:** separately verify interrupt,
  timing/delay, and stop arbitration once its technical contract is defined;
  verify the Machine scheduler's §3 boundary independently.

## Bounded deferrals and open questions

Only these remain for later contracts or implementation design:

1. Concrete Rust layouts, traits, callbacks, lifetimes, ownership, serialization,
   and wire formats.
2. Platform address map, device/host models, and native/TLM/SystemC APIs.
3. Interrupt sampling, time/delay semantics, safe async stop, and event priority
   belong to the interrupt, time, and stop-event decision; the Machine scheduler
   strategy remains bounded by §3.
4. Profile-specific Hart/device reset values not owned by an accepted contract.
5. Image-placement storage/snapshot and signature representations, plus migration
   and deprecation of existing wrappers/components.
6. Future multi-Hart ordering, shared-device arbitration, DMA/coherence, and global
   observation ordering.

The ownership, image-base distinction, HTIF retirement ordering, one-Hart
composition, and compatibility constraints are decisions, not open questions.
This ADR remains **Proposed**; it has no superseding record.
