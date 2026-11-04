# SystemC host integration

This CMake project is the pinned SystemC host for the blocking-transport smoke
(dev-plan D9). It builds one executable, `systemc_smoke`, which hosts one
ruscv-sim Machine through the C ABI and supplies the guest-RAM target:

- `ram_target.{h,cpp}`: a zero-delay blocking RAM target behind a
  `simple_target_socket`, with `b_transport`, `transport_dbg`, transport reset
  and the ruscv-sim atomic extension.
- `ruscv_initiator.{h,cpp}`: the C ABI `ruscv_target` callbacks mapped onto a
  `simple_initiator_socket` bound to that target.
- `ruscv_payload_extension.h`: the TLM-2.0 payload extension carrying the access
  category, the initiator and the atomic request and response.
- `main.cpp`: the `SC_THREAD` grant loop that starts each grant at
  `sc_time_stamp()`, waits the returned consumed time, and stops the kernel only
  after `ruscv_machine_result` reports a terminal result.

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
```

The exit status is the guest exit code for a terminal exited run, 1 for a failed
run, and 2 for a host stop before a terminal ruscv-sim result.

`--max-turns` bounds completed turns, the C ABI counterpart of the public CLI's
`--max-cycles` (default 100000). Each grant's budget is clamped to the turns
still allowed, so a live run that exhausts the limit stops with a diagnostic
instead of spinning the kernel on a guest loop. `--max-grants` is a second net
over the same run.
`--stop-after-grants` exists for the early-`sc_stop` control: it stops the kernel
before a run result, which must never be accepted as a pass.

The end-to-end comparison with the native run is
[`scripts/test_systemc_smoke.sh`](../../scripts/test_systemc_smoke.sh); the
`SystemC transport smoke` CI job runs it inside the development image.

## Limits

Linux x86-64 only. This project selects blocking transport without DMI or
temporal decoupling, and the RAM model annotates zero delay, so it does not
model a timed memory.
