// ram_target.cpp - the zero-delay blocking RAM target behind one TLM socket.

#include "ram_target.h"

#include <algorithm>
#include <cstring>

namespace ruscv_sim {
namespace {

// The granted pointer addresses byte-atomic storage: the ABI's byte-wise
// relaxed atomic copies, `b_transport` and `transport_dbg` must all meet the
// same bytes without a data race (direct-memory D10), which requires the
// atomic storage to be byte-sized and lock-free.
static_assert(sizeof(std::atomic<uint8_t>) == sizeof(uint8_t) &&
                  alignof(std::atomic<uint8_t>) == alignof(uint8_t),
              "std::atomic<uint8_t> must be byte-sized for a direct-memory pointer");
static_assert(std::atomic<uint8_t>::is_always_lock_free,
              "std::atomic<uint8_t> must be lock-free for a direct-memory pointer");

/// Reads one little-endian word of `width` bytes (4 or 8).
uint64_t load_le(const uint8_t* bytes, size_t width) {
    uint64_t value = 0;
    for (size_t index = 0; index < width; ++index) {
        value |= static_cast<uint64_t>(bytes[index]) << (8 * index);
    }
    return value;
}

/// Writes one little-endian word of `width` bytes (4 or 8).
void store_le(uint8_t* bytes, size_t width, uint64_t value) {
    for (size_t index = 0; index < width; ++index) {
        bytes[index] = static_cast<uint8_t>((value >> (8 * index)) & 0xff);
    }
}

/// Applies one encoded AMO operation to the old and operand words.
///
/// This is the target's half of dev-plan D5: the operation table must match the
/// Hart-owned definition bit for bit, so W results stay zero-extended, Min/Max
/// compare as signed and Minu/Maxu as unsigned.
uint64_t amo_result(int32_t operation, size_t width, uint64_t old_value, uint64_t operand) {
    if (width == 4) {
        const uint32_t old_word = static_cast<uint32_t>(old_value);
        const uint32_t operand_word = static_cast<uint32_t>(operand);
        uint32_t result = 0;
        switch (operation) {
            case RUSCV_AMO_SWAP: result = operand_word; break;
            case RUSCV_AMO_ADD: result = old_word + operand_word; break;
            case RUSCV_AMO_XOR: result = old_word ^ operand_word; break;
            case RUSCV_AMO_OR: result = old_word | operand_word; break;
            case RUSCV_AMO_AND: result = old_word & operand_word; break;
            case RUSCV_AMO_MIN:
                result = static_cast<uint32_t>(
                    std::min(static_cast<int32_t>(old_word), static_cast<int32_t>(operand_word)));
                break;
            case RUSCV_AMO_MAX:
                result = static_cast<uint32_t>(
                    std::max(static_cast<int32_t>(old_word), static_cast<int32_t>(operand_word)));
                break;
            case RUSCV_AMO_MINU: result = std::min(old_word, operand_word); break;
            case RUSCV_AMO_MAXU: result = std::max(old_word, operand_word); break;
            default: return 0;
        }
        // W writes a zero-extended 32-bit result: the low four bytes only.
        return static_cast<uint64_t>(result);
    }
    switch (operation) {
        case RUSCV_AMO_SWAP: return operand;
        case RUSCV_AMO_ADD: return old_value + operand;
        case RUSCV_AMO_XOR: return old_value ^ operand;
        case RUSCV_AMO_OR: return old_value | operand;
        case RUSCV_AMO_AND: return old_value & operand;
        case RUSCV_AMO_MIN:
            return static_cast<uint64_t>(
                std::min(static_cast<int64_t>(old_value), static_cast<int64_t>(operand)));
        case RUSCV_AMO_MAX:
            return static_cast<uint64_t>(
                std::max(static_cast<int64_t>(old_value), static_cast<int64_t>(operand)));
        case RUSCV_AMO_MINU: return std::min(old_value, operand);
        case RUSCV_AMO_MAXU: return std::max(old_value, operand);
        default: return 0;
    }
}

}  // namespace

RamTarget::RamTarget(sc_core::sc_module_name name)
    : sc_core::sc_module(name), socket("socket") {
    socket.register_b_transport(this, &RamTarget::b_transport);
    socket.register_transport_dbg(this, &RamTarget::transport_dbg);
    // direct-memory D13: this target grants direct memory, so it registers the
    // forward query. A host that leaves the C ABI query NULL never reaches it.
    socket.register_get_direct_mem_ptr(this, &RamTarget::get_direct_mem_ptr);
}

void RamTarget::b_transport(tlm::tlm_generic_payload& transaction, sc_core::sc_time& delay) {
    // dev-plan D7: this RAM model declares zero delay, so the response never
    // charges modeled time and a TLM run stays time-equivalent to native.
    delay = sc_core::SC_ZERO_TIME;
    ++b_transport_count_;

    PayloadExtension* extension = transaction.get_extension<PayloadExtension>();
    if (extension == nullptr) {
        // Without the extension the target cannot see the atomic request, so it
        // refuses rather than guessing (dev-plan D5).
        transaction.set_response_status(tlm::TLM_COMMAND_ERROR_RESPONSE);
        return;
    }
    if (transaction.get_byte_enable_ptr() != nullptr) {
        transaction.set_response_status(tlm::TLM_BYTE_ENABLE_ERROR_RESPONSE);
        return;
    }
    if (transaction.get_streaming_width() != transaction.get_data_length()) {
        // One access is one non-burst transaction: streaming width equals the
        // data length (dev-plan D3).
        transaction.set_response_status(tlm::TLM_BURST_ERROR_RESPONSE);
        return;
    }
    if (transaction.get_data_ptr() == nullptr && transaction.get_data_length() != 0) {
        transaction.set_response_status(tlm::TLM_COMMAND_ERROR_RESPONSE);
        return;
    }
    const uint64_t address = transaction.get_address();
    const size_t length = transaction.get_data_length();
    if (address > UINT64_MAX - length) {
        transaction.set_response_status(tlm::TLM_ADDRESS_ERROR_RESPONSE);
        return;
    }
    if (extension->atomic_present != 0) {
        apply_atomic(transaction, *extension);
        return;
    }
    switch (transaction.get_command()) {
        case tlm::TLM_READ_COMMAND: {
            const std::vector<uint8_t> bytes = read(address, length);
            if (!bytes.empty()) {
                std::memcpy(transaction.get_data_ptr(), bytes.data(), length);
            }
            break;
        }
        case tlm::TLM_WRITE_COMMAND:
            // One indivisible semantic write, applied before returning.
            write(address, transaction.get_data_ptr(), length);
            break;
        default:
            transaction.set_response_status(tlm::TLM_COMMAND_ERROR_RESPONSE);
            return;
    }
    // The hint is the only licence for ruscv-sim's one direct-memory query of
    // this access, so it goes on a completed non-atomic transaction only
    // (direct-memory D3/D13). It is ignored when the host offers no query.
    transaction.set_dmi_allowed(true);
    transaction.set_response_status(tlm::TLM_OK_RESPONSE);
}

bool RamTarget::get_direct_mem_ptr(tlm::tlm_generic_payload& transaction,
                                   tlm::tlm_dmi& data) {
    const auto chunk = chunks_.find(transaction.get_address() >> kChunkBits);
    if (chunk == chunks_.end()) {
        // Granting the query would allocate storage; this target grants only
        // the chunk that already exists, exactly like the in-tree
        // `ZeroDelayRam` (direct-memory D13). A denial caches nothing.
        return false;
    }
    const uint64_t start = (transaction.get_address() >> kChunkBits) << kChunkBits;
    // SAFETY: the chunk is this target's only copy of those bytes,
    // `b_transport` and `transport_dbg` serve the same atomics, and it is not
    // freed before `transport_reset`, which invalidates every registration on
    // the bound initiator first (direct-memory D10). A direct store bypasses
    // the committed-write records, which is exactly what the adapter's
    // write-suspension gate accounts for.
    data.set_dmi_ptr(reinterpret_cast<unsigned char*>(chunk->second.bytes.data()));
    data.set_start_address(start);
    data.set_end_address(start + kChunkMask);
    data.set_read_latency(sc_core::SC_ZERO_TIME);
    data.set_write_latency(sc_core::SC_ZERO_TIME);
    data.allow_read_write();
    return true;
}

void RamTarget::invalidate_all_direct_mem() {
    // The whole physical space is the backward call's full-invalidation range,
    // so the initiator drops every grant this target could have issued
    // (direct-memory D8).
    socket->invalidate_direct_mem_ptr(0, static_cast<sc_dt::uint64>(-1));
}

void RamTarget::apply_atomic(tlm::tlm_generic_payload& transaction,
                             PayloadExtension& extension) {
    const int32_t width = extension.atomic_width;
    const size_t length = transaction.get_data_length();
    uint8_t* data = transaction.get_data_ptr();
    if ((width != 4 && width != 8) || length != static_cast<size_t>(width) || data == nullptr) {
        // A malformed atomic request is a host failure, never a guest one.
        transaction.set_response_status(tlm::TLM_GENERIC_ERROR_RESPONSE);
        return;
    }
    const uint64_t address = transaction.get_address();
    std::array<uint8_t, 8> old_bytes{};
    std::array<uint8_t, 8> operand_bytes{};
    std::array<uint8_t, 8> updated_bytes{};
    std::memcpy(operand_bytes.data(), data, length);

    switch (extension.atomic_kind) {
        case RUSCV_ATOMIC_RMW: {
            if (extension.atomic_operation < RUSCV_AMO_ADD ||
                extension.atomic_operation > RUSCV_AMO_MAXU) {
                transaction.set_response_status(tlm::TLM_GENERIC_ERROR_RESPONSE);
                return;
            }
            const std::vector<uint8_t> old = read(address, length);
            std::memcpy(old_bytes.data(), old.data(), length);
            const uint64_t updated =
                amo_result(extension.atomic_operation, length, load_le(old_bytes.data(), length),
                           load_le(operand_bytes.data(), length));
            store_le(updated_bytes.data(), length, updated);
            // Read-modify-write inside this one transaction: the new value is
            // stored, the old value is returned, and no second access is issued.
            write(address, updated_bytes.data(), length);
            std::memcpy(data, old_bytes.data(), length);
            extension.atomic_outcome = RUSCV_ATOMIC_OUTCOME_RMW;
            break;
        }
        case RUSCV_ATOMIC_LOAD_RESERVED: {
            const std::vector<uint8_t> old = read(address, length);
            std::memcpy(data, old.data(), length);
            // The snapshot is this target's committed-write record for the
            // reserved span: one little-endian 64-bit version per byte.
            for (size_t index = 0; index < length; ++index) {
                store_le(extension.snapshot.data() + index * 8, 8,
                         version_at(address + index));
            }
            extension.snapshot_length = static_cast<uint32_t>(length * 8);
            extension.atomic_outcome = RUSCV_ATOMIC_OUTCOME_LOAD_RESERVED;
            break;
        }
        case RUSCV_ATOMIC_STORE_CONDITIONAL: {
            bool committed = false;
            const bool covered = extension.reserved != 0 &&
                                 extension.reserved_address <= address &&
                                 address <= UINT64_MAX - length &&
                                 static_cast<uint64_t>(extension.reserved_length) <=
                                     UINT64_MAX - extension.reserved_address &&
                                 address + length <= extension.reserved_address +
                                                          extension.reserved_length;
            if (covered) {
                // The snapshot is this target's own format: one little-endian
                // 64-bit version per reserved byte. A length that disagrees with
                // the reserved span, or that the ABI buffer cannot hold, is a
                // malformed completion and never an ordinary conditional failure.
                // The product is formed in 64 bits so no wrapping length passes.
                const uint64_t snapshot_bytes =
                    static_cast<uint64_t>(extension.reserved_length) * 8;
                if (extension.reserved_snapshot_length == 0 ||
                    snapshot_bytes != extension.reserved_snapshot_length ||
                    extension.reserved_snapshot_length > extension.reserved_snapshot.size()) {
                    // The snapshot does not have this target's format: a host
                    // failure, never an ordinary conditional failure and never a
                    // guest-visible fault.
                    transaction.set_response_status(tlm::TLM_GENERIC_ERROR_RESPONSE);
                    return;
                }
                bool overwritten = false;
                for (uint32_t index = 0; index < extension.reserved_length; ++index) {
                    const uint64_t recorded =
                        load_le(extension.reserved_snapshot.data() + index * 8, 8);
                    if (recorded != version_at(extension.reserved_address + index)) {
                        overwritten = true;
                        break;
                    }
                }
                if (!overwritten) {
                    write(address, operand_bytes.data(), length);
                    committed = true;
                }
            }
            extension.atomic_outcome =
                committed ? RUSCV_ATOMIC_OUTCOME_STORE_CONDITIONAL_SUCCESS
                          : RUSCV_ATOMIC_OUTCOME_STORE_CONDITIONAL_FAILURE;
            break;
        }
        default:
            transaction.set_response_status(tlm::TLM_GENERIC_ERROR_RESPONSE);
            return;
    }
    transaction.set_response_status(tlm::TLM_OK_RESPONSE);
}

unsigned int RamTarget::transport_dbg(tlm::tlm_generic_payload& transaction) {
    ++transport_dbg_count_;
    const uint64_t address = transaction.get_address();
    const size_t length = transaction.get_data_length();
    if (transaction.get_data_ptr() == nullptr && length != 0) {
        transaction.set_response_status(tlm::TLM_COMMAND_ERROR_RESPONSE);
        return 0;
    }
    if (address > UINT64_MAX - length) {
        transaction.set_response_status(tlm::TLM_ADDRESS_ERROR_RESPONSE);
        return 0;
    }
    switch (transaction.get_command()) {
        case tlm::TLM_READ_COMMAND: {
            const std::vector<uint8_t> bytes = read(address, length);
            if (!bytes.empty()) {
                std::memcpy(transaction.get_data_ptr(), bytes.data(), length);
            }
            break;
        }
        case tlm::TLM_WRITE_COMMAND:
            write(address, transaction.get_data_ptr(), length);
            break;
        default:
            transaction.set_response_status(tlm::TLM_COMMAND_ERROR_RESPONSE);
            return 0;
    }
    transaction.set_response_status(tlm::TLM_OK_RESPONSE);
    return static_cast<unsigned int>(length);
}

bool RamTarget::transport_reset() {
    ++transport_reset_count_;
    // Every grant is invalidated before any chunk is discarded, and this
    // module runs on the single SystemC kernel thread, so no grant is issued
    // across the window and no cached pointer outlives its storage
    // (direct-memory D9/D13).
    invalidate_all_direct_mem();
    chunks_.clear();
    // The version records go with the bytes, but the clock must not restart:
    // a repeated version could let a reservation retained across this reset
    // pass a store-conditional that fails without direct memory
    // (direct-memory D10).
    return true;
}

std::vector<uint8_t> RamTarget::read(uint64_t address, size_t length) const {
    std::vector<uint8_t> bytes(length, 0);
    size_t done = 0;
    while (done < length) {
        const uint64_t current = address + done;
        const auto chunk = chunks_.find(current >> kChunkBits);
        const size_t offset = static_cast<size_t>(current & kChunkMask);
        const size_t count = std::min(static_cast<size_t>(kChunkBytes) - offset, length - done);
        if (chunk != chunks_.end()) {
            // Byte-wise relaxed atomic loads: these are the same bytes a
            // granted pointer serves (direct-memory D10).
            for (size_t index = 0; index < count; ++index) {
                bytes[done + index] =
                    chunk->second.bytes[offset + index].load(std::memory_order_relaxed);
            }
        }
        done += count;
    }
    return bytes;
}

void RamTarget::write(uint64_t address, const uint8_t* bytes, size_t length) {
    store(address, bytes, length);
    commit_versions(address, length);
}

void RamTarget::store(uint64_t address, const uint8_t* bytes, size_t length) {
    size_t done = 0;
    while (done < length) {
        const uint64_t current = address + done;
        const size_t offset = static_cast<size_t>(current & kChunkMask);
        const size_t count = std::min(static_cast<size_t>(kChunkBytes) - offset, length - done);
        Chunk& chunk = chunks_[current >> kChunkBits];
        for (size_t index = 0; index < count; ++index) {
            chunk.bytes[offset + index].store(bytes[done + index], std::memory_order_relaxed);
        }
        done += count;
    }
}

uint64_t RamTarget::version_at(uint64_t address) const {
    const auto chunk = chunks_.find(address >> kChunkBits);
    if (chunk == chunks_.end()) {
        return 0;
    }
    return chunk->second.versions[static_cast<size_t>(address & kChunkMask)];
}

void RamTarget::commit_versions(uint64_t address, size_t length) {
    if (length == 0) {
        return;
    }
    ++next_version_;
    size_t done = 0;
    while (done < length) {
        const uint64_t current = address + done;
        const size_t offset = static_cast<size_t>(current & kChunkMask);
        const size_t count = std::min(static_cast<size_t>(kChunkBytes) - offset, length - done);
        Chunk& chunk = chunks_[current >> kChunkBits];
        for (size_t index = 0; index < count; ++index) {
            chunk.versions[offset + index] = next_version_;
        }
        done += count;
    }
}

}  // namespace ruscv_sim
