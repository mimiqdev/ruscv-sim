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

| Record | Decision | Status |
| --- | --- | --- |
| [0001 — Hart execution outcome and observation records](0001-hart-execution-outcome-and-observation.md) | Hart step outcomes, always-present control facts, and optional subscriber-gated observation | Proposed |
| [0002 — Physical-access transaction and fault contract](0002-physical-access-transaction-and-fault.md) | Hart-initiator physical-access contract, raw bytes, fault taxonomy, atomics, and deferred inbound masters | Proposed |
| [0003 — Runner, Machine, and Platform ownership](0003-runner-machine-and-platform-ownership.md) | Runner taxonomy, one-or-more-Hart Machine composition, dual hosting, and unclassified control facts | Proposed |
| [0004 — Interrupt, time, scheduling, and stop-event boundaries](0004-interrupt-time-scheduling-and-stop-boundaries.md) | Platform-input admission, Hart/profile-provided sampling slots, modeled time, Machine exchanges, WFI/idle scheduling, deterministic event order, and non-lossy stop facts | Proposed |

No architecture decision records have been accepted yet. Proposed records are working
contracts; the diagrams and principles remain the working baseline in the meantime.
