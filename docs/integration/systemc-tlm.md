# SystemC/TLM Boundary

**Status:** Implemented for the selected blocking-transport mapping, direct
memory, the quantum selection, the non-blocking AT path, its retirement rules,
the fault-option controls (U3/U4), the blocking device targets of the UART and
HTIF windows (device D2–D9) and the host-served HTIF window (htif D1–D10). The
U4 exit of the [current contract](../dev-plan.md) added documentation and
acceptance mapping, not behavior.

**Authority:** Normative for the selected blocking-transport mapping, FFI
ownership and threads, error mapping, the time unit, re-entrancy and stop
behavior, and the build and version pins (D12 of the
[archived blocking-transport contract](../archive/milestones/blocking-tlm-systemc-transport.md),
to which this page's `dev-plan` D numbers refer), and for direct memory: grant,
permission, latency, invalidation, lifetime, version-clock persistence across
reset and the FFI rules (direct-memory D3–D10, D12 and D13 of the
[archived direct-memory contract](../archive/milestones/tlm-direct-memory-interface.md);
this page's `direct-memory D<n>` citations name that contract). Quantum D14 makes
this page normative for the local offset, the grant return, the sync and the
bound rules on this host, and this page's `quantum D<n>` citations name the
[archived quantum contract](../archive/milestones/quantum-temporal-decoupling.md).
The [archived non-blocking contract](../archive/milestones/non-blocking-tlm-transport.md)'s D7–D10 make this page normative for
suspension, the non-blocking base-protocol mapping, the commit rule, retirement
and the AT target mode; this page's `nb D<n>` citations name that contract. The
[archived devices contract](../archive/milestones/tlm-modeled-devices.md)'s
D5–D9 make this page normative for the device-target
mapping, the event rule, host views, reset and the attach entry; this page's
`device D<n>` citations name that contract. The
[current contract](../dev-plan.md)'s D8 and D9 make this page normative for the
host HTIF mapping, its event and run-stop rules and its fault options; this
page's `htif D<n>` citations name that contract. Bounded by
ADR-0002, ADR-0003 and ADR-0004 with their amendments, and by
[`docs/dev-plan.md`](../dev-plan.md), which governs where this page disagrees
with them.

**Last reviewed:** 2026-11-08

The repository implements one SystemC/TLM transport integration: a C ABI facade
([`src/capi`](../../src/capi), [`include/ruscv_sim.h`](../../include/ruscv_sim.h))
and a pinned SystemC host ([`integration/systemc`](../../integration/systemc)).
This page is the normative reference for that mapping, its ownership and thread
rules, its error and time mapping, its stop behavior, and its build pins. It
records the SystemC halves of the direct-memory contract's grant,
permission, latency, invalidation, lifetime and version-clock obligations and
its FFI rules, normative where they are not already covered by the
[archived contract](../archive/milestones/tlm-direct-memory-interface.md) and the
header's direct-memory obligations. It also records the SystemC halves of the
quantum contract: the local offset as the `b_transport` delay argument, the
grant return as the only quantum keeper, the sync and commit rule, and the two
fault-option controls of its §4. For the archived non-blocking contract it
records the AT path that serves a suspending `may_suspend` transaction through
`nb_transport_fw`/`nb_transport_bw`, its one-outstanding-transaction rule, its
`d = e` reporting, the RAM target's five return paths and the grant commit rule
(nb D7/D8/D10). For the same contract it records the timeout, the retirement of
a timed-out transaction, the late phases it ignores, the drain and the withheld
reset acknowledgement (nb D9). For the same contract it records the
non-blocking fault options. Blocking device targets are implemented on the same
ABI (device D1–D9): one device window becomes one blocking `b_transport`
transaction answered by an in-tree target or by a host target attached through
the C ABI, with at most one platform event per accepted access. Either window
may be served by the host: the UART window by the archived devices contract's
`UartTarget` (device D9) and the HTIF endpoint by the current contract's
`HtifTarget` (htif D8). The in-tree targets stay the default and the reference.

## Selected mapping

| ruscv-sim concept | SystemC/TLM concept |
| --- | --- |
| One physical access request | One `tlm_generic_payload` transaction |
| Blocking functional access | `b_transport` |
| Access delay | Annotated `sc_time` |
| ruscv-sim payload extension | `tlm_extension<PayloadExtension>` |
| Debug transport of image installation and host views | `transport_dbg` |
| Transport reset | The target's own reset operation |
| Guest-RAM target | `simple_target_socket` on `RamTarget` |
| Machine grant | Bounded grant over the C ABI, then `wait(consumed_fs - host_elapsed_fs)` |
| Hart local offset (quantum D11) | The `b_transport` `delay` argument |
| Quantum selection (quantum D12) | `ruscv_target.quantum_ticks` |
| Counted sync ending a grant (quantum D12) | The grant returns, then `wait(consumed)`; a quantum never combines with a suspending target |
| Direct RAM fast path | `get_direct_mem_ptr`/`invalidate_direct_mem_ptr` via the C ABI (direct-memory D12) |
| Suspending `may_suspend` transaction (nb D7) | The TLM-2.0 base protocol on the same socket |
| AT latency `lambda` (nb D8) | The target's `--nb-latency`, its declared `max_delay_fs` and its phase annotations |
| Host elapsed `e` (nb D4/D7) | Host time measured around the callback; `d = e` on the AT path |
| Grant commit (nb D5) | `wait(consumed_fs - host_elapsed_fs)` |
| AT timeout and retirement (nb D9) | `max_delay_fs + 1` fs after `BEGIN_REQ`, then `mark_unknown_completion` |
| Late phase (nb D9) | A phase taking effect after the timeout: ignored and counted |
| Device window (device D3) | One fixed register range: UART `0x1000_0000`/`0x100` or HTIF `0x4000_8000`/8 |
| Device target (device D2) | An in-tree `BlockingDeviceTarget`, or a host target attached through `ruscv_machine_attach_device` |
| Device transaction (device D2) | One `b_transport` call with zero delay and zero host elapsed time |
| Device event (device D4) | At most one `PlatformEvent` reported by an accepted access: `UartTransmit` or `HtifWrite` |
| Host-served UART window (device D9) | `UartTarget` in [`uart_target.h`](../../integration/systemc/uart_target.h), attached as `RUSCV_DEVICE_UART` |
| Host-served HTIF window (htif D8) | `HtifTarget` in [`htif_target.h`](../../integration/systemc/htif_target.h), attached as `RUSCV_DEVICE_HTIF` |
| Device reset (device D6) | The selected target's `transport_reset`, once per `Platform::build` and before the image is placed |

`tlm_generic_payload` and `sc_time` stay outside the Rust side and outside ISA
instruction functions. The only Rust/C++ boundary is the C ABI.

## RAM target module

[`ram_target.h`](../../integration/systemc/ram_target.h) is a zero-delay
blocking RAM target with one `tlm_utils::simple_target_socket`:

- `b_transport` serves reads, writes and every atomic, and adds no delay: it
  leaves the `delay` argument unchanged (quantum D13). That argument arrives as
  the Hart's local offset in exact femtoseconds, so a model that adds nothing
  keeps the run's committed time identical with the quantum on and off. A write
  is applied as one indivisible semantic write before it returns.
- AT mode (nb D8): under `--nb` the target also registers `nb_transport_fw`.
  Every return path applies the access exactly once inside the `BEGIN_REQ`
  call, through the same storage, atomic table, version clock and hint as
  `b_transport`; later phases carry only timing. The phases are scheduled
  through `peq_with_cb_and_phase`, whose zero-time notifications keep a
  one-delta gap between `END_REQ` and `BEGIN_RESP`, so no phase arrives in the
  delta that sent the request. `accepted` returns `TLM_ACCEPTED` and sends
  `END_REQ` in a later delta and `BEGIN_RESP` after `lambda`; `updated` returns
  `TLM_UPDATED` with `END_REQ` and sends `BEGIN_RESP` after `lambda`; `early`
  returns `TLM_UPDATED` with `BEGIN_RESP` and an annotated `lambda`; `annotated`
  returns `TLM_ACCEPTED` and sends `END_REQ` in a later delta and `BEGIN_RESP` at
  once with an annotated `lambda`; `completed` returns `TLM_COMPLETED` with an
  annotated `lambda`. A backward `BEGIN_RESP` answered with `TLM_COMPLETED` ends
  the transaction for the target, and a forward `END_RESP` is answered with
  `TLM_COMPLETED`. The target reports its `BEGIN_REQ` and applied-access counts
  at exit, along with whether a transaction is still open. It tracks the phases
  it has scheduled but not delivered, which the host's drain waits on (nb D9),
  and it withholds the transport reset acknowledgement while a transaction is
  open.
- Direct memory (direct-memory D13): a completed non-atomic transaction sets
  the generic payload's `dmi_allowed` hint, which is the only licence for the
  initiator's one query of that access. `get_direct_mem_ptr` grants the whole
  4 KiB chunk that contains the queried address, read-write with zero
  latency; a chunk that does not exist yet is denied, so a query never
  allocates storage. A direct store lands in the same bytes `b_transport`
  serves and bypasses the committed-write records, exactly as the adapter's
  write-suspension gate accounts for.
- The chunk bytes behind a granted pointer are `std::atomic<uint8_t>`, and
  every access to them, including `b_transport` and `transport_dbg`, is a
  relaxed atomic byte access: the granted storage must tolerate the C ABI's
  byte-wise relaxed atomic copies without a data race (direct-memory D10).
- `transport_dbg` serves image installation and the host views with no delay, no
  side effects beyond the bytes and no transaction accounting. It returns the
  bytes transferred; a short transfer is a failed debug transport.
- The transport reset withholds its acknowledgement before it invalidates or
  clears anything while an AT transaction is open at the target, from its
  `BEGIN_REQ` until the target has sent or received its final phase, so no fresh
  generation coexists with an open transaction (nb D8). Otherwise it
  invalidates every direct-memory registration on the bound initiator before it
  discards any chunk — atomically with respect to new grants, because the
  module runs on the single kernel thread — then discards every byte and every
  committed-write version record, so the Platform's zero-fill after it starts
  from an empty target. The version clock keeps running across the reset, so no
  committed-write version repeats (direct-memory D10/D13). It returns the
  acknowledgement the Platform requires (D6).
- Storage is sparse in 4 KiB chunks. Each chunk keeps the bytes and one
  committed-write version per byte. A load-reserved returns that snapshot as
  one little-endian 64-bit version per reserved byte; a store-conditional
  re-checks it and commits only when no committed write overlapped the reserved
  span. This mirrors the in-tree Rust target's reservation semantics.
- Atomics arrive in one transaction carrying the atomic extension and are applied
  through the target's own operation table inside that call: RMW for all nine AMO
  operations with W results zero-extended and signed or unsigned comparisons as
  the operation requires, load-reserved with its snapshot, and
  store-conditional with its committed or failed outcome. The target never
  performs a read followed by a write.
- Target-side request checks return the standard statuses: a missing extension
  or an unusable command is a command error, byte enables are a byte-enable
  error, a streaming width different from the data length is a burst error, and
  an address range that overflows is an address error. A malformed atomic
  request or snapshot is a generic error, that is, a host failure.

A target that does not accept the atomic extension answers
`TLM_COMMAND_ERROR_RESPONSE`, which ruscv-sim maps to the access fault of the
access kind, exactly as on native.

## Initiator and the C ABI

[`ruscv_initiator.h`](../../integration/systemc/ruscv_initiator.h) is the
initiator side of the same composition: the C ABI's `ruscv_target` callbacks are
mapped onto one `tlm_utils::simple_initiator_socket` bound to the RAM target.

- The address, command, data length and streaming width are the request fields.
  Byte enables are absent, the streaming width equals the data length, and one
  ruscv-sim access is one transaction. The adapter never splits, merges, caches,
  or retries an access.
- The response status starts incomplete. Only the seven standard blocking
  statuses are recognized; any other value is reported as incomplete. Of those
  seven, `TLM_INCOMPLETE_RESPONSE` is not a completion: the mapping table below
  classifies it as a protocol failure.
- The target's address, command and data length are echoed back into the ABI
  payload, so a target that changed the request is visible to ruscv-sim and is
  classified as a protocol failure by the blocking adapter rather than here.
- The atomic extension's request half is copied into the TLM extension and its
  response half is copied back into the ABI payload. No function pointer and no
  C++ object crosses the ABI.
- With `--dmi`, the table's `get_direct_mem_ptr` calls the IEEE 1666 forward
  query of the same name on the initiator socket: the query's address and
  access direction are the transaction's address and command, a `true` answer
  becomes `RUSCV_DMI_GRANTED` with the `tlm_dmi` fields copied verbatim
  (inclusive range, host pointer, granted access, exact femtosecond
  latencies), and `false` becomes `RUSCV_DMI_DENIED`, which caches nothing.
  A contained C++ exception becomes `RUSCV_DMI_FAILED`, the contained-
  exception case: an unknown completion, never a denial. Without `--dmi` the
  table field stays `NULL` and ruscv-sim issues no query.
- The initiator's registered `invalidate_direct_mem_ptr` forwards to
  `ruscv_machine_invalidate_direct_mem`, the one ABI entry a target may call
  inside a callback and the only ABI entry this backward call uses
  (direct-memory D12). A call while no handle is bound is a no-op: after
  destroy the handle's target wrapper and its registry no longer exist, and no
  live grant can still name them. The
  `dmi_allowed` response hint on the ABI payload is the transaction's TLM
  hint flag.
- The annotated delay is the delay the target adds, reported in exact
  femtoseconds. The delay argument is seeded with the payload's
  `local_offset_fs` before the call, so
  the request carries the Hart's local time in the sense of IEEE 1666: with the
  grant's start as the kernel clock, `sc_time_stamp() + delay` is the
  transaction's local time. On return the callback subtracts that offset and
  reports only `delay - offset` as `delay_fs`, so the offset is never charged as
  a delay and is not an added latency (quantum D11/D13). The declared
  per-transaction maximum is zero for the blocking model, which adds no delay,
  and `lambda` under `--nb` (nb D8).
- A returned `delay` below the offset cannot be represented as an added delay,
  and the target may already have committed the write, so the callback reports
  that transaction through the existing `mark_unknown_completion` path with
  `outcome_unknown` set, which ruscv-sim classifies as an unknown completion
  rather than a plain incomplete response (quantum D13).
- The target table's `quantum_ticks` carries the `--quantum` selection (zero is
  off) and is copied at create, so the selection persists across a reset
  (quantum D12). A NULL table selects the native route and cannot select one.
- With `--fault-keep-offset` the callback reports the full `delay` argument,
  offset included, as an added delay. That exceeds the declared maximum, which
  is the `--fault-keep-offset` control's failure and never a normal bridge
  (quantum §4).
- Suspension (nb D6/D7): `--nb` sets `ruscv_target.suspends_host_time`, which
  the ABI copies at create and keeps across reset. The adapter's flag arrives
  in `ruscv_payload.may_suspend`, and the callback's measured host time returns
  in `ruscv_payload.host_elapsed_fs` for every response, including the
  `Incomplete` response the ABI builds for a changed payload. Each grant
  reports its own `W` in `ruscv_grant.host_elapsed_fs`, after `facts`, and the
  host waits `consumed_fs - host_elapsed_fs`. A suspending table with a
  non-zero `quantum_ticks` is refused at create, and `--nb-latency` above 10^15
  fs, or `--nb-latency`/`--nb-path` without `--nb`, is a usage error.
- A debug request requires a complete transfer and an unchanged address, command
  and length; a short transfer or any changed request field is a failed debug
  transfer.
- AT path (nb D7): a payload with `may_suspend` set is served through
  `nb_transport_fw` instead of `b_transport`, from the callback's own process,
  the driver `SC_THREAD`. One transaction is outstanding. The process sends
  `BEGIN_REQ` with zero delay and is the only process that waits. `TLM_ACCEPTED`
  leaves the next phase to `nb_transport_bw`; `TLM_UPDATED` with `END_REQ` or
  `BEGIN_RESP`, and `TLM_COMPLETED`, take effect after their annotated delay.
  `nb_transport_bw` runs in the target's process, records the phase with its
  effective host time (`sc_time_stamp() + annotation`) and notifies the waiting
  process's event; the process acts only on phases whose effective time has
  arrived and re-arms for a later one, so no phase takes effect early. It
  returns `TLM_ACCEPTED` for `END_REQ` and `TLM_COMPLETED` for `BEGIN_RESP`,
  which ends the transaction without a forward `END_RESP`. After a forward
  `TLM_UPDATED`/`BEGIN_RESP` takes effect, the process sends `END_RESP` with
  zero delay; a return other than `TLM_COMPLETED` or `TLM_ACCEPTED` is a target
  protocol error reported as an unknown completion. A `BEGIN_RESP` before
  `END_REQ` also ends the request. The transaction completes at the host time
  its `BEGIN_RESP` or forward `TLM_COMPLETED` takes effect, and the callback
  reports `d = e` for the host time from `BEGIN_REQ` to completion. The
  transaction is bounded by the declared maximum as described under
  [Retirement, late phases and the drain](#retirement-late-phases-and-the-drain).
- The AT transaction is initiator-owned: it has its own generic payload, a
  memory manager, its own data storage and its own payload extension. Request
  bytes are copied in before `BEGIN_REQ` and response bytes are copied out at
  completion, so the target never sees the ABI's buffer. The status, bytes,
  atomic response half and `dmi_allowed` hint come from the transaction.
- On every `b_transport` the initiator measures `e` around the callback and
  reports it, and an exception's `mark_unknown_completion` keeps that measured
  value instead of only zeroing the delay. It also measures host time around
  `transport_dbg`, `transport_reset` and `get_direct_mem_ptr`; one that lets host
  time pass becomes a failed debug transfer, an unacknowledged reset or
  `RUSCV_DMI_FAILED`.

Error mapping across the boundary:

| TLM response status | ABI status code | ruscv-sim classification |
| --- | --- | --- |
| `TLM_OK_RESPONSE` | `RUSCV_TLM_OK` | Completion |
| `TLM_ADDRESS_ERROR_RESPONSE` | `RUSCV_TLM_ADDRESS_ERROR` | Guest-visible target fault |
| `TLM_COMMAND_ERROR_RESPONSE` | `RUSCV_TLM_COMMAND_ERROR` | Guest-visible target fault |
| `TLM_BURST_ERROR_RESPONSE` | `RUSCV_TLM_BURST_ERROR` | Guest-visible target fault |
| `TLM_BYTE_ENABLE_ERROR_RESPONSE` | `RUSCV_TLM_BYTE_ENABLE_ERROR` | Guest-visible target fault |
| `TLM_GENERIC_ERROR_RESPONSE` | `RUSCV_TLM_GENERIC_ERROR` | Host failure |
| `TLM_INCOMPLETE_RESPONSE` | `RUSCV_TLM_INCOMPLETE` | Protocol failure |
| Anything else | `RUSCV_TLM_INCOMPLETE` | Protocol failure |

An unknown completion (the extension's outcome-unknown flag), an adapter panic,
or a C++ exception stays an unknown completion. No TLM status, callback return,
or host stop becomes a ruscv-sim terminal result.

## Device targets

device D1–D9 add a second target kind to the same composition. The UART window
`0x1000_0000`/`0x100` and the HTIF endpoint `0x4000_8000`/8 are device windows.
CLINT, PLIC and the source-1 gateway stay Platform state on both routes, because
they own Platform time, the presented interrupt lines and the gateway's
retraction rule.

- Window policy stays in the Platform. A UART access is byte-only, a fetch is
  rejected and the window is `0x100` bytes wide; the HTIF endpoint requires its
  exact start, a full eight-byte dword and no fetch, and its atomic RMW
  envelope stays Platform-side and never becomes a device transaction. A target
  therefore never sees a request the Platform would reject, and the in-tree
  target and the native branch share one register helper
  (`Uart16550::write_reg_reporting`).
- Carrier and annotation. One validated access is one blocking `b_transport`
  transaction with zero delay and zero host elapsed time. A non-zero
  `delay_fs`, `local_offset_fs`, `host_elapsed_fs`, `may_suspend` or
  `dmi_allowed`, a changed address, command or length, more than one event, an
  event whose tag does not match its window, and an event on a non-`Ok` status
  are protocol failures of that access (device D2/D4; ADR-0002's 2026-11-07
  device-target amendment). The outcome-unknown classification runs first, so
  an unknown completion that carries a valid event publishes nothing. A device
  transaction adds nothing to `transport_delay_femtoseconds`,
  `host_elapsed_femtoseconds`, `mcycle`, `minstret`, the turn completion bound
  or a grant's horizon. A Hart device access still flushes the local offset
  under the bus lock before the target sees it (quantum F1), so the target
  observes committed Platform time and can neither read nor move the offset.
- Events. Each window has one publication sink, installed once by
  `Platform::build`: the UART sink pushes `PlatformEvent::UartTransmit(byte)` and
  then calls `config.uart_output(byte)`; the HTIF sink pushes
  `PlatformEvent::HtifWrite(value)` and then calls `config.htif_write(value)`.
  An accepted THR write, the typed byte write of an in-tree window and a host
  target's response all publish through that sink before the store retires; a
  faulted or unknown access publishes nothing.
- Host views. A host-served UART window has no Rust register state, so
  `Platform::uart_inspection` returns `None`, `Platform::receive_uart` calls the
  target's `receive`, and a typed `MemoryInterface` read or write of the window
  is `MemoryError::InvalidAddress`. An in-tree window keeps today's answers: the
  typed byte write applies the register write and publishes through the UART
  sink, and the typed HTIF dword read answers zero on both routes.
- Direct memory. A device window is never offered direct memory. The rule is
  dispatch, not coverage: the bus hands the RAM slot only a request whose whole
  span it matched as RAM, so a device span takes the device path, which never
  sets `dmi_allowed` and never queries.
- Reset. `Platform::build` calls the selected target's `transport_reset` once,
  ahead of the RAM slot's `begin_window` and before the image is placed; a zero
  return fails the build through the existing placement channel and leaves the
  current association intact. Every install builds a fresh `Platform`
  (`fresh_reset`, `reload_for_ndmreset` and `ruscv_machine_reset` alike), so no
  in-tree register state and no host state crosses a generation, and no device
  transaction outlives a Platform.
- Attach entry. `ruscv_machine_attach_device(machine, RUSCV_DEVICE_UART, &table)`
  runs after `ruscv_machine_create` and before the first
  `ruscv_machine_load_elf`. A NULL handle, NULL table, missing callback or
  unknown window is `RUSCV_ERROR_INVALID_ARGUMENT`; a mismatched
  `window_base`/`window_size`, a second attach for the window, a late attach and
  a handle without a `ruscv_target` (no TLM route) are `RUSCV_ERROR_CONFIG`. The
  table is copied at attach, so its callbacks and `user` are fixed for the
  handle's life and `user` must outlive it. The callbacks obey the RAM target's
  rules: creating thread only, no re-entry (every ABI entry that reaches the
  callback check returns `RUSCV_ERROR_REENTRANT`, except
  `ruscv_machine_invalidate_direct_mem`), no waiting, no host time and no
  exception across the boundary.
- Host model. [`uart_target.h`](../../integration/systemc/uart_target.h) is the
  smoke's host UART: a plain callback object, not a SystemC module. It models
  RBR/THR, IER, IIR/FCR, LCR, MCR, LSR, MSR, SCR, DLL/DLM and 16-byte receive
  and transmit FIFOs, answers `RUSCV_TLM_OK` with one
  `RUSCV_DEVICE_EVENT_UART_TRANSMIT` on an accepted THR write with DLAB clear,
  echoes that byte to the host console as one `uart-console:` line, and reports
  its transaction, read, write and transmitted-byte counts at exit.
  `transport_reset` discards every register and FIFO and acknowledges.
- Host HTIF model. [`htif_target.h`](../../integration/systemc/htif_target.h) is
  the smoke's host endpoint (`--htif-target host`, htif D8), a plain callback
  object attached as `RUSCV_DEVICE_HTIF` 1 with the same table, attach entry
  and callback rules as the UART target. Its read answer is its own register
  state (htif D3), which this model fixes at zero so the parity checks have a
  fixed point, and an accepted dword write returns `RUSCV_TLM_OK` and one
  `RUSCV_DEVICE_EVENT_HTIF_WRITE` whose value is the guest's eight little-endian
  bytes; the D10 value rule refuses any other value before publication. It
  renders the standard console device's byte
  (device 1, command 1, payload byte) as one `htif-console:` line as host
  output: the crate decodes no HTIF device code and routes no HTIF value to
  `config.uart_output`. `receive` is never called, because the endpoint has no
  host-to-guest direction (htif D3), and `transport_reset` discards the window's
  own state.
- HTIF run stop. The published value is the response's eight write bytes, the
  sink is the existing `config.htif_write`, and the run stop is the existing
  `try_extract_exit_code` decode of that value (htif D4/D5). The guest store
  retires, the sink publishes once, and the run loop classifies `PlatformExit`
  on that turn, exactly as for the in-tree endpoint. The host cannot stop the
  run, choose the exit code, clear the signal or suppress the retired store; a
  host that omits the event leaves no exit fact, so the run continues to its
  turn bound and fails. A guest that writes a non-exit value continues and the
  value is still published once.
- Sink observation. The C ABI has no per-value sink entry for an HTIF write, so
  this host observes publication through the run stop (the same exit code, exit
  turn, grant and consumed facts as the matching in-tree pass, with the exit
  word as the last accepted value), a dropped event through the run that
  reaches its turn bound with no exit fact, and a refused write through the
  protocol failure and its absence from the target's accepted sequence. The
  sink-level rules for a refused or a non-exit write are Rust-level
  (`src/blocking/device.rs`, proven in `src/machine/tests.rs`), and a
  sink-level console assertion would need a new ABI entry, which htif D7 keeps
  append-only and out of this contract.

## FFI ownership and threads

- Every handle belongs to the thread that created it. A call from any other
  thread fails with `RUSCV_ERROR_NOT_CREATOR_THREAD` and changes no state. The
  SystemC host creates and drives the handle on the SystemC kernel thread, so the
  rule holds even though SystemC runs the thread as a coroutine.
- Target callbacks run on the SystemC kernel thread inside a ruscv-sim call. A
  callback that calls back into any ABI handle fails with
  `RUSCV_ERROR_REENTRANT`, except `ruscv_machine_invalidate_direct_mem`, which
  the direct-memory backward path uses (direct-memory D12). A `b_transport`
  callback whose payload has `may_suspend` set may suspend the calling process
  and let host time pass, provided it resumes and returns on the creator thread
  (nb D6/D7). Every other callback must not wait or advance SystemC time; this
  initiator checks that rule for debug transport, transport reset and the
  direct-memory query, and the blocking callback still reports its measured
  elapsed time so the ABI's unmarked-elapsed rule can reject a wait.
- `MachineDriver::run` compares its own thread id with the one `sc_main`
  recorded before it creates the handle. A mismatch means a pthread-process
  build, which is unsupported; the host refuses it as a host failure. The pinned
  coroutine build passes the check (nb D7).
- The callback table and its `user` pointer must stay valid until destroy. The
  initiator and the RAM target are members of `sc_main`, so they outlive the
  handle.
- Payload data pointers are owned by Rust and valid only for one callback.
- A Rust panic is caught at the boundary, the handle is poisoned, and the call
  returns `RUSCV_ERROR_PANIC`; later calls return `RUSCV_ERROR_POISONED`. No Rust
  or C++ exception crosses the ABI: each target callback contains every C++
  exception, so it cannot unwind into the ruscv-sim call. A blocking exception
  becomes an unknown completion, a debug exception becomes a failed debug
  transfer, and a reset exception withholds the acknowledgement. A report of
  failure allocates nothing, so reporting it cannot itself throw.

## Time unit and commit rule

The SystemC host sets a 1 fs time resolution before the kernel starts, so one
`sc_time` unit is exactly one femtosecond. The declared Platform tick period is
one nanosecond, the default of `MachineConfig.tick_period`; `ruscv_machine_create`
carries the C ABI's declaration of the same period.

Each grant starts at the current `sc_time_stamp()`. The kernel owns global time
and commits only the amount a grant returns. With no host time inside the grant
the host waits exactly `consumed_fs`; with suspending transactions it waits
`consumed_fs - host_elapsed_fs`, because that host time already passed in the
callback. Either way it checks after the wait that the kernel clock equals
`committed_end_fs`, and a mismatch ends the run as a host failure. A grant whose
host elapsed exceeds its consumed amount commits nothing more and is a host
failure unless the run has already failed (nb D5). This blocking target adds no
transport delay, so a run stays time-equivalent to native; every AT transaction
reports `d = e = lambda`, whose charge is rounded once through the existing
delay path (ADR-0004 §4.4, §5.3; nb D4/D5/D10).

### Retirement, late phases and the drain

The initiator owns the timeout of an AT transaction (nb D9):

- Every wait of the callback's process is bounded by `max_delay_fs + 1` fs after
  `BEGIN_REQ`, where `max_delay_fs` is the target's declared maximum — the
  `lambda` of `--nb-latency`. A completion at exactly the declared maximum is
  therefore in time, including a delta-notified one at `lambda` = 0.
- A phase whose annotated delay would take effect after the timeout is *late*.
  There are three forms, and each is counted once and ignored for the Machine: a
  backward call made after the timeout, a phase recorded before it whose
  effective time falls after it, and a forward `TLM_UPDATED` or `TLM_COMPLETED`
  whose annotation reaches past it. A backward `END_REQ` is still answered with
  `TLM_ACCEPTED` and a backward `BEGIN_RESP` with `TLM_COMPLETED`, which ends the
  target's transaction, so a late phase cannot change target state.
- A transaction that is not complete at the timeout is retired: the callback
  reports it through `mark_unknown_completion` with the outcome-unknown flag
  set, and the ABI charges the time waited as `e` under D4 step 1. The overshoot
  rule does not reclassify it and the unknown completion poisons the transport,
  so no later step can turn it into a plain status.
- The retired transaction's storage stays allocated until the target has
  completed the transaction or released every reference it acquired. A target
  that completed it on the forward path (`TLM_COMPLETED`) never held it, so its
  storage is released at once. On the `early` path the initiator keeps the
  transaction as drain state and sends its forward `END_RESP` in the drain, no
  earlier than the `BEGIN_RESP` phase's effective time. Late response bytes
  never reach the ABI.
- At every stop after a retirement, whether a terminal result or a host failure,
  the host first runs the kernel until the RAM target has no scheduled phase and
  the initiator holds no drain state, so every late phase is delivered and
  ignored. Only then does it destroy the handle and stop the kernel. Before the
  drain it probes `ruscv_machine_reset` while the target still holds the
  transaction: the reset must not be acknowledged, so the recovery path returns
  a non-OK status. The probe is skipped when the forward path already released
  the transaction.
- At exit the host reports the late-phase count, the retirement count, the
  forward `END_RESP` phases its drain sent with the host time of the one it
  sent, and whether a transaction is still open at the target. A run without a
  retirement reports zero for the first three and no open transaction.

The RAM target supports the same rule by construction: it applies the access
once at `BEGIN_REQ`, so a late phase carries only timing, and it withholds its
`transport_reset` acknowledgement while a transaction is open, so no fresh
generation can coexist with a retired one.

### Quantum and temporal decoupling

With `--quantum` the Machine may run ahead of committed Platform time by a local
offset `O`, bounded by the selected quantum (quantum D3–D5). This host's rules:

- The kernel clock is the origin of the offset for a whole grant: the anchor
  `E` equals the committed time at grant start, and each counted sync that moves
  committed time ends the grant, as does an idle jump with a virtual deadline
  (quantum D12). The grant loop is therefore the only quantum keeper; this
  composition has one initiator, so `tlm_quantumkeeper` and
  `tlm_global_quantum` are not used (quantum D13).
- A transaction's `delay` argument is the offset: `local_offset_fs`, the Hart's
  local time minus the anchor plus the turn's already-charged ticks, in exact
  femtoseconds. A device access inside a turn commits time mid-turn, which moves
  `O` and not `E`, so a later RAM access in the same turn still carries its
  correct local time (ADR-0002's 2026-11-06 amendment). The offset is timing
  metadata only: no address, length, status or permission depends on it.
- Every grant return is a sync, so `committed_end_fs` and `consumed_fs` are
  post-sync, `committed_start_fs + consumed_fs == committed_end_fs`, and
  `wait(consumed_fs)` leaves `sc_time_stamp()` equal to `committed_end_fs`
  (quantum F7/D12). The host checks that equality after every grant and ends the
  run as a host failure on a mismatch.
- The grant's horizon, `deadline_blocked` and `residual_slack_fs` tests use the
  Machine's local time, so `committed_end_fs` never passes the horizon and
  deadline overshoot is zero (quantum D8/D12).
- The timer-lateness rule this page fixes is D8's: a time-derived MTIP crossing
  has zero lateness for `Q` <= 1; for `Q` >= 2 the run's first presentation `R`
  follows its own first turn-start local time `X` at or past the armed
  `mtimecmp` by at most `Q - 1 + t` ticks, where `t` is the target's declared
  turn completion bound. The bound is measured inside that run after the first
  differing recognition, not against the parity run. MTIP is never early;
  MSIP, MEIP and SEIP, a compare or `mtime` store and a crossing during a wait
  have zero lateness (quantum D8, F1, F3). `Q` = 1 is precise; `Q` >= 2 is the
  explicit reduced-accuracy request (quantum D10).
- `--quantum 1` syncs at every turn boundary, so each grant ends after one turn
  and the same guest takes about one grant per completed turn. `--quantum 1024`
  commits once per quantum, so a RAM-only stretch takes far fewer grants.

### Fault-option controls

`--fault-keep-offset`, `--fault-decrease-delay`, the five non-blocking
options and the six device options below are the fault-option controls of
their contract's §4, in the style of `--stop-after-grants`. The quantum pair
requires `--quantum`, the non-blocking trio (`--fault-late-response`,
`--fault-uncounted-elapsed`, `--fault-short-delay`) requires `--nb`, the
blocking pair (`--fault-wait-blocking`, `--fault-wait-debug`) is a usage error
with `--nb`, the UART device trio requires `--uart-target host` (device D9) and
the HTIF trio requires `--htif-target host` (htif D8).
Exactly one fault option may be selected in a run. They exist because the
mutation runner builds only Rust, so a C++ fault cannot be a control row there
(quantum D14, nb D11, device D10). None is a normal host, and a run that passes
with one set is a smoke failure:

- `--fault-keep-offset` reports the full `delay` argument as an added delay. The
  run must report `result state=failed`, the failed boundary's `uncertain` flag
  must be 0, and its diagnostic must name the exceeded declared maximum, which
  is zero for this target.
- `--fault-decrease-delay` makes `RamTarget` lower `delay` below the offset it
  received. The run must report `result state=failed`, the failed boundary's
  `uncertain` flag must be 1, and its diagnostic must name the outcome the
  target could not establish, which is the C ABI's unknown-completion
  classification.
- `--fault-late-response` makes `RamTarget` delay the run's first AT access's
  final phase to one Platform tick past the declared maximum. That access is
  the guest's first fetch. The run must report `result state=failed` with the
  boundary's `uncertain` flag at 1 and the unestablished-outcome diagnostic, and
  it must ignore and count at least one late phase. On `accepted` and `early`
  the reset probe must be refused while the target still holds the transaction,
  and the drain must leave no transaction open; on `early` the drain sends the
  retained transaction's `END_RESP`. On `completed` the target released the
  transaction on the forward path, so the host skips the probe and the storage
  is released at once.
- `--fault-uncounted-elapsed` reports `e` = 0 while reporting `d` as the host
  time that passed. The host must stop with a host failure whose diagnostic
  names the kernel clock that does not match the Machine cursor.
- `--fault-short-delay` reports `d` one femtosecond below `e`. The run must
  report `result state=failed` with `uncertain` 0 and a diagnostic naming host
  elapsed time above the response delay.
- `--fault-wait-blocking` makes `RamTarget::b_transport` wait one tick. The run
  must report `result state=failed` with `uncertain` 0 and a diagnostic naming
  host elapsed time on a request that may not suspend.
- `--fault-wait-debug` makes `RamTarget::transport_dbg` wait one tick. Image
  installation must fail, and the host must report a failure of
  `ruscv_machine_load_elf`.
- `--fault-uart-no-lsr` makes `UartTarget` answer `LSR` with the
  transmit-holding-empty bit clear. `hello` never leaves its wait, so the run
  must exhaust its turn bound and fail without reporting a run result, and the
  target must transmit no byte.
- `--fault-uart-wait` makes `UartTarget::b_transport` wait one Platform tick and
  report it as host elapsed time, which a device callback may not do (device
  D4). The run must report `result state=failed` with a diagnostic naming host
  elapsed time on the UART window and the device-callback rule.
- `--fault-uart-event-on-fault` makes `UartTarget` report a transmit event with
  `RUSCV_TLM_ADDRESS_ERROR`. The run must report `result state=failed` as a
  protocol failure of that access, and no byte may reach the host's console.
- `--fault-htif-drop-event` makes `HtifTarget` answer the exit word's write with
  `RUSCV_TLM_OK` and no `HTIF_WRITE` event while the three console writes keep
  their events and values. No exit fact exists, so the guest's self-loop must
  exhaust the turn bound and fail with the turn-limit diagnostic, the run must
  report no run result, and the accepted values must be exactly the three
  console words.
- `--fault-htif-wait` makes `HtifTarget::b_transport` wait one Platform tick and
  report it as host elapsed time, which a device callback may not do (htif D9).
  The run must report `result state=failed` with a diagnostic naming host
  elapsed time on the HTIF window and the device-callback rule, and no value may
  reach the target's accepted sequence.
- `--fault-htif-event-on-fault` makes `HtifTarget` report the exit write's
  `HTIF_WRITE` event with `RUSCV_TLM_ADDRESS_ERROR`. The run must report
  `result state=failed` as a protocol failure of that access, and the refused
  write must be absent from the target's accepted sequence.

All three UART device options are usage errors without `--uart-target host`, and
with an explicit `--uart-target in-tree`. The same rule holds for the three HTIF
options and `--htif-target` (htif D8).

## Reentrancy and stop behavior

- The initiator drives one Machine on one `SC_THREAD` in bounded grants: a
  started-slot budget, an inclusive horizon, a grant guard, and a bound on
  completed turns (`--max-turns`, the C ABI counterpart of the public CLI's
  `--max-cycles`). Each grant's budget is clamped to the turns still allowed, so
  a live guest loop exhausts the limit and stops with a diagnostic instead of
  spinning the kernel.
- The kernel is stopped for one of three reasons. The ordinary stop follows a
  terminal result from `ruscv_machine_result`. A classified host failure stops it
  too: an ABI error, a kernel clock that disagrees with the Machine cursor, a
  grant boundary that consumed no modeled time while the run is still live, an
  exhausted grant or turn bound, or a refused direct-memory invalidation. That
  is a host decision and never a ruscv-sim result. The third is the deliberate
  `--stop-after-grants` control, which stops the kernel before any result to
  prove that a host stop is not one; it must never pass. Every stop runs the
  drain first when a transaction was retired (nb D9).
- `--dmi` selects direct memory for the target table and
  `--dmi-invalidate-every <grants>` (which requires it) asks the RAM target to
  invalidate every grant after each Nth Machine grant: the target issues the
  backward `invalidate_direct_mem_ptr` call, which the initiator forwards to
  the ABI. Invalidation is target-originated here even though the driver
  schedules it, so the full backward path is exercised. At exit the host
  reports the RAM target's `b_transport` count and the forwarded-invalidation
  count.
- The process exit status is the guest exit code for a terminal exited run and 1
  for a failed run or a host-classified failure. The early-stop control, which
  leaves no run result at all, exits 2.

## Build and version pins

- The development image builds Accellera SystemC 3.0.2 at commit
  `70b0fc8e4a74acc677b0fc73cea08f940c2115d5` with C++17, and fails if the
  checked-out commit or the `SYSTEMC_3_0_2` version macro does not match the pin.
  The static library and TLM-2.0 headers install to `/opt/systemc`
  (`SYSTEMC_HOME`). The image also compiles and runs a small SystemC/TLM program,
  so a version-printing executable cannot stand in for a working kernel.
- [`integration/systemc/CMakeLists.txt`](../../integration/systemc/CMakeLists.txt)
  requires the installed `SystemCLanguage` package at 3.0.2 and the ruscv-sim C
  ABI static library built with `cargo rustc --lib --features capi --crate-type
  staticlib`. The guest suite's SystemC host links both statically.
- `cargo build`/`test --all-features` need no C++ toolchain and no SystemC. Only
  `scripts/test_systemc_smoke.sh`, which the `SystemC transport smoke` CI job
  runs inside the development image, needs them.
- Linux x86-64 only.

`scripts/test_systemc_smoke.sh` runs `atomic_tohost_exit` under `rv64imafdc` and
`fd_fma` under `rv64imafd` through the public CLI and through the SystemC host
and requires the same exit code and a byte-identical commit log. It runs both
guests again with `--dmi` and requires the same facts with a `b_transport`
count strictly below the same guest's non-direct pass, and one guest with
`--dmi-invalidate-every` under an unchanged commit log with the scheduled
invalidation count equal to grants/N and the ABI-forwarded count covering it.
It then runs both guests at `--quantum 1` and `--quantum 1024`, with and without
`--dmi`, and requires the native exit code and commit log in every pass, a
number of kernel-clock checks equal to the number of grants, and a grant count
at `--quantum 1024` strictly below the same guest's `--quantum 1` count. It runs
both guests under `--nb` on every return path at `lambda` = 0 and
`lambda` = 1500000 fs, requires a non-zero `BEGIN_REQ` count equal to the
applied-access count and the same exit code and commit log, compares the
`lambda` = 0 `--grant-trace` files with the parity traces, and checks the three
D10 sums from the host's exit totals at `lambda` = 1500000. It repeats the
`--dmi` `lambda` = 0 passes on `accepted` and `completed`, and requires the
`--dmi` `lambda` = 1500000 `accepted` pass to serve fewer AT transactions than
the non-direct pass. It also requires the `--nb --quantum` refusal and the
non-blocking usage errors. It runs `hello` under `--uart-target in-tree` and
`--uart-target host` at the baseline, with `--dmi`, with `--nb` and with
`--quantum 1024`, requiring the native exit code and commit log, zero host
device transactions for the in-tree selection, a non-zero host transaction
count equal to its reads plus writes, exactly `Hello!\n` in the target's bytes
and on its `uart-console:` channel, and a repeated host pass with identical
bytes, commit log and exit code. The two existing guests run again under
`--uart-target host` and must keep their facts with zero device transactions,
because neither touches the UART. It runs `htif_u3_console` under
`--htif-target in-tree` and `--htif-target host` at the baseline, with `--dmi`,
with `--nb` and with `--quantum 1024`, requiring the native exit code and commit
log, zero host HTIF transactions for the in-tree selection, a non-zero host
transaction count equal to its reads plus writes, the guest's three console
words and exit word as the accepted value sequence, exactly `Hi\n` on the
`htif-console:` channel, the matching in-tree pass's grant, consumed and turn
facts, and a repeated host pass with identical values, console, transactions,
facts and commit log. The two existing guests run again under `--htif-target
host` and must keep their facts with zero HTIF transactions, because neither
touches the endpoint. It runs the two quantum fault-option controls under
`--quantum 1024`, the five non-blocking ones, the three UART device ones and the
three HTIF ones in their stated mode, and requires each to fail with its own
assertion. It passes the
same turn bound as the CLI's `--max-cycles` and wraps every run in a wall-clock
`timeout`, so neither a guest loop nor a host defect can hang CI.

## Open items

- SystemC-modeled interrupt lines, which need their own contract.
- Device delay and host time: a device target that annotates either, which needs
  the turn completion bound and the grant horizon extended with it.
- HTIF fromhost, console input and syscall proxying: the endpoint has no
  host-to-guest direction, the ABI has no entry that publishes a fromhost word,
  and the crate decodes no HTIF device code (htif D3/D5). These are retained
  directions of the [current contract](../dev-plan.md) §5 and need their own
  contract.
- More than one outstanding transaction, on the non-blocking path or a device
  window, and pipelining.

## Direct-memory grant, permission, latency, invalidation, lifetime and FFI

This section states the SystemC host's rules for the direct-memory topics
D14 makes normative here — grant, permission, latency, invalidation,
lifetime, version-clock persistence across reset, and the FFI rules — under
the direct-memory contract. The Rust-side rules live in
[its D3–D10](../archive/milestones/tlm-direct-memory-interface.md) and in the header's direct-memory
obligations; this section binds the host of
[`integration/systemc`](../../integration/systemc) to them and states what only
this host can state.

**Grant.** `RamTarget` sets the `dmi_allowed` hint on a completed non-atomic
`b_transport` transaction, and never on an atomic one, an atomic-extension
payload, a debug transfer or an error status. That hint is the only licence
for the query. `register_get_direct_mem_ptr` grants the whole 4 KiB chunk that
contains the queried address — inclusive start and end, the chunk's host
pointer, read-write access whatever the query asked (D13 grants one storage
chunk read-write), zero read and write latency —
and denies a query whose chunk does not exist, so a query never allocates
storage. The IEEE 1666 `tlm_dmi` fields cross the C ABI verbatim: the
initiator's `serve_get_direct_mem_ptr` maps `true` to `RUSCV_DMI_GRANTED`,
`false` to `RUSCV_DMI_DENIED`, and a contained C++ exception (or an
unreachable target) to `RUSCV_DMI_FAILED`, the contract's contained-exception
case, which is an unknown completion, never a denial. The granted access
values equal the `RUSCV_DMI_ACCESS_*` constants (statically asserted).

**Permission.** A granted range is read-write because the chunk is ordinary
RAM: every `b_transport` command the target would complete with
`TLM_OK_RESPONSE` over that range completes with `TLM_OK_RESPONSE`, so the
direct path cannot observe a fault `b_transport` would not produce
(direct-memory D10). A denied query caches nothing and changes nothing.

**Latency.** Both granted latencies are zero. The host's time resolution is
1 fs, so a non-zero grant latency would still cross the ABI as exact
femtoseconds and be charged through the Machine's one accounting path
(direct-memory D6); this target annotates zero delay in `b_transport` and grants
the same zero, so a blocking run is time-equivalent with `--dmi` and without it.
Under `--nb` with a positive `lambda`, direct copies bypass AT transactions and
therefore their latency charge; the D10 sum checks therefore use non-direct
runs, and the direct pass only compares the AT transaction count.

**Invalidation.** The initiator registers its backward
`invalidate_direct_mem_ptr` on its initiator socket
(`register_invalidate_direct_mem_ptr`), and the target module registers its
forward `get_direct_mem_ptr` on its target socket; every forward of the
backward callback calls
`ruscv_machine_invalidate_direct_mem`, the one ABI entry accepted inside a
callback and on a poisoned handle, always on the kernel thread that owns the
handle. A backward call while no handle is bound is a no-op: after `destroy`
the handle's target wrapper and its registry no longer exist, so no live grant
can still name them. A refused forward — the ABI rejected the invalidation —
ends the run as a host failure. `--dmi-invalidate-every <grants>` asks the
target to issue a whole-space backward invalidation after each Nth Machine
grant, so the target→initiator→ABI path is exercised end to end; the smoke
requires the scheduled count to equal grants/N exactly and the forwarded count
to cover it (the extra forward is `RamTarget::transport_reset`'s own
invalidate-all before it discards chunks, which traverses the same backward
path).

**Lifetime.** `RamTarget` invalidates every registration before it frees,
moves or discards granted storage: its `transport_reset` calls
`invalidate_direct_mem_ptr(0, UINT64_MAX)` before `chunks_.clear()`, atomic
with respect to new grants because the module runs on the single SystemC
kernel thread (direct-memory D13). Between resets a granted chunk pointer
stays valid; chunk allocation never moves existing storage. No pointer, grant
or cached region survives above the C ABI: the Machine's adapter owns its own
cache and drop rules (direct-memory D9), and this host adds no second copy.

**Version-clock persistence across reset.** Each chunk byte carries one
committed-write version, and the target's version clock keeps running across
`transport_reset`: the records go with the discarded bytes, the clock does not
restart, so no committed-write version repeats after a reset (direct-memory
D10/D13). A direct store deliberately leaves a byte's version in place — that
is why the Machine's write-suspension gate exists — so this target's
load-reserved snapshot and store-conditional check behave identically with
`--dmi` on and off.

**FFI rules.** The direct-memory additions to the C ABI (`get_direct_mem_ptr`,
`dmi_allowed`, `ruscv_direct_mem`, the `RUSCV_DMI_*` constants,
`ruscv_machine_invalidate_direct_mem`) follow the header's layout rule (fields
are appended; earlier offsets never move) and its direct-memory D10
obligations, which bind this target exactly as they bind a C target: granted
storage is `std::atomic<uint8_t>` and every access to it, including
`b_transport`, `transport_dbg` and the granted pointer, is a relaxed atomic
byte access; the pointer stays valid until a covering invalidation; and the
invalidate-before-free order above holds. The query and its answer carry no
C++ object, pointer or operation callback: `ruscv_direct_mem` is plain data,
and the host copies the `tlm_dmi` fields into it value by value. (The
pre-existing callback table is C-linkage function pointers, as before; this
contract adds none.)

## Source and test map

- [`src/capi`](../../src/capi) and [`include/ruscv_sim.h`](../../include/ruscv_sim.h):
  the C ABI facade, its header, and the device table with
  `ruscv_machine_attach_device`.
- [`integration/systemc`](../../integration/systemc): the CMake project, the RAM
  target, the UART and HTIF device targets, the initiator bridge and the grant
  loop.
- [`tests/bare-metal-riscv-test/rv64i/htif_u3_console.S`](../../tests/bare-metal-riscv-test/rv64i/htif_u3_console.S):
  the endpoint guest, which reads the window, writes three console bytes and the
  standard exit word for code 0 and self-loops.
- [`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh): native
  comparison, the `--dmi` and `--dmi-invalidate-every` passes, the `--quantum`
  passes, the five `--nb` return paths at both latencies, the `--dmi` AT
  passes, the `--uart-target` passes with their console and transaction
  assertions, the `--htif-target` passes with their value, console and parity
  assertions, the `--nb --quantum` refusal, the usage errors, the two quantum
  and five non-blocking fault-option controls, the three UART and three HTIF
  device fault-option controls and the early-`sc_stop` control.
- [`scripts/test_capi_smoke.sh`](../../scripts/test_capi_smoke.sh): the plain-C
  compile-and-link smoke of the same ABI, including its direct-memory and
  device-attach cases.
- [`src/blocking/device.rs`](../../src/blocking/device.rs) and
  [`src/blocking/in_tree.rs`](../../src/blocking/in_tree.rs): the device target
  vocabulary, the adapter's validation and the in-tree UART and HTIF targets,
  with `src/blocking/device_tests.rs`, `src/executor_device_tests.rs` and
  `src/machine/tests.rs` as their test side and `src/capi/tests.rs` for the ABI
  table.
- [`tests/tlm_u4_guest_parity.rs`](../../tests/tlm_u4_guest_parity.rs): guest
  parity on the in-tree Rust TLM route, now three passes per guest.
- [`tests/direct_mem_cli.rs`](../../tests/direct_mem_cli.rs),
  [`tests/direct_mem_u3_guests.rs`](../../tests/direct_mem_u3_guests.rs) and
  [`src/blocking/direct_mem_tests.rs`](../../src/blocking/direct_mem_tests.rs):
  the direct-memory selection, guest and adapter suites on the Rust side.
