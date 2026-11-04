# Selectable F/D Floating-Point Profiles contract

**Status:** Historical. Completed 2026-11-04 at PR #160; the [RV64GC default-profile contract](rv64gc-default-profile.md) succeeded it, completed 2026-11-04 at PR #171.

**Authority:** Preserved approved contract, not an active plan or an execution report. CI and PR history retain check and review outcomes.

**Original approval:** 2026-11-03. The owner selected F/D floating point as the next milestone under the delegation of full planning authority to the maintaining agent, who selected decisions D1–D12, all recommended choices. Activated by PR #144 (`fab6904`). The [proposal entry](../../proposals/post-zca-floating-point.md) is informational.

**Completion:** Units U1, U2, U3a, U3b and U4–U7 merged as PR #153, #154, #155, #157, #156, #158, #159 and #160 (issues #145–#152), completed 2026-11-04 at merge head `149eabc`. The contract-rotation merge that activates the successor formally archives this contract; execution and review outcomes stay in CI and PR history.

**Known limitations:** F/D and Zcd are opt-in through `--isa rv64imafd`/`rv64imafdc`; the default remains `rv64ima`. Arithmetic is a pure-Rust Berkeley SoftFloat 3e port checked against committed pinned reference vectors. External ISA evidence is the frozen F (82), D (114) and Zcd (4) ACT4 selections; it is not a whole-extension certification. No Zfa, Zfh, Zfinx, Q, V, misaligned load/store or writable `misa`. The FP ACT4 UDB configurations also declare Zicsr because the pinned UDB rejects F without it; Zicsr is already part of every profile. The Stage 3 performance-certification waiver and the unsealed A10 oracle remain unchanged.

## 1. Objective and inherited profile

Make ordinary RV64GC toolchain output executable on the public path. Add the single- and double-precision floating-point extensions F and D, and the compressed FP loads/stores of Zcd under the compressed profile, as selectable Machine ISA profiles. With an FP profile selected, the Hart has 32 64-bit floating-point registers, the `fflags`/`frm`/`fcsr` CSRs and a writable `mstatus.FS`, and executes every F and D instruction with bit-exact IEEE 754-2008 results and exception flags as the RISC-V ISA Manual specifies. The default `rv64ima` profile and the existing `rv64imac` profile keep today's behavior unchanged, bit for bit.

The inherited profile is RV64IMA/Zicsr plus Zifencei, M/S/U, the selected Stage 5 platform/interrupt/WFI rules, Stage 6 Debug Specification 1.0 behavior, the completed program-buffer and SBA debug decisions, and the completed Zca decisions D1–D10. Existing zero-entry PMP and Sv39 behavior remain unchanged. This contract adds no Zfa, Zfh/Zfhmin, Zfbfmin, Zfinx/Zdinx, Q, V, Zcb/Zcmp/Zcmt, misaligned load/store support, writable `misa`, trigger module, second hart, SBI, Linux target, or TLM/SystemC integration.

The existing `src/fpu`, `src/isa/rv64f` and `src/isa/rv64d` components are not on the active path and are not fit to be switched on. They round results to integers instead of to the destination format, ignore the instruction's static rounding mode, compute FMA unfused, hard-wire `f0` to zero, misdecode `fmt`/`funct7`, and lack FSGNJ, FMIN/FMAX and FMV. This milestone replaces their arithmetic rather than patching it.

The inherited Stage 3 performance-certification waiver continues. `tools/a10/public-v1.json`, its fixtures and their pinned build argv remain unchanged and continue to run under the default profile.

## 2. Specification and selected decisions

The normative ISA reference is the RISC-V ISA Manual document version 20250508 (Unprivileged "F", "D" and "C" chapters, including Zcd; Privileged `misa`, `mstatus.FS`/`SD` and `sstatus`). IEEE 754-2008 is the arithmetic reference as the ISA Manual profiles it. The Debug reference remains Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. OpenOCD is 0.12.0. The comparison ISS is Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`; comparison checks named behavior, not an output digest. External ISA tests are riscv-arch-test 4.0.0, commit `a7c99303516f4e668f7488f172043392e23b9dfd`, with the existing A5 Sail and toolchain pins.

Every decision below was approved on 2026-11-03 under the delegation recorded above.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery | F/D is the next single bounded milestone, before making a compressed or FP profile the default, TLM/SystemC, multi-hart, or performance certification. |
| D2. Extension set | F and D (version 2.2 as in the pinned manual), with their CSRs, plus Zcd (C.FLD, C.FSD, C.FLDSP, C.FSDSP) only under the compressed FP profile. Zcf does not exist on RV64. Every other FP-related extension stays out, so its encodings remain illegal. |
| D3. Profile selection | The Machine-owned ISA profile gains two values, giving exactly four: `rv64ima` (default, `misa` `0x8000000000141101`), `rv64imac` (`0x8000000000141105`), `rv64imafd` (IALIGN=32, `0x8000000000141129`) and `rv64imafdc` (IALIGN=16, `0x800000000014112D`). `run --isa` and the native (`load_and_run`) and flat (`RiscVSimulator`) facades accept all four. No other names or aliases; no ELF inference; `misa` stays read-only. Profile capability checks are explicit (`has_f`/`has_d`-style), not inferred from compression. |
| D4. Arithmetic | Every rounded result, conversion, comparison and classification is bit-exact with IEEE 754-2008 as the ISA Manual specifies. That covers all five rounding modes: static `rm` and dynamic `frm`, with RMM distinct from RNE. Exceptions are detected with tininess after rounding. FMADD/FMSUB/FNMSUB/FNMADD are fused with a single rounding. NaN results of arithmetic, square root, fused multiply-add, conversion and FMIN/FMAX are canonical (`0x7fc00000` and `0x7ff8000000000000`). Sign injection (FSGNJ/FSGNJN/FSGNJX), FMV and FP loads/stores are not arithmetic and preserve NaN payloads bit for bit. Signaling-NaN rules apply to FEQ/FLT/FLE, FMIN/FMAX (version 2.2 `-0 < +0` and NaN rules) and FCVT, with the manual's saturation and invalid-flag table, RV64 sign extension of 32-bit results, and FCLASS bits 8 and 9 distinguished. The implementation is pure Rust with explicit rounding and status: a vetted crate (for example `rustc_apfloat`) or a port of Berkeley SoftFloat 3e. Host `f32`/`f64` arithmetic must not produce a rounded result or flag. No C library or FFI dependency is added. |
| D5. Floating-point state | Thirty-two 64-bit FPRs, all ordinary (`f0` is not hard-wired). Single-precision values are NaN-boxed: every 32-bit write to an FPR sets the upper 32 bits to ones. Every non-transfer single-precision operand read (arithmetic, FSGNJ*, FMIN/FMAX, compares, FCLASS and conversions) from an FPR whose upper 32 bits are not all ones yields the canonical NaN; sign injection preserves the payload of a correctly boxed operand. Narrow transfers out of the register (FSW, FMV.X.W and a 32-bit abstract register read) use the low 32 bits unchanged, as the ISA Manual specifies. `fflags` (5 bits), `frm` (3 bits) and `fcsr` (8 bits) alias exactly; other `fcsr` bits read zero and ignore writes. A reserved static `rm` (5 or 6), or a dynamic `rm` while `frm` holds 5–7, raises illegal-instruction. FP exceptions only accrue flags; they never trap. |
| D6. `mstatus.FS` | Under an FP profile `mstatus.FS` (and its `sstatus` view) is a writable two-bit field resetting to Off (0), and `SD` reads as `FS == Dirty`. While FS is Off every F/D/Zcd instruction raises illegal-instruction. That includes FLW/FLD/FSW/FSD and Zcd loads/stores, FMV.X.W/FMV.X.D, and instructions that only write a GPR (FEQ/FLT/FLE, FCLASS, FCVT to integer). Every `fflags`/`frm`/`fcsr` CSR instruction also raises illegal-instruction. FS becomes Dirty, following pinned Spike, on any retired instruction that writes an FPR, writes `fflags`/`frm`/`fcsr` (CSRRW/CSRRWI always write; CSRRS/CSRRC with `rs1=x0` and CSRRSI/CSRRCI with a zero immediate do not), or accrues a nonzero exception-flag set even when no `fflags` bit changes. FSW/FSD and an FEQ/FLT/FLE/FCLASS/FCVT-to-integer that raises no exception leave FS unchanged. Under `rv64ima`/`rv64imac`, FS and SD stay hard-wired to 0 as today. |
| D7. Decode, dispatch and memory | The 32-bit decoder gains the R4 opcodes (MADD/MSUB/NMSUB/NMADD) and an explicit `rm`/`fmt` decode. Every FP encoding executes through the existing single Hart execution engine. A malformed, reserved or out-of-profile FP encoding raises illegal-instruction; none becomes a `SimulatorFailure`. FLW/FLD/FSW/FSD (and Zcd) use the existing raw DataRead/DataWrite path with its existing alignment, Sv39, fault and reservation-visibility rules. FLW NaN-boxes its result, and FSW stores the low 32 bits unchanged. |
| D8. Zcd | Under `rv64imafdc` the four Zcd encodings expand to FLD/FSD with the specified scaled offsets. Under `rv64imac` and `rv64imafd` they remain illegal-instruction exactly as today. The Zca classification reference gains a profile dimension or a v2 reference covering Zcd. The existing v1 reference and its exhaustive test remain valid for the non-FP profiles. |
| D9. Observation and logs | Commit facts carry FPR and `fcsr` effects (the existing `fpr`/`fcsr` observation fields), and observers never re-derive them. Under FP profiles the commit log prints FPR and `fcsr` writes in pinned Spike's commit-log format, established from the pinned source. Under `rv64ima`/`rv64imac` the log text is unchanged. |
| D10. Debug | Under FP profiles, DM abstract Access Register supports `f0`–`f31` (regno `0x1020`–`0x103f`, `aarsize` 2 or 3 as the spec allows for a 64-bit FLEN) and `fflags`/`frm`/`fcsr` through the CSR regnos, independent of `mstatus.FS`. Following pinned Spike, abstract register access leaves `mstatus.FS` unchanged. A 32-bit FPR write stores the value NaN-boxed, and a 32-bit read returns the low 32 bits unchanged. Program-buffer FP instructions follow the ordinary FS rule. The DM reports the profile's `misa`. Under non-FP profiles the existing `cmderr=3` behavior is unchanged. |
| D11. ISA verification | New frozen ACT4 selections at the pinned commit: every source in `rv64i/F` (82) and `rv64i/D` (114) under `rv64imafd`, and every source in `rv64i/Zcd` (4) under `rv64imafdc`. The F and D selections use a Sail/UDB configuration that adds only F and D to the existing RV64I configuration. The Zcd selection uses one that adds only F, D and Zcd to the existing Zca configuration. Both follow the existing A5 flow pattern. The frozen 51-source RV64I and 32-source Zca selections stay unchanged under their profiles. Every other source stays excluded, including the MisalignF, MisalignD, ZfaF, ZfaD, ZfaZfh, ZfaZfhD, ZfaZvfh, Zfh, ZfhD, Zfhmin, ZfhminD and Zfbfmin directories. Project-authored guests built with `-march=rv64imafd_zicsr_zifencei` or `rv64imafdc_zicsr_zifencei` join the fresh guest suite under their profile. Existing guests are unchanged, including `s4_fp_absent`, which still runs under `rv64ima`. |
| D12. Tool verification | One new OpenOCD/GDB session on an `rv64imafdc` guest: read and write `f0`–`f31` and `fcsr` through the debugger, then continue and verify those values from the guest. It also covers a breakpoint and a step over FP instructions, with `mstatus.FS` checks. Named behavior is compared with pinned Spike run with `--isa=rv64imafdc_zicsr`. Existing sessions and their Spike ISA strings remain regression controls. Only explicitly justified profile differences appear in test expectations. |

## 3. Ownership and architectural semantics

### Profile and Hart state

The Machine owns profile selection as composition configuration and passes it to the Hart at construction, per the ADR-0003 amendments. Frontends and the Runner forward an explicit selection only. The Hart alone owns FPR state, `fcsr`, `mstatus.FS`, FP legality, rounding, exception accrual, NaN-boxing and retirement. The Platform sees FP loads and stores as ordinary data transactions of 4 or 8 bytes and gains no FP knowledge. Reset restores the selected profile's reset state: FPRs zero, `fcsr` zero, FS Off.

### Execution and observation

An FP instruction retires or traps exactly once like any other instruction. Accrued exception flags are architectural effects of retirement, not traps. A trapping FP instruction (illegal encoding, FS Off, reserved rounding mode, or a load/store fault) leaves no FPR, `fcsr` or FS effect. The arithmetic implementation is a Hart-internal component with no host floating-point environment dependency, so results do not vary with host CPU, compiler flags or rounding state. Counters, turn accounting and `ExecutionResult.cycles` are per instruction and unchanged.

### Debug and lifecycle

Debug Mode entry, resume, step, abstract commands, the program buffer and SBA keep their ownership. The DM reaches FPRs and FP CSRs only through the Hart's abstract-register boundary, exactly as for GPRs and CSRs. Under `rv64ima`/`rv64imac` nothing in the DM changes.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. FP state and profiles | Add `rv64imafd`/`rv64imafdc` with exact `misa` through `run --isa` and both facades. Cover 64-bit FPRs with an ordinary `f0`, NaN-boxing reads, `fflags`/`frm`/`fcsr` aliasing and masking, writable FS/SD with the `sstatus` view, FS Off on reset, and reset of FP state. Prove the default and `rv64imac` profiles are unchanged: existing tests, the guest suite and the A10 controls pass unmodified. Existing FP component tests that encode the recorded defects are corrected, not preserved. |
| U2. Arithmetic core | A pure-Rust FP arithmetic component covering every F/D operation: add/sub/mul/div/sqrt, fused multiply-add, min/max, compare, classify, sign injection, and all integer/float and float/float conversions. It must take an explicit rounding mode and return exact exception flags. Check it against an independent pinned reference: Berkeley TestFloat 3e vectors, or an equivalent generator, committed as test inputs with the generator, its pin, and a fixed level/seed chosen so the committed vectors stay of reviewable size. Cover all five rounding modes, subnormals, signed zeros, infinities, quiet and signaling NaNs, and the FCVT saturation table. No host-float rounded results. |
| U3a. Decode and execution | Decode R4 and all OP-FP/LOAD-FP/STORE-FP forms with `fmt`/`rm`, and execute every F/D instruction on the public path under FP profiles through the U2 core. Cover static and dynamic rounding, reserved-`rm` and FS-Off illegal-instruction traps, the D6 FS-Dirty rule, D4/D5 NaN-boxing and payload preservation, and FP loads and stores through the existing physical path (including misaligned and Sv39 faults). Malformed FP encodings must trap rather than produce `SimulatorFailure`. Commit facts carry FPR and `fcsr` effects. |
| U3b. Zcd and FP commit log | Expand Zcd under `rv64imafdc` only, with the Zca reference extended by a profile dimension or a v2 reference while the v1 exhaustive test stays valid. Prove the D9 Spike-format FPR and `fcsr` commit-log output under FP profiles against the pinned Spike format, and that the commit log is unchanged under `rv64ima`/`rv64imac`. |
| U4. Debug integration | Under FP profiles, abstract Access Register reads and writes `f0`–`f31` at `aarsize` 2 and 3 and `fflags`/`frm`/`fcsr`, independent of FS and leaving FS unchanged. Cover program-buffer FP instructions with FS On and Off, and the DM `misa` read for all four profiles. Existing abstract, program-buffer and SBA tests pass unchanged under `rv64ima`. |
| U5. External ISA verification | Freeze the F (82), D (114) and Zcd (4) ACT4 selections. Extend the A5 tooling: FP opcodes in the audit allow-list, and UDB and Sail configurations enabling only the selected extensions. Run them through the pinned flow on the PR head, and record the runs in PR history. The RV64I and Zca selections still pass unchanged. Add project-authored FP guests to the fresh CI suite covering rounding modes, flags, NaN-boxing, FS traps, FMA, conversions and Zcd. |
| U6. Public tools | Extend the existing tool script with the D12 FP session against pinned Spike `--isa=rv64imafdc_zicsr`. Retain every existing session unchanged as a regression control. |
| U7. Exit | CI runs the full Rust quality gate, the fresh guest suite with each guest under its profile, the extended tool script, legacy regressions and unit-specific negative controls. Record the ACT4 F/D/Zcd runs in PR history. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Dependencies: U1 and U2 may proceed in parallel. U3a requires both. U3b and U4 require U3a. U5 requires U3a and U3b. U4 and U5 may proceed in parallel. U6 requires U4 and U5. U7 requires all.

Required negative controls:

- rounding a result to an integer instead of to the format;
- ignoring a static `rm`;
- RMM treated as RNE;
- FMA computed with two roundings;
- `f0` hard-wired to zero;
- an unboxed single-precision read not yielding the canonical NaN;
- an FP instruction accepted with FS Off;
- an FPR write that does not set FS to Dirty;
- FEQ setting NV for a quiet NaN;
- FCVT.WU.S of a negative value not saturating to 0;
- C.FLD accepted under `rv64imac` or `rv64imafd`;
- a dynamic `rm` accepted while `frm` holds 5–7;
- the default profile advertising F.

Keep their small patches, commands and expected assertions as verification code or test inputs under the existing mutation-runner pattern.

The standard gate remains `cargo fmt --all -- --check`, `cargo check --all-features`, strict all-target Clippy, `cargo test --all-features`, `cargo doc --all-features --no-deps`, the fresh guest ELF suite and the required live debug-attach CI. Do not add an execution ledger, fixed successful-output inventory or hash gate to certify these requirements.

## 5. Exclusions and retained direction

No Zfa, Zfh/Zfhmin, Zfbfmin, Zfinx/Zdinx, Q, V, Zcb/Zcmp/Zcmt/Zcmop, misaligned load/store execution, writable `misa` or run-time extension disabling, profile aliases (for example `rv64gc`), change of the default profile, ELF-driven profile inference, floating-point traps, second hart, Sdtrig, Quick Access, new protection/virtualization profile, legal DRET, Linux/SBI, TLM/SystemC/DMI integration, performance certification, oracle sealing, A10 suite change, or new public application result field.

Making a richer profile the default is a later, separate decision. It requires deliberately re-deriving the A10 oracle and updating the fixtures that pin the IMA `misa`. Conditional transport integration and the multi-master remainder of ADR-0002 §9 stay deferred. Neither is silently carried into this contract.

## 6. Activation boundary

This documentation-only rotation preserves the entire Zca contract, applies the bounded 2026-11-03 floating-point ADR-0003 amendment and activates this target atomically on merge. CI/PR history records delivery and review. It does not assert runtime support, start implementation work, publish a release, or seal an oracle. Later implementation must meet the units above on its own reviewed commits.

## References

- [RISC-V ISA Manual 20250508 release](https://github.com/riscv/riscv-isa-manual/releases/tag/20250508) (Unprivileged F, D and C/Zcd chapters; Privileged `misa`, `mstatus.FS`).
- [Berkeley SoftFloat 3e](http://www.jhauser.us/arithmetic/SoftFloat.html) and [TestFloat 3e](http://www.jhauser.us/arithmetic/TestFloat.html) (arithmetic and reference-vector options).
- [Debug Specification 1.0 DM](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc).
- [ADR-0001](../../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [riscv-arch-test 4.0.0 pinned commit](https://github.com/riscv/riscv-arch-test/tree/a7c99303516f4e668f7488f172043392e23b9dfd) (`tests/rv64i/F`, `D`, `Zcd`), [pinned Spike](https://github.com/riscv-software-src/riscv-isa-sim/tree/5d99611782ef8dd81b65b842369bd351a9d4702a) and [OpenOCD 0.12.0](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
- [Post-A7 roadmap](../../proposals/post-a7-roadmap.md) and [F/D proposal entry](../../proposals/post-zca-floating-point.md).
