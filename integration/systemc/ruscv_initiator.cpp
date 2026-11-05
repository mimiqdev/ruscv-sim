// ruscv_initiator.cpp - the C ABI target callbacks over one TLM initiator socket.

#include "ruscv_initiator.h"

#include <array>
#include <cstdio>
#include <cstring>

// The IEEE 1666 access values are the C ABI's `RUSCV_DMI_ACCESS_*` values, so
// the granted access crosses the boundary without a mapping table.
static_assert(static_cast<int32_t>(tlm::tlm_dmi::DMI_ACCESS_NONE) == RUSCV_DMI_ACCESS_NONE,
              "RUSCV_DMI_ACCESS_* must match tlm::tlm_dmi::dmi_access_e");
static_assert(static_cast<int32_t>(tlm::tlm_dmi::DMI_ACCESS_READ) == RUSCV_DMI_ACCESS_READ,
              "RUSCV_DMI_ACCESS_* must match tlm::tlm_dmi::dmi_access_e");
static_assert(static_cast<int32_t>(tlm::tlm_dmi::DMI_ACCESS_WRITE) == RUSCV_DMI_ACCESS_WRITE,
              "RUSCV_DMI_ACCESS_* must match tlm::tlm_dmi::dmi_access_e");
static_assert(static_cast<int32_t>(tlm::tlm_dmi::DMI_ACCESS_READ_WRITE) ==
                  RUSCV_DMI_ACCESS_READ_WRITE,
              "RUSCV_DMI_ACCESS_* must match tlm::tlm_dmi::dmi_access_e");

namespace ruscv_sim {
namespace {

/// The ruscv-sim command code for one TLM command.
///
/// A non-blocking command is not a request the ABI can have made, so it maps to
/// a code the adapter never accepts. The adapter then reports the protocol
/// failure instead of treating the transaction as completed.
int32_t command_code(tlm::tlm_command command) {
    switch (command) {
        case tlm::TLM_READ_COMMAND: return RUSCV_COMMAND_READ;
        case tlm::TLM_WRITE_COMMAND: return RUSCV_COMMAND_WRITE;
        default: return -1;
    }
}

/// The ABI status code for one blocking response status.
///
/// A value outside the seven standard statuses is not a completion, so it maps
/// to the untouched incomplete status, exactly as the adapter classifies it.
int32_t status_code(tlm::tlm_response_status status) {
    switch (status) {
        case tlm::TLM_OK_RESPONSE: return RUSCV_TLM_OK;
        case tlm::TLM_ADDRESS_ERROR_RESPONSE: return RUSCV_TLM_ADDRESS_ERROR;
        case tlm::TLM_COMMAND_ERROR_RESPONSE: return RUSCV_TLM_COMMAND_ERROR;
        case tlm::TLM_BURST_ERROR_RESPONSE: return RUSCV_TLM_BURST_ERROR;
        case tlm::TLM_BYTE_ENABLE_ERROR_RESPONSE: return RUSCV_TLM_BYTE_ENABLE_ERROR;
        case tlm::TLM_GENERIC_ERROR_RESPONSE: return RUSCV_TLM_GENERIC_ERROR;
        case tlm::TLM_INCOMPLETE_RESPONSE: return RUSCV_TLM_INCOMPLETE;
        default: return RUSCV_TLM_INCOMPLETE;
    }
}

/// Prepares the request half of a generic payload for one ABI transaction.
void prepare(tlm::tlm_generic_payload& transaction, uint64_t address, int32_t command,
             uint8_t* data, size_t length) {
    transaction.set_command(command == RUSCV_COMMAND_READ ? tlm::TLM_READ_COMMAND
                                                          : tlm::TLM_WRITE_COMMAND);
    transaction.set_address(address);
    transaction.set_data_ptr(data);
    transaction.set_data_length(length);
    // One access is one non-burst transaction (dev-plan D3).
    transaction.set_streaming_width(length);
    transaction.set_byte_enable_ptr(nullptr);
    transaction.set_byte_enable_length(0);
    transaction.set_response_status(tlm::TLM_INCOMPLETE_RESPONSE);
}

/// Copies the ABI atomic request half into the TLM extension.
void copy_atomic_request(const ruscv_atomic& atomic, PayloadExtension& extension) {
    extension.atomic_present = atomic.present;
    extension.atomic_kind = atomic.kind;
    extension.atomic_operation = atomic.operation;
    extension.atomic_width = atomic.width;
    extension.acquire = atomic.acquire;
    extension.release = atomic.release;
    extension.reserved = atomic.reserved;
    extension.reserved_address = atomic.reserved_address;
    extension.reserved_length = atomic.reserved_length;
    extension.reserved_snapshot_length = atomic.reserved_snapshot_length;
    if (atomic.reserved_snapshot_length <= RUSCV_SNAPSHOT_MAX) {
        std::memcpy(extension.reserved_snapshot.data(), atomic.reserved_snapshot,
                    atomic.reserved_snapshot_length);
    }
}

/// Copies the TLM extension's atomic response half back into the ABI payload.
void copy_atomic_response(const PayloadExtension& extension, ruscv_atomic& atomic) {
    atomic.outcome = extension.atomic_outcome;
    atomic.snapshot_length = extension.snapshot_length;
    if (extension.snapshot_length <= RUSCV_SNAPSHOT_MAX) {
        std::memcpy(atomic.snapshot, extension.snapshot.data(), extension.snapshot_length);
    }
}

/// Marks one blocking payload as an unknown completion (dev-plan D4).
///
/// Used when no target was reachable or the target raised a C++ exception. It
/// allocates nothing, so reporting the failure cannot itself throw.
void mark_unknown_completion(ruscv_payload* payload) {
    payload->status = RUSCV_TLM_INCOMPLETE;
    payload->outcome_unknown = 1;
    payload->delay_fs = 0;
    payload->atomic.outcome = RUSCV_ATOMIC_OUTCOME_NONE;
    payload->atomic.snapshot_length = 0;
}

}  // namespace

RuscvInitiator::RuscvInitiator(sc_core::sc_module_name name, RamTarget& ram,
                               bool direct_mem)
    : sc_core::sc_module(name), socket("socket"), ram_(ram), direct_mem_(direct_mem) {
    socket.register_nb_transport_bw(this, &RuscvInitiator::nb_transport_bw);
    // The target reaches the handle's direct-memory registry through this
    // backward call (direct-memory D13).
    socket.register_invalidate_direct_mem_ptr(this, &RuscvInitiator::invalidate_direct_mem_ptr);
    socket.bind(ram.socket);
}

tlm::tlm_sync_enum RuscvInitiator::nb_transport_bw(tlm::tlm_generic_payload&, tlm::tlm_phase&,
                                                  sc_core::sc_time&) {
    // Unused: the RAM target answers only blocking calls (dev-plan D1/D9).
    return tlm::TLM_ACCEPTED;
}

void RuscvInitiator::invalidate_direct_mem_ptr(sc_dt::uint64 start_range,
                                             sc_dt::uint64 end_range) {
    // The one ABI entry a target may call inside a callback: it touches only
    // the handle's direct-memory registry, which the Rust target wrapper keeps
    // outside the run-state lock (direct-memory D12).
    if (machine_ == nullptr) {
        return;
    }
    const int32_t status = ruscv_machine_invalidate_direct_mem(
        machine_, static_cast<uint64_t>(start_range), static_cast<uint64_t>(end_range));
    if (status == RUSCV_OK) {
        ++direct_mem_invalidations_;
    } else {
        ++direct_mem_errors_;
        std::fprintf(stderr,
                     "systemc-smoke: ruscv_machine_invalidate_direct_mem failed "
                     "with status %d\n",
                     status);
    }
}

ruscv_target RuscvInitiator::target_table() {
    ruscv_target table{};
    // The RAM model annotates zero delay, so its declared maximum is zero too.
    table.max_delay_fs = 0;
    table.user = this;
    table.b_transport = &ruscv_systemc_b_transport;
    table.transport_dbg = &ruscv_systemc_transport_dbg;
    table.transport_reset = &ruscv_systemc_transport_reset;
    // NULL means the target offers no direct memory: the handle then behaves
    // exactly as direct memory off (direct-memory D12).
    table.get_direct_mem_ptr =
        direct_mem_ ? &ruscv_systemc_get_direct_mem_ptr : nullptr;
    return table;
}

void RuscvInitiator::serve_b_transport(ruscv_payload& payload) {
    PayloadExtension extension;
    extension.access_kind = payload.access_kind;
    extension.initiator = payload.initiator;
    copy_atomic_request(payload.atomic, extension);

    tlm::tlm_generic_payload transaction;
    prepare(transaction, payload.address, payload.command, payload.data, payload.length);
    transaction.set_extension(&extension);
    sc_core::sc_time delay = sc_core::SC_ZERO_TIME;
    socket->b_transport(transaction, delay);

    // Echo every request field a target could have changed. ruscv-sim compares
    // them with what it issued, so its own classification of a mutated payload
    // stays in charge (dev-plan D4).
    payload.address = transaction.get_address();
    payload.command = command_code(transaction.get_command());
    payload.length = transaction.get_data_length();
    payload.status = status_code(transaction.get_response_status());
    payload.delay_fs = delay.value();
    payload.outcome_unknown = extension.outcome_unknown;
    // The TLM hint flag is the ABI payload's `dmi_allowed` response field, the
    // only licence for ruscv-sim's one query of this access (direct-memory D3).
    payload.dmi_allowed = transaction.is_dmi_allowed() ? 1 : 0;
    copy_atomic_response(extension, payload.atomic);
}

size_t RuscvInitiator::serve_transport_dbg(ruscv_debug& debug) {
    tlm::tlm_generic_payload transaction;
    prepare(transaction, debug.address, debug.command, debug.data, debug.length);
    const unsigned int transferred = socket->transport_dbg(transaction);
    if (transaction.get_response_status() != tlm::TLM_OK_RESPONSE) {
        return 0;
    }
    if (transaction.get_address() != debug.address ||
        command_code(transaction.get_command()) != debug.command ||
        transaction.get_data_length() != debug.length) {
        return 0;
    }
    return transferred;
}

int32_t RuscvInitiator::serve_transport_reset() {
    return ram_.transport_reset() ? 1 : 0;
}

int32_t RuscvInitiator::serve_get_direct_mem_ptr(ruscv_direct_mem& query) {
    // The forward DMI interface: the query's address and direction are the
    // transaction's address and command. The ABI wrapper zero-initializes the
    // answer half of the struct on the Rust side, so the target may overwrite
    // every member of it (direct-memory D3/D13).
    tlm::tlm_generic_payload transaction;
    transaction.set_address(query.address);
    transaction.set_command(query.access == RUSCV_DMI_ACCESS_READ
                              ? tlm::TLM_READ_COMMAND
                              : tlm::TLM_WRITE_COMMAND);
    tlm::tlm_dmi data;
    const bool granted = socket->get_direct_mem_ptr(transaction, data);
    if (!granted) {
        return RUSCV_DMI_DENIED;
    }
    query.start = static_cast<uint64_t>(data.get_start_address());
    query.end = static_cast<uint64_t>(data.get_end_address());
    query.pointer = data.get_dmi_ptr();
    query.access = static_cast<int32_t>(data.get_granted_access());
    query.read_latency_fs = data.get_read_latency().value();
    query.write_latency_fs = data.get_write_latency().value();
    return RUSCV_DMI_GRANTED;
}

void RuscvInitiator::invalidate_all_direct_mem() {
    // The invalidation is the target's: its storage changed, so it issues the
    // backward call on the bound initiator socket (direct-memory D8).
    ++direct_mem_scheduled_;
    ram_.invalidate_all_direct_mem();
}

extern "C" void ruscv_systemc_b_transport(void* user, ruscv_payload* payload) {
    // No Rust or C++ exception may cross the ABI boundary (dev-plan D8): a C++
    // exception is an unknown completion (D4), so it is reported through the
    // payload instead of unwinding into the ruscv-sim call that made it. The
    // handler allocates nothing, so reporting the failure cannot itself throw.
    try {
        if (user == nullptr || payload == nullptr) {
            // No target was reachable, so the outcome is unknown just as it is
            // for a target that could not establish one.
            if (payload != nullptr) {
                mark_unknown_completion(payload);
            }
            return;
        }
        static_cast<RuscvInitiator*>(user)->serve_b_transport(*payload);
    } catch (...) {
        if (payload != nullptr) {
            mark_unknown_completion(payload);
        }
    }
}

extern "C" size_t ruscv_systemc_transport_dbg(void* user, ruscv_debug* debug) {
    try {
        if (user == nullptr || debug == nullptr) {
            return 0;
        }
        return static_cast<RuscvInitiator*>(user)->serve_transport_dbg(*debug);
    } catch (...) {
        // A short transfer is a failed debug transport, which is a host failure
        // during installation and a host failure during polling (dev-plan D6).
        return 0;
    }
}

extern "C" int32_t ruscv_systemc_transport_reset(void* user) {
    try {
        if (user == nullptr) {
            return 0;
        }
        return static_cast<RuscvInitiator*>(user)->serve_transport_reset();
    } catch (...) {
        // Without the acknowledgement the Platform keeps the handle unusable
        // instead of trusting a half-reset transport (dev-plan D6).
        return 0;
    }
}

extern "C" int32_t ruscv_systemc_get_direct_mem_ptr(void* user,
                                                  ruscv_direct_mem* direct_mem) {
    try {
        if (user == nullptr || direct_mem == nullptr) {
            // No target was reachable to establish an answer: this is the
            // contained-failure answer, not a denial (direct-memory D3/D12).
            return RUSCV_DMI_FAILED;
        }
        return static_cast<RuscvInitiator*>(user)->serve_get_direct_mem_ptr(*direct_mem);
    } catch (...) {
        // A contained C++ exception is the failed answer: the access becomes an
        // unknown completion and the transport is poisoned.
        return RUSCV_DMI_FAILED;
    }
}

}  // namespace ruscv_sim
