# Archive context

**Status:** Historical

**Authority:** Informational; the original contract below is no longer active

**Archived:** 2026-09-07

A0 is handed over to the approved [A1 contract](../../dev-plan.md). See the
[A0 closeout record](a0-closeout-record.md) for criterion-level evidence,
completion date and limitations. The original text below is preserved, with
relative links relocated; its Active, Draft and pending-approval statements
describe the pre-handoff state, not current authority.

The full migration candidate was not approved. A1 selects only the public
behavior baseline; no remaining increment is automatically scheduled.

---

# First Implementation Scope Candidate

**Status:** Draft — successor-scope proposal awaiting approval

**Authority:** Informational; not an active milestone contract

**Scope:** A bounded single-Hart native execution migration derived from the accepted A0 boundary contracts

This candidate supports the [A0 closeout assessment](a0-pre-closeout-assessment.md).
It is not a delivery commitment, a selected ISA/compliance profile, or a claim
that the target boundaries are implemented. [A0](a0-architecture-baseline.md) remains the only
current milestone. ADR-0001 through ADR-0004 were accepted on 2026-09-07;
that acceptance does not approve this candidate. The choices below still require
explicit resolution and approval before this can become a successor contract.

## Proposed objective

Run the existing public ELF product through one native
`Runner → Machine → {Hart, Platform}` composition, retaining one architectural
engine and public compatibility facades. Establish trustworthy execution facts
and observations for an explicitly bounded profile before integrating richer
VP components or external architectural verification.

The milestone is not merely a module rename: a migrated success, synchronous
trap, target access fault, and simulator failure must have distinct, tested
boundary behavior. Nor is it a promise to repair the entire current instruction
set while calling the work a refactor.

## Choices required before approval

| Choice | Recommended boundary | Why it must be resolved |
| --- | --- | --- |
| Execution profile | Select a named single-Hart RV64, native, fixed-width-fetch/identity-addressing configuration and an exact instruction/CSR/trap capability matrix. Preserve existing public instruction paths unless a separate change is approved. Do not claim extension-wide compliance. | Neither component presence nor a decoder arm selects a correct profile. Legality versus unsupported-legal-operation classification requires an explicit specification/profile reference. Full CSR/FPU/atomic coverage may make the complete migration too large. |
| Precision of Hart effects | Include precise outcomes and optional Hart-originated observations for every path claimed as migrated. Choose staging/journaling or equivalent during implementation design, with fault-injection evidence. | A wrapper around `Result<()>` cannot establish no-partial-state guarantees. If the covered surface is too large, approve a smaller preparatory milestone with explicit residual gaps rather than advertising partial behavior as ADR compliance. |
| Legacy run limit | Preserve `--max-cycles` and its default/zero/successful-step behavior through a named compatibility policy; keep started-turn budget, retirements, ISA counters and virtual time separate. | ADR-0003 §9 preserves the legacy surface; ADR-0004 does not redefine it as a hardware-cycle count. Document how completed synchronous trap entry is reported without counting it as retirement. |
| Public Rust surfaces | Keep existing exported names/signatures through adapters unless separately approved; make `RiscVSimulator` a facade over common execution control with its flat configuration. | The library's addressing/device behavior differs from the CLI. A shared engine does not justify silently switching its platform or removing state/memory helpers. |
| Native platform and time | Retain current verified RAM/UART/HTIF behavior; select explicit native zero-delay and `iss_tick` units. Implement only the supported synchronous native exchange subset and reject unsupported configurations. | No need for SystemC, CLINT/PLIC, replay or multi-Hart machinery, but counters, budgets and time cannot be aliases. This subset is not a claim of full ADR-0004 implementation. |
| Known observable defects | Decide which migration-relevant defects are explicitly included, each with reproduction and a regression before repair. Prioritize authoritative opcode/effect logging and observer-failure reporting. | Correct tracing is part of the objective, but unrelated instruction fixes and undocumented UART-map changes must not enter as cleanup. Preserve intended compatibility, not known incorrect log data. |

If these choices cannot be made narrowly, the fallback is a separately approved
compatibility-and-composition milestone, explicitly deferring precise Hart
outcomes. Do not silently expand or weaken the objective above during coding.

## Proposed implementation increments

This is a technical order for scoping, not a schedule or task-status register.
Each increment must leave runnable public behavior and reviewable evidence.

### 1. Establish public-path characterization

Strengthen assertions in existing executor/CLI tests and add minimal executable
ELF fixtures. Cover entry and nonzero load base, RAM effects, success and nonzero
exit, exact/zero limits, tohost precedence, UART bytes, signature data, and actual
commit-log contents. Exercise the flat facade separately rather than asserting
that it already behaves like the native bus.

Reproduce each selected defect before repair. Do not bless an opcode-zero log or
a swallowed observer error as the expected target behavior. Existing logger and
trap component tests remain component evidence.

### 2. Introduce the native physical/composition boundary

Move physical routing and target behavior behind the approved physical contract;
connect the same Hart via Machine. Separate load-image metadata from physical
installation and host inspection. Do not move RISC-V sign extension, alignment,
translation or traps into Platform. Keep compatibility storage offsets below the
architectural engine.

For all advertised access categories, test widths/raw bytes, failure-before-
mutation and target-fault versus host-failure normalization. AMO/LR/SC may not be
emulated as separately visible ordinary transactions; preserve their Hart-owned
semantics and physical indivisibility if included in the approved profile.

### 3. Establish precise Hart outcomes and observation

Make the Hart the source of instruction identity, PC transition, retirement,
trap-entry effects and architectural effects for the supported profile. Classify
illegal profile encodings separately from legal but unimplemented operations.
Inject faults to prove no partial Hart state escapes and no faulting instruction
retires. Check trap CSRs, saved PC, handler target, privilege and `x0` explicitly.

Materialize records only when subscribed. Preserve final state and control facts
with observations disabled, without per-instruction record allocation or a
required per-instruction Runner callback. A sink failure after retirement must
not undo the commit or fabricate a Hart trap.

### 4. Consolidate Runner policy and compatibility facades

Move limits, observation demand/delivery and result assembly into Runner. Machine
returns complete facts and owns coherent reset/inspection/drain boundaries;
Platform owns exit provenance. Both ELF and flat-library facades use this shared
control implementation, with no second ISA engine or duplicate run policy.

Keep retire-before-exit ordering, distinguish legacy limits from framework turn
budgets, and retain coincident facts before selecting a primary reason. Exercise
fresh rerun/reset of image-owned contents and device state under the supported
native lifecycle. Unknown-completion handling must follow the accepted recovery
clarification, not reset while a transaction can still mutate state.

## Proposed acceptance evidence

An approved successor should translate these into exact tests and deliverables:

- Existing named public APIs and CLI options remain usable; chosen ELF fixtures
  and the project-authored guest suite run through the new composition.
- One shared Hart semantics and shared Runner policy serve native and flat
  configurations; source dependency checks show no ELF/device/CLI dependency in
  the architectural engine and no result-presentation dependency below Runner.
- Profile-scoped success, synchronous trap, physical target fault, and host
  failure each have asserted state/outcome/retirement behavior, including
  no-partial-effect fault tests and preserved physical transaction atomicity.
- Enabled logs contain the Hart-observed opcode and supported architectural
  effects without external refetch/snapshots; disabled observation preserves
  identical architectural state/control facts without materialized records.
- Zero and exact limits, successful exit on the final slot, and post-retirement
  observer failure retain the correct facts and deterministic result policy.
- Installation, signature inspection and reset use composed ownership without
  guest instruction execution or counting host inspection as guest progress.
- The full repository quality gate and project guest scripts have recorded
  results. Missing dependencies are reported as unavailable, not passed.
- User-facing documentation and the current-state inventory name the integrated
  profile and residual gaps; focused tests are not relabeled as ACT4 evidence.

## Explicit exclusions

No new ISA extensions, full ISA repair campaign, compressed-fetch integration,
MMU/PMP integration, timer/controller/WFI platform integration, production GDB,
RISC-V Debug Mode, multi-Hart execution, DMA/coherence, replay, SystemC/TLM/HDL
adapter, DMI, block execution, translation/JIT, Linux boot, ACT4 integration or
external compliance baseline selection is proposed here. Existing components
remain regression-protected without becoming public-path support claims.

These exclusions defer capabilities, not the ownership/precision obligations of
any capability that is included. Subsequent capability work requires its own
approved scope; this candidate does not schedule it or carry historical backlog
forward automatically.
