# A5 F/D and Zcd ACT4 selections (U5)

**Status:** Current verification tooling note

**Authority:** Describes the frozen F/D and Zcd selections and their flow; the
milestone contract is the [archived F/D contract](../archive/milestones/selectable-fd-floating-point-profiles.md) D11 and unit U5.

## What is frozen

| Input | Path | Role |
| --- | --- | --- |
| F/D selection | `docs/verification/a5-fd-source-inventory.json` | The 82 `tests/rv64i/F/*.S` and 114 `tests/rv64i/D/*.S` sources at ACT4 commit `a7c99303516f4e668f7488f172043392e23b9dfd`, with per-source SHA-256 and the pinned `tests/env` header hashes |
| Zcd selection | `docs/verification/a5-zcd-source-inventory.json` | The 4 `tests/rv64i/Zcd/*.S` sources at the same pin |
| RV64I selection (unchanged) | `docs/verification/a5-source-inventory.json` | The 51-case frozen RV64I selection, unchanged by U5 |
| Zca selection (unchanged) | `docs/verification/a5-zca-source-inventory.json` | The 32-source frozen Zca selection, unchanged by U5 |
| Tool pins (unchanged) | `docs/verification/a5-feasibility-pins.json` | ACT4 commit, Sail 0.10, GCC and dependency hashes |
| F/D UDB config | `scripts/a5/ruscv-rv64imafd.yaml` | `I (=2.1.0)`, `Zicsr (=2.0.0)`, `F (=2.2.0)`, `D (=2.2.0)`, `MXLEN: 64`; nothing else |
| Zcd UDB config | `scripts/a5/ruscv-rv64imafdc-zcd.yaml` | `I (=2.1.0)`, `Zca (=1.0.0)`, `Zicsr (=2.0.0)`, `F (=2.2.0)`, `D (=2.2.0)`, `Zcd (=1.0.0)`, `MXLEN: 64`; nothing else |
| F/D Sail derivation | `scripts/a5/sail_config_fd.py` | Everything optional off, then exactly `Zicsr`, `Zifencei`, `F`, `D` on; V `Disabled`; `writable_misa=false`; no PMP |
| Zcd Sail derivation | `scripts/a5/sail_config_zcd.py` | Everything optional off, then exactly `Zicsr`, `Zifencei`, `Zca`, `F`, `D`, `Zcd` on; the same base settings |
| ACT4 DUT header | `scripts/a5/rvtest_config_fd.h` | Declares `F_SUPPORTED` and `D_SUPPORTED` for the two FP selections |

`scripts/a5/inventory_fd.py` and `scripts/a5/inventory_zcd.py` regenerate their
expected selections from the pinned tree through the same header parser as
`inventory.py`, and fail when the checked-in manifest differs. Neither script
modifies the frozen RV64I or Zca manifests.

`Zicsr` is named in both FP configurations because UDB declares it as a
requirement of `F` and rejects a configuration that implements `F` without it.
It is already part of the inherited `rv64ima` profile and of every selection's
`MARCH` header (`rv64if_zicsr`, `rv64ifd_zicsr`, `rv64ifd_zicsr_zcd`), so the
enabled instruction surface is still exactly RV64I plus F and D, or RV64I plus
Zca, F, D and Zcd. The frozen RV64I and Zca configurations are untouched and
still declare only what they declared before.

The ACT4 environment enables `mstatus.FS` only for a DUT header that declares
floating-point support, so the FP selections supply their own
`rvtest_config.h`. The RV64I and Zca selections keep the unchanged header that
declares no floating-point support.

## Flow

`scripts/a5/experiment.sh` selects the frozen flow with `A5_SELECTION`:

| Value | Flow |
| --- | --- |
| `smoke` | One-source RV64I feasibility probe (unchanged) |
| `proposal` | The frozen 51-case RV64I selection through the full accounting (unchanged) |
| `zca` | The frozen 32-source Zca selection under the Zca-only configs (unchanged) |
| `fd` | The frozen 196-source F/D selection under `rv64imafd`, `A5_RUN_ISA=rv64imafd` |
| `zcd` | The frozen 4-source Zcd selection under `rv64imafdc`, `A5_RUN_ISA=rv64imafdc` |

Both new selections reuse `scripts/a5/accounting.py`; `fd` uses
`A5_CONFIG=ruscv-rv64imafd`, `A5_MANIFEST=docs/verification/a5-fd-source-inventory.json`
and `A5_PLAN=.a5/evidence/fd-plan.json`, and `zcd` uses the Zcd config, manifest
and `.a5/evidence/zcd-plan.json`. Generation uses `--extensions F,D` and
`--extensions Zcd` against a selection directory that contains only those
subdirectories, so the ACT4 framework only sees sources whose directory matches
and whose `REQUIRED_EXTENSIONS` are implemented by the UDB config.

`MisalignF` and `MisalignD` carry the same `REQUIRED_EXTENSIONS` as their
natural-alignment neighbours, so the directory is the exclusion mechanism: the
`ZfaF`, `ZfaD`, `ZfaZfh`, `ZfaZfhD`, `ZfaZvfh`, `Zfh`, `ZfhD`, `Zfhmin`,
`ZfhminD` and `Zfbfmin` directories are never copied into the selection and stay
excluded, as do `MisalignF`, `MisalignD`, Zcb, Zcmop and every other
subdirectory of `tests/rv64i`.

`scripts/a5/linked_audit.py` accepts FP encodings only when the caller asks for
them. The RV64I and Zca flows pass no FP permission, so their audit still
rejects LOAD-FP, STORE-FP, OP-FP, R4 and every SYSTEM encoding exactly as
before. The two FP flows allow exactly the F and D mnemonics plus the six Zicsr
CSR instruction forms, and a SYSTEM word must also name one of the four CSRs
these selections address (`mstatus` for the `FS`-enable sequence, and
`fcsr`/`fflags`/`frm` for flag and rounding control). Any other CSR address,
`ecall`, `ebreak`, `mret` and any out-of-profile FP format stay unsupported.
The Zcd flow additionally allows the four compressed double-precision parcels
on top of the Zca mapping.

## Running it

The flow needs the pinned Sail 0.10, RISC-V GCC and the ACT4 Ruby/Python
dependencies. The manual GitHub workflow runs it on `ubuntu-22.04` with the
pinned tools:

```text
Actions -> "A5 ACT4 selected compatibility" -> Run workflow
  selection: fd         # or zcd; proposal and zca remain the regression controls
```

The `fd` and `zcd` selections have a larger job bound than the recorded
30-minute RV64I and Zca controls because they generate, compile and simulate
196 and 4 sources instead of 51 and 32.

Locally, `experiment.sh` expects the same `uv`, `bundler` and `sail_riscv_sim`
executables on `PATH`; without them the run reports the missing tool instead of
recording a pass. A run is complete when `.a5/evidence/selection-results.json`
reports `"success": true` with `executed_elfs == passed_elfs` equal to the
selection size (196 for `fd`, 4 for `zcd`), the linked audits have no
unsupported instructions, and `suite_controls.py` passed.

## Limits

The selections prove F, D and Zcd instruction behavior for the non-trapping
ACT4 sources under the Sail oracle. They are not an RV64GC certification, they
do not replace the frozen 51-case RV64I or 32-source Zca results, and the A10
oracle and performance certification remain unchanged.
