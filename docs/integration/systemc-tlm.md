# SystemC/TLM Boundary

**Status:** Implemented for the selected blocking-transport mapping

**Authority:** Architectural constraint under ADR-0002, ADR-0003 and ADR-0004 and
their 2026-11-04 blocking-transport amendments. [`docs/dev-plan.md`](../dev-plan.md)
governs where this page disagrees with it.

**Last reviewed:** 2026-11-04

The repository implements one blocking-transport integration: a C ABI facade
([`src/capi`](../../src/capi), [`include/ruscv_sim.h`](../../include/ruscv_sim.h))
and a pinned SystemC host ([`integration/systemc`](../../integration/systemc)).
This page records that mapping, its ownership and thread rules, its error and
time mapping, its stop behavior, and its build pins. DMI, non-blocking
transport, temporal decoupling and TLM-modeled devices stay out of scope, as the
contract's §5 records.

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
| Machine grant | Bounded grant over the C ABI, then `wait(consumed)` |
| Direct RAM fast path | Not selected; DMI stays a later contract |

`tlm_generic_payload` and `sc_time` stay outside the Rust side and outside ISA
instruction functions. The only Rust/C++ boundary is the C ABI.

## RAM target module

[`ram_target.h`](../../integration/systemc/ram_target.h) is a zero-delay
blocking RAM target with one `tlm_utils::simple_target_socket`:

- `b_transport` serves reads, writes and every atomic, and always annotates zero
  delay (`dev-plan` D7). A write is applied as one indivisible semantic write
  before it returns.
- `transport_dbg` serves image installation and the host views with no delay, no
  side effects beyond the bytes and no transaction accounting. It returns the
  bytes transferred; a short transfer is a failed debug transport.
- The transport reset discards every byte and every committed-write version, so
  the Platform's zero-fill after it starts from an empty target. It returns the
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
  statuses are completions; any other value is reported as incomplete.
- The target's address, command and data length are echoed back into the ABI
  payload, so a target that changed the request is visible to ruscv-sim and is
  classified as a protocol failure by the blocking adapter rather than here.
- The atomic extension's request half is copied into the TLM extension and its
  response half is copied back into the ABI payload. No function pointer and no
  C++ object crosses the ABI.
- The annotated delay is copied as exact femtoseconds, and the declared
  per-transaction maximum is zero because this model annotates zero delay.
- A debug request requires a complete transfer and an unchanged address, command
  and length; a short transfer or any changed request field is a failed debug
  transfer.

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

## FFI ownership and threads

- Every handle belongs to the thread that created it. A call from any other
  thread fails with `RUSCV_ERROR_NOT_CREATOR_THREAD` and changes no state. The
  SystemC host creates and drives the handle on the SystemC kernel thread, so the
  rule holds even though SystemC runs the thread as a coroutine.
- Target callbacks run synchronously inside a ruscv-sim call on that thread. A
  callback that calls back into any ABI handle fails with
  `RUSCV_ERROR_REENTRANT`. A callback must not wait or advance SystemC time; this
  initiator does neither.
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
one nanosecond (`ruscv_machine_create`).

Each grant starts at the current `sc_time_stamp()`. The kernel owns global time
and commits only the amount a grant returns: the host `wait()`s exactly the
returned consumed amount and never advances SystemC time on its own. After the
wait it checks that the kernel clock equals the grant's committed end, and a
mismatch ends the run as a host failure. Transport delay annotates zero for this
target, so a run stays time-equivalent to native (ADR-0004 §4.4, §5.3).

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
  grant boundary that consumed no modeled time while the run is still live, or an
  exhausted grant or turn bound. That is a host decision and never a ruscv-sim
  result. The third is the deliberate `--stop-after-grants` control, which stops
  the kernel before any result to prove that a host stop is not one; it must never
  pass.
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
and requires the same exit code and a byte-identical commit log. It passes the
same turn bound as the CLI's `--max-cycles` and wraps every run in a wall-clock
`timeout`, so neither a guest loop nor a host defect can hang CI.

## Open items

- DMI region qualification, lifetime and invalidation.
- Quantum, temporal decoupling and synchronization policy.
- Non-blocking transport, which needs its own timing contract.

## Source and test map

- [`src/capi`](../../src/capi) and [`include/ruscv_sim.h`](../../include/ruscv_sim.h):
  the C ABI facade and its header.
- [`integration/systemc`](../../integration/systemc): the CMake project, the RAM
  target, the initiator bridge and the grant loop.
- [`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh): native
  comparison and the early-`sc_stop` control.
- [`scripts/test_capi_smoke.sh`](../../scripts/test_capi_smoke.sh): the plain-C
  compile-and-link smoke of the same ABI.
- [`tests/tlm_u4_guest_parity.rs`](../../tests/tlm_u4_guest_parity.rs): guest
  parity on the in-tree Rust TLM route.
