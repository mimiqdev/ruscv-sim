// htif_target.cpp - the host-served HTIF device window for the ruscv-sim smoke.
//
// htif D8. See htif_target.h for the model and its three fault-option controls.
// The endpoint's read answer is zero on both routes (htif D3), so the in-tree
// run and this target have the same fixed point.

#include "htif_target.h"

#include <cinttypes>
#include <cstdio>
#include <cstring>
#include <string>

#include <systemc>

namespace ruscv_sim {
namespace {

/// One Platform tick in femtoseconds: the documented tick period (htif D9).
constexpr uint64_t kTickPeriodFs = 1000000;

/// The standard HTIF word's device, command and payload fields: device in bits
/// 63:56, command in bits 55:48 and the payload in bits 47:0.
constexpr unsigned kDeviceShift = 56;
constexpr unsigned kCommandShift = 48;
constexpr uint64_t kFieldMask = 0xff;
/// The standard console device and its putchar command (htif D8).
constexpr uint64_t kConsoleDevice = 1;
constexpr uint64_t kConsolePutchar = 1;
/// The syscall device and command whose payload carries the exit code
/// (htif D5, `try_extract_exit_code`'s standard branch).
constexpr uint64_t kSyscallDevice = 0;
constexpr uint64_t kSyscallCommand = 0;

/// Escapes one rendered byte for the target's console channel.
std::string escape_byte(unsigned char byte) {
    switch (byte) {
        case '\\': return "\\\\";
        case '\n': return "\\n";
        case '\r': return "\\r";
        case '\t': return "\\t";
        default: break;
    }
    if (byte >= 0x20 && byte < 0x7f) {
        return std::string(1, static_cast<char>(byte));
    }
    char escape[8] = {};
    std::snprintf(escape, sizeof(escape), "\\x%02x", byte);
    return escape;
}

}  // namespace

HtifTarget::HtifTarget(HtifFaults faults) : faults_(faults) { discard_state(); }

ruscv_device HtifTarget::table() {
    ruscv_device device{};
    device.window_base = kWindowBase;
    device.window_size = kWindowSize;
    device.user = this;
    device.b_transport = &HtifTarget::on_b_transport;
    device.receive = &HtifTarget::on_receive;
    device.transport_reset = &HtifTarget::on_transport_reset;
    return device;
}

void HtifTarget::on_b_transport(void* user, ruscv_payload* payload) {
    static_cast<HtifTarget*>(user)->serve(*payload);
}

void HtifTarget::on_receive(void* user, uint8_t /*byte*/) {
    // htif D3: the endpoint has no host-to-guest byte direction, so the ABI
    // never calls this. The count lets the smoke prove it did not happen.
    ++static_cast<HtifTarget*>(user)->receives_;
}

int32_t HtifTarget::on_transport_reset(void* user) {
    // htif D6: a reinstall takes the same path, so the model starts from a
    // fresh in-tree endpoint's state. The observation counters are the smoke's
    // exit evidence and are not part of the window's register state.
    static_cast<HtifTarget*>(user)->discard_state();
    return 1;
}

void HtifTarget::serve(ruscv_payload& payload) {
    ++b_transports_;
    if (faults_.wait) {
        // htif §4: --fault-htif-wait waits one Platform tick inside the callback
        // and reports the interval as host elapsed time. htif D9 forbids host
        // time inside a device callback, so the access is a protocol failure of
        // that access.
        const sc_core::sc_time before = sc_core::sc_time_stamp();
        sc_core::wait(sc_core::sc_time(kTickPeriodFs, sc_core::SC_FS));
        payload.host_elapsed_fs =
            static_cast<uint64_t>((sc_core::sc_time_stamp() - before).value());
    }
    if (payload.data == nullptr || payload.length != kWindowSize) {
        // The Platform's eight-byte width policy runs before this callback, so a
        // shorter or wider request is a broken caller rather than a guest
        // access.
        payload.status = RUSCV_TLM_BURST_ERROR;
        return;
    }
    if (payload.command == RUSCV_COMMAND_READ) {
        ++reads_;
        // htif D3: the read answer is the window's register state. This target
        // fixes it at zero, the in-tree endpoint's fixed point, so the parity
        // checks compare equal answers.
        std::memset(payload.data, 0, kWindowSize);
        payload.status = RUSCV_TLM_OK;
        return;
    }
    if (payload.command == RUSCV_COMMAND_WRITE) {
        ++writes_;
        uint64_t value = 0;
        for (size_t index = 0; index < kWindowSize; ++index) {
            value |= static_cast<uint64_t>(payload.data[index]) << (8 * index);
        }
        const bool exit_word = is_exit_word(value);
        if (faults_.event_on_fault && exit_word) {
            // htif §4: --fault-htif-event-on-fault reports the exit write's
            // event with a fault status. htif D4 accepts an event only on an
            // accepted access, so the write is not applied and no value
            // reaches the sink.
            payload.status = RUSCV_TLM_ADDRESS_ERROR;
            payload.device_event = RUSCV_DEVICE_EVENT_HTIF_WRITE;
            return;
        }
        if (faults_.drop_event && exit_word) {
            // htif §4: --fault-htif-drop-event answers the exit write with
            // RUSCV_TLM_OK and no event. The Platform then has no exit fact, so
            // the run continues to its turn bound; the console writes before it
            // keep their events.
            register_ = value;
            payload.status = RUSCV_TLM_OK;
            return;
        }
        // htif D4: one accepted dword write reports exactly one event whose
        // value is the guest's eight bytes, so leaving the data unchanged makes
        // the request's own value the response's.
        register_ = value;
        payload.status = RUSCV_TLM_OK;
        payload.device_event = RUSCV_DEVICE_EVENT_HTIF_WRITE;
        values_.push_back(value);
        std::printf("htif-value: 0x%016" PRIx64 "\n", value);
        render_console(value);
        return;
    }
    payload.status = RUSCV_TLM_COMMAND_ERROR;
}

bool HtifTarget::is_exit_word(uint64_t value) {
    const uint64_t device = (value >> kDeviceShift) & kFieldMask;
    const uint64_t command = (value >> kCommandShift) & kFieldMask;
    return device == kSyscallDevice && command == kSyscallCommand && (value & 1) != 0;
}

void HtifTarget::render_console(uint64_t value) {
    const uint64_t device = (value >> kDeviceShift) & kFieldMask;
    const uint64_t command = (value >> kCommandShift) & kFieldMask;
    if (device != kConsoleDevice || command != kConsolePutchar) {
        return;
    }
    const auto byte = static_cast<unsigned char>(value & kFieldMask);
    console_.push_back(static_cast<char>(byte));
    std::printf("htif-console: %s\n", escape_byte(byte).c_str());
    std::fflush(stdout);
}

void HtifTarget::discard_state() { register_ = 0; }

}  // namespace ruscv_sim