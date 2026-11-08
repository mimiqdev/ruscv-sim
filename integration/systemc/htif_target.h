// htif_target.h - the host-served HTIF device window for the ruscv-sim smoke.
//
// htif D8. The target answers the HTIF endpoint `0x4000_8000`/8 through the C
// ABI's window-indexed device table (htif D7): one validated guest dword access
// becomes one blocking `ruscv_device.b_transport` call on the creator thread,
// synchronously, without waiting and without letting host time pass. The window
// has one register: a dword read answers zero (htif D3) and an accepted dword
// write reports exactly one RUSCV_DEVICE_EVENT_HTIF_WRITE whose value is the
// guest's eight little-endian bytes (htif D4).
//
// The target renders the standard console device's byte (device 1, command 1,
// payload byte) as one `htif-console:` line and records every accepted value in
// order as one `htif-value:` line, so the smoke can compare the guest's value
// and console sequences exactly without reading the counters. The exit summary
// reports the selection, the transaction, read, write, value and receive counts
// and the rendered console sequence.
//
// The three fault-option controls of htif §4 break this model in one named
// place each, so the smoke can require its own failure and never a normal run:
//
// * `--fault-htif-drop-event` answers the exit word's write with RUSCV_TLM_OK
//   and no event, while the console writes keep theirs, so no exit fact exists
//   and the guest's self-loop exhausts the turn bound;
// * `--fault-htif-wait` waits one Platform tick inside `b_transport` and reports
//   the interval as host elapsed time, which htif D9 forbids: host time may not
//   pass inside a device callback;
// * `--fault-htif-event-on-fault` reports the exit write's event with
//   RUSCV_TLM_ADDRESS_ERROR, which htif D4 forbids: only an accepted access
//   publishes an event.
//
// delay D8 adds the modeled access delay: `DeviceDelay::delay_fs` is written to
// `ruscv_payload.delay_fs` on every completed response, and
// `--fault-device-delay-above-max` annotates one femtosecond above the declared
// maximum on the first completed write instead, before that write is applied or
// recorded.

#ifndef RUSCV_SIM_SYSTEMC_HTIF_TARGET_H
#define RUSCV_SIM_SYSTEMC_HTIF_TARGET_H

#include <cstdint>
#include <string>
#include <vector>

#include <ruscv_sim.h>

#include "device_delay.h"

namespace ruscv_sim {

/// The smoke's fault-option controls that belong to the HTIF device target
/// (htif §4). They are taken one at a time and are never a normal model.
struct HtifFaults {
    /// `--fault-htif-drop-event`: answer the exit word's write with
    /// `RUSCV_TLM_OK` and no event.
    bool drop_event = false;
    /// `--fault-htif-wait`: wait one Platform tick inside `b_transport` and
    /// report it as host elapsed time.
    bool wait = false;
    /// `--fault-htif-event-on-fault`: report the exit write's event with
    /// `RUSCV_TLM_ADDRESS_ERROR`.
    bool event_on_fault = false;
};

/// The host's HTIF device window.
///
/// The object is not a SystemC module: htif D8 makes its `b_transport` a plain
/// C callback on the creator thread that never waits and never advances time.
/// Its C ABI table is copied at attach, so `user` and this object must outlive
/// the handle.
class HtifTarget {
public:
    /// The HTIF endpoint's base address and size (htif D3).
    static constexpr uint64_t kWindowBase = 0x40008000ULL;
    static constexpr uint64_t kWindowSize = 8;

    /// `faults` carries the smoke's fault-option controls and is empty in every
    /// normal run. `delay` carries the modeled access delay this target
    /// annotates on every completed response (delay D8), zero in every normal
    /// run that selects no device delay.
    explicit HtifTarget(HtifFaults faults = {}, DeviceDelay delay = {});

    /// The C ABI device table naming this object as `user`.
    ///
    /// The returned table is valid while this object lives. `transport_reset`
    /// discards the window's register state and accepts new accesses, so a
    /// reinstall starts from a fresh in-tree endpoint's state.
    ruscv_device table();

    /// Number of device transactions served (one per `b_transport` call).
    uint64_t b_transport_count() const { return b_transports_; }
    /// Number of those that were reads.
    uint64_t read_count() const { return reads_; }
    /// Number of those that were writes.
    uint64_t write_count() const { return writes_; }
    /// Number of accepted writes that reported the window's one event, which is
    /// exactly the value sequence the Platform publishes (htif D4).
    uint64_t value_count() const { return values_.size(); }
    /// The accepted values, in order (htif D8).
    const std::vector<uint64_t>& values() const { return values_; }
    /// The bytes the standard console device rendered, in order (htif D8).
    const std::string& console() const { return console_; }
    /// `receive` calls. The endpoint has no host-to-guest byte direction
    /// (htif D3), so the ABI never calls it and a normal run reports zero.
    uint64_t receive_count() const { return receives_; }

private:
    static void on_b_transport(void* user, ruscv_payload* payload);
    static void on_receive(void* user, uint8_t byte);
    static int32_t on_transport_reset(void* user);

    /// Serves one validated dword access.
    void serve(ruscv_payload& payload);
    /// True for the standard syscall exit word: device 0, command 0 and payload
    /// bit 0 set (htif D5).
    static bool is_exit_word(uint64_t value);
    /// Renders one standard console-device byte (device 1, command 1) and
    /// records it (htif D8).
    void render_console(uint64_t value);
    /// Discards the window's own register state (htif D6). The read answer
    /// stays the contract's fixed zero, so this value is never exposed to the
    /// guest; it is the state a reinstall discards.
    void discard_state();

    HtifFaults faults_;
    DeviceDelay delay_;
    /// True once the `--fault-device-delay-above-max` control has annotated its
    /// overshoot on the first completed write.
    bool above_max_done_ = false;
    uint64_t register_ = 0;
    std::vector<uint64_t> values_;
    std::string console_;
    uint64_t b_transports_ = 0;
    uint64_t reads_ = 0;
    uint64_t writes_ = 0;
    uint64_t receives_ = 0;
};

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_HTIF_TARGET_H
