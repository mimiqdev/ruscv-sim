# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A0 — ISS → Virtual Platform Architecture Baseline

**Status:** Active — architecture definition

**Authority:** Normative milestone contract

**Started:** 2026-08-26

**Last reviewed:** 2026-09-03

This is the only current milestone contract. Earlier milestone and sprint plans are historical records under [`archive/`](archive/). Historical plans do not direct current work; this document defines the current technical scope and acceptance criteria.

## Objective

Re-establish the product architecture from first principles: build a verifiable RISC-V ISS as the common execution engine, then evolve it into a composable Virtual Platform without creating a second architectural execution path.

## Scope

The milestone includes:

- Archiving superseded milestone, architecture, and SystemBus plans without claiming completion.
- Reorganizing documentation by authority while preserving historical sources.
- Establishing the target architecture, architecture principles, development image, and current-to-target implementation inventory.
- Defining and approving stable contracts between Hart, Runner, Machine, Platform, and physical access.
- Recording architecture decisions whose trade-offs materially constrain later implementation.
- Deriving the first implementation milestone from the approved architecture.

## Recorded baseline evidence

The following A0 scope items were already satisfied before this plan was converted to a contract-only presentation. These citations are a durable evidence register, not a second live task checklist:

- Superseded milestone and architecture/SystemBus plans were archived, the documentation hierarchy was reorganized, and historical source content was preserved in the [architecture reset commit `50d8c739f686b218a7e4fcc99619079d97833705`](https://github.com/mimiqdev/ruscv-sim/commit/50d8c739f686b218a7e4fcc99619079d97833705).
- The target-architecture entry point and its product-evolution, system-context, layering, Hart, memory/TLM, runtime, and capability-growth views are recorded in [`docs/architecture/README.md`](architecture/README.md) by that same [architecture reset commit](https://github.com/mimiqdev/ruscv-sim/commit/50d8c739f686b218a7e4fcc99619079d97833705).
- The repository-owned Rust/RISC-V guest/differential-testing development image was established and hardened in [`f7db92d`](https://github.com/mimiqdev/ruscv-sim/commit/f7db92d59b86b886d30919e25e863be614ffed2b) and [`6fc0976`](https://github.com/mimiqdev/ruscv-sim/commit/6fc097661bada37984376a4ba421d44e2b299ab7), with the quality-gate and guest commands recorded in [`docs/development-environment.md`](development-environment.md) and [`.github/workflows/dev-container.yml`](../.github/workflows/dev-container.yml).
- The already-satisfied acceptance criterion that the development image builds and runs the current project quality gate and guest ELF toolchain is supported by the image workflow and commands cited above. The remaining scope and acceptance criteria above remain current contract requirements.

## Architecture sequence

This sequence records the technical order for completing the baseline; it is not a task-status or dependency register.

1. Review the current implementation architecture in [`docs/architecture/current-state.md`](architecture/current-state.md).
2. Define Hart outcomes and observation records in [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md).
3. Define the physical-access contract in [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md).
4. Define Runner, Machine, and Platform ownership in [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md).
5. Define the interrupt, time, and stop-event boundaries.
6. Cross-check the baseline against the acceptance criteria and derive the successor implementation scope.

The architecture records and current implementation inventory are the durable references for this sequence.

## Non-goals

- Implementing new ISA extensions or repairing existing instruction behavior.
- Integrating ACT4 or selecting a compliance baseline.
- Implementing SystemC/TLM, DMI, block execution, or dynamic code translation.
- Claiming that independently tested MMU, TLM, debug, or peripheral components are integrated end to end.
- Selecting detailed delivery dates before the architecture boundaries are approved.

## Architectural constraints

1. The ISS and Virtual Platform must share one Hart implementation.
2. Hart owns architectural semantics; it must not depend on ELF, CLI, concrete peripherals, or SystemC/TLM.
3. Virtual-to-physical translation belongs to the Hart; physical address routing belongs to the Platform.
4. Architectural traps, platform stop events, debugger stops, and simulator faults must remain distinct.
5. TLM and future acceleration mechanisms are adapters or execution strategies, not alternate ISA semantics.
6. Target diagrams describe intended ownership and dependencies; they do not claim current implementation completeness.

## Deliverables

- A navigable documentation index.
- A reproducible repository-owned development and verification image.
- A reviewed multi-view architecture baseline.
- An inventory of current-to-target gaps.
- Approved boundary contracts and architecture decision records.
- A newly scoped implementation milestone based on those decisions.

## Acceptance criteria

- Every active architecture document has an explicit status and ownership boundary.
- The development image builds successfully and can run the current project quality gate and guest ELF toolchain.
- The target dependency direction is unambiguous from frontend to infrastructure.
- Hart, Runner, Machine, Platform, memory access, interrupts, time, and observation responsibilities are agreed.
- ISS and VP product forms can be explained as configurations around one architectural engine.
- The current code has been mapped to the target architecture without overstating integration.
- Open architecture decisions and deferred performance work are explicitly recorded.
- The next implementation milestone is approved and replaces A0 as the only current milestone contract.

Acceptance is established by recorded repository evidence.

## Closeout

When all acceptance criteria are satisfied, archive this plan with the review date, accepted decisions, known open questions, relevant repository evidence, and known limitations. Then replace it with exactly one approved implementation milestone contract.
