// ruscv_payload_extension.h - the ruscv-sim payload extension on one TLM payload.
//
// dev-plan D3/D5: one `ruscv_payload` becomes one `b_transport` transaction
// whose standard generic-payload fields carry the physical address, the
// command, the data length, the streaming width, the raw little-endian bytes
// and the blocking response status. Everything else ruscv-sim needs to know,
// or to learn, travels in this extension: the existing physical access
// category, the initiator, and the atomic extension with its request and
// response halves.
//
// The target applies an atomic operation indivisibly inside that one call and
// records its completion here. The extension never carries a Rust function
// pointer, and no field in it can become a ruscv-sim terminal result.

#ifndef RUSCV_SIM_SYSTEMC_PAYLOAD_EXTENSION_H
#define RUSCV_SIM_SYSTEMC_PAYLOAD_EXTENSION_H

#include <array>
#include <cstdint>

#include <ruscv_sim.h>
#include <tlm>

namespace ruscv_sim {

/// One transaction's ruscv-sim extension (a TLM-2.0 payload extension).
class PayloadExtension : public tlm::tlm_extension<PayloadExtension> {
public:
    /// Request: the existing physical access category (`RUSCV_ACCESS_*`).
    int32_t access_kind = RUSCV_ACCESS_DATA_READ;
    /// Request: who issued the transaction (`RUSCV_INITIATOR_*`).
    int32_t initiator = RUSCV_INITIATOR_HART;

    /// Request: nonzero when this transaction carries the atomic extension.
    int32_t atomic_present = 0;
    /// Request: `RUSCV_ATOMIC_*`.
    int32_t atomic_kind = RUSCV_ATOMIC_RMW;
    /// Request: `RUSCV_AMO_*` for RMW, -1 otherwise.
    int32_t atomic_operation = -1;
    /// Request: transfer width in bytes, 4 or 8.
    int32_t atomic_width = 0;
    /// Request: the aq/rl ordering bits.
    int32_t acquire = 0;
    int32_t release = 0;

    /// Request: nonzero when a store-conditional carries its reservation.
    int32_t reserved = 0;
    uint64_t reserved_address = 0;
    uint32_t reserved_length = 0;
    uint32_t reserved_snapshot_length = 0;
    std::array<uint8_t, RUSCV_SNAPSHOT_MAX> reserved_snapshot{};

    /// Response: `RUSCV_ATOMIC_OUTCOME_*`.
    int32_t atomic_outcome = RUSCV_ATOMIC_OUTCOME_NONE;
    /// Response: load-reserved committed-write snapshot length in bytes.
    uint32_t snapshot_length = 0;
    /// Response: load-reserved committed-write snapshot bytes.
    std::array<uint8_t, RUSCV_SNAPSHOT_MAX> snapshot{};
    /// Response: nonzero when the target cannot vouch for the outcome.
    uint32_t outcome_unknown = 0;

    tlm::tlm_extension_base* clone() const override {
        return new PayloadExtension(*this);
    }

    void copy_from(tlm::tlm_extension_base const& extension) override {
        *this = static_cast<PayloadExtension const&>(extension);
    }

    /// The initiator owns one extension instance for the duration of one call,
    /// so the payload's destructor must not delete it.
    void free() override {}
};

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_PAYLOAD_EXTENSION_H
