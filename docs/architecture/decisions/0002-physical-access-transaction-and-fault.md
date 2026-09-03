# ADR-0002: Physical-access transaction and fault contract

| Field | Value |
| --- | --- |
| Status | Proposed |
| Authority | Draft contract; normative only after acceptance |
| Date | 2026-09-03 |
| Related decisions | [ADR-0001](0001-hart-execution-outcome-and-observation.md), [ADR-0003](0003-runner-machine-and-platform-ownership.md), the interrupt, time, and stop-event decision |

## Context

The target architecture has one Hart implementation for the standalone ISS and the
Virtual Platform. The Hart performs architectural work—access checks, virtual-to-
physical translation, load interpretation, and retirement—while the Platform routes
physical addresses to RAM, ROM, MMIO, or an external model. That boundary must also
serve instruction fetches and page-table walks; otherwise the ISS and Virtual Platform
can acquire different memory and fault behavior.

The [current implementation inventory](../current-state.md) shows why a semantic boundary is needed:

- `MemoryInterface` combines typed storage operations with RISC-V sign- and
  zero-extension helpers.
- The MMU physical-memory model and the native/TLM buses expose separate access and
  error shapes.
- AMO and LR/SC component implementations do not yet establish a composed atomicity
  or reservation contract.

Those observations motivate this ADR; they do not prescribe a refactor, a Rust API, a
transport protocol, or a claim of end-to-end integration. No simulator behavior or
public API changes are part of this ADR.

## Decision

### 1. One transport-neutral physical port

The architecture defines one conceptual `PhysicalAccess` port. It is a semantic
boundary, not a prescribed Rust trait or data structure. Every physical transaction
issued by a Hart goes through this boundary, including:

- instruction fetches;
- data reads and writes;
- atomic operations, including AMO and LR/SC; and
- page-table walks, including physical PTE reads and A/D-bit writes.

A request carries an explicit physical address, access category, and width. It is one
contiguous byte span; width is not inferred from a host integer type or a
transport-specific default. The contract can represent at least byte, halfword, word,
and doubleword spans. A backend must not silently choose another width or split a
request to make an otherwise invalid operation appear valid.

Atomic requests carry an **operation envelope** that identifies the semantic atomic
operation and the operands or conditional context needed to execute it indivisibly.
The envelope is deliberately abstract: this ADR does not prescribe its encoding, Rust
type, wire form, or any reservation token used by an implementation.

For an instruction fetch or data access, the Hart supplies a physical address after
its architectural checks and translation. During translation, the Hart/MMU likewise
supplies each physical PTE address needed by the walk. The port does not receive a
virtual address as the address to route, perform virtual-to-physical translation, or
apply RISC-V privilege, alignment, or PMP checks. An implementation may carry
provenance such as Hart identity or access origin for observation and atomic
coordination, but that metadata does not move architectural ownership into the
Platform.

### 2. Raw bytes and little-endian interpretation

The transaction boundary carries raw bytes in increasing physical-address order:

- byte `i` corresponds to physical address `paddr + i`;
- for the current RISC-V profile, an integer's least-significant byte is at the lowest
  address (little-endian); and
- a read or fetch response contains exactly the requested width of bytes.

This rule is independent of host endianness, Rust integer layout, payload layout, or a
device's internal representation. The Hart converts raw bytes to the value needed by
an instruction and owns sign extension, zero extension, narrowing, and every other
RISC-V load interpretation. A physical port or target never returns a sign-extended
`LB`, `LH`, or `LW` value. A fetch likewise returns instruction bytes; decoding remains
Hart work.

A request span must not wrap the physical address space. Range overflow, an absent
target, or an unsupported width is handled as a physical-target result under section
4 rather than by reinterpreting the address or bytes.

### 3. Ownership and boundary of checks

| Concern | Owner | Physical port behavior |
| --- | --- | --- |
| Effective virtual address, access kind, and load interpretation | Hart | Receives the selected semantic category and width. |
| Architectural alignment policy | Hart, according to the selected ISA profile | Receives only an access admitted by that policy. |
| Virtual-address canonicality, page permissions, translation, TLB, and PMP | Hart/MMU | Never performs these checks or translation. |
| Page-table walk and A/D update policy | Hart/MMU | Uses the same `PhysicalAccess` port for physical PTE reads and writes. |
| Physical map, target selection, and physical address range | Platform | Routes or reports the physical transaction result. |
| Raw byte transfer and target/device semantics | Platform target | Owns RAM/ROM/MMIO effects and target capability checks. |
| Indivisible atomic operations and competing-write visibility | Physical-access/Platform domain | Provides the atomic envelope and visibility needed by Hart reservation state; never exposes an AMO or SC as an ordinary read/write pair. |
| Per-Hart reservation architectural state and SC architectural result | Hart | Records reservation state and produces the architectural SC result from the physical operation response. |
| Trap entry, retirement, and architectural register/CSR effects | Hart; architectural cause mapping is owned by [ADR-0001](0001-hart-execution-outcome-and-observation.md) | A port call neither takes a trap nor retires an instruction. |
| Delay metadata consumption and scheduler advancement | Runner/scheduler under the interrupt, time, and stop-event decision | A delay annotation does not make the Hart sleep or choose a scheduling algorithm. |

The Hart checks architectural alignment before translation and before issuing a
physical request when the selected profile requires alignment. If a profile admits a
misaligned operation, its decomposition and architectural atomicity remain Hart
semantics; a backend must not silently split a request. A target rejecting a valid,
aligned request because it does not support that width is a physical access fault, not
a misalignment exception.

### 4. Response and fault semantics

A successful response is the complete result of one physical transaction:

- fetch/read returns raw bytes of the requested width;
- write returns completion with no fabricated read value; and
- an atomic response returns its operation-specific raw value or conditional status
  without exposing an intermediate transport operation.

The Hart interprets an atomic response and owns architectural register effects,
including the SC result. The response representation is intentionally left open.
Responses may also carry optional delay metadata described in section 7.

A **physical access fault** means that the Platform or target could not complete a
valid physical request. Examples include an unmapped physical range, target access
denial, an unsupported width or atomic capability, and a target-reported bus/device
error. The normalized fault preserves, at semantic level, the physical address/span,
requested category and width, and enough target/cause context for observation and Hart
outcome mapping. It does not require the Hart to understand `MemoryError`, TLM status,
or a host exception.

The following remain different:

| Failure | Produced by | Meaning at this boundary |
| --- | --- | --- |
| Misaligned access | Hart architectural checks | A guest architectural misalignment candidate; no physical transaction is issued. |
| Invalid/canonicality failure, invalid PTE, or page-permission failure | Hart/MMU | A page-fault/translation outcome; it is not an unmapped physical access fault. |
| Failed physical PTE read or A/D write | `PhysicalAccess` plus Hart/MMU context | A physical access fault, unless the response or adapter is malformed or otherwise a simulator failure; it is not an invalid-PTE classification. |
| Unmapped, denied, unsupported, or target bus/device failure for a valid physical request | Platform/target | A physical access fault for architectural mapping by the Hart. |
| Malformed response, violated atomicity, adapter protocol failure, poisoned synchronization, host/FFI failure, or internal invariant failure | Simulator/adapter | A simulator/internal failure, never silently converted into a guest page or access fault. |

Page-table walks use the same port as all other physical accesses. A failed PTE read
or A/D write therefore follows the physical-fault-or-simulator-failure distinction
above; it is not silently treated as an invalid PTE. [ADR-0001](0001-hart-execution-outcome-and-observation.md) defines the
architectural cause matrix: using the Hart's access kind and translation-stage context,
it maps a physical access fault to the corresponding Hart outcome and trap record.
This ADR supplies the physical classification and does not reopen or duplicate that
architectural mapping.

An adapter may translate backend-specific statuses into the normalized physical fault,
but must not collapse an internal failure into a guest-visible fault merely because
both originated during a memory call. The Hart retains original virtual-address and
instruction-stage context for architectural mapping; the physical fault retains
physical context for diagnosis.

### 5. Fault and side-effect atomicity

`PhysicalAccess` has transaction-level all-or-nothing semantics:

1. The Platform/target validates routing, width, target capability, and target
   preconditions before making the transaction's effects visible.
2. A failed read, write, or atomic request produces no partial RAM update, no partial
   device/register update, no interrupt or callback caused by the failed request, and
   no reservation invalidation caused by a write that did not commit.
3. A multi-byte write is committed as one semantic write; an adapter must not split it
   into independently visible byte writes unless that is explicitly the target's
   successful transaction semantics.
4. A successful MMIO read or write may have the device side effects defined by that
   device. Such effects are part of the successful transaction, not an excuse to expose
   a failed partial operation.
5. An AMO or successful SC is indivisible: no other initiator can observe a state
   between its read and write. If the target cannot provide that guarantee, it must
   reject the operation before mutation or report a simulator contract failure; it
   must not emulate an atomic operation with two ordinary visible accesses.
6. The Hart applies register, CSR, PC, retirement, and trap-entry effects only from the
   response it accepts. A faulting instruction does not gain a destination register or
   a committed store merely because a backend began processing it.

MMU Accessed/Dirty-bit writeback is an **ordinary separate physical transaction** on
the same port. It is not bundled into the later fetch/load/store transaction and is
not part of that transaction's rollback unit. Its Hart-level visibility follows the
selected RISC-V profile and the architectural outcome rules owned by [ADR-0001](0001-hart-execution-outcome-and-observation.md); this ADR
does not invent a second A/D update scheme.

A target that cannot provide a failed-read-without-side-effect contract must model the
operation as a successful device read or reject it before the side effect. An unknown
or non-rollbackable completion is a simulator failure, not a fabricated guest access
fault.

### 6. Atomic operations and LR/SC

The minimum backend capability is an atomic operation envelope at the same physical
port. For a target that advertises the capability, it must support:

- a read-modify-write serialized as one transaction and returning the old raw value
  for an AMO;
- a load-reserved returning the loaded raw value, after which the Hart records a
  reservation only if the physical read succeeded; and
- a store-conditional that tests the Hart's reservation context and, when valid,
  performs one atomic write, returning conditional status without an intermediate
  ordinary write.

The Hart owns the AMO operation's arithmetic, signed/unsigned comparison, result
extension, `aq`/`rl` architectural interpretation, destination-register effects,
per-Hart reservation architectural state, and architectural SC result. The
PhysicalAccess/Platform domain supplies the indivisible operation and the visibility
of committed competing writes needed for the Hart to maintain that state. It does not
own a process-global or otherwise architectural reservation singleton.

A non-faulting SC with no valid reservation is a conditional failure and performs no
physical write; the Hart produces the profile-defined architectural result and
observation. A normal SC attempt consumes its reservation according to the selected
profile, whether the conditional operation succeeds or fails. A faulting SC's exact
reservation effects likewise follow the selected ISA profile; this ADR does not invent
a blanket clear-or-retain rule for a faulting instruction. A backend that cannot
support the required conditional atomic operation reports a physical access fault
with no write; it must not report ordinary SC failure as a substitute for missing
capability.

A committed conflicting write or AMO through the same physical-access domain must be
visible to the Hart so that an affected reservation can be invalidated. This includes
a same-Hart conflicting store and, when such agents exist, writes from other Harts or
modeled masters. Hart semantics apply the architectural invalidation; the mechanism
by which the domain reports the competing write is an implementation mechanism.

The initial contract is usable for a single Hart and targets that provide the atomic
envelope. It makes no claim of multi-Hart or DMA ordering/coherence support. A
configuration that lacks an advertised atomic or reservation capability must reject
that capability explicitly rather than weaken the semantics silently.

### 7. Delay metadata without a scheduling decision

A response may carry optional non-negative duration or latency metadata. It is
forward-compatible information about the physical transaction, not an architectural
load value, fault classification, cycle count, or permission to mutate Hart state.
Zero is a valid modeled latency; absence means that the backend has no timing
annotation. An annotation never turns a fault into a wait or a success into a trap.

The interrupt, time, and stop-event decision owns units, precision conversion, accumulation, whether a fault consumes modeled
time, Hart budgeting, temporal decoupling, and scheduler advancement. This ADR does
not select a scheduler algorithm or require Hart semantics to depend on a transport
time type.

### 8. TLM and other backends are adapters

Flat memory, a native Platform bus, the MMU's physical target, and a future
SystemC/TLM implementation are interchangeable implementations of `PhysicalAccess`,
not alternate Hart execution paths.

A TLM adapter must preserve the semantic contract by:

- forwarding explicit physical address, width, raw-byte order, and request category;
- mapping response statuses to physical faults or simulator/internal failures under
  section 4;
- preserving the atomic operation envelope and indivisibility, never exposing a
  read-then-write AMO or SC emulation; and
- keeping DMI, when used, behind the same routing, side-effect, atomicity, and
  invalidation rules. DMI cannot bypass a device side effect or substitute for an
  atomic operation.

TLM phases, payload lifetime, byte enables, streaming, blocking or non-blocking
transport, and C++/FFI details are implementation mechanisms, not this contract.
They are described only as a **non-normative pointer** in
[`docs/integration/systemc-tlm.md`](../../integration/systemc-tlm.md). The optional TLM
field on a Hart must not become a second path that bypasses `PhysicalAccess`.

## Alternatives considered

| Alternative | Why it is credible | Decision |
| --- | --- | --- |
| Keep `MemoryInterface` as the common boundary | It already serves typed storage and load helpers. | Rejected. It couples storage to RISC-V sign/zero extension, hides the raw width, lacks a normalized physical fault/delay result, and cannot express an indivisible atomic transaction. It may remain a compatibility facade during migration, but not the architectural contract. |
| Make a concrete native bus the canonical API | A native bus can route RAM and MMIO directly. | Rejected. It couples the Hart to one Platform and makes MMU/TLM integration a special case. It is one possible backend. |
| Make a TLM generic payload the canonical API | TLM carries bytes, statuses, delay, routing, and possible DMI. | Rejected. TLM command, phase, and payload mechanics would leak into Hart semantics and do not by themselves define RISC-V atomicity or traps. TLM remains an adapter. |
| Use separate fetch, data, page-walk, and atomic ports | Separate ports can simplify local implementations. | Rejected. They duplicate routing and fault rules and permit inconsistent physical address spaces. One port preserves one physical contract while allowing implementation-specific internals. |
| Implement AMO as Hart-issued read followed by write | It is simple and can pass a single-threaded RAM test. | Rejected. Another initiator can interleave between operations, and a failed write can expose the wrong side effects. The backend must provide an atomic envelope while the Hart retains operation semantics. |
| Let the Platform perform translation, alignment, or sign extension | Centralizing checks can reduce local Hart code. | Rejected. It violates Hart/Platform ownership and makes page faults, misalignment, and load interpretation transport-dependent. |

## Consequences

### Benefits

- One transaction and fault vocabulary can drive flat RAM, native MMIO, page-table
  walks, atomics, and TLM without a second ISA execution path.
- Raw byte width and order are explicit, while RISC-V load interpretation and
  architectural effects remain in the Hart.
- Physical access faults, page faults, misalignment, and simulator failures remain
  distinguishable for [ADR-0001](0001-hart-execution-outcome-and-observation.md) and observers.
- Transaction-level atomicity prevents a backend from silently weakening AMO/LR/SC
  semantics while leaving target-specific mechanisms open.
- Delay can be carried without prematurely selecting the interrupt, time, and stop-event decision's scheduler or time policy.

### Costs and risks

- Backends must normalize existing status taxonomies and either provide or explicitly
  reject advertised atomic capabilities.
- Device models need an explicit width and failed-transaction policy; a target that
  cannot guarantee all-or-nothing behavior must reject the request rather than mutate
  partially.
- Reservation visibility needs a coordination mechanism when multiple Harts, DMA, or
  other masters are introduced; ordering and coherence are outside the single-Hart
  baseline.
- A compatibility facade may temporarily duplicate conversions, so it must not become
  a second Hart execution path.

## Non-normative implementation and verification notes

This section records later evidence and migration considerations for the Proposed
contract.

### Compatibility and migration notes

Later implementation work may adapt `MemoryInterface`, the native `SystemBus`, the MMU
physical-memory model, and existing TLM components behind `PhysicalAccess`. The exact
Rust trait signatures, request/response representation, adapter layering, backend
atomic primitive, reservation token, TLM transport, FFI boundary, and migration
sequence are implementation mechanisms and are intentionally not selected here.

Migration must preserve the current public ELF flow and its verified RAM/UART/HTIF,
signature, and cycle-limit behavior until an approved Platform replacement exists.
Existing component tests remain regression evidence for their components, not proof of
end-to-end `PhysicalAccess` integration.

### Verification when implementing

When an implementation is introduced, verification should establish:

- **Raw access:** fetch/read/write preserve exact byte order, explicit widths, and
  complete responses without implicit sign or zero extension.
- **Ownership and fault taxonomy:** Hart-side misalignment and translation failures
  issue no physical request; physical target failures remain physical faults; malformed
  or host/adapter failures remain simulator failures.
- **Atomicity:** failed operations leave memory and device state unchanged, and AMO and
  successful SC expose no visible read/write gap.
- **Reservations:** LR/SC state is per Hart, same-Hart conflicting writes are visible,
  and competing-write invalidation is exercised within the supported agent scope.
- **MMU:** page-table reads and A/D writes use the same physical boundary, with failed
  PTE transactions retaining the physical-fault-or-simulator-failure distinction.
- **Adapter parity:** native and TLM adapters preserve the same raw data, faults,
  atomic behavior, and delay metadata semantics; transport-specific behavior stays
  below the port.

The existing MMU, A/D, AMO/LR/SC, native-bus, and TLM tests cited by
[`current-state.md`](../current-state.md) are component-level evidence to preserve
when implementation begins. They are not prerequisites for accepting this ADR, and no
implementation, public API, or simulator behavior is part of this documentation
change.

## Bounded deferrals

Only the following concerns are intentionally bounded outside this ADR:

1. **Reservation granule:** the granule and any profile/target-specific granule rules.
2. **Multi-Hart and DMA ordering/coherence:** the complete ordering, invalidation, and
   coherence model once additional agents exist.
3. **Implementation mechanisms:** concrete Rust types, backend atomic primitives,
   reservation-token representation, transport/FFI details, and migration structure.

The selected ISA profile, rather than this ADR, supplies exact architectural behavior
for profile-defined cases such as a faulting SC. [ADR-0003](0003-runner-machine-and-platform-ownership.md)
and the interrupt, time, and stop-event decision consume this contract for their
respective ownership and timing concerns. The cross-check with [ADR-0001](0001-hart-execution-outcome-and-observation.md)
keeps the cause matrix consistent; these records do not redefine the physical-access
contract.
