# A0 Closeout Assessment

**Status:** Draft — pre-review assessment, not milestone acceptance

**Authority:** Informational; does not change the active milestone or accept an ADR

**Assessment date:** 2026-09-07

**Scope:** A0 acceptance evidence, consistency of the four proposed boundary contracts, and prerequisites for selecting the first implementation milestone

## Conclusion and authority

A0 is **not ready to close**. The four proposed ADRs provide a substantial working
baseline, with diagram, recovery and ordering clarifications prepared for review.
Formal review, explicit acceptance (including placement alignment), and an
approved successor remain outstanding. This assessment is not an independent formal review or a
claim that the target architecture is implemented.

[The active plan](../dev-plan.md) remains the only current milestone contract.
[The ADR index](decisions/README.md) records all four decisions as Proposed and
none as Accepted. Existing normative principles and product boundaries remain in
force; this assessment does not silently resolve their differences from proposed
contracts. The [successor scope candidate](first-implementation-scope.md) is a
proposal for approval, not a second active plan.

The assessment covers all eight Markdown documents under `docs/architecture/`
that existed before this assessment: the target views, principles, current-state
inventory, ADR index, and ADR-0001 through ADR-0004. Related milestone,
documentation-policy, development-environment, verification, source, test, and CI
references supply the evidence below. New draft documents explicitly state their
scope and do not claim implementation completeness.

## A0 acceptance evidence matrix

The criterion text below comes from `docs/dev-plan.md`. An existing document is
evidence that a contract has been written, not that it has been approved or
implemented. This matrix assesses evidence; it is not a separate live task register.

| A0 acceptance criterion | Repository evidence | Assessment and remaining evidence |
| --- | --- | --- |
| Every active architecture document has an explicit status and ownership boundary. | [Target views](README.md), [principles](principles.md), [inventory](current-state.md), and [ADR index](decisions/README.md) with its four records state status/authority and describe their relevant scope or ownership. | Document-level evidence exists. The target diagrams now distinguish dependencies, Machine invocation and no-fetch interrupt entry. Placement wording still requires the explicit acceptance-time alignment below. An index has navigation scope, not runtime ownership. |
| The development image builds successfully and can run the current project quality gate and guest ELF toolchain. | The active plan's recorded baseline evidence cites `f7db92d` and `6fc0976`; [development commands](../development-environment.md) and the [image workflow](../../.github/workflows/dev-container.yml) retain the build/quality/guest gate. | Already satisfied in the active plan's evidence register; this assessment does not reopen that acceptance. No fresh container result or image digest was obtained here. Workflow source defines a gate but is not itself a new successful run. |
| The target dependency direction is unambiguous from frontend to infrastructure. | [ADR-0003 §1](decisions/0003-runner-machine-and-platform-ownership.md#1-vocabulary-and-dependency-direction) gives `Frontend → Runner → Machine → {Hart, Platform}` and Hart-to-port dependencies. | Text and revised diagrams express the same dependency direction, subject to review. Result production is a Runner dependency; backend implementation arrows point to the port. No return-data arrow authorizes an upward code dependency. |
| Hart, Runner, Machine, Platform, memory access, interrupts, time, and observation responsibilities are agreed. | [ADR-0001](decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md). | Defined as working contracts, not yet agreed through acceptance. The recovery precondition is now explicit in Proposed ADR-0004 §12.2; it and the other refinements still require review and acceptance. |
| ISS and VP product forms can be explained as configurations around one architectural engine. | [Target §6](README.md#6-one-execution-engine-multiple-product-forms) and [ADR-0003 §8](decisions/0003-runner-machine-and-platform-ownership.md#8-standalone-iss-and-future-vp-configurations). | Explanation exists for native and externally hosted forms, with N=1 as the initial configuration. This is an architectural explanation, not VP integration evidence. |
| The current code has been mapped to the target architecture without overstating integration. | [Current-state inventory](current-state.md); focused source checks and test results below. | Inventory exists and the sampled core/executor boundaries agree with it. Focused tests do not establish full ISA, interrupt, MMU, debug, TLM, or ACT4 integration. Weak public-path assertions must not be cited as behavioral proof. |
| Open architecture decisions and deferred performance work are explicitly recorded. | Deferrals in all four ADRs and [principles: future performance work](principles.md#future-performance-work). | Deferrals now distinguish ADR-0004's canonical ordering from deferred scheduling/coherence/transport mechanisms. Concrete Rust representations and acceleration remain outside A0; reconciliation still requires review. |
| The next implementation milestone is approved and replaces A0 as the only current milestone contract. | [Successor scope candidate](first-implementation-scope.md); [active plan closeout](../dev-plan.md#closeout). | Not satisfied. Scope/profile/compatibility choices must be approved before archiving A0 and installing exactly one successor contract. |

The plan also records the documentation reset and target-view baseline at
`50d8c739f686b218a7e4fcc99619079d97833705`. Those recorded deliverables are not
reclassified as unfinished merely because acceptance of later boundary decisions
is pending.

## Cross-contract consistency

These are pre-review observations of the proposed texts, not formal approval.
Section references identify the semantic owner; the other records consume it.

| Boundary | Cross-check | Assessment |
| --- | --- | --- |
| Retirement, traps, and observation | ADR-0001 §§1–6; ADR-0003 §5; ADR-0004 §§3–4 | Consistent distinction between retirement, completed trap entry, and simulator failure. Always-present control facts do not require optional per-instruction records or Runner callbacks. |
| Physical fault and transaction atomicity | ADR-0001 §2; ADR-0002 §§4–6; ADR-0004 §5.3 | Target/device faults remain architectural access-fault inputs; host failures and unknown completion remain simulator failures. Successful A/D writes are separate physical effects, not rollback of the later instruction access. |
| Atomic semantics and reservations | ADR-0001 §6; ADR-0002 §6 | Hart owns arithmetic, architectural reservation state and SC result; the physical domain supplies indivisibility and competing-write visibility. Mechanism selection must not turn this into ordinary visible read/write emulation. |
| Composition, placement, and inspection | ADR-0003 §§1–4 and §7; ADR-0002 §1 | Hart-initiated execution access differs from host-side image installation and inspection. The proposed placement refinement requires normative-document alignment below. |
| Interrupts, WFI, and counters | ADR-0001 §§1–2; ADR-0003 §3; ADR-0004 §§3, 6, 8 | Hart/profile owns eligibility, wake decisions and ISA counter deltas. Machine admits inputs and grants turns; waiting re-evaluation is control-only, not a retired instruction or budget slot. |
| Exit and coincident stops | ADR-0001 §7; ADR-0003 §§6–7; ADR-0004 §§9–10 | Successful exit-causing instruction retires first. Machine retains all facts; Runner selects the primary reason. A completed exit on the final budget slot is not a timeout-only result. |
| Budgets and CLI compatibility | ADR-0003 §9; ADR-0004 §§3.1, 5.1, 7.1 | Different quantities are intentional: legacy successful-step count is not started-turn budget, ISA cycles, or virtual time. A compatibility adapter is explicitly allowed; its concrete mapping remains an implementation-scope decision. |
| Lifecycle and uncertain completion | ADR-0003 §3; ADR-0004 §§5.3 and 12.2 | ADR-0004 §12.2 now requires proof of completed/terminated work before drain, and distinguishes lifecycle safety from prior-state trust. ADR-0003 consumes this clarification; acceptance remains pending. |

## Alignment and acceptance questions

### Placement wording: an explicit acceptance-time refinement

[Principles: address ownership](principles.md#address-ownership) says that ELF
segment placement is a loader responsibility. [ADR-0003 §4](decisions/0003-runner-machine-and-platform-ownership.md#4-elf-parsing-image-placement-and-address-meaning)
explicitly refines this into loader-produced metadata, Machine-coordinated
installation, and Platform physical writes, and explicitly requires alignment
when accepted.

**Acceptance-time wording proposed for the principles:**

- Runner owns image-loading orchestration, limits, ruscv-sim stop taxonomy,
  result production and observer demand/delivery.
- The loader validates the image and describes segments, zero-fill, entry and
  signature/tohost metadata. Machine coordinates installation; Platform performs
  physical placement and routing. Hart owns execution-time translation; an ELF
  base offset is not architectural translation.

Apply that wording only together with explicit acceptance of ADR-0003. Until
then, the principles remain unchanged and normative; the proposed split is not
silently adopted by this assessment. This is a known refinement, not an
implementation defect.

### Target diagrams: distinguish dependencies, data flow, and omitted layers

[Target §3](README.md#3-logical-layers-and-dependency-direction) places report
production under Runner and marks concrete backends as implementations of the
physical port, not dependencies of that port. Its placement edge explicitly
retains the current principles pending ADR-0003 acceptance.
[Target §6](README.md#6-one-execution-engine-multiple-product-forms) explicitly
shows the single-Hart Machine between ISS Runner and shared Hart semantics.

The [runtime view](README.md#7-runtime-control-time-and-events) separates accepted
interrupt entry (no fetch) from instruction attempts, and shows Runner-owned
optional sink delivery without Hart re-entry. Zero budget, effective stop,
reached deadline and Waiting single-step do not cause idle advancement. These
clarifications express existing boundary rules; they do not claim implementation
changes or formal review approval.

### Unknown completion: proposed recovery precondition

[ADR-0004 §12.2](decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md#122-quiesce-and-drain)
now makes the conservative interpretation explicit: ordinary reset always
requires `DrainComplete`. After unknown completion, the adapter must first prove
that operations finished or were conclusively terminated and no late effects can
reach the composed state. Timeout or a cancellation request alone is insufficient.
Without that proof, the Machine remains failed and cannot reset or resume.

After proven drain, lifecycle mutation is safe but prior state may still be
untrustworthy. Resumption requires resolved state/time uncertainty, or a fresh
reset that restores the promised initial state. No uncertain transaction is
retried and no commit/exit is invented retrospectively. ADR-0003 §3 references
this rule; ADR-0004 §16 includes future late-write/cancellation/reset verification.

This is a proposed semantic clarification for acceptance, not an implemented
recovery API. Force-reset/reconstruction exceptions remain outside the contract;
no new transport or FFI mechanism is selected.

### Earlier ordering deferrals: narrow the wording after ADR-0004

ADR-0001's deferral list and ADR-0003 §§2, 8 and its final deferral list now point
to ADR-0004 §§10–11 for canonical same-time fact/shared-effect order and the
semantic order preserved by observations. Scheduling/fairness, coherence,
observation transport/buffering and other implementation mechanisms remain
deferred. This reconciles the Proposed records without approving or implementing
multiple Harts, replay, or a scheduler in A0.

## Implementation evidence and its limits

The following source observations support the current-to-target mapping:

- [`RiscvCore::step`](../../src/core/mod.rs) returns `Result<()>`, reads one
  instruction word, calls the executor and updates PC. It does not invoke the
  trap-handler component. Its ELF-base subtraction is not a Hart-owned MMU.
- [`load_and_run`](../../src/executor.rs) constructs RAM/UART/SystemBus, resets
  the core with base zero, loops over steps, polls tohost and assembles results.
  It re-fetches the logging opcode using `pc_before.wrapping_sub(base_addr)`,
  snapshots GPRs, passes `mem_access = None`, and ignores `log_commit` errors.
  The nonzero-base logging mismatch is already recorded in the inventory; this
  assessment does not claim to have reproduced it with a new regression.
- [`RiscVSimulator::run`](../../src/executor.rs) is a separate flat-memory run
  loop around the same `RiscvCore`, not a separate ISA engine. Migration must
  preserve the facade without treating its current addressing/device behavior
  as identical to the CLI path.
- [`tests/executor.rs`](../../tests/executor.rs) discards results in
  `test_log_commit_path_with_logger` and `test_load_and_run_without_logger`.
  `test_load_and_run_zero_cycles` uses the tautology
  `result.is_ok() || result.is_err()`. These tests cannot prove the behaviors
  named in their titles.
- [`tests/commits_test.rs`](../../tests/commits_test.rs) asserts logger formatting
  using supplied register/memory facts. Those assertions do not prove that the
  public runner supplies correct facts. [`tests/trap_test.rs`](../../tests/trap_test.rs)
  similarly protects trap components, not trap entry through public ELF execution.

### Recorded local verification

On 2026-09-07, with Rust/Cargo 1.98.0 and unchanged simulator/test sources, this
command completed successfully:

```bash
cargo test --all-features \
  --test executor --test cli_test --test commits_test \
  --test test_elf_loader --test trap_test
```

| Test target | Passed | Failed / ignored |
| --- | ---: | --- |
| `executor` | 57 | 0 / 0 |
| `cli_test` | 15 | 0 / 0 |
| `commits_test` | 13 | 0 / 0 |
| `test_elf_loader` | 15 | 0 / 0 |
| `trap_test` | 36 | 0 / 0 |
| **Total** | **136** | **0 / 0** |

Passing tests with weak assertions remain weak evidence. No fresh full Rust
quality gate, guest assembly/ELF suite, Docker build, external compliance suite,
or differential run is recorded by this assessment. Docker inventory access
failed with permission denied on `/var/run/docker.sock`; the host
`riscv64-unknown-elf-gcc` executable was not found. Existing image evidence in the
active plan is retained, not replaced with a claimed local pass.

[Main CI](../../.github/workflows/ci.yml) runs its required quality job for
pull requests including documentation-only PRs. The full guest scripts run on
applicable pushes, and the [image workflow](../../.github/workflows/dev-container.yml)
is path-filtered to image/workflow changes or manual dispatch. A documentation PR
must not assume that it automatically produces a fresh verified container image.

### Documentation validation

The seven changed/new documentation files passed `git diff --check`, local
Markdown target/anchor checks (172 links), and primary language-server diagnostics
with no findings. Code fences and sequence-diagram control blocks were checked
for balance. All four ADRs remain Proposed; the active plan and principles are
unchanged. Mermaid graphical rendering was not run because no local renderer was
found; structural checks are not visual validation. No Rust tests were rerun for
the subsequent documentation-only clarifications; the focused results above
remain evidence for the unchanged source, not for runtime implementation of the
new contracts.

## Acceptance handoff

The bounded next decision is whether to adopt the proposed resolutions above and
the successor scope candidate. No source repair, ISA extension, ACT4 integration,
or runtime implementation is authorized by this assessment.

Formal independent review must follow the repository's
[review policy](../documentation-policy.md#change-and-review-provenance): review
the committed, pushed, ready PR head and its applicable verification evidence in
a separate context. This draft is only pre-review material. ADR acceptance,
A0 archival with limitations/evidence, and replacement by one approved successor
remain explicit decisions; none is implied by this document or by passing the
focused tests.
