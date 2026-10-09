# A10 duration-scaled calibration workloads

**Status:** Current

**Authority:** Informational; development implementation, not P3 acceptance.

## Boundary and identities

The [A10 contract §4](../dev-plan.md#4-initial-workload-and-oracle-contract)
authorizes bounded deterministic variants with new source/linker/ELF identities.
Original `public-v1`, its P0 oracle, historical guests and smoke remain unchanged
correctness controls. No production source, ISA engine, board, memory map or
public API is changed. Interrupted short-anchor calibration prefixes are not
full sessions or baselines.

[`calibration-workloads-v2.json`](../../tools/a10/calibration-workloads-v2.json)
(`calibration-workloads/1`, version 2) maps all twelve anchors to explicit
execution and load fixture IDs. [`calibration-oracle-v2.json`](../../tools/a10/calibration-oracle-v2.json)
(`a10-calibration-oracle/1`, version 2) pins twenty-four linked ELFs, producer
identities, source/linker hashes, placement, complete instruction/effect order,
register/RAM/device results and signatures. The calibrated policy records this
new series; reports identify actual workload/mapping/oracle digests. Old smoke
or short-anchor results cannot become compatible variant baselines.

| Original anchors | Actual execution algorithm | Iterations | Exact retirements |
| --- | --- | ---: | ---: |
| fib, control_loop | ADD/branch triangular sum | 128 | 29 + 4×128 = 541 |
| sw, ram_loop | ordinary word store/load, read-back, triangular checksum | 64 | 31 + 7×64 = 479 |
| amo_w, mixed_w | ordinary W → AMO.W → LR.W/SC.W success → invalidated SC.W failure → suppressed-x0 failed SC | 24 | 31 + 20×24 = 511 |
| amo_d, lrsc_loop, sc_after_store_failure, mixed_d | same declared D sequence | 24 | 31 + 20×24 = 511 |
| hello, native_device | actual byte UART writes and checksum; RAM-tohost or fixed-HTIF final exit respectively | 64 | 30 + 5×64 = 350 |

These are new algorithms, not new results attributed to the old anchors. Every
mixed iteration writes -2, observes sign-extended old -2, adds 5 to get 3,
checks LR and successful SC, invalidates a fresh reservation with an ordinary
store, checks SC failure and verifies no failed-SC write. Neighbor bytes and
zero-filled scratch are checked. Original atomic anchors retain their original
operation-specific correctness coverage.

UART callback output is exactly 64 `A` bytes, checksum 4160, and ordered
immutable device events. Existing UART storage retains the first sixteen
transmitted bytes; the full callback output is separate from that FIFO. Its
full-FIFO LSR is zero, not a fabricated ready bit. Variants never poll a full
FIFO to manufacture work. Native-only flat cells remain N/A. The fixed-HTIF
variant adds the final event value 7 and declares guest/process exit 3.

## Independent derivation and own oracles

[`variant_specs.py`](../../tools/a10/variant_specs.py) declares exact assembly
slots and algorithm effects without executing a simulator or decoding linked
instructions. Linked opcodes supply instruction identity only. It checks static
slot consistency, bounded iterations/immediates/address arithmetic, count
formulas, exact source text, ELF placement and pinned producer identities.
The checked source/linker files are in
[`calibration-fixtures/`](../../tools/a10/calibration-fixtures/).

Every execution variant self-checks its checksum and relevant RAM/neighbors/BSS,
and writes three signature words: iterations, checksum and MINSTRET before the
reader retires. Six retirements remain including the reader and completing
exit store. No instruction writes MINSTRET or enters a trap. Attempts, completed
turns and retirements are equal only for these independently declared successful
sequences. Expected successful exit is 3, never silently converted to zero.

The shared [`P0 validator`](../../tools/a10/mod.rs) validates each actual capture,
including result/PC/count/signature, registers/RAM where public, full immutable
facts/atomic effects, UART/events and exact file logs. CLI retains only its own
exposed signature metadata, self-check path/count, process status and output;
it never borrows companion signature bytes or registers. Native wire binding
still compares complete interval/clock/sample against original child stdout.
The reader selects the compiled new oracle by exact digest for bounded variant
fragments; original smoke continues selecting its original oracle.

## Load-only work is separate

Execution ELFs retain a 64-KiB installed image. Load-only IDs use a distinct
linker with a 512-KiB initialized `0xa5` payload and 8-KiB additional zero fill,
yielding a 1-MiB image within existing RAM composition. Executed instructions
and their effects are unchanged, but ELF/segment/linker identities differ.
Offline derivation checks every payload byte and declared zero-fill extent.

The timer still contains only pre-read bytes → parse → composition/install.
Full actual image/BSS bytes, metadata, entry, all registers/counters, cleared
reservation/signal/device state are checked after stop, before accepting time.
There are zero Hart turns and no invented exit. Version 1's 128-KiB payload,
256-KiB image, manifests and linker remain unchanged for identity provenance.
Development at `cbd158b020a526517da58ea2a9462fbc9d8a8660` reached only
996,815,750 and 989,940,373 timed ns in the fib load cells before their 20-second
caps. Its interrupted prefix is not a full session or baseline. Version 2 uses
new `-load-v2` IDs and a new linker/oracle/mapping digest; no old result is
relabelled. Execution ELF bytes/work are unchanged. Both payload choices are
explicit bounded variants; there is no adaptive until-green rescaling. An actual input digest mismatch
is rejected before any interval can become usable. Pure image hashes may be
cached only after complete equality of each newly observed byte array; no
inspection or verdict is cached. Guest loops do not masquerade as load work.

## Reproduction and remaining acceptance

With the approved pinned cross-tools, from the repository:

```bash
python3 -B tools/a10/build_fixtures.py --calibration --out target/a10-variants-new
python3 -B tools/a10/variant_specs.py check --build target/a10-variants-new
cargo test --all-features --test a10_p3_variants
python3 -B -m unittest discover -s tools/a10 -p 'test_*.py'
```

Missing required tools are unavailable, never a passed variant matrix. The Rust
variant test freshly builds all identities, checks offline derivation, executes
all 148 applicable public route/mode cells, exercises fresh Machine reruns,
rejects wrong result/PC/count/signature and fabricated failed-SC writes, and
checks the 22 actual load-only cells. Offline controls reject changed mapping,
versions, work sizes, source text, digest/placement and arithmetic bounds.

The stable calibrated/compare command is documented in the
[schema guide](a10-perf-schema.md#p3-qualified-local-calibration-under-development).
Integration, full-reader negative controls, empirical sufficiency and exact-head
three-session/baseline/comparison verification remain required. This document
claims no usable baseline, speedup, final review, P3 acceptance or P4 delivery.
Warmup/sample/clock/noise/30-minute budget gates and qualified same-allocation
limitations are not relaxed by adding these fixtures.
