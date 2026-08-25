# M7: Spike-Compatible Commit Log

**Status:** Completed (2026-02-11)

## Goal

Add instruction-commit logging that can be compared with Spike output.

## Recorded outcomes

- Added the `--log-commits <FILE>` CLI option
- Added `src/core/commits.rs`
- Added Spike-style hart, privilege, PC, instruction, and register-change formatting
- Added `scripts/log-compare.py`
- Added `scripts/compare.sh`
- Added reference logs and commit-log tests

## Known limitations

- The main execution loop currently passes `None` for memory-access information, so load/store memory details are not fully connected to commit logging.
- Commit logging is a debugging aid for ACT4 failures; it is not the conformance mechanism.
- Historical planning associated this milestone with `v0.7.0`, but the repository currently has no matching Git tag and `Cargo.toml` remains at `0.1.0`.

## Superseded follow-up

The earlier plan to integrate RISCOF was superseded by ACT4 4.0, which replaced the RISCOF flow with UDB configuration, Sail-generated expected results, and self-checking ELFs.
