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
// * the annotated delay is copied as exact femtoseconds, which requires the
//   1 fs time resolution the driver establishes;
// * the atomic extension's request half is copied in and its response half is
//   copied out, so no Rust or C++ function pointer and no C++ type crosses the
//   ABI.
//
// The callbacks run synchronously on the SystemC kernel thread inside one
// ruscv-sim call, so they must not advance time, wait, or call back into the C
// ABI.

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

    /// Binds `socket` to `ram.socket` and keeps `ram` for its transport reset.
    RuscvInitiator(sc_core::sc_module_name name, RamTarget& ram);

    /// The C ABI target table naming this object as `user`.
    ///
    /// The table declares zero per-transaction maximum delay, which is this RAM
    /// model's declared bound (dev-plan D7). The returned table is valid while
    /// this object lives.
    ruscv_target target_table();

    /// Serves one blocking transaction from the C ABI (one `b_transport` call).
    void serve_b_transport(ruscv_payload& payload);

    /// Serves one debug transport request; returns the bytes transferred.
    size_t serve_transport_dbg(ruscv_debug& debug);

    /// Serves the target's transport reset; returns nonzero when acknowledged.
    int32_t serve_transport_reset();

private:
    /// The backward path is unused: this composition issues only blocking
    /// transactions and never requests DMI, so neither callback can affect a
    /// ruscv-sim result.
    tlm::tlm_sync_enum nb_transport_bw(tlm::tlm_generic_payload& transaction,
                                       tlm::tlm_phase& phase, sc_core::sc_time& delay);
    void invalidate_direct_mem_ptr(sc_dt::uint64 start_range, sc_dt::uint64 end_range);

    RamTarget& ram_;
};

/// The C ABI callbacks, one per `ruscv_target` field.
///
/// They cast `user` to the `RuscvInitiator` named in the table and forward.
extern "C" void ruscv_systemc_b_transport(void* user, ruscv_payload* payload);
extern "C" size_t ruscv_systemc_transport_dbg(void* user, ruscv_debug* debug);
extern "C" int32_t ruscv_systemc_transport_reset(void* user);

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H
