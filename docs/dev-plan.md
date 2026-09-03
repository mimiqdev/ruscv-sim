# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A0 — ISS → Virtual Platform Architecture Baseline

**Status:** Active — architecture definition

**Authority:** Normative milestone contract; Linear is authoritative for execution status

**Started:** 2026-08-26

**Linear:** [`ruscv-sim` project](https://linear.app/mrtoniliu/project/ruscv-sim-7555af313020) · [MRT-5 milestone issue](https://linear.app/mrtoniliu/issue/MRT-5/a0-iss-virtual-platform-architecture-baseline)

This is the only current milestone contract. Earlier milestone and sprint plans are historical records under [`archive/`](archive/). Active task status, priority, ownership, and dependencies are maintained in Linear rather than duplicated here.

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

## Execution tracking

- [MRT-11](https://linear.app/mrtoniliu/issue/MRT-11/review-current-implementation-architecture-audit) — review the current implementation architecture audit.
- [MRT-9](https://linear.app/mrtoniliu/issue/MRT-9/adr-hart-execution-outcome-and-observation-records) — define Hart outcomes and observation records.
- [MRT-7](https://linear.app/mrtoniliu/issue/MRT-7/adr-physicalaccess-transaction-and-fault-contract) — define the physical-access contract.
- [MRT-6](https://linear.app/mrtoniliu/issue/MRT-6/adr-runner-machine-and-platform-ownership) — define Runner, Machine, and Platform ownership.
- [MRT-10](https://linear.app/mrtoniliu/issue/MRT-10/adr-interrupt-time-and-stop-event-boundaries) — define interrupt, time, and stop-event boundaries.
- [MRT-12](https://linear.app/mrtoniliu/issue/MRT-12/run-a0-architecture-acceptance-review) — perform the A0 acceptance review.
- [MRT-8](https://linear.app/mrtoniliu/issue/MRT-8/approve-successor-implementation-milestone-and-close-a0) — approve the successor milestone and close A0.

These links provide navigation only; Linear holds their live status and dependency graph.

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

Acceptance is established by recorded repository evidence and the A0 review; live completion state is tracked in Linear.

## Closeout

When all acceptance criteria are satisfied, archive this plan with the review date, accepted decisions, known open questions, relevant commit identifier, and Linear milestone issue. Then replace it with exactly one approved implementation milestone contract and link the successor Linear tracking issue.
