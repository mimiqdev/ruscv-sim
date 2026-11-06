# SystemC host integration

This CMake project is the pinned SystemC host for the blocking-transport smoke
(dev-plan D9). It builds one executable, `systemc_smoke`, which hosts one
ruscv-sim Machine through the C ABI and supplies the guest-RAM target:

- `ram_target.{h,cpp}`: a zero-delay blocking RAM target behind a
  `simple_target_socket`, with `b_transport`, `transport_dbg`, transport reset
  and the ruscv-sim atomic extension. Its chunk bytes are
  `std::atomic<uint8_t>`; it sets the `dmi_allowed` hint on completed
  non-atomic transactions, grants the containing chunk read-write at zero
  latency through `register_get_direct_mem_ptr`, and invalidates every
  registration before its transport reset discards chunks while keeping the
  version clock running (direct-memory D10/D13). `b_transport` leaves the
  `delay` argument unchanged, which carries the Hart's local offset: this model
  adds no delay (quantum D13).
- `ruscv_initiator.{h,cpp}`: the C ABI `ruscv_target` callbacks mapped onto a
  `simple_initiator_socket` bound to that target. `--dmi` selects the
  `get_direct_mem_ptr` query; the registered `invalidate_direct_mem_ptr`
  forwards to `ruscv_machine_invalidate_direct_mem`, the one ABI entry a
  target may call inside a callback. The callbacks pass the payload's
  `local_offset_fs` in as the `b_transport` delay argument and report only what
  the target returned above it (quantum D13).
- `ruscv_payload_extension.h`: the TLM-2.0 payload extension carrying the access
  category, the initiator and the atomic request and response.
- `main.cpp`: the `SC_THREAD` grant loop that starts each grant at
  `sc_time_stamp()`, waits the returned consumed time, and stops the kernel after
  a terminal `ruscv_machine_result` or a host-classified failure. It compares the
  kernel clock with `committed_end_fs` after every grant, which with a quantum
  selected also proves each grant return synced its local offset. The
  `--stop-after-grants` control stops it deliberately, without a result, and
  `--fault-keep-offset` and `--fault-decrease-delay` are the quantum contract's
  two fault-option controls.

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
       [--fault-keep-offset] [--fault-decrease-delay]
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

`--quantum <ticks>` selects temporal decoupling in Platform ticks (zero is off)
and travels in the C ABI target table (quantum D12/D13). The Machine may then
run ahead of the kernel clock by a bounded local offset; the callback's `delay`
argument carries that offset, `RamTarget` adds nothing to it, and the initiator
reports only the delay above it, so `committed_end_fs` stays exactly the amount
the kernel commits. A counted sync at a turn boundary ends the grant, so
`--quantum 1024` needs far fewer grants than `--quantum 1` for the same guest.

`--fault-keep-offset` and `--fault-decrease-delay` are the quantum contract's two
fault-option controls and require `--quantum`. The first reports the offset as
an added delay, above this target's declared maximum of zero; the second lowers
the target's delay below the offset it received. Each must fail the run with its
own diagnostic, and a control that passes is a smoke failure.

The end-to-end comparison with the native run is
[`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh); the
`SystemC transport smoke` CI job runs it inside the development image. The
smoke requires the direct pass's `b_transport` count strictly below the same
guest's non-direct pass and an unchanged commit log under the scheduled
invalidation.

## Limits

Linux x86-64 only. The RAM model annotates zero delay, so it does not model a
timed memory; the quantum defers the Machine's committed time by a bounded local
offset, but the target still adds no modeled delay of its own.
