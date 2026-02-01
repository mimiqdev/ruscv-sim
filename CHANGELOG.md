# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- RV64I base instruction set support (64-bit integer operations)
  - 64-bit register file (x0-x31 as 64-bit registers)
  - 64-bit arithmetic instructions: ADDW, SUBW, SLLW, SRLW, SRAW
  - 64-bit immediate instructions: ADDIW, SLLIW, SRLIW, SRAIW
  - 64-bit load/store: LD, SD, LWU (zero-extending word load)
- RV64M multiplication and division extension
  - 64-bit multiply: MUL, MULH, MULHU, MULHSU
  - 64-bit divide: DIV, DIVU, REM, REMU
  - Proper overflow handling per RISC-V spec (i64::MIN / -1 returns i64::MIN)
- RV64A atomic operation extension
  - Load-reserved/Store-conditional: LR.D, SC.D
  - Atomic memory operations: AMOADD.D, AMOSWAP.D, AMOAND.D, AMOOR.D, AMOXOR.D,
    AMOMAX.D, AMOMIN.D, AMOMAXU.D, AMOMINU.D
- RV64F single-precision floating-point extension
  - 32-bit floating-point register operations
  - IEEE 754-2008 compliant arithmetic
  - NaN boxing/unboxing for upper 32 bits of 64-bit registers
- RV64D double-precision floating-point extension
  - 64-bit floating-point register operations
  - Full IEEE 754-2008 compliant arithmetic
- CSR (Control and Status Register) framework
  - Machine mode CSRs: mstatus, misa, medeleg, mideleg, mie, mtvec, mcounteren,
    mscratch, mepc, mcause, mtval, mip, mhartid
  - Supervisor mode CSRs: sstatus, sie, stvec, scounteren, sscratch, sepc,
    scause, stval, sip, satp
  - Virtualization mode CSRs: vsstatus, vsie, vstvec, vsscratch, vsepc,
    vscause, vstval, vsip, vsatp
  - CSR instruction support: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI
- Privilege mode support
  - User (U), Supervisor (S), and Machine (M) modes
  - Privilege mode transitions and protection
  - Trap handling framework with MRET/SRET instructions

### Changed

- Migrated from RV32I to RV64I as the base architecture
- All integer registers upgraded from 32-bit to 64-bit
- Memory addressing upgraded to support 64-bit virtual address space
- MSTATUS register mask updated for RV64 (0x8000_0003_000D_FFEA)

### Technical Details

#### Division Overflow Handling
Per RISC-V Spec Volume I, Section 2.4:
- Division by zero returns all ones (-1)
- Overflow case (signed MIN / -1) returns MIN (not undefined behavior)

#### MSTATUS Register Layout (RV64)
Per RISC-V Privileged Spec, Section 3.1.6:
- Bit 63 (SD): State Dirty (read-only)
- Bits 35:34 (SXL): Supervisor XLEN
- Bits 33:32 (UXL): User XLEN
- Bit 22 (TSR): Trap SRET
- Bit 21 (TW): Timeout Wait
- Bit 20 (TVM): Trap Virtual Memory
- Bit 18 (MXR): Make Executable Readable
- Bit 17 (SUM): Supervisor User Memory Access
- Bit 13 (FS): Floating-point State
- Bits 12:11 (MPP): Machine Previous Privilege
- Bit 8 (SPP): Supervisor Previous Privilege
- Bit 7 (MPIE): Machine Previous Interrupt Enable
- Bit 5 (SPIE): Supervisor Previous Interrupt Enable
- Bit 3 (MIE): Machine Interrupt Enable
- Bit 1 (SIE): Supervisor Interrupt Enable

## [0.1.0] - 2025-01-XX

### Added

- Initial project setup with Rust 2024 Edition
- CI/CD pipeline with GitHub Actions
- Pre-commit and pre-push git hooks
- Basic project structure and documentation
- RV32I base instruction set foundation
