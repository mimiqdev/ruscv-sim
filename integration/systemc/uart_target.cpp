// uart_target.cpp - the host-served UART device window for the ruscv-sim smoke.
//
// device D9. See uart_target.h for the model, its three fault-option controls
// and the device-delay annotation of delay D8. The register answers mirror the
// in-tree `Uart16550` (`src/peripherals/uart16550.rs`), because the smoke's
// guest fixes the values the two routes must agree on (device D7).

#include "uart_target.h"

#include <cstdio>
#include <string>

#include <systemc>

namespace ruscv_sim {
namespace {

/// One Platform tick in femtoseconds: the documented tick period (device D9).
constexpr uint64_t kTickPeriodFs = 1000000;

// The in-tree register map (`reg_offset` in `src/peripherals/uart16550.rs`).
constexpr uint64_t kRbrThr = 0x00;
constexpr uint64_t kIer = 0x01;
constexpr uint64_t kIirFcr = 0x02;
constexpr uint64_t kLcr = 0x03;
constexpr uint64_t kMcr = 0x04;
constexpr uint64_t kLsr = 0x05;
constexpr uint64_t kMsr = 0x06;
constexpr uint64_t kScr = 0x07;

// The in-tree LSR bits (`lsr_bits`).
constexpr uint8_t kLsrDr = 0x01;
constexpr uint8_t kLsrOe = 0x02;
constexpr uint8_t kLsrPe = 0x04;
constexpr uint8_t kLsrFe = 0x08;
constexpr uint8_t kLsrBi = 0x10;
constexpr uint8_t kLsrThre = 0x20;
constexpr uint8_t kLsrTemt = 0x40;
constexpr uint8_t kLsrRfe = 0x80;

// The in-tree IER bits (`ier_bits`).
constexpr uint8_t kIerErbfi = 0x01;
constexpr uint8_t kIerEtbei = 0x02;
constexpr uint8_t kIerElsi = 0x04;
constexpr uint8_t kIerEdssi = 0x08;

// The in-tree IIR bits and interrupt identities (`iir_bits`).
constexpr uint8_t kIirNoInt = 0x01;
constexpr uint8_t kIirFifoEn = 0xC0;
constexpr uint8_t kIirModemStatus = 0x00;
constexpr uint8_t kIirTransmitEmpty = 0x02;
constexpr uint8_t kIirReceiveData = 0x04;
constexpr uint8_t kIirLineStatus = 0x06;
constexpr uint8_t kIirCharacterTimeout = 0x0C;

// The in-tree FCR bits (`fcr_bits`).
constexpr uint8_t kFcrFifoEnable = 0x01;
constexpr uint8_t kFcrRcvrFifoReset = 0x02;
constexpr uint8_t kFcrXmitFifoReset = 0x04;
constexpr uint8_t kFcrRcvrTriggerLsb = 0x40;
constexpr uint8_t kFcrRcvrTriggerMsb = 0x80;

// The in-tree LCR DLAB bit (`lcr_bits`).
constexpr uint8_t kLcrDlab = 0x80;

// The in-tree MCR OUT2 bit (`mcr_bits`).
constexpr uint8_t kMcrOut2 = 0x08;

/// Escapes one transmitted byte for the target's console channel.
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

UartTarget::UartTarget(UartFaults faults, DeviceDelay delay)
    : faults_(faults), delay_(delay) {
    reset_state();
}

ruscv_device UartTarget::table() {
    ruscv_device device{};
    device.window_base = kWindowBase;
    device.window_size = kWindowSize;
    device.user = this;
    device.b_transport = &UartTarget::on_b_transport;
    device.receive = &UartTarget::on_receive;
    device.transport_reset = &UartTarget::on_transport_reset;
    return device;
}

void UartTarget::on_b_transport(void* user, ruscv_payload* payload) {
    static_cast<UartTarget*>(user)->serve(*payload);
}

void UartTarget::on_receive(void* user, uint8_t byte) {
    // device D8: one byte the host pushes into the window. This ABI version adds
    // no entry that pushes a byte, so the callback is reached only through the
    // Machine's own UART receive path.
    static_cast<UartTarget*>(user)->receive_byte(byte);
}

int32_t UartTarget::on_transport_reset(void* user) {
    // device D6: a reinstall takes the same path, so the model starts from a
    // fresh in-tree UART's state. This target always quiesces.
    static_cast<UartTarget*>(user)->reset_state();
    return 1;
}

void UartTarget::serve(ruscv_payload& payload) {
    ++b_transports_;
    if (faults_.wait) {
        // device §4: --fault-uart-wait waits one Platform tick inside the
        // callback and reports the interval as host elapsed time. device D4
        // forbids host time inside a device callback, so the access is a
        // protocol failure of that access.
        const sc_core::sc_time before = sc_core::sc_time_stamp();
        sc_core::wait(sc_core::sc_time(kTickPeriodFs, sc_core::SC_FS));
        payload.host_elapsed_fs =
            static_cast<uint64_t>((sc_core::sc_time_stamp() - before).value());
    }
    if (payload.data == nullptr || payload.length != 1) {
        // The Platform's byte-only width policy runs before this callback, so a
        // wider request is a broken caller rather than a guest access.
        payload.status = RUSCV_TLM_BURST_ERROR;
        return;
    }
    const uint64_t offset = payload.address - kWindowBase;
    if (payload.command == RUSCV_COMMAND_READ) {
        ++reads_;
        payload.data[0] = read_reg(offset);
        payload.status = RUSCV_TLM_OK;
        // delay D8: a completed response annotates this window's modeled delay.
        payload.delay_fs = delay_.delay_fs;
        return;
    }
    if (payload.command == RUSCV_COMMAND_WRITE) {
        ++writes_;
        const uint8_t byte = payload.data[0];
        const bool transmit = offset == kRbrThr && !dlab();
        if (transmit && faults_.event_on_fault) {
            // device §4: --fault-uart-event-on-fault reports a transmit event on
            // a fault status. device D4 accepts an event only on an accepted
            // access, so the write is not applied and no byte is transmitted.
            payload.status = RUSCV_TLM_ADDRESS_ERROR;
            payload.device_event = RUSCV_DEVICE_EVENT_UART_TRANSMIT;
            return;
        }
        if (delay_.above_max && !above_max_done_) {
            // delay D8: --fault-device-delay-above-max annotates one femtosecond
            // above the declared maximum on the first completed write, before
            // the write is applied or recorded as accepted. The Machine's
            // declared-maximum check refuses the access, so the byte never
            // reaches this target's accepted sequence.
            above_max_done_ = true;
            payload.status = RUSCV_TLM_OK;
            payload.delay_fs = delay_.max_delay_fs + 1;
            return;
        }
        write_reg(offset, byte);
        if (transmit) {
            // device D7: one accepted THR write transmits exactly one byte, and
            // the event's value is that byte. The console channel is this
            // target's own observation, so the smoke can prove a byte reached
            // this host's console without reading the counters.
            transmitted_.push_back(static_cast<char>(byte));
            std::printf("uart-console: %s\n", escape_byte(byte).c_str());
            std::fflush(stdout);
            payload.device_event = RUSCV_DEVICE_EVENT_UART_TRANSMIT;
        }
        payload.status = RUSCV_TLM_OK;
        // delay D8: a completed response annotates this window's modeled delay.
        payload.delay_fs = delay_.delay_fs;
        return;
    }
    payload.status = RUSCV_TLM_COMMAND_ERROR;
}

uint8_t UartTarget::read_reg(uint64_t offset) {
    switch (offset) {
        case kRbrThr:
            return dlab() ? dll_ : read_rbr();
        case kIer:
            return dlab() ? dlm_ : ier_;
        case kIirFcr:
            return interrupt_id();
        case kLcr:
            return lcr_;
        case kMcr:
            return mcr_;
        case kLsr: {
            const uint8_t lsr = lsr_;
            // The in-tree read clears the four error bits.
            lsr_ &= static_cast<uint8_t>(~(kLsrOe | kLsrPe | kLsrFe | kLsrBi));
            // device §4: --fault-uart-no-lsr answers without the
            // transmit-holding-empty bit, so the guest never leaves its wait.
            return faults_.no_lsr ? static_cast<uint8_t>(lsr & ~kLsrThre) : lsr;
        }
        case kMsr:
            // The in-tree model returns a fixed zero.
            return 0x00;
        case kScr:
            return scr_;
        default:
            return 0;
    }
}

void UartTarget::write_reg(uint64_t offset, uint8_t value) {
    switch (offset) {
        case kRbrThr:
            if (dlab()) {
                dll_ = value;
            } else {
                write_thr(value);
            }
            return;
        case kIer:
            if (dlab()) {
                dlm_ = value;
            } else {
                ier_ = static_cast<uint8_t>(value & 0x0F);
            }
            return;
        case kIirFcr: {
            // FCR is write-only.
            fcr_ = value;
            if ((value & kFcrRcvrFifoReset) != 0) {
                rx_fifo_.clear();
                update_lsr();
            }
            if ((value & kFcrXmitFifoReset) != 0) {
                tx_fifo_.clear();
                update_lsr();
            }
            const uint8_t trigger =
                static_cast<uint8_t>((value & (kFcrRcvrTriggerMsb | kFcrRcvrTriggerLsb)) >> 6);
            switch (trigger) {
                case 0: rx_trigger_ = 1; break;
                case 1: rx_trigger_ = 4; break;
                case 2: rx_trigger_ = 8; break;
                case 3: rx_trigger_ = 14; break;
                default: rx_trigger_ = 1; break;
            }
            return;
        }
        case kLcr:
            lcr_ = value;
            return;
        case kMcr:
            mcr_ = value;
            return;
        case kLsr:
        case kMsr:
            // Both are read-only in the in-tree model.
            return;
        case kScr:
            scr_ = value;
            return;
        default:
            return;
    }
}

uint8_t UartTarget::read_rbr() {
    if (rx_fifo_.empty()) {
        return 0;
    }
    const uint8_t byte = rx_fifo_.front();
    rx_fifo_.erase(rx_fifo_.begin());
    update_lsr();
    return byte;
}

void UartTarget::write_thr(uint8_t byte) {
    if (tx_fifo_.size() < kFifoDepth) {
        tx_fifo_.push_back(byte);
        update_lsr();
    }
}

void UartTarget::receive_byte(uint8_t byte) {
    if (rx_fifo_.size() < kFifoDepth) {
        rx_fifo_.push_back(byte);
        update_lsr();
    } else {
        // The in-tree model flags an overrun instead of dropping the byte
        // silently.
        lsr_ |= kLsrOe;
        if ((fcr_ & kFcrFifoEnable) != 0) {
            lsr_ |= kLsrRfe;
        }
    }
}

void UartTarget::update_lsr() {
    if (!rx_fifo_.empty()) {
        lsr_ |= kLsrDr;
    } else {
        lsr_ &= static_cast<uint8_t>(~kLsrDr);
    }
    if (tx_fifo_.empty()) {
        lsr_ |= kLsrTemt;
    } else {
        lsr_ &= static_cast<uint8_t>(~kLsrTemt);
    }
    if (tx_fifo_.size() < kFifoDepth) {
        lsr_ |= kLsrThre;
    } else {
        lsr_ &= static_cast<uint8_t>(~kLsrThre);
    }
}

bool UartTarget::interrupt_pending() const {
    // OUT2 must be set before any interrupt is presented.
    if ((mcr_ & kMcrOut2) == 0) {
        return false;
    }
    if ((ier_ & kIerElsi) != 0 && (lsr_ & 0x1E) != 0) {
        return true;
    }
    if ((ier_ & kIerErbfi) != 0 && (lsr_ & kLsrDr) != 0) {
        return true;
    }
    if ((ier_ & kIerEtbei) != 0 && (lsr_ & kLsrThre) != 0) {
        return true;
    }
    // The in-tree model assumes no modem status change.
    return false;
}

uint8_t UartTarget::interrupt_id() const {
    if (!interrupt_pending()) {
        return kIirNoInt;
    }
    uint8_t iir = 0;
    if ((fcr_ & kFcrFifoEnable) != 0) {
        iir |= kIirFifoEn;
    }
    // Priority: line status, then receive data, then transmit empty, then modem.
    if ((ier_ & kIerElsi) != 0 && (lsr_ & 0x1E) != 0) {
        iir |= kIirLineStatus;
    } else if ((ier_ & kIerErbfi) != 0 && (lsr_ & kLsrDr) != 0) {
        if ((fcr_ & kFcrFifoEnable) != 0 && rx_fifo_.size() >= rx_trigger_ &&
            rx_trigger_ == 14) {
            iir |= kIirCharacterTimeout;
        } else {
            iir |= kIirReceiveData;
        }
    } else if ((ier_ & kIerEtbei) != 0 && (lsr_ & kLsrThre) != 0) {
        iir |= kIirTransmitEmpty;
    } else if ((ier_ & kIerEdssi) != 0) {
        iir |= kIirModemStatus;
    } else {
        iir |= kIirNoInt;
    }
    return iir;
}

bool UartTarget::dlab() const { return (lcr_ & kLcrDlab) != 0; }

void UartTarget::reset_state() {
    rx_fifo_.clear();
    tx_fifo_.clear();
    ier_ = 0;
    fcr_ = 0;
    lcr_ = 0;
    mcr_ = 0;
    lsr_ = static_cast<uint8_t>(kLsrThre | kLsrTemt);
    msr_ = 0;
    scr_ = 0;
    dll_ = 0;
    dlm_ = 0;
    rx_trigger_ = 1;
    update_lsr();
}

}  // namespace ruscv_sim
