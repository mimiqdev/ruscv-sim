# Documentation Policy

**Status:** Current

**Authority:** Normative

**Last reviewed:** 2026-08-26

## Purpose

Documentation must preserve useful knowledge without allowing obsolete architecture, schedules, or implementation claims to direct current development.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| Current | Verified current behavior or accepted target boundary |
| Draft | Proposed contract awaiting review; not yet an accepted decision |
| Research | Options, constraints, and evidence; not scheduled work |
| Historical | Superseded or completed context stored only under `archive/` |

## Authority vocabulary

- **Normative:** constrains current planning or implementation.
- **Informational:** explains, records, or researches without creating a requirement.

Every non-archived technical document must state both status and authority. A target-architecture document must explicitly say that it does not claim implementation completeness.

## Rules

1. `dev-plan.md` contains the only active milestone and active work checklist.
2. Accepted cross-cutting architecture decisions live in architecture decision records.
3. Research documents do not contain active schedules, completion claims, or implied backlog commitments.
4. Current behavior claims cite a source path, test, command, or recorded verification date.
5. Component implementation, public-path integration, and external compliance are described separately.
6. Historical content may be cited only as historical input, never as present authority.
7. When replacing a document, archive the full original and extract still-valid knowledge into a new current document.
8. External tools and suites are named with an exact project and pinned revision before their results become evidence.
9. Language choice follows subsystem boundaries; C++ content is not obsolete merely because the simulator core is Rust.

## Archive policy

Archived documents retain their original detail, including obsolete checklists and pseudocode, but receive a header explaining why they are no longer current. Broken internal links caused by reorganization should be repaired when their target is preserved; truly missing historical attachments are labeled as unavailable.
