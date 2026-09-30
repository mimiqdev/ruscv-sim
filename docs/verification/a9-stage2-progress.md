# A9 Stage 2 progress — frozen T0, T1 facts and T2 direct-owner checkpoints

## 1. Scope and evidence identity

This record covers **T0–T2 only**, under [the A9 contract](../dev-plan.md) §5.
Sections 2–5 preserve the frozen T0 inventory, evidence and original expectations;
§6 records the T1 Hart observation checkpoint; §7 records its CLI reporting-error
correction. §8 records T2's directly tested N=1 composition/lifecycle owner;
§9 records its same-scope empty-signature correction.
T3 facade/Runner migration and T4 fresh guest/final acceptance remain deferred,
requiring their separate continuations. Earlier checkpoint evidence/status is
historical and is not automatically reused as evidence of T2 or later integration.
ADR-0001–0004 remain accepted authorities; the [A8 assessment](a8-closeout-assessment.md)
remains unchanged and bounded, not full ADR conformance or ISA certification.

| Identity | Meaning |
| --- | --- |
| `b36b4d08e10b6919e096209be4817ab26441d526` | Frozen spawn/source baseline (merged plan rotation PR #68). All source line ranges in §2 refer to this revision. |
| `1e2733d7789988402d0a63ce92de37b04dc4e042` | Committed T0 characterization checkpoint; adds only `tests/a9_baseline_characterization.rs`. Source is unchanged from the frozen baseline. Focused observations below were rerun at this exact HEAD, not inferred from A8 evidence. |

The accepted T0 checkpoint is `2a14ea154fa991d44a2bdb68a1d845c56fe93787`,
the documentation-only addition following the characterization commit. It remains
an unchanged ancestor. The recorded runtime commands in §4 apply **only to the
characterization checkpoint SHA above**, not automatically to a later SHA. Final checkpoint verification must
be rerun against the submitted committed HEAD. Commit identities and exact commands
here stand on their own; private check identifiers are not proof of a pass.

## 2. Frozen source route and ownership inventory

The following is an **as-is inventory**, not the target ownership architecture.
Relative file links identify repository sources; named functions and frozen line
ranges disambiguate them even if later implementation moves the code.

### Hart transition and observation

- [`src/core/mod.rs`](../../src/core/mod.rs):33–113 defines
  `InstructionRetired`, `TrapEntered`, `SimulatorFailure`, and `StepOutcome`.
  `RiscvCore::step_outcome` (748–1020) checks terminal unresolved completion,
  fetch/alignment/legality, stages `CoreState`, executes, sets fall-through or
  branch PC, suppresses x0, applies `minstret`, then replaces architectural state.
  `enter_trap` (1054–1091) stages completed entry before returning trap facts;
  execution errors are classified in `classify_execute_error` (1093 onward).
  A failed instruction does not gain a staged destination or retirement.
- Retirement facts already contain the fetched instruction, before/after PC and
  privilege, `minstret`, and explicit-counter-write status. Trap facts contain
  cause, faulting PC, original architectural `mtval`, vector and privilege context,
  optional fetched instruction, and guest-handler continuation. They are not
  complete immutable architectural-effect records: no GPR/FPR/CSR delta or
  ordinary/atomic memory effects are returned. Core state staging itself is not
  subscriber-gated optional observation, and does not prove an observer-off
  allocation contract.
- Native [`src/executor.rs`](../../src/executor.rs)::`load_and_run`
  (1519–1550) copies pre-turn GPRs **even without a logger**, then copies post-turn
  GPRs on retirement. It supplies `retired.pc`, `retired.instruction` and source
  privilege to `CommitLogger`; **there is no logging opcode re-fetch** at this
  baseline. Traps count as completed turns but generate no commit log line.
- [`src/core/commits.rs`](../../src/core/commits.rs)::`log_commit` (83–132)
  compares x1–x31 values; equal-value destination writes are invisible. It can
  format one supplied `MemoryAccess`, but the native runner passes `None`.
  Ordinary loads/stores, successful SC and AMO writes therefore have no memory
  annotation. Failed SC has no write; only a changed non-x0 result appears.
  CSR/FPR/counter/reservation effects are absent from this interface. A changed
  AMO destination is not a record of its memory effect. Logger IO errors after
  retirement are discarded (`let _ = ...`), not a distinct reporting fact.
- The new `snapshot_log_omits_real_memory_atomic_csr_fpr_and_same_value_writes`
  test measures both real state effects and the actual native log. The
  self-modifying-store test overwrites the executing instruction and confirms
  the log still uses the fetched store word. Neither test claims a future fact
  builder or sink-error policy is implemented.

### Standard facades and compatibility routes

| Route | Frozen behavior and ownership |
| --- | --- |
| CLI → `load_and_run_file` → `load_and_run` | [`src/main.rs`](../../src/main.rs)::`run_elf` (88 onward) forwards options. Executor constructs native RAM/UART/SystemBus/HTIF, ports, logger, loop, stop policy and artifacts (1383–1670). |
| Flat `RiscVSimulator::run` | Executor 1932–2032 has a separate loop over the same Hart semantics, using shared `RunControl` but only a selected RAM exit poll. No native devices or commit-log subscription. `run` creates new per-call budget/count state; the Hart and memory survive return. |
| Flat `step` | Executor 1918–1922 calls compatibility `core.step`; no tohost poll, clear, run budget, or artifact collection. A completed guest trap is `Ok`, not an execution error. |
| Shared standard installer | `install_image_with_physical_ports` (1337–1370) constructs one RAM object, loads bytes, obtains typed and validated raw views, constructs and resets the core. Native typed and fetch/data views share the bus/RAM; flat views share RAM. This is wiring inside executor, not a Machine lifecycle. |
| Core compatibility helpers | `reset_core`, `step_once`, `get_core_state`, `run_until_exit` (1703–1725) delegate reset/step/state clone/flat run. `RiscvCore::run` (1427–1440) counts completed Hart turns, with no Platform stop handling. |
| Typed-only constructor | `RiscvCore::new` / `new_with_memory` (623–650) retain typed memory; physical-port constructors/setter (657–705) are separate seams. No-port atomics use `LegacyTypedMemoryAdapter` through [`src/execute/mod.rs`](../../src/execute/mod.rs)::`execute_amo` and [`src/isa/rv64a/dispatch.rs`](../../src/isa/rv64a/dispatch.rs)::`execute_amo_typed`. This is the explicitly non-conforming compatibility route, not a second certified Machine configuration. |

In `step_outcome`, port-configured fetch uses the fetch port; ordinary integer/FP
memory operations use the data port; atomics use one `execute_amo_port` envelope
(919–970). System/MiscMem dispatch retains typed handles for non-port operations.
Separate constructor handles are not silently merged. Source/test evidence for
the typed route is also in `a7_public_equivalence`'s old-backend width/order test
and the labeled A7/A8 compatibility suites.

### ELF metadata, placement, load/reset/reuse

- [`src/elf.rs`](../../src/elf.rs)::`ElfLoader::load` validates/parses
  segments, entry and section/symbol metadata; its accessors (829–846) expose
  these facts. `memory_footprint`, `load_into_memory`, and `load_elf_file`
  (849–947) flatten PT_LOAD data relative to minimum `p_vaddr`, zero-fill the
  memory extent, round allocation up with a minimum `0x10000`, and return
  `LoadedElf` with bytes/base/entry/signature/tohost. The loader mutates a host
  buffer, not a runtime Platform; this is not yet the planned metadata-only
  installation boundary. It provides no MMU/PMP permission integration.
- Executor `AddressForm`/`ImagePlacement` (1174–1298) separate guest-address
  native placement from checked flat offsets. Native RAM maps the image base;
  the core base is zero. Flat RAM starts at storage offset zero; the core
  subtracts the image base. These conversions are **storage adaptation, not
  virtual translation**. Signature extraction is host inspection, not a guest
  instruction. Native artifact read failures are suppressed; flat failures are
  reported without discarding the primary result (986–1026, 2036–2086).
- `RiscVSimulator::load_elf` (1830–1887) validates derived tohost placement and
  alignment before assigning a replacement core, memory and image metadata.
  A rejected image leaves the previous composition intact. A successful load
  allocates a different RAM domain and clears Hart dynamic state/reservation.
  An image-declared tohost supersedes a manual pre-load offset; absent metadata
  does not inherit the previous image-derived signal. Configuration such as
  facade maximum budget and verbosity survives the load.
- `RiscvCore::reset` (1417–1422) installs default Hart state, PC/base and trap
  handler. It clears reservations/CSRs/GPRs/FPRs but does **not** restore RAM,
  reset devices, replace ports, drain callbacks, or clear terminal unresolved
  completion. `reset_core` is core-only, not a fresh Machine reset. There is no
  flat facade fresh-reset API at this baseline.
- Budget and exit returns are resumable. Existing A8 tests already exercise
  successful SC after budget/exit return and disjoint tohost clear. New T0 tests
  show `step` leaves a pending signal, zero-budget `run` does not consume it,
  later `run` retires another instruction before polling/clearing, and ordinary
  unsupported-instruction failure can be bypassed via existing explicit state
  mutation. Such reuse is not proof of lifecycle readiness.

### Callbacks, writer domain and stop ordering

- Native `SystemBus::native_transact` and `native_transact_atomic` (297–607)
  validate complete spans/capabilities. RAM has first-match priority. UART is
  byte-only; HTIF is an exact dword endpoint, reads as zero, and invokes its
  callback on successful writes/RMW. HTIF LR/SC is rejected under A8 D-c; no
  rejected operation invokes an exit callback. Typed `SystemBus` methods remain
  compatibility surfaces (611–775), with their own admitted writers.
- UART output callback runs in `Uart16550::transmit_byte`
  ([`src/peripherals/uart16550.rs`](../../src/peripherals/uart16550.rs):371)
  inside the bus's successful byte write. HTIF callback runs inside the physical
  transaction before the backend acknowledgement. The native runner callback
  records an atomic exit code (1443–1451); outer exit presentation is deferred
  until `step_outcome` completes and `RunControl` accounts for the turn. The
  channel-gated T0 callback test proves callback work can still be in flight
  before retirement is returned, at the direct native core/backend seam.
- `RunControl` (1055–1170) reserves started slots before executing and counts
  completed retirements **and traps**. `ExecutionResult.cycles` is not necessarily
  retired instructions or `mcycle`/virtual time. Failure completes zero turns
  and skips exit observers; it is not relabeled timeout on the final slot. A
  completed final-slot exit wins over timeout. Native observers run HTIF first,
  then RAM, lazily short-circuiting lower-priority observation. These are
  classified decisions, not a non-lossy Machine fact set.
- Selected RAM exit is decoded before clearing. `clear_tohost` (962–983) issues
  eight typed byte writes on the same memory handle, retaining the decoded
  code, without a second retirement; clear errors are only verbose diagnostics.
  Poll and clear acquire separate locks, so this is not one atomic host-writer
  exchange. A competing host write can interleave. Fixed HTIF has no backing
  value and is observed through its callback, not cleared RAM.
- `RiscVSimulator::memory` (2100) returns a cloneable
  `Arc<Mutex<dyn MemoryInterface + Send + Sync>>`. `write_mem` (2143–2153) takes
  that mutex and writes bytes, admitting a committed prefix on a later byte
  error. Neither `&mut self` on run/load nor a returned result revokes clones.
  `state_mut` is also an existing uncoordinated control-mutation surface.
- [`src/memory/mod.rs`](../../src/memory/mod.rs)::`MemoryStorage` holds bytes
  and overlap-only committed-write bookkeeping under one RwLock (77–218).
  Typed writes (604–638), `load_program` (309–317), raw writes (358–369), RMW
  and successful SC all bump covered versions. Rejected writes and conditional
  SC failure commit nothing. LR reads bytes plus version snapshot under one
  critical section; SC checks it in its indivisible section (428–519).
- [`src/physical.rs`](../../src/physical.rs)::`NativeRamBackend` and
  `native_ram_atomic_transact` (862–1157) share that RAM domain. Target checks
  precede SC conditional failure, including absent/uncovered context.
  [`src/isa/rv64a/dispatch.rs`](../../src/isa/rv64a/dispatch.rs)::`execute_amo_port`
  owns arithmetic/result and per-Hart reservation; successful/failed conditional
  SC consumes the staged reservation, while a fault discards the staged state
  and retains the pre-turn reservation under the approved A8 profile.
- Host clone writes are thus real competing writers, not just inspection. T0
  channel-ordered clone tests show overlap fails SC, disjoint write preserves
  it, and target rejection preserves it. Reload currently separates old/new
  RAM objects: a stale clone can still write old RAM/tohost but cannot write the
  newly loaded image. This proves the tested replacement case, not universal
  lifecycle fencing, handle revocation, or coherent inspection during a run.

### Unknown completion and unresolved lifecycle debt

`step_outcome` immediately returns the retained failure on subsequent attempts
(core 748–751); `unresolved_physical_failure` (1044–1052) stores it. Target
rejection instead enters an architectural trap. Protocol/host/unknown failures
never fabricate guest traps. `clear_unresolved_physical_access` (718–720) is an
existing host assertion that resolution/reconstruction already happened, with
no adapter proof validation or drain protocol. It must not become automatic
recovery.

The new unknown-write seam accepts a possible late write, returns unknown, then
releases that effect after **core-only reset**. Reset retains terminal failure
and no retry/retirement occurs, but the backend can still change RAM. Therefore
reset is not drain, nor a trustworthy fresh run. Standard native RAM/bus do not
naturally produce asynchronous unknown writes; the direct injected backend is
explicitly negative component/seam evidence, not a public adapter claim. Flat
reload constructs another core and has no exposed unresolved/drain guard; the
standard facade does not expose a setter for this injected backend. No test
here claims that arbitrary unknown work can safely be replaced or torn down.

## 3. Characterization matrix

All new tests are in
[`tests/a9_baseline_characterization.rs`](../../tests/a9_baseline_characterization.rs)
at `1e2733d7789988402d0a63ce92de37b04dc4e042`. Names below are executable anchors;
line numbers refer to that checkpoint. Tests assert behavior, not type presence.
No timing sleeps are used; channels/join and controlled backend responses fix
causal ordering.

| Test (line) | Observed assertions / evidence level |
| --- | --- |
| `snapshot_log_omits_real_memory_atomic_csr_fpr_and_same_value_writes` (59) | Real native log plus flat state: SD/AMO/LR/successful SC/failed SC/x0/CSR/FLD/equal-value GPR destination effects, absent memory facts and identity-only effect lines. Enabled/disabled native result parity, not allocator proof. |
| `self_modifying_store_log_uses_fetched_instruction_not_replaced_bytes` (129) | Executing aligned store changes its own code bytes; log retains fetched store identity. |
| `failed_sc_retires_but_faulting_sc_does_not_write_rd_or_retire` (157) | Native port seam: rd=x0/non-x0, valid unreserved SC versus invalid target, exact `minstret`, unchanged RAM, cause/original value. Failed SC writes 1 iff non-x0; faulting SC writes neither rd nor retirement. |
| `completed_exit_wins_final_budget_and_step_does_not_poll_or_clear` (184) | Native final-slot exit; flat step/zero-budget/one-slot resume, retirement-before-clear and retained nonzero exit. |
| `failed_last_slot_does_not_present_pending_exit_or_timeout_and_facade_can_reuse` (216) | Both standard facades: final-slot unsupported legal FENCE.I completes zero turns, no exit/timeout; flat RAM signal survives, explicit PC mutation permits later retirement/exit. |
| `core_only_reset_clears_hart_but_preserves_mutated_ram_and_ports` (254) | Native core seam and compatibility helpers: cleared Hart/reservation/counter, unchanged mutated RAM, retained ports observe old bytes. |
| `cloneable_cross_thread_writers_affect_sc_only_for_committed_overlap` (271) | Actual public cloned handle writes between resumable runs: overlap/disjoint/rejected cases; SC result, final bytes and retirement, no reservation leakage. Not an in-flight exclusion proof. |
| `reload_isolates_stale_cross_thread_handle_and_failed_load_preserves_current_image` (310) | Public reload: channel-released stale writer updates old RAM/tohost only; new RAM/state/reservation stay fresh; subsequent bad-placement load preserves current memory identity/PC/GPRs. |
| `native_callbacks_run_inside_started_turn_before_retirement_is_returned` (346) | Direct native composition: gated UART/HTIF callback fires with real payload before completed-turn return; eventual PC and `minstret` verify successful retirement. No callback re-entry performed. |
| `unresolved_completion_survives_core_reset_while_late_backend_effect_is_not_drained` (434) | Injected ordinary-write backend: possible late effect, terminal unknown across reset, one request/no retry, no retirement, eventual RAM mutation remains failure evidence. |

## 4. Observed commands/results (not future-stage acceptance)

Host toolchain observed: `rustc 1.98.1 (48a229cea 2026-09-01)` and
`cargo 1.98.1 (797e8a9bc 2026-08-05)` on the assigned worktree. No cross toolchain
or development-image command was invoked for T0.

At the exact committed checkpoint `1e2733d7789988402d0a63ce92de37b04dc4e042`,
with a clean worktree:

```bash
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
qing verify a9-foundation --check a9-t0-focused -- \
  cargo test --all-features \
  --test a9_baseline_characterization --test a4_run_control \
  --test a6_task3_core_trap_test --test a7_public_equivalence \
  --test a8_hart_atomic --test a8_public_atomic_equivalence
```

Both exited 0. Focused Rust totals: **74 passed, 0 failed, 0 ignored**:
A9 10, A4 2, A6 17, A7 10, A8 Hart 23, A8 public 12. These hand-built Rust ELF
fixtures require no external guest assembler. `git diff --quiet
b36b4d08e10b6919e096209be4817ab26441d526..HEAD -- src` also exited 0, establishing
that the exercised runtime source equals the frozen baseline.

Pre-commit observations on the candidate test contents: focused suite passed,
`cargo fmt --all` followed by `cargo fmt --all -- --check` passed, and
`cargo clippy --all-features --all-targets -- -D warnings` passed after replacing
an overly complex test type with `PendingWrite`. Initial test construction
errors (missing test-backend atomic trait, an unaligned self-modifying store,
and an incorrect unsupported-instruction fixture) were corrected before commit;
these were test-fixture failures, not production behavior changes.

The captured final Rust quality contract (fmt/check/strict clippy/all-feature
tests/docs/diff) still requires exact submitted-HEAD execution; focused evidence
above does not replace it. T0 does **not** claim a fresh 58-guest project run,
toolchain-required A6 ELF evidence, new ACT4 execution, PR CI/review acceptance,
or milestone completion. Native all-feature tests may include optional
cross-toolchain skips; any such skip is not guest evidence or a fresh pass.
T4 requires the development image/separate output directories and fresh guest
identities prescribed by the plan. Historical A8 project/ACT4 evidence is not
carried forward as T0 or T4 evidence.

## 5. T0 next-stage expectations — T1 status is recorded separately in §6

1. **T1 Hart fact boundary:** staged state acceptance and `enter_trap` are the
   completed-transition seams. Capture authoritative effects internally where
   execution/physical completion actually occurs, then materialize only for
   subscribers at the completed Hart boundary. GPR snapshots cannot distinguish
   equal-value writes, successful SC with rd already zero, or a memory-only
   store. FPR/CSR/counter and trap-entry effects must be precise. Unknown failure
   cannot emit a commit/trap; a reporting error cannot unretire a completed turn.
   Preserve fetched opcode/log presentation without making snapshots the new API.
2. **T2 ownership/installation:** move the existing installer composition behind
   one N=1 lifecycle owner; preserve current native versus flat placement and
   artifact differences. Parsing metadata versus placement must separate without
   adding MMU semantics or copied active RAM. Keep initial image/zero-fill bytes
   available for promised fresh reset; core reset over mutated RAM is insufficient.
3. **Stale handles and admitted writers:** today's replacement allocates a new
   domain and isolates the tested stale clone. A proposed reset that reuses RAM
   would not have that protection. All existing host writer paths, including
   clones, typed methods, committed byte prefixes and `clear_tohost`, must share
   the reservation/version domain and a proven lifecycle admission/fencing rule.
   Preserve admitted writes during ordinary resumable execution; do not silently
   disable them to simplify reset. Generation/isolation or refusal mechanisms are
   open Rust choices subordinate to the contract, not a selected public policy.
   Positive and unsafe-operation/refusal cases must both be tested.
4. **In-flight work:** current callbacks execute inside a physical request;
   future observer delivery is separate work after architectural completion.
   A port/memory mutex alone cannot prove all callbacks, observers and late
   backend effects are drained. A pending quiesce must stop new turns and account
   for started work and causal events before acknowledging lifecycle readiness.
   No synchronous facade return, budget timeout, or run-level `NoProgress`
   establishes `DrainComplete`. NoProgress/drain states are not implemented here.
5. **Terminal uncertainty:** retain unresolved physical completion through unsafe
   mutation attempts. Require adapter evidence of no possible late effect before
   drain, without retrying the transaction. Proven drain does not validate old
   uncertain state or retrospectively retire; fresh reset must restore the
   promised image/device state or refuse. No force reset or unchecked flag-clear
   shortcut is authorized.
6. **T3 policy:** both loops consume the same composed Hart/Platform facts while
   Runner retains public priority/budget/artifact semantics. Preserve all applicable
   boundary facts before classification, including failure versus final-slot
   budget and retirement before exit. Existing `step`/`run` resumability and
   helper/typed compatibility APIs must remain explicit, not accidentally turned
   into automatic fresh resets. No interrupt/time scheduler or debug integration
   is implied by these seams.

The preceding sections were readiness evidence for the T0 checkpoint, not
current implementation claims. Neither checkpoint authorizes self-advancement
or declares A9 complete.

## 6. T1 completed Hart facts checkpoint

### Implementation and ownership

T1 implementation/test checkpoint:
`4a2ba70125464459b1f03a03fcfc8e6cee35a6a9`. The documentation-only commit
adding this section does not change its implementation. The final submitted HEAD
must still be independently reverified; checkpoint evidence is not automatically
portable to later commits.

- [`RiscvCore::step_transition`](../../src/core/mod.rs) executes the existing
  staged Hart transition once. Its always-present `ControlFacts` describes PC,
  privilege, attempted instruction, retirement/trap status and MINSTRET before/
  after. Terminal repeat calls after unknown completion report no new attempt.
  `step_outcome()` remains an observation-off compatibility wrapper.
- [`core::observation`](../../src/core/observation.rs) is the single Hart-owned
  completed record builder. `CommitRecord` carries the exact fetched word and
  four-byte active-profile length, Hart identity and retirement/control identity;
  `TrapRecord` carries precise trap entry, optional fetched identity (absent on
  fetch/alignment failure), and accepted trap CSR effects. No completed record
  is built for `SimulatorFailure`.
- [`Executor::execute_transition`](../../src/execute/mod.rs) returns destination
  intent from the executed dispatch, not from a second Runner decoder. Equal
  GPR writes are facts; x0, stores and FP-only destinations do not fabricate GPR
  writes. [`CsrFile`](../../src/csr/mod.rs) journals accepted explicit/implicit
  writes, including equal writes, trap-entry/MRET writes and MINSTRET precedence.
  [`FpuRegisterFile`](../../src/fpu/mod.rs) journals actual written indices;
  FCSR changes are captured at acceptance. This observes existing ISA behavior,
  including its existing FPU dispatch/suppression rules, not new F/D certification.
- [`ObservedMemory`](../../src/core/observed_memory.rs) forwards each ordinary
  typed operation once and captures accepted bytes, width, guest address and
  issued address. Atomic completion capture in `PhysicalAtomicAdapter` preserves
  validated old bytes, transformed AMO bytes, successful SC payload/status and
  aq/rl metadata. Failed SC has no memory write (even rd=x0); faulting SC has no
  retirement/destination. Typed compatibility atomics explicitly carry
  `indivisible=false`; a typed failed SC with no issued operation has no issued
  address. Reservation before/after is retained. No observer refetch/readback,
  synthetic physical atomicity, or address translation is added.
- Returned observations are owned completed values, independent of mutable Hart
  state; sinks borrow them read-only through `HartTransition::deliver`. No sink
  runs inside Hart execution or a physical transaction. Native Runner delivery
  is synchronous and ordered once per completed turn, before the next turn.
  Sink errors are outer reporting errors and do not replace the returned facts
  or undo architectural/device effects. No async observer queue/drain is claimed.
- Native [`load_and_run`](../../src/executor.rs) consumes these facts, eliminating
  Runner GPR snapshots and refetch. [`CommitLogger`](../../src/core/commits.rs)
  preserves the text format, equal-value GPR omission, no FP/CSR/memory text and
  no trap line; the old public formatting helper remains available. Log delivery
  errors are now reported in `ExecutionResult.error` after causal exit polling
  and completed-turn accounting, preserving guest exit code/PC/cycles and final
  budget context. This is distinct from a failed Hart turn. [README](../../README.md)
  documents the observable reporting behavior.

### Demand gate and deterministic evidence

[`tests/a9_hart_facts.rs`](../../tests/a9_hart_facts.rs) contains 13 tests:

- GPR equal/suppressed writes, CSR read-only/equal writes, MINSTRET explicit write
  precedence, branch identity, ordinary B/H/W/D loads/stores, FP load/store/equal
  writes and FCSR changes; flat base conversion is observed once without MMU.
- W/D LR/SC/AMO through physical and typed routes, rd=x0, reservation changes,
  successful versus failed SC; precise traps/MRET and fetch/data fault identity.
- Target versus host/protocol errors, malformed atomic completions and terminal
  unknown writes: no commit/trap on simulator failure, no retry or invented guest
  trap. Sink failure preserves both a completed retirement and trap entry.
- On/off state/control/physical-call parity and no off sink calls. A thread-local
  global allocator spy measures only a warmed-up ADDI turn: off allocates once
  for existing CSR state staging, on allocates more for subscribed effects. Host
  setup/TLS initialization is excluded; this is not an allocation-free execution
  or performance claim. The builder spy below covers first/disabled turns too.

Two unit tests in `core::observation::tests` directly spy on completed record
construction: eight off turns build zero records; an on retirement builds one;
unsupported simulator failure builds none; a completed trap builds another.
A native HTIF store callback sees zero materialized records while in the physical
request, then the returned retirement contains its accepted store bytes and is
presented after the boundary without another physical write.

Two `executor::tests::t1_reporting*` tests inject a failing writer into the actual
native Runner (not another execution path). They verify completed PC/cycles,
a successful exit-causing final-slot store and preserved guest exit code, and
separate reporting-failure classification with final-budget context.

### Recorded committed-checkpoint verification

At `4a2ba70125464459b1f03a03fcfc8e6cee35a6a9`, these commands passed:

```bash
cargo test --all-features \
  --test a9_hart_facts --test a9_baseline_characterization \
  --test a6_task3_core_trap_test --test a8_hart_atomic \
  --test a8_public_atomic_equivalence --test a4_run_control \
  --test a7_public_equivalence
cargo test --all-features --lib core::observation
cargo test --all-features --lib t1_reporting
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
```

Focused totals: A9 T1 13, frozen T0 10, A6 17, A8 Hart 23, A8 public 12,
A4 2, A7 10 (**87 integration tests**), plus **four T1 unit tests**. The full
all-feature suite also passed (850 library tests and 27 doctests, plus integration
suites). The configured Rust check set and focused commands were recorded against
that exact committed HEAD, separately from all T0 evidence. No fresh cross-toolchain
project guest, ACT4, development-image, PR CI or independent T1 review evidence is
claimed. Optional guest skips in native tests do not establish T4 verification.

### Remaining boundaries and risks

At the T1 checkpoint, there was no Machine owner, installation change, fresh reset, writer fencing,
quiesce/drain, facade composition migration, scheduler or debugger integration.
All lifecycle debt from the frozen inventory remains: reset over mutated RAM,
clone writer admission, callbacks and unknown late effects cannot be declared
drained by a returned transition or successful observation delivery. An unknown
completion remains terminal; its lack of a completed observation is not evidence
that no physical effect occurred. Existing typed APIs/ISA behavior are retained,
not newly certified. T2–T4 were deferred at this checkpoint, not silently
implemented by the Hart fact boundary. See §8 for subsequent T2 evidence.

## 7. T1 reporting-error presentation correction

Implementation/regression checkpoint:
`c5df7221029037317d5d2501f2e3e176d4557575`. Independent review of the
previous submitted HEAD `051ba1ac7e4366946d85c048d9d36f8e850c122a` found a
source defect despite its passing Rust checks: a commit-log error coincident
with guest exit 0 was retained in `ExecutionResult.error`, but the CLI printed
`SUCCESS` and exited 0. Passing checks at that HEAD did not establish correct
presentation of this coincidence.

The new regression reproduced that defect before the CLI correction: a NOP
image with preloaded `tohost=1`, one started slot and a failing log write printed
`Exit Code: 0`, `Cycles: 1`, final PC `0x80000004`, `SUCCESS`, and the reporting
error, with process code 0.

[`src/main.rs`](../../src/main.rs) now requires both guest code 0 and no error
before printing `SUCCESS`; a result error with guest code 0 gives process code 1.
The displayed/library guest code is not overwritten. Existing nonzero guest
process codes and timeout presentation remain unchanged. No Hart facts, sink
semantics, lifecycle behavior or public API layout changes are made.
[`ExecutionResult::exit_code`](../../src/executor.rs) documents that a zero code
alone does not establish success; [README](../../README.md) explains the distinction
between preserved guest code and CLI process status.

Deterministic new coverage:

- `executor::tests::t1_reporting_failure_preserves_zero_guest_exit_after_completed_nop`
  injects the existing failing writer into the real native Runner and proves guest
  code 0, completed cycle/PC and no timeout remain available alongside a reporting
  error and retained exit context.
- [`tests/a9_cli_reporting.rs`](../../tests/a9_cli_reporting.rs) executes the actual
  CLI binary twice. Its negative case gives only the child a zero regular-file
  size limit, ignoring SIGXFSZ so the log write returns an I/O error. Log creation
  succeeds, then the first write fails, without timing sleeps, `/dev/full`, or a
  production injection API. It proves `FAILED`, process code 1, preserved displayed
  guest code 0, completed cycle/PC, and a reporting-error diagnostic. The control
  case writes one log line, prints `SUCCESS`, and exits 0. These two process tests
  are Unix-gated and ran locally; no non-Unix process-fault injection is claimed.

At the exact committed checkpoint above, the following commands passed:

```bash
cargo test --all-features \
  --test a9_cli_reporting --test a9_hart_facts --test a9_baseline_characterization \
  --test a6_task3_core_trap_test --test a8_hart_atomic \
  --test a8_public_atomic_equivalence --test a4_run_control \
  --test a7_public_equivalence
cargo test --all-features --lib core::observation
cargo test --all-features --lib t1_reporting
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
```

Totals: **89 focused integration tests**, two builder/boundary unit tests and
three native reporting unit tests; full all-feature tests passed including
851 library tests and 27 doctests. This is fresh checkpoint verification, not
reuse of the preceding HEAD's evidence. A final documentation-only HEAD still
requires its own verification and independent review. At this R1 checkpoint,
T2–T4 remained deferred, with no cross-toolchain/ACT4/lifecycle claims. The
subsequent T2 checkpoint is recorded below.

## 8. T2 N=1 composition and fenced lifecycle checkpoint

Implementation/test checkpoint:
`d22fd7bbcf7959ac9ac8f23659b8a126ba0b8a71`, based on the unchanged ancestor
`1a45cd257f751adbfa1d86ba480176170a810508` (T1/R1). The documentation-only
commit adding this section requires its own final-HEAD verification. Neither
prior checkpoint review nor these local checks constitute formal PR-head/fresh-
guest acceptance or authorize T3 advancement.

### Ownership, installation and state restoration

- [`machine::Machine`](../../src/machine/mod.rs) actually owns one installed Hart
  and one native or flat Platform, wires the existing validated fetch/data/atomic
  ports and invokes the **same `RiscvCore` semantic engine**. Control clones share
  one owner, not separate cores. No call to `load_and_run` or second ISA dispatcher
  hides behind the owner. The pre-install state is configured/drained with no
  image; execution before installation is refused.
- [`elf::LoadImage::parse`](../../src/elf.rs) uses the existing ELF parser/profile
  to describe ordered file bytes, explicit zero tails, entry, image base/rounded
  allocation size, flags, physical-address metadata, signature and tohost. It
  allocates immutable restoration inputs, not active execution RAM. Placement
  retains the existing `p_vaddr` policy; flags/`p_paddr` metadata do not introduce
  permission enforcement or MMU translation.
- [`machine::platform::Platform`](../../src/machine/platform.rs) performs placement
  into **one active RAM allocation**, shares it with typed/raw/atomic views, and
  owns existing `SystemBus`/UART/HTIF routing and raw device events. Segment order,
  pure zero-fill/overlap, gaps and allocation padding match the compatibility
  loader. [`image`](../../src/image.rs) supplies shared address/metadata conversions
  without Runner error/result/policy dependencies; the small compatibility adapter
  in [`executor`](../../src/executor.rs) preserves existing `ExecutorError` behavior.
- Install/replacement and `fresh_reset` require an acknowledged drain. Construction
  and validation occur before publishing the new domain, so rejected placement
  leaves current identity, memory capability, PC and generation unchanged. Fresh
  reset retains the same immutable image identity and static configuration/
  callback connections, but allocates a fresh, exclusively active generation,
  restores image bytes/zero fill, resets Hart PC/privilege/GPR/FPR/FCSR/CSR/counters/
  reservation, and creates fresh device registers/FIFOs/event queues. Initial RAM
  tohost bytes are restored from the image, not arbitrarily rewritten to zero.
  This is explicitly different from the retained legacy core-only reset.
- `inspect` coordinates owned Hart, device, metadata, signature and signal views
  under admission exclusion. It refuses in-flight or uncertain inspection; raw
  host memory views remain volatile, not coherent Machine snapshots. Signature
  inspection retains the existing native bus versus checked flat address forms.
  Selected tohost samples and HTIF/UART events are raw facts, **not decoded guest
  exit or Runner stop policy**. No virtual-time/cycle model is introduced.

### Admission, causal delivery and drain proof

[`machine::admission`](../../src/machine/admission.rs) tracks admitted Hart work,
host operations and completed boundary receipts independently of the Platform
mutex. The synchronous physical operation and native callback are inside their
parent Hart lease. A `MachineTurn` receipt keeps completed control/Platform facts
and accepted observation delivery admitted until the caller consumes/drops it;
sinks borrow the immutable T1 facts, without entering Hart execution. The next
turn is refused while that receipt exists, preserving delivery order. No
per-instruction callback, materialized architectural record or serialization is
added for observation-off control. Returning from a step is not drain.

`request_quiesce` stops **new live-generation work**. Already admitted writes,
turns, callbacks and boundary/sink delivery finish normally. `try_drain` is a
nonblocking acknowledgment only after a request and all admitted work/receipts
have completed, and returns residual causal signal/event facts with the
acknowledgment. Install/reset/control edit/teardown refuse while work exists or
while no drain has been acknowledged. Zero outstanding work, run return,
timeout, an outer NoProgress claim or a single mutex acquisition cannot grant
mutation. This does not implement a scheduler or infer NoProgress eligibility.

All admitted work leases retain the **entire owner**, so dropping the last control
handle cannot destroy the domain during host, physical, callback or observation
work. The drained control-state edit API takes an owned value rather than leaking
`&mut RiscvCore`; edits invalidate reservations to prevent importing a prior
storage generation's opaque LR snapshot. No GDB/debug integration is implied.

### Host writer and unknown-completion boundaries

[`machine::host_memory`](../../src/machine/host_memory.rs) keeps typed B/H/W/D
writes, `write_mem` (including accepted byte prefixes) and RAM-backed `clear_tohost`
in the same `SimpleMemory` bytes/version domain as Hart atomics. Ordinary live-
generation writes remain admitted between and during resumable turns; quiesce
refuses new writes explicitly, not silently. The new native host view is RAM-only
and storage-offset addressed, like the flat host view; devices/ports are not
exposed through it. New owner APIs require `resume` after drained install/reset
before admitting ordinary writes. Existing unmanaged facade writer APIs are
unchanged in T2.

After proven drain, reset/replacement publishes another RAM generation. Stale
clones remain usable on detached old RAM, but cannot mutate fresh RAM, invalidate
fresh reservations or trigger a new native callback/exit. No active bytes are
copied into a second running Hart/storage domain. Detached writes are not work
on the current Machine. No raw active RAM/port/device mutation capability escapes
the owner.

Unknown completion is **terminal and fail-closed**: the original typed failure
remains in status, no commit/trap/exit is fabricated, and no turn, state edit,
install, fresh reset, coherent inspection or teardown bypasses uncertainty.
This fixed synchronous configuration has no adapter termination/state-resolution
proof API; it conservatively refuses recovery even when a test has released and
joined a possible late effect. A joined test thread, cleared flag, cancellation
request or timeout is not an accepted adapter proof. Known completed failures
can drain, but resume still requires fresh restoration.

If the last control handle is dropped while completion/safety is unknown or the
owner is poisoned, implicit cleanup quarantines the installed domain rather than
claiming teardown/drain or disconnecting an unresolved transport. **Resources are
intentionally retained until process exit.** This is a bounded safety trade-off,
not an async transport, force recovery or proof that uncertain prior state was
correct. External callback-owned state/output streams are not Platform state and
are not rolled back by reset; configured callbacks are synchronous connections.

### Deterministic direct-owner evidence

[`tests/a9_machine_lifecycle.rs`](../../tests/a9_machine_lifecycle.rs) has **13 tests**:

- Native/flat metadata and initial bytes versus the existing loader; construct/
  install/reset/execute/inspect/drain/fresh rerun/teardown; BSS and pure zero-fill
  overlapping segments; image identity, counters/CSR/privilege/FPR/FCSR/reservation
  restoration. The pure zero-fill case first reproduced a placement-seam defect
  (empty raw write rejected) before its focused correction.
- Replacement entry/base/signature/tohost isolation; rejected flat placement;
  stale cloned and channel-controlled cross-thread writers; active overlap versus
  disjoint writes and clear-tohost preserve SC version semantics and byte prefixes.
- Retained off/on boundary and blocked observer receipts refuse mutation/drain;
  sink failure preserves retirement; UART/HTIF callback work blocks drain before
  retirement and its returned causal receipt also blocks drain afterward.
- Native UART registers/FIFOs and callback connections rerun freshly; HTIF store
  retirement, event/payload, cleared per-run event state and no stale signal leak;
  failed versus faulting SC/x0/HTIF D-c retain T1/A8 facts; drained state edits do
  not import stale-generation reservation snapshots.

Four unit tests in [`machine::tests`](../../src/machine/tests.rs) use controlled
seams, not sleep races or public recovery switches:

- Channel-gate the **real admitted writer path before RAM locking**, covering
  typed dword, prefix `write_mem` and `clear_tohost` across native/flat and fresh
  reset/replacement (12 combinations). New writes are rejected after quiesce;
  started writes finish and are reflected in the drain report before mutation.
- Inject one unknown write into the already composed native/flat port, then
  release an actual delayed RAM effect or native HTIF callback through that same
  original port. Mutation/drain/retry remain refused both before and after the
  channel-confirmed late effect; generation/PC/MINSTRET and original failure
  remain unchanged; exactly one request was issued. Failure emits no observation
  or presented exit. Last-owner drop retains the uncertain domain.
- Native callback unwind also cannot be declared drained/restorable.
- Boundary/host leases keep the whole owner alive after the last control handle
  is dropped; after proven synchronous completion, retained RAM is detached.

### Exact committed-checkpoint verification and integration limits

At `d22fd7bbcf7959ac9ac8f23659b8a126ba0b8a71`, these commands passed:

```bash
cargo test --all-features \
  --test a9_machine_lifecycle --test a9_hart_facts \
  --test a9_baseline_characterization --test a9_cli_reporting \
  --test a6_task3_core_trap_test --test a8_hart_atomic \
  --test a8_public_atomic_equivalence --test a4_run_control \
  --test a7_public_equivalence
cargo test --all-features --lib machine::tests
cargo test --all-features --lib core::observation
cargo test --all-features --lib t1_reporting
cargo test --all-features --test a9_cli_reporting
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
```

Focused totals: **102 integration tests** (T2 13 plus the preceding 89), **four
T2 unit tests**, **two T1 builder/boundary tests** and **three reporting tests**.
The full all-feature suite passed including 855 library tests and 27 doctests.
R1's actual CLI failing/working-log cases both ran and passed at this exact HEAD;
guest code 0 and retired PC/cycles remain preserved, with reporting failure
presented as FAILED/nonzero. Frozen T0 tests and evidence identity remain intact.

This is **direct Machine composition/lifecycle integration**, not migration of
`load_and_run`/CLI or `RiscVSimulator`. Those facades/helpers/typed compatibility
constructor and their existing run/step/host-writer behavior remain unchanged;
legacy core-only reset is still not a fresh Machine reset. T3 must integrate the
owner and preserve all public policy/artifact/configuration differences. No
T4 fresh 58-guest cross-toolchain run, ACT4, development-image run, PR CI/formal
PR-head review, full ADR/ISA certification, or A9 completion is claimed. No new
board, topology, MMU/PMP, scheduler/interrupt/WFI/time, TLM/SystemC, debugger,
multi-Hart/DMA, block/JIT or performance infrastructure was added. T3/T4 remain
deferred, not self-activated by this checkpoint.

## 9. R2: empty signature inspection before address resolution

The T2 checkpoint `314d9d7162af3e508b2f35527276bc08270178d6` had a
**source defect**, not merely missing evidence: `Platform::signature` resolved
flat metadata before checking its length. An empty `.signature` at
`BASE + 0x30_000`, outside allocated RAM, therefore produced `Some(Err(...))`
through the direct Machine owner. The existing facade's empty artifact behavior
and [ADR-0003 §7](../architecture/decisions/0003-runner-machine-and-platform-ownership.md#7-runner-result-limits-and-inspection)
require `Some(Ok(Vec::new()))` without a mapping check. The earlier A7 facade
regression did not exercise the new owner path.

At correction checkpoint `1929d5f5916885b5b59c0d2325a0fd3b085cd706`,
[`Platform::signature`](../../src/machine/platform.rs) returns an empty artifact
for `size == 0` **before** resolving addresses, converting sizes, locking the
memory target or reading bytes. Absent metadata still yields `None`; nonempty
unreadable regions still yield a raw artifact error for subsequent Runner
policy. Metadata is retained unchanged; this adds no guest access, architectural
transition or new facade/stop policy. T0/T1/R1 and the remaining T2 lifecycle
mechanisms are unchanged.

The added
`signature_inspection_preserves_empty_absent_and_unreadable_regions_after_fresh_reset`
in [`tests/a9_machine_lifecycle.rs`](../../tests/a9_machine_lifecycle.rs) first
failed before the production correction with the exact flat outside-RAM error.
It now checks **seven metadata cases on native and flat, before and after fresh
reset** (28 inspection combinations): absent; empty above RAM, below image base
and at `u64::MAX`; readable nonempty; and unreadable nonempty above/below RAM.
Each inspection preserves image identity and address/size/file-offset metadata,
keeps PC/MINSTRET unchanged, and does not create pending events/work. Each fresh
reset follows a real NOP retirement and acknowledged drain, increments the
storage generation and restores the starting Hart state. Nonempty errors remain
distinct from empty success and absent output. README documents the direct-owner
artifact contract; the public native/flat facade artifact policies remain unchanged.

The correction checkpoint above was freshly verified using:

```bash
cargo test --all-features signature_ \
  --test a9_machine_lifecycle --test a7_public_equivalence
cargo test --all-features \
  --test a9_machine_lifecycle --test a9_hart_facts \
  --test a9_baseline_characterization --test a9_cli_reporting \
  --test a6_task3_core_trap_test --test a8_hart_atomic \
  --test a8_public_atomic_equivalence --test a4_run_control \
  --test a7_public_equivalence
cargo test --all-features --lib machine::tests
cargo test --all-features --lib core::observation
cargo test --all-features --lib t1_reporting
cargo test --all-features --test a9_cli_reporting
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
git diff --check b36b4d08e10b6919e096209be4817ab26441d526..HEAD
```

All passed at that committed HEAD: **103 focused integration tests** (T2 now 14),
four lifecycle unit tests, two builder/boundary unit tests, three reporting unit
tests, and both owner/facade signature filters. The full suite passed including
855 library tests and 27 doctests. The documentation-only child adding this
record must receive its own exact-HEAD verification and independent review;
these checks do not approve a later HEAD automatically. All T3/T4 deferrals,
unknown-completion quarantine/proof limitations, cross-toolchain/ACT4 and
formal PR-head acceptance non-claims in §8 remain unchanged.
