// device_delay.h - the modeled access delay the host device windows annotate
// (delay D8).
//
// The UART window's `UartTarget` and the HTIF endpoint's `HtifTarget` share
// this configuration. `delay_fs` is the modeled delay every completed response
// of a selected host device target adds; the Machine converts it to Platform
// ticks, rounds each access up on its own and charges it once through the guest
// turn's existing delay ledger. A zero here annotates no delay, which is the
// zero-delay host behavior of the previous contracts.
//
// A device transaction still never lets host time pass: `host_elapsed_fs` stays
// zero, the target never waits, and a delay above the declared maximum is the
// existing overshoot host failure of that access rather than a guest fault
// (delay D3/D4/D6).
//
// `above_max` is the smoke's `--fault-device-delay-above-max` control. The
// first completed write annotates `max_delay_fs + 1` instead, before the target
// applies or records that write, so the Machine's declared-maximum check
// refuses the access and the run fails with the overshoot diagnostic.

#ifndef RUSCV_SIM_SYSTEMC_DEVICE_DELAY_H
#define RUSCV_SIM_SYSTEMC_DEVICE_DELAY_H

#include <cstdint>

namespace ruscv_sim {

/// One host device window's delay annotation (delay D8).
struct DeviceDelay {
    /// The modeled delay every completed response adds, in exact femtoseconds.
    uint64_t delay_fs = 0;
    /// The `--fault-device-delay-above-max` control: annotate
    /// `max_delay_fs + 1` on the first completed write, before it is applied or
    /// recorded as accepted.
    bool above_max = false;
    /// The declared per-transaction maximum the control overshoots.
    uint64_t max_delay_fs = 0;
};

}  // namespace ruscv_sim

#endif  // RUSCV_SIM_SYSTEMC_DEVICE_DELAY_H
