# AGENTS.md — ruscv-sim Repository Guide

## Current plan

- Read `docs/dev-plan.md` before planning or implementing project work.
- `docs/dev-plan.md` is the single source of truth for the active milestone and contains exactly one milestone.
- Treat `docs/archive/` as historical context only. Archived checkboxes, old version targets, and superseded frameworks are not current tasks.
- When `README.md`, `CHANGELOG.md`, design documents, or archived plans conflict with `docs/dev-plan.md`, follow `docs/dev-plan.md` and note the stale documentation if it affects the task.
- Do not silently add unrelated cleanup, old backlog items, or speculative future features to the active milestone.

## Rolling milestone workflow

When the active milestone is complete:

1. Verify every acceptance criterion in `docs/dev-plan.md` with recorded evidence.
2. Move the completed plan to `docs/archive/milestones/` as a standalone record.
3. Record its completion date, test evidence, known limitations, and relevant commit or release identifiers.
4. Re-evaluate unfinished work; do not automatically carry it forward.
5. Replace `docs/dev-plan.md` with the next approved milestone, keeping only one active milestone.

Do not delete or rewrite historical milestone records merely to match the current architecture.

## Repository facts

- Primary branch: `main`
- Language: Rust, edition 2021
- Crate version: read from `Cargo.toml`; do not infer releases from milestone numbers
- Current product priority: read from `docs/dev-plan.md`
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
