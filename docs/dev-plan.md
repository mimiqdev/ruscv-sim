# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A2 — Reliable Flat-Library ELF Execution and Inspection

**Status:** Active — capability contract; implementation not yet verified

**Authority:** Normative milestone contract

## Objective

A library caller can load a supported RAM-only ELF, run it under a finite budget,
receive the actual guest exit or a distinguishable timeout/execution error, and
inspect the resulting registers, RAM and declared signature without hanging or
confusing ELF addresses with flat storage offsets. Loading a subsequent image
must not reuse the previous image's exit or signature metadata.

This is one end-to-end capability, not a quota of repaired defects. A successful
`read_mem` repair alone does not complete A2. A1 established the baseline; A2
makes this explicitly bounded library workflow dependable before larger runtime
migration. [A1's closeout](archive/milestones/a1-closeout-record.md) and its
G-02-only successor recommendation remain historical records. This contract
replaces that task-sized scope without reopening A1 or claiming A2 is implemented.
Milestone identifiers are not release numbers.

## Starting evidence and capability boundary

The [A1 matrix](verification/public-behavior-matrix.md),
[gap register](verification/public-behavior-gaps.md),
[`RiscVSimulator`](../src/executor.rs) and
[`tests/public_behavior.rs`](../tests/public_behavior.rs) provide the baseline:

- G-01 reproduces a library ELF run missing an exit visible on the public ELF path.
- G-02 reproduces an inspection request that never returns.
- Source inspection shows `run` clears an exit signal before `get_result` reads
  it again, risking loss of a nonzero code. This is source-observed, not yet a
  verified reproduction; reproduce it as part of the result task.
- The flat wrapper passes ELF signature metadata directly to memory inspection
  and suppresses read errors. Address correspondence and error reporting require
  focused verification, not an inference from successful CLI signature tests.

The supported acceptance configuration is one existing Hart, synchronous flat
RAM, little-endian ELF64/RV64I fixtures using already exercised instructions,
RAM-backed tohost and bounded allocations/requests. CLI device behavior is a
regression constraint, not a promise that the flat wrapper acquires UART/HTIF
MMIO or every CLI capability. Neither fixture success nor component tests prove
ISA-wide or ACT4 compliance.

## Required observable behavior

### Load and address correspondence

- Preserve ELF entry and reported metadata as image/guest addresses. Keep public
  flat `read_mem`, `write_mem` and manual `set_tohost` addresses as storage offsets.
  Convert image-derived tohost/signature locations at the backend boundary using
  checked range arithmetic; do not change guest PC or call this MMU translation.
- Test base zero and nonzero base, a nonzero entry offset, file bytes and BSS.
  Reuse the existing loader contract; do not introduce an ELF parser redesign.
- ELF tohost metadata selects the image's RAM-backed signal. An explicit
  `set_tohost` after loading still overrides it in flat offsets. With no metadata,
  preserve the documented manual/default configuration without accidentally
  retaining a previous image-derived selection. Define and test the before-load
  setter precedence in the API documentation, preserving existing successful
  uses rather than silently reinterpreting addresses.
- A second successful image load replaces image-owned state and metadata. This
  is verification of the existing load operation, not a new reset/checkpoint API
  or an all-or-nothing guarantee for every malformed ELF load.

### Run and result

- Decode and retain the actual supported zero/nonzero exit value before clearing
  its RAM signal. Report exit only after the writing instruction succeeds; final
  PC and successful-step count must describe that boundary.
- Preserve supported exit encodings and existing public signatures. Zero budget
  executes no instructions; explicit/configured limits bound non-exiting guests;
  an exit in the final permitted slot is not a timeout.
- Distinguish guest failure exit, timeout and execution error through existing
  result/error surfaces. Do not convert failed instructions into successful
  cycles or fabricate a successful guest exit after a memory read failure.

### Post-run inspection

- Bounded flat reads return exactly the requested bytes in ascending address
  order or an explicit `ExecutorError`; never wrap addresses, retry without
  progress, fabricate bytes or return a partial vector as complete success.
- Empty reads return an empty vector without reading memory. Cover aligned and
  unaligned accesses, mixed widths/lengths, the final valid byte, range crossing,
  out-of-range starts and address overflow. Keep the original G-02 subprocess
  test bounded with readiness, timeout, kill/reap and failure-path checks.
- Signature results preserve the guest metadata address while reading the
  corresponding flat bytes. Absent metadata yields no artifact; a zero-length
  region yields an empty artifact. An unreadable declared region must produce
  an explicit diagnostic through existing error surfaces, not silent absence.
  Preserve the completed run's exit/count/PC and any primary execution failure
  when reporting an artifact failure; no new public field is required.
- Inspection must not execute guest instructions or alter PC, registers, RAM or
  execution accounting. Verify against the state left by actual guest execution,
  not only an isolated helper round-trip.

## Task decomposition and PR cadence

These are delivery tasks under one milestone, not additional milestones. They
may be split or combined into reviewable PRs without changing the acceptance goal.

| Task | Contribution to the capability | Dependency / evidence |
| --- | --- | --- |
| T1 — Bounded RAM inspection | Repair G-02 and establish exact bytes/error behavior and safe hang-regression harness. | Reproduce before repair; valid/boundary/overflow and state-preservation tests. |
| T2 — ELF placement metadata and guest completion | Adapt image-derived RAM tohost, retain decoded exit before clearing, and verify limit/error boundaries. | G-01 fixture plus zero/nonzero exit, manual configuration and final-slot tests. |
| T3 — Post-run artifacts and image replacement | Read flat signatures correctly, expose artifact failures and prevent old metadata leaking into a subsequent image. | Uses checked correspondence from T2 and safe inspection from T1 where applicable. |
| T4 — Integrated acceptance and documentation | Prove load → run → result → inspect as one workflow and record retained differences from CLI. | Depends on T1–T3; exact-head review, full gate and guest-suite evidence. |

Reproduce source-observed failures before repairing them. Keep new discoveries
within this workflow explicit; unrelated findings go to the gap register, not
silently into implementation. Repairing two named gaps is necessary but not
sufficient: the integrated acceptance scenarios decide completion.

## Architectural constraints

The [accepted ADRs](architecture/decisions/README.md), particularly
[ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md),
and [principles](architecture/principles.md) remain authoritative:

- Reuse the existing Hart; no second ISA engine or new execution loop.
- Storage adaptation is not guest virtual-to-physical translation. Host image
  installation and inspection do not synthesize Hart instructions.
- This is a compatibility repair of the existing facade, not approval to retain
  independent loops as the target architecture. Runner/Machine/Platform ownership
  remains the target; this milestone does not claim those boundaries integrated.
- Preserve CLI options, public API signatures, successful CLI ELF/device behavior,
  and existing flat helper address semantics. Correct misleading address comments
  and document observable repairs alongside the relevant implementation PRs.

## Non-goals and retained gaps

- Runner/Machine/Platform migration, new physical-access ports, precise Hart
  observations, multi-Hart, MMU/PMP integration, SystemC/TLM or new ISA support.
- Public commit-log fixes G-03/G-09, UART aperture G-04, verbose CLI diagnostic
  G-05, and unrelated guest README discrepancy G-08.
- Redesigning CLI signature-failure policy (G-06): A2 addresses the flat-library
  artifact path only. Keep the CLI limitation explicit even if shared helpers
  receive compatible internal improvements.
- Device-backed flat execution, arbitrary huge allocation hardening, termination
  guarantees for blocking custom backends/poisoned locks, general `write_mem`
  transactional semantics, exhaustive malformed ELF/permission/overlap handling,
  or a new reset/quiesce/checkpoint protocol.

These exclusions bound the capability; they are not extra future milestones or
a blanket waiver of defects within the required workflow.

## Deliverables and acceptance criteria

Deliver repaired library behavior, focused regressions, an integrated acceptance
suite, public API usage/address documentation and updated evidence records.
Completion requires all of the following:

1. **Load/run/result/inspect works together.** RAM-only fixtures at zero and
   nonzero bases execute from their declared entries, modify RAM/registers,
   terminate through image-derived tohost, and expose exact exit code, count,
   PC and post-run bytes. Include both zero and nonzero guest exit codes.
2. **Control is bounded and truthful.** Zero budget, configured/default selection,
   explicit exhaustion, final-slot exit and instruction-error cases assert
   distinct outcomes and correct successful-step accounting. The original G-01
   contrast becomes a correct library-exit regression, not a deleted test.
3. **Inspection is safe.** The original G-02 request returns an error under the
   bounded child harness; alignment/size/boundary/overflow/empty cases have exact
   assertions. Successful and failing inspection leave guest state unchanged.
4. **Artifacts and subsequent loads are coherent.** Guest-written signatures at
   nonzero bases return correct bytes and original metadata addresses. Absent,
   empty and unreadable regions are distinguishable. Load another image on the
   same wrapper and verify that RAM/image metadata and exits do not leak from the
   prior image; manual flat tohost configuration remains tested and documented.
5. **Compatibility and evidence are current.** Full quality gate, strict rustdoc,
   public ELF/CLI regressions and actual project-authored guest compilation/runs
   are recorded for the reviewed implementation or verified merge. Matrix/gap
   updates identify precisely what was repaired and what remains unverified;
   old logs, skipped checks and component-only passes are not integration proof.
6. **Capability acceptance, not task counting.** A final assessment maps these
   scenarios to committed tests, records limitations and independent review,
   and proposes a successor from remaining architectural needs. T1 completion
   alone, or merely closing G-01/G-02, cannot close A2.

The quality gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, plus
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
Use the [development environment](development-environment.md) and
[guest commands](verification/bare-metal-tests.md). Record unavailable tools and
verification gaps explicitly; unmet capability criteria cannot be silently
reclassified as optional tasks.

## Closeout

Keep this as the only active contract. After capability acceptance and required
review/merge evidence, archive completion, limitations and revision identifiers;
re-evaluate unfinished work and select one separately approved successor.
Task/PR granularity does not determine milestone granularity. No A2 production
implementation or completion is claimed by this planning correction.
