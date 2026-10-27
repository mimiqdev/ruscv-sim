# Git Hooks

This directory contains git hooks for the project.

## Setup

Run once to enable hooks:

```bash
git config core.hooksPath .githooks
```

## Hooks

- `pre-commit`: Runs `cargo fmt --all` and `cargo check --all-features`

## Note

The pre-push hook uses Cargo on PATH first, with an optional `~/.cargo/env`
rustup fallback if Cargo is not on PATH and the file exists.
