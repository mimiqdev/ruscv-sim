# Stage 4 round 10 progress

**Status:** Implementation progress on `feat/stage4-sv39-9`. Not a substitute for the milestone contract.

**Base:** `main` at `f964bfa` (PR #84). The implementation commit is `67ae539`. The commit after it changes only this note, to record the A10 smoke.

## Audit against `docs/dev-plan.md`

| Item | Before this round | This round |
| --- | --- | --- |
| §4 SFENCE.VMA, §7 item 4 | `s4_mapped_rw` executed the four forms with translation still working, but never wrote a new PTE before a fence, and no public-route guest ran SFENCE.VMA in U or another SYSTEM funct3 = 0 encoding. The U-mode case existed only as a core driver test. | New guest `s4_sfence_vma`. |
| §4 privilege transition | `s4_sret_vectored` covered `mret` to S, SRET from S to U, SRET in U, SRET clearing MPRV and setting SPIE, and SEPC with low bits written. It did not cover `mret` to U, SRET from M to S, or ECALL from U and from S. | New guest `s4_privilege_transition`. |
| §4 bare sanity | Present. `fib` in `tools/a10/public-v1.json` is `tests/bare-metal-riscv-test/rv64i/fib.S` linked with `linker.ld`, with `satp` left at its reset value 0. A10 P0 compares its full instruction-effect trace on every public route and mode. | No change. |
| §7 item 2 driver test | Superseded in round 11; see below. Present, split over two files. `tests/s4_sv39_translation.rs` covers the Hart side on the validated ports: walk reads are `DataRead` requests on the data port, separate from the guest access (`walk_reads_are_data_reads_on_the_data_port_distinct_from_the_access`); a target rejection is an access fault of the original kind; host, protocol, and unknown are `SimulatorFailure`; and invalid, reserved, misaligned-superpage, and non-canonical cases are page faults. `tests/sv39_ad_atomic_update.rs` covers the A/D race on the walker the Hart uses: a leaf that changes between the walk read and the conditional write gets no write and is re-read (step 2), and a host write between the two is never overwritten. | No change in round 10. Consolidated in round 11. |
| §3 "no unsupported legal instruction" | The decoder never returns `DecodeError::UnimplementedInstruction`, so the `SimulatorFailureKind::UnsupportedLegalInstruction` arm in `src/core/mod.rs` cannot be reached. No test proved it. | New sweep test `tests/s4_no_simulator_failure.rs`. The variant stays, because removing a public enum variant is a breaking change D7 does not record. |
| A10 nits | The `s4_tvm.S` comment said `0x188`, and also listed MIE, but the code writes `0x188 << 4 = 0x1880`, which is MPIE plus MPP = M. The report field `nontrapping_witness` was also used for the trap-inclusive witnesses. | Comment fixed, field renamed, oracle resealed. |

## New guests

Both are assembled fresh by their route tests and run on the CLI child, `load_and_run`, and `RiscVSimulator`.

`s4_sfence_vma` (in `tests/s4_sv39_guests.rs`). S mode runs on an identity 1 GiB superpage with U = 0, so it can rewrite its own leaf PTEs with ordinary stores. The guest does the following in order:

- It remaps VA `0x3000` between two physical pages. Before each of the four forms (`x0,x0`; `x0,rs2`; `rs1,x0`; `rs1,rs2`) it writes a new leaf and then loads through it. After form 4 it stores through the new mapping, which must land in the new page and leave the old page untouched.
- It turns a leaf that was 0 (VA `0x4000`) into a valid leaf, fences by VA, and loads through it.
- It runs `sfence.vma` with a non-canonical `rs1`, which raises no exception.
- It runs HFENCE.VVMA (`0x22000073`) in S, which traps illegal-instruction with `mtval = 0x22000073`. With H absent this encoding is reserved, so the trap is asserted as consistency with Spike and Sail, not as a specification requirement (open question 1).
- It runs SFENCE.VMA in U, which is illegal-instruction with `mtval = 0x12000073`, and checks that the trap came from U.

The rewritten leaf is also visible before the fence. The guest checks that once and records it as a property of this cache-less model (M4), not as an architectural guarantee.

`s4_privilege_transition` (in `tests/s4_profile_guests.rs`), in Bare mode. The handler checks `mcause`, `mtval`, and `mepc` exactly.

- `mret` to U. U-mode `csrr sstatus` is illegal with `mtval` equal to the instruction. ECALL is cause 8.
- `mret` to S. `csrr sstatus` retires, and `csrr mstatus` is illegal. ECALL is cause 9.
- SRET from M to S. The guest first sets MPRV, SPP = S, and SIE = 1 with SPIE = 0, and writes `sepc` with bits 1:0 = 3. `sepc` reads back with bits 1:0 clear, and SRET lands there. In S, `sstatus` shows SPP = 0, SPIE = 1, and SIE = 0. The next trap shows that MPRV is clear.
- SRET from S to U, with `sepc` bit 0 written as 1. The first U instruction is an illegal `csrr sstatus` at the masked address. The trap-entry `mstatus` shows SIE = 0, SPIE = 1, SPP = 0, and MPP = U.
- SRET in U is illegal with `mtval = 0x10200073`. SIE, SPIE, SPP, and the privilege do not change, and the following ECALL is still cause 8.

The handler's mismatch exit code is `(mcause << 16 | mtval) << 8 | 0x7f`. The low byte is never 0, so the CLI process status fails even when the low byte of `mtval` is 0, as it is for ECALL. The first draft used the older `mcause << 16 | mtval` encoding. A negative mutation then showed that the CLI process exited 0 for a wrong ECALL cause. The library and flat routes in the same test would still have failed, because they assert `exit_code == 0`.

### Negative mutations

Each mutation below was assembled from the guest source with one changed expectation and run on the CLI in the pinned image. Every one exited nonzero.

| Mutation | Exit |
| --- | --- |
| ECALL from S expected as cause 8 | 127 |
| ECALL from U expected as cause 9 | 127 |
| SRET M to S expects SIE = 1 | 1 |
| Expected `mepc` for the S-to-U landing moved off the masked address | 62 |
| MPRV expected set after SRET | 1 |
| SFENCE pre-fence visibility expects the old page | 1 |
| SFENCE newly valid leaf expects the wrong page | 1 |
| HFENCE.VVMA `mtval` expected as the SFENCE.VMA bits | 127 |
| U-mode SFENCE.VMA `mtval` expected as HFENCE.VVMA bits | 127 |
| Store through the new mapping expected in the old page | 1 |
| HFENCE.VVMA expected `mepc` moved to the next label (after review) | 62 |
| HFENCE.VVMA trap expected from MPP = M instead of S (after review) | 1 |
| Load after the `rs2 = 0xffff` fences expects the old page (after review) | 1 |

## No simulator failure for any encoding

`tests/s4_no_simulator_failure.rs` steps one instruction from reset in M, S, and U on a Hart with the validated physical ports, with every GPR pointing into RAM. It covers three sets:

- every major opcode × funct3 × funct7 × six register-field patterns, one of which reaches WFI (589,824 steps)
- every CSR address × the six Zicsr funct3 values × `rs1` of x0 and x1 (147,456 steps)
- 100,000 xorshift32 encodings (300,000 steps)

No step is a `SimulatorFailure`, and both retirements and traps occur in every set. A debug build takes about 5 seconds.

## A10

- `tools/a10/fixtures/s4_tvm.S`: the comment now reads `0x188 << 4 = 0x1880: MPIE | MPP = M`. Only `source_sha256` changes. The ELF is byte-identical.
- The report evidence field `nontrapping_witness` is now `oracle_witness`. It is written and checked only in `tools/a10/report_schema.py`, where the checker rebuilds the evidence and compares it. The JSON schema types `evidence` as an open object, so the schema bytes do not change. `witness_method()` is documented as covering both the no-trap fixtures and the trap-inclusive ones. The N/A provenance text "exact linked no-trap path" is now "exact linked oracle path (oracle_witness.method)". `docs/verification/a10-perf-schema.md` describes the field.
- `derive_oracles.py`: `--extend` rejects a change to an existing fixture, so it cannot reseal `s4_tvm`. The no-flag mode rederives the whole oracle and replaces this platform's producer pins from `build.json`. It now also rewrites the `suite-v1.json` pin, which before only `--extend` did. The pin is defined as the oracle file's digest, so every write keeps them equal.
- The reseal ran in the pinned image `ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2…` on fresh fixtures (`build_fixtures.py --out target/a10-r10-fixtures-1`). The only oracle diff is `s4_tvm.source_sha256`. The x86_64 producer pins came out identical. `--check-artifacts` and strict `--check` both match. New oracle and suite pin: `411933d75f0e546825f3ad9b1a51a8edaf649ae096098ad0d803d0712021b426`.
- `python3 -m unittest discover -s tools/a10 -p 'test_*.py'`: 29 run, OK, 1 skipped.

### A10 smoke

The smoke ran in the pinned image on a clean tree at `67ae539`, the implementation commit, using the default 2 basic repetitions plus one warmup. The process exited 2, which is the harness rule that a valid smoke report is still not a performance pass. Report and logs stay under `target/` and are not committed. Harness output:

```text
P0 artifact-equivalence/oracle audit: matches; NOT pinned-toolchain evidence (no simulator invoked)
INCONCLUSIVE: valid ruscv-perf/1 evidence at /workspace/target/perf/r10-smoke/report.json; 1086 ordered rows. P3 calibration/baseline/ratios and P4 retention deferred; NO performance PASS
```

`semantic_status` is `correct`, `comparison_status` is `inconclusive`, and diagnostics are empty. 684 rows are `correct` and 402 are `not_applicable`. There are 228 aggregates. Each of `s4_csr_fields`, `s4_ssip`, and `s4_tvm` has 48 correct rows and 26 explicit N/A rows, the same as round 9. The report's only oracle digest is `411933d7…b426`, which matches `suite-v1.json`. Every evidence entry carries `oracle_witness`, and none carries `nontrapping_witness`. `source.head` is `67ae539` and `source.clean` is true.

`./scripts/perf-test.sh validate target/perf/r10-smoke/report.json` reported `Strict schema/integrity/own-P0 status 0; NEVER a speed/regression PASS`. The checker rebuilds every evidence entry, including `oracle_witness`, and compares it with the report.

## Gate

`tools/a10/replay.rs` on `main` at `f964bfa` was not rustfmt-clean, so `cargo fmt --all -- --check` failed before any change of this round. The pinned image's rustfmt 1.9.0 (`8bab26f4f6`) and the host's (`88d9e12ae1`) agree. This round applies `cargo fmt` to it. The change is whitespace only, in two places.

The gate ran in the pinned image with `--cpus=4`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, a separate container target, and `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`. One cargo invocation at a time:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features -- --test-threads=4
cargo doc --all-features --no-deps
```

Result on the implementation commit's tree: exit 0, wall time 268 seconds. `cargo test` reported 1954 passed, 0 failed, 0 ignored, summed across 69 test binaries. Round 9 recorded 1951. The difference is the three `#[test]` functions in the new sweep binary. The new guests run inside the two existing guest tests. The log has no toolchain skip line.

`./scripts/run_elf_tests.sh` with a fresh output directory: 75 of 75 fresh ELF cases passed on the CLI, including both new guests.

## Open questions

1. **HFENCE.VVMA as the "other SYSTEM funct3 = 0 encoding".** **Resolved** by Dragonite's spec ruling (https://app.notion.com/p/3f837873fb548147b58df2dfd974e9d8). The premise above was wrong: HFENCE.VVMA without H is no more required to trap than SFENCE.VMA with `rd ≠ 0`. Both are reserved encodings. With `misa.H` = 0 the H opcodes become reserved (machine chapter, `misa`, and its NOTE), the SFENCE.VMA encoding fixes rd = 0 (supervisor chapter, "Supervisor Memory-Management Fence Instruction"), and reserved behavior is unspecified (unprivileged RV32I chapter). Spike and Sail both raise illegal-instruction for them, so ruscv-sim keeps the trap. The guest, `sfence_all_forms_privilege_tvm_and_reserved_fields`, the code comments, and dev-plan §3 and D8 now assert it as consistency with those reference models, not as a specification requirement. Behavior is unchanged.
2. **"The same driver test" in §7 item 2.** **Resolved** by Tony: consolidate into one Hart-level driver test with the A/D race checks going through the Hart's ports. The statement above that the component walker "is the walker the Hart uses" was wrong. The Hart walks in `src/core/translation.rs` (`RiscvCore::walk` and `update_leaf_ad`), not with `mmu::sv39::PageTableWalker`, so `tests/sv39_ad_atomic_update.rs` never covered the Hart's A/D update. See "Round 11" below.
3. **SRET clearing MPRV from S.** **Resolved** by Dragonite's spec ruling (https://app.notion.com/p/3f837873fb548147b58df2dfd974e9d8): keep the M-to-S SRET demonstration, which is valid under the privileged specification's xRET rule (machine chapter, "Privilege and Global Interrupt-Enable Stack in mstatus", and the MPRV description). Round 11 adds the MRET-with-MPP = M control. A guest cannot observe SRET from S clearing MPRV, also because `sstatus` does not expose MPRV (supervisor chapter, `sstatus`). MPRV is always 0 while the hart is in S or U, because every way into those modes leaves it 0: MRET and SRET to a lower mode clear it, and a trap into S from U would find it already 0. That holds for any delegation setting, not only on this profile. The M-to-S SRET case does observe the clearing.

The same gate on the note-only commit also exited 0: wall time 246 seconds, with 1954 passed, 0 failed, and 0 ignored across 69 binaries.

## Independent review

A separate read-only agent reviewed PR #86 at `0004561` against `f964bfa`. It found no blocker. It checked the `mstatus` and PTE constants, the five claimed encodings, the Sv39 table and `satp` construction, and the SRET semantics against `src/isa/rv64i/system.rs`. It confirmed that `UnimplementedInstruction` is never constructed and that the sweep is not vacuous. It also confirmed that the only `public-v1.json` change is `s4_tvm.source_sha256`, which equals the SHA-256 of the edited source, and that the suite pin equals the oracle file's digest. It did not re-run the gate, the ELF set, or the smoke.

| Finding | Severity | Disposition |
| --- | --- | --- |
| The deterministic sweep never reached WFI (rs2 = 5) | should-fix | Added the `(rd, rs1, rs2) = (0, 0, 5)` pattern. The count is now 589,824. |
| Case D comments said "sepc bits 1:0 written as 1", but the code wrote only bit 0 | nit | Case D now writes `+3`, matching case C and the comments. |
| The `s4_sfence_vma` handler did not check `mepc`, and nothing checked that HFENCE.VVMA trapped from S | nit | The handler now compares `mepc` with `s0`, which is the HFENCE.VVMA address or VA `0x2000`. `after_hfence` checks that the trap-entry MPP is 1. |
| The ASID forms used only rs2 = 0 | nit | Added `sfence.vma x0, t3` and `sfence.vma s4, t3` with `t3 = 0xffff`, followed by a load through the mapping. |
| Open question 3 gave the wrong reason | nit | Reworded above. |

The fixes are in the commit after `0004561`. On that tree, the same container gate exited 0: wall time 249 seconds, with 1954 passed, 0 failed, and 0 ignored across 69 binaries, and no skip line. `run_elf_tests.sh` with a fresh output directory passed 75 of 75. The fixes change only test guests, the sweep, and this note, and the A10 oracle, witnesses, and harness are unchanged, so the A10 smoke was not repeated.

## Round 11

Follows the open-question rulings above. Behavior of the simulator is unchanged.

- **Reserved encodings (question 1).** SFENCE.VMA with `rd ≠ 0` and HFENCE.VVMA without H stay illegal-instruction traps. `docs/dev-plan.md` §3 and D8, the comments at `RiscvCore::is_sfence_vma` and in `src/isa/rv64i/system.rs`, `tests/s4_instruction_profile.rs`, `tests/s4_sv39_guests.rs`, and `s4_sfence_vma.S` now label that trap as consistency with Spike and Sail, not as a specification requirement. The `s4_instruction_profile` comment also corrects the bit-26 flip: it gives funct7 `0001011`, an unassigned encoding, not HFENCE.VVMA.
- **MPRV (question 3).** `s4_privilege_transition` keeps case C: in M with MPRV = 1, SRET to S, ECALL back to M, and the trap-entry `mstatus` has MPRV = 0. New case C0 is the control: MRET with MPP = M keeps MPRV = 1, because an xRET clears MPRV only when the new privilege is below M. The guest reads `mstatus` before any load or store, since MRET leaves MPP = U, and then clears MPRV.
- **§7 item 2 (question 2).** `tests/s4_sv39_translation.rs::sv39_walk_reads_d3_outcomes_and_ad_race_on_the_hart_ports` is now the single driver test. It builds a `RiscvCore` with separate traced fetch and data ports (`DriverBackend` over one `SimpleMemory`). The data port can fail one issued address and can run a host write before or after the Hart's walk-private load-reserved. It replaces seven tests: `walk_reads_are_data_reads_on_the_data_port_distinct_from_the_access`, `walk_read_target_rejection_is_an_access_fault_of_the_original_kind`, `port_rejected_walk_read_is_an_access_fault_of_the_original_kind`, `walk_read_host_protocol_and_unknown_are_simulator_failures`, `invalid_reserved_and_pointer_only_walks_page_fault`, `misaligned_superpage_page_faults_instead_of_translating`, and `noncanonical_va_is_a_page_fault_of_the_original_kind`. It shows:
  - Every walk read is a `DataRead` on the data port. The fetch port carries only the one fetch. The guest load is a separate `DataRead` after both walks.
  - Target rejection of the walk read is an access fault of the original kind, for a load, a store, and a fetch. A root below the RAM base is rejected before any port request.
  - Host, protocol, and unknown completions are `SimulatorFailure`, and `mcause` stays 0.
  - An invalid leaf, reserved PTE bits 63, 61, and 54, a level-0 pointer, a misaligned megapage, and a non-canonical VA are page faults. The non-canonical load issues no walk read.
  - A/D race on the Hart's port. (a) A host write between the walk read and the load-reserved: the load-reserved sees the new PTE, no store-conditional is issued, and the walk resumes at step 2 with that value. (b) A host remap between the load-reserved and the store-conditional: the store-conditional fails and writes nothing, the walk re-reads the PTE with a `DataRead`, and the load uses the new page. (c) A host invalidation between the two, for a load and a store: the PTE keeps the host's bytes, and the access takes a page fault of its own kind.

  `tests/sv39_ad_atomic_update.rs` stays as component coverage of `mmu::sv39::PageTableWalker`. It is not §7 item 2 evidence.

### Round 11 negative mutations

Each mutation was applied to the implementation, run in the pinned image, and then reverted.

| Mutation | Result |
| --- | --- |
| A failed A/D store-conditional treated as committed (`src/core/translation.rs`) | `sv39_walk_reads_d3_outcomes_and_ad_race_on_the_hart_ports` fails (race case b: the load reads the stale page) |
| Load-reserved value compare removed, so the store-conditional writes the checked PTE over a changed one | Same test fails (race case a: the stale PTE overwrites the host write) |
| MRET clears MPRV even when MPP = M (`src/isa/rv64i/system.rs`) | `s4_profile_guests` fails in `s4_privilege_transition` case C0 |

### Round 11 gate

The gate ran in the pinned image with `--cpus=4`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`, a separate container target, and `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`. It ran the same five commands, one at a time. Result: exit 0, wall time 255 seconds. `cargo test` reported 1948 passed, 0 failed, 0 ignored across 69 binaries. Round 10 recorded 1954. The difference is seven removed tests and one added test in `tests/s4_sv39_translation.rs`. The log has no toolchain skip line.

`./scripts/run_elf_tests.sh` with a fresh output directory reported 75 of 75. **That count overstates the S4 guests.** The script links every guest with the default linker script, not `linker-s4.ld`. Eight Sv39 and profile guests then stop after about 8 cycles with simulator `Status: FAILED` and exit code `0x10000400`: `s4_ad_observation`, `s4_delegation`, `s4_mapped_rw`, `s4_mprv`, `s4_permission_fault`, `s4_sfence_vma`, `s4_sret_vectored`, and `s4_walk_fault`. The low byte of that exit code is 0, so the CLI process exits 0, and the script counts each as a pass. The same applied to the round-10 claim above that `run_elf_tests.sh` passed "both new guests": `s4_sfence_vma` was among these false passes. This round's changes did not cause it. The script is unchanged, and `s4_sfence_vma.S` changed only in comments. The evidence for these guests is the route tests `s4_sv39_guests` and `s4_profile_guests`. They link with `linker-s4.ld`, run each guest on the CLI, `load_and_run`, and `RiscVSimulator`, and assert exit code 0. Both pass. Fixing `run_elf_tests.sh` is left as a separate item.
