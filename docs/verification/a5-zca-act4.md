# A5 Zca ACT4 selection (U4)

**Status:** Current verification tooling note

**Authority:** Describes the frozen Zca selection and its flow; the milestone
contract is [`docs/dev-plan.md`](../dev-plan.md) D9 and unit U4.

## What is frozen

| Input | Path | Role |
| --- | --- | --- |
| Zca selection | `docs/verification/a5-zca-source-inventory.json` | The 32 `tests/rv64i/Zca/*.S` sources at ACT4 commit `a7c99303516f4e668f7488f172043392e23b9dfd`, with per-source SHA-256 and the pinned `tests/env` header hashes |
| RV64I selection (unchanged) | `docs/verification/a5-source-inventory.json` | The 51-case frozen RV64I selection, unchanged by U4 |
| Tool pins (unchanged) | `docs/verification/a5-feasibility-pins.json` | ACT4 commit, Sail 0.10, GCC and dependency hashes |
| UDB config | `scripts/a5/ruscv-rv64imac-zca.yaml` | `I (=2.1.0)` and `Zca (=1.0.0)`, `MXLEN: 64`; nothing else |
| Sail derivation | `scripts/a5/sail_config_zca.py` | Everything optional off, then exactly `Zca`, `Zicsr`, `Zifencei` on; V `Disabled`; `writable_misa=false`; no PMP |

`scripts/a5/inventory_zca.py` regenerates the expected selection from the pinned
tree with the same header parser as `inventory.py` and fails when the checked-in
manifest differs. It does not modify the frozen RV64I manifest.

## Flow

`scripts/a5/experiment.sh` selects the frozen flow with `A5_SELECTION`:

| Value | Flow |
| --- | --- |
| `smoke` | One-source RV64I feasibility probe (unchanged) |
| `proposal` | The frozen 51-case RV64I selection through the full accounting |
| `zca` | The frozen 32-source Zca selection under the Zca-only configs |

The `zca` run reuses `scripts/a5/accounting.py` with
`A5_CONFIG=ruscv-rv64imac-zca`, `A5_MANIFEST=docs/verification/a5-zca-source-inventory.json`
and `A5_PLAN=.a5/evidence/zca-plan.json`. Generation uses `--extensions Zca` against a selection directory that contains
only `tests/rv64i/Zca`, so the ACT4 framework only sees sources whose
directory is `Zca` and whose `REQUIRED_EXTENSIONS` (`['I', 'Zca']`) are
implemented by the UDB config. The `MisalignZca`, `ZihintntlZca`, Zcb, Zcd and
Zcmop directories are never copied into that directory and stay excluded.
`A5_RUN_ISA=rv64imac` adds `--isa rv64imac` to every public-CLI execution.

`scripts/a5/linked_audit.py` accepts 16-bit parcels only when the caller's
mnemonic-keyed mapping allows them. The Zca selection allows exactly the RV64
Zca mnemonics; `C.ADDI4SPN` and the Zcmop names stay out even though they share
quadrants with allowed neighbours. The RV64I flow passes no mapping, so its
audit behavior is unchanged.

## Running it

The flow needs the pinned Sail 0.10, RISC-V GCC and the ACT4 Ruby/Python
dependencies. The manual GitHub workflow runs it on `ubuntu-22.04` with the
pinned tools:

```text
Actions -> "A5 ACT4 selected compatibility" -> Run workflow
  selection: zca        # or proposal for the RV64I regression control
```

Locally, `experiment.sh` expects the same `uv`, `bundler` and `sail_riscv_sim`
executables on `PATH`; without them the run reports the missing tool instead of
recording a pass. A run is complete when `.a5/evidence/selection-results.json`
reports `"success": true` with `executed_elfs == passed_elfs == 32`, the linked
audits have no unsupported instructions, and `suite_controls.py` passed.

## Limits

The selection proves Zca instruction behavior for the 32 non-trapping ACT4
sources under the Sail oracle. It is not an RV64GC or whole-extension
certification, it does not replace the frozen 51-case RV64I result, and the
A10 oracle and performance certification remain unchanged.
