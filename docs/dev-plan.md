# Active Development Plan

**Project:** `ruscv-sim`

**Active milestone:** A2 — Bounded Flat-Memory Inspection

**Status:** Active — focused G-02 repair and verification

**Authority:** Normative milestone contract

**Approved / started:** 2026-09-08

This is the only current milestone contract. [A1 is completed](archive/milestones/a1-closeout-record.md)
with documented limitations. Its [acceptance assessment](archive/milestones/a1-acceptance-assessment.md)
selected one bounded successor: eliminate the reproduced non-progress behavior in
`RiscVSimulator::read_mem`. Milestone identifiers are not releases.

## Objective

For bounded byte-range inspection requests against the existing flat-memory
configuration, `read_mem` must complete with the requested bytes or an explicit
error rather than looping indefinitely on a failed wider read.

[G-02](verification/public-behavior-gaps.md#g-02--flat-library-read_mem-can-loop-indefinitely-on-an-out-of-range-aligned-read)
and the persistent child-process reproduction are the starting evidence. This
milestone does not consolidate run loops or implement Machine/Platform boundaries.

## Scope

- Read the existing helper, memory interface and regression harness; reproduce
  the aligned out-of-range read failure before changing production behavior.
- Make each iteration of flat-memory inspection advance toward completion or
  return an error. Preserve the existing public signature, byte ordering and
  flat address meaning. Do not fabricate bytes or return a partial vector as
  successful completion of a longer request.
- Verify valid aligned/unaligned reads, byte/half/word/dword-sized and mixed
  lengths, memory-end/boundary-crossing reads, zero length and address overflow.
- Replace the known-hang expectation with a bounded error-return regression;
  retain safe child startup/timeout/cleanup checks so a regression cannot leave
  the test harness hanging or leaking a busy child.
- Update G-02 and the relevant matrix rows using actual tests and gate evidence,
  retaining historical reproduction provenance and unrelated gap dispositions.

## Error and compatibility boundaries

- Empty requests return an empty vector without a memory read. Requests within
  the supported flat range return exactly the requested bytes in ascending
  address order.
- Invalid or overflowing nonempty ranges must fail through the existing
  `ExecutorError` surface; they must not wrap into a different valid address.
  Keep the error diagnostic informative without freezing incidental wording as
  a new public string contract.
- Preserve successful in-range behavior and the ability to read unaligned bytes.
  A failed wider access must not retry forever. Concrete safe fallback versus
  byte-wise implementation is an implementation choice, subject to regression
  evidence and existing interface constraints.
- The guarantee is for bounded requests on the synchronous existing flat-memory
  backend. This milestone does not guarantee allocation success for arbitrary
  enormous `size` values, introduce arbitrary request-size limits, or promise
  termination of user-supplied blocking devices/backends or poisoned host locks.
- Host inspection must not change Hart registers, PC, guest memory, run budgets
  or retirement. No guest instruction execution or platform mapping change is
  introduced by the repair.

## Non-goals

- Repairing G-01 or logging, UART, signature and verbose diagnostic gaps;
  rewriting `write_mem` or the memory subsystem; changing public APIs.
- Runner/Machine/Platform migration, precise Hart outcomes/observations, new ISA
  or devices, MMU/PMP, multi-Hart, SystemC/TLM, acceleration or ACT4 integration.
- A broad performance project, unbounded allocation hardening or a new device
  inspection protocol. The old migration candidate remains unapproved reference.
- Automatically carrying forward the remaining A1 limitations as tasks.

## Architectural constraints

The [accepted ADRs](architecture/decisions/README.md) and
[principles](architecture/principles.md) remain authoritative. Keep one Hart
engine, flat-library versus CLI configurations distinct, and host inspection
separate from Hart-initiated physical transactions. Preserve
[ADR-0003 §9 compatibility](architecture/decisions/0003-runner-machine-and-platform-ownership.md#9-compatibility-constraints-for-the-current-product).
A helper repair does not establish target inspection/lifecycle integration.

## Deliverables

- Minimal production repair of `RiscVSimulator::read_mem` progress/error handling.
- Focused value/error/boundary tests plus a bounded regression and cleanup-path
  evidence for the previously hanging request.
- Updated G-02/matrix evidence, exact verification commands and limitations.
- An acceptance summary and one evidence-based successor recommendation, not a
  second active plan.

## Delivery cadence

Use reviewable batches: focused repair and regressions; then acceptance evidence
and successor recommendation. Small PRs are delivery units, not extra milestones.
Escalate any necessary scope change instead of mixing unrelated fixes into A2.

## Acceptance criteria

1. The original `new(0x1000).read_mem(0x2000, 4)` failure is reproduced before
   repair and thereafter returns an error under a bounded test, without leaving
   child processes. Handshake/early-exit/timeout cleanup remains verified.
2. In-range reads return exact bytes for aligned/unaligned and mixed-size cases;
   zero-size reads, end-of-memory, crossing and overflow cases have explicit
   success/error expectations and passing assertions.
3. The helper has no non-progress retry path for the supported flat backend;
   failure does not masquerade as a complete successful byte vector. Tests
   verify inspection leaves guest memory, PC and registers unchanged.
4. Public signatures and unrelated runtime behavior remain unchanged. The
   compatibility matrix records the narrow repair rather than declaring other
   gaps fixed or the target architecture integrated.
5. Full repository quality gate, strict rustdoc and project-authored guest-suite
   results are recorded for the reviewed implementation or its verified merge.
   Missing tools, skipped checks and failures are never recorded as passes;
   unresolved verification gaps require explicit acceptance disposition.
6. Completion evidence and remaining limitations are summarized; the successor
   recommendation is derived from evidence and separately approved before the
   next handoff.

The quality gate is `cargo fmt --all -- --check`, `cargo check --all-features`,
`cargo clippy --all-features --all-targets -- -D warnings`,
`cargo test --all-features`, `cargo doc --all-features --no-deps`, plus
`RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`.
Use the [development environment](development-environment.md) and
[guest commands](verification/bare-metal-tests.md); an old reference log or
workflow definition alone is not a successful run.

## Closeout

Assess every criterion against committed code/tests and recorded verification.
Record completion, limitations and revision identifiers, re-evaluate unfinished
work, then archive A2 and replace this file only with one approved successor.
Approval of this contract is not blanket authorization to commit, push, open PRs,
merge, tag or release future implementation changes.
