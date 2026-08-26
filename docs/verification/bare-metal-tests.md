# Project-Authored Bare-Metal Tests

**Status:** Current guide

**Authority:** Normative for guest tests under `tests/bare-metal-riscv-test/`

**Last verified:** 2026-08-26

## Role

These tests are small RISC-V assembly programs compiled into ELF files and executed through the public CLI path. They are project regressions, not a substitute for an external architecture-compliance suite.

## Current layout

```text
tests/bare-metal-riscv-test/
├── Makefile
├── linker.ld
├── rv64i/*.S
└── rv64m/*.S
```

The Makefile knows extension-directory names beyond RV64I/RV64M, but only source files actually present are built. The helper scripts currently discover RV64I and RV64M sources and ELF files.

## Toolchain

The default prefix is `riscv64-unknown-elf-`. Override it with `RISCV_PREFIX` when necessary.

```bash
./scripts/compile_riscv_tests.sh
./scripts/run_elf_tests.sh
```

Or build from the guest-test directory:

```bash
make -C tests/bare-metal-riscv-test
```

Run one ELF directly:

```bash
cargo run -- run tests/bare-metal-riscv-test/rv64i/add.elf --max-cycles 100000
```

## Link and exit contract

The linker places code at `0x8000_0000` and `.tohost` at `0x8000_1000`. Guest sources align `.tohost` to eight bytes.

Current project-authored tests normally write:

```text
(1 << 63) | exit_code
```

The simulator also recognizes an HTIF syscall/exit payload. A passing project-authored test exits with code zero; a nonzero exit is a guest-test failure. Timeout or a simulator error must not be reported as a guest pass/fail result.

## Adding a regression

1. Put the smallest reproducing `.S` program in the appropriate extension directory.
2. Use the existing `.tohost` section and self-check the architectural result in guest code.
3. Make success write exit code zero and failure write a stable nonzero code.
4. Build and run the single ELF.
5. Add or update a Rust integration test when the regression depends on loader, Runner, or process behavior.
6. Do not infer extension-wide support from the new test alone.

## CI behavior

The Rust test job installs the RISC-V GNU toolchain because at least one integration test assembles a guest program. On pushes to `main`, CI also compiles and runs the project-authored RV64I/RV64M ELF set.
