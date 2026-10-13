# Development Plan

**Current milestone on rotation merge:** A11 — Sv39 Virtual-Memory End-to-End Integration

**Status:** Current accepted target — **approved for activation on merge**, 2026-10-10 UTC, by user selection of functional advancement. Exactly one successor contract; not an implementation-completeness claim. Until this documentation-only rotation merges, `main` retains A10 as Current. On merge A10 is formally archived and this A11 contract becomes sole Current. Merge authorization remains separate.

**Authority:** Normative milestone contract, constrained by accepted [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md) and [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md). ADR-0004 implementation is explicitly out of scope; existing lifecycle/fact boundaries are preserved, not overridden. The [informational drafting snapshot](proposals/a11-sv39-integration.md) is not a second contract.

**Approved:** 2026-10-10 UTC — bounded Sv39 functional advancement, not interrupts/time scheduling, performance certification, a new board or API break. This rotation changes documentation only; P0–P4 are subsequent scoped implementation deliverables after activation.

**Planning/evidence baseline:** `5d23d337a6732d178a2e14445be9f5c28154f270`, squash-merged A10 PR #72. The [complete A10 archive and completion record](archive/milestones/a10-performance-baseline-infrastructure.md) preserve the right-sizing amendment, final branch-head checks/review, merge-head CI and separate development baseline identities. A10's informational single-session costs do not prove translation support. No Rust, guest, MMU or ACT4 pass is attributed to this rotation's document SHA.

## 1. Approved objective and architecture

Wire the existing Sv39 walker and TLB components in `src/mmu/` into the **one Hart execution path**. In an explicitly enabled Sv39-capable Machine profile, architectural `satp` mode and effective privilege govern fetch/load/store translation; the Hart issues physical PTE reads, successful A/D updates and final accesses through ADR-0002's validated `PhysicalAccess` domain. Provide precise original-VA faults, Hart-local translation caching and guest `sfence.vma` semantics. Prove execution of page-table-bearing ELF fixtures through the public Machine boundary, not just direct translator calls.

Ownership remains fixed:

- **Hart/MMU:** effective virtual address, privilege/alignment/permission checks, `satp`, translation/TLB, original access-kind fault mapping, retirement and completed observations (ADR-0001).
- **PhysicalAccess/Platform:** physical raw-byte routing and transaction-level all-or-nothing effects; physical target rejection is distinct from host/protocol/unknown failure (ADR-0002). A PTE operation is physical, not recursively translated.
- **Machine:** profile selection, Hart/Platform connection, coherent image placement/inspection/reset and lifecycle (ADR-0003). Runner owns outer results, limits and observation delivery; it neither translates nor implements another engine.

This is functional integration of one bounded capability. It is not full privileged-ISA, supervisor OS, MMU/PMP, ADR or external compliance certification.

## 2. Entry evidence and integration gaps

Source inventory at the planning baseline, **implemented components/seams, not Sv39 public-path support**:

| Actual seam | Existing source/tests | Limit relevant to A11 |
| --- | --- | --- |
| Hart outcome and optional facts | [`RiscvCore::step_outcome`, `step_transition`, `execute_turn`](../src/core/mod.rs), [A9 Hart facts](../tests/a9_hart_facts.rs), [A6 trap regression](../tests/a6_task3_core_trap_test.rs) | Raw fetch uses the architectural PC directly; ordinary/atomic adapters use the untransformed effective address. No Hart-owned MMU is called in this path. |
| Physical transaction/fault taxonomy | [`PhysicalAccess`, `ValidatedPhysicalAccess`, `PhysicalDataAccess`](../src/physical.rs), [physical contract](../tests/a7_physical_contract.rs), [Hart physical tests](../tests/a7_hart_physical.rs), [atomic Hart tests](../tests/a8_hart_atomic.rs) | Existing fetch/data ports and atomic envelopes are integration seams; page walks/A-D writes are not yet connected. Keep one composed storage/version domain, not a copied MMU RAM. |
| Sv39 walk and A/D behavior | [`Mmu::translate`](../src/mmu/mod.rs), [`PageTableWalker`](../src/mmu/sv39.rs), [`AddressTranslator`](../src/mmu/translator.rs), [Sv39 tests](../tests/sv39_test.rs), [A/D tests](../tests/ad_bits_test.rs) | Walks use separate [`PhysicalMemoryInterface`](../src/mmu/physical.rs). Read/write errors collapse to component `WalkResult::AccessFault`; this cannot preserve the ADR host/unknown taxonomy without integration work. Permission handling is simplified; `mstatus` is supplied but not consumed by the translator. |
| TLB storage and flush helpers | [`Tlb`](../src/mmu/tlb.rs), `Mmu::flush_tlb`, [TLB tests](../tests/tlb_test.rs) | Direct helpers are not guest `sfence.vma`. Audit VA versus VPN units, global/ASID rules, superpage-offset construction, hit-path permissions and D-bit upgrades; component presence is not evidence these are architecturally correct end to end. |
| Architectural control and composition | [`supervisor::SATP` and CSR storage](../src/csr/mod.rs), [`SYSTEM` execution](../src/isa/rv64i/system.rs), [`MachineConfig`, `Machine::install/step/fresh_reset`](../src/machine/mod.rs), [facade](../tests/a9_facade_composition.rs) and [lifecycle](../tests/a9_machine_lifecycle.rs) tests | SATP storage exists, but no integrated `sfence.vma` dispatch/translation profile. Current public `MachineConfig` fields have no translation selection; extend compatibly, not by breaking existing struct-literal callers. |
| Public-route oracle practice | [A10 P0](../tests/a10_p0_oracles.rs), [manifest/validator](../tools/a10/mod.rs), [route adapters](../tools/a10/routes.rs) | Reuse own-run correctness checks, explicit N/A and negative mutations where sensible; existing nontrapping oracles do not prove page faults or VM. A11 does not enlarge the performance matrix. |

Existing component reproduction (no public VM claim):

```bash
cargo test --test sv39_test
cargo test --test tlb_test
cargo test --test ad_bits_test
cargo test --test a7_hart_physical
cargo test --test a9_hart_facts
cargo test --test a9_machine_lifecycle
```

These are executable entry checks, **not passes newly recorded by this document**. The Sv39/A-D tests exercise 4 KiB/2 MiB/1 GiB component walks, permission failures and hardware A/D updates; the TLB suite exercises direct lookup/flush/ASID/global behavior. They do not execute a page-table guest through Machine/CLI. The legacy translator's no-memory passthrough method must not be used as integration evidence. P0 must reproduce gaps with focused regressions before repairing them.

## 3. Selected scope and compatibility invariants

### 3.1 Profile, address meaning and permissions

- **Bare remains the default**, with byte-identical existing guest-visible results, RAM/signature/UART/log output and unchanged PC, budgets, counters, exit and error expectations. No expectation changes to existing guests/tests. Existing constructors and named public APIs remain usable; no mandatory new CLI option, result shape or public struct field. A compatible additive Machine configuration method/builder is an implementation choice, not authorization to break struct literals.
- A **Sv39-capable profile is opt-in at Machine composition**; `satp` mode Bare within that profile is still untranslated. Selecting capability does not itself turn on paging or perform image translation. In that profile implement architectural Bare/Sv39 mode selection and WARL behavior for unsupported modes; Sv48/Sv57 are not new capabilities. Characterize default-profile CSR behavior before any changes and preserve it.
- Effective privilege governs translation: Machine fetch stays untranslated; MPRV affects data, not instruction fetch. S/U accesses apply Sv39 `satp` rules, U/R/W/X permissions, SUM/MXR and relevant privilege restrictions on `satp`/`sfence.vma` (including TVM). Implement only the privileged-control changes needed for this VM slice, not a general supervisor/interrupt migration.
- Cover canonical addresses, invalid/reserved PTE encodings, non-leaf legality, superpage alignment and correct 4 KiB/2 MiB/1 GiB offsets, on both miss and hit paths. Existing fixed IALIGN=32 and admitted widths remain; architectural misalignment is checked **before translation**, so it starts no PTE/final physical request. No compressed fetch or misaligned decomposition is added.
- ELF installation and signature inspection remain host-side physical placement/inspection, not guest translation. Page-table fixtures explicitly describe physical segment/root placement and virtual execution addresses. Flat storage offsets and native RAM relocation are backend adaptations, never a second virtual mapping. The entry boot stub establishes page tables/`satp` and enters the translated privilege path using the same Hart.

### 3.2 A/D and caching profile

The selected A11 profile uses **hardware-managed A/D updates**, matching the existing component direction, not a second fault-on-required-update profile. ADR-0001/0002 do not themselves select this scheme. Tests cover all four initial A/D combinations for fetch/load/store, cold and warm TLB paths: reads/fetches set A, stores set A and D, unrelated PTE bits are preserved, and no write occurs when the required bits are already set. Permission-invalid accesses cannot be made valid merely by updating A/D.

PTE updates are separate physical transactions. A successful A/D write remains visible if the later fetch/load/store/AMO target faults; it is **not** rolled back with Hart staging or reported as a retired memory effect of the faulting instruction. Failed PTE writes must not partially change memory. Updates must not silently clobber competing admitted host writes; preserve the existing physical-domain serialization/reservation visibility contract. Unknown completion remains quarantined without retry/forced drain. Alternative A/D schemes and concurrent multi-Hart PTE arbitration are non-goals.

Each Hart owns its own TLB(s), never a process-global cache. Permissions/effective privilege and A/D obligations must be correct on hits as well as misses. A11 permits spec-minimal **ASIDLEN=0** in the enabled profile; no nonzero-ASID capacity/performance target. State/readback and fence tests must match the implemented minimal profile, not imply tagged multi-address-space support from component tests. Conservative over-invalidation is allowed by `sfence.vma`; selective retention is not an acceptance requirement.

Guest `sfence.vma` must order prior PTE writes against subsequent translations and invalidate the required instruction/data mappings for its rs1/rs2 operands, including nonzero registers containing zero, global mappings, superpage spans and the declared ASID profile. After a fence, subsequent accesses must see changed mapping/permissions/A-D obligations. No test requires a stale mapping before a fence (implementation may flush more). `satp` root/mode transitions cannot reuse an unrelated cached mapping; fresh reset/image replacement clears translation caches and restores profile reset state. No leak across separate Machine instances or fresh runs.

### 3.3 Existing atomic route

Do not leave AMO/LR/SC as a virtual-address bypass of the new data path. In the enabled profile, apply the same required translation/permissions and ADR fault matrix while retaining indivisible physical atomic envelopes and Hart-owned reservation/results. LR uses load classification; SC/AMO use store/AMO classification. Check W/D aliasing, sign extension, original-VA faults and unchanged destination/memory on rejected attempts. Preserve the existing single-Hart reservation profile; no new coherence or RVWMO certification. Ordinary successful PTE writes are real physical-domain writes, not invisible to reservation coordination.

### 3.4 Exclusions

No interrupts, WFI wake behavior, virtual-time/clock/delay scheduler, CLINT/PLIC integration or ADR-0004 implementation; multi-Hart, DMA/coherence, ASIDs beyond spec-minimal, PMP integration, Sv48/Sv57, hypervisor/nested translation, OS/Linux/new board, debug/GDB/DMI/TLM wiring, block/JIT or performance work, A10 cohort/comparison certification, API breaks, releases/tags or new ACT4 selection/execution. Existing lifecycle/unknown-completion and native/flat artifact differences are preserved. No old backlog is automatically inherited.

## 4. Fault and observation acceptance matrix

The ADR-0002 physical taxonomy feeds the ADR-0001 original-access mapping:

| Original architectural access | Translation condition: noncanonical/invalid/reserved/permission | Target rejection at any PTE read, required A/D write or final translated access |
| --- | --- | --- |
| Fetch | Instruction page fault (12) | Instruction access fault (1) |
| Load / LR | Load page fault (13) | Load access fault (5) |
| Store / SC / AMO | Store/AMO page fault (15) | Store/AMO access fault (7) |

For each address fault **`mtval` is the original faulting VA**, not the physical target/PTE address, including full-width upper canonical VAs. Saved PC is the faulting instruction PC; fetch faults need no invented fetched opcode. If an existing delegated trap route applies, its `stval` has the same original-VA rule; no broad supervisor trap certification is implied. Physical/PTE/span/backend context stays diagnostic only.

Every injected PTE-read/A-D-write/final-access case also distinguishes host/backend/protocol/malformed/unknown failure: those return **SimulatorFailure**, no fabricated access/page trap, guest CSR mutation or completed record. Unknown-effect domains refuse later retry/reuse/reset/drain under the existing lifecycle contract. A target/device rejection is a guest-visible access fault, not SimulatorFailure. Architectural misalignment has its own cause and issues no physical request.

Falsifiable assertions per case: exact outcome/cause/value/saved PC/handler PC/privilege and `minstret` delta, untouched destination/x0 and instruction memory effects on faults, exact PTE traffic and successful A/D visibility, raw diagnostic physical addresses, and no failed target side effects. Faulting attempts do not retire. Observations off/on yield the same control/state/results; on emits exactly one completed trap or commit and no commit for a fault. Successful translated tohost/MMIO writes retire before outer exit; failed writes emit no exit. Runner logging must not re-fetch a VA through a physical port or reconstruct architectural effects.

## 5. Staged deliverables and executable exits

The `a11_*` filenames/commands below are **future deliverables**, not existing tests or proof of implementation. Implementation may choose ordinary Rust internals; changing this scope or an accepted ADR requires a separate decision. Each stage records focused failures before repair and exact committed-head verification after repair.

| Stage / dependency | Deliverable and falsifiable executable exit |
| --- | --- |
| **P0 — inventory and frozen baseline** | Record fetch/data/atomic/CSR/Machine/reset routes from source; run existing component and A6–A10 regressions, distinguish required-toolchain runs from skips. Add `tests/a11_vm_baseline.rs` with default-profile exact state/result/bytes/log characterization and failing gap reproducers for port-walk taxonomy, VA/VPN fences, superpages, hit permissions/D upgrade. Pin the enabled profile's hardware A/D, ASIDLEN=0, WARL and privilege rules to a named/versioned RISC-V privileged specification in repository documentation. Exit: `cargo test --test a11_vm_baseline`; the baseline cases pass without changing any old expectation, and each separately recorded gap reproducer demonstrates the targeted missing behavior before migration. |
| **P1 — Hart translation path**, after P0 | Adapt the walker to the composed physical port without losing target/host/unknown distinctions; integrate fetch and all admitted ordinary/FP/atomic data accesses, `satp`/effective privilege and per-Hart TLB/fence dispatch into the existing staged transition. No raw typed-memory bypass or copied page-table RAM. Exit: `cargo test --test a11_hart_translation` plus `cargo test --test a11_tlb_sfence`; spy ports prove physical PTE widths/addresses, no recursive walks, correct PA effects and original VA state, all page sizes, SUM/MXR/MPRV, mode/root transitions, fence operands/global/superpage coverage, permission/A-D hit parity and independent Hart caches. Default bare makes no PTE access. |
| **P2 — faults, A/D and observations**, after P1 | Implement the complete §4 matrix with injected physical targets and all initial A/D combinations; preserve successful A/D writes across later faults without retired-effect leakage. Exit: `cargo test --test a11_vm_fault_matrix` and `cargo test --test a11_ad_visibility`; table-driven tests cover each PTE walk level, required update and final fetch/load/store/atomic request, target versus host/protocol/unknown responses, original full VA/trap state, no partial GPR/CSR/PC/store/device changes, non-retirement, off/on parity and no retry after uncertainty. Negative oracle mutations of cause/VA/retirement/PTE bytes must fail. |
| **P3 — Machine/public ELF enablement**, after P2 | Expose a backward-compatible opt-in Machine configuration seam. Add source/linker/page-table-bearing guest fixtures and a fresh isolated build/identity pipeline under repository-owned A11 paths, no stale ELF reuse. Exit: `RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a11_vm_public_routes`; native and flat public Machine runs execute boot→Sv39 fetch/load/store→trap-handler/signature/exit and map VA≠PA, identity mapping, fence-remap/permission changes and A/D outcomes with own-run oracles. Companion component runs cannot bless missing public results. Compare bare versus identity-mapped useful-work architecture/bytes/output, accounting explicitly for boot/control instructions and profile-only PTE effects. Include observation off/on, translated atomic and native MMIO/exit tests, drain→fresh reset→rerun and stale-handle safety. CLI/native/flat convenience APIs remain tested unchanged in default bare; no VM CLI support is claimed unless separately wired and verified through this same engine with a compatible seam. |
| **P4 — closeout evidence**, after P3 | Add a bounded repository verification/closeout page mapping every criterion to actual source/tests, exact head/tree, commands/environment, fresh fixture hashes, expected/observed counts and limitations. Exit: full Rust gate in §6, required-toolchain public VM and existing guest regressions, existing A10 correctness/schema regressions, unchanged-expectation/scope/diff audit, applicable PR CI and independent read-only review at the final committed PR head. No component-only, skipped or old-head pass satisfies public VM acceptance. A11 closes only after authorized merge and required repository evidence; future successor selection/rotation is separate. |

Fixture oracles are derived from pinned assembly/page tables, not generated by recording the simulator's output as expected data. Declare exact traps/retirements/PC/signatures/PTE bytes and relevant registers/memory/device effects. Reuse A10's public-route oracle style and negative mutations, not its nontrapping `turns=retirements` assumption or timing certification. Each route validates its own exported evidence; unsupported native-device/flat cells are explicit N/A, not parity claims.

## 6. Verification and evidence rules

Run the full local Rust quality gate on the final implementation head:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

Run the focused staged suites above, retain all existing Sv39/TLB/A-D component regressions, A6 trap/ELF, A7 physical, A8 atomic, A9 facts/lifecycle/facade and A10 P0–P2 oracle/phase/schema tests. With required cross tools, fresh-build/run the existing project guests using a new disposable output:

```bash
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
RISCV_TEST_OUTDIR=target/a11-existing-guests ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a11-existing-guests ./scripts/run_elf_tests.sh
./scripts/test_riscv_elf_guards.sh
git diff --check 5d23d337a6732d178a2e14445be9f5c28154f270..HEAD
```

These commands refer to repository files, not private execution systems. Use a fresh output name for each verification session; do not overwrite retained evidence. The 58 existing project guests and frozen ACT4 selection are distinct sets; a new ACT4 run is not required. P3 adds its own VM fixtures and required-toolchain test, not expectation edits to the 58 guests. Full Rust process success when cross-tools are skipped cannot substitute for actual VM/ELF execution.

For this **documentation-only rotation**, the bounded gate is fmt, all-features check, no-dependency all-features docs and the fixed-base diff check. It does not establish a VM runtime pass. Implementation closeout requires the larger gate above, coherent committed source, exact-head PR verification and independent review. Record head/tree, clean state, commands, tool/environment/fixture identities and result/skip counts; content-tree equality is not execution, and a changed head requires fresh evidence/review. Required missing tools, unavailable evidence or an inconclusive matrix cell block acceptance; no semantic waiver or optimistic completeness claim.

## 7. Risks and activation boundary

Principal risks: simplified component permissions or superpage/TLB behavior mistaken for spec correctness; collapsed PTE error taxonomy; original VA lost in physical adapters; D updates skipped on hits; successful PTE effects accidentally rolled back or logged as retired stores; page-table RAM diverging from the composed domain; lock/re-entrancy and unknown-completion safety; and additive configuration accidentally breaking public struct construction. Port spies, fault injection, same-run public oracles, warm/cold hit tests and unchanged bare expectations are mandatory controls.

Accepted direction is Sv39 integration into the current N=1 Hart/Machine/Platform, hardware A/D updates, spec-minimal ASID and no public compatibility break. Ordinary internal adapters/builders/cache layouts do not require new approval. A material privileged-profile/API change outside this boundary, new board, scheduler/interrupt work or performance mandate requires a separate decision. Do not silently change scope to fix unrelated old debt.

This contract activates on the actual documentation rotation merge; implementation begins in subsequent scoped work. Approval of the contract is not approval to merge, release or claim that Sv39 is already runnable. [A10's archive](archive/milestones/a10-performance-baseline-infrastructure.md) remains a standalone historical record, never rewritten to match A11. The rotation's actual SHA/PR/date belong in Git/PR provenance, not invented future identifiers.
