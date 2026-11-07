// main.cpp - the SystemC host that drives one Machine through the ruscv-sim C ABI.
//
// dev-plan D9. The initiator is one bounded-grant loop on an `SC_THREAD`:
//
// * each grant starts at the current `sc_time_stamp()`, which is the kernel's
//   committed time and therefore the Machine's cursor;
// * the grant is bounded by a started-slot budget and an inclusive horizon;
// * after it the thread `wait()`s the consumed amount minus the host time that
//   passed inside the grant's transactions, so the kernel commits global time
//   only from that amount and still ends at `committed_end_fs` (ADR-0004 §4.4,
//   nb D5);
// * `sc_stop()` follows a terminal `ruscv_machine_result`, or a host
//   classification (an ABI error, a kernel clock that disagrees with the Machine
//   cursor, a live grant that consumed no modeled time, or an exhausted grant or
//   turn bound). A host stop before a result is never a run result and never
//   passes: the `--stop-after-grants` control proves that.
//
// With `--quantum` the Machine may run ahead of the kernel's committed time by
// a bounded local offset (quantum D12/D13). This loop stays the only quantum
// keeper: there is one initiator, so `tlm_quantumkeeper` and
// `tlm_global_quantum` are not used. A counted sync at a turn boundary ends the
// grant, and every grant return syncs before it reports its facts, so
// `committed_end_fs` is the Machine's committed time at each return and the
// kernel's `wait(grant.consumed_fs)` keeps `sc_time_stamp()` equal to it. The
// check below therefore holds after every grant, and a mismatch fails the run.
//
// `--nb` selects the non-blocking AT path (nb D7/D8), with `--nb-latency` and
// `--nb-path`; `--grant-trace` writes one line per grant for the smoke's D10
// comparisons. `--fault-keep-offset` and `--fault-decrease-delay` are the
// contract's two quantum fault-option controls (quantum §4), and
// `--fault-late-response`, `--fault-uncounted-elapsed`, `--fault-short-delay`,
// `--fault-wait-blocking` and `--fault-wait-debug` are the five non-blocking
// ones (nb §4): each breaks the host or the target in one named place so the
// smoke can require its own named failure. None is a normal host.
//
// At every stop after a retirement the driver first probes the transport reset
// while the target still holds the transaction, then runs the kernel until the
// target has no scheduled phase and the initiator holds no drain state, and
// only then destroys the handle and stops the kernel, so every late phase is
// delivered and ignored (nb D9). The exit summary reports the late-phase count,
// the retirement count, the drain's forward `END_RESP` phases and whether a
// transaction is still open at the target.
//
// The C ABI handle is created and destroyed on this thread, which is the
// SystemC kernel thread, so the ABI's creator-thread affinity holds for every
// call and every target callback. The 1 fs time resolution the adapter's exact
// femtosecond mapping requires is established before the kernel starts.

#include <cinttypes>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <string>
#include <thread>
#include <utility>

#include <ruscv_sim.h>
#include <systemc>

#include "ram_target.h"
#include "ruscv_initiator.h"

namespace {

/// nb D9: the most steps the drain takes before the host calls it a failure.
/// A legitimate drain needs a handful: each step waits for the next scheduled
/// phase or the retained `END_RESP` time, not a fixed increment.
constexpr uint64_t kMaxDrainSteps = 1000;
/// The documented default Platform tick period: one nanosecond, in femtoseconds.
constexpr uint64_t kTickPeriodFs = 1000000;
/// One bounded grant never crosses more than this much modeled time: 1 ms.
constexpr uint64_t kHorizonFs = 1000000000000;
/// The default started-slot budget of one bounded grant.
constexpr uint64_t kTurnBudget = 10000;
/// A host safety net: this many grants without a terminal result is a failure.
constexpr uint64_t kMaxGrants = 10000;
/// The default bound on completed turns, the C ABI counterpart of the public
/// CLI's `--max-cycles` default in the guest suite. A guest that loops forever
/// (for example after a route corrupts its exit signal) must fail the host's
/// budget instead of spinning the kernel.
constexpr uint64_t kMaxTurns = 100000;
/// nb D8: the largest accepted AT latency, so no timeout or fault offset can
/// overflow a femtosecond value.
constexpr uint64_t kMaxNbLatencyFs = 1000000000000000ULL;

/// One smoke run's configuration.
struct Options {
    std::string elf;
    int32_t isa = RUSCV_ISA_RV64IMAFDC;
    std::string commit_log;
    uint64_t turn_budget = kTurnBudget;
    uint64_t horizon_fs = kHorizonFs;
    /// Zero: the deliberate early-stop control is off. A terminal result or a
    /// host-classified failure still stops the run.
    uint64_t stop_after_grants = 0;
    uint64_t max_grants = kMaxGrants;
    uint64_t max_turns = kMaxTurns;
    /// The `--dmi` selection: the target table offers a direct-memory query.
    bool direct_mem = false;
    /// Zero: no scheduled invalidation. Otherwise the driver asks the target
    /// to invalidate every grant after each Nth grant.
    uint64_t direct_mem_invalidate_every = 0;
    /// The quantum selection in Platform ticks; zero is off (quantum D2/D13).
    uint64_t quantum = 0;
    /// nb D7/D8: the AT selection and its target latency and return path.
    bool non_blocking = false;
    uint64_t nb_latency_fs = 0;
    bool nb_latency_set = false;
    ruscv_sim::AtPath nb_path = ruscv_sim::AtPath::Accepted;
    bool nb_path_set = false;
    /// nb D10: one line per grant for the smoke's per-grant comparisons.
    std::string grant_trace;
    /// The two fault-option controls (quantum §4). Each requires a quantum: with
    /// no local offset neither could produce its failure.
    bool fault_keep_offset = false;
    bool fault_decrease_delay = false;
    /// The five non-blocking fault-option controls (nb §4). `--fault-late-response`,
    /// `--fault-uncounted-elapsed` and `--fault-short-delay` need `--nb`; the
    /// blocking-mode controls `--fault-wait-blocking` and `--fault-wait-debug`
    /// must not be combined with it.
    bool fault_late_response = false;
    bool fault_uncounted_elapsed = false;
    bool fault_short_delay = false;
    bool fault_wait_blocking = false;
    bool fault_wait_debug = false;
};

void print_usage(const char* program) {
    std::printf(
        "usage: %s <elf> [--isa rv64ima|rv64imac|rv64imafd|rv64imafdc]\n"
        "       [--log-commits <path>] [--turn-budget <turns>]\n"
        "       [--horizon-fs <femtoseconds>] [--stop-after-grants <grants>]\n"
        "       [--max-grants <grants>] [--max-turns <turns>]\n"
        "       [--dmi] [--dmi-invalidate-every <grants>] [--quantum <ticks>]\n"
        "       [--nb] [--nb-latency <femtoseconds>]\n"
        "       [--nb-path accepted|updated|early|annotated|completed]\n"
        "       [--grant-trace <path>]\n"
        "       [--fault-keep-offset] [--fault-decrease-delay]\n"
        "       [--fault-late-response] [--fault-uncounted-elapsed]\n"
        "       [--fault-short-delay] [--fault-wait-blocking] [--fault-wait-debug]\n"
        "\n"
        "Runs one ruscv-sim Machine under this SystemC kernel. The RAM target is\n"
        "the zero-delay blocking model the initiator socket is bound to. Exit\n"
        "status is the guest exit code for a terminal exited run and 1 for a\n"
        "failed run or a host-classified failure; the --stop-after-grants control,\n"
        "which leaves no run result, exits 2.\n"
        "\n"
        "--max-turns bounds completed turns, the counterpart of the public CLI's\n"
        "--max-cycles; exhausting it without a terminal result is a failure.\n"
        "--dmi lets the RAM target answer direct-memory queries so covered\n"
        "accesses copy through a host pointer; --dmi-invalidate-every <grants>\n"
        "makes the driver ask the target to invalidate every grant after each\n"
        "Nth grant, and requires --dmi.\n"
        "\n"
        "--quantum <ticks> selects temporal decoupling in Platform ticks (zero is\n"
        "off), so the Machine may run ahead of the kernel clock by a bounded local\n"
        "offset; the kernel still commits only what a grant returns, so the clock\n"
        "equals committed_end_fs after every grant. --nb makes the target suspend\n"
        "host time and serves a may_suspend access through the non-blocking base\n"
        "protocol (nb D7/D8); --nb-latency <fs> sets its latency lambda and the\n"
        "declared maximum, and --nb-path selects its return path. --grant-trace\n"
        "<path> writes consumed_fs, committed_end_fs and host_elapsed_fs for every\n"
        "grant (nb D10). --fault-keep-offset and --fault-decrease-delay are the\n"
        "two fault-option controls of the quantum contract: each requires\n"
        "--quantum, they are taken one at a time, and each must fail the run with\n"
        "its own diagnostic. The five non-blocking fault options are the same kind\n"
        "of control (nb §4): --fault-late-response, --fault-uncounted-elapsed and\n"
        "--fault-short-delay require --nb, while --fault-wait-blocking and\n"
        "--fault-wait-debug are blocking-mode controls and must not be combined\n"
        "with it. Exactly one fault option may be selected in a run.\n",
        program);
}

/// Reads one unsigned 64-bit decimal option value.
bool parse_u64(const char* text, uint64_t& out) {
    if (text == nullptr || *text == '\0') {
        return false;
    }
    uint64_t value = 0;
    for (const char* cursor = text; *cursor != '\0'; ++cursor) {
        if (*cursor < '0' || *cursor > '9') {
            return false;
        }
        const uint64_t digit = static_cast<uint64_t>(*cursor - '0');
        if (value > (UINT64_MAX - digit) / 10) {
            return false;
        }
        value = value * 10 + digit;
    }
    out = value;
    return true;
}

/// Resolves one ISA profile name to its ABI code.
bool parse_isa(const char* text, int32_t& out) {
    if (std::strcmp(text, "rv64ima") == 0) {
        out = RUSCV_ISA_RV64IMA;
    } else if (std::strcmp(text, "rv64imac") == 0) {
        out = RUSCV_ISA_RV64IMAC;
    } else if (std::strcmp(text, "rv64imafd") == 0) {
        out = RUSCV_ISA_RV64IMAFD;
    } else if (std::strcmp(text, "rv64imafdc") == 0) {
        out = RUSCV_ISA_RV64IMAFDC;
    } else {
        return false;
    }
    return true;
}

/// Resolves one non-blocking return-path name (nb D8).
bool parse_nb_path(const char* text, ruscv_sim::AtPath& out) {
    if (std::strcmp(text, "accepted") == 0) {
        out = ruscv_sim::AtPath::Accepted;
    } else if (std::strcmp(text, "updated") == 0) {
        out = ruscv_sim::AtPath::Updated;
    } else if (std::strcmp(text, "early") == 0) {
        out = ruscv_sim::AtPath::Early;
    } else if (std::strcmp(text, "annotated") == 0) {
        out = ruscv_sim::AtPath::Annotated;
    } else if (std::strcmp(text, "completed") == 0) {
        out = ruscv_sim::AtPath::Completed;
    } else {
        return false;
    }
    return true;
}

/// Parses the command line; returns false after printing the reason.
bool parse_options(int argc, char** argv, Options& options) {
    bool have_elf = false;
    for (int index = 1; index < argc; ++index) {
        const char* argument = argv[index];
        const auto value = [&](const char* name) -> const char* {
            if (index + 1 >= argc) {
                std::fprintf(stderr, "systemc-smoke: %s needs a value\n", name);
                return nullptr;
            }
            return argv[++index];
        };
        if (std::strcmp(argument, "--help") == 0 || std::strcmp(argument, "-h") == 0) {
            print_usage(argv[0]);
            std::exit(0);
        } else if (std::strcmp(argument, "--isa") == 0) {
            const char* text = value("--isa");
            if (text == nullptr || !parse_isa(text, options.isa)) {
                std::fprintf(stderr, "systemc-smoke: unknown ISA profile\n");
                return false;
            }
        } else if (std::strcmp(argument, "--log-commits") == 0) {
            const char* text = value("--log-commits");
            if (text == nullptr) {
                return false;
            }
            options.commit_log = text;
        } else if (std::strcmp(argument, "--turn-budget") == 0) {
            const char* text = value("--turn-budget");
            if (text == nullptr || !parse_u64(text, options.turn_budget) ||
                options.turn_budget == 0) {
                std::fprintf(stderr, "systemc-smoke: --turn-budget needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--horizon-fs") == 0) {
            const char* text = value("--horizon-fs");
            if (text == nullptr || !parse_u64(text, options.horizon_fs) ||
                options.horizon_fs == 0) {
                std::fprintf(stderr, "systemc-smoke: --horizon-fs needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--stop-after-grants") == 0) {
            const char* text = value("--stop-after-grants");
            if (text == nullptr || !parse_u64(text, options.stop_after_grants) ||
                options.stop_after_grants == 0) {
                std::fprintf(stderr, "systemc-smoke: --stop-after-grants needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--max-grants") == 0) {
            const char* text = value("--max-grants");
            if (text == nullptr || !parse_u64(text, options.max_grants) ||
                options.max_grants == 0) {
                std::fprintf(stderr, "systemc-smoke: --max-grants needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--max-turns") == 0) {
            const char* text = value("--max-turns");
            if (text == nullptr || !parse_u64(text, options.max_turns) ||
                options.max_turns == 0) {
                std::fprintf(stderr, "systemc-smoke: --max-turns needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--dmi") == 0) {
            options.direct_mem = true;
        } else if (std::strcmp(argument, "--dmi-invalidate-every") == 0) {
            const char* text = value("--dmi-invalidate-every");
            if (text == nullptr || !parse_u64(text, options.direct_mem_invalidate_every) ||
                options.direct_mem_invalidate_every == 0) {
                std::fprintf(stderr,
                             "systemc-smoke: --dmi-invalidate-every needs a positive count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--quantum") == 0) {
            const char* text = value("--quantum");
            if (text == nullptr || !parse_u64(text, options.quantum)) {
                std::fprintf(stderr, "systemc-smoke: --quantum needs a tick count\n");
                return false;
            }
        } else if (std::strcmp(argument, "--nb") == 0) {
            options.non_blocking = true;
        } else if (std::strcmp(argument, "--nb-latency") == 0) {
            const char* text = value("--nb-latency");
            if (text == nullptr || !parse_u64(text, options.nb_latency_fs)) {
                std::fprintf(stderr, "systemc-smoke: --nb-latency needs a femtosecond count\n");
                return false;
            }
            options.nb_latency_set = true;
        } else if (std::strcmp(argument, "--nb-path") == 0) {
            const char* text = value("--nb-path");
            if (text == nullptr || !parse_nb_path(text, options.nb_path)) {
                std::fprintf(stderr,
                             "systemc-smoke: --nb-path must be accepted, updated, early, "
                             "annotated or completed\n");
                return false;
            }
            options.nb_path_set = true;
        } else if (std::strcmp(argument, "--grant-trace") == 0) {
            const char* text = value("--grant-trace");
            if (text == nullptr || *text == '\0') {
                std::fprintf(stderr, "systemc-smoke: --grant-trace needs a path\n");
                return false;
            }
            options.grant_trace = text;
        } else if (std::strcmp(argument, "--fault-keep-offset") == 0) {
            options.fault_keep_offset = true;
        } else if (std::strcmp(argument, "--fault-decrease-delay") == 0) {
            options.fault_decrease_delay = true;
        } else if (std::strcmp(argument, "--fault-late-response") == 0) {
            options.fault_late_response = true;
        } else if (std::strcmp(argument, "--fault-uncounted-elapsed") == 0) {
            options.fault_uncounted_elapsed = true;
        } else if (std::strcmp(argument, "--fault-short-delay") == 0) {
            options.fault_short_delay = true;
        } else if (std::strcmp(argument, "--fault-wait-blocking") == 0) {
            options.fault_wait_blocking = true;
        } else if (std::strcmp(argument, "--fault-wait-debug") == 0) {
            options.fault_wait_debug = true;
        } else if (argument[0] == '-') {
            std::fprintf(stderr, "systemc-smoke: unknown option %s\n", argument);
            return false;
        } else if (!have_elf) {
            options.elf = argument;
            have_elf = true;
        } else {
            std::fprintf(stderr, "systemc-smoke: more than one ELF path\n");
            return false;
        }
    }
    if (!have_elf) {
        std::fprintf(stderr, "systemc-smoke: an ELF path is required\n");
        return false;
    }
    if (!options.non_blocking && (options.nb_latency_set || options.nb_path_set)) {
        // nb D7: latency and path only select a return path for --nb.
        std::fprintf(stderr, "systemc-smoke: --nb-latency and --nb-path require --nb\n");
        return false;
    }
    if (options.nb_latency_fs > kMaxNbLatencyFs) {
        // nb D8: the declared maximum must stay representable.
        std::fprintf(stderr,
                     "systemc-smoke: --nb-latency must be at most %" PRIu64 " fs\n",
                     kMaxNbLatencyFs);
        return false;
    }
    if (options.direct_mem_invalidate_every != 0 && !options.direct_mem) {
        // Without a grant there is nothing to invalidate, so this control
        // would check nothing: require the selection it exercises.
        std::fprintf(stderr, "systemc-smoke: --dmi-invalidate-every requires --dmi\n");
        return false;
    }
    if (options.fault_keep_offset && options.fault_decrease_delay) {
        // The two fault controls break the same offset arithmetic in different
        // places, so one run cannot exercise both: require one at a time.
        std::fprintf(stderr, "systemc-smoke: --fault-keep-offset and --fault-decrease-delay "
                             "cannot be combined\n");
        return false;
    }
    if ((options.fault_keep_offset || options.fault_decrease_delay) && options.quantum == 0) {
        // Both fault controls break the offset arithmetic of quantum D13, so
        // with no local offset neither could fail: require the selection they
        // exercise instead of silently passing.
        std::fprintf(stderr, "systemc-smoke: --fault-keep-offset and --fault-decrease-delay "
                             "require --quantum\n");
        return false;
    }
    const int fault_options = static_cast<int>(options.fault_keep_offset) +
                              static_cast<int>(options.fault_decrease_delay) +
                              static_cast<int>(options.fault_late_response) +
                              static_cast<int>(options.fault_uncounted_elapsed) +
                              static_cast<int>(options.fault_short_delay) +
                              static_cast<int>(options.fault_wait_blocking) +
                              static_cast<int>(options.fault_wait_debug);
    if (fault_options > 1) {
        // Every fault option changes one place and produces its own named
        // failure, so one run cannot exercise two of them.
        std::fprintf(stderr, "systemc-smoke: the fault options are taken one at a time\n");
        return false;
    }
    if ((options.fault_late_response || options.fault_uncounted_elapsed ||
         options.fault_short_delay) &&
        !options.non_blocking) {
        // Each of these three breaks the AT path, so without --nb it would check
        // nothing: require the selection it exercises.
        std::fprintf(stderr,
                     "systemc-smoke: --fault-late-response, --fault-uncounted-elapsed and "
                     "--fault-short-delay require --nb\n");
        return false;
    }
    if ((options.fault_wait_blocking || options.fault_wait_debug) && options.non_blocking) {
        // These two break a blocking callback, whose requests carry no
        // `may_suspend`; under --nb they are not the control they name.
        std::fprintf(stderr,
                     "systemc-smoke: --fault-wait-blocking and --fault-wait-debug must not be "
                     "combined with --nb\n");
        return false;
    }
    return true;
}

/// The profile name for one ABI ISA code.
const char* isa_name(int32_t isa) {
    switch (isa) {
        case RUSCV_ISA_RV64IMA: return "rv64ima";
        case RUSCV_ISA_RV64IMAC: return "rv64imac";
        case RUSCV_ISA_RV64IMAFD: return "rv64imafd";
        case RUSCV_ISA_RV64IMAFDC: return "rv64imafdc";
        default: return "unknown";
    }
}

/// The name of one non-blocking return path.
const char* nb_path_name(ruscv_sim::AtPath path) {
    switch (path) {
        case ruscv_sim::AtPath::Accepted: return "accepted";
        case ruscv_sim::AtPath::Updated: return "updated";
        case ruscv_sim::AtPath::Early: return "early";
        case ruscv_sim::AtPath::Annotated: return "annotated";
        case ruscv_sim::AtPath::Completed: return "completed";
    }
    return "unknown";
}

/// The name of one run classification.
const char* state_name(int32_t state) {
    switch (state) {
        case RUSCV_RUN_RUNNING: return "running";
        case RUSCV_RUN_EXITED: return "exited";
        case RUSCV_RUN_FAILED: return "failed";
        default: return "unknown";
    }
}

/// Drives one Machine in bounded grants until it reports a terminal result or the
/// host classifies a failure.
class MachineDriver : public sc_core::sc_module {
public:
    MachineDriver(sc_core::sc_module_name name, Options options,
                  ruscv_sim::RuscvInitiator& initiator, ruscv_sim::RamTarget& ram,
                  std::thread::id creator_thread)
        : sc_core::sc_module(name),
          options_(std::move(options)),
          initiator_(initiator),
          ram_(ram),
          creator_thread_(creator_thread) {
        SC_THREAD(run);
    }

    /// True when `ruscv_machine_result` reported a terminal result.
    bool terminal() const { return terminal_; }
    /// True when the host stopped the run before a terminal result.
    bool stopped_early() const { return stopped_early_; }
    /// True when the calls or the Machine failed the run.
    bool failed() const { return failed_; }
    /// Number of grants whose kernel clock equalled `committed_end_fs`.
    uint64_t clock_checks() const { return clock_checks_; }
    uint64_t consumed_total_fs() const { return consumed_fs_; }
    uint64_t host_elapsed_total_fs() const { return host_elapsed_fs_; }
    uint64_t transport_delay_fs() const { return transport_delay_fs_; }
    const ruscv_result& result() const { return result_; }

private:
    /// Reports one failed call, then drains and destroys the handle.
    void stop_with_failure(const char* what, int32_t status, ruscv_machine* machine) {
        char diagnostic[512] = {};
        if (machine != nullptr) {
            ruscv_machine_diagnostic(machine, diagnostic, sizeof(diagnostic));
        }
        std::fprintf(stderr, "systemc-smoke: %s failed with status %d%s%s\n", what, status,
                     diagnostic[0] != '\0' ? ": " : "", diagnostic);
        failed_ = true;
        finish(machine);
    }

    /// Reports one host-level failure, then drains and destroys the handle.
    void stop_with_message(const char* message, ruscv_machine* machine) {
        std::fprintf(stderr, "systemc-smoke: %s\n", message);
        failed_ = true;
        finish(machine);
    }

    /// Drains after a retirement, destroys the handle and stops the kernel.
    void finish(ruscv_machine* machine) {
        if (machine != nullptr) {
            drain_after_retirement(machine);
        }
        destroy(machine);
        sc_core::sc_stop();
    }

    /// nb D9: at every stop after a retirement the host first withholds the
    /// transport reset while the target is still open, then runs the kernel
    /// until the RAM target has no scheduled phase and the initiator holds no
    /// drain state, so every late phase is delivered and ignored. Only then is
    /// the handle destroyed and the kernel stopped.
    void drain_after_retirement(ruscv_machine* machine) {
        if (initiator_.retirement_count() == 0) {
            return;
        }
        if (initiator_.target_holds_transaction()) {
            // nb D8/D9: while a transaction is open at the target, its
            // transport reset must withhold the acknowledgement, so the
            // recovery path reports a non-OK status.
            const int32_t reset = ruscv_machine_reset(machine);
            std::printf(
                "systemc-smoke: reset with an open AT transaction returned status %d\n", reset);
            std::fflush(stdout);
            if (reset == RUSCV_OK) {
                std::fprintf(stderr,
                             "systemc-smoke: the target acknowledged a transport reset while an "
                             "AT transaction was still open\n");
                failed_ = true;
            }
        } else {
            // The forward path completed the transaction, so no target state
            // stays open and there is no acknowledgement to withhold.
            std::printf(
                "systemc-smoke: reset probe skipped: the target holds no open AT transaction\n");
            std::fflush(stdout);
        }
        for (uint64_t step = 0; step < kMaxDrainSteps; ++step) {
            initiator_.drain_step();
            if (!initiator_.drain_pending() && ram_.scheduled_phase_count() == 0) {
                return;
            }
            const sc_core::sc_time now = sc_core::sc_time_stamp();
            bool have_next = false;
            sc_core::sc_time next = now;
            if (ram_.scheduled_phase_count() != 0) {
                next = ram_.next_scheduled_phase();
                have_next = true;
            }
            sc_core::sc_time end_resp_due = now;
            if (initiator_.drain_end_resp_due(end_resp_due) &&
                (!have_next || end_resp_due < next)) {
                next = end_resp_due;
                have_next = true;
            }
            if (have_next && next > now) {
                // Run the kernel to the next late phase rather than stepping.
                sc_core::wait(next - now);
            } else {
                sc_core::wait(sc_core::SC_ZERO_TIME);
            }
        }
        std::fprintf(stderr,
                     "systemc-smoke: the drain did not release the retired transaction\n");
        failed_ = true;
    }

    /// Unbinds the handle from the initiator, then destroys it. The unbind
    /// keeps a late backward invalidation from naming a stale handle.
    void destroy(ruscv_machine* machine) {
        if (machine == nullptr) {
            return;
        }
        initiator_.bind_machine(nullptr);
        ruscv_machine_destroy(machine);
    }

    void run() {
        if (std::this_thread::get_id() != creator_thread_) {
            // nb D7: the pinned build's coroutine package runs every SC_THREAD on
            // the kernel's OS thread. A pthread-process build would resume a
            // suspended callback on another thread, which the ABI forbids.
            stop_with_message(
                "the SystemC kernel runs SC_THREAD on another OS thread; a pthread-process "
                "build is unsupported",
                nullptr);
            return;
        }
        if (!options_.grant_trace.empty()) {
            grant_trace_.open(options_.grant_trace, std::ios::out | std::ios::trunc);
            if (!grant_trace_) {
                char message[512] = {};
                std::snprintf(message, sizeof(message),
                              "cannot open the --grant-trace file %s",
                              options_.grant_trace.c_str());
                stop_with_message(message, nullptr);
                return;
            }
        }
        ruscv_machine* machine = nullptr;
        const ruscv_target table = initiator_.target_table();
        int32_t status = ruscv_machine_create(&table, options_.isa, kTickPeriodFs, &machine);
        if (status != RUSCV_OK) {
            stop_with_failure("ruscv_machine_create", status, nullptr);
            return;
        }
        initiator_.bind_machine(machine);
        status = ruscv_machine_load_elf(machine, options_.elf.c_str());
        if (status != RUSCV_OK) {
            stop_with_failure("ruscv_machine_load_elf", status, machine);
            return;
        }
        if (!options_.commit_log.empty()) {
            status = ruscv_machine_set_commit_log(machine, options_.commit_log.c_str());
            if (status != RUSCV_OK) {
                stop_with_failure("ruscv_machine_set_commit_log", status, machine);
                return;
            }
        }
        for (;;) {
            if (options_.stop_after_grants != 0 && grants_ >= options_.stop_after_grants) {
                // The early-stop control: the kernel stops although ruscv-sim
                // has not reported a terminal result. This is not a run result.
                stopped_early_ = true;
                std::printf(
                    "systemc-smoke: host stop after %" PRIu64
                    " grant(s) before a terminal ruscv-sim result\n",
                    grants_);
                std::fflush(stdout);
                finish(machine);
                return;
            }
            if (grants_ >= options_.max_grants) {
                stop_with_message("no terminal ruscv-sim result within the grant guard", machine);
                return;
            }
            if (turns_seen_ >= options_.max_turns) {
                // The whole turn budget is spent and the run is still live, so no
                // grant could reach a terminal result: fail loudly instead of
                // letting a guest loop (for example after a route corrupts its
                // exit signal) spin the kernel. The CLI's --max-cycles is the
                // same bound on the native route.
                char message[256] = {};
                std::snprintf(message, sizeof(message),
                              "exceeded the %" PRIu64
                              " turn limit with %" PRIu64
                              " completed turns and no terminal ruscv-sim result",
                              options_.max_turns, turns_seen_);
                stop_with_message(message, machine);
                return;
            }
            // Never grant past the turn limit: the grant budget is the smaller of
            // the configured grant size and the turns still allowed.
            const uint64_t remaining_turns = options_.max_turns - turns_seen_;
            const uint64_t grant_budget =
                remaining_turns < options_.turn_budget ? remaining_turns : options_.turn_budget;
            const uint64_t start_fs = sc_core::sc_time_stamp().value();
            ruscv_grant grant{};
            status = ruscv_machine_run_grant(machine, start_fs, start_fs + options_.horizon_fs,
                                             grant_budget, &grant);
            if (status != RUSCV_OK) {
                stop_with_failure("ruscv_machine_run_grant", status, machine);
                return;
            }
            ++grants_;
            consumed_fs_ += grant.consumed_fs;
            host_elapsed_fs_ += grant.host_elapsed_fs;
            transport_delay_fs_ = grant.facts.transport_delay_fs;
            if (grant_trace_) {
                grant_trace_ << "grant " << grants_ << " consumed_fs=" << grant.consumed_fs
                             << " committed_end_fs=" << grant.committed_end_fs
                             << " host_elapsed_fs=" << grant.host_elapsed_fs << '\n';
            }
            // The scheduled invalidation is target-originated: the target
            // issues the backward call and the initiator forwards it to
            // ruscv_machine_invalidate_direct_mem, the one ABI entry a target
            // callback may use (direct-memory D8/D13). It runs here, after the
            // grant and before the terminal check, so the scheduled count is
            // exactly grants/N even when the run ends on a scheduled grant;
            // invalidating a handle that is about to be destroyed is
            // harmless, because the wrapper's registry stays alive until
            // destroy and destroying invokes no target callback.
            if (options_.direct_mem_invalidate_every != 0 &&
                grants_ % options_.direct_mem_invalidate_every == 0) {
                initiator_.invalidate_all_direct_mem();
                if (initiator_.direct_mem_error_count() != 0) {
                    stop_with_message("a direct-memory invalidation was refused", machine);
                    return;
                }
            }
            const bool elapsed_exceeds_consumed = grant.host_elapsed_fs > grant.consumed_fs;
            const uint64_t remainder_fs =
                elapsed_exceeds_consumed ? 0 : grant.consumed_fs - grant.host_elapsed_fs;
            if (!elapsed_exceeds_consumed) {
                if (remainder_fs != 0) {
                    // nb D5: the host time inside the grant's transactions has
                    // already passed, so only the remainder is waited.
                    sc_core::wait(sc_core::sc_time::from_value(remainder_fs));
                }
                if (sc_core::sc_time_stamp().value() != grant.committed_end_fs) {
                    char message[256] = {};
                    std::snprintf(message, sizeof(message),
                                  "the kernel clock %" PRIu64
                                  " fs does not match the Machine cursor %" PRIu64 " fs",
                                  static_cast<uint64_t>(sc_core::sc_time_stamp().value()),
                                  grant.committed_end_fs);
                    stop_with_message(message, machine);
                    return;
                }
                // quantum D12/D13: a grant return is a sync, so after the
                // remainder wait the kernel clock is the Machine's committed
                // time. Counting the comparisons lets the smoke assert that
                // every grant was checked.
                ++clock_checks_;
            }
            ruscv_result result{};
            status = ruscv_machine_result(machine, &result);
            if (status != RUSCV_OK) {
                stop_with_failure("ruscv_machine_result", status, machine);
                return;
            }
            turns_seen_ = result.turns;
            if (result.state != RUSCV_RUN_RUNNING) {
                if (elapsed_exceeds_consumed && result.state != RUSCV_RUN_FAILED) {
                    // nb D5: only an already-failed run keeps its result; any
                    // other result with host time above the consumed amount is
                    // a host failure.
                    char message[256] = {};
                    std::snprintf(message, sizeof(message),
                                  "the grant's host elapsed time %" PRIu64
                                  " fs exceeds its consumed amount %" PRIu64 " fs",
                                  grant.host_elapsed_fs, grant.consumed_fs);
                    stop_with_message(message, machine);
                    return;
                }
                terminal_ = true;
                result_ = result;
                std::printf(
                    "systemc-smoke: result state=%s exit_code=%u turns=%" PRIu64
                    " pc=0x%016" PRIx64 " grants=%" PRIu64 " consumed_fs=%" PRIu64 "\n",
                    state_name(result.state), result.exit_code, result.turns, result.pc, grants_,
                    consumed_fs_);
                std::fflush(stdout);
                if (result.state == RUSCV_RUN_FAILED) {
                    // A classified failure keeps its message in the handle; print
                    // it here so a CI log names the cause instead of only "failed".
                    char diagnostic[512] = {};
                    ruscv_machine_diagnostic(machine, diagnostic, sizeof(diagnostic));
                    std::fprintf(stderr, "systemc-smoke: failed run diagnostic: %s\n",
                                 diagnostic);
                    // quantum §4: the failed grant's `uncertain` flag is the C
                    // ABI's unknown-completion classification, so a fault-option
                    // control can require that class and not only a message.
                    ruscv_boundary facts{};
                    if (ruscv_machine_boundary(machine, &facts) == RUSCV_OK) {
                        std::fprintf(stderr, "systemc-smoke: failed run facts uncertain=%d\n",
                                     facts.uncertain);
                    }
                    std::fflush(stderr);
                }
                finish(machine);
                return;
            }
            if (elapsed_exceeds_consumed) {
                // nb D5: a run whose host elapsed exceeds the consumed amount
                // commits nothing more; a failed run keeps its own result.
                char message[256] = {};
                std::snprintf(message, sizeof(message),
                              "the grant's host elapsed time %" PRIu64
                              " fs exceeds its consumed amount %" PRIu64 " fs",
                              grant.host_elapsed_fs, grant.consumed_fs);
                stop_with_message(message, machine);
                return;
            }
            if (grant.consumed_fs == 0) {
                // A boundary that neither advanced modeled time nor ended the
                // run cannot make progress: the wait would never be reached.
                char message[256] = {};
                std::snprintf(message, sizeof(message),
                              "the Machine reached a grant boundary without consuming modeled "
                              "time (no_progress=%d waiting=%d)",
                              grant.facts.no_progress, grant.facts.waiting);
                stop_with_message(message, machine);
                return;
            }
        }
    }

    Options options_;
    ruscv_sim::RuscvInitiator& initiator_;
    ruscv_sim::RamTarget& ram_;
    std::thread::id creator_thread_;
    std::ofstream grant_trace_;
    bool terminal_ = false;
    bool stopped_early_ = false;
    bool failed_ = false;
    ruscv_result result_{};
    uint64_t grants_ = 0;
    uint64_t consumed_fs_ = 0;
    uint64_t host_elapsed_fs_ = 0;
    uint64_t transport_delay_fs_ = 0;
    /// Grants whose kernel clock matched `committed_end_fs` (quantum D12).
    uint64_t clock_checks_ = 0;
    /// Completed turns observed after the last grant.
    uint64_t turns_seen_ = 0;
};

}  // namespace

int sc_main(int argc, char** argv) {
    Options options;
    if (!parse_options(argc, argv, options)) {
        print_usage(argv[0]);
        return 2;
    }
    // The adapter copies an annotated delay verbatim, so the kernel clock and
    // its femtosecond mapping must agree exactly (dev-plan D7/D9).
    sc_core::sc_set_time_resolution(1, sc_core::SC_FS);
    const sc_core::sc_time resolution = sc_core::sc_get_time_resolution();
    if (resolution != sc_core::sc_time(1, sc_core::SC_FS)) {
        std::fprintf(stderr, "systemc-smoke: cannot establish a 1 fs time resolution\n");
        return 2;
    }

    std::printf("systemc-smoke: SystemC %s (IEEE 1666-%ld, SYSTEMC_VERSION %d), resolution %s\n",
                sc_core::sc_release(), IEEE_1666_SYSTEMC, SYSTEMC_VERSION,
                resolution.to_string().c_str());
    if (options.quantum == 0) {
        // A run with no quantum keeps the banner it printed before this unit: the
        // selection appears only when one is selected.
        std::printf("systemc-smoke: running %s under --isa %s\n", options.elf.c_str(),
                    isa_name(options.isa));
    } else {
        std::printf("systemc-smoke: running %s under --isa %s --quantum %" PRIu64 "\n",
                    options.elf.c_str(), isa_name(options.isa), options.quantum);
    }
    if (options.non_blocking) {
        std::printf("systemc-smoke: non-blocking --nb-path %s --nb-latency %" PRIu64 " fs\n",
                    nb_path_name(options.nb_path), options.nb_latency_fs);
    }
    std::fflush(stdout);

    ruscv_sim::RamTarget ram("ram",
                             ruscv_sim::TargetFaults{options.fault_decrease_delay,
                                                    options.fault_late_response,
                                                    options.fault_wait_blocking,
                                                    options.fault_wait_debug},
                             options.non_blocking, options.nb_latency_fs, options.nb_path);
    ruscv_sim::RuscvInitiator initiator(
        "initiator", ram, options.direct_mem, options.quantum,
        ruscv_sim::InitiatorFaults{options.fault_keep_offset, options.fault_uncounted_elapsed,
                                   options.fault_short_delay},
        options.non_blocking, options.nb_latency_fs);
    MachineDriver driver("driver", options, initiator, ram, std::this_thread::get_id());
    sc_core::sc_start();

    std::printf(
        "systemc-smoke: ram b_transport=%" PRIu64 " transport_dbg=%" PRIu64
        " transport_reset=%" PRIu64 " begin_req=%" PRIu64 " applied_accesses=%" PRIu64
        " direct_mem_invalidations=%" PRIu64 " direct_mem_scheduled=%" PRIu64 "\n",
        ram.b_transport_count(), ram.transport_dbg_count(), ram.transport_reset_count(),
        ram.begin_req_count(), ram.applied_access_count(),
        initiator.direct_mem_invalidation_count(), initiator.direct_mem_scheduled_count());
    std::printf(
        "systemc-smoke: AT late_phases=%" PRIu64 " retirements=%" PRIu64
        " drain_end_resp=%" PRIu64 " drain_end_resp_fs=%" PRIu64
        " open_transaction=%d\n",
        initiator.late_phase_count(), initiator.retirement_count(),
        initiator.drain_end_resp_count(), initiator.drain_end_resp_fs(),
        ram.has_open_transaction() ? 1 : 0);
    std::printf(
        "systemc-smoke: totals consumed_fs=%" PRIu64 " host_elapsed_fs=%" PRIu64
        " transport_delay_fs=%" PRIu64 "\n",
        driver.consumed_total_fs(), driver.host_elapsed_total_fs(),
        driver.transport_delay_fs());
    std::printf("systemc-smoke: kernel clock matched committed_end_fs after %" PRIu64
                " grant(s)\n",
                driver.clock_checks());
    std::fflush(stdout);

    if (initiator.direct_mem_error_count() != 0) {
        std::fprintf(stderr,
                     "systemc-smoke: %" PRIu64 " direct-memory invalidation(s) were refused\n",
                     initiator.direct_mem_error_count());
        return 1;
    }

    if (driver.stopped_early()) {
        std::fprintf(stderr,
                     "systemc-smoke: a host stop before a terminal ruscv-sim result is not a "
                     "run result\n");
        return 2;
    }
    if (!driver.terminal()) {
        std::fprintf(stderr, "systemc-smoke: the run ended without a terminal ruscv-sim result\n");
        return driver.failed() ? 1 : 2;
    }
    if (driver.result().state == RUSCV_RUN_EXITED) {
        std::printf("systemc-smoke: exit exit_code=%u\n", driver.result().exit_code);
        std::fflush(stdout);
        return static_cast<int>(driver.result().exit_code & 0xffu);
    }
    std::fprintf(stderr, "systemc-smoke: the run failed; it is not a passing result\n");
    return 1;
}
