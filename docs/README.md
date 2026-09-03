# Documentation

**Status:** Current index

**Authority:** Normative navigation

This index is the entry point for project decisions and technical documentation. Files under `archive/` preserve history but must not drive current implementation.

## Sources of truth

1. [`ruscv-sim` Linear project](https://linear.app/mrtoniliu/project/ruscv-sim-7555af313020) — active work items, status, priority, ownership, and dependencies.
2. [Current milestone contract](dev-plan.md) — objective, scope boundaries, non-goals, constraints, deliverables, and acceptance criteria.
3. [Target architecture](architecture/README.md) — intended product boundaries and ISS → VP evolution.
4. [Architecture principles](architecture/principles.md) — normative ownership, language, address, and error boundaries.
5. [Development environment](development-environment.md) — normative container toolchain and usage.
6. [Documentation policy](documentation-policy.md) — status, authority, and archival rules.
7. Source code and verified tests — authority for what is implemented today.

Target architecture is not implementation status. Component presence is not end-to-end support.

## Architecture

- [Architecture diagrams](architecture/README.md)
- [Architecture principles](architecture/principles.md)
- [Architecture decision records](architecture/decisions/README.md)

## Verification

- [Verification architecture](verification/README.md)
- [Project-authored bare-metal tests](verification/bare-metal-tests.md)
- [External RISC-V test integration contract](verification/external-riscv-tests.md)
- [Commit tracing and differential testing](verification/commit-tracing.md)

## Integration

- [Integration boundaries](integration/README.md)
- [SystemC/TLM boundary](integration/systemc-tlm.md)

## Reference

- [Reference index and authority rules](reference/README.md)
- [Code-generation component status](reference/code-generation.md)

## Research

- [Research index](research/README.md)
- [Execution performance directions](research/performance.md)
- [Heterogeneous platform directions](research/heterogeneous-platforms.md)
- [Linux boot requirements](research/linux-boot-requirements.md)

Research records options and constraints. It does not add work to the active milestone.

## Archive

- [Archive policy and categories](archive/README.md)
- [Milestone records](archive/milestones/README.md)

Archived files retain useful context, pseudocode, measurements, and rejected or superseded approaches. Any idea returning from the archive must be revalidated and accepted through the current architecture process.
