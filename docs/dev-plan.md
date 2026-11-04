# Development Plan

**Current milestone on rotation merge:** Blocking TLM and SystemC Transport

**Status:** Current approved target, activated by this contract-rotation merge. Until that merge, `main` retains RV64GC Default Profile as Current. On merge this is the sole Current contract and the [entire RV64GC default-profile contract](archive/milestones/rv64gc-default-profile.md) becomes historical. Activation changes no simulator behavior and makes no implementation-completeness claim.

**Authority:** Normative milestone contract, constrained by accepted ADR-0001–0004 and their amendments through the 2026-11-04 blocking-transport amendments to ADR-0002, ADR-0003 and ADR-0004. Source code and behavioral tests establish implemented behavior; CI and PR history retain check and review outcomes.

**Approved:** 2026-11-04. After the RV64GC default, the owner selected TLM/SystemC integration (post-A7 roadmap Stage 7) as the next milestone under the delegation of full planning authority to the maintaining agent, who selected decisions D1–D12 below, all recommended choices. The [proposal entry](proposals/post-gc-blocking-tlm.md) is informational, not another current contract.

## 1. Objective and inherited profile

Make guest RAM replaceable by a blocking TLM transport without a second Hart execution path. A Machine selects either the existing native Platform route (the default, unchanged) or a TLM route in which RAM accesses, atomics and image installation go through a blocking `b_transport`-style target. The same public fixture produces equivalent architectural, fault, event and time facts under both. A narrow C ABI then lets one pinned SystemC kernel host the Machine and supply that RAM target, proven by one smoke fixture.

The inherited profile is everything delivered through the completed RV64GC default-profile contract: `rv64imafdc` as the default with `rv64ima`, `rv64imac` and `rv64imafd` selectable, M/S/U, the Stage 5 platform rules, Debug Specification 1.0 with program buffer and SBA, and zero-entry PMP plus Sv39. The existing `src/tlm` vocabulary (`TlmGenericPayload`, `TlmResponseStatus`, `ScTime`, `TlmTarget`) is component code with no public route today; this contract wires a bounded part of it.

The inherited Stage 3 performance-certification waiver continues. `tools/a10/public-v1.json`, its fixtures and its pinned argv stay unchanged, and A10 runs only on the native route.

## 2. Selected decisions

References: ISA Manual 20250508, Debug Specification 1.0 at `5695c0aaa7a3c228e14702e1268eb5a5e50e3cb8`, IEEE 1666-2023 (SystemC, including TLM-2.0), and Accellera SystemC 3.0.2 at commit `70b0fc8e4a74acc677b0fc73cea08f940c2115d5`. The OpenOCD, Spike and riscv-arch-test pins of the archived contract are unchanged.

Every decision below was approved on 2026-11-04 under the delegation recorded above.

| Decision | Accepted selection |
| --- | --- |
| D1. Delivery | One bounded first cut of roadmap Stage 7: a blocking Rust TLM RAM route, then a C ABI, then one pinned SystemC smoke. DMI, non-blocking transport, TLM-modeled devices and temporal decoupling are later contracts. |
| D2. Transport selection | `MachineConfig` gains an additive transport selection: `native` (default, behavior unchanged) or `tlm` with a caller-supplied RAM target. The TLM route replaces only the guest RAM region. CLINT, PLIC, UART, the HTIF `tohost`/`fromhost` words and the simulator-only test device stay native Platform devices. Hart, Platform address map and DM rules are unchanged; ISA code never sees a TLM type. Existing public signatures keep their shape. `run --transport native\|tlm` exposes the selection on the CLI; `tlm` there uses the in-tree Rust RAM target. Both facades keep the native route. |
| D3. Request mapping | One `PhysicalRequest` becomes one blocking generic-payload transaction: explicit physical address, `Read`/`Write` command, data length equal to the access width, streaming width equal to the length, no byte enables, little-endian raw bytes, and the request category carried in a ruscv-sim payload extension. Instruction fetch, data, page-table/A-D, debug SBA and program-buffer accesses to RAM all use this route. No access is split, merged, cached or retried by the adapter. |
| D4. Status mapping | Per ADR-0002 §4: `TLM_OK_RESPONSE` is completion. `TLM_ADDRESS_ERROR_RESPONSE`, `TLM_COMMAND_ERROR_RESPONSE`, `TLM_BURST_ERROR_RESPONSE` and `TLM_BYTE_ENABLE_ERROR_RESPONSE` are guest-visible target faults (the access kind's access fault). `TLM_GENERIC_ERROR_RESPONSE` is a host failure. `TLM_INCOMPLETE_RESPONSE` left by the target, or a payload whose address, command or length the target changed, is a protocol failure. A target that reports it cannot vouch for the outcome, an adapter panic or a C++ exception is an unknown completion. Only target faults are guest-visible; no TLM status, `sc_stop` or callback return becomes a ruscv-sim terminal result. |
| D5. Atomics | LR, SC and every AMO cross the transport as one transaction with a ruscv-sim atomic payload extension that encodes the operation, width, ordering bits and the SC committed-write check as data, not as a Rust function pointer. The target applies the operation indivisibly within that one call and returns the old value and SC outcome. A target that does not accept the extension answers `TLM_COMMAND_ERROR_RESPONSE`, which is a store/AMO access fault. Read-then-write emulation is forbidden. Reservations stay per-Hart architectural state. |
| D6. Image installation and reset | The Platform installs ELF segments into TLM RAM through the target's debug transport (`transport_dbg` semantics: no delay, no side effects), never through `b_transport`. Any installation failure fails the load before the first turn. An unknown completion poisons the transport: no further access is issued, and reset first obtains the target's transport-reset acknowledgement, then reinstalls the image. A target that tries to apply a held write after an unknown completion cannot change the reinstalled image. |
| D7. Delay and time | Native responses carry zero delay. A TLM response's annotated delay is converted to Platform ticks with a declared tick period (default 1 ns), exactly when representable and otherwise rounded up, per ADR-0004 §5.2; overflow or a malformed delay is an adapter failure. It is consumed per ADR-0004 §5.3 and advances the same Platform time (`mtime`) as completed turns, so timer behavior sees it. `mcycle` and `minstret` never include it. The Machine reports the total consumed transport delay in its boundary facts (zero on native). |
| D8. C ABI | A `capi` cargo feature adds a C ABI module plus a hand-written header `include/ruscv_sim.h`; the static library is built with `cargo rustc --features capi --crate-type staticlib`, so `Cargo.toml` crate types and every default build are unchanged. Opaque handles; create, load ELF, run with a turn budget, read boundary facts and result, reset, destroy. The RAM target is a table of C callbacks (`b_transport`, `transport_dbg`, transport reset) with user data, valid until destroy. Calls are accepted only on the creating thread; callbacks run only synchronously inside a ruscv-sim call on that thread; any re-entry from a callback is rejected with an error code. Rust panics are caught at the boundary and poison the handle. Errors are integer status codes; no Rust or C++ exception crosses the ABI. Payload buffers are owned by Rust and valid only for the duration of one callback. |
| D9. SystemC host | The dev image builds Accellera SystemC 3.0.2 at the pinned commit with C++17 through a reviewed Dockerfile change. A small CMake project under `integration/systemc/` builds a RAM target module with a `simple_target_socket` (`b_transport`, `transport_dbg` and the atomic extension) and an initiator `SC_THREAD` that drives the Machine through the C ABI in bounded grants. After each grant it calls `wait()` with the returned consumed time, and it calls `sc_stop` only after ruscv-sim reports a terminal result. The SystemC side commits global time only from the returned amount (ADR-0004 §4.4). Linux x86-64 only. |
| D10. Parity fixtures | Rust tests compare native and the Rust TLM route on widths 1/2/4/8, raw bytes, each D4 status, injected host/protocol/unknown completions, every AMO and LR/SC outcome, and a non-zero-delay control. The guest suite runs every guest a second time under `--transport tlm`; for the named parity set (`hello`, `s5_timer`, `s5_external`, `atomic_tohost_exit` and `fd_fma`, each under its declared profile) the `--log-commits` output is byte-identical to native. The SystemC smoke runs `atomic_tohost_exit` and `fd_fma` and compares exit code and commit log with the native run. |
| D11. CI and build hygiene | `cargo build`/`test --all-features` need no C++ toolchain or SystemC. The C ABI gets a plain-C compile-and-link smoke in the `Quality and tests` job. The SystemC smoke runs in a new CI job on the dev image. The unused `RiscvCore::tlm_interface` field stays unchanged and is proven not to route accesses. |
| D12. Documentation | `docs/integration/systemc-tlm.md` becomes the normative reference for D2–D9 (closing its required-decisions list), with the README, CLI help, CHANGELOG and verification docs stating the transport option, the C ABI and the SystemC pin. |

## 3. Ownership and semantics

The Machine owns the transport selection as composition configuration; the Platform owns the RAM route, its installation and its reset, as amended in ADR-0003. The TLM route is another implementation behind the existing validated physical and atomic access boundary, per ADR-0002 §8, not a second Hart path. In the SystemC host the external kernel owns global time and the outer thread; the C ABI host is a Runner-side adapter that only grants budgets and classifies results, per ADR-0003 and ADR-0004 §4.4.

## 4. Sequenced units and acceptance

These are requirements for subsequent implementation. Activating this contract does not implement them.

| Unit | Required observable proof |
| --- | --- |
| U1. Rust TLM RAM route | Per D2–D4 and D6: transport selection on `MachineConfig` and `run --transport`, request and status mapping, debug-transport image installation, unknown-completion poisoning and reset. Rust parity tests for widths, bytes, every status and injected host/protocol/unknown completions, plus debug SBA and program-buffer RAM access through the route. |
| U2. Atomic extension | Per D5: every AMO, LR and SC outcome over the extension matches native; a target without the extension yields the access fault; a counting target proves exactly one transaction per atomic and no read-then-write. |
| U3. Delay and time | Per D7: exact and rounded-up conversion, overflow and malformed-delay failures, consumption by outcome per ADR-0004 §5.3, `mtime` advanced by turns plus delay ticks, counters unaffected, and the boundary-fact total. |
| U4. Guest parity | Per D10: the guest suite's second pass under `--transport tlm` and byte-identical commit logs for the parity set. Native results and A10 unchanged. |
| U5. C ABI | Per D8 and D11: header, `capi` module and plain-C smoke in CI covering lifecycle, wrong-thread and re-entry rejection, panic poisoning, and an unknown completion followed by reset. |
| U6. SystemC smoke | Per D9: the reviewed dev-image change with the pinned SystemC build, `integration/systemc/` and the new CI job running both smoke guests with native comparison. |
| U7. Exit | CI runs the full Rust quality gate, the guest suite with its TLM pass, the C ABI smoke, the SystemC smoke, the tool script and the mutation runner. Obtain an independent review of the final committed PR head. The A10 oracle and certification deferral remain unchanged. |

Dependencies: U1 first. U2, U3 and U5 require U1 and may proceed in parallel. U4 requires U2 and U3. U6 requires U2, U3 and U5. U7 requires all.

Required negative controls, under the existing mutation-runner pattern:

- the default transport not being native;
- `TLM_GENERIC_ERROR_RESPONSE` mapped to a guest-visible fault;
- an incomplete or target-modified payload accepted as completion;
- an AMO emulated as a separate read and write;
- a positive sub-tick delay rounded down to zero;
- transport delay counted into `mcycle` or `minstret`;
- image installation issued through `b_transport`;
- an access issued after an unknown completion without reset;
- a callback re-entering the C ABI accepted;
- an `sc_stop` or TLM status reported as a passing ruscv-sim result.

Keep their small patches, commands and expected assertions under the existing mutation-runner pattern. The standard gate and the no-ledger rule are unchanged.

## 5. Exclusions and retained direction

No DMI, `nb_transport`, TLM phases beyond blocking, TLM-modeled CLINT/PLIC/UART/HTIF, SystemC-modeled interrupt lines, temporal decoupling or quantum keeper, multi-Hart or DMA, inbound masters other than the existing DM, non-Linux FFI targets, package or crate publication, A10 change, ACT4 selection change, or new ISA extension or profile.

DMI for a qualified RAM region, with permissions, latency, invalidation and breakpoint/self-modifying/MMU/atomic interactions, is the retained next step of Stage 7 and requires its own contract. Non-blocking transport requires a separately approved timing contract. Stage 8 remains conditional.

## 6. Activation boundary

This documentation-only rotation preserves the entire RV64GC default-profile contract, applies the bounded 2026-11-04 blocking-transport amendments to ADR-0002, ADR-0003 and ADR-0004, and activates this target atomically on merge. It does not assert runtime support, start implementation, change the dev image, publish a release or seal an oracle.

## References

- [ADR-0001](architecture/decisions/0001-hart-execution-outcome-and-observation.md), [ADR-0002](architecture/decisions/0002-physical-access-transaction-and-fault.md), [ADR-0003](architecture/decisions/0003-runner-machine-and-platform-ownership.md), [ADR-0004](architecture/decisions/0004-interrupt-time-scheduling-and-stop-boundaries.md).
- [Archived RV64GC default-profile contract](archive/milestones/rv64gc-default-profile.md), [post-A7 roadmap](proposals/post-a7-roadmap.md), [SystemC/TLM boundary](integration/systemc-tlm.md) and [blocking-transport proposal entry](proposals/post-gc-blocking-tlm.md).
