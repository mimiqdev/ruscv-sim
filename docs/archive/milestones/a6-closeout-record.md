# A6 Closeout-Preparation Record

**Status:** Draft closeout preparation; not a formal milestone closeout

**Authority:** Historical informational record. The active normative contract
remains [`../../dev-plan.md`](../../dev-plan.md) until a separately approved
successor is available.

**Prepared:** 2026-09-18

## Boundary and disposition

This record prepares the evidence and archive materials for Milestone A6 —
Machine-Mode Synchronous Trap Entry and Return. It does **not** claim that this
closeout-preparation PR is merged or that the rolling milestone switch is
complete. The original contract is preserved in
[`a6-machine-mode-trap-entry-return.md`](a6-machine-mode-trap-entry-return.md).

Per `docs/dev-plan.md` §9 and `AGENTS.md`, final rolling closeout is a later
operation: all nine criteria must be rechecked against the committed reviewed
head, this record must be accepted, and a successor contract must be approved
before `docs/dev-plan.md` can be replaced. No successor is approved here. This
work therefore leaves the A6 contract text in `docs/dev-plan.md` unchanged; it
does not restore an A5 forwarding record, enable A7, migrate old backlog items,
or weaken documentation policy. Archive preparation and the final rolling-plan
switch are separate decisions.

The detailed criterion-to-source/test/command/limitation matrix is in the
[draft A6 capability assessment](../../verification/a6-capability-assessment.md).
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

## Acceptance evidence summary

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

## Limitations and pending state

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
- This record remains pending independent review and merge of the closeout
  preparation. It does not state that this PR is complete, merged, released, or
  a replacement for `docs/dev-plan.md`.

## Final rolling-switch prerequisites

Before a future maintainer performs final A6 closeout, the reviewed committed
head must be checked against all nine rows, the closeout record must be updated
with that exact head and resolved findings, and a separately approved successor
contract must exist. Only then may the completed contract be moved as a final
historical record and `docs/dev-plan.md` be replaced. Until that decision, the
current A6 contract remains in place unchanged.
