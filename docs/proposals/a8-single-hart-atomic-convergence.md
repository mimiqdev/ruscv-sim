# A8 Candidate Contract — Single-Hart Atomic/Physical Convergence

**Status:** Draft — candidate milestone contract awaiting maintainer/user approval;
not activated, not implemented

**Authority:** Informational, non-normative candidate milestone contract. This
document does not claim implementation completeness. Approval of this Draft is
approval to activate a successor contract through the repository rolling process;
it is not permission to implement before activation, and
[`docs/dev-plan.md`](../dev-plan.md) (A7) remains the sole Current technical
contract until a separately approved replacement lands. A8 is a candidate
number, not a commitment to an A8/A9 schedule.

**Drafted:** 2026-09-20

**Evidence baseline:** `c90e455bdfe2a0fa87b5210a8e1d3ef9e0697e22` (merged
post-A7 roadmap PR #55 head), source inspected 2026-09-20 for this Draft.
Every current-behavior claim below cites the inspected source or a named test;
the proposed behavior is a proposal, not an implementation claim. This change
is documentation-only: no runtime, test, ADR, historical-evidence, or
`.qing/config.toml` change is part of this Draft.

**Direction:** this Draft realizes Stage 1 (single-Hart physical/atomic
convergence) of the approved-for-planning
[post-A7 roadmap](post-a7-roadmap.md), leaving Stage 2 (Hart facts and N=1
Machine lifecycle) and the independent Stage 3 performance-test infrastructure
follow-on gate in place as later, separately approved candidates. It does not
pull Stage 2/3 work forward and does not schedule Stages 4–8.

## 1. Objective, decision and authority

Close the largest semantic hole left by A7: give the selected single-Hart
RV64A profile an **explicit indivisible atomic operation envelope** on the same
physical domain that A7 created, repair the AMO/LR/SC decode/width defects
under an approved profile table, move reservation ownership from the
process-global singleton to per-Hart architectural state, and make every
in-scope committed writer's reservation effect explicit. Exit the legacy typed
atomic bridge from the two standard public facades while keeping the old typed
constructor usable as an explicitly labeled compatibility adapter.

The architecture is unchanged. No ADR is rewritten or amended by this Draft:

* [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md)
  keeps owning Hart outcomes, retirement/trap rules, and the Hart/Platform
  LR/SC ownership split (ADR-0001 §6 "LR/SC ownership boundary").
* [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md)
  already requires the atomic operation envelope, indivisible AMO/SC, Hart-owned
  reservation state, committed-competing-write visibility, and capability-based
  rejection (ADR-0002 §§1, 4, 5, 6). A8 implements that accepted atomic portion
  for one selected domain. Retained non-conformance elsewhere is labeled debt,
  not an ADR exception.
* [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
  owns composition; A8 adds no Machine lifecycle and claims none.
* [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
  owns time/scheduling; A8 adds no time consumer, interrupt, or WFI behavior.

A7's rejected option B stays rejected: existing successful atomic programs are
not disabled, rejected, or reclassified as unsupported-instruction failures as
a shortcut. Where the approved profile changes an old result, the change is
enumerated in §6 with exact old→new expectations, affected fixtures, and user
approval items (§11); there is no blanket disable.

Out of authority for this Draft: `docs/dev-plan.md` is not modified here. Upon
approval, the rolling sequence in AGENTS.md applies (A7's contract is already
archived at
[`a7-non-atomic-physical-access-migration.md`](../archive/milestones/a7-non-atomic-physical-access-migration.md);
the approved A8 contract replaces `dev-plan.md` as the sole Current contract
in the activation change, and this Draft is then preserved as the archived
proposal snapshot). No activation is performed by this document.

## 2. Normative source decision for the atomic profile

**Selected specification:** the *RISC-V Unprivileged ISA Specification*,
2024-04-11 release (`riscv-isa-manual` tag `v20240411-DRAFT`) — the same
manual revision this repository already pins in
[ADR-0004 §17](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
for Zicntr — for the "A" extension (Zalrsc LR/SC and Zamo AMO) instruction
tables, result semantics, and alignment rules. The ratified 2019-12-13 edition
is a cross-check; its AMO/LR/SC encoding tables are identical. Where this
Draft cites a rule as **spec-mandated**, the cited manual revision is the
source; where it cites **profile freedom**, the manual explicitly leaves the
behavior to the implementation and this profile selects one option. The
operative `funct5` table (identical across the cited editions):

| funct5 | Instruction family | W (funct3=010) | D (funct3=011) |
| --- | --- | --- | --- |
| 00000 | AMOADD | AMOADD.W | AMOADD.D |
| 00001 | AMOSWAP | AMOSWAP.W | AMOSWAP.D |
| 00010 | LR (rs2 must be 0) | LR.W | LR.D |
| 00011 | SC | SC.W | SC.D |
| 00100 | AMOXOR | AMOXOR.W | AMOXOR.D |
| 01000 | AMOOR | AMOOR.W | AMOOR.D |
| 01100 | AMOAND | AMOAND.W | AMOAND.D |
| 10000 | AMOMIN | AMOMIN.W | AMOMIN.D |
| 10100 | AMOMAX | AMOMAX.W | AMOMAX.D |
| 11000 | AMOMINU | AMOMINU.W | AMOMINU.D |
| 11100 | AMOMAXU | AMOMAXU.W | AMOMAXU.D |

All other `funct5` values in the AMO major opcode are reserved encodings
(this includes `10001` and `10101`, which an earlier revision of this Draft
mis-listed as AMOMINU/AMOMAXU; they are reserved, and the review that caught
this is why T3 requires an exhaustive decode matrix over all 32 `funct5`
values). `aq` is bit 26 and `rl` is bit 25. **Spec-mandated:** the decode identity
above; W/D width semantics; AMO/LR `rd` receives the old value sign-extended
for W and full-width for D; SC `rd` receives 0 on success and a nonzero value
on failure; AMO arithmetic for W uses the low 32 bits of `rs2` and writes a
zero-extended 32-bit result; MIN/MAX are signed and MINU/MAXU unsigned; a
misaligned AMO/LR/SC raises an exception; LR with `rs2 != 0` is a reserved
encoding; the reservation set must include the addressed bytes; `rd = x0`
suppresses the register write but not the memory operation.
**Profile freedom selected by this contract:** SC failure code value (1);
reservation-set granularity and key (exact reserved byte span, keyed by the
port-issued physical address); number of outstanding reservations per Hart
(one; a successful LR replaces any prior reservation); deterministic SC success
conditions (stronger than the manual's "may fail for any reason" latitude);
faulting-SC reservation effect (retain — §5.4); `aq`/`rl` implementation
strength (no additional ordering effect, §5.3); device targets may reject the
atomic capability (ADR-0002 §6 capability rejection); misaligned accesses
raise the misaligned-address exception (not access fault), preserving the
current Hart precheck classification.

Extension certification is not claimed: passing A8 does not certify RV64A,
Zalrsc, Zamo, or any external conformance suite (§10).

## 3. Verified starting point (source-inspected at the baseline)

The following is what the code does today, verified directly at the baseline;
it is the "old" column of §6. It is recorded from source, not inferred from
the roadmap.

### 3.1 AMO/LR/SC dispatch and widths

* `src/execute/mod.rs::execute_amo` decodes `funct5 = raw >> 27` and
  dispatches: `00001 → exec_amoadd` (spec: AMOSWAP), `00111 → exec_amoand`,
  `00110 → exec_amoor`, `01000 → exec_amomin`, `01001 → exec_amominu`,
  `01010 → exec_amomax`, `01011 → exec_amomaxu`, `00100 → exec_amoxor`
  (correct), `00010` → LR-or-SC selected by `rs2 == 0`, `00011 → exec_sc`
  (dword fallback). Every other `funct5` — including the spec encodings
  AMOADD (`00000`), AMOAND (`01100`), AMOMIN (`10000`), AMOMAX (`10100`),
  AMOMINU (`11000`), AMOMAXU (`11100`) — returns
  `ExecuteError::InvalidOperation`, which
  `src/core/mod.rs::classify_execute_error` maps for `Opcode::Amo` to
  `SimulatorFailure` with `UnsupportedLegalInstruction` ("legal extension
  instruction is not implemented"), not an illegal-instruction trap. The
  spec AMOOR encoding `01000` *is* matched, but to `exec_amomin` — it
  executes the wrong operation. The unassigned values `00110`, `00111`,
  `01001`, `01010`, `01011` are likewise matched to wrong operations
  (AMOOR, AMOAND, AMOMINU, AMOMAX, AMOMAXU respectively).
* LR width is reversed: within `funct5 = 00010`, `rs2 == 0`, the W encoding
  (`funct3 = 010`, matched as `Funct3::Slt`) selects `exec_lr` (64-bit
  `read_dword`, no sign extension), and the D encoding (`funct3 = 011`)
  selects `exec_lr_w` (32-bit `read_word`, sign-extended). SC dispatch has
  two distinct paths that must not be conflated: (a) the real SC encodings
  `funct5 = 00011` are unconditionally dispatched to `exec_sc`, which writes
  a 64-bit dword — so real SC.D (`funct3 = 011`) already writes the correct
  width while real SC.W (`funct3 = 010`) wrongly writes 8 bytes; and (b) a
  malformed LR encoding (`funct5 = 00010`, `rs2 != 0`) is dispatched as SC
  with reversed widths — `funct3 = 010` selects `exec_sc` (64-bit write)
  and `funct3 = 011` selects `exec_sc_w` (32-bit write). Only path (b)'s
  `funct3 = 011` case ever performs a 32-bit SC write.
* Consequences verified in source/tests: a real AMOSWAP encoding executes
  AMOADD arithmetic (`amo.rs` has no swap helper at all); the D encoding of
  every AMO performs a 32-bit RMW and a sign-extended 32-bit `rd`
  (`src/isa/rv64a/amo.rs`; asserted by
  `tests/a7_migration_characterization.rs::opcode_funct5_dispatch_preserves_current_amo_width_debt`);
  LR.W at an address ≡4 (mod 8) passes the Hart's 4-byte alignment precheck
  (`src/core/mod.rs::data_access` maps AMO `funct3` 2→4, 3→8) but then
  faults as a load access fault inside the misaligned dword read.
* `aq`/`rl` are read (`raw >> 26`, `raw >> 25`) and ignored (`_aq`/`_rl`).

### 3.2 Reservation state

`src/isa/rv64a/lr_sc.rs` owns `GLOBAL_RESERVATION`, a process-global
`Lazy<Mutex<ReservationSet>>` with an exact-address `Option<u64>` key — the
guest effective address `regs[rs1]` before any base conversion — and no
reservation granule. Verified behaviors: LR reserves only after a successful
read; SC tests exact address equality, writes on match (`rd = 0`) or fails
with `rd = 1`, and clears the reservation on both outcomes — **except** that a
matching SC whose write errors returns before its clear, retaining the
reservation; ordinary integer stores, FP stores, AMO writes, host
`RiscVSimulator::write_mem`, `clear_tohost`, and image (re)installation never
invalidate it (`clear_if_matching` has no production caller;
`ruscv_sim::execute::clear_reservation` is exported but has no production
caller); `RiscvCore::reset` does not clear it; a flat image reload replaces
the core and RAM while the singleton key survives
(`tests/a7_legacy_atomic_compat.rs::public_flat_reload_replaces_storage_but_retains_legacy_lr_key`).
All of this is asserted as intentional A7 preservation by
`tests/a7_migration_characterization.rs::global_reservation_characterization_isolated`
(11 transcript rows) and `tests/a7_legacy_atomic_compat.rs`.

### 3.3 Routes, locking, and entry points

`src/core/mod.rs::step_outcome` routes ordinary `Load/Store/LoadFp/StoreFp`
through the validated raw physical port and everything else — including
`Opcode::Amo` — through `LegacyTypedMemoryAdapter` while holding the outer
`data_mem` lock for the whole helper call. The two standard facades
(`load_and_run`/CLI native bus; `RiscVSimulator` flat RAM) install raw
fetch/data ports over the same storage/device objects
(`src/executor.rs::install_image_with_physical_ports`). The public write
entry points while a run is live: Hart ordinary/FP stores (raw
`DataWrite`), legacy AMO/SC typed writes, host `RiscVSimulator::write_mem`
(byte loop, `&self`), and `clear_tohost` (8-byte loop, terminal-exit
observation). `RiscVSimulator::memory()` exposes the shared handle, so a
caller *could* write from another thread, though `run(&mut self)` and the
borrow checker serialize same-thread use. Image installation happens between
runs. No debug, TLM, or MMU writer is wired into the public path.

### 3.4 Device atomic behavior and old typed surface

The typed `SystemBus` accepts a legacy dword read/write at any *starting*
address inside the HTIF window (`is_htif` start-address check), reads return
0 and writes fire the exit callback; therefore LR.D/SC.D at the HTIF base
succeed today and SC.W at base+4 reaches a dword callback through the width
debt (`tests/a7_migration_characterization.rs`, transcript row
`sc.w@htif+4=retired/dword-callback`;
`tests/a7_legacy_atomic_compat.rs::successful_legacy_sc_width_debt_mmio_callback_is_retained`).
The raw native port is stricter (exact 8-byte endpoint, dword-only, no fetch)
but has no atomic category: `src/physical.rs` states there is intentionally
no atomic envelope. `RiscvCore::new` (typed-only constructor),
`ruscv_sim::execute::{clear_reservation, exec_*}` helpers, `ReservationSet`,
`run_until_exit`, `reset_core`, and `step_once` remain public typed surfaces.

### 3.5 Evidence boundary

A7's recorded implementation evidence (1,667 full Rust/doc results, 354
focused tests at `fb6f51c`, fresh 51/51 project guests, separately scoped
frozen ACT4 51-case run) is historical, bound to its exact heads, and is not
relabelled by this Draft. The two 51-case sets remain distinct and neither
certifies atomics.

## 4. Scope

In scope for A8:

1. An atomic operation envelope in the physical vocabulary: one indivisible
   target-visible event per AMO/LR/SC; typed target-rejection, host, protocol,
   and unknown-completion results consistent with ADR-0002 §§4–6.
2. Spec-correct W/D decode and widths for LR/SC and all AMOs including
   AMOSWAP, with reserved encodings becoming illegal-instruction traps.
3. Per-Hart reservation state owned by the Hart in `CoreState`, keyed by the
   port-issued physical address and reserved byte span, cleared by
   reset/image reload, replaced by a new successful LR.
4. Explicit writer policy for every public entry point that can write while a
   run is live, with tested reservation effect or documented quiescence.
5. Legacy atomic bridge exit from the two standard public facades; the old
   typed constructor remains usable with explicitly labeled non-conforming
   typed AMO/LR/SC behavior sharing the same per-Hart reservation state.
6. Fixture reclassification of the A7 atomic-preservation tests per §9, mixed
   ordinary/atomic public ELF guests, and exact-head evidence preserving A6
   budget/exit/observation and A7 ordinary-path regressions.

Non-goals: multi-Hart *execution* support (scheduler, coherence, shared-memory
ordering — constructing additional `RiscvCore` test objects to prove
reservation isolation is test-object evidence only, not multi-Hart support);
DMA or any inbound Platform master; TLM/SystemC/DMI/FFI; MMU/PMP/page-table
walks on the port (the MMU remains unwired; no "all accesses unified" claim);
interrupts/WFI/time/counters; Debug Mode or public debug wiring; Machine
lifecycle, observation plane, or commit/trap record changes (Stage 2);
performance facility or any runtime optimization (Stage 3); RVWMO formal
memory-model proof; misaligned AMO/LR/SC *success*; AMOCAS or any future
extension; full-ISA or extension certification; changes to public
`ExecutionResult`, cycle accounting, artifact, or CLI surface beyond the
atomic behavior matrix; rewriting or amending any ADR.

## 5. Proposed semantic contract

### 5.1 Ownership split (unchanged from the ADRs, made executable)

The Hart owns: decode/legality (reserved `funct5`, LR `rs2 != 0` → illegal
instruction; `funct3` outside {2,3} already illegal via
`is_illegal_encoding`); effective/original address, privilege, alignment
precheck (W→4, D→8; misaligned → load/store-address-misaligned trap with no
physical request, exactly the current precheck); **the AMO arithmetic
implementation** — one Hart-owned module for the read-modify-write
arithmetic, signed/unsigned comparison, and result extension (sign-extended
W, full D, `x0` suppression) — applied inside the indivisible envelope
(§5.2); `aq`/`rl`
acceptance and their profile strength; per-Hart reservation state and the
architectural SC result; trap mapping (target rejection → load access fault
cause 5 for LR, store/AMO access fault cause 7 for SC/AMO, original guest
address in `mtval`); retirement and accounting (an atomic instruction is one
Hart turn; no new time consumer).

The physical domain owns: the indivisible envelope (the one critical-section
read-transform-write event for an AMO; LR as one atomic-category read; SC
success as one atomic-category write); complete-span/width/payload
validation before mutation; exactly-once target effects; rejection with
**no partial effect** — no RAM byte change, no
UART register/FIFO change, no HTIF callback, no exit signal; and visibility of
committed competing writes within the domain so the Hart can invalidate
reservations. The domain executes the atomic critical section and the
physical effects; it does **not** implement ISA arithmetic or comparison
(ADR-0002 §6 keeps those in the Hart — see §5.2 M1). The reservation
authority is **not** moved to the Platform:
there is exactly one reservation store, per-Hart, in `CoreState`.

### 5.2 Envelope interface mechanism (candidates, recommendation flagged)

All candidates extend the existing validated port
(`src/physical.rs`) with an atomic category; none adds a second port, a
second RAM, or a second lock domain, and none represents an AMO/SC as two
ordinary visible accesses.

* **M1 (recommended) — indivisible critical-section envelope with a
  Hart-owned transform.** An atomic-category `PhysicalRequest` carries
  `{paddr, width (4|8), kind: RMW | LoadReserved | StoreConditional,
  aq/rl bits (informational)}`; an RMW additionally carries the operand
  bytes and **the Hart-supplied pure transform** — one function, produced by
  the single Hart-owned AMO arithmetic module (swap/add/and/or/xor/min/
  minu/max/maxu with W/D extension rules), mapping old bytes to new bytes.
  The backend executes exactly one locked critical section: read the span,
  apply the Hart-supplied transform, write the result, then release; the
  response returns the exact old bytes (RMW/LR) or a conditional status
  (SC). The transform's concrete representation (closure, descriptor
  interpreted by Hart-owned code, or equivalent) is a T1 mechanism choice —
  the contract requires only that the arithmetic implementation exists
  once, in the Hart layer, and is the same for every backend, so targets
  never implement or duplicate ISA arithmetic (ADR-0002 §6). The **Hart**
  decides SC validity from its own reservation state and issues the SC
  envelope only when its reservation covers the span; a conditional failure
  produces no physical request at all. Indivisibility is the backend's own
  critical section spanning read→transform→write: no competing initiator
  can enter it, and the intermediate read is never exposed as an ordinary
  observable access. Rationale: keeps both reservation authority and
  arithmetic authority entirely Hart-side (no duplicate conditional state
  and no per-backend ISA semantics, unlike a domain-side reservation token
  or a target-side operation interpreter), matches ADR-0002's general
  implementation-mechanism deferral (envelope encoding, reservation token,
  backend primitive), and is the
  smallest change to the A7 seam: `step_outcome` already holds the
  data-port lock across the executor call. "Two ordinary accesses wrapped
  as atomic" is structurally impossible: AMO is one critical-section
  transaction, SC-success is one write request, LR is one read request.
* **M2 — conditional-context envelope.** The SC request carries an opaque
  Hart reservation token; the target validates it against a domain-side
  per-initiator epoch before writing. Rejected: for a single-Hart domain this
  duplicates reservation knowledge in the Platform (a second authority in
  fact), adds state the domain must reset, and still needs Hart-side profile
  rules for granule/fault cases. It becomes attractive only with future
  multi-Hart/DMA scope, where the domain must arbitrate anyway.
* **M2b — target-side operation interpreter.** The envelope carries an ISA
  operation enum and each backend implements the RMW arithmetic itself.
  Rejected: it places ADR-0002 §6's Hart-owned arithmetic and
  signed/unsigned comparison into every target, inviting per-backend
  divergence (exactly the mis-coded-table class of defect) and requiring N
  arithmetic parity proofs instead of one. M1 keeps one Hart-owned
  implementation by construction.
* **M3 — typed-pair emulation under one lock.** Keep two typed calls but hold
  the domain lock across both. Rejected as the *contract*: it is exactly the
  ADR-0002-rejected read-then-write exposure; lock coverage is a
  compatibility mechanism, not atomicity evidence. It survives only as the
  documented non-conforming behavior of the old typed constructor route
  (§7.1), never on the standard facades.

The concrete Rust shapes (enum variants, request/response types, validation
hooks) are implementation mechanisms selected at T1; the contract fixes the
semantics above, the single-event rule, and the no-partial-effect rule.

### 5.3 Decode, widths, results, and `aq`/`rl`

Dispatch follows the §2 table: `funct5` selects the operation; `funct3`
selects W (4-byte, results sign-extended into `rd`, 32-bit RMW with
zero-extended writeback and low-32-bit operand) or D (8-byte, full-width);
reserved `funct5` and LR with `rs2 != 0` raise illegal instruction; AMOSWAP
stores `rs2`'s (low) value and returns the old value; AMOADD/AND/OR/XOR/MIN/
MINU/MAX/MAXU compute their W/D results per §2; LR.W/LR.D read and reserve;
SC.W/SC.D conditionally write. `rd = x0` suppresses only the register write.
`aq`/`rl`: all four combinations are legal encodings; in this single-Hart
in-order profile they impose no additional ordering effect, because the one
Hart's accesses are program-ordered and immediately visible through the one
domain, and host writers are quiescent (§5.5). This is a documented
stronger-than-required profile strength, not an RVWMO claim. FENCE behavior
is unchanged.

### 5.4 Reservation profile

Per-Hart reservation state lives in `CoreState` (participating in staged
state; `reset` installs a fresh `CoreState`, so reset clears it; facade image
reload constructs a new core, so reload clears it). The record is the exact
reserved byte span: `{paddr issued at the port, width}`. One reservation per
Hart; a successful LR replaces any prior reservation. SC validity: the SC
span must be contained in the reserved span; otherwise conditional failure
with no physical request. Deterministic success rule (profile guarantee):
an SC succeeds **iff** the reservation is present, covers the SC span, and no
in-scope committed write has overlapped the reserved span since the LR.
Executed SC consumes the reservation on success and on conditional failure.
**Faulting SC:** an SC that raises an exception (misalignment precheck or
target rejection) performed no architectural completion, so the reservation
is **retained** — this codifies the current observable behavior
(`sc.write-fault` characterization) instead of the accident; the alternative
(clear on any attempted SC) is listed in §11. Rejected writes never
invalidate (ADR-0002 §5.2: no invalidation from a write that did not commit).
SC failure writes `rd = 1` (profile value; current behavior).

The reservation key is the **physical address issued at the port** (after the
single configured base conversion): in the native bus form that equals the
guest address (pass-through translation base, `AddressForm::Bus`), in the
flat form it is the storage offset. Both facades therefore share one rule;
the key never mixes address forms, and a moving image base cannot silently
alias reservations.

### 5.5 Writer visibility policy (candidates, recommendation flagged)

Writer inventory at the baseline (all public entry points that can write
while a run is live, or between runs):

| # | Writer | Route | Proposed policy |
| --- | --- | --- | --- |
| W1 | Hart ordinary integer store | raw `DataWrite` port | In-domain: committed overlapping write invalidates the Hart's own reservation (same-Hart conflicting store). |
| W2 | Hart FP store | raw `DataWrite` port | Same as W1. |
| W3 | Hart AMO / successful SC | atomic envelope | In-domain: committed overlapping write invalidates. |
| W4 | Host `RiscVSimulator::write_mem` | typed flat byte loop, `&self` | **P1 (recommended): quiescent control writer** — structurally only callable between steps/runs (single-threaded borrows); documented not to participate in invalidation, so an SC after a host write to the reserved bytes still succeeds and overwrites it. Alternative P2 (participating) requires a notification/epoch mechanism because `write_mem(&self)` cannot reach per-Hart state; it is deferred to a future contract with concurrent-host support. |
| W5 | `clear_tohost` exit-signal clear | typed 8-byte loop at terminal exit observation | Quiescent by construction: runs while reporting the terminal guest exit, after the final completed turn; no reservation-affecting step follows. Documented, not silently ignored. |
| W6 | Image (re)installation (`load_program`, `load_elf`) | between runs | Replaces the core/domain; reservation cleared with it. |
| W7 | Cross-thread writes through the exposed `memory()` handle | caller-side | Documented unsupported: outside the writer domain; no conformance claim. |
| W8 | Debug/TLM/DMA/device-originated writes | none wired | Out of scope; unchanged. |

Verification for W1–W3 is by negative/positive reservation tests (§10);
W4/W5/W7 are documented control/inspection semantics with their observable
SC interactions asserted; W6 is tested by reload fixtures. Nothing outside
the table is silently admitted: any future writer enters only through a
contract change.

### 5.6 Device atomic capability

UART (byte-only window) and the HTIF endpoint do **not** advertise the atomic
capability: an atomic request is a target rejection with
`UnsupportedCategory` before any register/callback mutation, and the Hart
maps it to the access fault of its access class (load fault for LR, store/AMO
fault for SC/AMO) with the original guest address in `mtval`. Rationale:
modeling RMW against a callback-firing write register would make exit
observable mid-envelope and the read-0/write-callback semantics are not a
register file; ADR-0002 §6 explicitly blesses capability rejection with no
write. Affected historical successes are enumerated in §6 (HTIF rows) — this
is a user-approval item, not a silent tightening. RAM is the atomic-capable
native target; generic third-party raw backends decide capability themselves
and must reject before mutation if they cannot provide the envelope.

### 5.7 Fault, side-effect, and unknown taxonomy for atomics

Reuses the A7 physical taxonomy without weakening: **target rejection**
(mapped to causes 5/7 with original address, no retirement, no partial RAM/
device/callback effect, no reservation invalidation, no `rd` write);
**host/backend failure** (lock poisoning, unavailable backend →
`SimulatorFailure`, no fabricated trap); **protocol/invariant failure**
(malformed envelope/response → `SimulatorFailure`); **unknown completion**
(terminal `SimulatorFailure` with retained unresolved state on the core, no
retry on the next step, no fabricated retirement/trap/exit; reset does not
clear it — recovery is host resolution via
`clear_unresolved_physical_access` after the domain is actually resolved, or
reconstruction with a fresh core/domain; this is A7's boundary carried to the
envelope). An **unknown completion after a possible physical effect** must be
exercised at the seam (a test backend that reports unknown after beginning a
device-visible effect): the run fails terminally with uncertainty recorded,
nothing is retried, and no "restored" claim is made. Successful HTIF exit
ordering is untouched: atomic or not, an exit-causing store retires before
the platform exit is reported; a rejected atomic never produces an exit.

## 6. Compatibility: old → proposed behavior matrix

"Old" is the verified §3 behavior; "Proposed" is this contract. **Every row
that changes an observable result is an approval item (§11).** Existing
successful paths change only where the approved profile says so; none is
disabled wholesale.

| # | Surface | Old (verified) | Proposed | Affected programs/fixtures |
| --- | --- | --- | --- | --- |
| C1 | `funct5=00000` AMOADD | `SimulatorFailure` (unsupported legal instruction) | Retires with correct AMOADD.W/D | Real-toolchain guests currently fail; unlocked. New tests. |
| C2 | `funct5=00001` AMOSWAP | Executes **AMOADD** arithmetic and retires | Executes AMOSWAP.W/D (rd=old, memory=rs2) | Repo-encoded fixtures (`amo_test.rs` direct helper calls unaffected); characterization row `amoadd.w=…` flips semantics. **Result change.** |
| C3 | `funct5=00100` AMOXOR | Correct word RMW | Correct W/D RMW | D result changes from word/sign-extended to full 64-bit. |
| C4 | `funct5=01100/10000/10100/11000/11100` (spec AMOAND/MIN/MAX/MINU/MAXU) | `SimulatorFailure` (unsupported legal instruction) | Retire with correct W/D operations | Real-toolchain guests currently fail; unlocked. New tests. |
| C5 | `funct5=01000` (spec AMOOR) | Executes **AMOMIN** arithmetic (wrong operation) and retires | Correct AMOOR.W/D | **Result change.** |
| C6 | `funct5=00110/00111/01001/01010/01011` (unassigned values) | Wrong-op successes (`00110→AMOOR`, `00111→AMOAND`, `01001→AMOMINU`, `01010→AMOMAX`, `01011→AMOMAXU`) | Illegal-instruction trap (reserved encodings) | Only programs using the repo's own mis-numbered encodings; enumerated fixtures flip. **Result change.** |
| C7 | LR.W (`00010`,`rs2=0`,`funct3=010`) | 64-bit read, no reservation-width record; LR.W at addr≡4 (mod 8) access-faults | 4-byte read, `rd` sign-extended, reserves 4-byte span | Fixtures flip; the 4-mod-8 access-fault artifact disappears. **Result change.** |
| C8 | LR.D (`00010`,`011`) | 32-bit read sign-extended | 8-byte read, full `rd` | **Result change.** |
| C9 | Real SC.W (`00011`,`funct3=010`) | 64-bit dword write (unconditional `exec_sc` fallback) | 4-byte conditional write, `rd=1` on failure | Includes the HTIF base+4 dword callback row. **Result change (width).** |
| C10 | Real SC.D (`00011`,`funct3=011`) | 64-bit dword write — the width is already correct via the same fallback | 8-byte conditional write (same width; only the reservation/condition rules of C13–C16 change) | Width unchanged; condition semantics change. |
| C11 | Malformed LR encodings (`00010` with `rs2!=0`: `funct3=010`→64-bit write, `funct3=011`→32-bit write) | Execute SC (reversed-width helpers) | Illegal-instruction trap (reserved encoding) | Repo-encoded fixtures only; no A7 fixture exercises these encodings — new T3 tests cover the transition. |
| C12 | `aq`/`rl` | Parsed, ignored | All four combos legal; documented no-op strength | No observable change for in-scope single-Hart programs. |
| C13 | SC no-reservation / span not covered | Retires, `rd=1`, no write | Same (`rd=1`, no physical request) | Unchanged. |
| C14 | SC success | `rd=0`, write, reservation cleared | Same; reservation keyed per-Hart on port paddr | Unchanged for well-formed single-core guests. |
| C15 | SC write fault | Store access fault; reservation **retained** (early return), later SC can succeed | Store access fault; reservation retained as **profile rule**; same observable | Same behavior, relabeled debt→profile. |
| C16 | Reservation ownership | Process-global exact-guest-address singleton; survives reset/reload; shared across Core instances | Per-Hart `CoreState`, port-paddr byte-span key; reset/reload clear; new LR replaces | Rows `lr->other-core-sc`, `lr->reset->sc`, `…retains_legacy_lr_key` flip to `rd=1`. **Result change.** |
| C17 | Ordinary/FP store between LR and SC | No invalidation; SC succeeds and overwrites | Overlapping committed write invalidates → SC fails `rd=1`; non-overlapping never invalidates | Rows `lr->sd->sc`, `lr->fsd->sc` flip. **Result change.** |
| C18 | AMO write between LR and SC | No invalidation | Overlapping committed AMO write invalidates | New rule; consistent with C17. |
| C19 | Failed/rejected ordinary write between LR and SC | Reservation retained | Retained (no commit → no invalidation) | Unchanged. |
| C20 | Host `write_mem` between steps | No invalidation (row `lr->host-write_mem->sc`) | **P1:** unchanged observable (quiescent control writer, documented); P2 alternative flips it | Keep expectation under P1; relabel from "debt" to "documented control semantics". |
| C21 | LR/SC/AMO on HTIF endpoint | Legacy dword read 0/write callback succeeds (incl. SC.W@base+4 via width debt) | Atomic capability rejected → load (LR) or store/AMO access fault; no callback/exit, no partial effect | Fixtures `sc.w@htif+4=…`, `successful_legacy_sc_width_debt_mmio_callback_is_retained` retire into rejection tests. **Result change; user approval.** |
| C22 | LR/SC/AMO on UART | Byte-only typed path: wide ops already fail | Same observable (atomic rejected) with access-fault mapping | Unchanged in effect. |
| C23 | `GLOBAL_RESERVATION`, `ruscv_sim::execute::clear_reservation`, `ReservationSet` public surface | Public process-global reservation API | Removed/relocated: reservation state becomes per-Hart (`CoreState`); free function removed (breaking, 0.x); `ReservationSet` either becomes the per-Hart record type or is replaced | Test helpers updated; API removal is an approval item. |
| C24 | Old typed `RiscvCore::new` route for AMO/LR/SC | Typed read/write pair with global reservation | Typed pair **retained as labeled non-conforming compatibility adapter**, using the same per-Hart reservation state (one reservation authority); standard facades never use it | `amo_test.rs`/typed helper tests keep passing; doc labels the route. |
| C25 | Standard facades' atomic route | Legacy typed bridge | Atomic envelope through the raw data port; bridge removed from facades | Mixed ordinary/atomic equivalence tests. |
| C26 | Misaligned AMO/LR/SC | Hart precheck → load/store-address-misaligned trap, no physical request | Same | Unchanged. |
| C27 | `rd=x0` | No `rd` write, memory op occurs | Same | Unchanged. |
| C28 | Budget/exit/observation (A6) | Started-slot budget, retire-before-exit, commit log no-refetch, `mem_access=None` | Unchanged | A6 regressions must stay green. |

## 7. Bridge exit, adapters, and what "ADR-0002 atomic portion attained" means

### 7.1 Exit scope

The legacy typed atomic route is removed from `load_and_run`/CLI and
`RiscVSimulator` (the two standard facades). "Removed" means: no
AMO/LR/SC instruction on those facades issues typed `MemoryInterface`
read/write calls; all of them issue exactly one envelope request per
operation through the validated data port. The old typed constructor
(`RiscvCore::new` without ports) keeps the typed pair behavior explicitly
labeled: it is a compatibility adapter, not a conforming atomic backend, and
it shares the one per-Hart reservation authority (C24).

### 7.2 One storage/domain, one ISA engine

RAM/device objects stay shared references between the raw ordinary path, the
new atomic path, and host inspection; no copied buffer, write-through mirror,
or second RAM; `memory()` and image replacement keep the views bound to the
same objects. One reservation store. One decoder/dispatcher (the AMO dispatch
is rewritten in place, not duplicated). Lock order: the raw/atomic path locks
the selected port handle for the complete operation exactly as the ordinary
path does today; the typed compatibility route keeps its outer-handle
coverage; no path acquires the same mutex twice.

### 7.3 Attainment statement (exact)

At A8 closeout the repository may state, and only state:

> **Single-Hart atomic/physical convergence completed for the standard
> facades' native domain: AMO/LR/SC execute as indivisible envelope
> operations with per-Hart reservation state and explicit writer visibility,
> with the legacy typed bridge retired from the standard facades.**

It may **not** state: full ADR-0002 conformance for every access (MMU
page-table walks and A/D writes are not on the port; the MMU remains unwired;
TLM/DMI remain adapters with no atomic envelope); multi-Hart/DMA ordering or
coherence; RV64A/Zalrsc/Zamo certification; RVWMO proof; atomic support on
device targets. ADR-0002's atomic portion is attained **for the single-Hart
native domain of the standard facades**; the old typed constructor route is
explicit non-conformance. Any future TLM/SystemC adapter must provide the
envelope or reject the capability before mutation.

## 8. Tasks, dependencies, and executable acceptance criteria

The tasks define the approved implementation scope; this Draft performs none
of them. New test names are deliverables, not existing passes. Each
increment runs its narrow row first; all rows run at the final reviewed head.
Missing/skipped required tools block acceptance.

| Task / dependency | Deliverable and checks | Command / exit criterion |
| --- | --- | --- |
| T0 — fixture reclassification ledger and baseline (no behavior change) | Commit `docs/verification/a8-fixture-reclassification.md` mapping every A7 atomic fixture row to **keep / flip / retire / relabel** with the new expected value and the approving §6 row; add `tests/a8_atomic_baseline.rs` asserting the verified old behavior at the unchanged code (dispatch table, LR/SC width reversal, global reservation rows) so each later flip is a diff against an executable baseline. | `cargo test --test a8_atomic_baseline --test a7_migration_characterization --test a7_legacy_atomic_compat --test amo_test`; exit: ledger complete, baseline green, no unresolved classification. |
| T1 — atomic envelope vocabulary, after T0 | Extend `src/physical.rs` with the atomic category: RMW/LoadReserved/StoreConditional request kinds, the Hart-supplied pure-transform representation (§5.2 M1), operand/result bytes, conditional status, validation and response binding; update the module doc that today denies an atomic category. `tests/a8_atomic_contract.rs`: widths 4/8 only, payload rules, transform application for every AMO operation through the one Hart-owned arithmetic module, binding/completion validation, single-backend-call, unknown completion terminal with no retry, malformed envelope rejection — all at the vocabulary level without targets. | `cargo test --test a8_atomic_contract --test a7_physical_contract`; exit: envelope taxonomy complete; no ordinary read/write pair can represent an AMO/SC; A7 non-atomic contract tests unchanged. |
| T2 — native targets, after T1 | RAM executes the critical-section primitive — one locked read → Hart-supplied transform → write transaction with exact old bytes, exactly-once effect, complete-span/width validation, no partial write — and implements no ISA arithmetic; UART/HTIF reject the atomic category pre-mutation. `tests/a8_atomic_targets.rs`: success/negative spans, overflow, unsupported width/category, rejection leaves RAM/registers/FIFO/callback/exit unchanged and fires no callback; a recording spy target proves **one locked target-visible transaction** per AMO/SC whose internal read and write are not separately observable by a competing reader view; an arithmetic-parity test drives two different conforming backends through the same Hart transform and asserts byte-identical results for every operation/operand pattern; a recorded source/route audit shows no backend or target module references ISA operation semantics; lock-poison → host failure. | `cargo test --test a8_atomic_targets --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests`; exit: native critical-section + rejection semantics proven, arithmetic ownership stays Hart-side, ordinary native behavior unchanged. |
| T3 — Hart dispatch, reservation, writers, after T2 | Rewrite `execute_amo` per §5.3 (spec table, AMOSWAP, W/D, reserved → illegal); per-Hart reservation in `CoreState` with §5.4 profile (key/span/consume/fault-retain/reset/reload); envelope issue via the data port for port-configured cores; invalidation from W1–W3 committed overlapping writes; typed compatibility route on the old constructor sharing the reservation state; remove `GLOBAL_RESERVATION`/`clear_reservation`. `tests/a8_hart_atomic.rs`: **an exhaustive decode matrix enumerating all 32 `funct5` values × W/D** — every assigned encoding (incl. AMOMINU `11000`, AMOMAXU `11100`) retires its named operation and every reserved value (incl. the previously mis-listed `10001`/`10101`) raises illegal instruction — plus LR-`rs2!=0` malformed encodings; W/D results and sign extension; `rd=x0`; alignment precheck with zero physical requests; aq/rl combos retire; SC success/no-reservation/span-not-covered/consume-on-both; faulting SC retains; reset/reload clear; two-Core reservation isolation (test-object evidence, labelled not multi-Hart support); writer table W1–W3 negative/positive; typed-adapter route labeled non-conforming; no re-entrancy. | `cargo test --test a8_hart_atomic --test a8_atomic_baseline --test amo_test --test a7_migration_characterization --test a7_legacy_atomic_compat --test a6_task3_core_trap_test --test trap_test --test csr_access_test --test mret_conformance_test`; `cargo test --lib isa::rv64a`; exit: flipped fixtures assert new expectations per ledger; unchanged rows stay green; single reservation and arithmetic authority evidenced. |
| T4 — facade bridge exit and public equivalence, after T3 | Standard facades issue envelopes only; mixed ordinary+atomic guests identical across CLI/`load_and_run`/flat within documented configuration differences; host control writers (W4–W7) documented and observable behavior asserted; A6 budget/exit/observation regressions. `tests/a8_public_atomic_equivalence.rs`: mixed ELF fixtures, exit-after-atomic ordering (retire before platform exit), zero/exact/final-slot budgets, rejected atomic produces no exit, artifacts/reload differences retained, commit log no-refetch unchanged. New project-authored bare-metal atomic guests (LR/SC loop, AMOSWAP/ADD/MIN/MAX W/D, aq/rl set, SC-after-store failure, atomic near RAM end, atomic+tohost exit) added to the fresh-build guest suite. | `cargo test --test a8_public_atomic_equivalence --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test`; exit: no typed atomic call from standard facades (route audit recorded), equivalence within documented differences, new guests fresh-compiled and passing. |
| T5 — evidence and bounded closeout, after T0–T4 | Full gate below; fresh project ELF suite = the retained 51 guests **plus** the new atomic guests with a recorded new total (the historical 51 stays recorded as its own identity); frozen ACT4 51-case selection **unchanged and separately recorded** — no A-extension claim, no merged counts; residual-debt ledger updated (MMU/TLM/performance/Stage 2 items). | See commands below; exit: exact-head evidence complete, no unapproved compatibility delta beyond §6's approved rows, §7.3 statement only. |

Final implementation commands (not claimed run by this Draft):

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
RISCV_TEST_OUTDIR=target/a8-fresh-riscv-elves ./scripts/compile_riscv_tests.sh
RISCV_TEST_OUTDIR=target/a8-fresh-riscv-elves ./scripts/run_elf_tests.sh
```

Record toolchain/container identity, revision, commands, and per-case
results; preexisting ELFs are not a fresh build. External ACT4 replay remains
historical; any **new** atomic differential/oracle selection (for example a
pinned Spike/Sail comparison) is a separate explicit proposal and is not
authorized by this contract.

## 9. A7 fixture reclassification (classification contract for T0)

The A7 atomic-preservation fixtures do not silently carry forward as
specification. Classification (each lands in the T0 ledger with new expected
values):

* **Flip (expectation changes, fixture retained):**
  `opcode_funct5_dispatch_preserves_current_amo_width_debt` (both sub-cases:
  `00001/W` becomes AMOSWAP semantics; `00001/D` becomes full-width swap),
  transcript rows `amoadd.w=…`, `lr->other-core-sc=…`, `lr->reset->sc=…`,
  `lr->sd->sc=…`, `lr->fsd->sc=…`, `lr->reset->…`/`…retains_legacy_lr_key`
  (reset/reload rows), `ordinary_store_and_fp_store_interleave_with_unchanged_legacy_lr_sc`,
  `legacy_lr_survives_reset_and_replacement_storage_with_global_key_behavior`.
* **Retire (tests removed debt machinery; replaced by new-equivalent
  assertions):** `sc.w@htif+4=retired/dword-callback` transcript row and
  `successful_legacy_sc_width_debt_mmio_callback_is_retained` (replaced by
  atomic-rejection-at-HTIF tests), the global-singleton mechanics of the
  isolated child transcript (replaced by per-Hart state tests; process
  isolation harness retained only if still needed).
* **Keep (expectation unchanged, relabel debt→profile where noted):**
  `sc.no-reservation=…`, `lr@b0->sc@b8=…`, `lr->failed-sd->sc=…` (rejected
  write no-invalidation), `sc.write-fault=…` (retain-on-fault, relabeled),
  `lr->host-write_mem->sc=…` under P1 (relabel: documented control writer),
  `ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain`
  (same-domain visibility both directions),
  `rejected_ordinary_write_and_faulting_sc_preserve_characterized_reservation`
  (retain half),
  `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection`,
  `legacy_lock_reentry_child` (non-reentrancy, adapted to the envelope path),
  `amo_test.rs` helper-level tests (typed compatibility route).
* **New coverage with no A7 fixture (verified):** no A7 fixture exercises a
  real SC.D (`00011`,`funct3=011`), a malformed LR encoding
  (`00010`,`rs2!=0`), AMOMINU (`11000`) or AMOMAXU (`11100`). Their old→new
  transitions (C10, C11, C4) are therefore covered only by the new T3
  exhaustive decode-matrix and width tests, and the T0 ledger records that
  no existing expectation changes for them beyond those rows.
* **Historical evidence untouched:** the recorded A7 transcripts, closeout,
  and verification records stay as-is; reclassification happens in new/edited
  test files at the A8 implementation head with the ledger citing them.

## 10. Verification strategy and explicit non-claims

Negative and boundary coverage required at the final head: reserved
encodings; LR `rs2!=0`; W/D at both widths incl. sign-extension boundaries and
wrapping AMO arithmetic; MIN/MAXU signed/unsigned split; `rd=x0`; span
end/one-past/overflow at RAM boundaries; rejected envelope with **no** RAM/
UART/HTIF/callback/exit/reservation/`rd` side effect; unknown completion
before and after a possible physical effect (terminal, no retry, no fabricated
retirement/trap, reset does not clear; recovery only via host resolution or
reconstruction); atomic single-event with a competing reader never observing
an intermediate value; the full writer table; facade equivalence for mixed
guests; A6 budget/exit/observation regressions. Non-claims: no multi-Hart
execution, DMA, coherence, or RVWMO proof; no MMU/page-walk/TLM unification;
no interrupt/time/debug/Machine-lifecycle work; no performance measurement or
optimization; no extension certification; the two 51-case historical sets
stay separately recorded and the new guests are a separately counted
addition.

## 11. Decisions requested from the user (nothing below is chosen silently)

Recommended defaults are marked; each still requires explicit approval, and
alternatives are listed:

1. **Profile adoption (§2/§5.3):** spec table + W/D repair + AMOSWAP +
   reserved-encoding illegal traps + aq/rl all-legal-no-op. *(Recommended.)*
   Alternative: keep any single legacy mis-numbering alive (rejected: leaves
   the envelope semantically ambiguous).
2. **Reservation profile (§5.4):** per-Hart `CoreState`, port-paddr byte-span
   key, one reservation replaced by new LR, span-containment SC validity,
   deterministic success rule, consume-on-executed-SC, **retain on faulting
   SC**. *(Recommended.)* Alternatives: clear-on-any-attempted-SC; fixed
   granule larger than the accessed bytes.
3. **Writer policy (§5.5):** P1 quiescent host control writers with the
   documented SC interaction; W7 cross-thread documented unsupported.
   *(Recommended.)* Alternative: P2 participating host writers (needs a new
   notification mechanism and concurrent-domain work — larger scope).
4. **Device atomic capability (§5.6/C21):** UART/HTIF reject atomic with
   access-fault mapping. *(Recommended.)* Alternative: model HTIF tohost as
   an atomic-capable register with envelope-visible callback semantics
   (rejected: exit observability inside an envelope).
5. **Public API breakage (C23):** remove `GLOBAL_RESERVATION`,
   `ruscv_sim::execute::clear_reservation`, and relocate `ReservationSet`
   into per-Hart state (0.x breaking change, documented in the ledger).
   *(Recommended.)* Alternative: keep a deprecated no-op shim (rejected:
   suggests a global authority still exists).
6. **Old typed route (C24/C25):** typed pair stays on the old constructor as
   labeled non-conforming adapter; standard facades exit the bridge.
   *(Recommended.)* Alternative: remove typed AMO/LR/SC entirely now
   (breaking `amo_test.rs`-style callers; larger blast radius).
7. **Guest-suite growth (T4/T5):** add project-authored atomic ELF guests and
   record the new total; keep ACT4 selection frozen and separately recorded;
   new differential/oracle selection as its own future proposal.
   *(Recommended.)*

## 12. Activation sequence and navigation

This Draft activates nothing. Upon user approval: (1) the approved contract
replaces `docs/dev-plan.md` via the AGENTS.md rolling process (A7's contract
is already archived; the A7 closeout record stays historical); (2) this file
is preserved as the archived A8 proposal snapshot; (3) implementation tasks
run under the repository branch/review workflow. Until then A7 remains the
sole Current contract and no A8 work is authorized by this document.

Related records:

- [Current A7 contract (sole Current)](../dev-plan.md)
- [Post-A7 roadmap proposal (Stage 1 source)](post-a7-roadmap.md)
- [ADR-0002 physical-access contract](../architecture/decisions/0002-physical-access-transaction-and-fault.md)
- [ADR-0001 Hart outcome contract](../architecture/decisions/0001-hart-execution-outcome-and-observation.md)
- [ADR-0003 ownership boundaries](../architecture/decisions/0003-runner-machine-and-platform-ownership.md)
- [ADR-0004 time/scheduling boundaries](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
- [Archived A7 contract](../archive/milestones/a7-non-atomic-physical-access-migration.md)
- [A7 closeout record](../archive/milestones/a7-closeout-record.md)
- [A7 migration characterization](../verification/a7-migration-characterization.md)
- [A7 Hart/legacy-bridge record](../verification/a7-hart-physical.md)
- [Documentation policy](../documentation-policy.md)
