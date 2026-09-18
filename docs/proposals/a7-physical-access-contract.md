# A7 Proposal — Unified Physical-Access Boundary

**Status:** Draft — not approved or activated

**Authority:** Informational, non-normative proposal. Approval to draft is not
approval of this contract or implementation. This document does not claim
implementation completeness and does not replace [dev-plan.md](../dev-plan.md).

**Evidence baseline:** `024d15d546dc3b711f593cd44bb107612fd8b600`, inspected
2026-09-18. Only documentation changes are proposed here.

## 1. Objective and authority

Converge the two public execution configurations on **one transport-neutral
physical transaction contract**, implementing the bounded public-path portion of
[ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md).
A common trait name alone is insufficient: fetch, integer and floating-point
data accesses, and atomic attempts must reach the same semantic boundary without
a fallback to the old typed-memory path. RAM, UART and HTIF remain targets, not
Hart semantics. This is not a proposal to activate MMU translation or to deliver
the entire future Machine composition.

[ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md)
owns outcomes, fault mapping and architectural effects;
[ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
owns placement/composition/Runner responsibilities;
[ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
owns budget/time/control boundaries. These accepted decisions constrain this
proposal; their historical Context sections are not a current implementation
inventory. [Documentation policy](../documentation-policy.md) and the unchanged
A6 active contract remain authoritative until a separate activation decision.

## 2. Starting point: retain A6, do not implement it twice

The [A6 assessment](../verification/a6-capability-assessment.md) and
[archive record](../archive/milestones/a6-closeout-record.md) distinguish merged
closeout preparation from the still-pending rolling-plan switch. PR #46 merged
as the baseline above. Historical test results retain their original revisions.

| Actual baseline source / focused regression | Implemented fact or remaining debt |
| --- | --- |
| `src/core/mod.rs::step_outcome`, `enter_trap`, `classify_execute_error`; `tests/a6_task3_core_trap_test.rs`, `tests/trap_test.rs` | Typed retirement/trap/failure outcomes, Machine-mode synchronous entry, pre-transaction alignment, staged Hart state and original-address `mtval` already exist. `step` is a compatibility wrapper. No interrupt sampling or MMU wiring. |
| `src/csr/mod.rs`, `src/isa/rv64i/system.rs`; `tests/csr_access_test.rs`, `tests/mret_conformance_test.rs` | MRET restoration/privilege checks, fixed C-disabled IALIGN=32, `mepc` masking, authoritative `minstret` and explicit-write precedence are A6 work to preserve. |
| `src/executor.rs::RunControl`, both public loops; `tests/a4_run_control.rs`, `tests/a4_integrated_equivalence.rs` | Shared started-slot limits, completed-turn reporting, exit decisions and installation already exist; no full Machine lifecycle abstraction. |
| `src/executor.rs::load_and_run`; `tests/public_behavior.rs::public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps` | Logging uses Hart-fetched opcode/privilege and only retired outcomes; no opcode re-fetch. Runner still snapshots GPRs for logger deltas, and passes `mem_access = None`. Full ADR-0001 observations are not implemented. |
| `src/memory/mod.rs`, `src/core/mod.rs::MemoryAdapter`, `src/isa/rv64i/{load,store}.rs`, `src/isa/rv64{f,d}/load_store.rs` | Typed reads/writes and load-extension helpers remain the active boundary. Fetch and data have separate handles. ELF-base subtraction is storage adaptation despite stale source comments calling it VA translation. |
| `src/executor.rs::SystemBus`; `tests/memory_bounds.rs`, `tests/executor.rs` | RAM validates complete typed spans; UART is byte-only in a 0x100-byte public window; HTIF is dword-only but checks only its starting address. RAM-first route precedence and device fallthrough are observable. |
| `src/isa/rv64a/{amo,lr_sc}.rs`, `src/execute/mod.rs::execute_amo`; `tests/amo_test.rs` | AMO helpers use ordinary word read then word write. LR/SC use a process-global reservation, not staged Hart state. Existing helper tests do not establish an atomic port, competing-write invalidation, or correct public A-extension decoding. See §5. |
| `src/mmu/physical.rs`, `src/mmu/sv39.rs`; `tests/ad_bits_test.rs`, `tests/translation_test.rs` | Separate byte/dword physical interface; walker collapses memory errors into `AccessFault`. A/D writes are separate transactions. Component only, not a ready public-path adapter. |
| `src/tlm/{traits,payload,bus,status}.rs`, `src/peripherals/uart16550.rs`; `tests/tlm_tests.rs` | Separate payload/status/delay/DMI vocabulary; no atomic envelope. UART TLM target consumes the first byte, unlike the public width checks. Core's optional TLM field is unused. Component presence is not adapter parity. |

## 3. Proposed scope and non-goals

In scope after approval: native synchronous single-Hart physical request/result
vocabulary, raw RAM and existing UART/HTIF adapters, Hart-side typed compatibility
helpers, both public facades, failure injection, migration characterization and
exact-head regression evidence. Concrete Rust signatures are design choices, not
prescribed here. Public names and callable APIs remain available, including
`MemoryInterface`, `SystemBus`, `RiscvCore`, `load_and_run`, `RiscVSimulator`,
inspection and stepping helpers. Compatibility wrappers must delegate to the
same semantic implementation rather than retain an independent execution path.

Not in scope: new ISA instructions or extension certification; MMU/Sv39/PMP
wiring; page faults; asynchronous interrupts, CLINT/PLIC, WFI scheduling;
SystemC/C++/FFI, TLM public execution or DMI acceleration; multiple Harts/DMA;
full Machine lifecycle, debugger composition, scheduler, virtual-time engine,
`mcycle` or new observation schemas. No automatic A8/A9 commitment follows.
Future integration directions are non-normative candidates only.

Atomic migration is an explicit **approval gate**, not an exception to ADR-0002.
The alternatives in §5 must be resolved before activation. A smaller milestone
may reject unavailable target capabilities, but cannot secretly preserve split
atomic execution under a “unified” name.

## 4. Proposed semantic contract

### 4.1 Request and ownership

One request identifies an explicit 64-bit **physical address**, nonempty width
(at least 1/2/4/8 bytes), semantic category (fetch, data read, data write, atomic),
and raw write bytes or an atomic operation envelope. A response matches that
request. Page-walk origin can be represented for future adapters, but no public
walk is enabled in A7. Host installation/inspection is not a synthetic Hart
access; it reuses target storage/range validation without retirement or traps.

* Byte `i` is at `paddr + i`. Fetch/read success returns exactly `width` bytes;
  write success has no invented read value. Integer encoding is explicitly
  little-endian, independent of host layout. Hart performs signed/unsigned load
  extension, narrowing, FP interpretation, decode and destination updates.
* Hart retains effective/original address, access stage, privilege checks,
  alignment policy, trap cause and retirement. Misaligned fetch/data attempts
  issue **zero** physical requests. Target width denial of an aligned access is
  not a Hart misalignment exception.
* The selected physical map validates the entire nonwrapping span before target
  invocation. Overflow, unmapped range, unsupported width/category, and a span
  crossing targets are target rejection; no splitting or width substitution.
  Zero width or mismatched request payload length is a caller/protocol failure,
  not a legal empty guest transaction. Empty host inspections remain valid.
* Fetch is explicit, not an ordinary data read guessed from width. Current
  4-byte RAM fetch behavior remains; UART/HTIF do not gain fetch support merely
  because a data method exists. Tests distinguish fetch/load/store fault causes.
* Hart is physically identity-addressed in these configurations. The backend
  adapter owns checked `paddr - ram_base` or image-base-to-storage-offset
  conversion. Guest PC and `mtval` remain guest addresses. Flat public
  `read_mem`/`write_mem`/`set_tohost` remain offset APIs; their compatibility
  conversion happens at the facade, not inside ISA code. Image placement still
  owns metadata resolution. No subtraction is MMU translation; no double
  subtraction is allowed. Nonaligned image bases need explicit characterization
  of legacy offset-alignment rejection before changing it (§6).

### 4.2 Results: no generic “memory error” shortcut

| Physical result | Boundary guarantee | Hart / Runner consequence |
| --- | --- | --- |
| Complete success | Exact bytes or complete write/atomic result; successful target side effects included | Hart completes the instruction normally when all architectural requirements succeed. |
| Guest target rejection | Valid request cannot complete: range overflow, absent/denied target, unsupported width/atomic capability, target-reported bus/device error; preserve physical span/category/width and target context | Hart selects cause 1 for fetch, 5 for load/LR, 7 for store/SC/AMO, preserving original-address `mtval`; no retirement. |
| Host/backend failure | Unavailable resource/backend, poisoned lock, host failure; not guest denial | `SimulatorFailure`, not guest CSRs/trap/commit; consume started slot, not completed turn or retirement. |
| Protocol/invariant failure | Wrong response length/category, malformed envelope, violated atomicity, contradictory completion | `SimulatorFailure`; never infer guest denial from error text. |
| Unknown completion | Effects cannot be established, especially after external/MMIO activity | Terminal simulator failure with uncertainty; no retry, fabricated exit, trap or commit. No reuse until resolution/restoration is safe. |

The categories must survive compatibility adapters even if existing public error
types remain. A legal but unimplemented **Hart instruction** is simulator failure;
a supported Hart operation rejected by a target for absent **physical capability**
is an access fault. These are not interchangeable ways of disabling RV64A.

### 4.3 Transaction side effects and accounting

Prevalidate routing, complete span, width, capability, payload and target
preconditions before mutation. Rejected writes leave all RAM bytes, device
registers/FIFOs, callbacks/output, exit signals and reservation-invalidation
notifications unchanged. Rejected reads must not consume RX bytes or clear
status bits. No failed multi-byte operation may emit a prefix of byte writes.
Known failures before mutation must have the same no-effect property.

Successful UART reads (RX dequeue/status clear) and writes/output, and successful
HTIF callbacks are committed target effects. They are **not rollbackable** by
cloning `CoreState`. Validate all remaining fallible architectural conditions
before such access; if a later host/protocol failure makes completion uncertain,
report simulator failure rather than pretending to restore the device. A backend
unable to promise failed-read-without-effect must reject before invocation or
report a complete successful read, never a false access fault after consumption.

Successful exit-causing stores retire before Runner reports exit; rejected writes
produce no exit event. Preserve A6 continuation and every trap boundary, `minstret`
write precedence, zero budget, recursive-trap bounds, last-slot failure (not
Timeout) and final-slot exit (wins over Timeout). Physical calls are not turns.
Optional delay metadata must remain transport-neutral; native A7 has no modeled
delay/time consumption. Adding delay consumers later requires ADR-0004's exactly
once accounting, not sleeps or changes to public `cycles`.

## 5. Atomic scope: mandatory decision before activation

Inspection reveals more than a missing bus method:

* AMO helpers read/write 32 bits even for D encodings. The public dispatcher
  maps `funct5=00001` to add and several other encodings incorrectly; dispatch
  presence is not A-extension correctness. Correcting encodings/widths is ISA
  work, not a transparent physical refactor.
* LR word/dword dispatch is reversed for the usual width encoding; SC fallback
  uses dword. `aq`/`rl` are read but not enforced. Global reservation is outside
  staged `CoreState`, not reset with a core, and ordinary stores do not notify it.
  Holding the outer data-memory mutex does not establish a domain-wide envelope
  or per-Hart ownership, especially with exposed RAM handles.
* `tests/amo_test.rs` primarily calls helpers directly; A6 atomic tests establish
  misalignment/unmapped cause and non-retirement boundaries, not successful
  atomic indivisibility or full extension support.

Every viable option requires an operation envelope at the **same port**. Hart
owns arithmetic/signedness, `aq`/`rl` interpretation, result extension, reservation
state and architectural SC result. Physical domain serializes an advertised AMO
or conditional SC indivisibly and supplies committed-write visibility. Neither a
platform-global reservation nor ordinary `read` + `write` is acceptable.

| Option awaiting maintainer selection | Real dependency / compatibility cost | Exit condition |
| --- | --- | --- |
| **A — capable native RAM atomic migration** | Requires a separately approved bounded ISA prerequisite for encoding/width defects and per-Hart reservation/profile rules, then atomic RAM serialization/visibility. UART/HTIF reject atomics before effects. Larger than a scalar-port refactor; cannot be called “no ISA work.” | Correct W/D envelopes; no read/write gap; successful/failed SC, same-Hart overlapping stores, reset and two independent simulator instances verified. Unsupported legal Hart operations remain simulator failures. |
| **B — bounded explicit capability rejection** | Native targets advertise no atomic capability initially. Recognized, correctly represented atomic attempts reach the port and receive target rejection, not ordinary SC failure. Existing successful AMO/LR/SC programs can now trap. Encoding/width characterization must identify attempts that cannot yet be represented correctly; those remain unsupported-Hart simulator failures, not invented capability faults. User approval of this behavior change is required. | Table of exact encodings/widths and old→new outcomes approved; no legacy atomic helper reachable from either public loop; target rejection has no side effects, cause 5 for LR and 7 for SC/AMO; no global reservation touched. Successful native atomic support is not claimed. Exported legacy atomic helpers also delegate/reject; they cannot retain a public split-access escape hatch. |
| **C — defer activation pending atomic prerequisite** | Preserve current runtime while a separate approved contract fixes Hart atomic semantics/reservations; then re-evaluate A7's size. No date or successor milestone is promised. | Prerequisite evidence reviewed and this proposal revised before activation; not an A7 implementation completion claim. |

No option is selected by this draft. B is not “fully compatible,” and A cannot
silently expand this documentation task into ISA implementation. If B is selected,
retain the envelope and explicit capability contract even without a capable
production target; test its rejection end to end. If A is selected, additionally
fix a profile/spec revision, reservation granule, normal/faulting SC consumption
rules and invalidation rules before implementation. A nonfaulting SC with invalid
reservation writes nothing and returns conditional failure for Hart interpretation;
missing physical capability must never masquerade as that conditional result.
Failed writes must not invalidate reservations; committed overlapping writes must
be visible to the Hart. No multi-Hart/DMA coherence claim is made.

## 6. Migration and compatibility ledger

| Surface | Proposed destination and preserved behavior | Approval / negative regression boundary |
| --- | --- | --- |
| Core fetch/data handles and `MemoryAdapter` | One physical semantic port, explicit categories; legacy constructor may wrap separate supplied handles in one category-aware compatibility adapter. Both public facades install one shared address domain. Typed helpers live on Hart side. | No public execution fallback to `read_*_sext` on a backend; no direct optional TLM route. A test spy must observe every fetch and data attempt at the port. Separate-handle compatibility is not a second public platform. |
| `MemoryInterface` and exported execution helpers | Keep signatures through compatibility delegation; raw native implementation is authoritative. Third-party legacy adapters need documented guarantees and cannot advertise atomics automatically. | Unknown backend semantics/partial writes cannot be normalized as clean guest faults. No default byte-loop implementation of a multi-byte guest write. |
| Native `SystemBus` | RAM-first **whole-span** match, then whole-span device candidate; preserve existing overlapping RAM/device fallthrough where one complete target accepts, including RAM size 4 at HTIF versus size 8. | Cross-target stitching forbidden. HTIF interior-address dword currently succeeds on start-only checks; rejecting spans beyond its 8-byte endpoint is an observable tightening requiring approval. Add direct API and guest tests; no callback on rejection. |
| UART / HTIF | Preserve UART public 0x100-byte window, byte-only access and current reserved-offset zero/ignored-write behavior; preserve HTIF base dword zero-read/callback-write. | Do not shrink UART to the TLM 8-byte map. Multi-byte UART rejection cannot dequeue RX, clear status or emit output. Successful callbacks exactly once. |
| Flat facade | No UART/HTIF device capability is added; guest RAM addresses map to storage offsets, host offset APIs stay offsets. Shared installation replaces RAM/core/metadata together. | Below-base, overflow, nonzero base/entry, image replacement and failed load preserving prior image. Preserve legacy offset alignment rejection until an approved change; moving checks must not accidentally grant accesses. |
| Host placement/inspection/`memory()` | Same target storage and validation domain, explicitly host-side, no fabricated Hart transactions or retirement. Exposed compatibility handles delegate; future capable atomics require all committed writes to participate in visibility. | `write_mem` currently byte-loops and can leave a prefix on failure. Proposed complete-span prevalidation/all-or-nothing host RAM write is a compatibility tightening to approve and test, not a claim about baseline. Empty reads remain empty; huge/overflow ranges fail without wrap. |
| Exit, signature, diagnostics, CLI/API | CLI tohost override > ELF metadata > fixed default; flat manual offset and load precedence retained. Decode before clear; native HTIF before RAM observer, flat RAM only. Preserve absent/empty/readable signatures, native suppression versus flat reporting of artifact failure, and existing diagnostic differences. | Same ELF equality only for shared RAM behavior; explicit device-difference and artifact-policy tests must remain. No broad claim of identical configurations. |
| MMU / TLM / DMI / peripherals beside public path | Retain component APIs/tests unwired in A7. Document future adapters to the same port: physical PTE reads/A-D writes, raw bytes and fault/failure taxonomy; TLM payload/status conversion and atomic envelope. | MMU error collapsing, TLM width/overflow/status handling and DMI visibility must be repaired before later wiring. A/D writes remain separate successful effects, not rollback units of later loads. No certification by type alias. |

Required approval ledger: atomic option; HTIF span tightening; host `write_mem`
failure atomicity; treatment of nonaligned image-base storage constraints; legacy
third-party adapter capability/error guarantees. Characterize exact old/new cases
before accepting the implementation contract. Unanticipated observable changes
return for approval rather than silently changing “compatibility.”

## 7. Layered tasks and executable acceptance matrix

All tasks below are **proposed**, not work authorized by this draft. New test
file names are deliverables, not tests claimed to exist or pass today. Each
implementation increment runs its narrow row first; final acceptance runs all
rows at one committed reviewed head. No skipped toolchain/external suite is a pass.

| Layer / dependency | Deliverable and precise checks | Command / exit criterion |
| --- | --- | --- |
| T0 — approval and characterization | Record §5 option and §6 decisions; add baseline cases for AMO opcode/width dispatch, HTIF interior addresses, map overlap/fallthrough, nonaligned image bases and partial host writes. Distinguish expected changes from regressions. | Existing `cargo test --test memory_bounds --test public_behavior --test a4_integrated_equivalence --test a6_task3_core_trap_test`; new `cargo test --test a7_migration_characterization`. Exit: approved old→new table, no unresolved runtime trade-off. |
| T1 — vocabulary, after T0 | Request/result validation independent of bus/TLM. New `tests/a7_physical_contract.rs`: widths 1/2/4/8, asymmetric byte patterns, exact read lengths; zero width, short/long payload/response, wrong category, unknown completion; distinct target/host/protocol failures. | `cargo test --test a7_physical_contract`. Exit: all taxonomy/raw-byte negatives fail closed, without trap logic or load extension in targets. |
| T2 — native targets, after T1 | New `tests/a7_native_targets.rs`: last valid span/one past/`u64::MAX`/empty RAM; cross-target and unsupported widths; approved overlap policy; all rejected read/write states and callback counts unchanged. UART RX/status success occurs once and is not rolled back; HTIF success once, rejection never exits. Inject lock/backend/protocol failures. | `cargo test --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests`. Exit: native adapters pass common T1 contract, no partial guest or approved host RAM writes, legacy API parity except approved deltas. |
| T3 — Hart connection, after T2 and atomic decision | New `tests/a7_hart_physical.rs`: spy sees one fetch and correct data request, exact address/width/category; LB/LBU/LH/LHU/LW/LWU/LD and FP raw transfers; misaligned fetch/load/store/atomic issue zero target requests. Inject target faults to causes 1/5/7, and host/protocol/unknown results with unchanged trap CSRs/rd/retirement and no fabricated record. Preserve original address under nonzero-base adapter. | `cargo test --test a7_hart_physical --test trap_test --test a6_task3_core_trap_test --test csr_access_test --test mret_conformance_test`; FP load/store unit tests. Exit: no ordinary public fetch/data route bypasses port; no new ISA semantics. |
| T4 — atomic boundary, after T0/T1; before public convergence exits | New `tests/a7_atomic_boundary.rs`: B tests recognized envelope rejection with zero ordinary reads/writes and no reservation singleton access; malformed envelope is simulator failure; unavailable target capability never SC failure. A additionally requires approved prerequisite tests for W/D, old value, indivisible serialized commit, per-Hart reservation/reset/isolation, conflicting write visibility, SC success/no-reservation/consumption/fault rules and `aq`/`rl` profile. | `cargo test --test a7_atomic_boundary --test amo_test`; `cargo test --lib isa::rv64a`. Exit: selected option proven, no public read+write emulation. Preserve arithmetic coverage separately; under B, legacy successful-helper assertions must be explicitly migrated to approved rejection expectations, not silently deleted or left on a bypass. Old arithmetic tests are not envelope evidence. C blocks activation rather than passing this row. |
| T5 — both facades, after T3/T4 | New `tests/a7_public_equivalence.rs`: same nonzero-base/entry ELF, data, signatures, replacement image, handler exit and exact budgets through both facades; test known expected states, not equality alone. Retain CLI-only UART/fixed HTIF, flat offsets/manual tohost, artifact policy and diagnostic differences. Log on/off preserves outcomes; fetched opcode logged without refetch, no trap commit. | `cargo test --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test`. Exit: API examples compile; spy/call-site audit proves both loops and helper stepping use the same physical contract; host maintenance shares target storage without becoming Hart execution. |
| T6 — closure, after all above | Refresh current inventory, migration ledger and exact-head evidence; retain component tests without enabling them. Run full Rust gate, fresh project guests and frozen external baseline separately. | Commands below. Exit: all required evidence observed for final implementation head, approved changes only, no second public physical route, no overstated component/ISA certification. |

Final implementation gate (not required to pretend-run for this documentation PR):

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
RISCV_TEST_OUTDIR=target/a7-fresh-riscv-elves ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a7-fresh-riscv-elves ./scripts/run_elf_tests.sh
```

Use the repository's fresh-directory guards and recorded toolchain/container
identity; do not accept preexisting checked-in ELFs as a fresh build. Retain all
**51 project-authored guests** (including five A6 trap guests) with per-case
results. This is distinct from the **51 frozen A5 ACT4 nontrapping RV64I cases**.
For the latter, execute `.github/workflows/a5-feasibility.yml` at the final
implementation head with its pinned tools/selection, require 51 generated,
executed and passed, linked audits and fail-closed controls, and retain run/head/
artifact hashes. Do not change the frozen selection to make a refactor pass.
The [A6 replay JSON](../verification/a6-act4-replay-35325844246.json) records
ACT4 `a7c99303516f4e668f7488f172043392e23b9dfd`, exact source head, selection
and config hashes; replaying that old ZIP is historical comparison, **not** fresh
execution of A7. Neither suite certifies the full A extension or new trap ACT4
coverage. Missing dependencies block that acceptance row rather than yielding
an inferred pass.

## 8. Approval, activation and remaining boundaries

Review may accept this as a useful draft without activating it. A later activation
PR must obtain explicit approval of the concrete scope/compatibility ledger,
resolve the atomic choice and dependencies, recheck A6's nine criteria with their
true evidence identities, and perform the final A6 rolling switch. Until then
`docs/dev-plan.md` stays byte-for-byte unchanged and A7 implementation is not
authorized by this draft. Accepted ADR history and historical A5 results are not rewritten.

Completion of an eventual A7 means public physical-path convergence under the
approved option, not completion of all four ADRs, an ISS/VP Machine framework,
MMU/TLM integration or a new ISA certification. No future route is allowed to
bypass this port merely because its component existed before A7.
