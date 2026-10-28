# Stage 6 acceptance assessment

**Status:** Current assessment of implementation `5c1d031`. Stage 6 remains the sole current contract; formal completion and successor activation await owner decisions.

**Authority:** Informational evidence against [the Stage 6 contract](../dev-plan.md) §7 and accepted [ADR-0001–0004](../architecture/decisions/README.md).

## Implementation and verification identities

Stage 6 was activated by [PR #96](https://github.com/mimiqdev/ruscv-sim/pull/96), merge `a8851c4ac57500eb54dadc89919cd7314b626594`, on 2026-10-19. That rotation completed and archived Stage 5.

| Commit | Result |
| --- | --- |
| `4332841a565fb845952cd52ccf84283959f373bf` | U1–U6 baseline. The initial cumulative review requested changes for the two reset defects below. The local gate passed 2033 Rust tests and 85 fresh guests on 2026-10-28; [baseline CI](https://github.com/mimiqdev/ruscv-sim/actions/runs/40628551229) is separate evidence. |
| `654dda9fe63cf9d05d39ed6901700e73ca4edb95` | Both reset regressions fail their intended assertions, one test per command with exit 101. |
| `5c1d031b2a66bc17d058b5a7355d88ebe80eb652` | Corrects both defects. The full local gate passed 2039 Rust tests and 85 fresh guests on 2026-10-28. The independent [PR #107 code review](https://github.com/mimiqdev/ruscv-sim/pull/107#pullrequestreview-5678303368) records APPROVE and 100 focused passes. |

[CI 40638546079](https://github.com/mimiqdev/ruscv-sim/actions/runs/40638546079) passed Quality and tests and Debug attach for the 5c PR head. Coverage was skipped. CI checked out synthetic merge `f7a2bb1e99fbba34fa5da9e3160e261f31870129`; its tree equals the 5c source tree `6f488e7788508f3a79a40f1d92e329c5eb26b8b5`. CI reports 2039 Rust passes across 82 summaries, zero failed or ignored, 85 fresh compiled/passed guests, and live OpenOCD/GDB/Spike sessions with both tool negatives.

The 5c local gate ran from 2026-10-28T05:34:41Z to 05:41:17Z with Rust 1.99.1 and GNU RISC-V Binutils 2.45. All six commands below exited 0. It did not run OpenOCD. The earlier 433 gate also passed its five Rust commands; an initial guest invocation rejected a nested output path with exit 2 before a corrected invocation compiled and passed 85 guests.

These are results at the identified implementation commits and CI checkout. Each later documentation head needs its own applicable checks and independent review in PR history. The code review's GitHub state is COMMENTED; its body records the independent APPROVE verdict. It grants no merge or successor approval.

## Acceptance map

The linked tests contain the detailed assertions for each contract clause. They distinguish component behavior, native integration, flat RAM, and tool behavior.

| Contract §7 | Source and behavioral proof |
| --- | --- |
| 1. Hart Debug Mode | [Hart](../../src/core/mod.rs), [debug state](../../src/core/debug_mode.rs), and [U1 tests](../../tests/s6_u1_hart_debug.rs) cover debug CSR WARL, each halt cause/privilege/PC, EBREAK trap versus redirect, saved trap state, WFI, single-step/interrupt ordering, MPRV on resume, illegal guest debug/trigger CSRs, lost LR reservation, and halted time. The `s6_ebreak` and `s6_debug_csr` guests exercise the public route. |
| 2. Debug Module | [DM](../../src/dm/mod.rs), [abstract commands](../../src/dm/abstract_cmd.rs), and [U2 tests](../../tests/s6_u2_debug_module.rs) cover discovery, command errors/W1C, GPR/CSR WARL, physical-memory access, native PLIC claim consumption, misalignment, hart selection, and sticky reset status. |
| 3. Machine integration | [Admission](../../src/machine/owned.rs), [lifecycle](../../src/machine/mod.rs), [U3 tests](../../tests/s6_u3_machine.rs), and [Machine tests](../../src/machine/tests.rs) cover coherent cross-thread halt, reset, WFI and flat RAM. [Native Runner tests](../../src/executor_u3_tests.rs) additionally read CLINT `mtime` through abstract memory and verify all four halted counts. Reset regressions are described below. |
| 4. Transport and CLI | [JTAG](../../src/jtag/mod.rs), [session](../../src/jtag/session.rs), and [U4 tests](../../tests/s6_u4_remote_bitbang.rs) cover reset IDCODE, unknown-IR BYPASS, DTMCS fields, DM version, sticky DMI error/reset, invalid flag combinations, and native/flat public execution. |
| 5. OpenOCD/GDB/Spike | [Attach script](../../scripts/debug_attach.sh), [GDB script](../../scripts/debug/s6-u5-session.gdb), and the preserved [original U5 record](stage6-exit.md) cover register/CSR reads, step, software breakpoint, writes, resume/halt, reset halt, guest exit/disconnect, named Spike comparisons, and version-4/datacount-2 negatives. The 5c CI reruns these tool checks. |
| 6. Old debugger removed | `src/debug/` and its [crate exports](../../src/lib.rs) are absent. The cumulative source review checked the removed definitions; the all-target build/test/Clippy gate and CI passed. `ExceptionCause::Breakpoint` remains a separate trap variant. |
| 7. A10 boundary | `git diff a8851c4ac57500eb54dadc89919cd7314b626594 5c1d031b2a66bc17d058b5a7355d88ebe80eb652 -- tools/a10` is empty. The public oracle remains unchanged and unsealed. |
| 8. Quality gate | Full local gate and applicable CI passed at 5c as recorded above. Coverage was skipped. |
| 9. Independent review | The initial cumulative review requested changes at 433; the independent 5c code review approved the corrections. A later head requires fresh applicable verification and independent review. |

U1–U4 negative controls passed at both 433 and 5c: original source passed, the intended assertion failed under mutation, and restored source passed. The retained [mutation cases and patches](../../scripts/debug/stage6-mutations/cases.json) reproduce `dpc=PC+4`, `datacount=2`, mid-instruction DMI admission, and incorrect reset IR. U5 has its tool-level negatives. U6 is a deletion check and has no behavior mutation.

## Reset defects and regressions

The initial cumulative review covered Hart/CSR, DM/Machine, and transport/tooling/deletion. It found two combinations absent from the green baseline tests:

- An unresolved abstract store reported `cmderr=7`, but `ndmreset` bypassed the Machine's uncertainty boundary. `abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse` failed with `ndmreset bypassed an unresolved abstract memory completion`. At 5c, DMI work publishes unresolved-completion facts before replying; reset, resume and lifecycle reuse remain blocked, including after a late backend completion. A separate in-flight test checks Busy before uncertainty is reported.
- Releasing `ndmreset` while selecting nonexistent hart 1 lost hart 0's halt request. `ndmreset_keeps_hart_zero_halt_request_when_selection_changes` failed with `hart 0 lost its halt request when reset released with hart 1 selected`, actual `haltsum0 & 1 = 0`, expected `1`. At 5c, the DM owns the persistent request across reconstruction. Tests also cover explicit clears, ordinary reset/install, managed request writers, and poisoned-lock publication failure.

Run these commands at 654 for the original failures or 5c for the corrected behavior:

```bash
cargo test --all-features --offline --lib \
  machine::tests::abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse \
  -- --exact --nocapture
cargo test --all-features --offline --test s6_u3_machine \
  ndmreset_keeps_hart_zero_halt_request_when_selection_changes \
  -- --exact --nocapture
```

## Rerun verification

With Rust dependencies and the RISC-V GNU toolchain available, run the normal quality gate and fresh guest suite from the repository root:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
scripts/run_elf_tests.sh
```

Replay the U1–U4 controls with Python 3.12 or later, Git, GNU patch, Rust, cached Cargo dependencies, and the recorded source commit available:

```bash
python3 scripts/verify_stage6_mutations.py
```

The default source is 5c. `--source-ref` selects another committed source; `--unit u1` through `--unit u4` select individual controls. The runner extracts source into temporary storage under `target/stage6-mutations/`, never patches the checkout, and requires original pass, intended assertion failure, and restored pass. Compilation failure, timeout, zero matching tests, or an unexpected assertion fails verification. Generated logs and reports remain under `target/`.

Run `scripts/debug_attach.sh --require-spike` in the pinned development image for U5. [The original U5 record](stage6-exit.md) specifies OpenOCD 0.12.0, the recorded GDB versions, Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`, its compatibility patch, and allowed comparison differences. Debug Specification 1.0 is pinned to `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`; the ISA Manual is document version 20250508.

## Limits and remaining decisions

This is one RV64IMA/Zicsr M/S/U hart with physical abstract memory access, `datacount=4`, `progbufsize=0`, no implicit EBREAK, SBA, triggers, or instruction execution in Debug Mode. FENCE.I and DRET remain illegal. Flat RAM does not prove native PLIC effects. The U5 script observes target exit/disconnect and then stops GDB/OpenOCD; GDB does not return from its final continue. The original U5 statement that the old debugger was still present applies to that checkpoint only.

This assessment claims no full ISA/Debug certification, new ACT4 result, Linux/SBI, second hart, or performance-certification completion. The A10 oracle remains unsealed. PR #107 must merge before its fixes can be described as merged-main behavior.

The [program-buffer successor proposal](../proposals/post-stage6-program-buffer-debug.md) remains Draft and informational. Its D1–D9 choices, successor direction, required accepted ADR amendments, code/document merges, and formal Stage 6 completion remain owner decisions. This change archives no contract and activates no successor.
