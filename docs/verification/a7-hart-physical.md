# A7 T3 Hart physical-access wiring and legacy bridge

**Status:** T3 implementation and focused regression record. This is not a
claim of complete ADR-0002 atomic convergence, multi-Hart coherence, or the
T4/T5 facade/evidence rows.

## Wiring delivered

`RiscvCore` keeps the caller-provided instruction and data
`MemoryInterface` handles independent. `new_with_physical_access` and
`new_with_physical_ports` add independent raw fetch and raw data handles; they
do not merge those objects or copy storage.

`step_outcome` now has these deliberate routes:

| Hart operation | Route | Hart-owned work |
| --- | --- | --- |
| Instruction fetch | `PhysicalMemoryAdapter` -> validated `Fetch` request, width 4 | Guest address/alignment, one base conversion, little-endian decode, decode/legality and trap mapping |
| RV64I load/store | `PhysicalMemoryAdapter` -> validated `DataRead`/`DataWrite` request, width 1/2/4/8 | Effective address, alignment, extension/narrowing, retirement and original `mtval` |
| RV64F/RV64D load/store | The same raw data route | FP bit interpretation/NaN boxing and FP register effects |
| AMO/LR/SC | `LegacyTypedMemoryAdapter` -> existing `Executor`/ISA helpers | Existing typed widths, address conversion, reservation singleton and characterized outcomes |
| Non-memory instructions | Existing typed compatibility view | Existing executor behavior |

The two standard facades install separate validated raw fetch/data ports over
their existing domain:

* `load_and_run` uses two `NativeSystemBusBackend` views over the same
  `Arc<Mutex<SystemBus>>` (RAM, UART, HTIF and callback). Its Hart adapter is
  also given the native RAM range so a guest-aligned transfer at a
  non-aligned storage offset retains the old typed access fault.
* `RiscVSimulator::new` and flat `load_elf` use two `NativeRamBackend` views over
  the same `Arc<Mutex<SimpleMemory>>`, with the flat base subtraction performed
  once in the Hart adapter.

Host inspection and image placement remain on their existing typed APIs. The
new route does not wire MMU, TLM/DMI, interrupts, SystemC, a scheduler, a
second ISA dispatcher, or a second memory buffer.

## Lock and ownership order

The migration intentionally has two non-reentrant paths:

1. **Ordinary/fetch raw path:** Hart locks the selected raw-port handle, then
   the native backend locks its shared target (`SystemBus` or `SimpleMemory`),
   then the target may lock its RAM/UART child. `PhysicalMemoryAdapter` only
   reborrows the already-held port guard; it never acquires the legacy
   `data_mem` mutex.
2. **Legacy atomic path:** Hart locks the complete legacy `data_mem` handle,
   constructs `LegacyTypedMemoryAdapter`, and retains that guard across the
   entire existing Executor/AMO/LR/SC helper call. The helper may then lock
   the target's child storage and its existing global reservation mutex. It
   never drops the outer guard to call the raw port and never reacquires that
   same mutex through an adapter.

For the standard bus, raw order is port -> bus -> RAM/device; legacy order is
bus -> RAM/device plus the existing reservation lock. For flat RAM, raw order
is port -> RAM and legacy order is the typed handle -> RAM. The raw and legacy
views therefore share storage/device objects and lock domains without nesting
the same mutex. A bounded child-process regression kills and reaps the child
on timeout (`tests/a7_legacy_atomic_compat.rs::legacy_lock_reentry_child`).
Reservation-sensitive tests in that file take an explicit process-local
serialization mutex; the fixture makes no concurrent-safety claim.

## Compatibility and fault boundary

The typed legacy `SystemBus` methods retain start-address-only HTIF behavior,
including interior calls and the characterized SC.W-at-HTIF+4 dword callback.
Raw HTIF is stricter: one complete eight-byte endpoint, no Fetch, and full-span
validation. UART remains the public byte-only 0x100-byte window. Flat host
`write_mem` remains the byte-loop API, including visible successful prefixes
on failure. The checked flat storage-offset alignment rejection remains after
the single base conversion. The native facade applies the same rule only
inside its configured RAM range, without changing the generic raw backend's
ability to transfer unaligned physical spans or changing guest physical
addresses.

A target rejection is converted at the Hart boundary to the original
fetch/load/store access cause (1/5/7) and original architectural address.
Malformed and host failures become `SimulatorFailure`; unknown completions
also become `SimulatorFailure` but are retained as an explicit terminal Hart
state. A later `step_outcome` replays that unresolved diagnostic without
fetching or issuing another physical request. Only explicit host resolution
(`clear_unresolved_physical_access`) or reconstruction with a new core/domain
may make execution reusable; uncertainty is never silently retried. These
failures do not fabricate trap state, retirement, or a guest exit. Misaligned
guest accesses are checked before the raw port, so the spy sees zero physical
requests for those attempts. Successful MMIO effects are not rolled back after
the target has completed.

The legacy route deliberately retains the T0 defects: global exact-address
reservation state, no scalar/FP/host-write invalidation, reset retention,
legacy AMO width behavior, and SC write-fault clear behavior. No new
reservation, invalidation, reset clear, or atomic capability denial was added.

## Focused evidence

The T3 tests are:

```text
cargo test --test a7_hart_physical --test a7_legacy_atomic_compat
cargo test --test a7_hart_physical --test a7_legacy_atomic_compat \
  --test amo_test --test a6_task3_core_trap_test --test trap_test \
  --test csr_access_test --test mret_conformance_test
```

They cover raw fetch/ordinary integer/FP traces, endian/extension/FP results,
pre-port alignment, native non-aligned-base fetch/load/store faults, physical
target causes, host/protocol/unknown failures including the no-retry terminal
state, shared storage, ordinary-store→legacy AMO/LR call order,
legacy-write→raw-load/fetch/signature visibility, scalar/FP/rejected/host
writes around LR/SC, faulting SC, LR→reset→replacement-storage SC with the
baseline global address key, legacy HTIF, and a timeout-bounded lock-reentry
child. T0/T1/T2 component tests remain separate evidence; T4 public
equivalence and T5 fresh guest/ACT4 evidence remain future rows.

No performance acceptance gate is added. Allocation/lock-cost observation for
this adapter was not benchmarked in T3 and is not reported as a no-regression
claim.
