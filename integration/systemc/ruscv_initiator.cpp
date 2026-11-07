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
/// Used when no target was reachable, the target raised a C++ exception, or an
/// AT transaction was retired after its timeout (nb D9). It allocates nothing,
/// so reporting the failure cannot itself throw.  The C callback keeps the host
/// time it measured as the payload's `e`, which D4 step 1 charges.
void mark_unknown_completion(ruscv_payload* payload) {
    payload->status = RUSCV_TLM_INCOMPLETE;
    payload->outcome_unknown = 1;
    payload->delay_fs = 0;
    payload->atomic.outcome = RUSCV_ATOMIC_OUTCOME_NONE;
    payload->atomic.snapshot_length = 0;
}

}  // namespace

RuscvInitiator::RuscvInitiator(sc_core::sc_module_name name, RamTarget& ram,
                               bool direct_mem, uint64_t quantum_ticks,
                               InitiatorFaults faults, bool non_blocking,
                               uint64_t nb_latency_fs)
    : sc_core::sc_module(name),
      socket("socket"),
      ram_(ram),
      direct_mem_(direct_mem),
      quantum_ticks_(quantum_ticks),
      faults_(faults),
      non_blocking_(non_blocking),
      nb_latency_fs_(nb_latency_fs) {
    socket.register_nb_transport_bw(this, &RuscvInitiator::nb_transport_bw);
    // The target reaches the handle's direct-memory registry through this
    // backward call (direct-memory D13).
    socket.register_invalidate_direct_mem_ptr(this, &RuscvInitiator::invalidate_direct_mem_ptr);
    socket.bind(ram.socket);
}

tlm::tlm_sync_enum RuscvInitiator::nb_transport_bw(tlm::tlm_generic_payload&,
                                                  tlm::tlm_phase& phase,
                                                  sc_core::sc_time& delay) {
    // nb D7: this runs in the target's process and never waits. It records the
    // phase with its effective host time and wakes the driver SC_THREAD.
    if (at_state_ != AtState::Active) {
        // nb D9: a phase outside the one transaction in flight can only belong
        // to a retired transaction.  It is ignored for the Machine and counted
        // once, and answered with the phase the base protocol requires.
        if (at_state_ == AtState::Retired) {
            ++late_phases_;
            if (phase == tlm::BEGIN_RESP) {
                // Answering it with `TLM_COMPLETED` ends the target's
                // transaction, so it no longer holds the retired payload.
                at_target_holds_ = false;
            }
        }
        return phase == tlm::END_REQ ? tlm::TLM_ACCEPTED : tlm::TLM_COMPLETED;
    }
    if (phase == tlm::END_REQ) {
        if (!record_phase(phase, delay, false)) {
            ++late_phases_;
        }
        return tlm::TLM_ACCEPTED;
    }
    if (phase == tlm::BEGIN_RESP) {
        // nb D9: a `BEGIN_RESP` that is already late is counted and answered
        // here; one that arrives in time is recorded for the waiting process.
        // Either way the answer ends the target's transaction.
        if (!record_phase(phase, delay, false)) {
            ++late_phases_;
        }
        at_target_holds_ = false;
        return tlm::TLM_COMPLETED;
    }
    protocol_error_ = true;
    // A phase this protocol does not list is answered as a completion, so the
    // target ends its transaction and no storage is retained for it.
    at_target_holds_ = false;
    phase_event_.notify(sc_core::SC_ZERO_TIME);
    return tlm::TLM_COMPLETED;
}

bool RuscvInitiator::record_phase(const tlm::tlm_phase& phase,
                                  const sc_core::sc_time& delay, bool forward) {
    if (at_state_ != AtState::Active) {
        return false;
    }
    const sc_core::sc_time effective = sc_core::sc_time_stamp() + delay;
    if (effective.value() > at_deadline_fs_) {
        // nb D9: a phase whose annotation reaches past the timeout takes effect
        // after it, so it is late: ignored for the Machine and counted once.
        return false;
    }
    PendingPhase pending;
    pending.phase = phase;
    pending.effective = effective;
    pending.forward = forward;
    pending_phases_.push_back(pending);
    phase_event_.notify(delay == sc_core::SC_ZERO_TIME ? sc_core::SC_ZERO_TIME : delay);
    return true;
}

bool RuscvInitiator::wait_for_at_completion() {
    for (;;) {
        if (protocol_error_) {
            return false;
        }
        const uint64_t now = sc_core::sc_time_stamp().value();
        for (size_t index = 0; index < pending_phases_.size();) {
            if (pending_phases_[index].effective.value() > now) {
                ++index;
                continue;
            }
            const PendingPhase pending = pending_phases_[index];
            pending_phases_.erase(pending_phases_.begin() +
                                  static_cast<std::ptrdiff_t>(index));
            if (pending.phase == tlm::END_REQ) {
                continue;
            }
            if (pending.phase != tlm::BEGIN_RESP) {
                protocol_error_ = true;
                return false;
            }
            if (pending.forward) {
                tlm::tlm_phase end_resp = tlm::END_RESP;
                sc_core::sc_time delay = sc_core::SC_ZERO_TIME;
                const tlm::tlm_sync_enum sync =
                    socket->nb_transport_fw(at_transaction_->payload(), end_resp, delay);
                if (sync != tlm::TLM_COMPLETED && sync != tlm::TLM_ACCEPTED) {
                    protocol_error_ = true;
                    return false;
                }
            }
            // nb D9: the transaction completed at this phase's effective time,
            // and the target no longer holds it.
            at_target_holds_ = false;
            return true;
        }
        if (now >= at_deadline_fs_) {
            // nb D9: a transaction that is not complete at the timeout is
            // retired.  A completion at exactly the declared maximum is in
            // time, because the timeout is one femtosecond past it.
            return false;
        }
        // nb D9: every wait is bounded by the timeout, so the process wakes at
        // the earliest recorded phase or at the deadline, whichever is first.
        sc_core::sc_time next = sc_core::sc_time::from_value(at_deadline_fs_);
        for (const PendingPhase& pending : pending_phases_) {
            if (pending.effective < next) {
                next = pending.effective;
            }
        }
        sc_core::wait(next - sc_core::sc_time_stamp(), phase_event_);
    }
}

void RuscvInitiator::retire(ruscv_payload& payload) {
    // nb D9: a transaction that is not complete at the timeout is retired as an
    // unknown completion, with the time waited as `e`.  The C callback reports
    // that time, so this only sets the payload's unknown-completion fields.
    ++retirements_;
    mark_unknown_completion(&payload);
    at_state_ = AtState::Retired;
    pending_phases_.clear();
    phase_event_.cancel();
    protocol_error_ = false;
    if (!at_target_holds_ && !drain_end_resp_pending_) {
        // nb D9: the target already completed the transaction on the forward
        // path, so its storage is released at once.
        at_transaction_.reset();
    }
    // Otherwise the storage stays allocated until the target has completed the
    // transaction or released every reference it acquired; `drain_step` frees
    // it once the target has let go.
}

bool RuscvInitiator::drain_end_resp_due(sc_core::sc_time& due) const {
    if (!drain_end_resp_pending_) {
        return false;
    }
    due = drain_end_resp_due_;
    return true;
}

void RuscvInitiator::drain_step() {
    if (at_transaction_ == nullptr) {
        return;
    }
    if (drain_end_resp_pending_) {
        const sc_core::sc_time now = sc_core::sc_time_stamp();
        if (now < drain_end_resp_due_) {
            // nb D9: the retained transaction's `END_RESP` is sent no earlier
            // than the `BEGIN_RESP` phase's effective time.
            return;
        }
        tlm::tlm_phase end_resp = tlm::END_RESP;
        sc_core::sc_time delay = sc_core::SC_ZERO_TIME;
        const tlm::tlm_sync_enum sync =
            socket->nb_transport_fw(at_transaction_->payload(), end_resp, delay);
        drain_end_resp_pending_ = false;
        ++drain_end_resp_count_;
        if (sync == tlm::TLM_COMPLETED || sync == tlm::TLM_ACCEPTED) {
            // nb D8/D9: the forward `END_RESP` completes the retained
            // transaction for the target, which releases it.
            at_target_holds_ = false;
        }
    }
    if (!at_target_holds_ && !drain_end_resp_pending_) {
        // nb D9: the target has completed the retired transaction or released
        // every reference it acquired, so its storage can be freed.
        at_transaction_.reset();
    }
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
    // nb D8: under --nb the RAM model declares lambda as its per-transaction
    // maximum; otherwise it annotates zero delay and declares zero.
    table.max_delay_fs = non_blocking_ ? nb_latency_fs_ : 0;
    table.user = this;
    table.b_transport = &ruscv_systemc_b_transport;
    table.transport_dbg = &ruscv_systemc_transport_dbg;
    table.transport_reset = &ruscv_systemc_transport_reset;
    // NULL means the target offers no direct memory: the handle then behaves
    // exactly as direct memory off (direct-memory D12).
    table.get_direct_mem_ptr =
        direct_mem_ ? &ruscv_systemc_get_direct_mem_ptr : nullptr;
    // quantum D12/D13: the selection travels in the copied target table, so the
    // Machine bounds the local offset and every grant return syncs it.
    table.quantum_ticks = quantum_ticks_;
    // nb D6/D8: --nb declares that this target suspends host time.
    table.suspends_host_time = non_blocking_ ? 1 : 0;
    return table;
}

void RuscvInitiator::serve_b_transport(ruscv_payload& payload) {
    if (payload.may_suspend != 0) {
        serve_at_b_transport(payload);
        return;
    }
    PayloadExtension extension;
    extension.access_kind = payload.access_kind;
    extension.initiator = payload.initiator;
    copy_atomic_request(payload.atomic, extension);

    tlm::tlm_generic_payload transaction;
    prepare(transaction, payload.address, payload.command, payload.data, payload.length);
    transaction.set_extension(&extension);
    // quantum D13: the IEEE 1666 delay argument carries the Hart's local offset
    // on entry. This model's declared maximum is zero, so the target adds
    // nothing and the callback reports only what it returned above the offset.
    const uint64_t offset_fs = payload.local_offset_fs;
    sc_core::sc_time delay = sc_core::sc_time::from_value(offset_fs);
    socket->b_transport(transaction, delay);

    // Echo every request field a target could have changed. ruscv-sim compares
    // them with what it issued, so its own classification of a mutated payload
    // stays in charge (dev-plan D4).
    payload.address = transaction.get_address();
    payload.command = command_code(transaction.get_command());
    payload.length = transaction.get_data_length();
    payload.status = status_code(transaction.get_response_status());
    payload.outcome_unknown = extension.outcome_unknown;
    // The TLM hint flag is the ABI payload's `dmi_allowed` response field, the
    // only licence for ruscv-sim's one query of this access (direct-memory D3).
    payload.dmi_allowed = transaction.is_dmi_allowed() ? 1 : 0;
    copy_atomic_response(extension, payload.atomic);
    if (faults_.keep_offset) {
        // The smoke's --fault-keep-offset control (quantum §4): the full delay
        // argument is reported as an added delay, so the offset this target was
        // given exceeds the declared maximum of zero.
        payload.delay_fs = delay.value();
    } else if (delay.value() < offset_fs) {
        // quantum D13: a returned delay below the offset cannot be represented
        // as an added delay, and the target may already have committed a write,
        // so this is an unknown completion rather than an incomplete response.
        mark_unknown_completion(&payload);
    } else {
        payload.delay_fs = delay.value() - offset_fs;
    }
}

void RuscvInitiator::report_host_time(ruscv_payload& payload, uint64_t elapsed_fs) const {
    if (faults_.uncounted_elapsed) {
        // The smoke's --fault-uncounted-elapsed control (nb §4): the initiator
        // reports no host elapsed time while it reports the delay as the host
        // time that passed, so the kernel clock and the Machine cursor part.
        payload.host_elapsed_fs = 0;
    } else {
        payload.host_elapsed_fs = elapsed_fs;
    }
    if (faults_.short_delay && elapsed_fs != 0) {
        // The smoke's --fault-short-delay control (nb §4): a terminal response
        // whose host elapsed time exceeds its delay trips D4 step 5.
        payload.delay_fs = elapsed_fs - 1;
    }
}

void RuscvInitiator::serve_at_b_transport(ruscv_payload& payload) {
    const uint64_t begin_fs = sc_core::sc_time_stamp().value();
    at_transaction_ =
        std::make_unique<AtTransaction>(payload.address, payload.command, payload.data,
                                        payload.length);
    PayloadExtension& extension = at_transaction_->extension();
    extension.access_kind = payload.access_kind;
    extension.initiator = payload.initiator;
    copy_atomic_request(payload.atomic, extension);

    pending_phases_.clear();
    phase_event_.cancel();
    protocol_error_ = false;
    at_state_ = AtState::Active;
    at_target_holds_ = false;
    drain_end_resp_pending_ = false;
    drain_end_resp_due_ = sc_core::SC_ZERO_TIME;
    // nb D9: every wait of this process is bounded by the declared maximum plus
    // one femtosecond, so a completion at exactly the maximum, including a
    // delta-notified one at lambda = 0, is in time.
    at_deadline_fs_ = begin_fs + nb_latency_fs_ + 1;

    tlm::tlm_phase phase = tlm::BEGIN_REQ;
    sc_core::sc_time delay = sc_core::SC_ZERO_TIME;
    const tlm::tlm_sync_enum sync =
        socket->nb_transport_fw(at_transaction_->payload(), phase, delay);

    bool completed = false;
    switch (sync) {
        case tlm::TLM_ACCEPTED:
            // nb D7: the backward path supplies the next phase.
            at_target_holds_ = true;
            break;
        case tlm::TLM_UPDATED:
            // nb D7: the updated phase takes effect after its annotation, and
            // the target keeps the transaction until its final phase.
            at_target_holds_ = true;
            if (phase != tlm::END_REQ && phase != tlm::BEGIN_RESP) {
                protocol_error_ = true;
            } else if (!record_phase(phase, delay, true)) {
                // nb D9: a forward phase whose annotation reaches past the
                // timeout is late: counted and ignored.
                ++late_phases_;
                if (phase == tlm::BEGIN_RESP) {
                    // The early path keeps the transaction as drain state, and
                    // the drain sends its `END_RESP` no earlier than the
                    // phase's effective time.
                    drain_end_resp_pending_ = true;
                    drain_end_resp_due_ = sc_core::sc_time_stamp() + delay;
                }
            }
            break;
        case tlm::TLM_COMPLETED: {
            // nb D9: the target completed the transaction on the forward path,
            // so it holds no reference and the storage is released at once.
            at_target_holds_ = false;
            if (!pending_phases_.empty()) {
                protocol_error_ = true;
                break;
            }
            const sc_core::sc_time effective = sc_core::sc_time_stamp() + delay;
            if (effective.value() > at_deadline_fs_) {
                // A late forward completion is counted, and the wait below
                // still runs to the timeout before the transaction retires.
                ++late_phases_;
                break;
            }
            if (delay != sc_core::SC_ZERO_TIME) {
                sc_core::wait(delay);
            }
            completed = true;
            break;
        }
        default:
            // No other forward return value is a completion this path lists.
            protocol_error_ = true;
            break;
    }
    if (!protocol_error_ && !completed) {
        completed = wait_for_at_completion();
    }
    if (!completed || protocol_error_) {
        retire(payload);
        return;
    }

    const tlm::tlm_generic_payload& response = at_transaction_->payload();
    payload.address = response.get_address();
    payload.command = command_code(response.get_command());
    payload.length = response.get_data_length();
    payload.status = status_code(response.get_response_status());
    payload.outcome_unknown = extension.outcome_unknown;
    payload.dmi_allowed = response.is_dmi_allowed() ? 1 : 0;
    copy_atomic_response(extension, payload.atomic);
    if (payload.data != nullptr && !at_transaction_->data().empty()) {
        std::memcpy(payload.data, at_transaction_->data().data(), at_transaction_->data().size());
    }
    // nb D7: d = e, the host time from BEGIN_REQ to completion, so no
    // annotated remainder is left.
    payload.delay_fs = sc_core::sc_time_stamp().value() - begin_fs;
    at_state_ = AtState::Idle;
    at_transaction_.reset();
    pending_phases_.clear();
    phase_event_.cancel();
}

size_t RuscvInitiator::serve_transport_dbg(ruscv_debug& debug) {
    const uint64_t begin_fs = sc_core::sc_time_stamp().value();
    tlm::tlm_generic_payload transaction;
    prepare(transaction, debug.address, debug.command, debug.data, debug.length);
    const unsigned int transferred = socket->transport_dbg(transaction);
    if (sc_core::sc_time_stamp().value() != begin_fs) {
        // nb D7: a debug transport may not let host time pass.
        return 0;
    }
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
    const uint64_t begin_fs = sc_core::sc_time_stamp().value();
    const bool acknowledged = ram_.transport_reset();
    if (sc_core::sc_time_stamp().value() != begin_fs) {
        // nb D7: a reset may not let host time pass.
        return 0;
    }
    return acknowledged ? 1 : 0;
}

int32_t RuscvInitiator::serve_get_direct_mem_ptr(ruscv_direct_mem& query) {
    const uint64_t begin_fs = sc_core::sc_time_stamp().value();
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
    if (sc_core::sc_time_stamp().value() != begin_fs) {
        // nb D7: a direct-memory query may not let host time pass.
        return RUSCV_DMI_FAILED;
    }
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
    // nb D7: e is measured around the callback, so an unknown completion keeps
    // the host time that passed instead of reporting only zero.
    const uint64_t begin_fs = sc_core::sc_time_stamp().value();
    try {
        if (user == nullptr || payload == nullptr) {
            // No target was reachable, so the outcome is unknown just as it is
            // for a target that could not establish one.
            if (payload != nullptr) {
                mark_unknown_completion(payload);
            }
        } else {
            static_cast<RuscvInitiator*>(user)->serve_b_transport(*payload);
        }
    } catch (...) {
        if (payload != nullptr) {
            mark_unknown_completion(payload);
        }
    }
    if (payload != nullptr) {
        // nb D7/D9: `e` is the host time the callback measured, which a retired
        // transaction keeps as the time it waited, and the initiator applies
        // this build's two host-time fault controls on top of it.
        const uint64_t elapsed_fs = sc_core::sc_time_stamp().value() - begin_fs;
        if (user == nullptr) {
            payload->host_elapsed_fs = elapsed_fs;
        } else {
            static_cast<RuscvInitiator*>(user)->report_host_time(*payload, elapsed_fs);
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
