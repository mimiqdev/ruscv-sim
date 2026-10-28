# Stage 6 cumulative review

**Status:** Current cumulative review record. The initial findings at `4332841` are corrected and independently reviewed at `5c1d031`; final quality evidence is recorded separately.

**Authority:** Informational verification evidence, constrained by [the Stage 6 contract](../dev-plan.md), accepted ADR-0001–0004, and the [verification policy](README.md).

**Initial inspected implementation:** `4332841a565fb845952cd52ccf84283959f373bf`, source tree `3457fb9793b722f06ac364a363235b927522b46e`. The initial review covers U1–U6, not only the U6 deletion diff. The corrected implementation and independent verdict below apply to `5c1d031b2a66bc17d058b5a7355d88ebe80eb652`. The [acceptance overview](stage6-closeout/README.md) maps all nine criteria to evidence.

**Specification:** RISC-V Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`; ISA Manual document version 20250508, tag commit `584390d9d872835b1bc45ab9ea946b56db3ad485`.

## Initial verdict

**REQUEST_CHANGES.** Independent cumulative source review found two reset-boundary defects. Both were reproduced before changing production code. Regression commit `654dda9fe63cf9d05d39ed6901700e73ca4edb95` adds the failing tests. Passing baseline tests and CI do not establish the missing combinations.

Three independent read-only review contexts covered Hart/CSR, Debug Module/Machine, and transport/tooling/deletion. Reviewers inspected implementation and actual test assertions; they did not execute tests or mutations. Local execution and CI evidence below are recorded separately. The previous PR #104 review covered U6 and retained affected paths; it is not substituted for this cumulative review.

## Findings

| Finding | Trigger and observed result | Required correction |
| --- | --- | --- |
| F1. Unresolved abstract-memory completion can be bypassed by reset. | A halted abstract store through the controlled unknown-completion backend reports `cmderr=7`. An immediate `ndmreset` succeeds. `machine::tests::abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse` fails with `ndmreset bypassed an unresolved abstract memory completion`. | Admit debug work into the Machine's lifecycle boundary and publish the Hart's unresolved-completion fact before replying. Refuse reset/reuse without adapter proof, preserve error inspection, and prevent an in-flight DMI operation from being treated as drained. |
| F2. Non-DM reset loses hart 0's halt request. | Write `dmactive | ndmreset | haltreq`, then release reset with `dmactive | hartsel_field(1)`. `ndmreset_keeps_hart_zero_halt_request_when_selection_changes` reads `haltsum0 & 1` as `0`, expected `1`. | Preserve the DM's per-hart request across Hart reconstruction, apply writes only to the newly selected hart, and preserve explicit clear and DM-reset behavior. |

At the initial 433 review, F1 violated the unknown-completion boundaries in ADR-0002, ADR-0003 Reset, and ADR-0004 §12.2. `RiscvCore::debug_mem_fault` already set the Hart latch; `OwnedMachine::apply_dmi` omitted the Machine receipt used by ordinary `Installed::turn`.

F2 violated the fixed Debug specification's Run Control and `dmcontrol.haltreq` semantics and the selected Stage 6 reset-entry behavior. The request was stored only on the old Hart. The original reset test reasserted `haltreq` on release and therefore did not detect its loss.

The focused commands at regression commit `654dda9` are:

```bash
cargo test --all-features --offline --lib \
  machine::tests::abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse \
  -- --exact --nocapture
cargo test --all-features --offline --test s6_u3_machine \
  ndmreset_keeps_hart_zero_halt_request_when_selection_changes \
  -- --exact --nocapture
```

Both exit 101 after running one test and failing the intended assertion. F1 must remain blocked even if the retained backend operation later completes. F2 must halt at the reset entry before retirement while selection remains on nonexistent hart 1.

## Corrected implementation and independent review

The corrections are committed in `5c1d031b2a66bc17d058b5a7355d88ebe80eb652`, source tree `6f488e7788508f3a79a40f1d92e329c5eb26b8b5`. A fresh independent read-only PR-head review returned **APPROVE**, with no new runtime finding, for that exact head and base `4332841`. Its [public review record](https://github.com/mimiqdev/ruscv-sim/pull/107#pullrequestreview-5678303368) records the independent code verdict as a GitHub review comment, not a platform approval by a second maintainer. The [regression receipt and logs](stage6-closeout/reset-regressions/receipt.json) preserve both red tests at `654dda9` and green results at `5c1d031`.

DMI operations now acquire an existing-style work lease before touching the DM/Hart and commit unresolved facts through `Lease::boundary` before replying. The failed Machine keeps DMI error inspection and W1C, but refuses new abstract commands, reset, and resume. A controlled in-flight backend proves lifecycle mutation returns Busy before completion, then UnknownCompletion after the unresolved result, without a new generation.

The DM now owns hart 0's persistent request. All managed request writers update that owner, and `Machine::publish` drives the fresh Hart while publishing locks are held. The corrected tests prove request preservation across selection changes and ordinary reset/install, explicit clear and DM reset, managed setter cancellation, and failure to publish on a poisoned DM lock.

The reviewer ran 100 focused tests and checked the successful current debug-attach CI log, the matching merge/source trees, deletion references and unchanged A10 boundary. It did not run the heavy local gate or local OpenOCD. The code verdict is separate from the required full quality result. A later documentation or rotation head requires applicable fresh verification and review.

## Contract-to-source and test audit

The table records correspondence at the corrected `5c1d031` implementation, with the initial findings labeled where relevant. Execution identities appear in the next sections. A named test is not by itself an execution claim.

| Contract clause | Implementation and focused proof |
| --- | --- |
| §7.1, Debug CSR values and WARL | `src/core/debug_mode.rs` and public Hart accessors; `dcsr_dpc_and_scratch_exist_with_the_decided_warl_rules`. Guest CSR access is rejected by the existing CSR view and SYSTEM validation. |
| §7.1, halt causes and saved state | `RiscvCore::enter_debug` and `finish_step`; `each_halt_cause_reports_dcsr_cause_prv_and_dpc`, both EBREAK tests, and the supervisor redirect test. |
| §7.1, WFI, step, interrupts, and resume privilege | Pre-fetch `execute_turn` decision and `debug_resume`; focused tests cover halt during WFI, stepped WFI, pending timer masked during step, stepped ECALL, halt before interrupt, and MPRV clear below M. Halt-first is the approved D6 choice, not a universal specification priority. |
| §7.1, illegal CSR and reservation | `guest_csr_access_to_debug_and_tselect_is_illegal`, `sc_after_a_halt_fails`; guest DRET remains illegal for this no-execution-in-Debug-Mode profile. |
| §7.1, frozen time and retained retirement facts | `Installed::turn` skips clock advancement for halted entries. `mtime_does_not_move_while_the_hart_is_halted`, stepped retirement/guest-exit tests, and native Runner U3 tests cover the observable boundaries. |
| §7.2, discovery and command errors | `src/dm/mod.rs` and `abstract_cmd.rs`; version discovery, unsupported size/postexec/virtual options, FPR error, running error, sticky `cmderr`, and bitwise W1C tests. |
| §7.2, GPR/CSR access and physical memory | GPR/misa WARL round-trip, x0, short access, physical MPRV bypass, native PLIC claim consumption, misalignment, and legacy Hart data-path tests. Flat proves RAM only. |
| §7.2, simulator faults | `debug_mem_fault` distinguishes target exceptions from backend/protocol/unknown errors. Bare-Hart fault tests cover the latch; the corrected Machine tests for F1 cover uncertainty propagation and in-flight admission. |
| §7.2, selection/reset state | Selection, nonexistent, havereset, resumeack and register-set tests, plus the F2 reset/selection regression and persistent-request/clear tests added at 5c. |
| §7.3, admission/reset/frozen counts | `DebugHub::post`, `OwnedMachine::debug_boundary`/`admit_direct`, shared native/flat Runner paths; cross-thread halt, reset image/DM preservation, four frozen counts, WFI and flat RAM round-trip tests. The corrected F1 and F2 regressions cover reset combinations absent at 433. |
| §7.4, TAP and DMI | `src/jtag/mod.rs`, `session.rs`, and `tests/s6_u4_remote_bitbang.rs`; reset IDCODE, unknown-IR BYPASS, DTMCS fields, DM version, sticky error/reset, flags, native/flat halt entry, running boundary polling and transport close. |
| §7.5, OpenOCD/GDB/Spike | The saved U5 session, command assertions, named comparison and two negative controls remain in [the original U5 exit record](stage6-exit.md) and `stage6-u5/`. The main CI requires the live Spike comparison. |
| §7.6, deletion | `src/debug/` and its crate exports are absent. Searches for all 19 deleted public definitions find no surviving build dependency; `ExceptionCause::Breakpoint` is a distinct surviving enum variant. |
| §7.7, A10 boundary | `tools/a10/` is unchanged from Stage 6 activation `a8851c4` to the inspected implementation. The public oracle and suite pin are unchanged. Existing schema fields are not a new oracle pin. |
| §7.8, gate | Full local gate and CI pass at corrected implementation 5c. The 433 baseline remains separately labeled below. Neither is execution at a later documentation head. |
| §7.9, independent final review | Initial cumulative review at 433 requested changes. The corrected 5c code verdict is APPROVE. This documentation PR still requires independent review and applicable CI at its own final head. |

## Executed baseline evidence

The [local gate receipt](stage6-closeout/baseline-local-gate/receipt.json) records the exact implementation commit and tree, UTC times, tool versions, all command exits, and checked logs. On 2026-10-28 it passed fmt, all-feature check, strict all-target Clippy, all-feature tests, documentation, and fresh guest compilation/execution. The Rust run reports 2033 passed, zero failed or ignored, across 82 test summaries. The guest run reports 85 compiled and 85 passed, zero failed.

The first guest invocation used a nested output path that the guard rejected before compilation, exit 2. The retained successful invocation uses a fresh permitted direct child of `target/`. This invocation error is not recorded as a guest pass; the [manifest](stage6-closeout/baseline-local-gate/guest-manifest.txt) identifies the successful source and output.

Main CI run [40628551229](https://github.com/mimiqdev/ruscv-sim/actions/runs/40628551229) passed on the same implementation commit. Its raw logs confirm the Rust quality gate, 85 fresh guests, and the required OpenOCD/GDB and Spike sessions with both negative controls. Coverage was skipped by configuration. Local and CI runs are separate evidence; the local run did not execute OpenOCD.

[U1–U4 mutation records](stage6-closeout/mutations/README.md) now preserve patches, exact commands, expected assertion failures, restored-source passes, and the reachable implementation commit. They supplement earlier PR descriptions. U5 retains its separate tool-level controls; U6 has no behavior mutation.

## Executed corrected-source evidence

The [5c local gate receipt](stage6-closeout/fixed-local-gate/receipt.json) records a clean source at `5c1d031` and tree `6f488e77`. It ran from 2026-10-28T05:34:41.029109+00:00 to 05:41:17.819245+00:00. All six commands exited 0. The saved Rust log reports 2039 passed, zero failed or ignored, across 82 summaries. The fresh guest run compiled and passed all 85 cases. The receipt, logs, manifest, and captured ELF identities are retained without changing measurements. Artifact hashes identify the captured files; they are not functional oracle pins. This host gate did not run OpenOCD.

[CI run 40638546079](https://github.com/mimiqdev/ruscv-sim/actions/runs/40638546079) passed Quality and tests and Debug attach for PR #107 head `5c1d031`. Coverage was skipped. Its checkout was the synthetic merge `f7a2bb1e99fbba34fa5da9e3160e261f31870129`, whose tree `6f488e7788508f3a79a40f1d92e329c5eb26b8b5` equals the PR head tree. The [CI record](stage6-closeout/fixed-ci.json) records 2039 Rust passes, 82 summaries, zero failed or ignored, 85 fresh compiled/passed guests, live Spike, and both tool negatives. This is CI execution on the identified checkout, separately from the local gate and code review.

No missing 5c full local gate remains. PR #107's corrected code and this documentation delivery still require owner-controlled merge. The final documentation PR head needs its own applicable CI and independent review recorded in Git/PR history. No future head, run, or review is asserted. Stage 6 remains the current contract; this record does not formally complete or rotate it.

## Review boundaries and limitations

- This is a single-hart, RV64IMA/Zicsr, M/S/U profile with the selected trap, interrupt, WFI and Debug Mode rules. It is not full ISA or Debug Specification certification.
- Program buffer, SBA, triggers, a second hart, and execution in Debug Mode are outside Stage 6. The abstract-command module remains the approved temporary implementation.
- U5's GDB/OpenOCD processes are stopped by the script after guest exit and a recorded transport disconnect; GDB itself does not return from its last continue.
- The saved Spike outputs match the documented differences. The comparison helper also accepts equality in fields where it permits those specific differences; the saved run, fixed profile tests and raw values are the evidence for the actual pair.
- At `5c1d031`, preservation tests start with nonzero trap/debug causes and the native halted-time test reads CLINT `mtime` directly through Abstract Access Memory. These strengthened assertions passed locally and in the independent focused review; they are not retroactively attributed to `4332841`.
- Performance-certification comparison, publication and calibration remain deferred. The A10 oracle is not sealed or rewritten.
