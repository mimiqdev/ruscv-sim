# RV64GC Default Profile contract

**Status:** Historical. Completed 2026-11-04 at PR #171; the [blocking-transport contract](blocking-tlm-systemc-transport.md) succeeded it, completed 2026-11-05 at PR #186.

**Authority:** Preserved approved contract, not an active plan or an execution report. CI and PR history retain check and review outcomes.

**Original approval:** 2026-11-04. The owner selected "RV64GC as default" as the next milestone, followed by TLM/SystemC integration, under the delegation of full planning authority to the maintaining agent, who selected decisions D1–D10, all recommended choices. Activated by PR #161 (`6e83dd4`). The [proposal entry](../../proposals/post-fd-gc-default.md) is informational.

**Completion:** Units U4, U3, U2, U1 and U5 merged as PR #167, #169, #170, #168 and #171 (issues #165, #164, #163, #162 and #166), completed 2026-11-04 at merge head `d70f6d2`. U2 and U3 landed before U1 so that every profile-dependent route was explicitly pinned before the default flipped. The contract-rotation merge that activates the successor formally archives this contract; execution and review outcomes stay in CI and PR history.

**Known limitations:** `rv64imafdc` is the default for `run`, `MachineConfig::new` and both facades; the raw `RiscvCore` compatibility constructors and a standalone `CsrFile::new` stay `rv64ima`. There is no profile alias, ELF-based inference or writable `misa`. The A10 oracle stays bound to `rv64ima` and unsealed, and the Stage 3 performance-certification waiver is unchanged. External ISA evidence remains the frozen RV64I (51), Zca (32), F/D (196) and Zcd (4) ACT4 selections under their explicit profiles.

## 1. Objective and inherited profile

Make ordinary RV64GC toolchain output run without any option: `rv64imafdc` becomes the default ISA profile for `run`, `MachineConfig::new` and both library facades. Every existing profile stays selectable by name with unchanged behavior. All evidence whose expectations belong to a specific profile (the A10 oracle, the frozen ACT4 selections, profile-specific guests, Rust tests and debug sessions) is bound to that profile by an explicit selection, so no recorded expectation changes.

The inherited profile is everything delivered through the completed F/D contract: RV64IMA/Zicsr/Zifencei, Zca, F, D and Zcd as selectable profiles, M/S/U, the Stage 5 platform rules, Debug Specification 1.0 with program buffer and SBA, and zero-entry PMP plus Sv39. This contract adds no ISA extension, profile name or alias, writable `misa`, second hart, SBI, Linux target or TLM/SystemC integration.

The inherited Stage 3 performance-certification waiver continues. `tools/a10/public-v1.json`, its fixtures and their pinned build argv remain unchanged, and the oracle stays bound to `rv64ima`.

## 2. Selected decisions

References are unchanged from the archived F/D contract: ISA Manual 20250508, Debug Specification 1.0 at `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`, OpenOCD 0.12.0, Spike `5d99611782ef8dd81b65b842369bd351a9d4702a`, and riscv-arch-test `a7c99303516f4e668f7488f172043392e23b9dfd` with the existing A5 pins.

Every decision below was approved on 2026-11-04 under the delegation recorded above.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery | The default switch is the next single bounded milestone. TLM/SystemC integration is the owner-selected direction after it, under its own contract; nothing of it is carried into this one. |
| D2. Default | `IsaProfile::default()`, `MachineConfig::new`, `run --isa`'s default, and the native (`load_and_run`, `load_and_run_file` and their debug variants) and flat (`RiscVSimulator::new`) facades all select `rv64imafdc` (`misa` `0x800000000014112D`). The raw `RiscvCore` compatibility constructors (`new`, `new_with_memory`, `new_with_physical_access`, `new_with_physical_ports`) stay `rv64ima` by explicit selection, a standalone `CsrFile::new` keeps its IMA reset state, and every core's CSR state (`misa`, `epc_ialign32`, FP enablement) and debug capabilities match its profile from construction, not only after `reset()`. `rv64ima`, `rv64imac`, `rv64imafd` and `rv64imafdc` remain the only names, each with unchanged behavior when selected explicitly. No alias, no ELF inference, `misa` read-only. |
| D3. Pinning, not rewriting | A test, guest, fixture or session whose expectations depend on the profile (for example `misa`, FP absence, IALIGN=32 masks or compressed parcels being illegal) is bound to its profile by an explicit selection with its assertions unchanged. Changing an existing assertion is allowed only where it asserted the default itself, and each such change is named in the PR. |
| D4. Guest suite | Every guest declares its profile. Existing guests whose expectations are profile-independent run under the new default with no `--isa`. Profile-dependent guests (including `s4_fp_absent`, `s4_csr_policy` and the Zca/F/D guests) run with their explicit profile. Guest build flags are unchanged. |
| D5. A10 | Every A10 route selects `rv64ima` explicitly: the CLI route and the baseline/perf scripts pass `--isa rv64ima`; the `native-bytes`, `native-file`, `flat`, `machine-native` and `machine-flat` routes use the `*_with_isa`, `new_with_isa` or `MachineConfig` profile selection. The exact-argv check in `tools/a10/report_schema.py` is updated to the new pinned CLI argv. `tools/a10/public-v1.json` and its fixtures are byte-identical; the oracle is not re-derived and no new suite is added. |
| D6. ACT4 | Each frozen selection runs with its explicit profile, including the RV64I flow (`SELECTED_ISA` in `scripts/a5/experiment.sh`, `run_isa()` in `scripts/a5/accounting.py`) and the smoke path (`scripts/a5/run.py`): RV64I (51) under `rv64ima`, Zca (32) under `rv64imac`, F/D (196) under `rv64imafd`, Zcd (4) under `rv64imafdc`. Selections, inventories and replay validators are unchanged. |
| D7. Debug sessions | Every existing OpenOCD/GDB session passes its explicit `--isa` and keeps its Spike ISA string. The F/D session runs with no `--isa` to prove the default, still compared with Spike `--isa=rv64imafdc_zicsr`. |
| D8. Observation | Under the default, commit facts and the commit log follow the `rv64imafdc` rules already delivered. Under explicit `rv64ima` the log text is unchanged. |
| D9. Documentation | The top-level README, CLI help, CHANGELOG and verification docs state the new default and how to select `rv64ima`. Stale current-milestone statements in the top-level README are corrected. |
| D10. CI hygiene (owner wrap-up) | Split combined `bash -n a b` lines in `.github/workflows/ci.yml` into one check per file, since `bash -n` with several arguments parses only the first. Raise the `Quality and tests` timeout from 30 to 45 minutes; observed runs reach 22–26 minutes and the toolchain install alone has taken 17.5. Correct the stale flat 30-minute A5 bound in `docs/verification/a5-feasibility.md` to match the workflow. No other CI behavior changes. |

## 3. Ownership and semantics

The Machine owns the profile as composition configuration, as amended in ADR-0003. Only the default value changes; frontends and the Runner still forward an explicit selection when given one, and the Hart, Platform and DM rules for each profile are unchanged. The switch is a public default-behavior change, not an API removal: every public signature keeps its shape, and callers that need IMA behavior select it explicitly.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. Default switch | Change the default per D2. Prove through `run` with no `--isa`, `MachineConfig::new` and both facades that the default is `rv64imafdc`, that the raw-core constructors stay `rv64ima`, that every core's CSR state matches its profile at construction, and that each explicit name keeps its `misa` and behavior. Pin profile-dependent Rust tests per D3. The existing `zca-default-misa-c` and `fd-default-misa-f` controls already target explicit `rv64ima` and are retained. Update D9 docs. |
| U2. Guest, A10 and ACT4 binding | Per D4–D6: per-guest profile declarations in the guest suite, explicit `rv64ima` on every A10 route with the `report_schema.py` argv check updated and `public-v1.json` byte-identical, and explicit profiles for every ACT4 selection. Run all four ACT4 selections through the pinned flow on the PR head and record them in PR history. The fresh guest suite passes with each guest under its declared profile. |
| U3. Debug sessions | Per D7: explicit `--isa` on every existing session, and the F/D session under the default. `scripts/debug_attach.sh --require-spike` passes in the live debug-attach CI job with all existing named results unchanged. |
| U4. CI hygiene | Per D10, with each split `bash -n` check proven to parse its file (a deliberately broken file fails it). |
| U5. Exit | CI runs the full Rust quality gate, the guest suite, the tool script and the mutation runner. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Dependencies: U1 first. U2 and U3 require U1 and may proceed in parallel. U4 is independent and may proceed at any time. U5 requires all.

Required negative controls:

- the default profile remaining `rv64ima`;
- the CLI default diverging from `MachineConfig::new`;
- a raw `RiscvCore::new` core whose CSR `misa` disagrees with its profile before `reset()`;
- an A10 library or Machine route left on the default;
- a profile-pinned guest (for example `s4_fp_absent`) run under the default without detection, caught by a committed cargo test over the guest-suite profile declarations so the mutation runner can express it.

Explicit `rv64ima` advertising C or F is already covered by the retained `zca-default-misa-c` and `fd-default-misa-f` controls.

Keep their small patches, commands and expected assertions under the existing mutation-runner pattern. The standard gate and the no-ledger rule are unchanged.

## 5. Exclusions and retained direction

No new extension or profile, alias, writable `misa`, ELF inference, A10 oracle re-derivation or suite change, ACT4 selection change, second hart, Linux/SBI, TLM/SystemC/DMI integration, performance certification or new result field.

TLM/SystemC integration (post-A7 roadmap Stage 7) is the owner-selected next direction. Its contract must still define a concrete adapter fixture, the native/TLM parity proof and the external pins before activation.

## 6. Activation boundary

This documentation-only rotation preserves the entire F/D contract, applies the bounded 2026-11-04 ADR-0003 default-profile amendment and activates this target atomically on merge. It does not assert runtime support, start implementation, publish a release or seal an oracle.

## References

- [ADR-0001](../../architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](../../architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](../../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [Archived F/D contract](selectable-fd-floating-point-profiles.md), [post-A7 roadmap](../../proposals/post-a7-roadmap.md) and [default-profile proposal entry](../../proposals/post-fd-gc-default.md).
