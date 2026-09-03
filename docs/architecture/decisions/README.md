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

| Record | Decision | Status | Tracking |
| --- | --- | --- | --- |
| [0001 — Hart execution outcome and observation records](0001-hart-execution-outcome-and-observation.md) | Hart step outcomes, retirement/trap boundaries, and immutable observation ownership | Proposed | [MMQ-9](https://linear.app/mrtoniliu/issue/MMQ-9/adr-hart-execution-outcome-and-observation-records) |
| [0002 — Physical-access transaction and fault contract](0002-physical-access-transaction-and-fault.md) | One transport-neutral physical-access contract, raw bytes, fault taxonomy, and atomic operations | Proposed | [MMQ-7](https://linear.app/mrtoniliu/issue/MMQ-7/adr-physicalaccess-transaction-and-fault-contract) |
| [0003 — Runner, Machine, and Platform ownership](0003-runner-machine-and-platform-ownership.md) | Semantic ownership, lifecycle, event aggregation, and one-Hart ISS/VP composition | Proposed | [MMQ-6](https://linear.app/mrtoniliu/issue/MMQ-6/adr-runner-machine-and-platform-ownership) |

No architecture decision records have been accepted yet. Proposed records are working
contracts until their individual review and acceptance; the diagrams and principles
remain the working baseline in the meantime.
