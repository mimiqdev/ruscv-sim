# Zca expansion-decoder reference generator

`gen_zca_reference.py` classifies every 16-bit parcel of the RV64 Zca
compressed-instruction space and emits `zca-reference-v1.tsv`, the pinned
independent reference used by `tests/zca_expansion_test.rs` (issue #132,
contract U1).

The generator is an independent Python implementation of the RV64 Zca
encoding space, written from the RISC-V ISA Unprivileged Specification
version 20250508 (Chapter "C") and the Selectable Zca
Compressed-Instruction Profile contract (`docs/dev-plan.md` decisions
D2 and D6, activation merge `9db1bed`). It is deliberately structured
unlike the Rust decoder in `src/isa/rv64c/decoder_16bit.rs`: compressed
encodings are declarative table rows (selector bits, class predicate, a
generic immediate bit-field map) walked by one evaluator, instead of
per-quadrant hand-written classify functions, so a defect in one
implementation produces a diff against the other.

Each output row is `parcel class expansion` where:

- `parcel` is the 16-bit encoding in hex (`0x0000`..`0xffff`),
- `class` is `E` legal expansion, `H` contract D2 HINT, `R` reserved, or
  `F` FP-compressed illegal (C.FLD / C.FSD / C.FLDSP / C.FSDSP),
- `expansion` is the equivalent 32-bit encoding in hex for `E`/`H` rows or
  `-` for `R`/`F` rows.

Regenerate (deterministic; no external inputs):

```bash
python3 tools/zca/gen_zca_reference.py > tools/zca/zca-reference-v1.tsv
```
