// uart_target.h - the host-served UART device window for the ruscv-sim smoke.
//
// device D9. The target answers the UART window `0x1000_0000`/`0x100` through
// the C ABI's window-indexed device table (device D8): one validated guest
// register access becomes one blocking `ruscv_device.b_transport` call on the
// creator thread, synchronously, without waiting and without letting host time
// pass. The model reproduces the register set the suite's UART guest uses:
// RBR/THR, IER, IIR/FCR, LCR, MCR, LSR, MSR, SCR, DLL/DLM and a receive FIFO.
//
// An accepted THR write with DLAB clear reports one
// RUSCV_DEVICE_EVENT_UART_TRANSMIT, the ABI's at-most-one guest-visible effect,
// and echoes the byte to this host's console. The event's value travels in the
// access's own data bytes, so no second field carries it. Every other accepted
// access reports RUSCV_DEVICE_EVENT_NONE.
//
// The three fault-option controls of device §4 break this model in one named
// place each, so the smoke can require its own failure and never a normal run:
//
// * `--fault-uart-no-lsr` answers LSR without the transmit-holding-empty bit, so
//   a guest that waits for it exhausts its turn bound instead of passing;
// * `--fault-uart-wait` reports one Platform tick of host elapsed time on a
//   device transaction, which device D4 forbids: host time may not pass inside
//   a device callback;
// * `--fault-uart-event-on-fault` reports a transmit event with
//   RUSCV_TLM_ADDRESS_ERROR, which device D4 forbids: only an accepted access
//   publishes an event.

#ifndef RUSCV_SIM_SYSTEMC_UART_TARGET_H
#define RUSCV_SIM_SYSTEMC_UART_TARGET_H

#include <cstdint>
#include <string>
#include <vector>

#include <ruscv_sim.h>

namespace ruscv_sim {

/// The smoke's fault-option controls that belong to the UART device target
/// (device §4). They are taken one at a time and are never a normal model.
struct UartFaults {
    /// `--fault-uart-no-lsr`: answer LSR with the transmit-holding-empty bit
    /// clear.
    bool no_lsr = false;
    /// `--fault-uart-wait`: report one Platform tick of host elapsed time on a
    /// device transaction.
    bool wait = false;
    /// `--fault-uart-event-on-fault`: report a transmit event with
    /// `RUSCV_TLM_ADDRESS_ERROR`.
    bool event_on_fault = false;
};

/// The host's UART device window.
///
/// The object is not a SystemC module: device D9 makes its `b_transport` a plain
/// C callback on the creator thread that never waits and never advances time.
/// Its C ABI table is copied at attach, so `user` and this object must outlive
/// the handle.
class UartTarget {
public:
    /// The UART window's base address and size (device D3).
    static constexpr uint64_t kWindowBase = 0x10000000ULL;
    static constexpr uint64_t kWindowSize = 0x100ULL;
    /// The transmit FIFO's depth, as in the in-tree `Uart16550`.
    static constexpr size_t kFifoDepth = 16;

    /// `faults` carries the smoke's fault-option controls and is empty in every
    /// normal run.
    explicit UartTarget(UartFaults faults = {});

    /// The C ABI device table naming this object as `user`.
    ///
    /// The returned table is valid while this object lives. `transport_reset`
    /// quiesces the model and accepts new accesses, so a reinstall starts from
    /// the same register state as a fresh in-tree UART.
    ruscv_device table();

    /// Number of device transactions served (one per `b_transport` call).
    uint64_t b_transport_count() const { return b_transports_; }
    /// Number of those that were reads.
    uint64_t read_count() const { return reads_; }
    /// Number of those that were writes.
    uint64_t write_count() const { return writes_; }
    /// Number of bytes an accepted THR write transmitted.
    uint64_t transmitted_count() const { return transmitted_.size(); }
    /// The bytes an accepted THR write transmitted, in order (device D7).
    const std::string& transmitted() const { return transmitted_; }

private:
    static void on_b_transport(void* user, ruscv_payload* payload);
    static void on_receive(void* user, uint8_t byte);
    static int32_t on_transport_reset(void* user);

    /// Serves one validated register access.
    void serve(ruscv_payload& payload);
    /// Reads one register, with the in-tree `Uart16550`'s answers.
    uint8_t read_reg(uint64_t offset);
    /// Applies one register write with the in-tree `Uart16550`'s effects.
    void write_reg(uint64_t offset, uint8_t value);
    /// Reads RBR, popping the receive FIFO.
    uint8_t read_rbr();
    /// Writes THR.
    void write_thr(uint8_t byte);
    /// Pushes one byte into the receive FIFO (device D8).
    void receive_byte(uint8_t byte);
    /// Recomputes DR, TEMT and THRE from the two FIFOs.
    void update_lsr();
    /// The in-tree interrupt-pending answer.
    bool interrupt_pending() const;
    /// The in-tree IIR answer.
    uint8_t interrupt_id() const;
    /// True when the divisor latch replaces RBR/THR and IER.
    bool dlab() const;
    /// Discards every byte and every register value (device D6).
    void reset_state();

    UartFaults faults_;
    std::vector<uint8_t> rx_fifo_;
    std::vector<uint8_t> tx_fifo_;
    uint8_t ier_ = 0;
    uint8_t fcr_ = 0;
    uint8_t lcr_ = 0;
    uint8_t mcr_ = 0;
    uint8_t lsr_ = 0;
    uint8_t msr_ = 0;
    uint8_t scr_ = 0;
    uint8_t dll_ = 0;
    uint8_t dlm_ = 0;
    uint8_t rx_trigger_ = 1;
    uint64_t b_transports_ = 0;
    uint64_t reads_ = 0;
    uint64_t writes_ = 0;
    std::string transmitted_;
};

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_UART_TARGET_H
