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

The hook requires cargo to be in PATH. If cargo is installed via rustup,
ensure `$HOME/.cargo/bin` is in your PATH.
