# A6 Formal Closeout Record

**Status:** Historical — completed and accepted on 2026-09-18

**Authority:** Informational historical acceptance record. The sole active
normative contract is [A7](../../dev-plan.md).

**Completed / accepted:** 2026-09-18

**Approval:** The maintainer explicitly accepted A6 formal closeout and approved
activation of the complete A7 non-atomic migration contract and conservative
compatibility defaults at candidate `2de6e96e45764e2d21731b2a5e619cc23f8338ca`.
This acceptance is not approval of atomic capability rejection, a release, or
permission to merge the activation delivery PR.

## Boundary and disposition

This record preserves the evidence and archive materials for Milestone A6 —
Machine-Mode Synchronous Trap Entry and Return. Closeout preparation was merged
in [PR #46](https://github.com/mimiqdev/ruscv-sim/pull/46) on
2026-09-18 at `024d15d546dc3b711f593cd44bb107612fd8b600` (merge timestamp
`2026-09-18T10:04:05Z`). The maintainer's subsequent explicit acceptance on the
same date closes A6 and authorizes the A7 plan rotation in this documentation
change. PR #47 carries that activation and is still subject to review and merge;
this record does not claim it is already merged. The original contract is preserved in
[`a6-machine-mode-trap-entry-return.md`](a6-machine-mode-trap-entry-return.md).

The archived A6 contract §9 and `AGENTS.md` closeout sequence is now fulfilled
by the nine-criterion acceptance below, preservation of the complete A6 contract,
and explicit approval of the successor. A7 replaces A6 in `docs/dev-plan.md`;
no A5 record or accepted ADR is rewritten. This change implements the approved
documentation transition only, not A7 Rust implementation. The milestone's
acceptance decision and the activation PR's unmerged delivery state are distinct.

The detailed criterion-to-source/test/command/limitation matrix is in the
[A6 capability assessment](../../verification/a6-capability-assessment.md).
It records evidence rather than checking criteria merely because their task PRs
merged.

## Implementation and review identities

| Work | PR | exact implementation/source head | exact merge head | scope |
| --- | --- | --- | --- | --- |
| Task 1 | [#42](https://github.com/mimiqdev/ruscv-sim/pull/42) | `68495ab14190486b165a14c3907a1f7c8c78e20d` | `ece94463e476075fa89ba6e2dff56e4cf2e2abfc` | Trap CSRs, cause/vector rules, `minstret`, WARL and CSR classification |
| Task 2 | [#43](https://github.com/mimiqdev/ruscv-sim/pull/43) | `8cbf98369de3693c7977aa81bf282f86049d93b1` | `2f14cc5e83869c7a383cb4b9c1ffd706e19cb053` | MRET validation/restoration and MPRV behavior |
| Task 3 | [#44](https://github.com/mimiqdev/ruscv-sim/pull/44) | `8b509939728c7229de359671f82ef800e03c9560` | `51c6e6332f07dceecc0cfae8d54fbfd7c7fb2cab` | Core typed outcomes, trap continuation, runner budgets and exit ordering |
| Task 4 | [#45](https://github.com/mimiqdev/ruscv-sim/pull/45) | `845c63325db5ac87ab2ff0ed260453dc3b396ae9` | `f12b1f80907e92b1c82b33ac669b961cc17f59c9` | Fresh trap guests, induced-fault integration, ELF path guards and evidence |

The Task 4 verification head and the merge head are distinct commits with the
same tree `ce60a0a9c984fd42e43ecbb156e860176ecd6c47`. The exact tree relation is
recorded and checked by the offline ACT4 replay; it does not turn the old
verification command into a command run at a later documentation head.

## Merged preparation CI identity

GitHub PR and Actions metadata checked on 2026-09-18 identify final PR #46
head `90e7dfe470e93be799f1e8e1047ab438e0285516` and
[CI run 35331709689](https://github.com/mimiqdev/ruscv-sim/actions/runs/35331709689).
`Quality and tests` succeeded; Coverage was skipped. Formatting, A5 accounting
controls, Clippy, Rust tests, documentation and guest-script guards succeeded.
The release build/smoke and fresh project ELF compile/run steps were **skipped**
in this PR run. It is not fresh project-guest or ACT4 execution at the merge
head. The local `b8c0bba...` checks and Task 4 fresh evidence below retain their
original identities; no old run is relabeled as `024d15d...` evidence.

Separately, exact-merge [main CI 35332790906](https://github.com/mimiqdev/ruscv-sim/actions/runs/35332790906)
ran at `024d15d546dc3b711f593cd44bb107612fd8b600`. Its `Quality and tests`
job succeeded, including release build/smoke, fresh project ELF compilation and
public CLI execution. The log reports **51 total / 51 passed / 0 failed**,
including the five A6 trap guests. Coverage was skipped. This is new exact-merge
project evidence, not a relabeling of Task 4 evidence and not a new ACT4 run.
The nine criterion rows below retain their original preparation references;
this merge CI additionally verifies their Rust regressions and project guests.
Remaining acceptance boundaries are the frozen external profile's historical
revision and the explicit non-goals. The rolling switch is now explicitly approved.

## Final nine-criterion acceptance

The maintainer accepted all nine A6 criteria on 2026-09-18 on the evidence
below and the detailed [assessment](../../verification/a6-capability-assessment.md).
The retained preparation table records the original evidence identity, not a
new test execution at the activation HEAD.

| Criterion | Final disposition | Basis for acceptance |
| --- | --- | --- |
| 1. Precise synchronous entry and induced faults | Accepted | Cause/state/no-transaction regressions, WARL/CSR tests, exact-merge Rust gate |
| 2. Direct/vectored synchronous BASE | Accepted | Trap/vector tests and fresh vectored guest |
| 3. Non-retirement, budget and counter precedence | Accepted | Core/CSR/RunControl tests, including PR46's zero/recursive assertions at exact merge |
| 4. Handler continuation and distinct trap boundaries | Accepted | Typed outcomes consumed before next turn, consecutive/recursive tests and five guests |
| 5. Legal MRET and MPRV restoration | Accepted | MRET conformance tests and guest return |
| 6. Lower-privilege MRET rejection | Accepted | Typed illegal-instruction/no-return-effects tests and composed guest trap |
| 7. Five public CLI trap ELFs | Accepted | Main CI 35332790906 freshly passed project 51/51, including all five |
| 8. Runner/exit/diagnostics invariants | Accepted | Executor/public/peripheral regressions and handler exit ordering |
| 9. Quality and no regressions in scope | Accepted | Recorded quality gates, exact-merge project run, frozen ACT4 51-case evidence and source identity bridge |

### Historical preparation evidence summary

| Criterion | Preparation disposition | Exact evidence identity | Important limit |
| --- | --- | --- | --- |
| 1. Trap entry and induced causes 0/1/4/5/6/7 | Evidenced in the draft matrix | Rust focused tests; Task 4 fresh tree at `845c633...`; PR45 CI `35328370857` | Flat physical access only; no MMU/PMP/page-fault certification |
| 2. Vectoring and trap guest behavior | Evidenced in the draft matrix | `trap_test`, MRET/vector guests, fresh source `845c633...` | Synchronous Machine-mode profile; no asynchronous interrupt path |
| 3. Outcomes, budget accounting and `minstret` precedence | Evidenced in the draft matrix | Core/CSR/runner assertions; implementation merge `f12b1f8...` | `mcycle`, counter shadows and full ADR-0004 timing remain deferred |
| 4. Handler continuation and non-lossy boundaries | Evidenced in the draft matrix | Typed Hart facts, recursive runner timeout, five guest handlers | Current runner is not the future ADR Machine/observer architecture |
| 5. Legal MRET restoration | Evidenced in the draft matrix | `mret_conformance_test`, core regression, `trap_mret_priv.S` | No SRET/delegation integration claim |
| 6. Lower-privilege MRET rejection | Evidenced in the draft matrix | Helper/composed trap assertions and `trap_mret_priv.S` | Supported User/Supervisor/Machine profile only |
| 7. Five guest ELFs | Evidenced in recorded fresh run | `845c633...`: five A6 guests included in project total 51/51; PR45 CI | Project-authored regression, not ACT4 |
| 8. Runner/exit/diagnostic invariants | Evidenced in current tests and fresh guests | `executor`, peripheral, CLI and fresh ELF checks at `845c633...` | Current UART/HTIF path only |
| 9. Quality/no regressions | Evidenced for preparation | PR45 CI `35328370857`; immutable-image quality/guard/project run; ACT4 replay is separate | No whole-ISA or extension-wide certification |

The complete command-to-assertion mapping, including the test additions used to
make zero-budget and recursive-timeout observations explicit, is maintained in
the [capability assessment](../../verification/a6-capability-assessment.md).
Only commands actually run may be recorded as passed in a final closeout
revision.

## Fresh project verification at the Task 4 tree

The recorded fresh verification used:

```text
source head: 845c63325db5ac87ab2ff0ed260453dc3b396ae9
merge head:  f12b1f80907e92b1c82b33ac669b961cc17f59c9
image:       ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c
host:        Colima ARM64, 4 GB VM
shell:       non-login bash -c
jobs:        CARGO_BUILD_JOBS=2
project:     freshly compiled and run, 51/51 passed
A6 guests:   five included in the project total
```

The same recorded run ran `cargo fmt --all -- --check`, `cargo check
--all-features`, strict all-target Clippy, `cargo test --all-features`,
`cargo doc --all-features --no-deps`, and the ELF guard tests. The exact PR CI
identity is [run 35328370857](https://github.com/mimiqdev/ruscv-sim/actions/runs/35328370857),
whose `Quality and tests` job passed at `845c633...`. This record does not
relabel the project total as ACT4.

## Closeout-preparation worktree checks

The closeout-only additions were committed as
`b8c0bba7e00029bc0d6fd3b6b10c5310ad41dea7`. The checks below were run at that
exact closeout-preparation revision on 2026-09-18. They apply to this branch's
closeout work, not retroactively to the Task 4 source head:

```text
cargo fmt --all -- --check                         passed
cargo check --all-features                         passed
cargo clippy --all-features --all-targets -- -D warnings  passed
cargo test --all-features                           841 passed, 0 failed
cargo doc --all-features --no-deps                  passed
cargo test --test a6_task3_core_trap_test          17 passed, 0 failed
bash scripts/test_riscv_elf_guards.sh              passed
python3 -m unittest discover -s scripts/a6 -p 'test_*.py'  5 passed
```

The local `RISCV_TEST_OUTDIR=target/a6-closeout-riscv-elves
./scripts/run_elf_tests.sh` probe returned explicit unavailable-toolchain status
77 because `riscv64-unknown-elf-as` and `riscv64-unknown-elf-ld` are not
installed on this host. It is not recorded as a pass; the required fresh guest
run is the immutable-image/CI evidence above. `git diff --check` was also clean.

## Retained A5 ACT4 replay

The separate ACT4 run [35325844246](https://github.com/mimiqdev/ruscv-sim/actions/runs/35325844246)
was generated and executed again at `845c63325db5ac87ab2ff0ed260453dc3b396ae9`
with the approved/frozen A5 nontrapping profile. Its read-only artifact is:

```text
artifact id: 10538929657
bytes:       21675875
sha256:      9c33a06e458bb99331aad141c44f3604979139d82502216d04d73290bbabf452
result:      51 generated / 51 executed / 51 passed
```

[`scripts/a6/replay_act4.py`](../../../scripts/a6/replay_act4.py) verifies the
artifact identity, selection and configuration hashes, every strict CLI result,
all linked audits, twelve fail-closed accounting mutations, six guest controls,
and the equality of the source and merge trees. The compact durable output is
[`a6-act4-replay-35325844246.json`](../../verification/a6-act4-replay-35325844246.json).
It is offline retained-evidence replay only: no new guest execution, generated
ELF, large ZIP, private path, or orchestration state is committed. The A5
`replay_accounting.py` remains an historical verifier for its own hard-coded
run and is deliberately not overwritten.

## Limitations and delivery state

- The A6 evidence covers the public flat RAM/UART/HTIF path and the supported
  Machine-mode synchronous trap profile. It does not certify asynchronous
  interrupts, interrupt controllers, MMU/Sv39/PMP integration, compressed
  execution, Debug Mode, multi-Hart ordering, SystemC/TLM public integration,
  OS boot, or M/A/F/D/C end-to-end support.
- The A5 ACT4 51/51 result is explicitly the frozen nontrapping RV64I
  selection. It is not A6 trap evidence and does not enlarge the A6 scope.
- Native hosts without `riscv64-unknown-elf-as`/`ld` skip the optional guest
  integration test unless `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`; the recorded
  immutable-image run supplied the required toolchain.
- Evidence is bound to the exact revisions above. Any implementation change
  after review requires fresh relevant verification; a documentation commit
  does not retroactively change the guest source identity.
- PR #46's preparation is merged. A6 is now formally accepted and the approved
  [A7 contract](../../dev-plan.md) is activated in this documentation change.
  PR #47 is not claimed merged; review and delivery remain separate. No release
  or A7 implementation completion is claimed.
- Existing atomic dispatch is not complete A-extension certification. Split AMO
  read/write, encoding/width debt and global reservation remain explicit legacy
  debt; A7 preserves behavior rather than disabling it. A6 acceptance does not
  assert complete ADR-0002 atomic conformance.

## Evidence applicability to the documentation activation

The approved candidate is `2de6e96e45764e2d21731b2a5e619cc23f8338ca`.
Activation changes only documentation. Exact-head checks compare every tracked
non-document file with PR46 merge `024d15d546dc3b711f593cd44bb107612fd8b600`,
including source, tests, build inputs, scripts and CI. Relevant Git identities:

| Protected path | Git tree/blob at PR46 merge, unchanged by activation |
| --- | --- |
| `src` | `1e0262344bee52ee3c7d616f62035decacddccfb` |
| `tests` | `337d1f3da8548eeb98aa272666087ad1080dfb93` |
| `scripts` | `1e1111777c5f5b7b0f24a89cb1b4227099a9f0c7` |
| `.github` | `759c47ca4e31bf25b9d9d9ea0260a07108f1d2f7` |
| `Cargo.toml` | `2a8ee26506188c29c7731125bf579fd39426071d` |
| `Cargo.lock` | `79e557f5205bb9b70b00bbd0ee0bf138866db71e` |

The ACT4 source `845c633...` has identical `src`, Cargo inputs and CI configuration
to PR46 merge. Intervening tests/scripts add only the documented closeout
assertions and offline replay utilities; they do not change the executed
simulator. This is an evidence-applicability argument, not a fresh ACT4 run.
Current activation checks cover links, complete archived bodies, approved-scope
preservation, the unique active contract and protected-file identity. Historical
Rust/ELF/ACT4 execution remains bound to the revisions above. New activation PR
CI and independent review apply only to their exact new HEAD, not by inheritance
from the candidate review.
