// ram_target.h - a zero-delay blocking RAM target for the ruscv-sim smoke.
//
// dev-plan D5/D6/D9. The module is the guest-RAM window behind one
// `simple_target_socket`: `b_transport` serves every read, write and atomic,
// `transport_dbg` serves image installation and the host views, and
// `transport_reset` is the acknowledgement the Platform needs before it
// zero-fills and reinstalls.
//
// Storage is sparse and byte-exact. Alongside the bytes it keeps one
// committed-write version per byte: a load-reserved returns that snapshot and a
// store-conditional re-checks it inside the same hold, which is the target half
// of the reservation protocol. Atomics arrive as one transaction carrying the
// ruscv-sim atomic extension and are applied indivisibly through this module's
// operation table; the target never performs a read followed by a write.
//
// Temporal decoupling (quantum D13): `b_transport`'s delay argument arrives as
// the Hart's local offset in exact femtoseconds and this zero-delay model adds
// nothing to it, so the argument is left unchanged. The initiator subtracts the
// offset before it reports the added delay, which keeps the run's committed
// time identical with the quantum on and off.
//
// Direct memory (direct-memory D13): a completed non-atomic transaction sets
// the `dmi_allowed` hint, and `get_direct_mem_ptr` grants the whole chunk that
// contains the queried address as read-write with zero latency. Chunk bytes are
// `std::atomic<uint8_t>`, because the C ABI's granted pointer is served by
// relaxed byte-wise atomic copies whose accesses must meet every target access
// without a data race (direct-memory D10). Before its transport reset discards
// any chunk the target invalidates every registration on the bound initiator,
// and the version clock survives the reset so no version repeats.

#ifndef RUSCV_SIM_SYSTEMC_RAM_TARGET_H
#define RUSCV_SIM_SYSTEMC_RAM_TARGET_H

#include <array>
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <map>
#include <vector>

#include <systemc>
#include <tlm>
#include <tlm_utils/peq_with_cb_and_phase.h>
#include <tlm_utils/simple_target_socket.h>

#include "ruscv_payload_extension.h"

namespace ruscv_sim {

/// The RAM target's non-blocking return path (nb D8).
enum class AtPath {
    /// `TLM_ACCEPTED`, then `END_REQ` in a later delta and `BEGIN_RESP` after
    /// `lambda`.
    Accepted,
    /// `TLM_UPDATED` with `END_REQ`, then `BEGIN_RESP` after `lambda`.
    Updated,
    /// `TLM_UPDATED` with `BEGIN_RESP` and an annotated `lambda`.
    Early,
    /// `TLM_ACCEPTED`, then `END_REQ` in a later delta and `BEGIN_RESP` at once
    /// with an annotated `lambda`.
    Annotated,
    /// `TLM_COMPLETED` with an annotated `lambda`.
    Completed,
};

/// A zero-delay blocking RAM target with the ruscv-sim atomic extension.
class RamTarget : public sc_core::sc_module {
public:
    /// Storage index granularity: one chunk covers 4 KiB of guest RAM.
    static constexpr uint64_t kChunkBytes = 4096;
    static constexpr uint64_t kChunkBits = 12;
    static constexpr uint64_t kChunkMask = kChunkBytes - 1;

    /// The guest-RAM target port. The initiator drives it through `b_transport`
    /// and `transport_dbg`; the C ABI callbacks in `ruscv_initiator` are the
    /// ruscv-sim side of the same calls.
    tlm_utils::simple_target_socket<RamTarget> socket;

    /// `fault_decrease_delay` is the smoke's `--fault-decrease-delay` control
    /// (quantum §4): the target lowers the received delay below the offset it
    /// was given, which the initiator must report as an unknown completion. It
    /// exists to prove that assertion and is never a normal model.
    ///
    /// `non_blocking` registers the AT `nb_transport_fw` path (nb D8), with
    /// `nb_latency_fs` as `lambda` and `nb_path` as its return path. The
    /// blocking, debug and direct-memory callbacks stay unchanged.
    explicit RamTarget(sc_core::sc_module_name name, bool fault_decrease_delay = false,
                       bool non_blocking = false, uint64_t nb_latency_fs = 0,
                       AtPath nb_path = AtPath::Accepted);

    /// Serves one blocking transaction. This model declares zero delay, so it
    /// adds nothing to `delay` and leaves the argument unchanged (quantum D13).
    void b_transport(tlm::tlm_generic_payload& transaction, sc_core::sc_time& delay);

    /// Serves one debug access: no delay, no side effects beyond the bytes, and
    /// no transaction accounting. Returns the bytes transferred.
    unsigned int transport_dbg(tlm::tlm_generic_payload& transaction);

    /// Transport reset: invalidates every direct-memory registration, then
    /// discards every byte and every committed-write version record. The
    /// version clock keeps running, so no committed-write version repeats
    /// across the reset (direct-memory D10/D13). Returns true when the target
    /// has quiesced and accepts new accesses.
    bool transport_reset();

    /// Invalidates every direct-memory grant on the bound initiator with the
    /// whole-space backward call (direct-memory D8/D13).
    void invalidate_all_direct_mem();

    /// Number of blocking transactions served since construction.
    uint64_t b_transport_count() const { return b_transport_count_; }
    /// Number of debug accesses served since construction.
    uint64_t transport_dbg_count() const { return transport_dbg_count_; }
    /// Number of transport resets acknowledged since construction.
    uint64_t transport_reset_count() const { return transport_reset_count_; }
    /// Number of non-blocking `BEGIN_REQ` phases received (nb D8).
    uint64_t begin_req_count() const { return begin_req_count_; }
    /// Number of accesses applied by the AT path (nb D8).
    uint64_t applied_access_count() const { return applied_access_count_; }

private:
    /// Bytes and per-byte committed-write versions for one 4 KiB chunk.
    struct Chunk {
        /// A fresh chunk reads as zero. `std::atomic<uint8_t>` has no zeroing
        /// value constructor in C++17, so the bytes are filled explicitly.
        Chunk() {
            for (auto& byte : bytes) {
                byte.store(0, std::memory_order_relaxed);
            }
        }

        /// Storage the direct-memory pointer aliases. Every access, including
        /// `b_transport` and `transport_dbg`, goes through relaxed atomic
        /// byte operations (direct-memory D10).
        std::array<std::atomic<uint8_t>, kChunkBytes> bytes;
        std::array<uint64_t, kChunkBytes> versions{};
    };

    /// The smoke's `--fault-decrease-delay` control (quantum §4).
    const bool fault_decrease_delay_;
    const bool non_blocking_;
    const uint64_t nb_latency_fs_;
    const AtPath nb_path_;
    /// The target's own phase queue (nb D8): it carries the phase and gives
    /// zero-time notifications their delta separation.
    tlm_utils::peq_with_cb_and_phase<RamTarget> phase_queue_;
    /// The one AT transaction from `BEGIN_REQ` to its final phase.
    tlm::tlm_generic_payload* open_transaction_ = nullptr;
    uint64_t begin_req_count_ = 0;
    uint64_t applied_access_count_ = 0;

    std::map<uint64_t, Chunk> chunks_;
    /// Monotonic committed-write clock; zero marks a byte no write has covered.
    uint64_t next_version_ = 0;
    uint64_t b_transport_count_ = 0;
    uint64_t transport_dbg_count_ = 0;
    uint64_t transport_reset_count_ = 0;

    /// Serves one non-blocking forward phase (nb D8).
    tlm::tlm_sync_enum nb_transport_fw(tlm::tlm_generic_payload& transaction,
                                       tlm::tlm_phase& phase, sc_core::sc_time& delay);

    /// Sends one phase the target scheduled through `phase_queue_`.
    void handle_phase(tlm::tlm_generic_payload& transaction, const tlm::tlm_phase& phase);

    /// Applies one request exactly as `b_transport` does; returns true when
    /// the access was applied.
    bool apply_request(tlm::tlm_generic_payload& transaction, PayloadExtension* extension);

    /// Applies one atomic transaction, or sets a failure status.
    bool apply_atomic(tlm::tlm_generic_payload& transaction, PayloadExtension& extension);

    /// Answers one direct-memory query: the whole chunk that contains the
    /// queried address, read-write, zero latency (direct-memory D13).
    bool get_direct_mem_ptr(tlm::tlm_generic_payload& transaction,
                            tlm::tlm_dmi& data);

    /// Reads `length` bytes; uncovered bytes read as zero.
    std::vector<uint8_t> read(uint64_t address, size_t length) const;

    /// Writes `length` bytes and records one committed write per covered byte.
    void write(uint64_t address, const uint8_t* bytes, size_t length);

    /// Copies bytes without recording a committed write.
    void store(uint64_t address, const uint8_t* bytes, size_t length);

    /// The committed-write version of one byte; zero when never written.
    uint64_t version_at(uint64_t address) const;

    /// Files one transaction's bytes as a committed write.
    void commit_versions(uint64_t address, size_t length);
};

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_RAM_TARGET_H
