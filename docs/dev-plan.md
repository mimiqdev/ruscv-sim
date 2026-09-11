# Development Plan

**Project:** `ruscv-sim`

**Prospective milestone:** A5 — ACT4 RV64I External Compatibility Baseline

**Status:** Draft — direction approved; detailed contract pending maintainer approval through the A4 closeout PR merge

**Authority:** Proposed normative milestone contract; not yet implementation authorization

**Prepared:** 2026-09-11

This is the only current prospective milestone contract. The
[A4 closeout](archive/milestones/a4-closeout-record.md) records completed
implementation and verification; formal acceptance and this successor contract
are proposed together, not already merged. A5 continues the A0–A4 milestone
sequence, not a release number or a revival of the
[superseded M8 plan](archive/milestones/m8-act4-rv64i-superseded.md).

## Objective

Generate and execute an explicit, reproducible ACT4 RV64I selection with
Sail-derived expected values through **`ruscv-sim run`**, and repair bounded
base-integer defects it exposes with public-path regressions. Deliver external
compatibility evidence for that entire selection, not another internal extraction
or a claim of RISC-V certification.

[A4's assessment](archive/milestones/a4-capability-assessment.md#one-successor-proposal-act4-rv64i-external-compatibility-validation)
recommended this pivot: shared host control and installation now have integrated
evidence, but the public ISA path has no selected external baseline.
[`Opcode::MiscMem`](../src/decode/mod.rs#L248) is still rejected, including
base-I FENCE. Component presence and project-authored guest passes do not establish
extension-wide support.

## Scope and selection contract

Use `riscv/riscv-arch-test` **ACT4 4.0.0**, pinned at
[`a7c99303516f4e668f7488f172043392e23b9dfd`](https://github.com/riscv/riscv-arch-test/tree/a7c99303516f4e668f7488f172043392e23b9dfd),
with self-checking ELFs containing Sail-derived expected values, on the current
single-Hart native RAM/UART/HTIF configuration. No test-only instruction engine,
direct ISA-function harness, legacy RISCOF plugin or legacy signature-dump
comparison may substitute for public CLI execution.

The selection is **all generated cases in the pinned upstream RV64I inventory
that meet the declared unprivileged, non-trapping profile**, not a hand-picked
green subset. Inventory source cases before running the DUT, and map every case
to inclusion or a specific architectural/environment exclusion. Include integer
ALU, RV64 word operations, branches/jumps, loads/stores and base-I FENCE wherever
the pinned suite provides cases. Identify absent upstream coverage explicitly;
add a focused public FENCE regression even if upstream offers no suitable case.
Do not invent an approximate test count.

Allowed boundary exclusions are tests whose purpose requires architectural trap
entry (including ECALL/EBREAK trap behavior and misaligned/access-fault traps),
privilege/CSR/MMU/PMP/interrupt behavior, other ISA extensions or unavailable
concurrent/device ordering. List their exact source identifiers, reason and
coverage consequence in a machine-readable exclusion manifest. Incidental startup,
linker or pass/fail macro requirements must first be adapted to the declared
profile; they are not a license to exclude the base instruction under test.
FENCE.I is **Zifencei**, not base I.

Freeze the source selection, profile parameters and exclusions at the feasibility
gate in a reviewed repository record before claiming full-selection success.
Required base-I failures, missing generation outputs and unsupported execution
must remain failures/blockers, never be silently removed to make counts green.
Any later denominator or profile change requires an explicit maintainer decision,
recorded rationale and regenerated evidence. If the full profile cannot be
generated without out-of-scope architecture, stop for that decision.

## Early feasibility gate

The pinned [upstream README](https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/README.md)
names Sail **0.10**, GCC 15/Binutils 2.44 or LLVM/Clang 21, Make/Python and
UDB configuration. Neither Sail 0.13.1 nor any exact working pairing has been
verified here. The existing [development image](development-environment.md)
and guest CI toolchain do not establish an ACT4/Sail environment.

First prove one compatible ACT4/Sail/compiler pairing from a clean documented
workspace or container, including submodule revisions and required UDB/Ruby/
Bundler/Python dependencies. Commit a concise immutable tool/dependency manifest
and setup command; record actual compiler/model versions and container digest
if used. Prefer upstream generation machinery and one thin execution harness,
not a new orchestration framework.

Generate and run a named small smoke selection with Sail expected values, inspect
startup and linked code for undeclared ISA requirements, and prove both guest
pass and guest-failure signaling through the public CLI. Record unsuccessful
setup attempts and blockers. The smoke result proves feasibility only: **it
cannot close A5 or be labeled the complete RV64I selection**. If tool compatibility
or generation is unavailable, report that blocker before implementing speculative
infrastructure or expanding simulator architecture.

## Required observable behavior

- Checked-in DUT configuration, linker script and pass/fail macros map to the
  existing native exit path. A documented command builds Sail-checked ELFs from
  the pinned sources; a separate or combined command runs them through the
  release CLI. Preserve the oracle; do not patch expected results to fit the DUT.
- Emit an enumerated generated-ELF manifest with source/variant identity, profile,
  hashes and tool revisions. Derive source, generated-variant and executed-ELF
  counts separately from the exact build. Detect missing/duplicate/extra outputs,
  empty selections and generation failures; none may count as a pass.
- Invoke the public CLI for every selected ELF with explicit cycle and host
  wall-time limits. Retain invocation, stdout/stderr, simulator revision, ELF
  hash and per-test machine-readable outcome. Distinguish guest pass, guest fail,
  cycle/wall timeout, unsupported execution, and simulator/harness failure.
  Ambiguous output is a failure, not success inferred from process status alone.
- Fail-closed negative controls cover a known failing guest/self-check,
  nontermination, an invalid/unsupported instruction and a missing or malformed
  ELF; also prove missing results and a generation failure make the overall job
  fail. Run controls through the same harness, not an independent success path.
- Reproduce each in-scope ISA defect before repair and add focused Rust and/or
  public ELF regressions. Minimal decoder/dispatcher/ISA fixes for required
  base-I behavior are in scope. FENCE must be correct for synchronous,
  single-Hart ordered memory without inventing a concurrent platform model;
  FENCE.I support is not implied. Keep PC, `x0`, retirement and memory effects
  explicit. Out-of-scope discoveries require a decision rather than scope drift.

## Architecture and compatibility constraints

The [accepted ADRs](architecture/decisions/README.md),
[principles](architecture/principles.md) and
[external-test boundary](verification/external-riscv-tests.md) remain authoritative.
One Rust Hart owns ISA semantics. Test setup and result classification remain
host orchestration; storage placement is not guest translation.

Use existing public APIs, CLI options, exit encoding and native configuration.
Preserve A1–A4 compatibility tests without deletion or weakening. A deliberately
repaired base-I rejection may change to successful execution only with a
reproducing regression and explicit documentation; an unrelated public behavior
change requires separate approval. This milestone does not implement or claim
the ADR target Runner/Machine/Platform or precise Hart outcome boundaries.

## Non-goals and re-evaluated gaps

No trap/privilege/interrupt/MMU/PMP integration, multi-Hart, SystemC/TLM,
platform migration, merged run loops, devices in the flat facade, new non-I ISA
extensions, acceleration, performance target, OS boot, RV64G certification or
whole-ISA compliance claim. No legacy RISCOF adapter or general-purpose suite
framework. No automatic revival of the old M8 checklist.

[Retained gaps](verification/public-behavior-gaps.md) G-03/G-04/G-05/CLI G-06/
G-08/G-09/G-11 and inherited default-limit/symbol-fallback/inspection limits
are not scheduled here. G-07 informs reproducible external tooling, not a promise
to install tools on every host. A discovery blocking a required case is reported
for explicit disposition; these exclusions cannot be used to discard required
base-integer failures.

## Deliverables and acceptance criteria

1. **Feasible pinned generation.** A clean environment reproduces the documented
   ACT4/Sail generation and smoke workflow with immutable pins and truthful
   startup/profile evidence. The reviewed selection inventory and exclusions
   cover the entire pinned RV64I source inventory; exact counts replace guesses.
2. **Fail-closed public execution.** The bounded harness executes every selected
   ELF through `ruscv-sim run`, produces complete machine-readable results and
   retained diagnostics, and passes all negative-control assertions above.
3. **Full selected compatibility.** Every ELF required by the frozen selection
   is generated and passes; required generated = executed = passed, with zero
   unexpected skips/missing cases/failures. Report exclusions separately. Smoke
   success alone is insufficient. Every repaired defect has a local regression
   and the FENCE gap has an explicit verified disposition.
4. **Reproducible CI evidence.** CI actually provisions the pinned environment,
   generates with Sail and executes the full selection and negative controls.
   Retain tool/configuration/selection manifests, generated-ELF hashes, generation
   logs, per-test outcomes, totals and failure-reproduction artifacts. Document
   artifact retrieval and retention. A cache is allowed only with validated pins
   and provenance plus a recorded clean generation run; unexplained cached ELFs
   or Cargo-only success cannot satisfy this criterion.
5. **Capability acceptance.** Record independent review of the exact committed
   implementation head, the full six-command Rust gate, public CLI regressions,
   current separately compiled project-authored guest results and successful
   full-selection ACT4 CI at the reviewed head or verified merge. Update the
   matrix, gap register, current-state and external-test instructions with exact
   revisions/counts/limitations. The final assessment says *selected external
   compatibility*, not certification, and proposes at most one successor.

The full gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, and
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
Use the [guest commands](verification/bare-metal-tests.md); record unavailable
tools and early-return Cargo tests honestly.

## Delivery and closeout

Reviewable delivery order: (1) feasible pinned generation, truthful inventory
and smoke gate; (2) full-selection CLI harness, negative controls and bounded
repairs; (3) clean CI generation/execution and final capability assessment.
These are tasks under one milestone, not three acceptance milestones.

Formal work requires approval of this contract. After every criterion has
recorded evidence and required review/merge, archive completion, limitations and
revision identifiers, re-evaluate unfinished work and select one separately
approved successor. Task completion cannot substitute for capability acceptance.
