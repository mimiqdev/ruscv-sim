# Architecture Decision Records

**Status:** Current index

**Authority:** Normative after an individual record is accepted

Architecture decisions belong here when they constrain more than one subsystem or would be expensive to reverse. Research notes and implementation sketches are not decisions.

## Lifecycle

```text
Proposed → Accepted → Superseded
                ↘ Rejected
```

Each record must state:

- Context and the concrete problem.
- Decision and ownership boundaries.
- Alternatives considered.
- Consequences and compatibility impact.
- Verification needed to prove the decision works.
- Status and superseding record, when applicable.

## Records

| Record | Decision | Status | Accepted |
| --- | --- | --- | --- |
| [0001 — Hart execution outcome and observation records](0001-hart-execution-outcome-and-observation.md) | Hart step outcomes, always-present control facts, and optional subscriber-gated observation | Accepted | 2026-09-07 |
| [0002 — Physical-access transaction and fault contract](0002-physical-access-transaction-and-fault.md) | Hart-initiator physical-access contract, raw bytes, fault taxonomy, atomics, and deferred inbound masters | Accepted | 2026-09-07 |
| [0003 — Runner, Machine, and Platform ownership](0003-runner-machine-and-platform-ownership.md) | Runner taxonomy, one-or-more-Hart Machine composition, dual hosting, and unclassified control facts | Accepted | 2026-09-07 |
| [0004 — Interrupt, time, scheduling, and stop-event boundaries](0004-interrupt-time-scheduling-and-stop-boundaries.md) | Platform-input admission, Hart/profile-provided sampling slots, modeled time, Machine exchanges, WFI/idle scheduling, deterministic event order, and non-lossy stop facts | Accepted | 2026-09-07 |

ADR-0001 through ADR-0004 were accepted by the maintainer on 2026-09-07,
including ADR-0003's loader-metadata / Machine-installation / Platform-write
split. The [principles](../principles.md) and [target views](../README.md) reflect
these normative semantic contracts. Acceptance does not claim implementation
completeness or select concrete Rust APIs. ADR acceptance alone does not approve
implementation scope or close a milestone. The separately approved A0-to-A1
handoff is recorded in the [A0 closeout record](../../archive/milestones/a0-closeout-record.md);
[A1's subsequent closeout](../../archive/milestones/a1-closeout-record.md) and
[the A2 closeout](../../archive/milestones/a2-closeout-record.md) record their
limitations and successor decisions. [A3](../../dev-plan.md) is the only active
milestone contract.
