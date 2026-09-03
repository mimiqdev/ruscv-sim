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

No architecture decision records have been accepted yet. The diagrams and principles define the working baseline; proposed records capture unresolved boundaries for review, and the A0 review evaluates and accepts those records rather than inventing them.
