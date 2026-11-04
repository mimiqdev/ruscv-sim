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

#ifndef RUSCV_SIM_SYSTEMC_RAM_TARGET_H
#define RUSCV_SIM_SYSTEMC_RAM_TARGET_H

#include <array>
#include <cstddef>
#include <cstdint>
#include <map>
#include <vector>

#include <systemc>
#include <tlm>
#include <tlm_utils/simple_target_socket.h>

#include "ruscv_payload_extension.h"

namespace ruscv_sim {

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

    explicit RamTarget(sc_core::sc_module_name name);

    /// Serves one blocking transaction and annotates zero delay.
    void b_transport(tlm::tlm_generic_payload& transaction, sc_core::sc_time& delay);

    /// Serves one debug access: no delay, no side effects beyond the bytes, and
    /// no transaction accounting. Returns the bytes transferred.
    unsigned int transport_dbg(tlm::tlm_generic_payload& transaction);

    /// Transport reset: discards every byte and every committed-write version.
    /// Returns true when the target has quiesced and accepts new accesses.
    bool transport_reset();

    /// Number of blocking transactions served since construction.
    uint64_t b_transport_count() const { return b_transport_count_; }
    /// Number of debug accesses served since construction.
    uint64_t transport_dbg_count() const { return transport_dbg_count_; }
    /// Number of transport resets acknowledged since construction.
    uint64_t transport_reset_count() const { return transport_reset_count_; }

private:
    /// Bytes and per-byte committed-write versions for one 4 KiB chunk.
    struct Chunk {
        std::array<uint8_t, kChunkBytes> bytes{};
        std::array<uint64_t, kChunkBytes> versions{};
    };

    std::map<uint64_t, Chunk> chunks_;
    /// Monotonic committed-write clock; zero marks a byte no write has covered.
    uint64_t next_version_ = 0;
    uint64_t b_transport_count_ = 0;
    uint64_t transport_dbg_count_ = 0;
    uint64_t transport_reset_count_ = 0;

    /// Applies one atomic transaction, or sets a failure status.
    void apply_atomic(tlm::tlm_generic_payload& transaction, PayloadExtension& extension);

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
