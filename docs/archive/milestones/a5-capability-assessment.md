# A5 — ACT4 RV64I External Compatibility Assessment

**Status:** Formally closed out (PR #37 merged `d1834cc6566b342824bca30772cc829953a5c5ef`).

**Authority:** Informational completion evidence, not a successor contract.

**Evidence completion:** 2026-09-13. **Reconciliation and profile approval:** 2026-09-15.

**Final assessed implementation merge:** `f2edf77f17a76bea3d7062f40f5f4f38ad4eb580`.

## Outcome and approval boundary

A5 establishes **selected external compatibility**, not RISC-V certification:
all **51 required sources / 51 variants / 51 generated self-check ELFs /
51 executed / 51 passed**, with no missing cases, skips or failures. Sail
signature ELFs are separately accounted oracle inputs, not additional DUT passes.
The [original contract](a5-act4-rv64i-external-compatibility.md) is preserved;
the [current forwarding contract](../../dev-plan.md) remains A5 because no
successor has been approved.

The maintainer explicitly confirmed both outstanding profile decisions on
2026-09-15: adopt the narrow UDB MXLEN provenance overlay as the **A5 test
adapter**, and adopt the **naturally aligned EEI**, excluding the eight
successful-misalignment sources. The [decision record](../../verification/a5-selection-proposal.md#final-decision-and-evidence)
preserves their original rationale and exact IDs. Approval of these decisions
does not authorize architectural trap delivery, privileged startup, or a
whole-machine capability claim.

The successful run occurred before confirmation, on the unchanged proposed
profile. Approval freezes that same selection; it does not select a green subset
after execution. The original 44/51 failed run and seven blockers remain
historical evidence, and both bounded ISA repairs have fail-before regressions.

## Five acceptance criteria

| Contract criterion | Disposition and repository evidence |
| --- | --- |
| 1. Feasible pinned generation | Satisfied. [Pins](../../verification/a5-feasibility-pins.json), [setup and unsuccessful attempts](../../verification/a5-feasibility.md), [provisioning](../../../scripts/a5/provision.py#L1), and [experiment](../../../scripts/a5/experiment.sh#L1) establish ACT4 4.0.0 `a7c9930`, Sail 0.10 and the recorded compiler/dependency pairing. The final clean Linux run provisions rather than reusing unexplained generated ELFs. Linked audits reject undeclared instructions; invalid MXLEN 128 is rejected. All 1,970 tracked assembly sources are classified before the DUT runs. |
| 2. Fail-closed public execution | Satisfied. [Accounting](../../../scripts/a5/accounting.py#L1) verifies exact source/variant/ELF identities, output hashes and complete results. [Classifier](../../../scripts/a5/cli_result.py#L1) requires a single complete consistent terminal host block. Each selected ELF uses release `ruscv-sim run`, 1,000,000 cycles and 60 seconds host limit. The final CI runs 23 Python tests and six real CLI controls, including a known failing guest. The earlier feasibility run separately proves expected-value corruption. Missing/extra/duplicate/empty results, missing/empty/extra generated artifacts, failed generation and malformed/ambiguous output cannot pass. See control scope below. |
| 3. Full selected compatibility | Satisfied. [Final results](../../verification/a5-final-results.json) retain all 51 identities, generated hashes, invocations, outcomes and diagnostics, with zero errors. [Branch regression](../../../tests/public_behavior.rs#L137) and [FENCE regression](../../../tests/public_behavior.rs#L178) accompany the [branch implementation](../../../src/isa/rv64i/branch.rs#L1) and [FENCE semantics/tests](../../../src/isa/rv64i/fence.rs#L1). G-13/G-14 preserve causal pre-fix evidence. Base-I FENCE is an ordered synchronous single-Hart no-op; FENCE.I remains excluded Zifencei. |
| 4. Reproducible CI evidence | Satisfied. [ACT4 run 34766332271](https://github.com/mimiqdev/ruscv-sim/actions/runs/34766332271) succeeded at the exact final implementation merge in 4m41s on 2026-09-13. [Manual workflow](../../../.github/workflows/a5-feasibility.yml#L1) provisions pins, generates using Sail, runs controls and the full selection, and retains logs, configuration, audits, generated artifacts and results for 14 days. The entire final ZIP was downloaded and its size/SHA-256 verified during reconciliation; the replay below verifies all identities and hashes without new generation. |
| 5. Capability acceptance | Satisfied: formal closeout accepted via PR #37 merge (`d1834cc6566b342824bca30772cc829953a5c5ef`) with independent review and exact-head CI recorded. Exact committed implementation reviews and six-command Rust gate records are below. [Standard push CI 34749931575](https://github.com/mimiqdev/ruscv-sim/actions/runs/34749931575) succeeded at the same `f2edf77` and separately compiled/executed **46/46 project-authored guests**, alongside release/smoke and Rust checks. Current-state, matrix, gap and external-test navigation now distinguish this selected evidence from component support and historical pending statements. No successor is approved. |

## Source accounting and unchanged-input proof

The [inventory](../../verification/a5-source-inventory.json) retains every exact
source ID, hash, required extensions, parameters, purpose, exclusion reason and
coverage consequence:

| Disposition | Sources |
| --- | ---: |
| Required nontrapping RV64I | 51 |
| Successful misalignment, explicitly approved EEI exclusion | 8 |
| Other RV64I extensions | 654 |
| Privileged purpose | 355 |
| Other base profiles (RV32I / RV32E / RV64E) | 902 |
| **Entire pinned source tree** | **1,970** |

The final CI log independently reports these counts. Inventory unit tests verify
uniqueness, exact totals, every source's rule-derived disposition and all eight
misalignment parameters. The selected set includes FENCE, not an exclusion for a
previously failing instruction. ECALL/EBREAK architectural trap delivery and
unavailable concurrent/device ordering remain outside the profile.

The final report preserves the original run's `proposed-not-frozen` metadata:
inventory SHA-256 `cfe80868322c76825cef3ac89701ce775bc81f6e6740150a8ec027fed7e23f1a`,
profile SHA-256 `3c53942a8e3cae9842d0e94a98deb463e27f08f8375af12750a16f5a87575a78`,
and tool-pin SHA-256 `1fe4c4882e338b79ffd75f6fa216c4589cf90a5e649d4d3157eff38b2f7fe9f1`.
Only the checked-in profile/inventory status bytes change to `approved-frozen`.
[Approval rebinding](../../../scripts/a5/replay_accounting.py#L19) reverses
exactly that single metadata substitution and verifies the old byte hashes,
entire source/variant plan, profile ID and tool-pin hash. A semantic change
cannot pass this comparison. Historical IDs, file names and the original list of
two approval requirements intentionally remain stable.

Replay also verifies every archived executable configuration hash, compares
checked-in linker/macros/configuration bytes (including the overlay path appended
by the existing adapter), every generated ELF/signature/results hash and every
strictly reclassified CLI result. The overlay's original and adapted hashes,
complete tool versions, simulator binary hash and configuration hashes are
retained in the final JSON. No Rust, oracle, upstream source, generation command
or executable configuration changes accompany closeout. Workflow display names
are updated, not job semantics. Approval metadata and local evidence replay do
not require gratuitous ACT4 regeneration.

## Negative controls and evidence limits

- Final ACT4 CI ran the original **23 Python tests (11 parser + 12 inventory/
  accounting)** before provisioning. The same fail-closed functions used by the
  suite reject missing/duplicate/extra/empty identities and results, missing/
  empty/extra ELF/signature/expected-result outputs, and generation return codes
  `1`, `-9`, `None`, and `False` even when every output exists. Failed summary
  or control assertions make the command and enclosing job fail.
- Six actual release-CLI controls use the same `accounting.execute` function:
  pass, deliberate guest fail, 1,000-cycle nontermination, invalid instruction,
  malformed ELF and missing ELF. All matched expectations. The separate
  expected-result corruption experiment retains the intact/corrupted self-check
  signaling evidence, without adapting the oracle to the DUT.
- Host wall-time is verified with a sleeping fake process through that same
  execution function, not claimed as a real Linux guest wall-time expiration.
  Failure category plus reason and retained `execution_result.error`/stderr
  distinguish cycle/wall timeout, invalid/unsupported instructions, simulator
  errors and launch/input failures. Unsupported instructions are not guest fails
  or passes; the top-level category remains `simulator-or-runner-failure`.
- Reconciliation replays **12 negative accounting assertions on the actual
  final artifact** (including missing result, failed generation and extra
  `.elf`/`.sig`/`.results`) and reclassifies all 51 outputs and six guest controls.
  These are replay assertions, not fresh guest execution or Sail generation.
- Closeout adds two Python regression tests for approval-only hash rebinding
  (rejecting changed fields) and final evidence schema/identities/outcomes.
  The current local suite is **25 tests**. Existing 23-test CI evidence is not
  relabeled as a historical 25-test run.

## Exact implementation reviews and Rust verification

| Work | Independent review / merge evidence |
| --- | --- |
| Feasibility/classifier, PR #32 | [Review](https://github.com/mimiqdev/ruscv-sim/pull/32#issuecomment-5637365265) approved `c66597be4d03ca83f4005129037984e13620fe82`; [identity review](https://github.com/mimiqdev/ruscv-sim/pull/32#issuecomment-5637595443) approved identical-tree `97f606f` and successful required CI. Merged `d568d45093bb643884740c751fa595f0f593f4dc`. |
| Complete accounting, PR #33 | [Review](https://github.com/mimiqdev/ruscv-sim/pull/33#issuecomment-5638249260) approved `0d0d2d9e5eb209e0ef9ff754cbb2f6e1aaf4157f`, including 1,938 independent identity mutations; required CI `34627722877` succeeded. Merged `727f51ea393a7e3ee31309789d07d8dc0f50aac0`. |
| Branch repair, PR #34 | [PR evidence](https://github.com/mimiqdev/ruscv-sim/pull/34) and [G-13](../../verification/public-behavior-gaps.md#g-13--rv64i-conditional-branch-used-a-12-bit-sign-extension-for-a-13-bit-b-immediate) preserve full six-command gate, fail-before public/unit regressions and six retained branch ELF failures changing to passes. Merged `978cce0a44a9a25a7ec6d7716b651a3da4f7c1de`; the final merged tree is additionally covered by the later exact reviews and fresh CI below. |
| FENCE repair, PR #35 | [Review record](https://github.com/mimiqdev/ruscv-sim/pull/35#issuecomment-5652411741) approves `070d8f5` and dispatcher alignment `8380f2e`, with full six-command gates and hash-verified 51-ELF replay; [final docs review](https://github.com/mimiqdev/ruscv-sim/pull/35#issuecomment-5652413335) approves `7ba4038`, required CI successful. |
| Separate memory robustness repair, PR #36 | [Independent review](https://github.com/mimiqdev/ruscv-sim/pull/36#issuecomment-5652392820) approves `f32db0a5549b33ba012eae3b79637a56274549d4`: full six-command gate, release tests, 11 debug and 11 release memory regressions, and 23 Python tests. [Merge-resolution review](https://github.com/mimiqdev/ruscv-sim/pull/36#issuecomment-5652492178) approves exact `4139c54217b3365e78695c451b22985c115323f6`, verifies unchanged reviewed memory/FENCE blobs, and passes fmt, memory debug/release, 46 public tests and focused FENCE tests plus required CI `34749828993`. Merged as final `f2edf77`. |

The six-command gate is `cargo fmt --all -- --check`,
`cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, and
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
These are recorded exact-head implementation checks, not new closeout Rust runs.
This closeout changed Python evidence handling, approval metadata and documents,
not Rust code. Closeout PR #37 supplied its own exact-head quality evidence,
recorded independent review, and merged as `d1834cc6566b342824bca30772cc829953a5c5ef`.

## Retrieval and durable evidence

Final artifact **10320897156**, **21,675,966 bytes**, expires
**2026-09-27T15:45:37Z**; verified full ZIP SHA-256:
`bb851d5486d16fe47d68fcc757f610fda7f1d2e3a2c431848dea35b7b514beb9`.

```bash
mkdir -p .a5
gh api repos/mimiqdev/ruscv-sim/actions/artifacts/10320897156/zip > .a5/final.zip
python3 scripts/a5/replay_accounting.py .a5/final.zip --final
python3 -m unittest discover -s scripts/a5 -p 'test_*.py'
bash -n scripts/a5/experiment.sh
```

Run from the repository root. The replay requires the entire hash-verified ZIP;
partial downloads fail. No signed download URLs, generated ELFs, archive ZIPs or
large raw generation logs are committed. The bounded
[final JSON](../../verification/a5-final-results.json) durably retains all
per-case results/diagnostics, tool/configuration hashes, audit summaries and
control outcomes after Actions retention expires. Re-generating binaries after
expiration requires the pinned setup and a separately recorded run; a report
alone does not reconstruct generated ELF bytes. The earlier
[44/51 accounting record](../../verification/a5-accounting-results.json) and
feasibility reports are preserved unchanged.

## Limitations and re-evaluated unfinished work

One Hart, RV64I, little-endian native RAM/UART/HTIF, physical identity addresses,
natural data alignment and nontrapping tests only. No successful-misalignment,
architectural trap-entry/return, CSR/privilege, interrupt, MMU/PMP, multi-Hart,
concurrent/device-ordering, FENCE.I, OS boot, SystemC/TLM or whole-ISA
certification is established. Project-authored guests and component tests do
not expand the 51-source external claim.

G-13/G-14 are repaired; G-15 is the separately authorized bounds robustness
repair, not an EEI expansion. Remaining architecture and verification gaps retain
their prior limits. The present host does not supply new separately compiled
guest or ACT4 execution evidence; exact Linux runs do. No 32-bit-host execution
or global tool installation is claimed.

**One optional successor recommendation:** consider bounded machine-mode
synchronous trap-entry/return end-to-end verification, with a separate
architecturally reviewed contract before implementation. It is **unapproved**;
this is not an A6 contract, an automatic carry-forward or implementation
authorization. Formal closeout in PR #37 accepted A5 only.
