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
| [0001 — Hart execution outcome and observation records](0001-hart-execution-outcome-and-observation.md) | Hart step outcomes, retirement/trap boundaries, and immutable observation ownership | Proposed |
| [0002 — Physical-access transaction and fault contract](0002-physical-access-transaction-and-fault.md) | One transport-neutral physical-access contract, raw bytes, fault taxonomy, and atomic operations | Proposed |
| [0003 — Runner, Machine, and Platform ownership](0003-runner-machine-and-platform-ownership.md) | Semantic ownership, lifecycle, event aggregation, and one-Hart ISS/VP composition | Proposed |

No architecture decision records have been accepted yet. Proposed records are working
contracts; the diagrams and principles remain the working baseline in the meantime.
