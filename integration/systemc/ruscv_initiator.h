// ruscv_initiator.h - the ruscv-sim C ABI target table on a TLM initiator.
//
// dev-plan D9. ruscv-sim is the initiator of every guest-RAM access, so the
// C ABI's `ruscv_target` callbacks are the initiator side of this composition:
// each callback builds one `tlm_generic_payload` plus the ruscv-sim extension
// and calls it through this module's initiator socket on the bound RAM target.
//
// The mapping is total and deliberately narrow:
//
// * the address, command, data length and streaming width are the ruscv-sim
//   request fields; byte enables stay absent and one access is one transaction;
// * the response status starts incomplete and must be one of the seven standard
//   statuses on return, and the target's address, command and length are echoed
//   back so the ruscv-sim adapter keeps its own protocol classification;
// * the payload's `dmi_allowed` hint is the transaction's TLM hint flag, the
//   only licence for a direct-memory query;
// * the added delay is reported as exact femtoseconds, which requires the 1 fs
//   time resolution the driver establishes;
// * the delay argument is the Hart's local offset (quantum D13): the request
//   field `local_offset_fs` is passed in as that argument, and the ABI's
//   `delay_fs` response carries only what the target added above it, so an
//   offset is never charged as a delay;
// * the atomic extension's request half is copied in and its response half is
//   copied out, so no Rust or C++ function pointer and no C++ type crosses the
//   ABI.
//
// With `--dmi` the table's `get_direct_mem_ptr` is the IEEE 1666 query of the
// same name, and the socket's registered `invalidate_direct_mem_ptr` forwards
// to `ruscv_machine_invalidate_direct_mem`, the one ABI entry a target may
// call inside a callback (direct-memory D12/D13).
//
// The callbacks run synchronously on the SystemC kernel thread inside one
// ruscv-sim call, so they must not advance time, wait, or call back into the C
// ABI beyond that one entry.

#ifndef RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H
#define RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H

#include <cstddef>
#include <cstdint>

#include <ruscv_sim.h>
#include <systemc>
#include <tlm>
#include <tlm_utils/simple_initiator_socket.h>

#include "ram_target.h"

namespace ruscv_sim {

/// Bridges the ruscv-sim blocking target callbacks onto one TLM initiator socket.
class RuscvInitiator : public sc_core::sc_module {
public:
    /// The initiator port the target callbacks drive.
    tlm_utils::simple_initiator_socket<RuscvInitiator> socket;

    /// Binds `socket` to `ram.socket` and keeps `ram` for its transport reset
    /// and the target-originated invalidation path. `direct_mem` selects
    /// direct memory for the table (`--dmi`), and `quantum_ticks` selects the
    /// quantum in Platform ticks (`--quantum`), zero for off (quantum D13).
    ///
    /// `fault_keep_offset` is the smoke's `--fault-keep-offset` control
    /// (quantum §4): the callbacks report the full delay argument, offset
    /// included, as an added delay, which exceeds this target's declared
    /// maximum of zero. It exists to prove that assertion and is never a
    /// normal bridge.
    RuscvInitiator(sc_core::sc_module_name name, RamTarget& ram, bool direct_mem,
                   uint64_t quantum_ticks, bool fault_keep_offset);

    /// The C ABI target table naming this object as `user`.
    ///
    /// The table declares zero per-transaction maximum delay, which is this RAM
    /// model's declared bound (dev-plan D7), and it carries the selected quantum
    /// (`--quantum`, zero for off) in `quantum_ticks` (quantum D12/D13). With
    /// `direct_mem` off the table's
    /// `get_direct_mem_ptr` stays NULL, so ruscv-sim issues no query and
    /// ignores every hint exactly as direct memory off. The returned table is
    /// valid while this object lives.
    ruscv_target target_table();

    /// Records the live C ABI handle, or nullptr once it is gone. Every call
    /// into the ABI, including a target-initiated invalidation inside a
    /// callback, names this handle on the creating thread.
    void bind_machine(ruscv_machine* machine) { machine_ = machine; }

    /// Serves one blocking transaction from the C ABI (one `b_transport` call).
    void serve_b_transport(ruscv_payload& payload);

    /// Serves one debug transport request; returns the bytes transferred.
    size_t serve_transport_dbg(ruscv_debug& debug);

    /// Serves the target's transport reset; returns nonzero when acknowledged.
    int32_t serve_transport_reset();

    /// Serves one direct-memory query (one `get_direct_mem_ptr` call); the
    /// answer is `RUSCV_DMI_GRANTED`, `RUSCV_DMI_DENIED` or `RUSCV_DMI_FAILED`.
    int32_t serve_get_direct_mem_ptr(ruscv_direct_mem& query);

    /// Invalidates every grant through the target's backward call, the
    /// `--dmi-invalidate-every` schedule the driver applies between grants.
    /// Each call is counted so a report can prove the schedule ran.
    void invalidate_all_direct_mem();

    /// Number of direct-memory invalidations this initiator forwarded to
    /// `ruscv_machine_invalidate_direct_mem`, from any cause.
    uint64_t direct_mem_invalidation_count() const { return direct_mem_invalidations_; }
    /// Number of `invalidate_all_direct_mem` calls the host scheduled.
    uint64_t direct_mem_scheduled_count() const { return direct_mem_scheduled_; }
    /// Number of backward invalidation calls the ABI refused.
    uint64_t direct_mem_error_count() const { return direct_mem_errors_; }

private:
    /// The non-blocking backward path is unused: this composition issues only
    /// blocking transactions, so the callback can never affect a result.
    tlm::tlm_sync_enum nb_transport_bw(tlm::tlm_generic_payload& transaction,
                                       tlm::tlm_phase& phase, sc_core::sc_time& delay);

    /// The target's backward invalidation forwards to
    /// `ruscv_machine_invalidate_direct_mem`, the one ABI entry accepted
    /// inside a callback, on the creating thread (direct-memory D8/D12). A
    /// call while no handle is bound is a no-op: after destroy the handle's
    /// target wrapper and its registry no longer exist, and no live grant can
    /// still name them, so there is nothing left to invalidate.
    void invalidate_direct_mem_ptr(sc_dt::uint64 start_range, sc_dt::uint64 end_range);

    RamTarget& ram_;
    const bool direct_mem_;
    const uint64_t quantum_ticks_;
    const bool fault_keep_offset_;
    ruscv_machine* machine_ = nullptr;
    uint64_t direct_mem_invalidations_ = 0;
    uint64_t direct_mem_scheduled_ = 0;
    uint64_t direct_mem_errors_ = 0;
};

/// The C ABI callbacks, one per `ruscv_target` field.
///
/// They cast `user` to the `RuscvInitiator` named in the table and forward. Each
/// one contains every C++ exception, so none can unwind into the ruscv-sim call
/// that made it: a blocking exception becomes an unknown completion, a debug or
/// reset exception becomes a failure result, and a direct-memory query
/// exception is the `RUSCV_DMI_FAILED` answer.
extern "C" void ruscv_systemc_b_transport(void* user, ruscv_payload* payload);
extern "C" size_t ruscv_systemc_transport_dbg(void* user, ruscv_debug* debug);
extern "C" int32_t ruscv_systemc_transport_reset(void* user);
extern "C" int32_t ruscv_systemc_get_direct_mem_ptr(void* user,
                                                    ruscv_direct_mem* direct_mem);

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H
