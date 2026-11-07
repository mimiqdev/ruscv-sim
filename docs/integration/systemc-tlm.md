# SystemC/TLM Boundary

**Status:** Implemented for the selected blocking-transport mapping, including
direct memory and the quantum selection

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
Bounded by
ADR-0002, ADR-0003 and ADR-0004 with their amendments, and by
[`docs/dev-plan.md`](../dev-plan.md), which governs where this page disagrees
with them.

**Last reviewed:** 2026-11-07

The repository implements one blocking-transport integration: a C ABI facade
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
fault-option controls of its §4. The Current contract selects non-blocking
transport through suspending callbacks; until its units land, this page
describes only the blocking mapping, and TLM-modeled devices stay out of scope.

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
| Hart local offset (quantum D11) | The `b_transport` `delay` argument |
| Quantum selection (quantum D12) | `ruscv_target.quantum_ticks` |
| Counted sync ending a grant (quantum D12) | The grant returns, then `wait(consumed)` |
| Direct RAM fast path | `get_direct_mem_ptr`/`invalidate_direct_mem_ptr` via the C ABI (direct-memory D12) |

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
- The transport reset invalidates every direct-memory registration on the
  bound initiator before it discards any chunk — atomically with respect to
  new grants, because the module runs on the single kernel thread — then
  discards every byte and every committed-write version record, so the
  Platform's zero-fill after it starts from an empty target. The version
  clock keeps running across the reset, so no committed-write version repeats
  (direct-memory D10/D13). It returns the acknowledgement the Platform
  requires (D6).
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
  per-transaction maximum is zero because this model adds no delay.
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
  `RUSCV_ERROR_REENTRANT`, except `ruscv_machine_invalidate_direct_mem`, which
  the direct-memory backward path uses (direct-memory D12). A callback must
  not wait or advance SystemC time; this initiator does neither.
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
and commits only the amount a grant returns: the host `wait()`s exactly the
returned consumed amount and never advances SystemC time on its own. After the
wait it checks that the kernel clock equals the grant's committed end, and a
mismatch ends the run as a host failure. This target adds no transport delay, so
a run stays time-equivalent to native (ADR-0004 §4.4, §5.3).

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

`--fault-keep-offset` and `--fault-decrease-delay` are the two fault-option
controls of the contract's §4, in the style of `--stop-after-grants`. Both
require `--quantum` and they cannot be combined. They exist because the mutation runner builds only Rust, so
a C++ fault cannot be a control row there (quantum D14). Neither is a normal
host, and a run that passes with either one set is a smoke failure:

- `--fault-keep-offset` reports the full `delay` argument as an added delay. The
  run must report `result state=failed`, the failed boundary's `uncertain` flag
  must be 0, and its diagnostic must name the exceeded declared maximum, which
  is zero for this target.
- `--fault-decrease-delay` makes `RamTarget` lower `delay` below the offset it
  received. The run must report `result state=failed`, the failed boundary's
  `uncertain` flag must be 1, and its diagnostic must name the outcome the
  target could not establish, which is the C ABI's unknown-completion
  classification.

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
  prove that a host stop is not one; it must never pass.
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
the two fault-option controls under `--quantum 1024` and requires each to fail
with its own assertion. It passes the same turn bound
as the CLI's `--max-cycles` and wraps every run in a wall-clock `timeout`, so
neither a guest loop nor a host defect can hang CI.

## Open items

- Non-blocking transport: selected by the Current contract, not yet
  implemented. Its D11 makes this page normative for suspension, the
  base-protocol mapping, the commit rule, retirement and the AT target mode
  once its units complete.
- TLM-modeled devices and SystemC-modeled interrupt lines, which need their
  own contract.

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
(direct-memory D6); this target simply annotates zero delay in `b_transport`
and grants the same zero, so a run is time-equivalent with `--dmi` and without
it.

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
  the C ABI facade and its header.
- [`integration/systemc`](../../integration/systemc): the CMake project, the RAM
  target, the initiator bridge and the grant loop.
- [`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh): native
  comparison, the `--dmi` and `--dmi-invalidate-every` passes, the `--quantum`
  passes, the two fault-option controls and the early-`sc_stop` control.
- [`scripts/test_capi_smoke.sh`](../../scripts/test_capi_smoke.sh): the plain-C
  compile-and-link smoke of the same ABI, including its direct-memory cases.
- [`tests/tlm_u4_guest_parity.rs`](../../tests/tlm_u4_guest_parity.rs): guest
  parity on the in-tree Rust TLM route, now three passes per guest.
- [`tests/direct_mem_cli.rs`](../../tests/direct_mem_cli.rs),
  [`tests/direct_mem_u3_guests.rs`](../../tests/direct_mem_u3_guests.rs) and
  [`src/blocking/direct_mem_tests.rs`](../../src/blocking/direct_mem_tests.rs):
  the direct-memory selection, guest and adapter suites on the Rust side.
