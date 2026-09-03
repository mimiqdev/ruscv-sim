# Documentation Policy

**Status:** Current

**Authority:** Normative

**Last reviewed:** 2026-08-26

## Purpose

Documentation must preserve useful knowledge without allowing obsolete architecture, schedules, or implementation claims to direct current development. Linear manages active execution; repository documents preserve milestone boundaries, accepted decisions, and durable evidence.

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

1. The [`ruscv-sim` Linear project](https://linear.app/mrtoniliu/project/ruscv-sim-7555af313020) is authoritative for active work items, status, priority, ownership, and dependencies.
2. `dev-plan.md` contains exactly one current milestone contract and links its Linear tracking issue; it does not duplicate the live task checklist.
3. Accepted cross-cutting architecture decisions live in architecture decision records. Linear issue text cannot accept or override an architecture decision.
4. A Linear item is marked complete only after its required repository artifact and verification evidence exist.
5. Research documents do not contain active schedules, completion claims, or implied backlog commitments.
6. Current behavior claims cite a source path, test, command, or recorded verification date.
7. Component implementation, public-path integration, and external compliance are described separately.
8. Historical content may be cited only as historical input, never as present authority.
9. When replacing a document, archive the full original and extract still-valid knowledge into a new current document.
10. External tools and suites are named with an exact project and pinned revision before their results become evidence.
11. Language choice follows subsystem boundaries; C++ content is not obsolete merely because the simulator core is Rust.

## Change and review workflow

- Active work is performed on an issue-linked branch or isolated worktree, not directly on `main`.
- A formal review begins only after the intended change is committed, pushed, and represented by a ready pull request.
- The review target is the PR head commit together with applicable CI or recorded verification evidence; local inspection before that point is pre-review only.
- Review findings are fixed on the same issue branch and re-reviewed against the updated PR head.
- Linear may move to `Done` only after merge and after durable repository evidence exists.

## Archive policy

Archived documents retain their original detail, including obsolete checklists and pseudocode, but receive a header explaining why they are no longer current. Broken internal links caused by reorganization should be repaired when their target is preserved; truly missing historical attachments are labeled as unavailable.
