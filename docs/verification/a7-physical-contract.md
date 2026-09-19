# A7 T1 non-atomic physical contract

**Status:** T1 interface and focused contract tests only. This record is not a
Hart migration or native-adapter conformance report.

## Boundary delivered

`src/physical.rs` defines the transport-neutral non-atomic boundary and is
re-exported from `ruscv_sim` without changing `MemoryInterface`, core
constructors, executor helpers, or public runner behavior.

- `PhysicalRequest` carries an explicit `u64` `paddr`, `PhysicalWidth::Byte`,
  `Halfword`, `Word`, or `Doubleword`, and exactly one of `Fetch`, `DataRead`,
  or `DataWrite`.
- Data-write payloads are borrowed and must be exactly the selected width.
  Fetches and reads cannot carry a payload.
- `PhysicalResponse` carries an explicit address/width/category binding.
  Reads and fetches return exactly the requested raw bytes; writes return only a
  write acknowledgement.
- Bytes are passed in increasing physical-address order. The interface does
  not deserialize integers, sign/zero extend loads, interpret FP values, map
  traps, or expose an atomic envelope.
- `ValidatedPhysicalAccess` performs request validation and complete-span
  checking before one backend call, then validates binding, completion kind,
  and exact response length before returning success. Valid transfers use
  fixed-size inline storage and a borrowed write payload; error context is
  allocated only on failure paths.

A valid request whose `paddr + width - 1` wraps is a typed
`PhysicalAccessError::TargetRejected` with `RangeOverflow`, preserving the
request descriptor and span. A backend's refusal of a valid width/category,
unmapped span, permission denial, or target/device error uses the same target
category. Invalid width, zero width, payload mismatch, malformed response,
wrong binding, and contradictory completion are typed protocol/invariant
failures. Host/backend failures and unknown completions remain distinct. An
unknown completion is terminal for this boundary: it is not retried and is not
converted to a guest trap or success.

## Native and legacy limits

The wrapper provides a native **validation guarantee** only for a backend
actually placed behind `ValidatedPhysicalAccess`. Implementing
`PhysicalBackend` alone is not a conformance claim: a backend still has to
provide target all-or-nothing behavior and the target-side fault context
required by ADR-0002. Existing `MemoryInterface` implementations and the
native `SystemBus` are not silently certified, adapted, or routed through this
module. Unknown legacy completion behavior remains unknown until a later
explicit adapter proves it.

T1 does not wire the port into `RiscvCore`, `load_and_run`, `RiscVSimulator`,
`SystemBus`, native devices, MMU/TLM, or any Hart fault/retirement path. It does
not alter cycle budgets, ISA behavior, reservation state, AMO/LR/SC behavior,
UART/HTIF behavior, or host `write_mem` semantics. T2 supplies native target
adapters; T3 supplies Hart connection and the explicitly legacy atomic bridge.
No atomic operation is represented by this T1 interface.

## Focused evidence

`tests/a7_physical_contract.rs` uses a spy/fake backend to verify complete
fetch/read/write acknowledgement for all widths, asymmetric raw-byte order,
pre-dispatch request rejection, non-wrapping span overflow, target metadata,
host failure, short/long responses, wrong category and request binding,
contradictory completion, backend protocol failure, and single-call unknown
completion handling.

## Exact-head verification

At committed HEAD
`15ad21f759a53f6cac060fde746c40928293f129`, the following recorded checks
passed:

```text
cargo test --test a7_physical_contract                         11 passed
cargo test --test a7_migration_characterization \
  --test memory_bounds --test public_behavior \
  --test a6_task3_core_trap_test                              85 passed
cargo fmt --all -- --check                                    passed
cargo check --all-features                                    passed
cargo clippy --all-features --all-targets -- -D warnings     passed
cargo test --all-features                                     passed
cargo doc --all-features --no-deps                            passed
git diff --check fc68fcfe39055bd8f13255e0199bb47c7d3c9dee..HEAD passed
```

The focused and full rows were executed through `qing verify --check` at this
exact HEAD. No fresh project-ELF or ACT4 suite was required for T1, so this
record makes no such pass claim. The T0 characterization and A6 regressions
remain unchanged; the new port is still not wired into public execution.
