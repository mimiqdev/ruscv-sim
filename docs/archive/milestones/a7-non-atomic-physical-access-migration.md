# A7 — Non-Atomic Physical-Access Boundary Migration

**Status:** Historical — complete approved contract archived 2026-09-19

**Authority:** Informational historical contract record. The sole current technical
contract remains [`docs/dev-plan.md`](../../dev-plan.md) until an approved successor
and the required rolling transition are recorded. This archive preserves the full
A7 contract as activated; it is not a new plan or implementation claim.

**Activated:** 2026-09-18

**Archived:** 2026-09-19, with bounded implementation acceptance and evidence recorded
in [the A7 closeout record](a7-closeout-record.md). The closeout delivery change was
not merged when this historical copy was prepared.

**Evidence baseline:** `024d15d546dc3b711f593cd44bb107612fd8b600`, source inspected
2026-09-18. The original activation change was documentation-only.

## 1. Objective, decision and authority

Migrate **instruction fetch and ordinary integer/FP loads and stores** in both
public execution configurations to a transport-neutral physical boundary with
explicit physical address, width, access category, raw bytes and typed results.
Keep architectural address interpretation, alignment, load extension, exception
mapping and retirement with the Hart. Preserve public behavior, including
existing successful atomic programs, without copying storage or adding another
ISA implementation.

This is a **staged non-atomic migration**, not “all physical accesses unified.”
AMO/LRSC repair and a complete atomic operation envelope are outside A7. The
maintainer rejected the previous capability-rejection option B: disabling or
rejecting existing successful atomic paths is not a deliverable or fallback.
Reclassifying those paths as unsupported-instruction simulator failures is also
forbidden. The previous A/B/C choice is no longer an activation gate.

The final architecture remains unchanged:

* [ADR-0001](../../architecture/decisions/0001-hart-execution-outcome-and-observation.md)
  owns Hart outcomes, architectural effects and observations.
* [ADR-0002](../../architecture/decisions/0002-physical-access-transaction-and-fault.md)
  requires one physical port, complete raw-byte transfers, fault separation,
  indivisible atomic envelopes and Hart-owned reservation semantics. A7 implements
  only the non-atomic portion. Retained legacy atomics are known nonconformance,
  not an exception added to the ADR or proof of transaction atomicity.
* [ADR-0003](../../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
  owns placement/composition/Runner responsibilities; offset conversion is not MMU
  translation.
* [ADR-0004](../../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
  owns budget/time/control boundaries; A6's existing accounting is preserved.

Under [documentation policy](../../documentation-policy.md) and `AGENTS.md`, this
was the sole Current milestone contract at activation. A6 was formally accepted
and closed on 2026-09-18; its [full contract](a6-machine-mode-trap-entry-return.md)
and [closeout evidence](a6-closeout-record.md) are preserved. The [approved proposal
snapshot](a7-non-atomic-physical-access-proposal.md) retains drafting history, not a
second active contract. Approval covered the full scope and preservation ledger
below, not incidental behavior tightening or the rejected atomic-capability-denial
option. The current closeout record preserves the distinction between this
historical contract, its bounded delivery evidence, and the still-pending rolling
selection of a successor.

## 2. Starting point and evidence identities

The [A6 assessment](../../verification/a6-capability-assessment.md) and
[closeout record](a6-closeout-record.md) record PR #46's
merge at the baseline above. PR-head CI `35331709689` and exact-merge main CI
`35332790906` are distinct: the latter freshly compiled and passed 51 project
ELFs, including five trap guests. Frozen ACT4 run `35325844246` remains evidence
at source `845c63325db5ac87ab2ff0ed260453dc3b396ae9`, not at this documentation
HEAD. A6 formal acceptance and this successor's activation were authorized on
2026-09-18. At activation, the documentation delivery PR still required
independent review and merge; that historical workflow state was distinct from
the maintainer's acceptance decision.

| Source / focused regression | Existing behavior to preserve or debt not to overclaim |
| --- | --- |
| `src/core/mod.rs::step_outcome`, `enter_trap`, `classify_execute_error`; `tests/a6_task3_core_trap_test.rs`, `tests/trap_test.rs` | Typed retirement/trap/failure outcomes, synchronous Machine entry, Hart pre-transaction alignment and original-address `mtval` already exist. `step` is a compatibility wrapper. |
| `src/csr/mod.rs`, `src/isa/rv64i/system.rs`; CSR and MRET tests | MRET restoration/privilege checks, C-disabled IALIGN=32, `mepc` masking and authoritative `minstret` with explicit-write precedence are implemented, not A7 deliverables to redo. |
| `src/executor.rs::RunControl`, `install_image`, both public loops; `tests/a4_integrated_equivalence.rs` | Shared placement, installation and run decisions; started-slot budget versus completed turns. No full Machine abstraction. |
| `src/executor.rs::load_and_run`; public commit-log test | Hart-fetched opcode/privilege is logged only on retirement, without refetch. Runner GPR snapshots and `mem_access = None` remain observation debt. |
| `src/memory/mod.rs`, `src/core/mod.rs::MemoryAdapter`, `src/isa/rv64i/{load,store}.rs`, `src/isa/rv64{f,d}/load_store.rs` | Typed accesses and extension helpers remain active; ELF-base subtraction is storage adaptation. |
| `src/executor.rs::SystemBus`; `tests/memory_bounds.rs`, `tests/executor.rs` | RAM whole-span checks and RAM-first routing; UART byte-only public 0x100-byte window; HTIF dword methods check only the starting address. |
| `src/execute/mod.rs::execute_amo`, `src/isa/rv64a/{amo,lr_sc}.rs`; `tests/amo_test.rs` | AMO word read/write pairs, encoding/width defects and global reservation. Helper arithmetic tests are not public A-extension certification. |
| `src/mmu/{physical,sv39}.rs`; translation/A-D tests | Separate physical interface, walker error collapsing, separate A/D writes. Not wired to public core. |
| `src/tlm/{traits,payload,bus,status}.rs`, `src/peripherals/uart16550.rs`; TLM tests | Separate status/delay/DMI vocabulary, no atomic envelope; TLM UART width behavior differs. Optional core TLM field is unused. |

## 3. Scope, non-goals and deliverables

In scope for A7:

1. Non-atomic request/result vocabulary and native RAM/UART/HTIF adapters for
   fetch and ordinary scalar/FP memory transfers, synchronous and single-Hart.
2. Both public facades (`load_and_run`/CLI and `RiscVSimulator`) using that
   boundary for the specified Hart accesses, retaining their configuration
   differences. Public APIs remain callable; compatibility adapters are explicit.
3. A minimal internal compatibility bridge for unchanged legacy atomic dispatch
   over the **same storage/device domain**, not a new engine or memory copy.
4. Negative physical-result tests, migration characterizations, mixed ordinary /
   legacy atomic regression tests, public equivalence checks, and exact-head
   evidence including A6 and both separately scoped 51-case suites.
5. A current inventory and residual-debt ledger that identify the legacy path and
   state precisely what remains before full ADR-0002 convergence.

Non-goals: new ISA semantics, atomic encoding/width fixes, `aq`/`rl` repair,
per-Hart reservation conversion, AMO/SC indivisibility, complete atomic envelopes,
new atomic capability denial; MMU/Sv39/PMP/page faults; asynchronous interrupts,
CLINT/PLIC/WFI; SystemC/FFI/TLM public wiring or DMI; multi-Hart/DMA/coherence;
full Machine lifecycle, scheduler/virtual time, `mcycle`, new trace schemas or
extension certification. Host `write_mem` hardening is not bundled into this
migration. No A8/A9 schedule or automatic future commitment is created.

## 4. Non-atomic semantic contract

### 4.1 Request, bytes and address ownership

Each migrated request carries a 64-bit physical address, explicit nonzero width
(1/2/4/8 bytes), category **Fetch / DataRead / DataWrite**, and exactly the write
bytes when applicable. A response is tied to that request. This vocabulary is
not an atomic envelope: an AMO's two legacy subcalls must not be presented as a
conforming atomic transaction merely because they transfer bytes.

* Byte `i` corresponds to `paddr + i`; success returns exactly the requested
  number of bytes or complete write acknowledgment, without an invented value.
  Integer serialization is little-endian, independent of host integer layout.
  Hart owns signed/unsigned extension, narrowing, FP interpretation and decode.
* Hart owns effective/original address, privilege, alignment and trap cause.
  Misaligned ordinary fetch/load/store issues no physical request. Preserve A6's
  atomic prechecks as well, without claiming new atomic semantics. Target width
  rejection of an aligned request is an access fault, not misalignment.
* Validate the entire nonwrapping physical span before invoking a migrated
  target. Overflow, absent/denied target, unsupported width/category or no single
  target for the full span is target rejection. No cross-target stitching,
  implicit byte splitting, truncation or substituted width. Zero-width guest
  requests and mismatched payload sizes are caller/protocol failures. Empty
  host inspections retain their existing semantics, outside this request model.
* Fetch remains a distinct category; current aligned 4-byte RAM fetch works.
  UART/HTIF gain no instruction-fetch ability. Fault tests distinguish causes
  1/5/7 for fetch/load/store, not a width-based guess.
* Native bus addresses remain guest physical addresses. Flat configuration's
  guest address maps to its existing image-relative storage offset in the
  backend/compatibility layer; preserve checked base subtraction exactly once.
  Neither conversion nor `ImagePlacement` metadata resolution is translation.
  Guest PC and `mtval` never become host offsets. Flat `read_mem`, `write_mem`
  and `set_tohost` retain public offset meaning. Legacy atomics retain the same
  address conversion and reservation-key address form as before (see §5).

### 4.2 Typed results and architectural mapping

| Result | Meaning and guarantee | Hart / Runner behavior |
| --- | --- | --- |
| Complete success | Exact read/fetch bytes or complete ordinary write, including defined target effects | Hart applies instruction effects and retires normally when all architectural conditions succeed. |
| Guest target rejection | Valid migrated request denied by range, width, map, permissions or target bus/device failure; retain address/span/category/width and target context | Hart maps fetch/load/store to cause 1/5/7 with original address in `mtval`; no retirement or partial destination/store. |
| Host/backend failure | Unavailable backend, host resource or poisoned synchronization | Simulator failure, not fabricated guest CSRs/trap/commit. |
| Protocol/invariant failure | Short/long response, wrong category, contradictory completion or malformed transfer | Simulator failure, not target denial inferred from generic error strings. |
| Unknown completion | Effects cannot be established after backend/MMIO activity | Terminal simulator failure with uncertainty; no retry, rollback claim, fabricated exit, trap or commit; no reuse until safely resolved. |

These categories apply to the migrated boundary, not a retrospective claim that
all legacy atomic or third-party errors have been normalized. Preserve the
existing distinction between unsupported legal Hart instructions and guest
faults; **do not introduce either classification to bypass the rejection of
option B**. Existing successful atomic operations must remain successful.

### 4.3 Side effects and accounting

Migrated targets validate routing, span, width, payload and target preconditions
before mutation. Rejected ordinary writes leave RAM bytes, MMIO registers/FIFOs,
callbacks, output and exit signals unchanged. Rejected reads must not dequeue
RX or clear status. Known pre-completion failures leave no partial effect;
uncertain/nonrollbackable completion is simulator failure, not a clean target
fault. No byte-prefix write is allowed for one migrated guest write.

Successful UART RX/status reads, output writes and HTIF callbacks are committed
physical effects and cannot be rolled back by cloning Hart state. Finish fallible
architectural checks before invoking them. If a later host failure makes state
uncertain, report failure rather than pretending the target was restored.
These guarantees cover **ordinary transactions only**, not the indivisibility of
a legacy AMO pair, legacy SC or a host byte-loop write.

Successful exit stores retire before exit reporting; failed ordinary stores
produce no exit. Preserve every A6 trap boundary, continue-to-handler policy,
`minstret` explicit-write precedence, zero budget, recursive trap bounds, final
failed slot as execution error (not timeout), and final successful exit winning
over timeout. Physical calls are not extra Hart turns. A7 adds no modeled time
consumer; any future delay annotation/consumer must follow ADR-0004, not sleep
or change public `cycles` implicitly.

## 5. Legacy atomic boundary and separability constraints

### 5.1 What is preserved, not certified

`execute_amo` currently chooses helpers with encoding/width debt; AMO helpers
perform word read then word write even for D encodings. LR word/dword selection
is reversed for the usual width encoding, SC fallback uses dword, and `aq`/`rl`
are not enforced. `GLOBAL_RESERVATION` lives outside staged `CoreState` and is not
reset by core reset. SC's write error returns before its normal reservation clear.
Do not “fix” any of these incidentally during non-atomic migration.

Ordinary integer/FP stores and host writes currently **do not invalidate** that
reservation (`src/isa/rv64i/store.rs`, FP stores, `src/executor.rs::write_mem`;
`clear_if_matching` has no production caller). A same-address store between LR
and SC may therefore leave SC successful. Preserve this behavior as an explicitly
labelled regression target, not as correct RISC-V reservation semantics. Do not
add notifications only to the new scalar path, a second reservation copy, a new
reset clear, or address-key changes. Failed ordinary writes likewise must not
change legacy reservation state. Keeping one known-deficient authority avoids
new state divergence; it does not establish per-Hart ownership or coherence.

### 5.2 Source-supported migration seam, not an implemented proof

`src/core/mod.rs::step_outcome` already knows the decoded opcode and holds the
data-memory lock across `execute_with_csr_access`. `src/execute/mod.rs` separately
dispatches `Load`, `Store`, `LoadFp`, `StoreFp` and `Amo`. Thus the proposed seam
is an access-context/adapter selection at that existing connection, **not** a
second decoder or copied implementation of `execute_amo`:

```text
existing Hart / existing Executor
  fetch, ordinary load/store -> non-atomic raw physical adapter
  Opcode::Amo                -> explicitly legacy typed-memory adapter
                                (unchanged AMO/LRSC helpers/reservation)
             both views -> same RAM/device objects and locking domain
```

Ordinary ISA helpers can receive a Hart-side typed view of the new raw boundary;
load extension must no longer be delegated to native physical targets. The
legacy view retains existing typed widths, error behavior, address conversion
and device routing. Public stepping still runs the same `step_outcome` and
Executor. No new public legacy-mode switch is needed. Preserve the outer
lock across the complete legacy helper call; do not drop it between AMO subcalls
or reacquire the same mutex through the new adapter and deadlock. That lock is a
compatibility mechanism, **not** evidence of domain-wide atomicity.

RAM/device objects remain shared references, not copied buffers, write-through
mirrors or periodically synchronized snapshots. Ordinary stores must be visible
to the next legacy LR/AMO; legacy writes must be visible to ordinary loads,
fetch, signature extraction and host inspection immediately. `memory()` and
image replacement must keep those views bound to the same object. Existing
separate instruction/data handles in the public core constructor retain their
configured correspondence; the two standard facades continue to use shared
backing storage. Do not silently merge distinct caller-supplied memories.

Source structure supplies a plausible bounded seam, not executed parity proof.
T0/T3 below must characterize and test it before migration exits. If the concrete
adapter requires copying storage, weakens existing lock coverage, changes
reservation keys/SC outcomes, or cannot preserve a successful legacy MMIO atomic
path, stop and present the exact call chain and failing case for a scope decision.
Do not silently repair ISA behavior, deny atomics, or claim separability without
those checks.

### 5.3 Eventual exit from legacy debt (not an A7 deliverable)

Full ADR-0002 convergence later requires separately approved atomic encoding/width
and profile rules, per-Hart reservation ownership and reset/SC fault behavior,
indivisible AMO/LR/SC envelopes, committed-write visibility from every writer,
and regression evidence before deleting the legacy bridge. Multi-Hart/DMA scope
would need its own contract. No future milestone is automatically scheduled.
A7 can close with this debt explicit; it cannot close claiming all accesses are
unified, atomics are transaction-safe, or the A extension is certified.

## 6. Conservative compatibility ledger

The defaults below are approved preservation rules, not approval of incidental
behavior tightening. Any deviation requires an explicit decision and exact
old→new tests. Rejecting option B does not approve HTIF, host-write, placement or
third-party API changes.

| Surface | Approved conservative default | Measurable boundary / escalation |
| --- | --- | --- |
| HTIF span checking | New non-atomic port validates a full span at the existing 8-byte endpoint. Existing public typed `SystemBus` methods and the legacy atomic adapter retain start-address-only behavior. An aligned ordinary guest SD can reach only the base within that endpoint. | Test unshadowed raw port interior-span rejection with no callback, old direct dword calls at base+1 through base+7 with their existing callback/zero-read behavior, and guest misalignment before target access. Legacy SC.W at base+4 can currently reach a dword callback due to width debt: preserve it on the legacy path. No public API tightening is silently authorized; shared target object, explicitly different legacy validation, not claimed universal conformance. |
| Host `write_mem` | Keep byte-loop semantics outside the migrated guest transaction guarantee; do not add all-or-nothing host writes in A7. It still writes the same RAM seen by both paths. | Characterize an in-range prefix followed by out-of-range failure and retain prefix visibility. Preserve empty writes and offset meaning; characterize overflow/panic behavior in a bounded harness without claiming robustness or requiring a new panic contract. Any hardening is a separate approved change. |
| Nonaligned image base | Preserve both Hart guest-alignment checks and legacy backend offset-alignment rejection. Do not grant previously faulting accesses merely because a raw backend can transfer unaligned bytes. | For aligned guest address mapping to misaligned offset, preserve access-fault classification, original `mtval`, no destination/target effect; below-base and overflow remain checked. Preserve flat load-time tohost offset alignment validation and prior-image retention on rejected load. Moving subtraction must not change reservation keys. |
| Third-party `MemoryInterface` implementations / core constructor | Keep existing constructor, trait/helper signatures and supplied handles usable. A conservative typed compatibility shim forwards one existing typed call per ordinary transaction (never synthesized byte loops), marks fetch vs data explicitly, and maps only known typed errors. | Compile a minimal old custom backend; compare call widths/addresses/order and supplied-handle identity. It cannot be advertised as a fully conforming raw backend without side-effect/error guarantees; do not silently reject previously successful calls. Short/malformed new-port responses remain simulator failure; unknown legacy completion cannot become a clean guest fault. If blanket compatibility requires an unsafe guarantee, seek a decision before changing admission/API. |
| RAM/UART routing | Preserve RAM-first full-span match and existing device fallthrough where one full target accepts, including RAM size 4 versus 8 at HTIF. Preserve UART public 0x100-byte window, byte-only access and reserved-offset zero/ignored writes. | Never stitch RAM/device spans. Test crossing/end/overflow, UART width refusal without RX/status/output changes, and byte success exactly once. Do not shrink UART to TLM's 8-byte map. |
| Public facades and artifacts | Preserve CLI tohost override > ELF > default, flat manual-offset/load precedence, HTIF-before-RAM versus flat RAM-only observers, decode-before-clear, configuration-specific diagnostics and signature failure policies. | Shared RAM cases must match known results and each other. Explicitly retain CLI-only devices and native artifact suppression versus flat reporting. Absent/empty/readable/unreadable signatures and replacement images remain covered. |
| MMU/TLM/DMI and other components | Keep unwired; component tests retained, no new public route. | Later adapters must repair error/width/overflow/atomic/visibility gaps and use the final common port. A/D writes remain separate physical effects; no current page-walk or adapter-conformance claim. |

The new raw port is the only ordinary Hart physical route in the standard
facades. The table explicitly retains a **legacy atomic route and compatibility
API surface**; these are not hidden behind a claim of universal unification.

## 7. Tasks, dependencies and executable acceptance criteria

The tasks below define the approved implementation scope; this activation PR
performs no Rust implementation. New test names are deliverables, not existing
tests or observed passes. Run each
increment's narrow row first; run all rows at the final reviewed implementation
head. Missing/skipped required tools or tests block acceptance, not count as pass.

| Task / dependency | Deliverable and positive, negative, equivalence checks | Command / exit criterion |
| --- | --- | --- |
| T0 — approved contract and baseline characterization | Record §6 defaults; new `tests/a7_migration_characterization.rs` captures actual opcode/width behavior, successful legacy AMO/LRSC, SC no-reservation/fault behavior, HTIF interior calls, map overlap, nonaligned base and partial host write. Use baseline and candidate on identical fixtures. Label defect-preservation separately from architectural conformance. | `cargo test --test a7_migration_characterization --test memory_bounds --test public_behavior --test a6_task3_core_trap_test`; exit: baseline expected outcomes captured and no unresolved compatibility change. Existing successful atomics must not be replaced with expected denial. |
| T1 — non-atomic vocabulary, after T0 | New `tests/a7_physical_contract.rs`: explicit Fetch/Read/Write, widths 1/2/4/8, asymmetric bytes and exact lengths; zero width, mismatched payload/response, wrong category and unknown completion. Target/host/protocol distinctions independently asserted. | `cargo test --test a7_physical_contract`; exit: all raw-byte/taxonomy tests pass without load extension or trap logic in targets; no atomic envelope claimed. |
| T2 — native adapters, after T1 | New `tests/a7_native_targets.rs`: last valid/one-past/empty/overflow spans, cross-target refusal, preserved overlap priority, unsupported width, RX/status/output/exit side-effect snapshots on rejection and exactly-once effects on success; lock/backend/protocol injection. Keep legacy HTIF entry behavior separately tested. | `cargo test --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests`; exit: complete ordinary transaction guarantees on native targets and unchanged legacy API behavior; no copied storage. |
| T3 — Hart connection and legacy bridge, after T2 | New `tests/a7_hart_physical.rs` traces fetch and ordinary integer/FP transfers through the port; tests extension, endian order, original address and zero requests for Hart misalignment. Inject causes 1/5/7 versus host/protocol/unknown failures with no fabricated trap/retirement. New `tests/a7_legacy_atomic_compat.rs` mixes ordinary store→AMO/LR and legacy write→load/fetch/signature, LR→overlapping scalar/FP/host write→SC, rejected write→SC, SC write fault, reset/reload and distinct facade instances. Compare actual baseline reservation behavior, including absence of invalidation, not idealized per-Hart semantics. Include successful legacy MMIO/HTIF width cases. | `cargo test --test a7_hart_physical --test a7_legacy_atomic_compat --test amo_test --test a6_task3_core_trap_test --test trap_test --test csr_access_test --test mret_conformance_test`; `cargo test --lib isa::rv64a`; exit: same backing/lock/address-key evidence, unchanged successful atomic outcomes, no new invalidation/reset singleton or capability denial. Serialize reservation-sensitive fixtures or isolate processes; do not conceal global-state debt as concurrency safety. Any demonstrated inseparability is an escalation, not a passing row. |
| T4 — facade equivalence, after T3 | New `tests/a7_public_equivalence.rs`: identical nonzero-base/entry ELF, ordinary plus legacy atomic memory effects, trap-handler exit, artifacts and replacement; budgets 0, exact exit, recursive trap and final-slot host failure. Keep UART/fixed HTIF, flat offset/tohost, artifact-policy and diagnostic differences. Logging on/off preserves outcomes; no refetch or trap commit. Old custom backend compile/call-order regression. | `cargo test --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test`; exit: known expected states plus shared-case equality, all public fetch/ordinary memory call sites audited, legacy bridge explicitly identified rather than claimed migrated. |
| T5 — evidence and bounded closeout, after T0–T4 | Run full gate and both separately scoped suites below; update inventory and residual atomic debt. Keep all A6 regressions and independent component tests. | Exit: final-head evidence complete, no unapproved compatibility delta, no new ISA engine/storage copy, ordinary paths migrated and legacy atomics preserved. No claim of complete ADR-0002, atomicity, per-Hart reservation or full A-extension support. |

Final implementation commands (not claimed run for this documentation change):

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

Use fresh-directory guards and record toolchain/container identity, revision,
commands and per-case results; preexisting ELFs are not a fresh build. Retain
**51 project-authored guests**, including five A6 trap guests. Independently run
`.github/workflows/a5-feasibility.yml` at the final implementation head with the
frozen pinned setup: **51 ACT4 nontrapping RV64I cases** generated/executed/passed,
linked audits and fail-closed controls. Keep selection unchanged and record
run/head/artifact hashes. These are two different sets of 51.

The [A6 replay JSON](../../verification/a6-act4-replay-35325844246.json) retains
ACT4 `a7c99303516f4e668f7488f172043392e23b9dfd`, source revision, selection and
configuration hashes. Replaying that ZIP is historical comparison, not fresh A7
execution. Neither 51-case suite proves atomic correctness; T0/T3/T4 add the
required preservation coverage without certifying the known defects.

## 8. Acceptance and residual debt at A7 closeout

A7 is accepted only when T0–T5 have recorded final-head evidence and all of these
are true:

1. Both standard public configurations use the typed raw boundary for fetch and
   ordinary integer/FP loads/stores, with native negative and side-effect tests.
2. Hart address/alignment/extension/trap/retirement ownership and A6 budget/exit
   facts are unchanged; no MMU or new ISA behavior is introduced.
3. Existing atomic success/failure and reservation behavior is regression-tested
   across the shared domain, not disabled or reclassified. The legacy seam is
   enumerated, storage/locking is shared and no second reservation state exists.
4. Section 6's conservative defaults hold; deviations have explicit approval and
   new evidence. Known legacy host/API limitations are documented, not silently
   included in the non-atomic guest-transaction guarantee.
5. Rust, fresh project ELF and frozen ACT4 evidence is complete at the reviewed
   implementation head, with no historical result relabeled as a new run.
6. Closeout states “non-atomic physical-access migration completed” only. Section
   5.3 remains the unscheduled exit checklist for eventual full convergence.
