# Selectable Zca Compressed-Instruction Profile contract

**Status:** Historical. Completed 2026-11-03 at PR #143; the [Selectable F/D Floating-Point Profiles contract](../../dev-plan.md) succeeds it as the sole Current contract on the actual rotation merge.

**Authority:** Preserved approved contract, not an active plan or an execution report. CI and PR history retain check and review outcomes.

**Original approval:** 2026-11-03, under the owner's explicit delegation of full planning authority to the maintaining agent, decisions D1–D10, all recommended choices. Activated by PR #131 (`9db1bed`). The [proposal entry](../../proposals/post-sba-compressed-profile.md) is informational.

**Completion:** Units U1–U6 merged as PR #138, #139, #141, #140, #142 and #143 (issues #132–#137), completed 2026-11-03 at merge head `34f1408`. The contract-rotation merge that activates the successor formally archives this contract; execution and review outcomes stay in CI and PR history.

**Known limitations:** Zca only, opt-in through `--isa rv64imac`; the default remains `rv64ima`, and existing guests run only under it. No Zcb/Zcmp/Zcmt/Zcmop, misaligned load/store or writable `misa`. Zcd encodings remain illegal because F/D was outside the profile. External ISA evidence is the frozen 32-source non-trapping `rv64i/Zca` ACT4 selection; it is not an RV64GC or whole-extension certification. The Stage 3 performance-certification waiver and the unsealed A10 oracle remain unchanged.

## 1. Objective and inherited profile

Make ordinary `rv64imac` toolchain output executable on the public path by adding the RV64 Zca compressed-instruction extension as a selectable Machine ISA profile. With the `rv64imac` profile selected, the Hart fetches 16-bit parcels, decodes and executes every Zca instruction through the existing execution engine, uses IALIGN=16 for control flow, trap and debug state, and advertises `misa.C`. The default `rv64ima` profile keeps today's behavior unchanged, bit for bit.

The inherited profile is RV64IMA/Zicsr plus Zifencei, M/S/U, the selected Stage 5 platform/interrupt/WFI rules, Stage 6 Debug Specification 1.0 behavior, the completed Program-buffer Debug Execution decisions D1–D9 and the completed System Bus Access Debug decisions D1–D10. Existing zero-entry PMP and Sv39 behavior remain unchanged. This contract adds no F/D/V extension, Zcb/Zcmp/Zcmt/Zcf/Zcd, misaligned load/store support, writable `misa`, trigger module, second hart, SBI, Linux target, or TLM/SystemC integration.

This is the first ISA-profile increment after the completed debug direction. Its value is that ordinary compiled code, which defaults to compressed instructions, becomes runnable. It is not a claim of RV64GC support or whole-extension certification beyond the selected external verification.

The inherited Stage 3 performance-certification waiver continues. Comparison publication, noise calibration, thresholds, oracle sealing and new performance payloads are not taken on. `tools/a10/public-v1.json`, its fixtures and their pinned build argv remain unchanged and continue to run under the default profile.

## 2. Specification and selected decisions

The normative ISA reference is the RISC-V ISA Manual document version 20250508 (Unprivileged "Zca" and "C" chapters; Privileged `misa`, `mepc`/`sepc` and IALIGN rules). The Debug reference remains Debug Specification 1.0, commit `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`. OpenOCD is 0.12.0. The comparison ISS is Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`; comparison checks named behavior, not an output digest. External ISA tests are riscv-arch-test 4.0.0, commit `a7c99303516f4e668f7488f172043392e23b9dfd`, with the existing A5 Sail and toolchain pins.

Every decision below was approved on 2026-11-03 under the delegation recorded above.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery | The Zca profile is the next single bounded milestone, before TLM/SystemC, multi-hart, F/D, or performance certification. |
| D2. Extension set | Zca only. Without F/D, `misa.C=1` denotes exactly Zca. C.FLD, C.FSD, C.FLDSP and C.FSDSP raise illegal-instruction, as their 32-bit FP counterparts do today. Exactly the specification HINT encodings retire with no architectural effect: C.NOP with nonzero immediate; C.ADDI with `rd≠0` and zero immediate; C.LI and C.SLLI with `rd=0`; C.LUI with `rd=0` and nonzero immediate; C.MV and C.ADD with `rd=0` and `rs2≠0`; and C.SLLI, C.SRLI and C.SRAI with zero shift amount. C.NOP with zero immediate is an ordinary NOP. Reserved encodings raise illegal-instruction with `mtval`/`stval` holding the 16-bit parcel zero-extended. These include the all-zero parcel and C.ADDI4SPN with zero immediate, C.ADDI16SP and C.LUI with zero immediate, C.LWSP and C.LDSP with `rd=0`, C.JR with `rs1=0`, C.ADDIW with `rd=0`, and the reserved RV64 C.SUBW/C.ADDW-group encodings. No Zcmop or Zcb meaning is given to any of them. |
| D3. Profile selection | The Machine owns an ISA profile with exactly two values: `rv64ima` (default; IALIGN=32; `misa` `0x8000000000141101`) and `rv64imac` (IALIGN=16; `misa` `0x8000000000141105`). The `run` CLI command gains one approved option, `--isa <rv64ima\|rv64imac>`. The native (`load_and_run`) and flat (`RiscVSimulator`) library facades accept the same profile. `ExecutionResult` keeps its shape. No profile is inferred from ELF `e_flags`. `misa` stays read-only in both profiles, so IALIGN never changes at run time. |
| D4. Parcel fetch | Under `rv64imac` the fetch rule is per 16-bit parcel: the first parcel at `pc`, and a second at `pc+2` only when the first's low two bits are `0b11`. Each parcel is a fetch transaction under ADR-0002 and is translated separately when it lies on another page. An instruction faults only for a parcel it occupies. The first parcel's fault takes priority. A second-parcel fault reports `tval` as that parcel's virtual address while `xepc` is the instruction's `pc`. A fault in either parcel retires nothing and leaves no partial effect. The implementation may combine parcels into one transaction only where this is observably identical. |
| D5. Control flow and alignment | Under `rv64imac`: fall-through and link values are `pc + length` (2 or 4); the instruction-address-misaligned check uses bit 0 only; `mepc`, `sepc` and `dpc` clear only bit 0 on write and on return; and MRET/SRET return to 2-byte-aligned targets. Under `rv64ima` every existing mask and check is unchanged. |
| D6. One execution engine | The Hart decodes a compressed parcel by expanding it to the equivalent 32-bit instruction and executing that through the existing executor. There is no second execution engine. The existing `src/isa/rv64c` component may be repaired or replaced; its known RV64 mapping, width, immediate and HINT defects must not reach the active path. Its standalone `exec_c_*` helpers do not become an execution path. |
| D7. Observation and logs | Commit facts and trap observations carry the original fetched bits and a length of 2 or 4, never the expansion. Under `rv64ima` the existing commit-log text is unchanged. Under `rv64imac` the format for 16-bit instructions follows pinned Spike's commit-log format, established from the pinned source in U2. |
| D8. Debug interaction | `dpc` legality follows the D5 profile mask. C.EBREAK is EBREAK: with `ebreakm/s/u` redirection it enters Debug Mode with `dcsr.cause=1` and `dpc` equal to its address; otherwise it raises Breakpoint. Single-step advances over one instruction of either length. Under `rv64imac` the two program-buffer words form a contiguous 8-byte parcel stream, followed by the implicit 32-bit EBREAK. C.EBREAK ends the command successfully, and compressed control transfers (C.J, C.JR, C.JALR, C.BEQZ, C.BNEZ) join the program-buffer D6 forbidden set with `cmderr=3`. A 32-bit instruction whose second parcel would extend past `progbuf1` raises `cmderr=3` with no effect. `progbufsize`, `datacount` and `impebreak` are unchanged. The DM reports the selected profile's `misa`. |
| D9. ISA verification | A new frozen ACT4 selection takes the 32 `rv64i/Zca` sources at the pinned commit. They run under `rv64imac` against a Sail configuration that enables only Zca beyond the existing profile, using the existing pinned A5 flow. The frozen 51-case RV64I selection stays unchanged under `rv64ima`. Only `rv64i/Zca` is selected. Every other source stays excluded, including the MisalignZca, Zcb, ZcbM, ZcbZba, ZcbZbb, Zcd and Zcmop directories, `rv64e` and PMP. Project-authored guests built with `-march=rv64imac_zicsr_zifencei` join the fresh guest ELF suite under `--isa rv64imac`. Existing guests keep their build flags and run only under the default `rv64ima`; some pin the IMA `misa` and IALIGN=32 masks by design. |
| D10. Tool verification | One new OpenOCD/GDB session on a mixed-length `rv64imac` guest covers four cases: a software breakpoint on a 2-byte instruction, so OpenOCD writes C.EBREAK; a breakpoint on a 4-byte instruction at `pc % 4 == 2`; continue and single-step across mixed lengths; and `dpc`/PC/register checks. Named behavior is compared with pinned Spike run with its existing ISA string plus `c`. Existing `rv64ima` sessions and their Spike ISA string remain regression controls. Only explicitly justified profile differences appear in test expectations. |

## 3. Ownership and architectural semantics

### Profile, Hart and fetch

The Machine owns profile selection as composition configuration and passes it to the Hart at construction. The Runner and frontends only forward the selected value. The Hart alone owns parcel assembly, instruction length, decode, legality, IALIGN rules, translation, traps and retirement. The Platform sees ordinary fetch transactions and gains no compressed-instruction knowledge.

Fetch remains a non-side-effecting transaction that is rejected for devices, exactly as today. A straddling instruction's two parcels are separate transactions under the ADR-0002 compressed-fetch amendment. The instruction is one Hart step: either both parcels are fetched and the instruction proceeds, or the step ends in one fetch fault with the D4 priority and `tval` rule. Translation of the second parcel follows the existing Sv39, A/D and fault semantics; this contract adds no new translation cache invalidation rule.

### Decode, execution and observation

A 16-bit parcel expands to exactly one existing 32-bit instruction form with identical architectural effects, except that its length is 2 for PC, link and observation purposes. Legality checks, including FP-absent, privilege and CSR rules, apply to the expansion exactly as to the 32-bit form, while trap values report the original parcel. Atomic, reservation, physical-access and SBA visibility semantics are unchanged; Zca contains no atomic instruction.

Retirement, trap entry, `minstret`/`mcycle`/`mtime` accounting and `ExecutionResult.cycles` are per instruction regardless of length. Commit facts originate from the Hart's fetched bits, and no observer re-fetches them. WFI detection, interrupt sampling and stop boundaries are unchanged.

### Debug and lifecycle

Debug Mode entry, resume, step, abstract commands, program-buffer execution and SBA keep their existing ownership. Only the D5/D8 profile rules change their IALIGN-dependent values. Under `rv64ima`, a compressed parcel in a program-buffer word remains an illegal instruction, so `cmderr=3` as today. Reset restores the selected profile's reset state; profile selection is fixed for the Machine's lifetime.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. Zca decode | Expand every Zca encoding in all three quadrants. An exhaustive 65,536-parcel classification test (legal expansion / HINT / reserved / FP-compressed illegal) checks against an independent pinned reference. That reference is committed as a test input with its generator and pin. Boundary-immediate tests cover every immediate format. Prove the recorded defects are absent: C.LD/C.SD/C.LDSP/C.SDSP mapping and widths, C.LW/C.SW/C.LWSP/C.SWSP widths, C.ADDIW opcode, C.ADDI4SPN/C.J/C.B*/C.*SP immediates, and C.NOP/HINT handling. |
| U2. Hart profile and fetch | Select the profile through the CLI and the native and flat library facades, with no ELF inference. Prove parcel fetch, 32-bit instructions at `pc % 4 == 2`, and straddling across a page boundary: each parcel's page or access fault, D4 priority and `tval`, and no partial effect. Also cover a compressed instruction ending exactly at a RAM-region end, fetch rejection from MMIO, D5 link/fall-through/masks/MRET/SRET, `misa`, and observation lengths and logs. Prove that the default `rv64ima` profile is unchanged: existing tests, the guest suite and the A10 controls pass unmodified. The only exemption is the off-path `src/isa/rv64c` component's own tests (`tests/rv64c_test.rs` and its module tests), which follow whatever D6 repair or replacement U1 selects. Defect-encoding assertions there are corrected, not preserved. |
| U3. Debug integration | Cover profile `dpc` legality; C.EBREAK redirection and Breakpoint trap; single-step over 2- and 4-byte instructions, including at `pc % 4 == 2`; and the program-buffer parcel stream with mixed lengths. Also cover C.EBREAK termination, compressed forbidden-control transfers with `cmderr=3`, the straddle into the implicit EBREAK, and the DM `misa` read under both profiles. Existing abstract, program-buffer and SBA tests pass unchanged under `rv64ima`. |
| U4. External ISA verification | Freeze the 32-source `rv64i/Zca` ACT4 selection. Extend the A5 tooling (audit of 16-bit parcels, UDB and Sail configuration enabling only Zca) and run it through the pinned flow on the PR head. Keep the 51-case RV64I selection and its replay validators unchanged. Add project-authored `rv64imac` guests (mixed-length control flow, link values, straddling fetch, C.EBREAK trap, HINTs) to the fresh CI guest suite. |
| U5. Public tools | Extend the existing tool script with the D10 compressed session against pinned Spike, `--isa` with C added. Retain the abstract, program-buffer and sysbus sessions as `rv64ima` regression controls. |
| U6. Exit | CI runs the full Rust quality gate, the fresh guest ELF suite (existing guests under `rv64ima`, compressed guests under `rv64imac`), the extended tool script, legacy regressions and unit-specific negative controls. Record the ACT4 Zca run in PR history. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Required negative controls include C.LW expanded as LH, a straddling fetch that translates only the first parcel, a C.JALR link of `pc+4`, `mepc` masking bits 1:0 under `rv64imac`, the default profile advertising `misa.C`, C.EBREAK redirection with a 4-byte-masked `dpc`, a program buffer accepting C.J, and C.NOP rejected as reserved. Keep their small patches, commands and expected assertions as verification code or test inputs under the existing mutation-runner pattern.

The standard gate remains `cargo fmt --all -- --check`, `cargo check --all-features`, strict all-target Clippy, `cargo test --all-features`, `cargo doc --all-features --no-deps`, the fresh guest ELF suite and the required live debug-attach CI. Do not add an execution ledger, fixed successful-output inventory or hash gate to certify these requirements.

## 5. Exclusions and retained direction

No F/D/V, Zcb/Zcmp/Zcmt/Zcf/Zcd, misaligned load/store execution, writable `misa` or dynamic IALIGN, ELF-driven profile inference, RV64GC claim, second hart, Sdtrig, Quick Access, new protection/virtualization profile, legal DRET, Linux/SBI, TLM/SystemC/DMI integration, performance certification, oracle sealing, A10 suite change, or new public application result field.

Conditional transport integration and the multi-master remainder of ADR-0002 §9 stay deferred and require a concrete consumer and fixture. F/D is a plausible later ISA-profile increment under its own contract; it is not silently carried into this one.

## 6. Activation boundary

This documentation-only rotation preserves the entire System Bus Access Debug contract, applies the bounded 2026-11-03 ADR-0001–0003 amendments and activates this target atomically on merge. CI/PR history records delivery and review. It does not assert runtime support, start implementation work, publish a release, or seal an oracle. Later implementation must meet the units above on its own reviewed commits.

## References

- [RISC-V ISA Manual 20250508 release](https://github.com/riscv/riscv-isa-manual/releases/tag/20250508) (Unprivileged Zca/C chapter; Privileged `misa` and `xepc`).
- [Debug Specification 1.0 DM](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/debug_module.adoc) and [Sdext](https://github.com/riscv/riscv-debug-spec/blob/5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8/Sdext.adoc).
- [ADR-0001](../../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [riscv-arch-test 4.0.0 pinned commit](https://github.com/riscv/riscv-arch-test/tree/a7c99303516f4e668f7488f172043392e23b9dfd) (`tests/rv64i/Zca/`), [pinned Spike](https://github.com/riscv-software-src/riscv-isa-sim/tree/5d99611782ef8dd81b65b842369bd351a9d4702a) and [OpenOCD 0.12.0](https://github.com/openocd-org/openocd/blob/v0.12.0/src/target/riscv/riscv-013.c).
- [Post-A7 roadmap](../../proposals/post-a7-roadmap.md) and [Zca proposal entry](../../proposals/post-sba-compressed-profile.md).
