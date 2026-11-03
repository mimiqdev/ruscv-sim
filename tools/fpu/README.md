# F/D arithmetic reference vectors

`fpu-reference-v1.tsv` is the pinned independent reference for the F/D
arithmetic core (`src/fpu/arith`, contract unit U2 and decision D4 in
`docs/dev-plan.md`). `tests/fpu_arith.rs` replays every row against the core.

Each row is four fields of context followed by the expected result and flags:

```text
op <TAB> rm <TAB> a <TAB> b <TAB> c <TAB> result <TAB> flags
```

* `op` — the operation, named after the format and instruction
  (`f32_add`, `f32_fmadd`, `f64_to_i32`, `f32_le`, ...);
* `rm` — `RNE`, `RTZ`, `RDN`, `RUP` or `RMM`; `-` for the comparisons;
* `a`, `b`, `c` — format bit patterns or integer operands in hexadecimal;
  `-` means the operand is not used;
* `result` — the required result bits, always 16 hex digits (32-bit integer
  results are already sign-extended as RV64 requires);
* `flags` — the required `fflags` set in architectural bit order
  (`NX=1`, `UF=2`, `OF=4`, `DZ=8`, `NV=16`).

## Reference and pin

The reference is **Berkeley SoftFloat Release 3e**, the arithmetic library
that Berkeley TestFloat 3e verifies. Regeneration downloads and verifies a
pinned release:

* URL: <https://www.jhauser.us/arithmetic/SoftFloat-3e.zip>
* SHA-256: `21130ce885d35c1fe73fc1e1bf2244178167e05c6747cad5f450cc991714c746`

SoftFloat is configured with tininess detection **after rounding**
(`softfloat_detectTininess = softfloat_tininess_afterRounding`), the mode
RISC-V requires.

`testfloat_gen` itself is not used as the committed input. At level 1 it
generates a minimum of 46,464 cases for each single-precision binary function
and 6,133,248 for `f32_mulAdd`, so a committed set would be tens of
megabytes and not reviewable. `gen_fpu_vectors.c` instead calls the same
pinned SoftFloat library with a deterministic case selection and a fixed seed,
which is the "equivalent independent generator" option in the contract.

## Case selection

* Fixed seed `0x0146000000000001` and a fixed xorshift64\* generator, so the
  file depends only on the pinned SoftFloat sources and this generator.
* Every rounding mode for every rounded operation.
* The complete cross product of a 28-value curated edge set for unary
  operations and conversions: signed zeros, minimum and maximum subnormals,
  minimum normal, values around 1, `2^31`/`2^32`, `2^63`/`2^64`, maximum
  finite, both infinities, canonical and payload quiet NaNs, and signaling
  NaNs of both signs.
* The complete cross product of a 14-value class-covering subset for the
  binary operations and the comparisons, plus 48 seeded random cases per
  binary operation and mode.
* Fused multiply-add: 8 × 8 curated operand pairs with a cycling addend, 48
  seeded random triples, and explicit rows for `0 × infinity + qNaN`, a zero
  product with a subnormal addend, exact cancellation and opposite
  infinities.
* 48 seeded random cases per unary/conversion operation and mode. Random
  patterns are biased towards subnormal, minimum-normal, huge, near-maximum
  and near-1 values so underflow, overflow and inexact flags are exercised.
* Conversion boundaries: the curated values include the `FCVT` integer
  boundaries and the seeded cases include nearby values. Conversions to
  integer use SoftFloat's exact-rounding mode, matching the RISC-V rule that
  `NX` is set when the rounded result differs and `NV` is clear.

The committed file has 17,551 rows and is about 1 MB.

## RISC-V adjustments applied by the generator

SoftFloat leaves two results to its platform layer. The generator applies the
RISC-V rule and the committed rows are the required RISC-V results:

1. NaN results of arithmetic, square root, fused multiply-add and
   float-to-float conversion are replaced with the canonical NaN
   (`0x7fc00000` / `0x7ff8000000000000`), as D4 requires. Sign injection and
   classification are not arithmetic and are not part of the generator; their
   payload behaviour is covered by unit tests. The comparisons are emitted
   with their boolean result and exception flags.
2. Invalid float-to-integer conversions use the RISC-V saturation table
   (`+inf`/NaN/positive overflow to the maximum, negative overflow to the
   minimum or zero) and 32-bit results are sign-extended into the 64-bit
   result. SoftFloat's platform header instead follows the host x86 SSE
   convention.

## Regenerate

```bash
tools/fpu/gen_fpu_vectors.sh           # rewrite the committed TSV
tools/fpu/gen_fpu_vectors.sh --check   # verify it is reproducible
```

Both commands need network access, `curl`, `unzip`, `make` and `gcc`; the
Rust test suite only reads the committed file.

## Deeper local differential runs

The generator accepts compile-time overrides for a larger locally generated
set. This is not committed; it is useful when changing the core:

```bash
gcc -O2 -I<softfloat>/source/include \
    -DNRAND_BINARY=20000 -DNRAND_UNARY=20000 -DNRAND_FMA=5000 \
    -DFPU_SEED=0x1234567890abcdefULL \
    -o /tmp/gen_fpu_vectors tools/fpu/gen_fpu_vectors.c \
    <softfloat>/build/Linux-x86_64-GCC/softfloat.a -lm
/tmp/gen_fpu_vectors > /tmp/fpu-reference.tsv
FPU_VECTOR_FILE=/tmp/fpu-reference.tsv cargo test --release --test fpu_arith
```

The differential runs on the reviewed head covered about 6.4 million
additional seeded and directed cases across several seeds with no mismatch.

## Licensing

The Rust arithmetic core in `src/fpu/arith` is a port of Berkeley SoftFloat
Release 3e, and the reference vectors are generated by running it. The
SoftFloat copyright notice, conditions and disclaimer are retained in
[`SOFTFLOAT-LICENSE.txt`](SOFTFLOAT-LICENSE.txt) and referenced from the
ported modules.

## What the vectors do not cover

`FMIN`/`FMAX` (RISC-V F/D version 2.2 rules), `FCLASS` and
`FSGNJ`/`FSGNJN`/`FSGNJX` are specified directly by the ISA manual and are
covered by the focused tests in `src/fpu/arith/tests.rs`. The committed
`FEQ`/`FLT`/`FLE` rows come from `gen_compare` and cover the 14-value
cross-product classes only, not the full 28-value edge set. `f32_fmadd` rows
also verify `FMSUB`, `FNMSUB` and `FNMADD`: those instructions are the same
fused operation with exactly negated operands, so `tests/fpu_arith.rs` runs
each row through all four with the corresponding sign bits flipped.
