# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A0 — ISS → Virtual Platform Architecture Baseline

**Status:** Active — architecture definition

**Authority:** Normative; the only active milestone

**Started:** 2026-08-26

This is the only active milestone. Earlier milestone and sprint plans are historical records under [`archive/`](archive/).

## Objective

Re-establish the product architecture from first principles: build a verifiable RISC-V ISS as the common execution engine, then evolve it into a composable Virtual Platform without creating a second architectural execution path.

## Scope

- [x] Archive the superseded M8 ACT4 plan without claiming completion.
- [x] Archive obsolete architecture and SystemBus expansion plans.
- [x] Reorganize documentation by authority and content rather than legacy filename.
- [x] Preserve complete historical sources while extracting still-valid current guidance.
- [x] Establish [`architecture/README.md`](architecture/README.md) as the target-architecture entry point.
- [x] Describe the product from product-evolution, system-context, layering, Hart, memory/TLM, runtime, and capability-growth perspectives.
- [ ] Audit the current source tree against the target boundaries.
- [ ] Define and approve the stable contracts between Hart, Runner, Machine, Platform, and physical access.
- [ ] Record architecture decisions whose trade-offs materially constrain later implementation.
- [ ] Derive the first implementation milestone from the approved architecture.

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
- A reviewed multi-view architecture baseline.
- An inventory of current-to-target gaps.
- Approved boundary contracts and architecture decision records.
- A newly scoped implementation milestone based on those decisions.

## Acceptance criteria

- [ ] Every active architecture document has an explicit status and ownership boundary.
- [ ] The target dependency direction is unambiguous from frontend to infrastructure.
- [ ] Hart, Runner, Machine, Platform, memory access, interrupts, time, and observation responsibilities are agreed.
- [ ] ISS and VP product forms can be explained as configurations around one architectural engine.
- [ ] The current code has been mapped to the target architecture without overstating integration.
- [ ] Open architecture decisions and deferred performance work are explicitly recorded.
- [ ] The next implementation milestone is approved and replaces A0 as the only active milestone.

## Closeout

When all acceptance criteria are satisfied, archive this plan with the review date, accepted decisions, known open questions, and relevant commit identifier. Then replace it with exactly one approved implementation milestone.
