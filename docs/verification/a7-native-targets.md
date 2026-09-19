# A7 T2 native target adapters

**Status:** T2 component verification only. This record does not claim that the
public Hart, Runner, or either public facade has migrated to `PhysicalAccess`.

## Boundary delivered

The native non-atomic adapters reuse the T1 request/response vocabulary and are
intended to be placed behind `ValidatedPhysicalAccess`:

- `NativeRamBackend` is a raw-byte view over a caller-supplied
  `Arc<Mutex<SimpleMemory>>`. It performs one contiguous checked slice read or
  write, does not apply typed alignment or integer extension, and does not copy
  the RAM buffer.
- `NativeSystemBusBackend` is a shared-lock view over a caller-supplied
  `Arc<Mutex<SystemBus>>`. It uses the same `SystemBus`, RAM, UART, and HTIF
  callback objects as the existing typed view; it is not a second bus or a
  write-through mirror.
- `SimpleMemory::read_bytes` and `write_bytes` are raw target helpers only.
  Existing `MemoryInterface` methods retain their typed alignment and error
  behavior. The native write validates the complete span before one slice copy.

Native routing is RAM-first only when RAM accepts the complete span. A request
that crosses a target boundary is rejected rather than byte-stitched. RAM
accepts Fetch, DataRead, and DataWrite at widths 1/2/4/8. UART retains the
native public `0x1000_0000..0x1000_00ff` window, accepts only byte DataRead and
DataWrite, and leaves reserved offsets at zero/ignored. HTIF is a raw exact
8-byte DataRead/DataWrite endpoint at `0x4000_8000`; it returns eight zero bytes
on read and invokes its callback once on a successful write. Neither device
accepts Fetch.

The raw map's full-span rules are deliberately separate from the old typed
`SystemBus` compatibility methods. In particular, typed HTIF dword calls at
`base+1..7` and the characterized SC.W-at-HTIF+4 width debt remain unchanged.
RAM size 4 versus 8 at the HTIF overlap retains RAM-first/fallback priority.

Target rejection preserves the request descriptor and span through the T1
validated boundary. Lock poison maps to a backend failure; injected backend
host, protocol, and unknown-completion results remain distinct. Malformed raw
responses are still rejected by T1 response-length/binding validation; no
retry or string-based reclassification is introduced.

## Evidence and limits

`tests/a7_native_targets.rs` covers last-valid, one-past, overflow, unsupported
width/category, cross-target spans, RAM/HTIF overlap, raw fetch/read/write
widths, raw byte order, shared old/new RAM and device visibility, UART RX/status
and TX side-effect snapshots, HTIF callback snapshots, exactly-once successful
effects, empty host inspection semantics, poisoned locks, injected backend
categories, unknown completion, and malformed responses. The focused T2 row is:

```text
cargo test --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests
```

These are adapter/component tests. Passing them proves neither Hart call-site
migration nor public ELF behavior through the new port. T3 must still connect
fetch and ordinary integer/FP accesses, keep Hart alignment/extension/trap and
retirement ownership, and provide the explicit legacy atomic bridge. The
connection must also avoid lock re-entry: the legacy core currently holds its
outer data-memory mutex across a complete helper call, while the shared native
SystemBus adapter acquires the same bus lock for a raw transaction. T3 must
choose one bounded locking seam rather than reacquiring that mutex through the
new view. No Hart, AMO/LR/SC, reservation, MMU, TLM, or public-facade wiring is
part of T2.
