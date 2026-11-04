// main.cpp - the SystemC host that drives one Machine through the ruscv-sim C ABI.
//
// dev-plan D9. The initiator is one bounded-grant loop on an `SC_THREAD`:
//
// * each grant starts at the current `sc_time_stamp()`, which is the kernel's
//   committed time and therefore the Machine's cursor;
// * the grant is bounded by a started-slot budget and an inclusive horizon;
// * after it the thread `wait()`s exactly the consumed amount the grant
//   returned, so the kernel commits global time only from that amount
//   (ADR-0004 §4.4);
// * `sc_stop()` runs only after `ruscv_machine_result` reports a terminal
//   result. A host stop before one is not a run result and exits non-zero: the
//   `--stop-after-grants` control proves that, and it must never pass.
//
// The C ABI handle is created and destroyed on this thread, which is the
// SystemC kernel thread, so the ABI's creator-thread affinity holds for every
// call and every target callback. The 1 fs time resolution the adapter's exact
// femtosecond mapping requires is established before the kernel starts.

#include <cinttypes>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <utility>

#include <ruscv_sim.h>
#include <systemc>

#include "ram_target.h"
#include "ruscv_initiator.h"

namespace {

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

/// One smoke run's configuration.
struct Options {
    std::string elf;
    int32_t isa = RUSCV_ISA_RV64IMAFDC;
    std::string commit_log;
    uint64_t turn_budget = kTurnBudget;
    uint64_t horizon_fs = kHorizonFs;
    /// Zero: only a terminal ruscv-sim result stops the run.
    uint64_t stop_after_grants = 0;
    uint64_t max_grants = kMaxGrants;
    uint64_t max_turns = kMaxTurns;
};

void print_usage(const char* program) {
    std::printf(
        "usage: %s <elf> [--isa rv64ima|rv64imac|rv64imafd|rv64imafdc]\n"
        "       [--log-commits <path>] [--turn-budget <turns>]\n"
        "       [--horizon-fs <femtoseconds>] [--stop-after-grants <grants>]\n"
        "       [--max-grants <grants>] [--max-turns <turns>]\n"
        "\n"
        "Runs one ruscv-sim Machine under this SystemC kernel. The RAM target is\n"
        "the zero-delay blocking model the initiator socket is bound to. Exit\n"
        "status is the guest exit code for a terminal run, 1 for a failed run,\n"
        "and 2 for a host stop before a terminal ruscv-sim result.\n"
        "\n"
        "--max-turns bounds completed turns, the counterpart of the public CLI's\n"
        "--max-cycles; exhausting it without a terminal result is a failure.\n",
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

/// The name of one run classification.
const char* state_name(int32_t state) {
    switch (state) {
        case RUSCV_RUN_RUNNING: return "running";
        case RUSCV_RUN_EXITED: return "exited";
        case RUSCV_RUN_FAILED: return "failed";
        default: return "unknown";
    }
}

/// Drives one Machine in bounded grants until it reports a terminal result.
class MachineDriver : public sc_core::sc_module {
public:
    MachineDriver(sc_core::sc_module_name name, Options options, ruscv_target target)
        : sc_core::sc_module(name), options_(std::move(options)), target_(target) {
        SC_THREAD(run);
    }

    /// True when `ruscv_machine_result` reported a terminal result.
    bool terminal() const { return terminal_; }
    /// True when the host stopped the run before a terminal result.
    bool stopped_early() const { return stopped_early_; }
    /// True when the calls or the Machine failed the run.
    bool failed() const { return failed_; }
    const ruscv_result& result() const { return result_; }

private:
    /// Reports one failed call, destroys the handle and stops the kernel.
    void stop_with_failure(const char* what, int32_t status, ruscv_machine* machine) {
        char diagnostic[512] = {};
        if (machine != nullptr) {
            ruscv_machine_diagnostic(machine, diagnostic, sizeof(diagnostic));
        }
        std::fprintf(stderr, "systemc-smoke: %s failed with status %d%s%s\n", what, status,
                     diagnostic[0] != '\0' ? ": " : "", diagnostic);
        failed_ = true;
        if (machine != nullptr) {
            ruscv_machine_destroy(machine);
        }
        sc_core::sc_stop();
    }

    /// Reports one host-level failure, destroys the handle and stops the kernel.
    void stop_with_message(const char* message, ruscv_machine* machine) {
        std::fprintf(stderr, "systemc-smoke: %s\n", message);
        failed_ = true;
        if (machine != nullptr) {
            ruscv_machine_destroy(machine);
        }
        sc_core::sc_stop();
    }

    void run() {
        ruscv_machine* machine = nullptr;
        int32_t status = ruscv_machine_create(&target_, options_.isa, kTickPeriodFs, &machine);
        if (status != RUSCV_OK) {
            stop_with_failure("ruscv_machine_create", status, nullptr);
            return;
        }
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
                ruscv_machine_destroy(machine);
                sc_core::sc_stop();
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
            if (grant.consumed_fs != 0) {
                // Commit global time only from the amount the grant returned.
                sc_core::wait(sc_core::sc_time::from_value(grant.consumed_fs));
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
            ruscv_result result{};
            status = ruscv_machine_result(machine, &result);
            if (status != RUSCV_OK) {
                stop_with_failure("ruscv_machine_result", status, machine);
                return;
            }
            turns_seen_ = result.turns;
            if (result.state != RUSCV_RUN_RUNNING) {
                terminal_ = true;
                result_ = result;
                std::printf(
                    "systemc-smoke: result state=%s exit_code=%u turns=%" PRIu64
                    " pc=0x%016" PRIx64 " grants=%" PRIu64 " consumed_fs=%" PRIu64 "\n",
                    state_name(result.state), result.exit_code, result.turns, result.pc, grants_,
                    consumed_fs_);
                std::fflush(stdout);
                ruscv_machine_destroy(machine);
                sc_core::sc_stop();
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
    ruscv_target target_;
    bool terminal_ = false;
    bool stopped_early_ = false;
    bool failed_ = false;
    ruscv_result result_{};
    uint64_t grants_ = 0;
    uint64_t consumed_fs_ = 0;
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
    std::printf("systemc-smoke: running %s under --isa %s\n", options.elf.c_str(),
                isa_name(options.isa));
    std::fflush(stdout);

    ruscv_sim::RamTarget ram("ram");
    ruscv_sim::RuscvInitiator initiator("initiator", ram);
    MachineDriver driver("driver", options, initiator.target_table());
    sc_core::sc_start();

    std::printf(
        "systemc-smoke: ram b_transport=%" PRIu64 " transport_dbg=%" PRIu64
        " transport_reset=%" PRIu64 "\n",
        ram.b_transport_count(), ram.transport_dbg_count(), ram.transport_reset_count());
    std::fflush(stdout);

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
