# AGENTS.md — ruscv-sim Repository Guide

## Project management and authority

- Read `docs/dev-plan.md` and the relevant issue in the [`ruscv-sim` Linear project](https://linear.app/mrtoniliu/project/ruscv-sim-7555af313020) before planning or implementing project work.
- Linear is the single source of truth for active work items, status, priority, ownership, and dependencies.
- `docs/dev-plan.md` contains exactly one current milestone contract: its objective, scope boundaries, non-goals, architectural constraints, deliverables, and acceptance criteria. It does not duplicate live task status.
- Accepted architecture decisions live in repository ADRs. Linear descriptions and status changes do not override the milestone contract, accepted ADRs, source code, or verified test evidence.
- If a Linear issue conflicts with `docs/dev-plan.md` or an accepted ADR, stop and surface the conflict instead of silently changing scope.
- Treat `docs/archive/` as historical context only. Archived checkboxes, old version targets, and superseded frameworks are not current tasks.
- Do not silently add unrelated cleanup, old backlog items, or speculative future features to the active milestone.

## Branch and review workflow

- Keep `main` clean. Perform project work on an issue-linked branch or isolated worktree.
- Move a Linear issue to `In Progress` when implementation begins. A local inspection or dirty working tree is not formal review.
- Commit and verify the intended change, then push it and open a pull request. Move the issue to `In Review` only when the PR is ready for review.
- Formal review targets the committed PR head and its CI evidence. Coding and review should use separate Agents or contexts; the reviewer must not modify the coding worktree.
- Address findings on the same issue branch and repeat review against the new PR head.
- Mark an issue `Done` only after the PR is merged and the required repository evidence exists.
- For a solo-maintainer repository, required CI and resolved review findings are sufficient; do not require an impossible self-approval.

## Rolling milestone workflow

When the active milestone is complete:

1. Verify every acceptance criterion in `docs/dev-plan.md` with recorded evidence.
2. Update the Linear milestone issue and its children only after the corresponding repository evidence exists.
3. Move the completed plan to `docs/archive/milestones/` as a standalone record.
4. Record its completion date, test evidence, known limitations, and relevant commit or release identifiers.
5. Re-evaluate unfinished Linear issues; do not automatically carry them forward.
6. Replace `docs/dev-plan.md` with the next approved milestone contract and link its Linear tracking issue.

Do not delete or rewrite historical milestone records merely to match the current architecture.

## Repository facts

- Primary branch: `main`
- Language: Rust, edition 2021
- Crate version: read from `Cargo.toml`; do not infer releases from milestone numbers
- Current product priority and task state: read from the `ruscv-sim` Linear project; use `docs/dev-plan.md` for the current milestone contract
- The repository contains independently tested components that may not be wired into the public ELF execution path. Distinguish component implementation, end-to-end integration, and ACT4 verification in code and documentation.

## Code map

- `src/main.rs`: public CLI
- `src/executor.rs`: ELF load/run loop, system bus, UART, HTIF/tohost, and execution results
- `src/core/`: architectural state and fetch/decode/execute loop
- `src/decode/`: 32-bit instruction decoder
- `src/execute/`: active execution dispatcher and ISA re-exports
- `src/isa/`: RV64 instruction implementations
- `src/elf.rs`: ELF64 parsing and segment/symbol discovery
- `src/memory/`: flat memory interface and implementation
- `src/mmu/`: MMU, Sv39, and TLB components
- `src/tlm/` and `src/peripherals/`: TLM-style bus and peripheral components
- `src/debug/`: GDB RSP, breakpoint, watchpoint, and debugger components
- `tests/`: Rust integration tests and project-authored bare-metal programs
- `docs/archive/`: completed or superseded plans; never the active plan

## Implementation practices

- Read the relevant implementation and focused tests before changing behavior.
- For bug fixes, reproduce the failure and add a focused regression test.
- Keep changes scoped to the active milestone and the user request.
- Preserve public APIs unless the task explicitly requires a breaking change.
- Keep `x0`, PC updates, address translation, privilege state, and memory side effects explicit when modifying instruction execution.
- Do not claim an ISA extension is supported end to end merely because its component tests pass.
- Update user-facing documentation when commands, configuration, or observable behavior changes.
- Do not commit, push, create tags, or publish releases unless explicitly requested.

## Verification

Use checks proportional to the change. The full local quality gate is:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

Useful focused commands:

```bash
cargo test <test_name>
cargo test --test <integration_test_name>
cargo run -- --help
cargo run -- run <elf-file> --max-cycles <count>
cargo bench --all-features
```

If dependencies or external ACT4 tools are unavailable, report which checks could not run instead of recording them as passed.

## Git hooks and CI

- Version-controlled hooks live in `.githooks/`.
- Enable them with `git config core.hooksPath .githooks`.
- The pre-commit hook formats staged Rust files and runs `cargo check --all-features` when Rust files are staged.
- The pre-push hook runs strict clippy across all features and targets.
- `.github/workflows/ci.yml` is the main CI workflow; `.github/workflows/bench-scheduled.yml` runs scheduled benchmarks.
- CI uses GitHub-hosted Ubuntu runners and also builds and runs the project-authored RISC-V ELF tests.

Hooks are a convenience, not verification evidence by themselves. Run and report the checks relevant to the task.
