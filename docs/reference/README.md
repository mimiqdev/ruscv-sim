# Reference Index

**Status:** Current index

**Authority:** Informational

## Sources of truth

- Rust dependencies and crate version: `Cargo.toml` and `Cargo.lock`.
- CLI behavior: `src/main.rs` plus CLI tests.
- Implemented instruction behavior: active decode/execute path plus focused tests.
- End-to-end support: public ELF-path verification, not component file presence.
- RISC-V architectural behavior: the applicable official RISC-V specifications.

## Current references

- [Code-generation component status](code-generation.md)

Historical RV64A/M/D implementation notes and dependency snapshots are retained under [`../archive/reference/`](../archive/reference/). They are useful investigation starting points but are not compliance evidence.
