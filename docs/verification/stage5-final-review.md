# Stage 5 final review

**Status:** Recorded outcome of the §7 item 9 review.

**Reviewed tree:** `main` at `fb8399b20ee1d6b44bc6da157c3cc3f18b784474`, the Stage 5 U5 merge (PR #94). The implementation merges on that history are U1 PR #90 `379ff8c`, U2 PR #91 `7fb0043`, U3 PR #92 `364375c`, and U4 PR #93 `59f70a5`. This pull request adds the review record and the check 9 update. It does not change instruction behavior.

**Authority:** This is the review record. It is not a substitute for the [Stage 5 contract](../dev-plan.md). Formal completion of the milestone is the Stage 5 to Stage 6 rotation merge, not this document.

**Method:** Read-only comparison of the public Hart and platform path with the Stage 5 contract and with the RISC-V privileged specification (the 20250508 text the contract cites). The paths read were `src/core/mod.rs` (`execute_turn`, `accepted_interrupt`, counter updates, trap entry), `src/isa/rv64i/system.rs` (WFI legality), `src/machine/mod.rs` (pre-fetch sample, `mtime` tick, `settle_wait`), `src/peripherals/plic.rs` (`claim_interrupt`, `read_guest_word`), and the Stage 5 guests and drivers named below. No Spike or Sail lock-step was run in this review. This review did not re-execute the suite. Execution evidence is the green Quality-and-tests run on the `main` push of `fb8399b`. Where the specification leaves the behavior unspecified, this record does not invent a requirement.

## Verdict

No public-path defect was found that violates the privileged specification or the Stage 5 contract's controlling rules. The code follows the 2026-10-18 owner reading of D3, not the earlier option A bullet that the same decision still contains. Findings F1 through F3 are wording and citation gaps. None of them is an instruction-behavior defect. This pull request does not fix them in simulator code.

## What was checked

| Area | Result |
| --- | --- |
| U1 clock and counters | The native CLINT window is at `0x0200_0000`. Reset `mtime` is 0 and reset `mtimecmp` is all-ones. `time` (`0xC01`) is a read-only shadow of `mtime`. A completed turn advances `mtime` by one. A `Waiting` turn and a `SimulatorFailure` do not. A retired instruction increments `mcycle` and `minstret` unless that instruction wrote the counter. Trap entry increments `mcycle` only (`src/core/mod.rs`). `s5_counters` is the guest. |
| U2 software and timer | MSIP follows `msip` bit 0. MTIP is the unsigned compare `mtime >= mtimecmp`. Both bits are read-only in `mip`. MSIE, MTIE, and MEIE are writable and reset to 0. `accepted_interrupt` orders MEI, MSI, MTI, SEI, SSI, STI. A take is before fetch, in M only when MIE is 1, and below M with MIE clear. The interrupted instruction has not retired. `s5_software` and `s5_timer` are the guests. MSI before MTI is the driver in F2. |
| U3 external | Source 1 is the level at `0x4000_0000`. The machine context presents MEIP. Trap entry does not claim. `claim_interrupt` ignores the priority threshold. A claim returns the ID and clears pending. A complete ends service, and a level that is still high makes the source pending again. `s5_external` is the guest. The supervisor SEIP OR is the driver in F2. |
| U4 WFI | A legal WFI retires in M, and in S with TW clear (`src/isa/rv64i/system.rs`). U-mode WFI, and S-mode WFI with TW set, trap and do not wait. After a legal retirement, `wfi_wait` is set only when `mip & mie` is zero. The next sample wakes on `mip & mie`, then takes a takeable interrupt at the WFI address plus 4. A takeable interrupt that is already pending is taken before the WFI, and `mepc` is the WFI address (`src/core/mod.rs`). An idle jump sets `mtime` to the next `mtimecmp` when MTIE is set and the compare is still ahead. That distance is not added to `mcycle`, `minstret`, or `cycles`. No future event stops with `NoProgress`. A deadline before the compare stops with `DeadlineReached`. |
| U5 routes | The seven guests run on the native CLI and `load_and_run`. `scripts/run_elf_tests.sh` requires `s5_counters`, `s5_software`, `s5_timer`, `s5_external`, `s5_wfi_event`, `s5_wfi_masked`, and `s5_wfi_none`. Flat does not serve the CLINT or the PLIC. `s4_tvm_tsr_tw` keeps the illegal WFI traps and runs the S-mode wait on the native routes only. |
| Delegation | `medeleg` and `mideleg` stay 0. `sip` and `sie` stay 0. A taken machine interrupt enters M. |
| Oracle | `tools/a10/public-v1.json` is not rewritten. Stage 5 guests are not hash-locked. That matches D8 and the 2026-10-13 standing rule. |
| §7 items 1 through 8 | Item 1 is the seven guests, each with a negative mutation, plus the ELF suite. Item 2 is two drivers, which `docs/verification/stage5-exit.md` already states. Item 3 is `a_takeable_pending_interrupt_is_taken_before_wfi`. Item 4 is `s5_wfi_event` and `s5_wfi_none_stops_with_no_progress`. Item 5 is the single-step driver, the deadline driver, and `a_virtual_deadline_stop_reports_deadline_reached`. Item 6 is the Stage 4 suites with the recorded WFI and `time` changes. Item 7 is `s4_delegation` and `s4_supervisor_interrupt`. Item 8 is the Quality-and-tests citation in F3. |

## Findings

| Id | Severity | Location | Finding | Disposition |
| --- | --- | --- | --- | --- |
| F1 | open | `docs/dev-plan.md` D3 | The option A bullet still says that a locally enabled interrupt which is already pending does not wait, and that the following boundary takes it. The 2026-10-18 owner reading in the same decision, the WFI paragraph in §2, and the U4 row say a takeable pending interrupt is taken before the WFI, with `mepc` at the WFI address. `execute_turn` follows that later reading. | Not an instruction-behavior defect. The controlling rule is the 2026-10-18 reading. This review does not edit the contract. The bullet stays until a later contract edit. |
| F2 | open | `docs/dev-plan.md` U2, U3, and U4 | The unit rows name `s5_software`, `s5_timer`, `s5_external`, and the three WFI guests. Those guests do not show every fact on the same row. MSI before MTI is `msi_precedes_mti_when_both_are_pending` in `tests/s5_u2_interrupts.rs`. The supervisor SEIP OR is `supervisor_line_ors_into_seip_without_sticking_the_software_bit` in `tests/s5_u3_interrupts.rs`. `mret` onto a WFI is `mret_into_s_mode_traps_on_a_pending_wfi`, and `csrs mstatus` immediately before a WFI is `enabling_mie_directly_before_wfi_traps_on_the_wfi`, both in `tests/s5_u4_wfi.rs`. | The behaviors are implemented and tested. The guest column does not name those drivers. This review does not add tests. |
| F3 | open | `docs/verification/stage5-exit.md` check 8 | Check 8 cites Quality-and-tests at `962e475` (run 39168555285). That commit is not `fb8399b`. `git diff 962e475 fb8399b` changes only this exit file. The `main` push of `fb8399b` is green (run 39185396954). §7 item 8 asks for a green check at the implementation head. | The code bytes match. The citation is not the merged SHA. This review leaves the U5 sentence in place and records the merged run here. |

F1 through F3 stay open. There is no simulator-code change in this pull request, and there is no code fix to re-verify.

## Open questions

1. `docs/architecture/current-state.md` line 120 says CI runs the Stage 4 guests, `s5_counters`, `s5_software`, and `s5_timer`. Line 32 of that file, and `scripts/run_elf_tests.sh`, name all seven Stage 5 guests. The Quality-and-tests job runs that script. The sentence is stale. It is not a missing job, and it is outside the Stage 5 contract's required artifacts. This review does not edit that file.

## §7 item 9

Item 9 is this document. The reviewed head is `fb8399b`. Findings F1 through F3 are recorded above. They are not instruction-behavior defects, and they are not fixed on an implementation branch. The reserved FENCE `fm` question from the Stage 4 review is closed by Stage 5 D6 without a behavior change. That decision is not reopened here.
