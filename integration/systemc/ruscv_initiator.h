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
// The callbacks run on the SystemC kernel thread inside one ruscv-sim call. A
// `b_transport` callback whose payload carries `may_suspend` may serve its
// access through the non-blocking base protocol and let host time pass (nb
// D7); every other callback must not wait, advance time or call back into the C
// ABI beyond the direct-memory invalidation entry.

#ifndef RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H
#define RUSCV_SIM_SYSTEMC_RUSCV_INITIATOR_H

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>
#include <vector>

#include <ruscv_sim.h>
#include <systemc>
#include <tlm>
#include <tlm_utils/simple_initiator_socket.h>

#include "ram_target.h"

namespace ruscv_sim {

/// The smoke's fault-option controls that belong to the initiator (nb §4).
///
/// Each one breaks the initiator in one named place so the smoke can require
/// its own failure. They are taken one at a time and are never a normal bridge.
struct InitiatorFaults {
    /// `--fault-keep-offset` (quantum §4): report the full `b_transport` delay
    /// argument, offset included, as an added delay.
    bool keep_offset = false;
    /// `--fault-uncounted-elapsed`: report `e` = 0 while reporting `d` as the
    /// host time that passed.
    bool uncounted_elapsed = false;
    /// `--fault-short-delay`: report `d` one femtosecond below `e`.
    bool short_delay = false;
};

/// One initiator-owned AT transaction, with its own payload, extension, data
/// and memory manager (nb D7).
///
/// The target never sees the ABI's buffer: request bytes are copied in before
/// `BEGIN_REQ` and response bytes are copied out at completion. A retired
/// transaction keeps this object alive until the target has completed it or
/// released every reference it acquired (nb D9).
class AtTransaction {
public:
    class MemoryManager : public tlm::tlm_mm_interface {
    public:
        void free(tlm::tlm_generic_payload* transaction) override { transaction->reset(); }
    };

    AtTransaction(uint64_t address, int32_t command, uint8_t* data, size_t length)
        : data_(length) {
        payload_.set_mm(&memory_manager_);
        payload_.acquire();
        payload_.set_extension(&extension_);
        if (length != 0 && data != nullptr) {
            std::memcpy(data_.data(), data, length);
        }
        payload_.set_command(command == RUSCV_COMMAND_READ ? tlm::TLM_READ_COMMAND
                                                           : tlm::TLM_WRITE_COMMAND);
        payload_.set_address(address);
        payload_.set_data_ptr(length == 0 ? nullptr : data_.data());
        payload_.set_data_length(length);
        payload_.set_streaming_width(length);
        payload_.set_byte_enable_ptr(nullptr);
        payload_.set_byte_enable_length(0);
        payload_.set_response_status(tlm::TLM_INCOMPLETE_RESPONSE);
    }

    ~AtTransaction() {
        if (payload_.get_ref_count() != 0) {
            payload_.release();
        }
    }

    AtTransaction(const AtTransaction&) = delete;
    AtTransaction& operator=(const AtTransaction&) = delete;

    tlm::tlm_generic_payload& payload() { return payload_; }
    PayloadExtension& extension() { return extension_; }
    const std::vector<uint8_t>& data() const { return data_; }

private:
    MemoryManager memory_manager_;
    // The extension must outlive the payload: the payload destructor frees its
    // remaining extensions, which would otherwise call through a destroyed
    // object. Members are destroyed in reverse declaration order.
    PayloadExtension extension_;
    tlm::tlm_generic_payload payload_;
    std::vector<uint8_t> data_;
};

/// Bridges the ruscv-sim target callbacks onto one TLM initiator socket,
/// including the non-blocking AT path for a suspending transaction (nb D7).
class RuscvInitiator : public sc_core::sc_module {
public:
    /// The initiator port the target callbacks drive.
    tlm_utils::simple_initiator_socket<RuscvInitiator> socket;

    /// Binds `socket` to `ram.socket` and keeps `ram` for its transport reset
    /// and the target-originated invalidation path. `direct_mem` selects
    /// direct memory for the table (`--dmi`), and `quantum_ticks` selects the
    /// quantum in Platform ticks (`--quantum`), zero for off (quantum D13).
    /// `faults` carries the smoke's fault-option controls and is empty in every
    /// normal run. `non_blocking` selects the target table's suspension flag and
    /// the AT path (`--nb`), and `nb_latency_fs` is its declared maximum `lambda`.
    RuscvInitiator(sc_core::sc_module_name name, RamTarget& ram, bool direct_mem,
                   uint64_t quantum_ticks, InitiatorFaults faults, bool non_blocking,
                   uint64_t nb_latency_fs);

    /// The C ABI target table naming this object as `user`.
    ///
    /// The table declares `lambda` as its per-transaction maximum under
    /// `--nb`, and declares that the target suspends host time (nb D6/D8). It
    /// carries the selected quantum (`--quantum`, zero for off) in
    /// `quantum_ticks` (quantum D12/D13). With `direct_mem` off the table's
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

    /// Reports the host time measured around one blocking callback: `e` in
    /// `host_elapsed_fs`, and the delay the smoke's `--fault-short-delay`
    /// control lowers (nb §4).
    void report_host_time(ruscv_payload& payload, uint64_t elapsed_fs) const;

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

    /// Number of AT transactions retired after the timeout (nb D9).
    uint64_t retirement_count() const { return retirements_; }
    /// Number of late phases ignored and counted since construction (nb D9).
    uint64_t late_phase_count() const { return late_phases_; }
    /// Number of forward `END_RESP` phases the drain sent for a retained
    /// transaction (nb D9).
    uint64_t drain_end_resp_count() const { return drain_end_resp_count_; }
    /// True while a retired transaction is still retained, or while the target
    /// may still hold a reference to it (nb D9).
    bool drain_pending() const { return at_transaction_ != nullptr; }
    /// True when the target still holds the retired transaction, so a transport
    /// reset must withhold its acknowledgement (nb D8/D9).
    bool target_holds_transaction() const {
        return at_transaction_ != nullptr && at_target_holds_;
    }

    /// Runs one drain step at the current host time: sends a due forward
    /// `END_RESP`, then releases the retained storage once the target has let
    /// go of it (nb D9).
    void drain_step();

    /// Sets `due` to the host time the drain's next `END_RESP` is due, and
    /// returns true when one is owed.
    bool drain_end_resp_due(sc_core::sc_time& due) const;

private:
    /// One phase the target sent on the backward path, or one the forward path
    /// returned as `TLM_UPDATED`, with the host time it takes effect (nb D7).
    struct PendingPhase {
        tlm::tlm_phase phase;
        sc_core::sc_time effective;
        /// True for a forward `TLM_UPDATED` phase, whose `BEGIN_RESP` needs a
        /// forward `END_RESP`; false for a backward phase.
        bool forward = false;
    };

    /// The one AT transaction's lifecycle (nb D7/D9).
    enum class AtState {
        /// No transaction is in flight.
        Idle,
        /// The callback is waiting for the transaction's final phase.
        Active,
        /// The callback retired the transaction after the timeout. Backward
        /// phases are then late: ignored for the Machine and counted once.
        Retired,
    };

    /// Serves one suspending transaction through the non-blocking base
    /// protocol (nb D7).
    void serve_at_b_transport(ruscv_payload& payload);

    /// Records one phase with its effective host time and wakes the waiting
    /// process. Returns false when the phase takes effect after the timeout, so
    /// it is a late phase the caller counts and ignores (nb D9). Called from the
    /// target's process for a backward phase.
    bool record_phase(const tlm::tlm_phase& phase, const sc_core::sc_time& delay,
                      bool forward);

    /// Waits for the recorded phases until the transaction completes or the
    /// timeout retires it (nb D9).
    bool wait_for_at_completion();

    /// Retires the active transaction as an unknown completion and keeps its
    /// storage while the target still holds it (nb D9).
    void retire(ruscv_payload& payload);

    /// The base-protocol backward callback. It runs in the target's process and
    /// never waits (nb D7).
    tlm::tlm_sync_enum nb_transport_bw(tlm::tlm_generic_payload&,
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
    const InitiatorFaults faults_;
    const bool non_blocking_;
    const uint64_t nb_latency_fs_;
    ruscv_machine* machine_ = nullptr;
    std::vector<PendingPhase> pending_phases_;
    sc_core::sc_event phase_event_;
    AtState at_state_ = AtState::Idle;
    /// The AT transaction in flight, or the retired one the target may still
    /// hold a reference to (nb D9).
    std::unique_ptr<AtTransaction> at_transaction_;
    /// `BEGIN_REQ` plus the declared maximum plus one femtosecond: the timeout
    /// after which an incomplete transaction is retired (nb D9).
    uint64_t at_deadline_fs_ = 0;
    /// True while the target still holds the transaction and may send a phase
    /// for it or receive the final one.
    bool at_target_holds_ = false;
    /// True while the drain still owes the retained transaction's forward
    /// `END_RESP` (the `early` path), and that phase's effective host time.
    bool drain_end_resp_pending_ = false;
    sc_core::sc_time drain_end_resp_due_ = sc_core::SC_ZERO_TIME;
    bool protocol_error_ = false;
    uint64_t retirements_ = 0;
    uint64_t late_phases_ = 0;
    uint64_t drain_end_resp_count_ = 0;
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
