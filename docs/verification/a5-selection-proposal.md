# A5 pinned inventory and nontrapping profile proposal

**Status: proposed, not frozen; no full-selection success claimed.**

This is the next reviewable slice after PR #32, merged as
`d568d45093bb643884740c751fa595f0f593f4dc`. It implements accounting and
proposes the source/profile boundary required by [A5](../dev-plan.md).
It neither accepts A5 nor repairs the known FENCE rejection.

## Decisions requested in this PR

1. Approve the narrow MXLEN provenance overlay as the canonical **A5 test
   adapter**, not a complete machine configuration. It changes UDB 0.1.9's
   `MXLEN` defining extension from `Sm` to `I`, retaining width constraints.
   The reviewed smoke approval authorized adapted feasibility only.
   Alternatives are falsely declaring Sm (rejected), adding privileged
   execution (out of scope), or changing the pinned upstream toolchain
   (not recommended after the successful bounded smoke).
2. Explicitly approve the proposed **naturally aligned EEI boundary** and the
   eight successful-misalignment exclusions below. The existing contract's
   trap exclusions do **not by themselves authorize these exclusions**.
   The recommendation is an aligned-only nontrapping profile, consistent with
   the ISA's EEI-dependent misaligned-access support. If the maintainer intends
   “all nontrapping base-I” to require successful misalignment, these eight
   sources must become required and the proposed 51-source denominator must
   not be frozen. That is a selection-contract decision, not permission to
   conceal failing required instructions.

The [machine-readable profile](../../scripts/a5/profile-proposal.json) records
both outstanding decisions. Its `proposed-not-frozen` status is retained in all
plans and reports, even if every provisional case later passes. A generation run
cannot approve either decision. No new architectural subsystem/ADR is proposed.

## Complete pinned source inventory

[`a5-source-inventory.json`](a5-source-inventory.json) enumerates **all 1,970
tracked `tests/**/*.S` sources** at ACT4 4.0.0
`a7c99303516f4e668f7488f172043392e23b9dfd`. Every record has an exact source
identifier, SHA-256, upstream required-extension list, parameter constraints,
MARCH, coverage-group/test purpose, disposition, reason and coverage consequence.
Environment headers are build inputs, not source cases.

| Proposed disposition | Source count | Coverage consequence |
| --- | ---: | --- |
| Required nontrapping `rv64i/I` | 51 | Includes ALU, word operations, control flow, memory, NOP and FENCE |
| RV64I successful misalignment | 8 | Proposed EEI exclusion, **pending explicit approval** |
| Other extensions under RV64I | 654 | No extension-wide coverage claim, including Zifencei |
| Privileged-purpose sources | 355 | No trap/CSR/PMP/translation claim |
| RV32I and RV32E/RV64E profiles | 902 | No alternate-width/reduced-register claim |
| **Entire pinned inventory** | **1,970** | No inventory source silently unclassified |

The directory totals are RV64I 713, RV32I 641, RV64E 148, RV32E 113,
privileged 355. Header-only filtering is insufficient: **177** sources declare
only `I`; they comprise 51 required, eight proposed misalignment exclusions,
44 alternate-base sources, and **74 privileged-purpose sources**. Of those
74, 43 declare MXLEN 64 and 31 MXLEN 32. Their exact IDs and parameter
requirements (for example `NUM_PMP_ENTRIES: '>0'`) remain in the inventory.
An `I` header does not turn PMP/CSR test purpose into unprivileged behavior.

### Eight consequential proposed exclusions

Every source below declares `REQUIRED_EXTENSIONS: ['I']`,
`MXLEN: 64`, `MISALIGNED_LDST: true`, and `MARCH: rv64i_zicsr`:

- `tests/rv64i/Misalign/Misalign-lh-00.S`
- `tests/rv64i/Misalign/Misalign-lhu-00.S`
- `tests/rv64i/Misalign/Misalign-lw-00.S`
- `tests/rv64i/Misalign/Misalign-lwu-00.S`
- `tests/rv64i/Misalign/Misalign-ld-00.S`
- `tests/rv64i/Misalign/Misalign-sh-00.S`
- `tests/rv64i/Misalign/Misalign-sw-00.S`
- `tests/rv64i/Misalign/Misalign-sd-00.S`

These check **successful** loads/stores at misaligned byte offsets using
`RVTEST_SIGUPD`; they do not test trap entry. Pinned
`framework/src/act/select_tests.py::check_test_params` requires the declared
`MISALIGNED_LDST` capability to match; it is absent from the I-only UDB config.
Do not use that absence alone as proof the intended contract authorizes omission.

The [unprivileged ISA load/store chapter](https://docs.riscv.org/reference/isa/unpriv/rv32.html)
leaves misaligned load/store behavior to the EEI; natural alignment is the
unconditional base-I guarantee. Current native
[`SystemBus`](../../src/executor.rs#L233) delegates RAM to
[`SimpleMemory`](../../src/memory/mod.rs#L108), which rejects misalignment.
That implementation fact explains why the recommendation accurately describes
the existing EEI; it is **not** a rule allowing required failures to be dropped.
No successful-misalignment or architectural trap-delivery support is declared.

### FENCE and absent coverage

`tests/rv64i/I/I-fence-00.S` is required, including regular FENCE,
`fence.tso`, reserved fields and hint encodings. Single-Hart synchronous ordered
memory can satisfy stronger ordering without claiming Ztso or a concurrent
platform. `Opcode::MiscMem` remains unsupported in the public
[decoder](../../src/decode/mod.rs#L248); this is a blocker, not an exclusion.
A reproducing public regression and any bounded Rust repair are deferred to
the next PR, with the full gate required before repair acceptance.

> Superseded by [G-14](public-behavior-gaps.md#g-14--base-i-fence-miscmem-funct3--0b000-was-unconditionally-rejected): base-I FENCE is implemented in PR #35. The blocker analysis above is retained as the pre-fix record.

`tests/rv64i/Zifencei/Zifencei-fence.i-00.S` belongs to Zifencei, not I.
The selected I sources provide no ECALL/EBREAK trap-entry validation,
privilege/CSR/MMU/PMP behavior, misaligned-success validation, or concurrent/
device-ordering proof. The FENCE source cannot establish concurrency ordering
on this single-Hart platform.

## Profile and oracle boundary

The proposed profile is little-endian RV64I 2.1, XLEN 64, 32 integer registers,
IALIGN 32, one Hart, naturally aligned native RAM at `0x80000000`, HTIF
pass/fail through the existing RAM `tohost` symbol. No CSR, Sm, trap, MMU,
PMP, interrupt or other extension capability is declared.

The existing linker/startup/pass-fail adaptation is reused without editing
upstream source instructions or Sail expected results. Upstream incidental
`rv64i_zicsr` assembly flags do not establish DUT Zicsr support: the linked
instruction audit covers startup and exit code and rejects CSR/other non-I
instructions. The oracle has its own Sail 0.10 configuration, generated from
the pinned example with optional extensions disabled. Oracle configuration is
not the DUT's extension declaration.

## Implemented accounting

[`inventory.py`](../../scripts/a5/inventory.py) reconstructs the complete
inventory from Git's pinned tracked tree, rejects a wrong revision/modified
tracked tests, and compares it with the checked-in proposal. Unknown header
syntax or a new base-I coverage group fails rather than defaulting to exclusion.

[`accounting.py`](../../scripts/a5/accounting.py) prepares the entire proposed
selection before generation and refuses reused selection/work directories.
Its source-to-variant rule mirrors pinned
`framework/src/act/build_plan.py::gen_compile_tasks`: one XLEN=64 self-check
ELF and one distinct Sail signature ELF per source. This is a **planned**
variant count, not observed generation and not a frozen denominator.
No source-level coverage bins are misrepresented as separate generated ELFs.

The harness checks exact expected versus actual ELF paths, required nonempty
signature/results files, generation command status, and complete unique result
identities. Empty, missing, duplicate, extra and hash-mismatched records fail.
It enumerates every generated required self-check ELF, retains missing cases as
suite errors, runs each through `ruscv-sim run` with 1,000,000 cycles and a
60-second host timeout, and preserves stdout/stderr, invocation, hashes,
configuration identity and simulator revision. Signature ELFs never count as
DUT executions. Audit errors still permit diagnostic public execution but keep
the suite failed.

The strict [`cli_result.py`](../../scripts/a5/cli_result.py) parser from PR #32's
P2 fix is unchanged: success needs one complete terminal host block, consistent
fields and status, no simulator error, and a matching process exit. Failure
reasons distinguish host/cycle timeouts and simulator errors; raw diagnostics
retain unsupported-instruction details.

[`test_accounting.py`](../../scripts/a5/test_accounting.py) adds negative
accounting/process controls to the existing smoke/parser tests. These fake-process
tests prove orchestration, **not guest execution**.
[`suite_controls.py`](../../scripts/a5/suite_controls.py) additionally compiles
real pass/fail, nontermination and invalid-instruction guests plus missing and
malformed ELF controls and invokes the same `accounting.execute` function.
The original corrupt-Sail-expected-value smoke control remains available;
it is not counted as a required source or a second selected pass.

## Reproduction and CI policy

Inventory-only, after a pinned checkout at `.a5/upstream`:

```bash
python3 scripts/a5/inventory.py
python3 -m unittest discover -s scripts/a5 -p 'test_*.py'
bash -n scripts/a5/experiment.sh
```

`inventory.py --write` regenerates the proposal for review, never freezes it.
From a fresh Linux workspace with the prerequisites in
[feasibility](a5-feasibility.md):

```bash
A5_SELECTION=proposal bash scripts/a5/experiment.sh
```

`A5_SELECTION=smoke` retains the original one-source workflow. The A5 workflow
now uses **only `workflow_dispatch`** for future explicit runs. A narrowly scoped
pre-merge branch push trigger was used once for accounting validation and then
removed; final-head/evidence commits do not rerun ACT4. This slice is bounded to **at most two Linux ACT4
attempts, 30 minutes each**; required ordinary PR CI is separate.
No `[skip ci]` commits or unrelated Codecov repair is used.

The ordinary required PR quality job now runs Python accounting/parser tests.
It is not an ACT4 guest run. Artifacts retain `.a5/evidence`, `.a5/config`,
and `.a5/work` for 14 days. Keep exact run/head/hash diagnostics in repository
evidence; retrieve artifacts before expiry rather than treating ephemeral URLs
as a durable execution record.

## Inherited evidence and limits

PR #32 required PR CI **34622758829** succeeded at `97f606f`; that empty
commit's tree was identical to `c66597b`. Manual CI **34622337290** passed
quality but failed coverage upload because the Codecov token was absent.
Neither run generated or executed a new ACT4 guest.
PR #32 merged at `d568d45093bb643884740c751fa595f0f593f4dc`; merge-push CI is
not inferred from PR/manual success.

The original successful adapted smoke and corrupt-self-check hashes remain in
[`a5-feasibility-results.json`](a5-feasibility-results.json), with strict-parser
replay in [`a5-classifier-replay.json`](a5-classifier-replay.json).
Its retained artifact expires **2026-09-25**. This slice does not overwrite
those hashes or run a fifth old feasibility experiment.

Local verification for this slice: 21 Python tests, deterministic pinned
inventory reconstruction, shell syntax, and all six commands in the A5 Rust
gate passed. The local host lacks `riscv64-unknown-elf-gcc`; Cargo tests that
early-return without external tools are not new separately compiled guest
evidence. Profile freeze and merge remain maintainer decisions.

## Bounded Linux accounting evidence

**Attempt 1 only:** [run 34624439277](https://github.com/mimiqdev/ruscv-sim/actions/runs/34624439277)
at `4e8a947300f6012d83b991017df4247cda06a449`, duration 4m9s, concluded
**failure**, correctly. Generation returned zero; **51 sources produced 51
self-check variants and 51 separate signature ELFs**. All 51 self-check ELFs
were executed: **44 guest-pass, one guest-fail, six simulator-error results**.
No missing generated cases or missing execution results; no linked-audit
rejections. This is provisional complete accounting, **not** full-selection
compatibility or a frozen 51-source denominator.

| Required source | Result | Retained first diagnostic |
| --- | --- | --- |
| `I-beq-00.S` | simulator error | PC `0x80009800`, invalid memory address `0xbaa00d3616bec80d` |
| `I-bge-00.S` | simulator error | PC `0x80009800`, invalid instruction `0x800097ec` |
| `I-bgeu-00.S` | simulator error | PC `0x8000981c`, invalid memory address `0x3f33662176fd1ca1` |
| `I-blt-00.S` | simulator error | PC `0x80009800`, invalid instruction `0x800097ec` |
| `I-bltu-00.S` | guest fail | Exit 1, 5,528 cycles, PC `0x8001c050`, no simulator error |
| `I-bne-00.S` | simulator error | PC `0x80009800`, invalid memory address `0x9ab3534837563804` |
| `I-fence-00.S` | simulator error | Unimplemented instruction at PC `0x80000260`, 150 retired cycles |

All identifiers in this table are under `tests/rv64i/I/`. All seven remain
required blockers. The branch symptoms are **not yet root-caused**: these
observations alone do not distinguish an adapter/layout problem from an ISA
defect. Do not implement speculative Rust branch repairs from this table.
The FENCE observation reproduces the known `MiscMem` rejection; the next
bounded repair PR still needs a focused public regression and the full gate.

> This table is retained as the pinned pre-fix record. Every row has since been
> repaired: the six conditional-branch rows by
> [G-13](public-behavior-gaps.md#g-13--rv64i-conditional-branch-used-a-12-bit-sign-extension-for-a-13-bit-b-immediate)
> (PR #34), and `I-fence-00.S` by
> [G-14](public-behavior-gaps.md#g-14--base-i-fence-miscmem-funct3--0b000-was-unconditionally-rejected)
> (PR #35).

The same execution function also ran six real public controls: pass, deliberate
guest fail, cycle nontermination, invalid instruction, malformed ELF, and missing
ELF. All matched expectations. Host wall-time handling is covered by the local
fake-process control; it is not relabeled a Linux guest-timeout experiment.

[`a5-accounting-results.json`](a5-accounting-results.json) durably retains all
source/variant identities, generated hashes, invocations, per-case host result
fields, stdout/stderr, audit summaries, tool versions, profile/config hashes,
and control diagnostics. Artifact **10274071547**, 21,676,628 bytes, expires
**2026-09-25T16:56:08Z**; its locally verified full-archive SHA-256 is
`08682429f62b8deaf656e1013dd1e11ad54795609353b48087f3810977cdbbd1`.

The follow-up accounting guards also reject extra `.sig` and `.results` files.
They were verified locally against the exact hash-checked artifact, along with
empty/missing/duplicate/extra result records, missing generation artifacts and a
failed generation command. All 12 retained-data negative assertions passed.
This replay does not rerun a guest or spend another Linux attempt:

```bash
gh api repos/mimiqdev/ruscv-sim/actions/artifacts/10274071547/zip > .a5/accounting-artifact.zip
python3 scripts/a5/replay_accounting.py .a5/accounting-artifact.zip
```

Download performance on the local host required bounded explicit HTTP ranges;
the reassembled complete archive hash was checked before evidence replay.
An incomplete download cannot pass the replay's SHA-256 check.

Required PR quality CI [34624444279](https://github.com/mimiqdev/ruscv-sim/actions/runs/34624444279)
succeeded at the Linux experiment head `4e8a947`. Final-head required PR CI is
reported separately on PR #33; this earlier success is not substituted for it.
Only one of the two authorized Linux attempts was used. No second generation
is necessary to validate the accounting-only follow-up against retained data.
