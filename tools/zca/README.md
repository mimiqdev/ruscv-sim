# Zca/Zcd expansion-decoder reference generator

`gen_zca_reference.py` classifies every 16-bit parcel of the RV64 compressed
instruction space and emits the pinned independent reference used by
`tests/zca_expansion_test.rs` (issue #132, contract U1; extended for issue
#152, contract U3b).

The generator is an independent Python implementation of the RV64
compressed encoding space, written from the RISC-V ISA Unprivileged
Specification version 20250508 (Chapter "C", including Zcd) and the
Selectable F/D Floating-Point Profiles contract (`docs/dev-plan.md`
decision D8). It is deliberately structured unlike the Rust decoder in
`src/isa/rv64c/decoder_16bit.rs`: compressed encodings are declarative
table rows (selector bits, class predicate, a generic immediate bit-field
map) walked by one evaluator, instead of per-quadrant hand-written classify
functions, so a defect in one implementation produces a diff against the
other.

The compressed profile is a generator dimension:

| Reference | Generator invocation | Machine profile |
| --- | --- | --- |
| `zca-reference-v1.tsv` | `gen_zca_reference.py --profile zca` | `rv64imac` (Zca only) |
| `zca-reference-v2.tsv` | `gen_zca_reference.py --profile zcd` | `rv64imafdc` (Zca + Zcd) |

Each output row is `parcel class expansion` where:

- `parcel` is the 16-bit encoding in hex (`0x0000`..`0xffff`),
- `class` is `E` legal expansion, `H` contract D2 HINT, `R` reserved, or
  `F` FP-compressed illegal (C.FLD / C.FSD / C.FLDSP / C.FSDSP),
- `expansion` is the equivalent 32-bit encoding in hex for `E`/`H` rows or
  `-` for `R`/`F` rows.

The two references differ only in the four Zcd slots. Under `--profile zca`
those parcels are class `F` because F/D is absent; under `--profile zcd`
they are class `E` and expand to FLD/FSD with the C.LD/C.SD and
C.LDSP/C.SDSP 8-byte scaled offsets.

Regenerate (deterministic; no external inputs):

```bash
python3 tools/zca/gen_zca_reference.py --profile zca > tools/zca/zca-reference-v1.tsv
python3 tools/zca/gen_zca_reference.py --profile zcd > tools/zca/zca-reference-v2.tsv
```
