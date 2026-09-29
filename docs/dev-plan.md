# Development Plan

**Current milestone on merge:** A9 — Hart Facts and Safe N=1 Machine Lifecycle (post-A7 roadmap Stage 2)

**Status:** Approved for activation on merge. This detailed A9 contract and documentation-only rotation were explicitly approved for merge on 2026-09-29, subject to required checks and independent review. Until the rotation merges, A8 remains the sole Current contract on `main`; at merge, this file becomes the sole Current technical contract. This approval covers the plan and rotation only: no A9 Rust implementation is included or claimed.

**Planning baseline:** A8 implementation merge `4c4d0a295f620ed85557b10475d46bef1be5ea17` and A8 bounded closeout assessment merge `902a4288e2dd8838f2a402fb54d9eab8ce440cf4` (2026-09-29). See the [archived A8 contract](archive/milestones/a8-single-hart-atomic-physical-convergence.md) and its [acceptance/evidence record](verification/a8-closeout-assessment.md). Evidence for source behavior is bound to the cited revisions and tests, never inferred from the roadmap. This plan does not retroactively certify RV64A, full ADR-0002, or a real VP board.

**Direction and boundary:** One existing RISC-V Hart semantic engine, composed with the already supported native SystemBus or flat RAM configuration as an N=1 Machine. Give the Machine a real ownership/lifecycle boundary and the Runner reliable Hart/Platform facts without prematurely selecting a new board, scheduler, device, guest OS, or external transport. This is Stage 2 of the [post-A7 technical roadmap](proposals/post-a7-roadmap.md), not a new overall product strategy. Stage 3's separately selected performance-test infrastructure remains a later candidate and is not delivered here.

## 1. Objective and accepted architecture

Make one Hart transition's control facts authoritative for both standard public facades, replace Runner reconstruction of architectural effects with subscriber-gated Hart facts, and compose the current native/flat physical configurations behind a safe single-Hart Machine lifecycle. Machine owns the Hart–Platform association, image installation/reset coordination, and quiesce/drain; Runner still owns user policy and terminal presentation. This milestone does **not** introduce a second ISA engine or turn independently tested MMU/TLM/debug components into public integration claims.

The accepted architecture remains unchanged:

- [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md) owns precise `InstructionRetired`/`TrapEntered`/`SimulatorFailure` semantics, Hart-originated commit/trap facts, optional materialization, and observer non-reentrancy. A failed operation cannot fabricate a commit or trap.
- [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md) owns the physical transaction and fault taxonomy. A8 established the single-Hart native atomic slice, not MMU page-walk/A-D or TLM integration. A9 preserves its one storage/atomic domain, target-vs-host failures, and terminal unknown completion.
- [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md) owns Runner/Machine/Hart/Platform separation, image metadata versus installation, reset, and public compatibility. This milestone implements those boundaries only for the two existing N=1 configurations.
- [ADR-0004](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md) owns non-lossy boundary facts and `DrainComplete` versus `NoProgress`. A9 must not relabel an instruction count as virtual time or `mcycle`, or implement an interrupt/WFI scheduler under a lifecycle name.

An implementation mechanism (Rust type, construction order, fact buffer, lock/generation arrangement) is not fixed by these ADRs or this plan. Any proposed API or observable compatibility change beyond the matrix in §4 requires a separate decision; do not infer authorization from a task name.

## 2. Verified starting point and evidence limits

At the planning baseline, `src/core/mod.rs::RiscvCore::step_outcome` already returns typed retired/trap/failure outcomes, and A8 uses a validated physical port for fetch, ordinary data and native AMO/LR/SC on standard facades. `src/executor.rs::install_image_with_physical_ports` shares the underlying RAM/device objects, but `load_and_run` and `RiscVSimulator::{run,step}` still organize separate public workflows. Native commit logging uses the fetched instruction rather than a re-fetch, yet Runner-side GPR snapshots and omitted memory effects do not meet ADR-0001's Hart-owned complete observation boundary. `RiscVSimulator::memory()` exposes a cloneable shared handle, including possible cross-thread host writes during a run; `run` returning does not end the Hart lifetime. An A7/A8 unknown-completion record is terminal until genuinely resolved, not a normal reset switch. See `tests/a7_public_equivalence.rs`, `tests/a8_hart_atomic.rs`, `tests/a8_public_atomic_equivalence.rs`, and the [A8 closeout assessment](verification/a8-closeout-assessment.md) for the bounded implemented/evidence identity.

Existing `src/mmu/`, `src/tlm/`, `src/peripherals/` CLINT/PLIC and `src/debug/` components are not by themselves integrated public Machine capabilities. The frozen ACT4 51-case selection is adapted nontrapping RV64I evidence at its recorded heads; the 58 project-authored guests include seven A8 atomic programs and are a separate set. Neither set is an ISA-extension certification or proof of this future milestone.

## 3. Scope, ownership, and required semantics

### 3.1 Hart facts and observation

A completed Hart transition always returns sufficient **control facts** for progress, boundary PC, architectural outcome, and downstream stop/accounting decisions. When subscribed, the Hart materializes precisely one immutable commit fact for each retired instruction or one trap fact for a completed trap entry, from the same transition that applied the state change. At minimum those facts distinguish instruction bytes/length and before/after PC, before/after privilege, GPR/FPR/CSR/counter changes where applicable, memory and atomic effects of the instruction, and trap cause/value/saved PC/entry effects. `rd=x0` and other suppressed writes are not reported as register writes. A completed conditional SC failure is a retired instruction with no committed memory write, but reports the architectural `rd=1` change when `rd != x0`. A faulting SC does not retire or write `rd`; an interrupt (not integrated by A9) cannot be fabricated by the observer.

With observation off, architectural outcomes, counters, Platform effects, and terminal decisions are identical; no per-instruction materialized record, serialization, or Runner callback is required to maintain control facts. With observation on, facts are ordered, non-speculative, immutable at delivery, and never delivered by an in-flight instruction or re-entrantly into Hart execution. A sink failure after a completed transition is an outer reporting failure, not rollback, a different Hart trap, or a second retirement. The Runner may format the existing Spike-style commit log, but must not reconstruct its architectural contents by register snapshots or opcode re-fetch. Existing log format and public library interfaces remain stable unless an explicit compatibility decision authorizes a change.

### 3.2 Machine/Platform and lifecycle

An N=1 Machine composes the **same** Hart semantics with the current native SystemBus/RAM/UART/HTIF configuration or existing flat RAM configuration. Platform owns routing and device events; Machine wires ports and owns coherent inspection and lifecycle; Runner owns ELF-loading orchestration, observation demand, budgets, stop classification, result and artifact assembly. ELF parsing describes segments, zero-fill, entry, `.signature` and `tohost` metadata; the Machine coordinates installation and the Platform performs placement. Image-base/flat-storage conversion does not become virtual translation. There is one active Platform storage/atomic domain per composition, not a copied second RAM.

Lifecycle is construct/configure → install → reset → run/control → inspect → quiesce/drain → teardown. Installed image identity and bytes, static configuration and connections survive a promised fresh reset; dynamic Hart state, reservations, device queues/registers, pending exit and observation state return to the documented fresh starting state. A legacy `RiscvCore::reset` that only changes the core must not be advertised as a fresh Machine reset. Replacing an image must not expose old RAM, tohost, callbacks or reservations to the new composition.

Installation, image replacement, fresh reset, debug/control mutation if exposed, and teardown are **illegal while work is in flight**. `QuiesceRequested` stops new turns, drains started Hart/physical/callback/observation work, and returns causal facts before `DrainComplete`. NoProgress, return from `run`, timeout, or clearing an error flag is **not** proof of drain. After unknown physical completion, the Machine must retain uncertainty and refuse unsafe reset/reuse/drain until an adapter establishes no late effect is possible. Proven drain permits lifecycle mutation, not retrospective retirement/trap or proof that uncertain prior state was correct; a fresh reset must restore the promised initial image or be refused. No force-reset shortcut is part of this milestone.

Host writes that A8 admits through `write_mem`, `memory()` handles, and `clear_tohost` remain real writers in the same storage/version domain; a `&mut self` Runner borrow does not serialize cross-thread handle clones. A9 must either prove coordinated isolation/fencing of every such path during lifecycle mutation (including stale handle generations), or refuse a claimed fresh lifecycle operation while it cannot guarantee safety. Do not silently disable an existing admitted host writer, claim `DrainComplete` from a mutex acquisition alone, or allow an old handle to mutate a reconstructed fresh Machine. The implementation choice is open, but both the positive and negative cases must be tested.

### 3.3 Runner control and compatibility

Machine reports **all applicable** completed-turn, trap/failure, Platform-exit, limit, observer-error and lifecycle facts supported in this N=1 configuration without choosing one presented stop reason. Runner applies existing public result and budget policies. A guest tohost write retires before PlatformExit is presented; a rejected physical/atomic write cannot exit. Zero budget starts no turn; a started failed turn consumes its existing budget slot but is not reported as a retired instruction. No virtual clock, new deadline API, timer consumer, or interrupt source is inferred from those facts.

Both standard facades use the same Hart/Machine semantic path, while keeping their documented native-device versus flat-memory and artifact differences. Public `load_and_run`/CLI, `RiscVSimulator`, `ExecutionResult`, signature/tohost priority, default and configured budgets, `--log-commits`, guest/host error distinction, and reusable run/step behavior remain observable compatibility obligations. The typed-only `RiscvCore::new` constructor remains a labeled non-conforming compatibility route, not an alternate Machine architecture or certified physical target.

## 4. Compatibility ledger and non-goals

| Surface | Baseline | A9 contract |
| --- | --- | --- |
| CLI and `load_and_run` | Native RAM/UART/HTIF ELF run, selected tohost, signatures, budgets and optional commit log | Same user-visible command/result semantics; Machine composes the existing objects and returns facts, Runner still classifies exit. No unapproved CLI/result format change. |
| Flat `RiscVSimulator` | Flat storage-offset configuration with resumable `run`/`step`, image replacement and host `memory()` handles | Preserve address and artifact differences, writer visibility and resumability. Provide a safe fresh lifecycle operation distinct from any core-only reset; reject unsafe mutation rather than pretending all outstanding handles are quiescent. |
| Observation | Hart retirement/trap outcome exists, native log still reconstructs partial GPR effects | Hart-originated subscribed commit/trap facts include memory/atomic/CSR/FPR context; disabled observation has the same semantics without per-instruction Runner reconstruction or materialization. Retain existing log presentation. |
| Stop/failure | A6 started-slot budgets and A8 terminal unknown completion | Preserve zero/exact/final-slot outcomes and retire-before-exit; expose non-lossy facts; unknown completion never becomes a guest trap, retry or fresh reset by fiat. |
| Old typed constructor | Usable explicitly non-conforming atomic compatibility route | Keep it usable and labeled. Its component tests are not public Machine integration evidence. |

Non-goals: new board or configurable platform topology; multi-Hart execution, DMA/coherence/RVWMO; MMU/Sv39/PMP integration, page-table walks/A-D writes on the port; asynchronous interrupts, WFI/time/virtual clock/counter timing changes; public Debug Mode/GDB wiring or breakpoint mutation; SystemC/TLM/DMI/FFI; block/JIT execution; checkpoint serialization; new ISA/profile certification; a performance facility or optimization (the separate Stage 3); force reset or silent compatibility break; amendment of ADR-0001–0004.

## 5. Dependencies, deliverables, and executable acceptance

The following task names and test files are **proposed deliverables**, not claims that they exist or pass at activation. Narrow checks run as each increment is built; all criteria must hold on a final reviewed, committed implementation head. Unavailable or skipped required tools block acceptance rather than becoming passes.

| Task | Deliverable and falsifiable exit |
| --- | --- |
| T0 — baseline and route inventory | Record current public Runner/Hart fact generation, opcode/log construction, both facade loops, image/reset/reuse paths, callback ordering, cloneable host writers, and unknown-completion behavior from source and focused tests. Add characterization for snapshot-only memory/atomic omissions, failure/exit coincidence, reset/reload and cross-thread handles before behavioral migration. Preserve a frozen baseline distinct from proposed expectations. |
| T1 — Hart control/observation facts, after T0 | Implement one Hart-owned fact builder at the completed transition. Tests cover GPR/FPR/CSR/counter and ordinary/atomic/memory effects, trap entry including cause/original `mtval`/saved PC/privilege, conditional SC failure (no memory-write fact; `rd=1` register fact iff `rd != x0`), x0, branch and exit-causing store. Spy/allocator checks show observer-off state parity and no per-instruction record construction or Runner callback. Enabled sink failure cannot unretire; no opcode re-fetch, snapshots, mid-transition callbacks or speculative records. Focused A6/A8 regressions remain green. |
| T2 — N=1 Machine composition/lifecycle, after T1 | Compose existing native and flat Platform variants with one Hart engine and physical domain; separate image metadata from installation, coordinate image/reset/device state, and return coherent inspection. Lifecycle tests exercise install→reset→run→inspect→drain→fresh reset, image replacement, old-handle isolation, cross-thread writer/mutation exclusion, failed drain and unknown-after-possible-effect with no retry/late callback; `NoProgress` never grants mutation. Expose no advertised safe reset unless it restores image-owned RAM and devices and proves drain. |
| T3 — Runner/facade migration, after T2 | Move both standard facades onto the composed Machine and consume unclassified control and optional observation facts. Public CLI/`load_and_run`/flat equivalence, A4/A6/A7/A8 ordinary/atomic/exit/guest-trap/zero and final-slot budgets, signature/artifact/log, image reload and resumable runs remain green within documented configuration differences. Exit-causing instruction retires before PlatformExit; observer failure is separately reported. Audit that neither facade retains an independent ISA execution or architectural snapshot/refetch path. |
| T4 — evidence and bounded acceptance, after T0–T3 | Full gate below; fresh-build/run all **58 project-authored** ELFs including seven A8 atomic guests, separate from the frozen historical ACT4 51-case RV64I selection. Record source/toolchain/image identities, exact commands and per-case results, public behavior and negative/lifecycle evidence, residual debt and explicit non-claims. Obtain independent review at exact committed PR head. No Stage 3 capability or full VP claim. |

Full local quality gate:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a9-fresh-riscv-elves ./scripts/run_elf_tests.sh
```

When the host lacks the cross-toolchain, use the repository development image documented in [the development environment](development-environment.md), a separate container Cargo target and fresh ELF directory, and `RISCV_REQUIRE_RISCV_TOOLCHAIN=1`. Existing ELF files or a local status-77 skip cannot count as a fresh pass. `run_elf_tests.sh` performs its own rebuild; preserve the exact source/head and per-case identity. The frozen ACT4 selection and its recorded results remain separate nontrapping RV64I evidence, not a requirement to generate a new selection or an A9 Hart-observation/Machine certification. An additional external conformance selection requires its own approved scope.

Final acceptance requires source and tests for each row, a clean committed implementation HEAD with relevant checks, applicable PR CI and independent review of that HEAD, and a bounded closeout record. A later documentation-only SHA must not be described as an unrun runtime test. Consider the milestone finished only after its acceptance and the approved rolling replacement; do not silently inherit A9 debt into a successor.

## 6. Activation and navigation

The A8 [archive record](archive/milestones/a8-single-hart-atomic-physical-convergence.md) preserves its full approved scope; the [A8 closeout assessment](verification/a8-closeout-assessment.md) holds its 2026-09-24 implementation evidence, separate ACT4 result and limitations. The A8 implementation merge is `4c4d0a295f620ed85557b10475d46bef1be5ea17` (PR #65); the closeout assessment merged as `902a4288e2dd8838f2a402fb54d9eab8ce440cf4` (PR #66) on 2026-09-29. This detailed A9 contract and documentation-only rotation were explicitly approved for merge on 2026-09-29, subject to required checks and independent review. A8 remains Current on `main` until merge; when merged, A9 becomes the sole Current contract. This rotation starts no Rust implementation and schedules no later milestone; Git history records the merge identity.

The [post-A7 roadmap](proposals/post-a7-roadmap.md) supplies Stage 2 technical planning context, not a second active contract; Stages 3–8 remain separately approved candidates. The target [ISS→VP architecture](architecture/README.md) and accepted ADRs remain authoritative across plans; use source and exact-head verification for integration claims, not diagrams or component availability.
