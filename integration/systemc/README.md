# SystemC host integration

This CMake project is the pinned SystemC host for the transport smoke
(dev-plan D9; nb D7/D8; device D9; htif D8). It builds one executable,
`systemc_smoke`, which hosts one ruscv-sim Machine through the C ABI and
supplies the guest-RAM target and, under `--uart-target host` and
`--htif-target host`, the UART and HTIF device windows:

- `ram_target.{h,cpp}`: a zero-delay blocking RAM target behind a
  `simple_target_socket`, with `b_transport`, `transport_dbg`, transport reset
  and the ruscv-sim atomic extension. Its chunk bytes are
  `std::atomic<uint8_t>`; it sets the `dmi_allowed` hint on completed
  non-atomic transactions, grants the containing chunk read-write at zero
  latency through `register_get_direct_mem_ptr`, and invalidates every
  registration before its transport reset discards chunks while keeping the
  version clock running (direct-memory D10/D13). `b_transport` leaves the
  `delay` argument unchanged, which carries the Hart's local offset: this model
  adds no delay (quantum D13). Under `--nb` it also registers its AT
  `nb_transport_fw` mode: every path applies the access once at `BEGIN_REQ`,
  schedules its phases through a `peq_with_cb_and_phase`, reports its
  `BEGIN_REQ` and applied-access counts and whether a transaction is still open,
  tracks the phases it has scheduled but not delivered so the host's drain can
  wait on them, and withholds a transport reset acknowledgement while a
  transaction is open (nb D8/D9).
- `ruscv_initiator.{h,cpp}`: the C ABI `ruscv_target` callbacks mapped onto a
  `simple_initiator_socket` bound to that target. `--dmi` selects the
  `get_direct_mem_ptr` query; the registered `invalidate_direct_mem_ptr`
  forwards to `ruscv_machine_invalidate_direct_mem`, the one ABI entry a
  target may call inside a callback. The callbacks pass the payload's
  `local_offset_fs` in as the `b_transport` delay argument and report only what
  the target returned above it (quantum D13). A `may_suspend` payload runs the
  initiator-owned base protocol with one outstanding transaction instead: the
  driver `SC_THREAD` sends `BEGIN_REQ`, records backward phases with their
  effective host time, sends `END_RESP` after a forward `BEGIN_RESP`, and
  reports `d = e` (nb D7). Every wait of that process is bounded by
  `max_delay_fs + 1` fs: a transaction that is not complete at the timeout is
  retired as an unknown completion, its storage is retained until the target
  lets go of it, and any phase that then takes effect is ignored and counted
  (nb D9).
- `ruscv_payload_extension.h`: the TLM-2.0 payload extension carrying the access
  category, the initiator and the atomic request and response.
- `uart_target.{h,cpp}`: the host-served UART window (`--uart-target host`,
  device D9) as a plain callback object behind `ruscv_device`, not a SystemC
  module. It models the register set the suite's UART guest uses (RBR/THR, IER,
  IIR/FCR, LCR, MCR, LSR, MSR, SCR, DLL/DLM and 16-byte receive and transmit
  FIFOs), answers an accepted THR write with `RUSCV_TLM_OK` plus one
  `RUSCV_DEVICE_EVENT_UART_TRANSMIT`, echoes each transmitted byte as one
  `uart-console:` line, and counts transactions, reads, writes and bytes.
- `htif_target.{h,cpp}`: the host-served HTIF window (`--htif-target host`,
  htif D8) as the same kind of callback object. It answers a dword read with
  zero, answers an accepted dword write with `RUSCV_TLM_OK` plus one
  `RUSCV_DEVICE_EVENT_HTIF_WRITE` whose value is the guest's eight bytes,
  records each accepted value as one `htif-value:` line, renders the standard
  console device's byte (device 1, command 1, payload byte) as one
  `htif-console:` line, and counts transactions, reads, writes, values and
  `receive` calls.
- `main.cpp`: the `SC_THREAD` grant loop that starts each grant at
  `sc_time_stamp()`, waits `consumed_fs - host_elapsed_fs`, and stops the kernel
  after a terminal `ruscv_machine_result` or a host-classified failure. It
  compares the kernel clock with `committed_end_fs` after every grant, which with
  a quantum selected also proves each grant return synced its local offset. It
  records the startup thread check and writes `--grant-trace` lines. Every stop
  drains before it destroys the handle when a transaction was retired, and
  probes the transport reset first while the target still holds it (nb D9).
  The `--stop-after-grants` control stops it deliberately, without a result;
  `--fault-keep-offset` and `--fault-decrease-delay` are the quantum contract's
  two fault-option controls, the five non-blocking options are the same kind of
  control for the AT path (nb §4), `--uart-target` with the three device
  options selects and breaks the host UART (device §4), and `--htif-target`
  with the three HTIF options does the same for the endpoint (htif §4).

See [`docs/integration/systemc-tlm.md`](../../docs/integration/systemc-tlm.md)
for the mapping, ownership and time rules.

## Prerequisites

- Accellera SystemC 3.0.2 with its TLM-2.0 headers, installed with C++17. The
  development image builds the commit `docs/dev-plan.md` pins and installs it to
  `/opt/systemc`.
- A C++17 compiler and CMake.
- The ruscv-sim C ABI static library.

`cargo build` and `cargo test --all-features` never need any of this.

## Build and run

```bash
# The C ABI static library (defaults to target/debug/libruscv_sim.a).
cargo rustc --lib --features capi --crate-type staticlib

cmake -S integration/systemc -B target/systemc-build \
  -DCMAKE_PREFIX_PATH=/opt/systemc
cmake --build target/systemc-build --parallel

target/systemc-build/systemc_smoke guest.elf \
  --isa rv64imafdc --log-commits target/systemc-build/commits.log
```

`SYSTEMC_HOME`, when exported, is used without `-DCMAKE_PREFIX_PATH`. Pass
`-DRUSCV_SIM_LIBRARY=<path>` when the static library is elsewhere.

```
usage: systemc_smoke <elf> [--isa rv64ima|rv64imac|rv64imafd|rv64imafdc]
       [--log-commits <path>] [--turn-budget <turns>]
       [--horizon-fs <femtoseconds>] [--stop-after-grants <grants>]
       [--max-grants <grants>] [--max-turns <turns>]
       [--dmi] [--dmi-invalidate-every <grants>] [--quantum <ticks>]
       [--uart-target in-tree|host]
       [--htif-target in-tree|host]
       [--nb] [--nb-latency <femtoseconds>]
       [--nb-path accepted|updated|early|annotated|completed]
       [--grant-trace <path>]
       [--fault-keep-offset] [--fault-decrease-delay]
       [--fault-late-response] [--fault-uncounted-elapsed]
       [--fault-short-delay] [--fault-wait-blocking] [--fault-wait-debug]
       [--fault-uart-no-lsr] [--fault-uart-wait]
       [--fault-uart-event-on-fault]
       [--fault-htif-drop-event] [--fault-htif-wait]
       [--fault-htif-event-on-fault]
```

The exit status is the guest exit code for a terminal exited run and 1 for a
failed run or a host-classified failure. The `--stop-after-grants` early-stop
control, which deliberately leaves no run result, exits 2.

`--max-turns` bounds completed turns, the C ABI counterpart of the public CLI's
`--max-cycles` (default 100000). Each grant's budget is clamped to the turns
still allowed, so a live run that exhausts the limit stops with a diagnostic
instead of spinning the kernel on a guest loop. `--max-grants` is a second net
over the same run.
`--stop-after-grants` exists for the early-`sc_stop` control: it stops the kernel
before a run result, which must never be accepted as a pass.

`--dmi` offers the C ABI's `get_direct_mem_ptr` callback, so hinted accesses
copy through the RAM target's granted chunk pointers instead of `b_transport`.
`--dmi-invalidate-every <grants>` requires `--dmi` and makes the driver ask the
target to invalidate every grant after each Nth grant; the initiator's
backward call forwards each one to `ruscv_machine_invalidate_direct_mem` and
the forwarded count is reported at exit.

`--nb` makes the target declare that it suspends host time and selects the AT
base-protocol path for every `may_suspend` transaction (nb D7/D8). The
declaration travels in `ruscv_target.suspends_host_time`, the payload's
`may_suspend` and `host_elapsed_fs` carry the flag and the measured host time,
and each grant's own `host_elapsed_fs` is its accumulated `W` (nb D5/D6).
`--nb-latency <fs>` sets the target latency lambda and its declared
per-transaction maximum (default 0, at most 10^15 fs), and `--nb-path` selects
`accepted`, `updated`, `early`, `annotated` or `completed`. `--nb-latency` and
`--nb-path` without `--nb` are usage errors, and `--nb` with a non-zero
`--quantum` is refused by `ruscv_machine_create`. `--grant-trace <path>` writes
one line per grant with `consumed_fs`, `committed_end_fs` and `host_elapsed_fs`
for the smoke's per-grant comparisons (nb D10).

`--quantum <ticks>` selects temporal decoupling in Platform ticks (zero is off)
and travels in the C ABI target table (quantum D12/D13). The Machine may then
run ahead of the kernel clock by a bounded local offset; the callback's `delay`
argument carries that offset, `RamTarget` adds nothing to it, and the initiator
reports only the delay above it, so `committed_end_fs` stays exactly the amount
the kernel commits. A counted sync at a turn boundary ends the grant, so
`--quantum 1024` needs far fewer grants than `--quantum 1` for the same guest.

`--fault-keep-offset` and `--fault-decrease-delay` are the quantum contract's two
fault-option controls: each requires `--quantum`, they are taken one at a time,
and each must fail the run with its own diagnostic. The first reports the offset
as an added delay, above this target's declared maximum of zero; the second
lowers the target's delay below the offset it received. A control that passes is
a smoke failure.

The five non-blocking fault options are the AT path's controls (nb §4), also
taken one at a time. `--fault-late-response`, `--fault-uncounted-elapsed` and
`--fault-short-delay` require `--nb`; they delay the first AT access's final
phase to one tick past the declared maximum, hide the reported host elapsed time
while keeping the delay, and report the delay one femtosecond below it.
`--fault-wait-blocking` and `--fault-wait-debug` are blocking-mode controls that
must not be combined with `--nb`: they wait one tick in `b_transport` and in
`transport_dbg`. Each must fail the run with its own diagnostic, and a control
that passes is a smoke failure.

`--uart-target in-tree|host` selects the UART window's target (device D9). The
default `in-tree` keeps the Rust `Uart16550`; `host` creates a `UartTarget` and
attaches it through `ruscv_machine_attach_device` after create and before the
first load, so the Machine reads the window through the C ABI's device table.
The host target serves one device transaction per validated register access with
zero delay and zero host elapsed time. Any other value is a usage error.

The three device fault options are the device contract's §4 controls. Each
requires `--uart-target host` and is a usage error with the in-tree selection.
`--fault-uart-no-lsr` answers LSR without the transmit-holding-empty bit, so
`hello` exhausts its turn bound instead of passing; `--fault-uart-wait` waits one
Platform tick inside `b_transport` and reports it as host elapsed time, which
fails the access; `--fault-uart-event-on-fault` reports a transmit event with
`RUSCV_TLM_ADDRESS_ERROR`, which fails as a protocol failure of that access. A
control that passes is a smoke failure.

`--htif-target in-tree|host` selects the HTIF endpoint's target (htif D8). The
default `in-tree` keeps the Rust endpoint; `host` creates an `HtifTarget` and
attaches it through `ruscv_machine_attach_device` after create and before the
first load, either alone or beside a host UART target. The host target answers a
dword read with zero, answers an accepted dword write with `RUSCV_TLM_OK` and
one `RUSCV_DEVICE_EVENT_HTIF_WRITE` whose value is the guest's eight bytes, and
renders the standard console device's byte as one `htif-console:` line. The
published value is the response's eight write bytes, so leaving the data
unchanged reports the guest's own value. Any other selection is a usage error.

The three HTIF fault options are the HTIF contract's §4 controls. Each requires
`--htif-target host` and is a usage error with the in-tree selection.
`--fault-htif-drop-event` answers the exit word's write with `RUSCV_TLM_OK` and
no event while the console writes keep theirs, so no exit fact exists and the
guest's self-loop exhausts the turn bound; `--fault-htif-wait` waits one
Platform tick inside `b_transport` and reports it as host elapsed time, which
fails the access; `--fault-htif-event-on-fault` reports the exit write's event
with `RUSCV_TLM_ADDRESS_ERROR`, which fails as a protocol failure of that
access. A control that passes is a smoke failure.

At exit the host reports the initiator's late-phase count, its retirement count,
the forward `END_RESP` phases its drain sent with the host time of the one it
sent, and whether the target still holds a transaction. A normal `--nb` pass
reports zero late phases (nb D9). It also reports each device target's selection
(`host` or `in-tree`): the UART target's transaction count, its read and write
counts and the transmitted byte sequence, which the smoke checks against the
guest's fixed `Hello!\n`; and the HTIF target's transaction, read, write, value
and `receive` counts plus the rendered console sequence, which the smoke checks
against the endpoint guest's three console words, exit word and `Hi\n`.

The end-to-end comparison with the native run is
[`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh); the
`SystemC transport smoke` CI job runs it inside the development image. The
smoke requires the direct pass's `b_transport` count strictly below the same
guest's non-direct pass, an unchanged commit log under the scheduled
invalidation, `hello` to transmit exactly `Hello!\n` under
`--uart-target host` with the matching in-tree pass's facts,
`htif_u3_console` to publish the three console words and the exit word under
`--htif-target host` with the matching in-tree pass's grant and consumed facts,
zero host HTIF transactions for the in-tree selection and for the existing
guests under the host selection, and each of the six device fault options to
fail with its own assertion.

## Limits

Linux x86-64 only. The blocking RAM model annotates zero delay, so it does not
model a timed memory; the quantum defers the Machine's committed time by a
bounded local offset, but the target still adds no modeled delay of its own. The
AT mode has one outstanding transaction and a fixed latency lambda; a retired
transaction is drained at every stop, and one fault option at a time exercises
the retirement, late-phase, drain and fault paths. The host UART target models
one guest's register set, not a timed serial line: it transmits on the accepted
THR write and accepts injected bytes only through the ABI's `receive` callback.
The host HTIF target answers one endpoint window: its read answer is fixed at
zero, it renders the standard console device's putchar command and nothing else,
and it exposes no fromhost register or host-to-guest input. The C ABI exposes
no per-value sink for a non-exit HTIF write: `config.htif_write` has no C entry,
so the smoke proves the exit word's publication through the run stop (same turn
and exit code as the in-tree pass) and the console words' publication through
the target's accepted reports and the byte-identical commit log. A sink-level
console assertion would need a new ABI entry, which the append-only rule keeps
out of this unit.
