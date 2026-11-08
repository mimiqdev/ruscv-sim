/*
 * ruscv_sim.h - the ruscv-sim blocking-transport C ABI (dev-plan D8).
 *
 * Build the library with the `capi` feature; no crate type and no C/C++
 * toolchain is added to a default or --all-features build:
 *
 *     cargo rustc --lib --features capi --crate-type staticlib
 *
 * Link the resulting `libruscv_sim.a` with the C runtime of the platform
 * (Linux x86-64 only), for example:
 *
 *     cc -std=c11 -Iinclude host.c libruscv_sim.a -lpthread -ldl -lm
 *
 * The facade is a Runner-side hosting adapter.  The caller owns global
 * simulation time and the outer thread; it grants bounded budgets here and
 * classifies the returned facts itself.  A transport status, a callback return
 * or a host stop never becomes a ruscv-sim terminal result (ADR-0003, the
 * 2026-11-04 blocking-transport amendment; ADR-0004 §4.4/§7).
 *
 * Rules:
 *
 *   - Every handle is owned by the thread that created it.  A call from any
 *     other thread fails with RUSCV_ERROR_NOT_CREATOR_THREAD and changes no
 *     state.
 *   - Target callbacks run inside a ruscv-sim call on that thread.  Any call
 *     back into this ABI from a callback fails with RUSCV_ERROR_REENTRANT,
 *     whichever handle it names.  One call is exempt:
 *     ruscv_machine_invalidate_direct_mem touches only the handle's
 *     direct-memory registry, so a target may call it from inside its own
 *     callback, and it is also accepted on a handle poisoned by a caught panic.
 *     Every other entry point still rejects a callback and a poisoned handle.
 *   - A b_transport callback whose payload has `may_suspend` set may suspend
 *     the calling process and let host time pass, provided it resumes and
 *     returns on the creating thread (nb D6).  The callback state stays set
 *     while it is suspended, so the re-entry rule above holds across it: every
 *     entry another process on that thread makes that reaches the callback
 *     check returns RUSCV_ERROR_REENTRANT, including ruscv_machine_destroy on a
 *     live handle, while ruscv_machine_invalidate_direct_mem stays accepted.
 *     An entry that fails its argument checks first, or
 *     ruscv_machine_destroy(NULL), keeps its existing result, and a call from
 *     another OS thread is still RUSCV_ERROR_NOT_CREATOR_THREAD.  A host must
 *     not stop or tear down its kernel, kill or reset the suspended process, or
 *     destroy the handle while a callback is suspended; a refused destroy
 *     leaks the handle rather than freeing it under a live frame.  Every other
 *     callback (transport_dbg, transport_reset, get_direct_mem_ptr, and
 *     b_transport with `may_suspend` clear) must not wait or advance host time.
 *   - A device-table callback (ruscv_device) obeys the RAM target's rules: it
 *     runs on the creating thread only, is never re-entered, never waits, never
 *     lets host time pass and never crosses the boundary as an exception.  A
 *     device transaction carries `may_suspend` clear, so a suspending callback
 *     there is refused like every other non-suspending one.
 *   - A Rust panic is caught at the boundary: the handle is poisoned and the
 *     call returns RUSCV_ERROR_PANIC.  Every later call returns
 *     RUSCV_ERROR_POISONED, except ruscv_machine_invalidate_direct_mem and
 *     ruscv_machine_destroy.
 *   - Payload data pointers are owned by ruscv-sim and valid only for the
 *     duration of one callback.
 *
 * Layout rule:
 *
 *   No published struct carries a size or version field, so fields are only
 *   ever appended: every existing field keeps its offset and its size.  The
 *   final fields are ruscv_target.suspends_host_time,
 *   ruscv_payload.device_event, ruscv_boundary.quantum_reached and
 *   ruscv_grant.host_elapsed_fs, which follow the direct-memory, quantum,
 *   suspension and device additions (ruscv_target.get_direct_mem_ptr and
 *   ruscv_target.quantum_ticks, ruscv_payload.dmi_allowed,
 *   ruscv_payload.local_offset_fs and ruscv_payload.host_elapsed_fs).  A host
 *   compiled against a previous header therefore keeps working source-level
 *   but must be recompiled against this header before it links this library;
 *   the ABI is not binary-compatible with the previous header.
 */

#ifndef RUSCV_SIM_H
#define RUSCV_SIM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ status */

/* Integer status codes; no Rust or C++ exception crosses the boundary. */
#define RUSCV_OK 0
#define RUSCV_ERROR_INVALID_ARGUMENT 1
#define RUSCV_ERROR_NOT_CREATOR_THREAD 2
#define RUSCV_ERROR_REENTRANT 3
#define RUSCV_ERROR_POISONED 4
#define RUSCV_ERROR_PANIC 5
#define RUSCV_ERROR_IO 6
#define RUSCV_ERROR_ELF 7
#define RUSCV_ERROR_CONFIG 8
#define RUSCV_ERROR_TIME_AUTHORITY 9
#define RUSCV_ERROR_MACHINE 10
#define RUSCV_ERROR_NO_IMAGE 11
#define RUSCV_ERROR_UNKNOWN_COMPLETION 12

/* ------------------------------------------------------------------- types */

/* Selectable Machine ISA profile (contract D3 of the RV64GC default profile). */
#define RUSCV_ISA_RV64IMA 0
#define RUSCV_ISA_RV64IMAC 1
#define RUSCV_ISA_RV64IMAFD 2
#define RUSCV_ISA_RV64IMAFDC 3

/* Blocking-transport command. */
#define RUSCV_COMMAND_READ 0
#define RUSCV_COMMAND_WRITE 1

/* The existing physical access category carried by the payload extension. */
#define RUSCV_ACCESS_FETCH 0
#define RUSCV_ACCESS_DATA_READ 1
#define RUSCV_ACCESS_DATA_WRITE 2

/* Who issued the request. */
#define RUSCV_INITIATOR_HART 0
#define RUSCV_INITIATOR_DEBUG_INBOUND 1

/* The seven IEEE 1666 blocking response statuses. */
#define RUSCV_TLM_INCOMPLETE 0
#define RUSCV_TLM_OK 1
#define RUSCV_TLM_ADDRESS_ERROR 2
#define RUSCV_TLM_COMMAND_ERROR 3
#define RUSCV_TLM_BURST_ERROR 4
#define RUSCV_TLM_BYTE_ENABLE_ERROR 5
#define RUSCV_TLM_GENERIC_ERROR 6

/* Atomic request kind. */
#define RUSCV_ATOMIC_RMW 0
#define RUSCV_ATOMIC_LOAD_RESERVED 1
#define RUSCV_ATOMIC_STORE_CONDITIONAL 2

/* AMO operation identity (the Hart-owned operation table). */
#define RUSCV_AMO_ADD 0
#define RUSCV_AMO_SWAP 1
#define RUSCV_AMO_XOR 2
#define RUSCV_AMO_OR 3
#define RUSCV_AMO_AND 4
#define RUSCV_AMO_MIN 5
#define RUSCV_AMO_MAX 6
#define RUSCV_AMO_MINU 7
#define RUSCV_AMO_MAXU 8

/* Atomic completion the target records in the same transaction. */
#define RUSCV_ATOMIC_OUTCOME_NONE 0
#define RUSCV_ATOMIC_OUTCOME_RMW 1
#define RUSCV_ATOMIC_OUTCOME_LOAD_RESERVED 2
#define RUSCV_ATOMIC_OUTCOME_STORE_CONDITIONAL_SUCCESS 3
#define RUSCV_ATOMIC_OUTCOME_STORE_CONDITIONAL_FAILURE 4

/* Capacity of one committed-write snapshot buffer, in bytes. */
#define RUSCV_SNAPSHOT_MAX 64

/* The device windows ruscv_machine_attach_device selects (device D8, htif D7).
 * RUSCV_DEVICE_UART is the UART window `0x1000_0000` with `0x100` bytes;
 * RUSCV_DEVICE_HTIF is the HTIF tohost endpoint `0x4000_8000` with 8 bytes. */
#define RUSCV_DEVICE_UART 0
#define RUSCV_DEVICE_HTIF 1

/* The at-most-one guest-visible effect an accepted device access reports
 * (device D8).  The value travels with the event, never in a second field. */
#define RUSCV_DEVICE_EVENT_NONE 0
#define RUSCV_DEVICE_EVENT_UART_TRANSMIT 1
#define RUSCV_DEVICE_EVENT_HTIF_WRITE 2

/* Direct-memory access (the IEEE 1666 DMI access values). */
#define RUSCV_DMI_ACCESS_NONE 0
#define RUSCV_DMI_ACCESS_READ 1
#define RUSCV_DMI_ACCESS_WRITE 2
#define RUSCV_DMI_ACCESS_READ_WRITE 3

/* The three answers a get_direct_mem_ptr callback may give. */
#define RUSCV_DMI_GRANTED 0
#define RUSCV_DMI_DENIED 1
#define RUSCV_DMI_FAILED 2

/* Run classification. */
#define RUSCV_RUN_RUNNING 0
#define RUSCV_RUN_EXITED 1
#define RUSCV_RUN_FAILED 2

/* Machine lifecycle acknowledgment. */
#define RUSCV_LIFECYCLE_RUNNING 0
#define RUSCV_LIFECYCLE_QUIESCE_REQUESTED 1
#define RUSCV_LIFECYCLE_DRAIN_COMPLETE 2
#define RUSCV_LIFECYCLE_TORN_DOWN 3

/* An unbounded Hart-turn budget. */
#define RUSCV_TURNS_UNBOUNDED UINT64_MAX

/* The ruscv-sim atomic payload extension (dev-plan D5). */
typedef struct ruscv_atomic {
    /* Request: nonzero when this transaction carries the atomic extension. */
    int32_t present;
    /* Request: RUSCV_ATOMIC_*. */
    int32_t kind;
    /* Request: RUSCV_AMO_* for RMW, -1 otherwise. */
    int32_t operation;
    /* Request: transfer width in bytes, 4 or 8. */
    int32_t width;
    /* Request: aq/rl ordering bits. */
    int32_t acquire;
    int32_t release;
    /* Request: nonzero when a store-conditional carries its reservation. */
    int32_t reserved;
    uint64_t reserved_address;
    uint32_t reserved_length;
    uint32_t reserved_snapshot_length;
    uint8_t reserved_snapshot[RUSCV_SNAPSHOT_MAX];
    /* Response: RUSCV_ATOMIC_OUTCOME_*. */
    int32_t outcome;
    uint32_t snapshot_length;
    uint8_t snapshot[RUSCV_SNAPSHOT_MAX];
} ruscv_atomic;

/*
 * One blocking transaction (one b_transport call, dev-plan D3).
 *
 * `data` points at a Rust-owned buffer of exactly `length` bytes: a read target
 * writes the response bytes there, a write target reads the request bytes.  The
 * status starts at RUSCV_TLM_INCOMPLETE and the target must select one terminal
 * status; a target that leaves it or changes address/command/length has not
 * completed the transaction.  A write target must apply the write as one
 * indivisible semantic write before returning.
 *
 * Direct memory: a target that can serve this address through a host pointer
 * sets `dmi_allowed` on a completed non-atomic transaction.  That hint is the
 * only licence for ruscv-sim's one direct-memory query of this access; it is
 * ignored with direct memory off, on a non-completion and on an atomic
 * transaction.
 *
 * Quantum: `local_offset_fs` is request metadata, the Hart's local time minus
 * the anchor plus the delay ticks this turn already charged, in exact
 * femtoseconds.  It is the counterpart of the IEEE 1666 `b_transport` delay
 * argument, so `sc_time_stamp() + delay` is the transaction's local time while
 * the grant's start is the kernel's clock.  It is timing metadata only: no
 * status, address, length or permission may depend on it, it is never charged
 * back as a delay and it is not an added latency.  It is zero with no quantum
 * selected, for every DebugInbound request and for debug transport.
 *
 * Suspension (nb D3/D4): `may_suspend` is request metadata, nonzero when this
 * is a charged Hart transaction to a target that declared
 * `suspends_host_time`.  Only such a callback may let host time pass, and it
 * reports the time that passed in `host_elapsed_fs`, in exact femtoseconds,
 * starting at zero.  Both are timing metadata: no status, address, length or
 * permission may depend on them, and every other callback must leave
 * `host_elapsed_fs` at zero.  A non-zero `host_elapsed_fs` on a request whose
 * `may_suspend` is clear is a protocol failure of that access, as is a
 * terminal response whose host elapsed time exceeds `delay_fs`.  `delay_fs`
 * counts from the transaction's issue, so it includes the host elapsed time.
 *
 * Device transaction (device D8): a request routed to a device window carries
 * `may_suspend` clear and `local_offset_fs` zero, and answers with `delay_fs`,
 * `host_elapsed_fs` and `dmi_allowed` zero.  `device_event` is the response's
 * at-most-one guest-visible effect, one of RUSCV_DEVICE_EVENT_*: NONE means the
 * access has none, UART_TRANSMIT means one byte left the UART's transmit
 * holding register, and HTIF_WRITE means one value was written to the HTIF
 * endpoint.  The event's value is read from the access's own data bytes, so no
 * second field carries it.  A tag that is not its window's, a non-zero delay,
 * host elapsed time or hint, and a changed address, command or length are each
 * a protocol failure of that access; the ABI never guesses what an
 * out-of-contract tag meant, so an unrecognized one makes the response an
 * incomplete one too.  A device target reports no more than one event per
 * access, and only on an accepted one.
 *
 * `device_event` is the final field of the struct.
 */
typedef struct ruscv_payload {
    uint64_t address;
    int32_t command;
    int32_t access_kind;
    int32_t initiator;
    uint8_t *data;
    size_t length;
    int32_t status;
    uint32_t outcome_unknown;
    uint64_t delay_fs;
    ruscv_atomic atomic;
    int32_t dmi_allowed;
    uint64_t local_offset_fs;
    /* Request: nonzero for a charged Hart transaction to a suspending target
     * (nb D3). */
    int32_t may_suspend;
    /* Response: the host time that passed inside the callback, in exact
     * femtoseconds, starting at zero (nb D4). */
    uint64_t host_elapsed_fs;
    /* Response: RUSCV_DEVICE_EVENT_* for a device transaction, zero otherwise
     * (device D8). */
    int32_t device_event;
} ruscv_payload;

/*
 * One direct-memory query and its answer (`ruscv_target.get_direct_mem_ptr`).
 *
 * The request half is filled by ruscv-sim: `address` is the physical start of
 * the completed access, `length` its width in bytes, and `access` is one of
 * RUSCV_DMI_ACCESS_READ or RUSCV_DMI_ACCESS_WRITE, the direction that access
 * needs.  The target answers exactly one of RUSCV_DMI_GRANTED,
 * RUSCV_DMI_DENIED or RUSCV_DMI_FAILED:
 *
 *   - RUSCV_DMI_GRANTED fills the response half.  `start`..`end` is the
 *     inclusive physical range, `pointer` addresses `start`, `access` is the
 *     granted RUSCV_DMI_ACCESS_READ, WRITE or READ_WRITE, and the two latencies
 *     are exact femtoseconds per access.  The range must contain the queried
 *     address, the pointer must be non-NULL, the range must not be inverted or
 *     overflow a representable length, `access` must not be
 *     RUSCV_DMI_ACCESS_NONE, and neither latency may exceed the table's
 *     `max_delay_fs`; anything else is a protocol failure of the access that
 *     queried it (never a guest fault).
 *   - RUSCV_DMI_DENIED caches nothing and is not an error.
 *   - RUSCV_DMI_FAILED means the target could not establish an answer, the C
 *     ABI's counterpart of the contained exception in the simulator's direct
 *     memory contract.  It is not a denial: the access is an unknown
 *     completion, the transport is poisoned and only the target's acknowledged
 *     transport reset followed by ruscv_machine_reset recovers.
 *
 * Anything other than those three return values has the effect of
 * RUSCV_DMI_FAILED: the ABI never guesses what an out-of-contract answer meant.
 *
 * Granting direct memory is an unsafe promise, and C has no type system to
 * carry it.  The target must satisfy every one of these obligations until it
 * has invalidated a covering range (see ruscv_machine_invalidate_direct_mem):
 *
 *   1. Pointer lifetime.  `pointer` addresses `start` and stays valid for the
 *      whole inclusive `start`..`end` range at that address.  ruscv-sim holds no
 *      pointer outside the adapter and never reads it after an invalidation
 *      returns, so the target must not free, move, remap or narrow that storage
 *      before it has invalidated the range.  Before the reset callback runs,
 *      ruscv-sim invalidates every registration this handle's target wrapper
 *      keeps, so storage that a reset discards is covered for this handle too.
 *   2. Aliasing.  The pointer addresses the same bytes b_transport and
 *      transport_dbg serve, so a write through any of the three is visible to
 *      the others at once.  A range may be granted only where b_transport
 *      would complete with RUSCV_TLM_OK for the granted access, so faults stay
 *      identical.
 *   3. Byte-wise atomic access.  ruscv-sim copies through the pointer one byte
 *      at a time with relaxed atomic loads and stores (it never assumes a Rust
 *      reference or an exclusive alias).  The bytes behind the pointer must
 *      therefore tolerate those accesses without a data race: use an array of
 *      _Atomic unsigned char (or equally atomic storage) and access it
 *      atomically everywhere, including in b_transport and transport_dbg.
 *   4. Invalidate-and-free ordering.  Before the target frees, moves, remaps or
 *      narrows a granted range, or changes the status b_transport would report
 *      for it, it must call ruscv_machine_invalidate_direct_mem for a covering
 *      range and must refuse new grants across that window.  A returning
 *      invalidation proves that no copy through a removed pointer is in flight.
 *      The target may call it between grants, from inside b_transport or from
 *      inside transport_reset, on the creating thread only.
 *   5. Version clock.  A load-reserved snapshot and a store-conditional check
 *      are committed-write versions, and no version may repeat across a
 *      transport reset.  A direct store leaves a byte's version in place, so a
 *      repeated version could let a reservation retained across a failed
 *      recomposition pass a store-conditional that fails with direct memory
 *      off.
 *
 * Running several direct-memory Machines concurrently on one target is
 * memory-safe but architecturally unspecified; sequential reuse of a shared
 * target is specified.  Because each handle owns its own target wrapper, a
 * host that shares one `user` target across handles must invalidate every such
 * handle before it resets or frees the shared storage; a reset invalidates the
 * registrations of the handle being reset only.
 */
typedef struct ruscv_direct_mem {
    uint64_t address;
    size_t length;
    int32_t access;
    uint64_t start;
    uint64_t end;
    uint8_t *pointer;
    uint64_t read_latency_fs;
    uint64_t write_latency_fs;
} ruscv_direct_mem;

/* One debug-transport request (dev-plan D6): no delay, no side effects. */
typedef struct ruscv_debug {
    uint64_t address;
    int32_t command;
    uint8_t *data;
    size_t length;
} ruscv_debug;

/*
 * The caller-supplied RAM target.
 *
 * All three callbacks are required.  `user` and everything it points at must
 * stay valid until ruscv_machine_destroy.  `max_delay_fs` is the declared
 * per-transaction maximum delay used for every conservative completion bound.
 * `transport_dbg` returns the number of bytes transferred; a short transfer is
 * a failed debug transport.  `transport_reset` returns nonzero when the target
 * has quiesced and accepts new accesses.
 *
 * `get_direct_mem_ptr` is optional.  NULL means the target offers no direct
 * memory: ruscv-sim selects direct memory off for that handle, issues no query
 * and ignores every `dmi_allowed` hint, exactly as a handle whose Machine has
 * direct memory disabled.  A non-NULL callback selects direct memory for the
 * handle and must obey every obligation documented above `ruscv_direct_mem`;
 * the callback answers RUSCV_DMI_GRANTED, RUSCV_DMI_DENIED or
 * RUSCV_DMI_FAILED.
 *
 * `quantum_ticks` is the selected quantum in Platform ticks.  Zero is off,
 * which runs exactly as the previous header's ABI did.  One syncs at every
 * turn boundary and is precise.  A value of 2 or more is the explicit
 * reduced-accuracy request: a time-derived timer crossing may be recognized up
 * to `quantum_ticks - 1` ticks plus the target's declared turn completion bound
 * late, while committed time never drifts.  The table is copied at create, so
 * the selection persists across ruscv_machine_reset, and a NULL table cannot
 * select one because the native route has no local offset to bound.  A value
 * ruscv_machine_create cannot represent is RUSCV_ERROR_CONFIG.
 *
 * `suspends_host_time` is nonzero when the target lets host time pass inside a
 * `b_transport` callback whose payload has `may_suspend` set (nb D6).  Like
 * every other table field it is copied at create, so the declaration persists
 * across ruscv_machine_reset, and a NULL table selects the native route, which
 * never suspends.  A suspending table with a non-zero `quantum_ticks` is
 * RUSCV_ERROR_CONFIG from ruscv_machine_create: host time passing inside a
 * grant would stop the kernel's clock from being the quantum anchor for the
 * whole grant.  Suspension composes with direct memory.  It is the final field
 * of the struct.
 */
typedef struct ruscv_target {
    uint64_t max_delay_fs;
    void *user;
    void (*b_transport)(void *user, ruscv_payload *payload);
    size_t (*transport_dbg)(void *user, ruscv_debug *debug);
    int32_t (*transport_reset)(void *user);
    int32_t (*get_direct_mem_ptr)(void *user, ruscv_direct_mem *direct_mem);
    uint64_t quantum_ticks;
    /* Nonzero when the target lets host time pass inside a `may_suspend`
     * transaction (nb D6). */
    int32_t suspends_host_time;
} ruscv_target;

/*
 * The caller-supplied device target (device D8, htif D7).
 *
 * One table serves one device window.  `window_base` and `window_size` must
 * equal the selected window's: the UART window `0x1000_0000` with `0x100`
 * bytes for RUSCV_DEVICE_UART, or the HTIF tohost endpoint `0x4000_8000` with
 * 8 bytes for RUSCV_DEVICE_HTIF; a mismatch is RUSCV_ERROR_CONFIG from
 * ruscv_machine_attach_device.  One attach per window is allowed, so a handle
 * may carry one table for each window.  Like the RAM table, this one is copied
 * at attach, so the callbacks and `user` are fixed for the handle's life, and
 * `user` and everything it points at must stay valid until
 * ruscv_machine_destroy.
 *
 * All three callbacks are required and run under the RAM target's rules:
 * creating thread only, never re-entered, synchronously, without waiting and
 * without letting host time pass.
 *
 * `b_transport` services one validated register access.  It must set exactly
 * one status, leave `address`, `command` and `length` as it received them,
 * leave `delay_fs`, `local_offset_fs`, `host_elapsed_fs`, `may_suspend` and
 * `dmi_allowed` zero, and set `device_event` to RUSCV_DEVICE_EVENT_NONE or to
 * the tag that matches its window: RUSCV_DEVICE_EVENT_UART_TRANSMIT for the
 * UART window and RUSCV_DEVICE_EVENT_HTIF_WRITE for the HTIF window.  The
 * window's width, category and start-address policy run before this callback,
 * so it never sees a request the Platform would reject, and an atomic envelope
 * never reaches it.
 *
 * A UART request is one register byte.  An HTIF request is one complete
 * eight-byte dword at the window base: a read answers the endpoint's register
 * state with no event, and an accepted write reports exactly one
 * RUSCV_DEVICE_EVENT_HTIF_WRITE whose value is the response's eight data
 * bytes, which must equal the request's (htif D4/D10).  A host that leaves the
 * request's data bytes alone therefore reports the guest's own value, and the
 * published value is the only exit vocabulary the endpoint carries.
 *
 * `receive` delivers one byte the host pushes into the window.  It returns
 * nothing: a host that cannot accept the byte drops it.  This ABI version adds
 * no entry that pushes a byte, so it reaches the callback only when an
 * embedding caller uses the Machine's own UART receive path; it is never
 * called for RUSCV_DEVICE_HTIF, because the endpoint has no host-to-guest
 * direction.  The ABI never calls it re-entrantly and never from another
 * thread.
 *
 * `transport_reset` returns nonzero when the target has quiesced and accepts
 * new accesses and zero when it refuses.  A refusal fails the Platform build
 * as a refused RAM transport reset does, so no device state crosses a
 * generation.
 */
typedef struct ruscv_device {
    uint64_t window_base;
    uint64_t window_size;
    void *user;
    void (*b_transport)(void *user, ruscv_payload *payload);
    void (*receive)(void *user, uint8_t byte);
    int32_t (*transport_reset)(void *user);
} ruscv_device;

/* One grant's facts. */
typedef struct ruscv_boundary {
    int32_t budget_exhausted;
    int32_t deadline_reached;
    int32_t deadline_blocked;
    int32_t waiting;
    int32_t no_progress;
    int32_t uncertain;
    int32_t failed;
    int32_t lifecycle;
    uint64_t residual_slack_fs;
    /*
     * The refused grant step's conservative completion bound, in femtoseconds.
     *
     * One step can complete the waiting turn and its wake turn, so the bound
     * reserves both, each with the target's declared per-transaction maximum for
     * its worst-case transaction count.
     */
    uint64_t candidate_bound_fs;
    uint64_t transport_delay_fs;
    uint64_t turns;
    /* The selected quantum in Platform ticks; zero when off. */
    uint64_t quantum_ticks;
    /* The declared interrupt-latency bound in ticks: zero for a quantum of 1,
     * for a larger one `quantum_ticks - 1` plus the target's conservative turn
     * completion bound. */
    uint64_t interrupt_latency_bound_ticks;
    /* Cumulative counted syncs of this generation, read after the grant-return
     * sync, so the last grant of a run includes it. */
    uint64_t quantum_syncs;
    /* Nonzero when the quantum threshold was reached at the last boundary,
     * including when a forced sync coincided with it. */
    int32_t quantum_reached;
} ruscv_boundary;

/*
 * One grant result.
 *
 * `consumed_fs` is the amount of modeled time the Machine committed during the
 * grant (completed turns, idle jumps and transport delay).  The caller commits
 * global time only from this amount, so the next grant's start may not precede
 * the value it has committed so far; an earlier start is
 * RUSCV_ERROR_TIME_AUTHORITY, exactly as an earlier start of the Machine's own
 * cursor is (ADR-0004 §4.4).
 *
 * `host_elapsed_fs` is the host time that passed inside this grant's
 * transactions, in exact femtoseconds: the increase of the Machine's host
 * elapsed total from the grant's start to its return, never the cumulative
 * value.  It is set for every grant, zero for one that ran no turn.  Each
 * counted transaction is charged at least its host elapsed time, so
 * `host_elapsed_fs <= consumed_fs`; the caller waits
 * `consumed_fs - host_elapsed_fs` and keeps its check that its clock then
 * equals `committed_end_fs` (nb D5).  It is the final field of the struct.
 */
typedef struct ruscv_grant {
    uint64_t consumed_fs;
    uint64_t committed_start_fs;
    uint64_t committed_end_fs;
    int32_t boundary_reached;
    ruscv_boundary facts;
    /* The host time that passed inside this grant's transactions, in exact
     * femtoseconds (nb D5). */
    uint64_t host_elapsed_fs;
} ruscv_grant;

/* The run classification. */
typedef struct ruscv_result {
    int32_t state;
    uint32_t exit_code;
    uint64_t turns;
    uint64_t pc;
} ruscv_result;

/* One opaque handle. */
typedef struct ruscv_machine ruscv_machine;

/* ------------------------------------------------------------------ create */

/*
 * Creates one handle.
 *
 * `target` may be NULL, which selects the native SimpleMemory RAM route; a
 * non-NULL table is copied and its `user` pointer must stay valid and unchanged
 * until destroy.  `tick_period_fs` is the declared Platform tick period in
 * femtoseconds (1000000, one nanosecond, is the documented default).  On
 * success writes a handle to *out and returns RUSCV_OK.
 *
 * A table whose declared maximum, or whose quantum, cannot be represented
 * under `tick_period_fs` is RUSCV_ERROR_CONFIG.  A quantum needs the TLM route,
 * so a non-zero quantum with no table is also RUSCV_ERROR_CONFIG, and so is a
 * suspending table that selects one.
 */
int32_t ruscv_machine_create(const ruscv_target *target, int32_t isa,
                             uint64_t tick_period_fs, ruscv_machine **out);

/*
 * Attaches one host device target to the selected window (device D8, htif D7).
 *
 * `window` is RUSCV_DEVICE_UART, the UART window `0x1000_0000` with `0x100`
 * bytes, or RUSCV_DEVICE_HTIF, the HTIF tohost endpoint `0x4000_8000` with 8
 * bytes.  Each window takes its own table and each may be attached once.  The
 * call must come after ruscv_machine_create and before the first
 * ruscv_machine_load_elf: a device target is composition configuration, chosen
 * once per handle and copied, so it survives ruscv_machine_reset and is reset
 * at every Platform build.
 *
 * It is refused with RUSCV_ERROR_INVALID_ARGUMENT for a NULL handle, a NULL
 * table or an unknown window, and with RUSCV_ERROR_CONFIG for a `window_base`
 * or `window_size` that is not the selected window's, a second attach for the
 * same window, an attach after the first load, or a handle created with a NULL
 * `ruscv_target`, which has no TLM route to serve a device window.  An
 * incomplete table, one whose callbacks are not all present, is
 * RUSCV_ERROR_INVALID_ARGUMENT like an incomplete RAM table.
 *
 * A handle with no attached device target answers each window with its in-tree
 * device, which is exactly the previous behavior; the two windows are selected
 * independently.
 */
int32_t ruscv_machine_attach_device(ruscv_machine *machine, int32_t window,
                                    const ruscv_device *device);

/* Parses and installs one ELF image, replacing any previous one. */
int32_t ruscv_machine_load_elf(ruscv_machine *machine, const char *path);

/*
 * Routes the commit log to `path` (created and truncated) for later grants.
 *
 * Requires an installed image, so call it after ruscv_machine_load_elf; a later
 * load keeps the log.  Every later grant streams its committed transitions
 * there, and an observer failure ends the run as a host failure.
 */
int32_t ruscv_machine_set_commit_log(ruscv_machine *machine, const char *path);

/*
 * Runs one bounded grant.
 *
 * `start_fs` is the caller's authoritative start time, `max_end_fs` its
 * inclusive horizon and `turn_budget` the started-slot budget
 * (RUSCV_TURNS_UNBOUNDED for none).  A start earlier than the Machine's last
 * committed time, or earlier than the caller's own committed time, is
 * RUSCV_ERROR_TIME_AUTHORITY and never rewinds either cursor.  A classified run
 * failure still returns RUSCV_OK and is visible through ruscv_machine_result.
 */
int32_t ruscv_machine_run_grant(ruscv_machine *machine, uint64_t start_fs,
                                uint64_t max_end_fs, uint64_t turn_budget,
                                ruscv_grant *out);

/* Reads the facts of the most recent grant. */
int32_t ruscv_machine_boundary(ruscv_machine *machine, ruscv_boundary *out);

/* Reads the run classification. */
int32_t ruscv_machine_result(ruscv_machine *machine, ruscv_result *out);

/*
 * Reinstalls the image through the Machine's reset lifecycle.
 *
 * An ordinary reset quiesces, zero-fills the RAM window and republishes the same
 * image.  After an unknown completion the poisoned composition is never resumed
 * or rewired: a fresh composition obtains the target's transport-reset
 * acknowledgement, zero-fills and reinstalls (dev-plan D6), and the failed
 * domain stays quarantined.  A target that withholds the acknowledgement keeps
 * the handle unusable.
 */
int32_t ruscv_machine_reset(ruscv_machine *machine);

/*
 * Invalidates every cached direct-memory grant overlapping the inclusive
 * physical range `start`..`end` (direct-memory D8).
 *
 * This is the one call the ABI accepts from inside a target callback and on a
 * handle poisoned by a caught panic, because it touches only the direct-memory
 * registry owned by the Rust target wrapper and no run state.  It is still
 * accepted on the creating thread only; a call from another thread fails with
 * RUSCV_ERROR_NOT_CREATOR_THREAD and changes no state.  Every other entry point
 * remains re-entry and poison protected.  An inverted range is treated as an
 * invalidation of the whole address space.  A native handle, or a target with a
 * NULL query, has no cached grant and the call succeeds.
 *
 * A target calls this before it frees, moves or remaps granted storage; the
 * call returns only after every in-flight copy through a removed pointer has
 * finished.
 */
int32_t ruscv_machine_invalidate_direct_mem(ruscv_machine *machine, uint64_t start,
                                            uint64_t end);

/*
 * Copies the most recent diagnostic message into `buffer`, NUL-terminated and
 * truncated to fit.  Returns the message length before truncation.
 */
int32_t ruscv_machine_diagnostic(ruscv_machine *machine, char *buffer, size_t length);

/*
 * Self-test only: provokes a Rust panic inside the ABI to prove the boundary
 * catches it and poisons the handle.  Not part of the hosting interface; call it
 * only on a throwaway handle.
 */
int32_t ruscv_machine_self_test_panic(ruscv_machine *machine);

/* Destroys one handle.  Call it on the creating thread. */
int32_t ruscv_machine_destroy(ruscv_machine *machine);

#ifdef __cplusplus
}
#endif

#endif /* RUSCV_SIM_H */
