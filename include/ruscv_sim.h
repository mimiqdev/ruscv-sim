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
 *   - Target callbacks run synchronously inside a ruscv-sim call on that
 *     thread.  Any call back into this ABI from a callback fails with
 *     RUSCV_ERROR_REENTRANT.
 *   - A Rust panic is caught at the boundary: the handle is poisoned and the
 *     call returns RUSCV_ERROR_PANIC.  Every later call returns
 *     RUSCV_ERROR_POISONED.
 *   - Payload data pointers are owned by ruscv-sim and valid only for the
 *     duration of one callback.
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
} ruscv_payload;

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
 */
typedef struct ruscv_target {
    uint64_t max_delay_fs;
    void *user;
    void (*b_transport)(void *user, ruscv_payload *payload);
    size_t (*transport_dbg)(void *user, ruscv_debug *debug);
    int32_t (*transport_reset)(void *user);
} ruscv_target;

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
} ruscv_boundary;

/*
 * One grant result.
 *
 * `consumed_fs` is the amount of modeled time the Machine committed during the
 * grant (completed turns, idle jumps and transport delay).  The caller commits
 * global time only from this amount; the next grant's start is the value it has
 * committed so far.
 */
typedef struct ruscv_grant {
    uint64_t consumed_fs;
    uint64_t committed_start_fs;
    uint64_t committed_end_fs;
    int32_t boundary_reached;
    ruscv_boundary facts;
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
 * non-NULL table is copied and its `user` pointer must stay valid until destroy.
 * `tick_period_fs` is the declared Platform tick period in femtoseconds
 * (1000000, one nanosecond, is the documented default).  On success writes a
 * handle to *out and returns RUSCV_OK.
 */
int32_t ruscv_machine_create(const ruscv_target *target, int32_t isa,
                             uint64_t tick_period_fs, ruscv_machine **out);

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
 * committed time is RUSCV_ERROR_TIME_AUTHORITY and never rewinds the cursor.
 * A classified run failure still returns RUSCV_OK and is visible through
 * ruscv_machine_result.
 */
int32_t ruscv_machine_run_grant(ruscv_machine *machine, uint64_t start_fs,
                                uint64_t max_end_fs, uint64_t turn_budget,
                                ruscv_grant *out);

/* Reads the facts of the most recent grant. */
int32_t ruscv_machine_boundary(ruscv_machine *machine, ruscv_boundary *out);

/* Reads the run classification. */
int32_t ruscv_machine_result(ruscv_machine *machine, ruscv_result *out);

/*
 * Reinstalls the image on a fresh, acknowledged transport domain.
 *
 * This is the recovery path after an unknown completion: the new composition
 * obtains the target's transport-reset acknowledgement and zero-fills the RAM
 * window before any new grant (dev-plan D6).
 */
int32_t ruscv_machine_reset(ruscv_machine *machine);

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
