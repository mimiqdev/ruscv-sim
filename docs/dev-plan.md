# Development Plan

**Current milestone:** A8 — Single-Hart Atomic/Physical Convergence

**Status:** Current

**Authority:** Normative milestone contract. The maintainer approved all seven
§11 profile choices at the recommended defaults on 2026-09-21 (encoding
repair; faulting-SC=retain; P2a-precise writer visibility; HTIF=D-c;
`GLOBAL_RESERVATION`/`clear_reservation` removal; typed adapter retained;
atomic guests added). An independent full-text review of the candidate passed
with no actionable findings (`no_actionable_findings` at `d82d6ab`), and the
maintainer explicitly authorized merging the approved candidate (PR #56,
squash-merged at candidate head
`41ece090737bd9b899ee2d8c9fd79bae70946f3f`) and this activation. This
contract does not claim implementation completeness: activation is a
documentation-only change and performs no Rust implementation. It remains the
sole current technical specification until a successor is separately
approved; no successor schedule is implied. The approved proposal file
[`proposals/a8-single-hart-atomic-convergence.md`](proposals/a8-single-hart-atomic-convergence.md)
is preserved unchanged as the archived snapshot source of this contract; it
retains drafting history and is not a second active contract.

**Activated:** 2026-09-21

**Evidence baseline:** `c90e455bdfe2a0fa87b5210a8e1d3ef9e0697e22` (merged
post-A7 roadmap PR #55 head), source inspected 2026-09-20 for the approved
proposal. Every current-behavior claim below cites the inspected source or a
named test; the proposed behavior is contract scope, not an implementation
claim. This change is documentation-only: no runtime, test, ADR,
historical-evidence, or `.qing/config.toml` change is part of this activation.

**Direction:** this contract realizes Stage 1 (single-Hart physical/atomic
convergence) of the approved-for-planning
[post-A7 roadmap](proposals/post-a7-roadmap.md), leaving Stage 2 (Hart facts and N=1
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

The architecture is unchanged. No ADR is rewritten or amended by this contract:

* [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md)
  keeps owning Hart outcomes, retirement/trap rules, and the Hart/Platform
  LR/SC ownership split (ADR-0001 §6 "LR/SC ownership boundary").
* [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md)
  already requires the atomic operation envelope, indivisible AMO/SC, Hart-owned
  reservation state, committed-competing-write visibility, and capability-based
  rejection (ADR-0002 §§1, 4, 5, 6). A8 implements that accepted atomic portion
  for one selected domain. Retained non-conformance elsewhere is labeled debt,
  not an ADR exception.
* [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md)
  owns composition; A8 adds no Machine lifecycle and claims none.
* [ADR-0004](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
  owns time/scheduling; A8 adds no time consumer, interrupt, or WFI behavior.

A7's rejected option B stays rejected: existing successful atomic programs are
not disabled, rejected, or reclassified as unsupported-instruction failures as
a shortcut. Where the approved profile changes an old result, the change is
enumerated in §6 with exact old→new expectations and affected fixtures;
§11 records the approved profile values. There is no blanket disable.

Activation record: the seven §11 choices were approved on 2026-09-21, and
this contract was separately authorized and activated on 2026-09-21 as the
sole Current contract by the rolling replacement of `docs/dev-plan.md`. A7
was formally accepted and closed before activation; its
[full contract](archive/milestones/a7-non-atomic-physical-access-migration.md)
and [closeout record](archive/milestones/a7-closeout-record.md) remain
archived history. The approved proposal file
[`proposals/a8-single-hart-atomic-convergence.md`](proposals/a8-single-hart-atomic-convergence.md)
is preserved unchanged as the archived snapshot source of this contract, not
a second active contract. Activation is a documentation change: it claims no
implementation and starts none.

## 2. Normative source decision for the atomic profile

**Selected specification:** the *RISC-V Unprivileged ISA Specification*,
2024-04-11 release (`riscv-isa-manual` tag `v20240411-DRAFT`) — the same
manual revision this repository already pins in
[ADR-0004 §17](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
for Zicntr — for the "A" extension (Zalrsc LR/SC and Zamo AMO) instruction
tables, result semantics, and alignment rules. The ratified 2019-12-13 edition
is a cross-check; its AMO/LR/SC encoding tables are identical. Where this
contract cites a rule as **spec-mandated**, the cited manual revision is the
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

**Reference investigation (pinned, replaces the earlier evidence
limitation):** the maintainer supplied a reference investigation against
`riscv-isa-manual` @ `b84f402bd6c436d984ca9ee85e2d4024293d5fe5`
(`src/unpriv/zalrsc.adoc`, `src/unpriv/zaamo.adoc`) and `riscv-isa-sim`
(Spike) @ `02b1dc182164bb73b19b050676dd89f0834f8b2e` (`riscv/mmu.h`,
`riscv/mmu.cc`, `riscv/sim.cc`, `riscv/simif.h`). Anchor names below
(`sc_*`, `lr_*`, Zaamo notes) refer to that manual tree; its operative text
is authoritative for what this contract calls spec-mandated. Spike is cited
**only as an implementation reference**, never as specification. The one
residual specification gap: a *trapping* SC's reservation effect is not
explicitly fixed by `sc_reservation_invalidate` (it covers "executing an
SC"). §11 item 2 **approves retain**, citing that gap plus Spike's
observable retain-on-trap behavior (§5.4).

**SC retirement and failure checks — normative rule vs selected order.** The
pinned manual @ `b84f402bd6c436d984ca9ee85e2d4024293d5fe5` requires, via
`sc_retire_permission`, that SC not retire without passing memory-permission
checks; `sc_addr_not_in_reservation_fail` requires an SC outside its
reservation set to fail; `sc_failed_as_store` permits a failed SC to be
checked as a store; `sc_failed_side_effects` leaves translation side effects
unspecified; and SC failure returns a nonzero `rd` value without a memory
write. Those are the specification mandates. The manual does **not** specify
our backend ordering or envelope representation.

The implementation references are not normative: Spike @
`02b1dc182164bb73b19b050676dd89f0834f8b2e`, `riscv/mmu.h:274–303`, performs
alignment, translation, and reservable checks before comparing reservation
equality; Sail @ `8890da7`,
`model/extensions/A/zalrsc_insts.sail:65–80` and
`model/sys/vmem_utils.sail:233–310`, performs alignment/translation and
PMA/PMP checks even when the reservation mismatches. A8 selects the
corresponding target-check order: after Hart legality/alignment and
guest-to-port address conversion, submit the existing SC envelope (with
optional reservation context); validate the complete target store span and
atomic capability first, then report reservation-based conditional failure.
This selected order is an implementation choice, **not** an extra spec
mandate. A8's physical target validation does not add the unwired MMU/PMP
permission path or claim it is implemented. In valid mapped RAM, absent or
uncovered reservation returns conditional failure without reading/writing
guest bytes or bumping bookkeeping; an unmapped or unsupported target faults
even without a reservation. Exact old→new cases are in C13/C21.

**Profile freedom selected by this contract (approved, §11):** SC failure
code value (1); reservation-set granularity and key (exact reserved byte
span, keyed by the port-issued physical address); number of outstanding
reservations per Hart (one; a successful LR replaces any prior reservation);
deterministic SC success conditions (stronger than the manual's "may fail
for any reason" latitude) with **P2a-precise** bookkeeping (only overlapping
committed writes fail SC); faulting-SC reservation effect **retain** (§5.4);
`aq`/`rl` implementation strength (no additional ordering effect, §5.3);
HTIF **D-c** (LR/SC rejected; AMO allowed as one indivisible envelope);
misaligned accesses raise the misaligned-address exception (not access
fault), preserving the current Hart precheck classification.

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
observation). `RiscVSimulator::memory()` exposes the shared handle: a caller
can write through a clone **between runs on the same thread** or **from
another thread during a run**; `run(&mut self)` and the borrow checker
serialize only same-thread facade calls, not handle clones. A run is not a
lifecycle boundary — `run`/`step` remain callable after any return, and the
native exit latch is reset "for potential re-use" (`src/executor.rs`,
`observe_htif`). Image installation happens between
runs. No debug, TLM, or MMU writer is wired into the public path.

### 3.4 Device atomic behavior and old typed surface

The typed `SystemBus` handles HTIF **only in its dword methods**: `read_dword`
returns 0 and `write_dword` fires the exit callback at any *starting* address
inside the window (`is_htif` start-address check), while `read_word`/
`write_word`/`write_half` have no HTIF branch and return `InvalidAddress`
(verified in `impl MemoryInterface for SystemBus`). These are **typed-helper
observations**; the architectural baseline below is what the **Hart** does
with each encoding, because two Hart mechanisms sit in front of the helpers:
the encoded-width alignment precheck (`data_access` maps AMO `funct3`
2→4/3→8 and traps load/store-address-misaligned **before any access**, with
SC and read-modify-write AMOs classified store-class by `is_load_reserved`)
and the reservation test inside `exec_sc`. All SC-family successes therefore
additionally require a live reservation at the same address — in practice
established by the preceding buggy LR.W dword read. Verified baseline
(`base = 0x40008000`, so `base` is 8-aligned and `base+4` is 4-aligned only):

| Legacy encoding at HTIF | At `base` | At `base+4` (interior) |
| --- | --- | --- |
| LR.W (`00010`,`010`) — width-4 precheck passes at both | `exec_lr`→`read_dword` → **retires**, rd=0, reservation set | same |
| LR.D (`00010`,`011`) — width-8 precheck | `exec_lr_w`→`read_word` → **load access fault** (no HTIF branch) | **load-address-misaligned trap**, no access |
| SC.W (`00011`,`010`) — width-4 precheck passes at both | `exec_sc`→`write_dword` → **retires rd=0 + callback** with reservation; rd=1, no callback without | same (the characterized `sc.w@htif+4` row) |
| SC.D (`00011`,`011`) — width-8 precheck | `exec_sc`→`write_dword` → **retires rd=0 + callback** with reservation | **store-address-misaligned trap**, no access |
| Malformed `00010`+`rs2!=0`,`010` — width-4 | as SC.W at `base` | as SC.W at `base+4` |
| Malformed `00010`+`rs2!=0`,`011` — width-8 | `exec_sc_w`→`write_word` → **store/AMO access fault (cause 7)** | **store-address-misaligned trap** |
| Dispatched AMO helpers (`00001`,`00100`,`00110`,`00111`,`01000`,`01001`,`01010`,`01011`), width-4 precheck passes | `read_word` → `InvalidAddress` → **store/AMO access fault (cause 7)** (store-class per `data_access`) | same |
| Unsupported `funct5` AMOs (`00000`,`01100`,`10000`,`10100`,`11000`,`11100`) | **`SimulatorFailure` (unsupported legal instruction)** before any memory access | same |

Only the LR.W-via-dword-bug and SC.W rows are demonstrated by the existing
fixtures (`tests/a7_migration_characterization.rs`, transcript row
`sc.w@htif+4=retired/dword-callback` uses `lr_encoding(…,0b010)` = LR.W;
`tests/a7_legacy_atomic_compat.rs::successful_legacy_sc_width_debt_mmio_callback_is_retained`);
real LR.D, SC.D, and AMO at HTIF have **no** A7 fixture. The raw native port
is stricter (exact 8-byte endpoint, dword-only, no fetch)
but has no atomic category: `src/physical.rs` states there is intentionally
no atomic envelope. `RiscvCore::new` (typed-only constructor),
`ruscv_sim::execute::{clear_reservation, exec_*}` helpers, `ReservationSet`,
`run_until_exit`, `reset_core`, and `step_once` remain public typed surfaces.

### 3.5 Evidence boundary

A7's recorded implementation evidence (1,667 full Rust/doc results, 354
focused tests at `fb6f51c`, fresh 51/51 project guests, separately scoped
frozen ACT4 51-case run) is historical, bound to its exact heads, and is not
relabelled by this contract. The two 51-case sets remain distinct and neither
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

* **M1 (recommended) — indivisible critical-section envelope: Hart-owned
  transform, Hart-owned reservation, domain-executed conditional check.**
  An atomic-category `PhysicalRequest` carries `{paddr, width (4|8), kind:
  RMW | LoadReserved | StoreConditional, aq/rl bits (informational)}`.
  An RMW additionally carries the operand bytes and **the Hart-supplied pure
  transform** — one function, produced by the single Hart-owned AMO
  arithmetic module (swap/add/and/or/xor/min/minu/max/maxu with W/D
  extension rules), mapping old bytes to new bytes; the backend executes
  exactly one locked critical section (read span → apply Hart transform →
  write result) and returns the exact old bytes. **LR** is one locked read
  that returns the old bytes **plus the committed-write version snapshot**
  of the covered blocks (§5.4). **SC** is one locked **conditional** write:
  the request carries optional Hart reservation context (reserved span + the
  LR-time snapshot); for an absent or uncovered context, the target returns
  conditional failure only after target validation. For a covered context,
  inside the same critical section the backend re-checks the snapshot against
  current bookkeeping and either performs the single write (bumping
  bookkeeping) or returns conditional failure with no write.
  This split follows ADR-0002 §6 — the Hart owns per-Hart reservation
  state and the architectural SC result; after Hart legality/alignment and
  guest-to-port address conversion, it issues the existing SC kind with an
  optional reservation context. The target validates the store span and its
  atomic capability before it reports conditional failure for absent or
  uncovered reservation state, and when valid performs one atomic write,
  returning conditional status. The domain's version bookkeeping is *not* a second reservation
  authority: it stores no per-Hart architectural state and produces no `rd`
  value — it is the open "mechanism by which the domain reports the
  competing write" that ADR-0002 §6 explicitly leaves to implementation.
  The transform's concrete representation (closure vs Hart-interpreted
  descriptor) remains a T1 mechanism choice. The snapshot/bookkeeping
  representation is **not** unconstrained: after approved P2a-precise, every
  T1 bookkeeping form — per-block or striped counters, a bounded write
  journal, or any equivalent — **must preserve exact reserved-span overlap
  semantics**. An unrelated committed write outside the reserved span must
  not fail SC. A standalone coarse/global counter is **not** an available T1
  choice: it would fail SC on any committed write and contradict §5.4,
  C17/C20, and T3's required non-overlap success. A coarse counter may exist
  only as an **auxiliary** (for example a generation stamp) if the SC check
  still determines exact overlap and does not treat the auxiliary alone as
  invalidation. The contract also requires that the arithmetic exists once in
  the Hart layer, identically for every backend, and that the snapshot check
  and write are one critical section. The
  check→write gap cannot be staled by any granted writer: every granted
  writer (Hart stores, envelopes, `write_mem`, `memory()` handles, image
  loads) takes the domain's single outer mutex (§5.5, verified), which the
  SC critical section holds. "Two ordinary accesses wrapped as atomic"
  remains structurally impossible: AMO is one critical-section transaction,
  SC-success is one conditional write request, LR is one read request.
* **M2 — Hart-check-then-write (unconditional write envelope).** The Hart
  reads the version snapshot itself and then issues a plain write envelope.
  Valid only when no granted writer can run between the two calls. For
  same-thread hosts that holds within a step, but the flat facade exposes
  the shared `Arc<Mutex<SimpleMemory>>` through `memory()`, so a
  **cross-thread** writer can commit between the Hart's check and its write
  and the SC would succeed over a committed competing write. Rejected as
  the contract for that reason; M1 folds the check into the backend's
  critical section instead. (The earlier Draft also rejected a
  conditional-token envelope as a "second reservation authority"; that
  judgment is withdrawn as too strong — see M1's split above.)
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
domain, and host writes participate through the visibility mechanism of
§5.5 (they cannot reorder a single Hart's accesses; they only affect
reservation validity). This is a documented
stronger-than-required profile strength, not an RVWMO claim. FENCE behavior
is unchanged.

### 5.4 Reservation profile, timing, and visibility

Per-Hart reservation state lives in `CoreState` (participating in staged
state; `reset` installs a fresh `CoreState`, so reset clears it; facade image
reload constructs a new core, so reload clears it). The record is the exact
reserved byte span plus the committed-write version snapshot taken atomically
with the LR read: `{paddr issued at the port, width, snapshot}`. One
reservation per Hart; a successful LR replaces any prior reservation.

**Timing.** The LR envelope returns the old bytes **and** the current
write-version snapshot of the covered blocks in one critical section; the
reservation is established in staged `CoreState` and becomes architectural
only when the LR retires. A faulting LR (misalignment precheck, target
rejection, or other trap/failure before retirement) **establishes no new
reservation and preserves any prior reservation**: staged state is discarded,
so a successful LR at address A followed by a rejected LR at address B leaves
the reservation at A intact. At SC, after Hart legality/alignment, the Hart converts the guest address to
its port-issued address and issues **one conditional envelope** with the
optional reservation context (span + snapshot). No reservation is represented
by absent context; a live but uncovered reservation is carried intact. The
target first validates the complete store span and atomic capability, then
returns conditional failure for absent/uncovered context or re-checks the
snapshot and writes — all as one target operation. The Hart owns the
reservation record, consume-on-completion rule, and `rd` result; the domain
owns target validation, the critical section, version bookkeeping, and the
committed write. Invalidation is therefore *computed* at SC time from the snapshot —
no cross-step writer-notification channel into `CoreState` exists or is
needed; W1–W5 writers simply bump the storage bookkeeping when they commit.
Deterministic success rule (profile guarantee, **approved P2a-precise**):
an SC succeeds **iff** the reservation is present, covers the SC span, and
no committed write has overlapped the reserved span since the LR. A
non-overlapping committed write does not fail the SC. Executed SC consumes
the reservation on success and on conditional failure. Rejected writes never
bump bookkeeping (ADR-0002 §5.2: no invalidation from a write that did not
commit). SC failure writes `rd = 1` (profile value; current behavior).

**Target-check order and SC failure.** The preceding order is our selected
implementation policy, not a spec-mandated backend sequence (the separation
and pinned references are in §2). For valid mapped RAM, an SC with no context
or an uncovered live context completes as conditional failure without reading
or writing guest bytes, callback/exit effect, or bookkeeping bump; an executed
conditional failure consumes the live reservation, if any. A target that
rejects the complete store span or lacks SC capability instead faults before
conditional failure can be reported. Hart address-conversion rejection also
faults before a target request. The original guest address is `mtval`; traps
write neither `rd` nor retirement state and staged discard retains any live
reservation. A8 physical target validation is not MMU/PMP permission checking.

**Faulting SC — approved retain.** The Zalrsc anchor
`sc_reservation_invalidate` ("executing an SC invalidates") covers a
*completed* SC, conditional success or failure; it does not explicitly fix
the effect of an SC that raises an exception before any write. Spike's
`store_conditional` performs the load-reservation check and the store such
that both can raise before `yield_load_reservation` is reached, and
`yield_load_reservation` appears only at MMU construction, the
hart-interleave switch (`riscv/sim.cc:375`), and after a completed SC — so
an exception SC **retains** the reservation in the reference implementation.
§11 item 2 **approves retain**: a faulting SC performed no architectural
completion, so it consumed nothing; this also matches Spike. The current
code's observable retain remains an early-return accident (`exec_sc`
returns before its clear on write error — verified §3.2) and is now the
approved profile rule, not an accident to preserve as debt. T3 asserts
retain: a later SC after a trapping SC can still succeed if the reservation
is otherwise valid.

The reservation key is the **physical address issued at the port** (after the
single configured base conversion): in the native bus form that equals the
guest address (pass-through translation base, `AddressForm::Bus`), in the
flat form it is the storage offset. Both facades therefore share one rule;
the key never mixes address forms, and a moving image base cannot silently
alias reservations.

**Granularity, key form, and recorded deviations.** The Zalrsc anchor
`lr_reservation_set_size` permits an arbitrarily large reservation set, so
both Spike's and this contract's choices are legal. Spike tracks a single
exact `paddr` equality without recording a width (`riscv/mmu.h:286`,
`load_reservation_address == paddr`), so `LR.W`→`SC.D` at the same address
succeeds in Spike — and, verified in §3.2, in the current code:
`LR.W@p→SC.D@p` succeeds today while `LR.D@p→SC.W@(p+4)` fails today;
both flip under span-containment (C30). This contract's
**span-containment** rule (record
`{paddr, width}`; the SC span must be contained in the reserved span) is a
**deliberate, recorded stricter deviation** — rationale: one deterministic
rule for a single implementation, and a wider SC cannot partially overwrite
a narrower reservation. The `lrsc_same_address_and_size` eventuality
constraint applies to forward-progress-constrained LR/SC loops only and is
not a general validity rule. Two further anchor confirmations recorded here:
`sc_failure_code` — the failure code is unspecified, any nonzero value is
legal, so this profile's `rd = 1` is a profile value; `sc_failed_as_store` —
a failing SC may perform its permission checks as a store, which matches
this contract's store-class classification of SC (and `sc_failed_side_effects`
leaves translation side effects UNSPECIFIED, so the profile's retain choice
is not constrained by it).

### 5.5 Writer visibility policy (approved: P2a-precise)

Writer inventory at the baseline — every public entry point that can write
bytes the Hart can observe, while a run is live or between runs. Verified
facts the policy rests on (all source-inspected): a run is **resumable and
not a lifecycle boundary** — `RiscVSimulator::run`/`step` take `&mut self`,
return a result, and leave the simulator reusable; the native facade's exit
latch is even reset with the comment "Reset for potential re-use"
(`src/executor.rs`, `observe_htif`). In the flat facade the **same
`Arc<Mutex<SimpleMemory>>`** is shared by the public `memory()` handle, both
`NativeRamBackend` raw ports, and the core's typed handles
(`RiscVSimulator::new`); the native facade likewise shares one
`Arc<Mutex<SystemBus>>` between both raw ports and the typed interface
(`install_image_with_physical_ports`), so every granted writer serializes on
the domain's outer mutex. `SimpleMemory::load_program` writes the internal
`Vec` directly, bypassing the `MemoryInterface` write methods.
`RiscVSimulator::write_mem` commits a visible prefix before an out-of-range
byte fails (§6 C20; asserted by
`tests/a7_migration_characterization.rs::host_write_mem_keeps_empty_offset_and_prefix_on_failure_semantics`).

| # | Writer | Route | Proposed policy |
| --- | --- | --- | --- |
| W1 | Hart ordinary integer store | raw `DataWrite` port | In-domain: committed write bumps storage bookkeeping; invalidation computed at SC (§5.4). Same-Hart overlapping store ⇒ SC fails. |
| W2 | Hart FP store | raw `DataWrite` port | Same as W1. |
| W3 | Hart AMO / successful SC | atomic envelope | Same as W1; the envelope's own committed write bumps. |
| W4 | Host writes on the flat facade: `write_mem` (byte loop, `&self`), the exposed `memory()` handle (same-thread **between runs** and cross-thread **during** a run), and the committed prefix of a failed `write_mem` | typed write methods of the shared storage object | **Approved P2a-precise:** every write method of the storage object bumps committed-write bookkeeping after its commit point; a failed write's committed prefix bumps only the bytes that committed; rejected writes bump nothing. Host-write overlap ⇒ SC fails; **non-overlap succeeds**. |
| W5 | `clear_tohost` exit-signal clear | typed 8-byte loop at exit observation | **Participates like W4.** A run returning is *not* a Hart-lifecycle boundary (verified above), so a resumed run's SC must see the cleared bytes: overlap ⇒ SC fails; run→budget→resume with no host write ⇒ SC still succeeds. At the HTIF endpoint the clear writes are ignored (no bytes committed) ⇒ no effect. |
| W6 | Image (re)installation (`load_program`, `load_elf`) | between runs | Facade reload replaces the core ⇒ reservation cleared; `load_program`'s direct `Vec` writes must bump bookkeeping for non-facade callers of the storage object. |
| W7 | Raw access to storage internals outside the object's write methods | none reachable through the public API | Documented unsupported: outside the granted-writer set; any future such path enters only via a contract change. |
| W8 | Debug/TLM/DMA/device-originated writes | none wired | Out of scope; unchanged. |

**Why P1 (quiescent host writes, no invalidation) is withdrawn.** ADR-0002
§6 requires that a committed conflicting write through the same
physical-access domain "must be visible to the Hart so that an affected
reservation can be invalidated." A flat-facade host write commits bytes into
the *same storage objects* the Hart reserves; absence of concurrency does
not remove the cross-instruction effect (LR → host write → SC silently
loses the host's committed bytes), and `write_mem(&self)`'s difficulty
reaching per-Hart state is an implementation detail, not an ADR exception.
Approved **P2a-precise** makes host writes visible without moving reservation
authority: the bookkeeping lives in the storage object (per-block/striped
counters or a bounded write journal — a single-Hart mechanism; no multi-Hart
semantics are claimed or required), while the Hart still owns the reservation
record and the SC result. This document does not amend the ADR. Because every
granted writer serializes on the domain's one outer mutex
(verified above), a conditional SC envelope holding that mutex across
check→write cannot have its snapshot staled by any granted writer,
same-thread or cross-thread.

**Specification basis and recorded divergences.** The Zalrsc must-fail set
is exactly `sc_addr_not_in_reservation_fail`, `sc_other_hart_store_fail`,
`sc_other_device_write_fail` (a write from another hart or a device to the
bytes the LR accessed), and `sc_intervening_sc_fail`; **the specification
imposes no requirement for same-Hart writes**, and Spike performs no
same-Hart invalidation at all. Two consequences are recorded explicitly:
(i) P2a's host-write visibility is **anchored**: a host write is an external
agent writing the LR-accessed bytes, the `sc_other_device_write_fail`
case, which also strengthens the ADR-0002 §6 argument above; (ii) this
contract's same-Hart overlapping-write invalidation (W1–W2, C17) is a
**deliberate divergence from both the minimum spec and Spike** — stricter,
closer to real hardware where a same-Hart store into the reservation set
clears it — chosen for deterministic single-Hart semantics; it is a profile
strength, not a spec requirement.

Verification for W1–W6 is by falsifiable reservation tests (§8 T3/T4);
nothing outside the table is silently admitted.

### 5.6 Device atomic capability (approved: D-c)

**UART:** atomic widths are 4 or 8 bytes and the public UART window is
byte-only, so the UART rejects atomic requests on width regardless of
policy; no callback question arises. What follows is about the HTIF
endpoint.

**What the source actually does with the callback.** The HTIF write callback
is a host-side latch: `load_and_run` registers a closure that stores an exit
code into an `AtomicU32`; the runner reads that latch only in its
after-step observers (`RunControl::after_completed` / `observe_htif`), i.e.,
at the completed-step boundary. Firing the callback inside an envelope is
therefore observably identical to an ordinary dword store firing it — the
existing retire-before-exit layering already prevents any mid-transaction
guest-visible exit. A callback does not by itself make an envelope unsafe.
The remaining semantic fact is that the endpoint has no backing store: reads
return zero bytes and writes invoke the callback (verified §3.4), so an
atomic "old value" at the endpoint is defined (zeros) exactly as today.

**Policy D-a — HTIF advertises atomic capability at the endpoint.** The
endpoint accepts atomic doubleword transactions at the exact 8-byte span
starting at the base, mirroring the ordinary raw-port rules: `LR.D` reads
zeros plus a version snapshot; a valid conditional `SC.D` performs one
callback-invoking write (bookkeeping bumped); `AMO.D` applies the Hart
transform to old-zeros and invokes the callback with the new value once.
Any other width or span is a target rejection → access fault of the access
class. Measured against the verified §3.4 baseline, D-a **enables** real
`LR.D` and `AMO.D` at the endpoint (both fault today — dispatched AMO
helpers as store/AMO access faults; unsupported `funct5` AMOs as simulator
failures before any access), **preserves**
`SC.D`-at-base conditional success (subject to a reservation — note the
legacy pairing used the buggy `LR.W` dword read as the reservation source,
which D-a removes), and **changes** `LR.W`/`SC.W` at the endpoint from
bug-induced dword successes into 4-byte rejections → access faults (the
characterized `SC.W`@base+4 row faults under D-a as well). Cost: a slightly
larger device contract (zero-read + single callback inside the critical
section) and the exit latch being set from inside an envelope (safe per the
layering above, and testable).

**Policy D-b — HTIF rejects all atomics.** Every LR/SC/AMO at the endpoint
is a target rejection before any callback or mutation → load access fault
(LR) or store/AMO access fault (SC/AMO) with the original guest address.
Measured against the baseline, D-b **changes** today's bug-induced
`LR.W`/`SC.W`/`SC.D` successes at the endpoint into faults and leaves the
already-faulting real `LR.D`/`AMO` unchanged — a behavior regression for
guests that reserve/store on `tohost` (unusual, but the legacy fixtures
prove the path), so it is not a free simplification. No policy
preserves the legacy LR/SC *pairing*: the legacy successful sequence is
buggy-`LR.W`-dword-read → dword-write SC, and its reservation source
(`LR.W`) faults under every policy.

**Reference behavior (Spike, pinned `02b1dc1`).** Spike's `reservable()`
is `addr_to_mem` — **RAM only**: an LR to an MMIO address raises a load
access fault (`riscv/mmu.cc:247`) and an SC to MMIO a store access fault
(`riscv/mmu.h:288`). AMOs, however, reach MMIO: Spike implements them as a
load followed by a store, and the Zaamo NOTE in the pinned manual states
that using AMOs to update MMIO registers is an intended use. This is the
basis of the new policy D-c.

**Policy D-c — approved (Spike reference surface: LR/SC rejected, AMO
allowed).** LR and SC at the HTIF endpoint are target rejections →
load/store-AMO access faults (as D-b); AMO at the endpoint is allowed as an
indivisible envelope: old value = the endpoint's zero-read, write side = the
callback invoked **exactly once** inside the critical section, bookkeeping
bumped. **Adaptation to our ADR:** ADR-0002 §5.5 forbids exposing an AMO as
a visible load-then-store pair, so D-c adopts Spike's *policy surface*
(which operations may touch the device) but not its two-step mechanism — the
AMO is our single critical-section envelope. Measured against the §3.4
baseline: D-c **enables** AMO at the endpoint (dispatched helpers trap
cause 7 today; unsupported encodings fail before access today and become
dispatched under the §5.3 decode repair), and **changes** the bug-induced
`LR.W`/`SC.W`/`SC.D` endpoint successes into faults exactly as D-b does.
§11 item 4 **approves D-c**.

**Policy comparison (compatibility effects).** D-a = endpoint dword
capability for LR/SC/AMO (enables `LR.D`/AMO, preserves `SC.D`-at-base,
faults `LR.W`/`SC.W`); D-b = reject everything (faults all of them, no
enables); D-c = reject LR/SC (faults, as D-b) + allow AMO (enables, as
D-a's AMO half). All three make the encoded-misalignment traps and
reservation preconditions policy-independent Hart behavior.

**Common Hart/device rules (independent of D-c):** after Hart legality,
alignment, and guest-to-port address conversion, every SC issues exactly one
StoreConditional envelope, whether the optional reservation context is absent,
covered, or uncovered. Valid RAM validates the complete store span and then
returns conditional Failure for absent/uncovered context: `rd = 1`, no guest
byte read/write, no bookkeeping bump, callback, or exit. A RAM-end/out-of-RAM
span, unsupported target, or target without SC capability rejects before that
failure result and maps to store/AMO access fault (cause 7) with original guest
`mtval`; no partial effect or `rd` write occurs, and a live reservation is
retained on the trap. Hart misalignment and guest-to-port conversion failures
still precede envelope dispatch. There is **no blanket disable of atomics**:
RAM remains atomic-capable, UART rejects atomic widths, and HTIF under approved
D-c rejects LR/SC while allowing AMO. Third-party raw backends must validate
target capability before returning conditional failure.

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
| C13 | SC reservation failure, before target check | On the starting standard physical route, no reservation: Hart returns `rd=1` without address conversion or request, even for an invalid target. Live but uncovered span: conversion occurs first, then Hart returns `rd=1` without a target request. On flat RAM, no-reservation SC below base or at an odd storage offset therefore retires `rd=1`; the same conversion failure with a live reservation faults cause 7 and retains it. All conditional failures leave bytes unchanged. | On the standard physical route, every legal aligned SC first converts the guest address and submits exactly one StoreConditional envelope with optional context. Valid RAM + absent/uncovered context → conditional Failure (`rd=1`, no guest-byte read/write, callback/exit, or bookkeeping bump); completed failure consumes an existing reservation. Flat below-base/odd-offset conversion failure → cause-7 fault with original guest `mtval`, before an envelope (no reservation to retain, or live reservation retained). UART, HTIF, or unmapped target rejection → cause-7 fault with original guest `mtval` after one envelope, no retirement/`rd` write or device/callback/exit effect; a live reservation is retained. `rd=x0` suppresses only the register write. | Old no-request behavior flips. `sc_failure_code` still permits any nonzero failure value; this profile keeps 1. Covered-success and span-containment rules remain C14/C30. W/D tests include valid RAM with no/disjoint reservation and below-base/odd-offset, UART, HTIF D-c, and unmapped cases with zero side effects. **Result change.** |
| C14 | SC success | `rd=0`, write, reservation cleared | Same; reservation keyed per-Hart on port paddr with recorded width | Unchanged **only for same-width, same-address pairs** (LR.W→SC.W@p, LR.D→SC.D@p); cross-width/address pairs change in both directions per C30. |
| C15 | SC target/access fault with a live reservation | A covered SC whose target store fails traps store/AMO access-fault and retains its reservation: the legacy typed helper returns before clear; the standard physical route discards staged Hart state on target error. An unreserved or uncovered SC never reaches the target under the old Hart precheck (C13). | Any faulting SC that had a live reservation (including target permission/capability rejection after an uncovered SC is submitted) does not retire or write `rd`; it retains that reservation by staged-state discard, so a later SC can still succeed if otherwise valid. With no reservation there is nothing to retain. `sc_reservation_invalidate` covers completed SC only; Spike also retains on trapping SC. | Retained live-reservation behavior remains, now explicitly approved rather than relying on either implementation path. New no-reservation target faults are listed in C13/C21. |
| C16 | Reservation ownership | Process-global exact-guest-address singleton; survives reset/reload; shared across Core instances | Per-Hart `CoreState`, port-paddr byte-span key; reset/reload clear; new LR replaces | Rows `lr->other-core-sc`, `lr->reset->sc`, `…retains_legacy_lr_key` flip to `rd=1`. **Result change.** |
| C17 | Ordinary/FP store between LR and SC | No invalidation; SC succeeds and overwrites | Overlapping committed write ⇒ SC fails `rd=1`; non-overlapping write: SC **succeeds** (approved P2a-precise) | Same-Hart invalidation is a **deliberate divergence** from the spec minimum and from Spike (§5.5), not a spec requirement. Rows `lr->sd->sc`, `lr->fsd->sc` flip for the overlapping case. **Result change.** |
| C18 | AMO write between LR and SC | No invalidation | Overlapping committed AMO write invalidates | New rule; consistent with C17. |
| C19 | Failed/rejected ordinary write between LR and SC | Reservation retained | Retained (no commit → no invalidation) | Unchanged. |
| C20 | Host writes between steps/runs (`write_mem`; `memory()` handle same-thread between runs or cross-thread during a run; committed prefix of a failed `write_mem`) | No invalidation (row `lr->host-write_mem->sc`) | **Approved P2a-precise:** committed overlapping write ⇒ SC fails `rd=1`; non-overlap succeeds; a failed write's committed prefix invalidates iff its committed bytes overlap | Row `lr->host-write_mem->sc` flips. **Result change.** |
| C21 | LR/SC/AMO on HTIF endpoint (per-encoding Hart baseline in §3.4) | `LR.W` at 4-aligned endpoint addresses succeeds via its dword-read bug and sets the reservation; aligned+reserved SC encodings succeed via the dword callback (SC.W at `base`/`base+4`, SC.D at `base`); SC.D@`base+4` and LR.D@interior trap encoded misalignment before any access; real LR.D@`base` faults load-access-fault; dispatched AMO helpers trap store/AMO access fault (cause 7); unsupported `funct5` AMOs are simulator failures before any access. Under the starting A8 Hart code, no-reservation or live-uncovered SC skips the target and retires `rd=1`. | **Approved D-c:** LR at HTIF faults load-access-fault; every legal aligned SC (no reservation, live-uncovered, or covered) issues one envelope and HTIF rejects LR/SC by category → store/AMO access fault (cause 7), no callback/exit/mutation/retirement/`rd` write; faulting live reservation is retained. AMO at the endpoint remains one indivisible envelope (zero-read old value, callback exactly once). Encoded misalignment and guest-to-port conversion failures occur before envelope dispatch. | Old aligned SC callback successes flip to faults; additionally no-reservation/live-uncovered SC at base flips from `rd=1` retirement/no request to target cause-7 fault/one request. LR.W endpoint flip and real LR.D stay as specified; AMO is **enabled**. HTIF callback and exit remain untouched for rejected SC. **Result change.** |
| C22 | LR/SC/AMO on UART | Byte-only typed path: wide ops already fail | Same observable (atomic rejected) with access-fault mapping | Unchanged in effect. |
| C23 | `GLOBAL_RESERVATION`, `ruscv_sim::execute::clear_reservation`, `ReservationSet` public surface | Public process-global reservation API | **Approved:** remove `GLOBAL_RESERVATION` and `clear_reservation`; reservation state becomes per-Hart (`CoreState`); `ReservationSet` is kept as the per-Hart record type | Test helpers migrate; 0.x breaking change. |
| C24 | Old typed `RiscvCore::new` route for AMO/LR/SC | Typed read/write pair with global reservation | Typed pair **retained as labeled non-conforming compatibility adapter**, using the same per-Hart reservation state (one reservation authority); standard facades never use it | `amo_test.rs`/typed helper tests keep passing; doc labels the route. |
| C25 | Standard facades' atomic route | Legacy typed bridge | Atomic envelope through the raw data port; bridge removed from facades | Mixed ordinary/atomic equivalence tests. |
| C26 | Misaligned AMO/LR/SC | Hart precheck → load/store-address-misaligned trap, no physical request | Same (`zaamo` uses the same alignment rule as `lr_sc_alignment`; Spike raises load-address-misaligned for LR and converts AMO load-side traps to store class via `convert_load_traps_to_store_traps`, matching the draft's cause-7 AMO mapping) | Unchanged. |
| C27 | `rd=x0` | No `rd` write, memory op occurs | Same | Unchanged. |
| C28 | Budget/exit/observation (A6) | Started-slot budget, retire-before-exit, commit log no-refetch, `mem_access=None` | Unchanged | A6 regressions must stay green. |
| C29 | Run-boundary resume (`run`→exit→`clear_tohost`→`run`; `run`→budget→`run`) | Reservation persists across runs; `clear_tohost`'s 8 committed bytes and any between-run host write do not invalidate; post-resume SC succeeds regardless | Runs are not lifecycle boundaries (verified §3.3/§5.5); `clear_tohost` bytes and between-run writes participate like C20: overlap ⇒ post-resume SC fails, budget-resume with no host write ⇒ SC succeeds | New falsifiable tests (T3/T4); the earlier Draft wrongly assumed no step follows a clear. **Result change (clear-overlap case).** |
| C30 | Cross-width/address SC validity (aligned RAM) | Key is exact starting address, no width recorded (`reserved_addr: Option<u64>`): `LR.W@p→SC.D@p` (p 8-aligned) **succeeds** today; `LR.D@p→SC.W@(p+4)` **fails** today | **Approved span-containment:** `LR.W@p→SC.D@p` **fails** `rd=1` (8-byte SC span ⊄ 4-byte reservation); `LR.D@p→SC.W@(p+4)` **succeeds** (4-byte span ⊆ 8-byte reservation, no competing write) | Both directions are behavior changes (recorded Spike divergence, §5.4). T0 baseline + T3 acceptance. **Result change.** |

## 7. Bridge exit, adapters, and what "ADR-0002 atomic portion attained" means

### 7.1 Exit scope

The legacy typed atomic route is removed from `load_and_run`/CLI and
`RiscVSimulator` (the two standard facades). "Removed" means: no
AMO/LR/SC instruction on those facades issues typed `MemoryInterface`
read/write calls; every legal, aligned operation whose guest address converts
issues exactly one envelope request through the validated data port. Hart-side
legality, misalignment, and guest-to-port conversion failures issue **zero**
physical requests (illegal encodings, misaligned AMO/LR/SC, conversion faults);
no-reservation and uncovered SC are not pre-target rejections (§5.2/§5.4/C13).
Split ordinary read/write pairs remain forbidden. The old typed constructor
(`RiscvCore::new` without ports) keeps the typed pair behavior explicitly
labeled: it is a compatibility adapter, not a conforming atomic backend, and
it shares the one per-Hart reservation authority (C24). It has no physical
target permission or atomic-capability validation; tests of this constructor
do not fabricate such support.

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
device targets beyond the approved D-c HTIF policy (§11 item 4). "Explicit
writer visibility" means precisely: every writer listed in §5.5's inventory
has a tested reservation effect under approved P2a-precise, and the
old typed constructor route remains explicit non-conformance (its
`MemoryInterface` backends carry no committed-write bookkeeping, so no
visibility guarantee is claimed for them). ADR-0002's atomic portion is
attained **for the single-Hart native domain of the standard facades**.
Any future TLM/SystemC adapter must provide the envelope or reject the
capability before mutation.

## 8. Tasks, dependencies, and executable acceptance criteria

The tasks define the approved implementation scope; this activation performs
none of them. New test names are deliverables, not existing passes. Each
increment runs its narrow row first; all rows run at the final reviewed head.
Missing/skipped required tools block acceptance.

| Task / dependency | Deliverable and checks | Command / exit criterion |
| --- | --- | --- |
| T0 — fixture reclassification ledger and baseline (no behavior change) | Commit `docs/verification/a8-fixture-reclassification.md` mapping every A7 atomic fixture row to **keep / flip / retire / relabel** with the new expected value and the approving §6 row; add `tests/a8_atomic_baseline.rs` retaining the verified pre-A8 expectations for unaffected and explicitly typed-compatibility rows; C13/C21 standard physical-route SC rows follow the correction below rather than treating the old Hart short-circuit as normative (dispatch table, LR/SC width reversal, global reservation rows, and the §3.4 HTIF per-encoding **Hart-outcome** baseline — LR.W at 4-aligned endpoint addresses succeeds via its dword-read bug and sets the reservation; real LR.D at `base` faults load-access-fault and at interior offsets traps load-address-misaligned before any access; SC-family callback success requires encoded alignment plus a live reservation (SC.W at `base`/`base+4` and SC.D at `base` succeed; SC.D at `base+4` traps store-address-misaligned); dispatched AMO helpers trap store/AMO access fault (cause 7); unsupported `funct5` values are simulator failures before any access — since no A7 fixture covers the LR.D/SC.D/AMO or cross-width SC rows — including `LR.W@p→SC.D@p` succeeding and `LR.D@p→SC.W@(p+4)` failing today under the width-less exact-address key, C30) so each later flip is a diff against an executable baseline. The ledger also records **Spike reference-evidence rows** (pinned `riscv-isa-sim` @ `02b1dc1`, documentation rows, not tests of our code): trapping SC retains the reservation; no same-Hart write invalidation; single exact-paddr reservation key without width (`LR.W`→`SC.D` same address succeeds); `reservable()` is RAM-only with LR/SC-to-MMIO faulting; AMO reaches MMIO as load+store. | `cargo test --test a8_atomic_baseline --test a7_migration_characterization --test a7_legacy_atomic_compat --test amo_test`; exit: ledger complete, baseline green, no unresolved classification. |
| T1 — atomic envelope vocabulary, after T0 | Extend `src/physical.rs` with the atomic category: RMW/LoadReserved/StoreConditional request kinds, the Hart-supplied pure-transform representation (§5.2 M1), operand/result bytes, optional SC reservation context, conditional status, validation and response binding; update the module doc that today denies an atomic category. `tests/a8_atomic_contract.rs`: widths 4/8 only, payload rules, transform application for every AMO operation through the one Hart-owned arithmetic module, binding/completion validation (SC Failure allowed without context; Success without context is a protocol failure), single-backend-call, unknown completion terminal with no retry, malformed envelope rejection — all at the vocabulary level without targets. | `cargo test --test a8_atomic_contract --test a7_physical_contract`; exit: envelope taxonomy complete; no ordinary read/write pair can represent an AMO/SC; A7 non-atomic contract tests unchanged. |
| T2 — native targets, after T1 | RAM executes the critical-section primitive — one locked read → Hart-supplied transform → write transaction with exact old bytes, exactly-once effect, complete-span/width validation, no partial write — and implements no ISA arithmetic; it validates each SC target span before conditional Failure for absent/uncovered reservation (no guest-byte read/write or bookkeeping bump); the UART rejects atomic requests on width (policy-independent, byte-only window); the HTIF endpoint implements **approved D-c**: LR/SC rejected before conditional failure/mutation (store-access fault, no callback); AMO allowed as one indivisible envelope with callback-exactly-once. `tests/a8_atomic_targets.rs`: success/negative spans, overflow, unsupported width/category, no-context and uncovered SC Failure on valid RAM with zero side effects, rejection leaves RAM/registers/FIFO/callback/exit unchanged and fires no callback; **D-c proves AMO envelope callback-exactly-once while no-context/covered LR/SC fault without callback**; a recording spy target proves **one locked target-visible transaction** per AMO/SC whose internal read and write are not separately observable by a competing reader view; an arithmetic-parity test drives two different conforming backends through the same Hart transform and asserts byte-identical results for every operation/operand pattern; a recorded source/route audit shows no backend or target module references ISA operation semantics; lock-poison → host failure. Bookkeeping tests: every write path (typed write methods, raw `write_bytes`, `load_program`) bumps after commit; rejected writes and failed-write suffixes bump nothing; a failed host write's committed prefix bumps exactly the committed bytes; **an unrelated committed write outside the reserved span must not fail SC** (P2a-precise; a standalone coarse counter is not a valid T1 form). | `cargo test --test a8_atomic_targets --test a7_native_targets --test memory_bounds --test executor --test peripheral_tests`; exit: native critical-section + bookkeeping + D-c HTIF policy proven, arithmetic ownership stays Hart-side, ordinary native behavior unchanged. |
| T3 — Hart dispatch, reservation, writers, after T2 | Rewrite `execute_amo` per §5.3 (spec table, AMOSWAP, W/D, reserved → illegal); per-Hart reservation in `CoreState` with §5.4 profile (key/span/consume/**faulting-SC retain**/reset/reload); envelope issue via the data port for port-configured cores; invalidation from W1–W3 committed overlapping writes; typed compatibility route on the old constructor sharing the reservation state; remove `GLOBAL_RESERVATION`/`clear_reservation`. `tests/a8_hart_atomic.rs`: **an exhaustive decode matrix enumerating all 32 `funct5` values × W/D** — every assigned encoding (incl. AMOMINU `11000`, AMOMAXU `11100`) retires its named operation and every reserved value (incl. the previously mis-listed `10001`/`10101`) raises illegal instruction — plus LR-`rs2!=0` malformed encodings; W/D results and sign extension; `rd=x0`; alignment precheck with zero physical requests; aq/rl combos retire; SC success; no-reservation valid-RAM failure and live uncovered/disjoint failure with one envelope; W/D consume-on-completion; unsupported/unmapped target faults; below-base/odd-offset conversion faults with original `mtval`; faulting-SC retain; the C30 cross-width transitions (`LR.W@p→SC.D@p` fails `rd=1`; `LR.D@p→SC.W@(p+4)` succeeds); reset/reload clear; two-Core reservation isolation (test-object evidence, labelled not multi-Hart support); writer table W1–W3 negative/positive; typed-adapter route labeled non-conforming and tested without claiming target permission/capability support; no re-entrancy; **writer/visibility suite**: LR-time snapshot and consume timing — a faulting LR establishes no new reservation and **preserves any prior reservation** (empty-start case: still none; `LR@A` then rejected `LR@B` then `SC@A` succeeds when no conflicting write occurred); same-Hart overlapping vs non-overlapping store; host `write_mem` overlap fails and non-overlap succeeds (**approved P2a-precise**); `memory()`-handle write between runs; committed prefix of a failed host write; cross-thread handle writer excluded by the domain mutex during a conditional SC; run→exit→`clear_tohost`→resume overlap fails and budget-resume succeeds; faulting SC **retains** (later SC can still succeed). | `cargo test --test a8_hart_atomic --test a8_atomic_baseline --test amo_test --test a7_migration_characterization --test a7_legacy_atomic_compat --test a6_task3_core_trap_test --test trap_test --test csr_access_test --test mret_conformance_test`; `cargo test --lib isa::rv64a`; exit: flipped fixtures assert new expectations per ledger; unchanged rows stay green; single reservation and arithmetic authority evidenced. |
| T4 — facade bridge exit and public equivalence, after T3 | Standard facades issue envelopes only; mixed ordinary+atomic guests identical across CLI/`load_and_run`/flat within documented configuration differences; approved P2a-precise writer visibility and D-c HTIF policy asserted at the facade level, incl. between-run host-write invalidation and resume-after-exit/budget; valid-RAM no-reservation SC conditional failure without mutation; no-reservation/live-uncovered SC to invalid HTIF/UART/unmapped targets faults without callback/exit; A6 budget/exit/observation regressions. `tests/a8_public_atomic_equivalence.rs`: mixed ELF fixtures, exit-after-atomic ordering (retire before platform exit), zero/exact/final-slot budgets, rejected atomic produces no exit, artifacts/reload differences retained, commit log no-refetch unchanged, HTIF D-c cases (LR/SC access-fault with no callback for no-reservation, uncovered, and covered SC; AMO envelope callback-exactly-once). New project-authored bare-metal atomic guests (LR/SC loop, AMOSWAP/ADD/MIN/MAX W/D, aq/rl set, SC-after-store failure, atomic near RAM end, atomic+tohost exit) added to the fresh-build guest suite. | `cargo test --test a8_public_atomic_equivalence --test a7_public_equivalence --test public_behavior --test a4_integrated_equivalence --test a4_run_control --test executor --test commits_test --test cli_test`; exit: no typed atomic call from standard facades (route audit recorded), equivalence within documented differences, new guests fresh-compiled and passing. |
| T5 — evidence and bounded closeout, after T0–T4 | Full gate below; fresh project ELF suite = the retained 51 guests **plus** the new atomic guests with a recorded new total (the historical 51 stays recorded as its own identity); frozen ACT4 51-case selection **unchanged and separately recorded** — no A-extension claim, no merged counts; residual-debt ledger updated (MMU/TLM/performance/Stage 2 items). The SC correction follow-up records exact-HEAD `qing verify` evidence only on a clean committed HEAD; it is not an A8 milestone closeout or an earlier-head evidence carry-forward. | See commands below; exit: exact-head evidence complete, no unapproved compatibility delta beyond §6's approved rows, §7.3 statement only. |

Final implementation commands (required at the final committed task HEAD;
earlier-head evidence is not transferable):

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
bash scripts/test_riscv_elf_guards.sh
RISCV_REQUIRE_RISCV_TOOLCHAIN=1 cargo test --test a6_trap_elf_integration
head=$(git rev-parse HEAD)
docker run --rm --init --env RISCV_SOURCE_HEAD="$head" \
  --volume "$PWD:/workspace" --workdir /workspace \
  ghcr.io/mimiqdev/ruscv-sim-dev:main bash -c \
  'export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/a8-sc-container-cargo RISCV_TEST_OUTDIR=target/a8-sc-fresh-riscv-elves RISCV_SOURCE_HEAD="${RISCV_SOURCE_HEAD}" RISCV_REQUIRE_RISCV_TOOLCHAIN=1; ./scripts/run_elf_tests.sh'
```

Record toolchain/container identity, exact revision, commands, and all 58
per-case results; the container invocation mounts this worktree at
`/workspace`, uses non-login `bash -c` and separate target/output directories,
and passes `RISCV_SOURCE_HEAD` because the linked `.git` is not container
resolvable. Preexisting ELFs are not a fresh build. Run
`git diff --check <base>..HEAD` and record the observed checks with `qing verify`
on the clean committed HEAD. No new ACT4 run is authorized; its replay
remains historical. Any **new** atomic differential/oracle selection (for
example a pinned Spike/Sail comparison) is a separate explicit proposal and
is not authorized by this contract.

## 9. A7 fixture reclassification (classification contract for T0)

The A7 atomic-preservation fixtures do not silently carry forward as
specification. Classification (each lands in the T0 ledger with new expected
values):

* **Flip (expectation changes, fixture retained):**
  `opcode_funct5_dispatch_preserves_current_amo_width_debt` (both sub-cases:
  `00001/W` becomes AMOSWAP semantics; `00001/D` becomes full-width swap),
  transcript rows `amoadd.w=…`, `lr->other-core-sc=…`, `lr->reset->sc=…`,
  `lr->sd->sc=…`, `lr->fsd->sc=…`, `lr->reset->…`/`…retains_legacy_lr_key`
  (reset/reload rows), `lr->host-write_mem->sc=…` (flips under approved
  P2a-precise), `ordinary_store_and_fp_store_interleave_with_unchanged_legacy_lr_sc`,
  `legacy_lr_survives_reset_and_replacement_storage_with_global_key_behavior`.
* **Retire (tests removed debt machinery; replaced by new-equivalent
  assertions):** `sc.w@htif+4=retired/dword-callback` transcript row and
  `successful_legacy_sc_width_debt_mmio_callback_is_retained` (replaced by
  D-c HTIF tests: LR/SC access-fault with no callback, AMO envelope
  callback-exactly-once), the global-singleton mechanics of the
  isolated child transcript (replaced by per-Hart state tests; process
  isolation harness retained only if still needed).
* **Keep (expectation unchanged, relabel debt→profile where noted):**
  `sc.no-reservation=…`, `lr@b0->sc@b8=…`, `lr->failed-sd->sc=…` (rejected
  write no-invalidation), `sc.write-fault=…` (approved retain: same
  observable, relabeled accident→profile),
  `ordinary_store_then_legacy_amo_and_lr_share_one_migrated_domain`
  (same-domain visibility both directions),
  `rejected_ordinary_write_and_faulting_sc_preserve_characterized_reservation`
  (retain half),
  `legacy_write_is_visible_to_raw_load_fetch_signature_and_host_inspection`,
  `legacy_lock_reentry_child` (non-reentrancy, adapted to the envelope path),
  `amo_test.rs` helper-level tests (typed compatibility route).
* **New coverage with no A7 fixture (verified):** no A7 fixture exercises a
  real SC.D (`00011`,`funct3=011`), a malformed LR encoding
  (`00010`,`rs2!=0`), AMOMINU (`11000`) or AMOMAXU (`11100`), the cross-width SC
  sequences of C30 (`LR.W@p→SC.D@p`, `LR.D@p→SC.W@(p+4)`), **real `LR.D`
  at the HTIF endpoint (load access fault at `base`, encoded-misalignment
  trap at interior offsets), aligned+reserved `SC.W`/`SC.D` at the HTIF
  base (dword callback today), or any dispatched AMO at HTIF (store/AMO
  access fault today)** — the existing HTIF
  fixtures cover only the buggy `LR.W` dword read and `SC.W`@base+4. Their
  original Hart/decode old→new transitions (C10, C11, C4, C21) are captured by
  the T0 baseline rows and T3 exhaustive decode-matrix/width tests. C21's
  standard-route target rejection and side effects are also exercised by the
  T4 facade suite; the covered-HTIF reservation remains a seeded direct-core
  case because no public guest can reserve the D-c endpoint.
* **SC-follow-up public HTIF coverage (C13/C21/T4):**
  `tests/a8_public_atomic_equivalence.rs::native_public_facade_sc_fault_matrix_checks_mtval_rd_ram_uart_htif_and_exit`
  runs public LR.W/D-on-RAM → live-uncovered SC.W/D-on-HTIF guests, plus the
  no-reservation HTIF cases. Trap handlers check cause, original `mtval`,
  untouched `rd`, unchanged RAM, and (for live reservations) successful retry;
  the SC payload is an exit marker so a premature HTIF callback cannot be
  mistaken for a successful trap path. This supplements, but does not rewrite,
  the historical A7 transcripts.
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
an intermediate value; the full writer table including between-run and
resume cases (host-write overlap/non-overlap, committed prefix of a failed
host write, `memory()`-handle writes, run→exit→clear→resume,
run→budget→resume, cross-thread writer exclusion by the domain mutex); HTIF
endpoint cases under approved D-c (LR/SC access-fault, AMO callback-once);
facade equivalence for
mixed guests; A6 budget/exit/observation regressions. Non-claims: no multi-Hart
execution, DMA, coherence, or RVWMO proof; no MMU/page-walk/TLM unification;
no interrupt/time/debug/Machine-lifecycle work; no performance measurement or
optimization; no extension certification; the two 51-case historical sets
stay separately recorded and the new guests are a separately counted
addition.

## 11. Approved profile choices

The maintainer approved all seven items at the recommended defaults on
2026-09-21. These values remain in force. The separately authorized SC
semantic correction below clarifies their spec-first execution order without
changing the seven profile selections or activating a successor contract.

1. **Encoding repair (§2/§5.3) — approved.** Spec table + W/D repair +
   AMOSWAP + reserved-encoding illegal traps + aq/rl all-legal-no-op.
2. **Reservation profile (§5.4) — approved.** Per-Hart `CoreState`,
   port-paddr byte-span key with LR-time snapshot, one reservation replaced
   by a new LR, span-containment SC validity, deterministic success rule,
   consume-on-executed-SC. **Faulting-SC = retain**, citing
   `sc_reservation_invalidate` (covers executing SCs only) and Spike's
   retain-on-trap. Span-containment remains a deliberate stricter deviation
   from Spike's width-less exact-paddr key, legal under
   `lr_reservation_set_size`.
3. **Writer visibility (§5.5/C20/C29) — approved P2a-precise.** Storage-level
   committed-write bookkeeping (per-block/striped counters or bounded
   journal); only overlapping committed writes fail SC. Same-Hart overlapping
   invalidation is a deliberate divergence from the spec minimum and Spike.
   P1 remains withdrawn as ADR-conflicting.
4. **Device atomic capability (§5.6/C21) — approved D-c.** HTIF LR/SC
   rejected (load/store-AMO access fault, no callback); AMO allowed as one
   indivisible envelope (zero-read old value, callback exactly once), not
   Spike's load+store pair. UART rejects on width. RAM remains atomic-capable.
5. **Public API (C23) — approved.** Remove
   `GLOBAL_RESERVATION` and `ruscv_sim::execute::clear_reservation`.
   Helpers keep their signatures; `ReservationSet` is the per-Hart record
   type; in-repo callers of the free function (tests) migrate to a fresh core
   or a per-core helper.
6. **Old typed route (C24/C25) — approved.** Typed pair stays on the old
   constructor as a labeled non-conforming adapter; standard facades exit the
   bridge.
7. **Guest-suite growth (T4/T5) — approved.** Add project-authored atomic ELF
   guests and record the new total; keep ACT4 selection frozen and separately
   recorded; new differential/oracle selection remains its own future proposal.
8. **SC target checks before reservation failure — maintainer-authorized
   correction.** The pinned Zalrsc mandates SC permission checks before
   retirement and failure outside the reservation set; a failed SC may be
   checked as a store, while translation side effects are unspecified and
   failure is nonzero with no write. Spike and Sail are implementation
   references only. A8 selects Hart legality/alignment, guest-to-port address
   conversion, then one existing StoreConditional envelope with optional Hart
   reservation context; the target validates store access and atomic
   capability before returning conditional Failure. On valid RAM, absent or
   uncovered context fails without guest-byte reads/writes or bookkeeping
   bumps. Unmapped/unsupported targets fault cause 7 even without a
   reservation. Faulting SC retains any live reservation; completed
   conditional failure consumes it. This order is an implementation choice,
   not a claim that the spec prescribes a backend sequence or that unwired
   MMU/PMP permissions are implemented. The old typed `RiscvCore::new` route
   remains explicitly non-conforming and does not claim target permission
   support.

## 12. Activation record and navigation

The profile in §11 is **approved**, and this contract **is now the sole
Current contract**: activation was separately authorized and completed on
2026-09-21. The activation change is documentation-only and claims no
implementation. (1) This approved contract replaced `docs/dev-plan.md`; (2)
the proposal file is preserved, unchanged, as the archived A8 proposal
snapshot source; (3) implementation tasks T0–T5 run under the repository
branch/review workflow in later, separately reviewed changes and are not
started by this activation.

Related records:

- [Archived A8 proposal snapshot (source of this contract)](proposals/a8-single-hart-atomic-convergence.md)
- [Post-A7 roadmap proposal (Stage 1 source)](proposals/post-a7-roadmap.md)
- [ADR-0002 physical-access contract](architecture/decisions/0002-physical-access-transaction-and-fault.md)
- [ADR-0001 Hart outcome contract](architecture/decisions/0001-hart-execution-outcome-and-observation.md)
- [ADR-0003 ownership boundaries](architecture/decisions/0003-runner-machine-and-platform-ownership.md)
- [ADR-0004 time/scheduling boundaries](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md)
- [Archived A7 contract](archive/milestones/a7-non-atomic-physical-access-migration.md)
- [A7 closeout record](archive/milestones/a7-closeout-record.md)
- [A7 migration characterization](verification/a7-migration-characterization.md)
- [A7 Hart/legacy-bridge record](verification/a7-hart-physical.md)
- [Documentation policy](documentation-policy.md)
- RISC-V ISA manual, reference-investigation pin: riscv-isa-manual @
  b84f402bd6c436d984ca9ee85e2d4024293d5fe5 (src/unpriv/zalrsc.adoc,
  src/unpriv/zaamo.adoc)
- RISC-V ISA simulator, implementation-reference pin: riscv-isa-sim
  (Spike) @ 02b1dc182164bb73b19b050676dd89f0834f8b2e (riscv/mmu.h,
  riscv/mmu.cc, riscv/sim.cc, riscv/simif.h)
