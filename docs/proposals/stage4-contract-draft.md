# Stage 4 virtual memory and protection (proposed milestone contract)

**Status:** Proposed contract. **Not approved, not active**. This document records the owner's decisions of 2026-10-10 and is the approval candidate for the next milestone. Activation still requires a separate approval of this contract, and then a separate documentation-only rotation that archives the A10 contract, mirroring the A9 to A10 transition (closeout #70, rotation #71). This document does not archive A10 and does not replace [docs/dev-plan.md](../dev-plan.md). Nothing here is implemented, scheduled, or claimed.

**Authority if approved:** would become the sole Current normative milestone contract, constrained by accepted [ADR-0001](../architecture/decisions/0001-hart-execution-outcome-and-observation.md) (Hart execution outcome and observation), [ADR-0002](../architecture/decisions/0002-physical-access-transaction-and-fault.md) (physical access transaction and fault), [ADR-0003](../architecture/decisions/0003-runner-machine-and-platform-ownership.md) (runner, Machine, and platform ownership), and [ADR-0004](../architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md) (interrupt-time scheduling and stop boundaries). Translation and protection are Hart-owned work feeding the existing physical port taxonomy. No new ADR is proposed. If review finds an ADR gap, that gap is surfaced before implementation, not patched mid-milestone.

**Decisions:** every row in §8 is DECIDED. Decider: owner (Tony), on independent advice. Date: 2026-10-10.

**Planning baseline:** `d5258f5cbedc8e020d1c918e0f0028ea1718ff75` (PR #73, 2026-10-10), which is `main` at the time this revision was written. The earlier draft cited `5d23d337a6732d178a2e14445be9f5c28154f270` (PR #72). PR #73 changed the fixture pin after that. Follows [post-A7 roadmap Stage 4](post-a7-roadmap.md). The Stage 3 exit gate is waived for Stage 4 by D0. That gate is a scheduling choice, not an architectural dependency.

**Relationship to the deferred certification layer:** the A10 performance framework (baseline profile, smoke profile, schema and bundle machinery) exists and is tested. This contract does not add performance payloads or thresholds, and it does not require the deferred certification layer.

## 1. Proposed objective and architecture

The objective, if later approved, is to connect one privilege and address-translation profile to the common physical port so a public ELF guest can run with translation on. The selected profile is M, S, and U, with M as the boot and handler mode (D1). The selected modes are Bare and Sv39 (D2). Fetch, load, store, and AMO effective addresses are translated. A permission, reserved-bit, or misaligned-superpage violation becomes an architectural page fault that keeps the original virtual access kind and address. A physical failure during a walk read or an A/D write-back becomes an access fault of that same kind (D3, D4). Hardware sets A and D. PMP is not enforced, and the standard PMP CSRs read as zero (D5). No A8, A9, or A10 guarantee is weakened.

Boundary, from roadmap Stage 4, as narrowed by §8. Page-table reads and A/D write-back flow through the same physical result taxonomy. The original virtual access kind and address are retained when mapping physical faults and page faults. A successful A/D write stays visible and is not a commit effect of a faulting instruction (ADR-0001 §2). There is no translation cache on the public path in v1, so the roadmap's "TLB integration" deliverable is deferred (M4). PMP enforcement is explicitly excluded (D5). The public ELF guests cover bare, mapped, permission-fault, page-walk-fault, A/D, and SFENCE.VMA cases. Image placement stays on the current image-base path, and translation sits in front of it (D7).

This is the proposed **first bounded milestone** of virtual-memory work. One hart, no OS or Linux boot, no hypervisor, no multi-hart TLB coherence, and no interrupt virtualization. Sv48, Sv57, PMP enforcement, and a translation cache stay out. A later milestone may add them.

## 2. Entry evidence and existing controls (verified at the baseline)

These are available seams, not capability claims. Component presence is not public-path support. Every path below was read at `d5258f5`.

- **Privilege state exists and is Hart-owned, and the public path barely exercises it.** `PrivilegeMode { User, Supervisor, Machine }` and staged trap transitions live in [`src/core/mod.rs`](../../src/core/mod.rs). `mret`, `sret`, and `uret` plus `ecall` and `ebreak` live in [`src/isa/rv64i/system.rs`](../../src/isa/rv64i/system.rs). CSRs, including `satp` at `0x180`, live in [`src/csr/mod.rs`](../../src/csr/mod.rs). [`tests/privilege_transition_test.rs`](../../tests/privilege_transition_test.rs) has 19 tests, [`tests/csr_access_test.rs`](../../tests/csr_access_test.rs) has 27, and the A6 suites cover machine-mode trap entry and return. No public ELF guest today changes privilege or turns translation on. `exec_mret` (`src/isa/rv64i/system.rs:270-292`) returns to the privilege in MPP, including S, and clears MPRV on a return below M. `exec_sret` (`src/isa/rv64i/system.rs:319-354`) exists, but the public core reports the SRET encoding as an unsupported legal instruction (`src/core/mod.rs:1524` and `src/core/mod.rs:1544`), so a public guest cannot execute it today.
- **Sv39 components are implemented and component-tested, and they are not integrated.** [`src/mmu/`](../../src/mmu/) parses `satp` (Bare = 0, Sv39 = 8, Sv48 = 9, Sv57 = 10, other values rejected, `src/mmu/mod.rs:127-133`), walks page tables through `PhysicalMemoryInterface`, and defines `MmuError { PageFault, AccessFault, PmpViolation, UnsupportedMode, InvalidSatpMode }`. `Pte` sets and clears A and D. `Mmu::flush_tlb` is labeled an SFENCE.VMA implementation. [`tests/translation_test.rs`](../../tests/translation_test.rs) has 23 tests and [`tests/ad_bits_test.rs`](../../tests/ad_bits_test.rs) has 10. The public core path never calls these. Fetch, load, and store use image-base and storage adaptation. Page-table accesses use the MMU-specific `PhysicalMemoryInterface`, not the A7 and A8 `PhysicalAccess` route. No public instruction decodes or executes SFENCE.VMA. A search for `sfence` under `src/decode`, `src/isa`, and `src/execute` finds nothing. `Opcode::MiscMem` (`src/decode/mod.rs:39`) is decoded only as base FENCE.
- **The component walker is not usable on the public path as it stands.** These are defects this milestone fixes, not behavior to preserve.
  - A walk read goes through `PhysicalMemoryInterface::read_dword`, and every error becomes `WalkResult::AccessFault { level }` (`src/mmu/sv39.rs:315`). The target, host, protocol, and unknown taxonomy of ADR-0001 and ADR-0002 is lost. The translator then reports `MmuError::AccessFault(vaddr)` (`src/mmu/translator.rs:126`).
  - The walker checks the reserved permission combination W = 1 and R = 0 (`src/mmu/sv39.rs:355`). It does not check reserved PTE bits 63:54, and it does not check a non-leaf PTE with D, A, or U set. `src/mmu/pte.rs` documents bit 63 as `N` and bits 62:54 as `RSW`, which is not the Sv39 reserved-bit rule.
  - `Sv39::build_physical_address` (`src/mmu/sv39.rs:197-230`) ORs the low VPN fields into the PPN of a 2 MiB or 1 GiB leaf. A misaligned superpage therefore translates instead of raising a page fault.
  - On a TLB hit, `translate_sv39_with_tlb` checks permissions and returns (`src/mmu/translator.rs:85-100`). It never sets D, so a read followed by a write of the same page leaves D clear.
  - `create_tlb_entry` stores `pte.ppn()` and ignores the walk level (`src/mmu/translator.rs:151-160`). A superpage hit then computes `(ppn << 12) | (vaddr & 0xFFF)` and drops VPN[0] for a megapage and VPN[1:0] for a gigapage.
- **PMP is nominal only.** `MmuConfig::pmp_entries` defaults to 16 (`src/mmu/mod.rs:84`) and `MmuError::PmpViolation` exists. There is no `pmpcfg` or `pmpaddr` CSR, no enforcement, and no test. An unknown CSR address returns `CsrError::InvalidAddress` (`src/csr/mod.rs:225-246`), which the core turns into an illegal-instruction trap. Treat PMP enforcement as absent.
- **S-mode CSR state is split, and delegation is inert.** `sstatus` is a separate map entry (`src/csr/mod.rs:162`), not a view of `mstatus`. S-trap entry writes that copy (`src/core/trap.rs:385-389`), and `exec_sret` reads and writes it (`src/isa/rv64i/system.rs:331`). `medeleg` and `mideleg` are stored by the generic CSR write path, and the handler ignores those values. It consults its own `TrapDelegation` (`src/core/trap.rs:313-320`), which the public core leaves at zero. The H-extension `vs*` CSRs are inserted and accept writes (`src/csr/mod.rs:174-182`) even though the hypervisor is a non-goal.
- **The integration seam is the physical port.** A8 and A9 put every fetch, load, store, and AMO through the validated physical data port, with the target, host, protocol, and unknown taxonomy and with the original address preserved in `mtval`. The Hart behind `Machine::step` owns that route. The translator plugs in between the Hart's virtual access and that route. Page-table walks are themselves physical accesses on the same route. `hart_issued_paddr` (`src/core/mod.rs:463`) subtracts `base_addr` from the guest effective address before the port sees it, so translation has to produce a guest physical address before that subtraction.

The milestone wires the existing components through the public path, fixes the walker and fault-classification defects above, adds SFENCE.VMA decode and dispatch, and makes the S-mode CSRs views of the M registers. It is not a from-scratch MMU, and it does not reuse the component walker unchanged.

## 3. Selected scope and exclusions

In scope, from the §8 decisions:

- Privilege modes M, S, and U, with M as the boot mode and the handler mode (D1). A public guest can enter S and U through `mret` and return. SRET becomes a supported instruction in M and in S, and it is illegal in U. `mstatus` bits TVM (20), TSR (21), and TW (22) are writable. With TVM = 1, an S-mode read or write of `satp`, or an S-mode SFENCE.VMA, raises an illegal-instruction exception. With TSR = 1, an S-mode SRET raises an illegal-instruction exception. With TW = 1, a WFI executed in S-mode or U-mode raises an illegal-instruction exception. The spec allows that trap immediately, and this profile takes it immediately rather than waiting out a time limit. All three bits reset to 0, so none of these traps fire until software sets the bit.
- `satp` modes Bare (0) and Sv39 (8) only (D2). A write whose MODE is anything else, including Sv48 and Sv57, is ignored in full. No field of `satp` changes, and the write raises no trap. MODE changes take effect at the next instruction. ASIDLEN is 0, so the ASID field is read-only zero. `Satp::asid` returning a 16-bit field (`src/mmu/mod.rs:151`) is component behavior, not the public-path width.
- Hart-side translation of instruction fetch, load, store, and AMO effective addresses when the effective privilege mode has Sv39 active (D2, D3). M-mode accesses are not translated unless MPRV applies (D9).
- Each walk step reads one PTE through the existing physical data port (D3). The walk result follows ADR-0001 §2, restated in D3 below. The walker rejects reserved PTE bits 63:54, a non-leaf PTE with D, A, or U set, and a misaligned superpage. Sv39 superpage sizes, 2 MiB and 1 GiB, are supported. The four component defects in §2 are fixed even though v1 walks on every access, so a later cache cannot inherit them.
- Architectural page faults keep the original virtual access kind and address and use the existing A6 trap entry. No new trap channel.
- Hardware sets A on a completed translation and sets D on a completed store or AMO translation (D4). The write-back is a physical write on the same port. A failed write-back is an access fault of the original access kind. A successful A/D write is visible in memory and is not part of a faulting instruction's `CommitRecord`.
- No PMP enforcement (D5). `MmuError::PmpViolation` and `MmuConfig::pmp_entries` stay off the public path. The standard RV64 PMP CSRs, `pmpcfg0`, `pmpcfg2`, and `pmpaddr0` through `pmpaddr15`, are implemented as read-only zero. A read returns 0 and a write is ignored. This is the zero-entry PMP the privileged specification allows, chosen so a boot stub that writes `pmpaddr0` or `pmpcfg0` does not trap. Odd `pmpcfg` addresses stay illegal on RV64, as the specification requires.
- Alignment is checked on the virtual effective address before translation (D6). A misaligned instruction fetch, load, store, or AMO raises the misaligned exception and takes priority over a page fault and an access fault. No access splits a page, because the Hart traps every misaligned access (`src/core/mod.rs:980-995`) and IALIGN is fixed at 32.
- No public API or CLI change (D7). Translation turns on only when the guest writes Sv39 to `satp`, which resets to 0 (`src/csr/mod.rs:171`). The address path is virtual address, then guest physical address from the PTE PPN, then the storage offset `hart_issued_paddr` already applies. A PTE PPN is a guest physical page number, not a storage offset. Walk reads use the same guest-physical path. The old behavior, `satp` stored and ignored, becomes the new behavior, a Sv39 write takes effect and an unsupported MODE is ignored.
- SFENCE.VMA is decoded and executed in its three forms, global, per-ASID, and per-virtual-address (D8). With no translation cache (M4) the instruction is an ordering no-op on the public path. It is legal in M and in S, and illegal in U. An invalid virtual address in `rs1` raises no exception.
- SUM (`mstatus` bit 18), MXR (bit 19), and MPRV (bit 17) follow the privileged specification (D9). With SUM = 0, an S-mode load or store to a U = 1 page is a page fault. MXR = 1 lets a load read an X = 1 page. S-mode fetch of a U = 1 page is a page fault regardless of SUM. MPRV = 1 makes a load or store use the translation and protection of MPP, and it does not affect instruction fetch. `mret` and `sret` to a mode below M clear MPRV, which `exec_mret` already does for `mret`.
- No trap delegation (M1). `medeleg` and `mideleg` are read-only zero. Every synchronous exception, including every page fault, is taken in M, and `mtvec`, `mepc`, `mcause`, and `mtval` record it. The writable `TrapDelegation` in `src/core/trap.rs` is not a public-path control.
- `sstatus`, `sie`, and `sip` are restricted views of `mstatus`, `mie`, and `mip` (M2). A write through either address changes the one register, and a read through either address returns that register's S-visible fields. The separate `sstatus` storage (`src/csr/mod.rs:162`) goes away. `misa` bit S (18) reads 1, and `mstatus` fields SXL and UXL are read-only 2, the encoding of XLEN 64. The H-extension `vs*` CSRs (`src/csr/mod.rs:174-182`) are removed from the public CSR file, so a write to one is an illegal-instruction trap instead of a stored value.
- No translation cache on the public path (M4). Every translated access walks. The component TLB stays a component, and its two defects in §2 are fixed so the component tests stop encoding them. Roadmap Stage 4's "TLB integration" deliverable is deferred to a later milestone and recorded as such.

Non-goals: hypervisor and the H extension, including `hgatp` and the `vs*` CSRs; Linux or SBI boot; multi-hart TLB coherence; a translation cache on the public path; instruction-cache or data-cache modeling; performance work on the walk; debug-mode interaction with translation; any change to A8 atomic semantics; Sv48 and Sv57; PMP enforcement beyond the read-only-zero CSRs; trap delegation; interrupts, WFI, and new counter behavior (D10). WFI stays a recognized legal instruction that the public core reports as unsupported (`src/core/mod.rs:1544`), and Stage 4 fixtures do not execute it. No public API break.

## 4. Workload and oracle contract

Every fixture below is part of the milestone. The oracle shape follows A10. Each fixture records source, linker, and ELF SHA-256, producer identities, exact instruction, retirement, and trap counts, and complete effect expectations. A trap-inclusive fixture reports attempts, completed turns, traps, and retirements independently. `ExecutionResult.cycles` includes completed trap turns. No guest writes MINSTRET.

| Family | What it proves | Oracle shape |
| --- | --- | --- |
| Bare sanity | D7. An existing guest, `rv64i/fib.S`, with `satp` left at 0. | Byte-identical to today's public routes. |
| Mapped read and write | D1 and D2. The guest builds Sv39 tables whose PTE PPNs are guest physical page numbers, writes `satp`, then loads and stores. | Exact translated bytes, untouched neighbors, final PC, and retirements. |
| Permission fault | D6 and D9. A store to a read-only PTE, a fetch from a non-executable PTE, an S-mode load of a U = 1 page with SUM = 0, an S-mode fetch of a U = 1 page, and a load of an X = 1 page with MXR = 0 and again with MXR = 1. | Exact cause, the original virtual address in `mtval`, and no partial store. |
| Walk fault | D3. An invalid PTE, a PTE with a reserved bit 63:54 set, a non-leaf PTE with D, A, or U set, a misaligned superpage, and a walk whose physical read the target rejects. | A page fault for the PTE cases and an access fault of the original kind for the rejected read, each with the original virtual address in `mtval` and no partial RAM effect. A host, protocol, or unknown completion is a `SimulatorFailure`, asserted by a driver test rather than a guest. |
| A/D observation | D4. A fresh page, read and then written. A second case where the A/D write-back is rejected. | A set after the read, A and D set after the write. The rejected write-back is an access fault of the original kind. Where observation is on, the successful write appears as a memory effect and does not appear inside the `CommitRecord` of an instruction that then faults. |
| SFENCE.VMA | D8 and M4. The guest executes the global form, the `rs1 = x0` form, and the `rs2 = x0` form, then uses the mapping it just wrote. One case executes SFENCE.VMA in U-mode. | The new translation is visible after the fence. Because v1 has no translation cache, the new translation is also visible before the fence. That earlier visibility is recorded as a property of this model, not as an architectural requirement. The U-mode case is an illegal-instruction trap. |
| MPRV | D9. In M, with MPRV set, MPP = S, and SUM = 1, the guest loads and stores through an Sv39 mapping, including a load of a U = 1 page, and fetches without translation. | The load and the store use the S-mode translation, and SUM permits the U = 1 load. The fetch does not use that translation. A second load of the same U = 1 page with SUM = 0 is a page fault. |
| TVM, TSR, and TW | D1. The guest sets each bit and then attempts the trapped operation from S-mode. | TVM = 1 makes an S-mode `satp` read, an S-mode `satp` write, and an S-mode SFENCE.VMA raise illegal-instruction. TSR = 1 makes an S-mode SRET raise illegal-instruction. TW = 1 makes an S-mode WFI raise illegal-instruction. The same operations with the bit clear do not raise that trap. Machine mode performs all of them with the bit set. |
| PMP CSR access | D5. The guest writes `pmpcfg0`, `pmpcfg14`, `pmpaddr0`, and `pmpaddr63`, then reads them back, and attempts `pmpcfg1`. | Each even read returns 0. Each write raises no trap and changes no architectural state. The odd `pmpcfg1` access raises illegal-instruction. |
| Delegation | M1. The guest writes ones to `medeleg` and reads it back, then takes a page fault. | The read returns 0. The page fault is taken in M, with `mtval` holding the original virtual address. |

The selected fixtures run on the public routes. Those routes are the real CLI child, the native library, and the flat facade where the fixture does not use a device. Component MMU tests do not prove this milestone.

## 5. Route and phase matrix

Reuse the A10 route matrix. Translation adds no new route and no new configuration cell. `satp` resets to 0, so every existing fixture stays bare without a new Machine or CLI input (D7). A10 baseline and smoke commands gain no mandatory cell from this contract. Timing of a translated path is not an acceptance criterion.

Observation stays on the existing path. A successful A/D write is a `MemoryEffect` in `ArchitecturalEffects.memory`, and it is not attributed to a faulting instruction's `CommitRecord` (ADR-0001 §2, D4). A faulting instruction produces a `TrapRecord` and no `CommitRecord`. No new observation plane.

## 6. Measurement and evidence policy

Correctness first, the same rule as A10. Each fixture, route, and mode cell is checked on every repetition. `perf-test.sh run --profile smoke` grows only by manifest entries for the fixtures in §4. No ratios, no thresholds, and no cohort. The A10 certification deferral stays deferred. Exact-head evidence for a future implementation PR is the full Rust gate, the focused new suites, the negative controls, and fresh guest builds with the pinned producer audit. This document has no implementation PR.

## 7. Verification and acceptance

These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.

1. Every fixture in §4 passes its oracle on every applicable public route. Each negative mutation fails its validator.
2. A driver test shows that a page-table walk read is a physical data read on the existing port, distinct from the guest's own data access. The same test shows the three D3 outcomes. A target rejection of the walk read is an access fault of the original kind. A host, protocol, or unknown completion is a `SimulatorFailure`. An invalid, reserved, non-canonical, or misaligned-superpage PTE is a page fault.
3. With `satp` left at 0, the existing A6 through A10 tests and guests stay byte-identical, including the A10 baseline floor.
4. The SFENCE.VMA fixture passes on a public route. `Mmu::flush_tlb` unit tests do not satisfy this check. The fixture records pre-fence visibility as a model property.
5. Every fixture-induced fault carries the original virtual access kind and address. No fault is classified `unknown` by omission. The A9 quarantine and safety regressions stay green.
6. A guest that writes Sv39 to `satp` observes translated accesses, and a guest that writes an unsupported MODE observes no change to `satp`. No CLI flag and no new public type is added.
7. The component defects in §2 have regression tests. A TLB hit sets D. A superpage TLB hit keeps the low VPN bits. A reserved PTE bit and a misaligned superpage raise page faults. A walk read keeps the target, host, protocol, and unknown distinction.
8. `medeleg` and `mideleg` read back as zero, and a page fault is taken in M. With `mideleg` = 0, every bit of `sip` and `sie` reads as zero and ignores a write. `sstatus` and `mstatus` observe the same S-visible fields. `misa` bit S reads 1, and SXL and UXL read 2. A write to a `vs*` CSR traps. The TVM, TSR, and TW fixture in §4 passes.
9. The full local quality gate is green (`cargo fmt --all -- --check`, `cargo check --all-features`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo test --all-features`, `cargo doc --all-features --no-deps`), and CI Quality-and-tests is green at the exact head.
10. An independent final PR-head review has a recorded outcome. Findings are addressed on the same branch and re-verified.

## 8. Decisions

Every row below is DECIDED. Decider: owner (Tony), on independent advice. Date: 2026-10-10. The unchosen options stay in the record so a later reader can see what was rejected.

Roadmap §11 items 1 through 4 were answered by later milestones. A8 answered the atomic profile and the host-writer policy, A9 answered the Hart and Machine lifecycle, and the earlier milestones answered the earlier public-compatibility choices. They are not reopened here. Roadmap §11 item 6, external integration, stays conditional. Item 7, stable runners, retention, CI and PR comparison, and thresholds, stays deferred by the A10 §8 amendment. Neither is reopened or answered here. Item 5, the Hart and profile boundary, is answered for its virtual-memory part by D1, D2, D5, and D9. Interrupt, WFI, and counter behavior stay with Stage 5 (D10).

**D0. Stage 3 exit and Stage 4 entry.** DECIDED, option B. The roadmap Stage 3 exit ([post-A7 roadmap](post-a7-roadmap.md), Stage 3 items 4 and 5) requires retained revision and PR baseline comparison, scheduled CI publication, and noise calibration. The A10 §8 amendment deferred those for A10 closure only, and [the A10 closeout](../archive/milestones/a10-closeout-record.md) records them as deferred rather than delivered. This contract waives that scheduling gate for Stage 4. The waiver does not say that A10 satisfied the gate, and it does not carry the deferred comparison, publication, or calibration into Stage 4. A10's own independent final review is still pending and remains an A10 acceptance item. It is not a Stage 4 deliverable, and Stage 4 is not activated by this document.

- Option A, rejected. Treating the right-sized A10 as satisfying the gate would claim a comparison, a publication, and a calibration that were not delivered.
- Option B, chosen. Waive the gate explicitly, without treating A10 as having satisfied it.
- Option C, rejected. Stage 4 is not blocked on the deferred certification work.

**D1. Privilege levels in v1.** DECIDED, option A. A public guest runs in M, S, and U and can transition among them. M is the boot mode and the handler mode. This matches the existing `PrivilegeMode` and the A6 trap machinery, and it adds no new privilege state. `mret` to MPP = S and `sret` already exist (`src/isa/rv64i/system.rs:245` and `src/isa/rv64i/system.rs:319`). Making SRET executable on the public path is part of this decision. So is closing the S-mode gaps that M1 and M2 record. `sstatus` is stored separately today, and `medeleg` and `mideleg` writes are stored and ignored.

TVM, TSR, and TW are writable because S-mode exists and `satp`.MODE is writable. Privileged-specification norm `mstatus_tvm_acc` says TVM is read-only 0 only when S-mode is absent or `satp`.MODE is read-only 0, and otherwise must be writable. Norms `mstatus_tsr_acc` and `mstatus_tw_acc` say the same for TSR and TW once a mode below M exists. The current mask `0x8000_0003_000F_FFEA` (`src/csr/mod.rs:272`) excludes bits 20, 21, and 22, so all three read as zero today. That changes.

The trap effects follow the same section. Norm `mstatus_tvm_op` says TVM = 1 makes an S-mode read or write of `satp`, or an S-mode SFENCE.VMA, raise illegal-instruction. Norm `mstatus_tsr_op` says TSR = 1 makes an S-mode SRET raise illegal-instruction. Norm `mstatus_tw_op` says TW = 1 makes a WFI in a mode below M raise illegal-instruction when the instruction does not finish within an implementation-specific time limit, and norm `mstatus_tw_always_illegal` allows the implementation to raise that exception immediately. This profile raises it immediately. Machine mode is unaffected by all three bits, and each bit resets to 0. The §4 fixture covers each bit from S-mode and the unaffected machine-mode case.

- Option A, chosen. M, S, and U, with M as the boot and handler mode.
- Option B, rejected. M and U without S is not a standard paging configuration, because `satp` is an S-mode CSR.
- Option C, rejected. M-mode accesses are never translated except through MPRV, so M alone cannot exercise translation.

**D2. `satp` modes.** DECIDED, option A, with the rejection rule stated as WARL. The public path implements Bare (0) and Sv39 (8). A write with any other MODE is ignored in full. No field changes and no trap is raised. This is the rule in privileged-specification norm `satp_mode_op_unsupported`. Translation changes apply at the next instruction. ASIDLEN is 0, which the same specification explicitly allows, so the ASID field is read-only zero. The component parser accepts Sv48 and Sv57 as well (`src/mmu/mod.rs:127-133`). That acceptance is not a public-path commitment, and the public write path filters those modes out.

- Option A, chosen. Bare and Sv39 only, with an unsupported MODE ignored in full.
- Option B, rejected. Storing Sv48 and behaving as Bare would report a mode the core does not implement.
- Option C, rejected. Implementing Sv48 is larger than this milestone.

**D3. Sv39 page-table walk.** DECIDED, option A, rewritten so the fault rule matches ADR-0001 §2. Each walk read is a physical data read on the existing A7 and A8 port. The result of that read is classified as follows.

| Walk condition | Result |
| --- | --- |
| The target rejects the walk read, or the target rejects the A/D write-back | Access fault of the original access kind. Cause 1 for a fetch, 5 for a load, and 7 for a store or AMO. |
| The completion is a host, protocol, or unknown failure | `SimulatorFailure`. Never an access fault and never a page fault. |
| The PTE is invalid, has W = 1 and R = 0, has a reserved bit 63:54 set, is a non-leaf with D, A, or U set, is a misaligned superpage, or fails the SUM, MXR, or permission check | Page fault of the original access kind. Cause 12 for a fetch, 13 for a load, and 15 for a store or AMO. |

`mtval` holds the original virtual address in every guest-visible case. The walk level and the PTE address may be kept as diagnostics, and they do not replace `mtval`. Sv39 superpages, 2 MiB and 1 GiB, are supported. The component walker does not implement this rule today. It collapses every read error into one access fault, and it misses the reserved-bit and misaligned-superpage checks (§2). Those are in-scope fixes.

- Option A, chosen, with the table above replacing the earlier "a failed walk read is the guest's page fault" wording.
- Option B, rejected. Leaving walks on the MMU-private `PhysicalMemoryInterface` would keep page-table reads off the A7 and A8 taxonomy, which the roadmap forbids.
- Option C, rejected. A software-managed walk cache is an optimization, and M4 already declines a translation cache.

**D4. A/D bit policy.** DECIDED, option A. Hardware sets A on a completed translation and sets D on a completed store or AMO translation. The write-back is a visible physical write on the same port as the walk. A failed write-back is an access fault of the original access kind, per ADR-0001 §2, not a page fault and not a dropped write. A successful A/D write is a separate physical effect. If the access that required it later faults, the write is not rolled back and it is not included in that instruction's `CommitRecord`. Its memory effect stays visible (ADR-0001 §2, the paragraph at line 74 of `docs/architecture/decisions/0001-hart-execution-outcome-and-observation.md`). The component already sets A and D on the walk path (`src/mmu/sv39.rs:450-510`) and fails to set D on a TLB hit (`src/mmu/translator.rs:85-100`). The TLB-hit defect is fixed even though the public path does not cache.

- Option A, chosen. Hardware sets A and D, with the fault and observation rules above.
- Option B, rejected. Trapping on a clear A or D bit adds a trap handler to every v1 fixture.
- Option C, rejected. A per-Machine flag is a second configuration with no consumer.

**D5. PMP entries and granularity.** DECIDED, option A, plus a defined CSR behavior. There is no PMP enforcement in this milestone. `MmuError::PmpViolation` and `pmp_entries` stay off the public path. The exclusion is a non-goal in §3. Because a zero-entry PMP is a conforming choice, the standard RV64 PMP CSRs `pmpcfg0`, `pmpcfg2`, and `pmpaddr0` through `pmpaddr15` are implemented as read-only zero. A read returns 0 and a write changes nothing and raises nothing. Privileged-specification norm `pmp_entry_count` allows zero entries, and norm `pmp_csrs_warl_access` allows the fields to be read-only zero. This is the behavior recorded so a boot stub that writes `pmpaddr0` or `pmpcfg0` before `mret` does not trap. Today those addresses are absent, and an unknown CSR returns `InvalidAddress` (`src/csr/mod.rs:225-246`), which becomes an illegal-instruction trap. Odd `pmpcfg` numbers stay illegal on RV64.

- Option A, chosen. No enforcement, with the read-only-zero CSRs above.
- Option B, rejected. Eight enforced entries is a larger milestone. Its granularity note was also wrong. NA4 and TOR allow 4 bytes, and NAPOT starts at 8 bytes.
- Option C, rejected. Sixteen enforced entries plus `mseccfg` is larger than v1.

**D6. Fault and trap behavior, including misaligned access.** DECIDED, option A, restated. The Hart checks alignment on the virtual effective address before translation, and the misaligned exception takes priority over a page fault and an access fault. This is the check already at `src/core/mod.rs:980-995` for a load, a store, and an AMO, extended to a translated access without changing the order. IALIGN is fixed at 32 and C is forced off in `misa` (`src/csr/mod.rs:254-258`), so no access can cross a 4 KiB page. The page-split sentence in the earlier option A described a case the core cannot reach, and the roadmap already lists successful misaligned accesses as a non-goal. Page faults use the existing cause codes and the existing `mtval` path. No new trap channel is added.

- Option A, chosen, in the restated form. No page-split behavior is specified, because no access splits a page.
- Option B, rejected as a change. The core already traps every misaligned access the RAM path could see, so there is no second population to add.
- Option C, rejected. Two alignment policies would contradict the single check the core already has.

**D7. Public compatibility.** DECIDED, option A, without a new configuration input. There is no public API change and no CLI flag. `satp` resets to 0 (`src/csr/mod.rs:171`), so translation is off until the guest writes Sv39, and existing constructors and the CLI default stay bare. The address layering is the part the roadmap actually asks about. Translation runs before image-base adaptation. A virtual address becomes a guest physical address, and the PTE PPN is a guest physical page number. `hart_issued_paddr` (`src/core/mod.rs:463`) then subtracts `base_addr` to reach the storage offset. Walk reads use that same guest-physical path. The compatibility change from the old behavior is recorded here. Previously a `satp` write was stored and had no effect. After this milestone a Sv39 write takes effect at the next instruction, and a write with an unsupported MODE is ignored in full.

- Option A, chosen, minus the new Machine or guest configuration input the earlier wording added. The guest's own `satp` write is the input.
- Option B, rejected. A `--satp-mode` flag adds a public input the guest can already express.
- Option C, rejected. A new facade type is a public API change this milestone does not need.

**D8. TLB invalidation.** DECIDED, option A, read together with M4. SFENCE.VMA is decoded and executed in all three forms. Global (`rs1 = x0`, `rs2 = x0`), per-ASID (`rs1 = x0`), and per-virtual-address. It is legal in M and in S and illegal in U. There is no public decode today, so the encoding currently falls through `Opcode::MiscMem` (`src/decode/mod.rs:248`) and is not executed as a fence. Because M4 puts no translation cache on the public path, the fence has no cache entry to invalidate and is an ordering no-op there. The architectural requirement is the one the specification states. A translation written before the fence is the translation used after the fence. Observing the old translation before the fence is implementation-defined, and the earlier fixture wording that required it is withdrawn. The fixture records the pre-fence behavior as a property of this model. The component TLB's superpage defect (`src/mmu/translator.rs:151-160`) is fixed so the component stops dropping VPN[0] and VPN[1] on a hit.

- Option A, chosen. Decode and execute all three forms, with the U-mode and oracle corrections above.
- Option B, rejected. Without SFENCE.VMA, a guest that fences has no legal instruction to execute.
- Option C, rejected. Flushing on `satp` writes only would not implement the instruction.

**D9. SUM, MXR, and MPRV.** DECIDED, option B, extended to MPRV. SUM, MXR, and MPRV follow the privileged specification, and §4 has a fixture for each. All three bits are already writable (`MSTATUS_MASK_RV64 = 0x8000_0003_000F_FFEA`, `src/csr/mod.rs:272`), and the component ignores them. `walk_check_permissions` lets S-mode reach a U = 1 page and says so (`src/mmu/sv39.rs:380-392`). Leaving the bits writable and ignored would be option C, which is rejected. With S-mode implemented, SUM and MXR are required to be writable unless `satp`.MODE is read-only zero, which D2 forbids. MPRV is how M-mode code, the handler mode, reads and writes a guest virtual address. `mret` already clears it on a return below M (`src/isa/rv64i/system.rs:285-292`).

- Option A, rejected. The bits cannot stay writable and also stay inert, and hardwiring them to zero is not available while S-mode exists and `satp` is writable.
- Option B, chosen, plus MPRV.
- Option C, rejected. Ignoring a set bit is a silent deviation from the specification.

**D10. Interrupt, WFI, and counter interaction.** DECIDED, option A. No interrupt delivery, no WFI execution, and no counter work in this milestone. Traps taken here are synchronous. Stage 5 owns asynchronous events, matching ADR-0001 and ADR-0004. A faulting instruction does not retire (ADR-0001 §2, line 56), and the A10 oracles already count traps separately from retirements, so no counter rule is added here. WFI is recognized and reported as an unsupported legal instruction (`src/core/mod.rs:1544`). A Stage 4 fixture does not execute it.

- Option A, chosen.
- Option B, rejected. A timer interrupt delivered in S-mode is Stage 5 work.
- Option C, rejected. Defining `mcycle` and `minstret` across a page fault is a counter decision, and A10 left the counters alone.

**M1. Trap delegation.** DECIDED. No trap delegation in v1. `medeleg` and `mideleg` are read-only zero, and every synchronous exception, including every page fault, is taken in M. The privileged specification makes the two registers WARL (norm `medeleg_mideleg_warl`) and forbids read-only one (norm `medeleg_no_rd1`). Read-only zero is the subset it allows, and writing ones and reading them back returns zero, which is how software discovers that subset. The current code stores the writes and ignores them (`src/csr/mod.rs` generic insert, `src/core/trap.rs:313-320`). After this milestone the writes do not change the value.

**M2. S-mode CSR fidelity.** DECIDED. `sstatus` is a restricted view of `mstatus`, and `sie` and `sip` are restricted views of `mie` and `mip`. The privileged specification states each of those directly (machine chapter on `mstatus`, supervisor chapter on `sstatus`, `sip`, and `sie`). One register is stored. The S-mode address reads and writes only the S-visible fields, and the M-mode address reads and writes the whole register. The separate `sstatus` storage (`src/csr/mod.rs:162`) is removed, and S-trap entry and `sret` stop using it.

With `mideleg` held at 0 by M1, no interrupt is delegated, so every bit of `sip` and `sie` is read-only zero. The machine chapter states that directly. A bit becomes visible in `sip` and writable in `sie` only when the matching `mideleg` bit is set, and no such bit is set here.

`misa` bit S reads 1. Norm `misa_s_op` says S = 1 means the hart supports supervisor mode, and S = 0 only says it might not. The reset value is `0x8000_0000_0010_0100` (`src/csr/mod.rs:148`), which sets I and U and leaves S clear. Setting bit 18 makes the reset value `0x8000_0000_0014_0100`. This is a guest-visible change and D7 records it.

SXL and UXL read 2 and ignore writes. For an RV64 hart, norm `mstatus_sxl_acc_mxlen64` makes SXL a WARL field encoding SXLEN once S-mode exists, and 0 is not an encoding. Norm `mstatus_uxl_acc_mxlen64` says the same for UXL once U-mode exists, and today bits 33:32 are writable and reset to 0. Norms `mstatus_sxl_rdonly_mxlen64` and `mstatus_uxl_rdonly_mxlen64` allow both fields to be read-only, and the MXL encoding table (`misa_mxl_enc`) gives 2 for XLEN 64. The current mask excludes bits 35:34, so SXL is already read-only, but it reads 0. Both fields read 2.

The H-extension `vs*` CSRs (`src/csr/mod.rs:174-182`) are not part of this profile. The hypervisor chapter defines those VS CSRs, and the CSR chapter lists them as hypervisor registers. Norm `misa_H_op` enables the H extension through `misa` bit 7, which stays 0. A hart that does not implement H does not have those addresses, and norm `Zicsr_nonexistent_rsv` reserves access to a non-existent CSR address. They are removed from the public CSR file, so access raises an illegal-instruction trap. The hypervisor remains a non-goal.

**M3. MPRV.** DECIDED. MPRV is in scope, and D9 carries it. No separate behavior is defined here.

**M4. Translation cache on the public path.** DECIDED. v1 does not cache translations on the public path. Every translated access walks the page table. SFENCE.VMA is still decoded and executed, and with nothing cached it is an ordering no-op. This drops the roadmap Stage 4 deliverable named "TLB integration" for v1, and that deliverable is recorded as deferred, not delivered. The component TLB stays available to its own tests, and the two defects in §2, the missing D update on a hit and the dropped superpage VPN bits, are fixed so those tests no longer record the wrong result.

**A/D observation.** DECIDED with D4. A successful A/D write is a visible memory effect and is not an entry in a faulting instruction's `CommitRecord`.

## 9. Risks and activation boundary

The risks that follow from the decisions:

- A walk read that loses the target, host, protocol, and unknown distinction would turn a `SimulatorFailure` into a guest fault. D3 makes the three outcomes separate acceptance checks.
- Reusing the component walker without the §2 fixes would accept a reserved PTE, translate a misaligned superpage, and report every failed walk read as one flattened access fault. Those fixes are in scope.
- The deferred Stage 3 comparison, CI publication, and noise calibration have no owner after the D0 waiver. Stage 5 and later inherit the waiver unless a later milestone picks the work up. This contract does not.
- A `satp` write implemented as a trap would break the WARL rule in D2. The acceptance check writes an unsupported MODE and expects the old value.
- Translating after `hart_issued_paddr` (`src/core/mod.rs:463`) would make a PTE PPN a storage offset. D7 puts translation first, and the mapped fixture uses guest physical PPNs.
- A fixture that executes WFI hits the existing unsupported-legal-instruction path. Stage 4 fixtures do not execute WFI, and D10 does not change that path.
- Bare-guest behavior can drift. The bare fixture and the A6 through A10 suites are the guard.

Activation boundary. This document becomes the Current contract only through the rolling workflow. It is not activated by the decisions recorded here. A separate approval has to accept this revised contract, and a separate documentation-only rotation then archives A10 and replaces `docs/dev-plan.md`. A10's own closeout still has a pending independent review, and that review is an A10 item. This document authorizes no implementation and no archive.
