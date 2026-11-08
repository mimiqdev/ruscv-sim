#!/bin/bash
# Build and run the pinned-SystemC smoke (dev-plan D9/D10; direct-memory D13;
# quantum D13).
#
# The smoke proves the SystemC host path end to end on the same guests the
# contract names:
#
#   1. build the ruscv-sim C ABI static library (`--features capi`);
#   2. assemble the project-authored guests with the suite's own flags;
#   3. run `atomic_tohost_exit` and `fd_fma` through the public CLI, which is
#      the native baseline for the exit code and the commit log;
#   4. build `integration/systemc/` against the pinned SystemC 3.0.2 kernel and
#      run the same two guests through that kernel, the C ABI and the blocking
#      RAM target;
#   5. require the same exit code and a byte-identical commit log;
#   6. run both guests again with `--dmi`: the direct-memory pass must produce
#      the same facts while the RAM target's `b_transport` count stays strictly
#      below the same guest's non-direct pass, so a run that never uses a
#      granted pointer fails;
#   7. run one guest with `--dmi --dmi-invalidate-every 2`: the commit log must
#      stay unchanged, the scheduled invalidation count must equal grants/2,
#      and the ABI-forwarded count must cover it;
#   8. run the early-`sc_stop` control, which must not pass: a host stop before
#      a terminal ruscv-sim result is never a run result;
#   9. run both guests at `--quantum 1` and `--quantum 1024`, with and without
#      `--dmi`: the native exit code and a byte-identical commit log, a kernel
#      clock equal to `committed_end_fs` after every grant, and strictly fewer
#      grants at `--quantum 1024` than the same guest's `--quantum 1` pass;
#  10. run both guests under `--nb` on every return path at lambda = 0 and
#      lambda = 1500000 fs: the native exit code and commit log, a non-zero
#      BEGIN_REQ count equal to the applied-access count, one clock check per
#      grant, parity grant traces at lambda = 0 and the three D10 sums at
#      lambda > 0;
#  11. run the `--nb --dmi` passes at lambda = 0 on accepted and completed and
#      at lambda = 1500000 on accepted, where the AT count must be strictly
#      below the non-direct pass;
#  12. require the `--nb --quantum` refusal and the non-blocking usage errors;
#  13. run the two quantum fault-option controls under `--quantum 1024`, each of
#      which must fail the run with its own named diagnostic;
#  14. run the five non-blocking fault-option controls in their stated mode
#      (nb §4): the late response on the accepted, early and completed paths,
#      with the withheld reset acknowledgement, the drain's retained
#      `END_RESP` and a counted late phase; the uncounted elapsed time as a
#      kernel-clock mismatch; and the short delay, the blocking wait and the
#      debug wait with their own named diagnostics. A normal `--nb` pass must
#      report no late phase, and each option is a usage error in the wrong
#      mode.
#  15. run the device U4 passes (device D9): `hello` under `--uart-target
#      in-tree` and `--uart-target host` at the baseline, `--dmi`, `--nb` and
#      `--quantum 1024` selections, where the in-tree pass reports zero host
#      device transactions and the host pass transmits exactly `Hello!\n` with
#      the same architectural facts as its in-tree parity run; a second host
#      run must produce the same bytes; the two existing guests under
#      `--uart-target host` must keep their facts with zero device
#      transactions; and the three device fault options
#      (`--fault-uart-no-lsr`, `--fault-uart-wait`,
#      `--fault-uart-event-on-fault`) must each fail the run with their own
#      assertion.
#  16. run the HTIF U3 passes (htif D8): `htif_u3_console` under
#      `--htif-target in-tree` and `--htif-target host` at the baseline,
#      `--dmi`, `--nb` and `--quantum 1024` selections, where the in-tree pass
#      reports zero host HTIF transactions and the host pass publishes the
#      guest's three console words and exit word, renders exactly `Hi\n` and
#      keeps the same grant and consumed facts as its in-tree parity run; a
#      second host run must report the same facts; the two existing guests
#      under `--htif-target host` must keep their facts with zero HTIF
#      transactions; and the three HTIF fault options
#      (`--fault-htif-drop-event`, `--fault-htif-wait`,
#      `--fault-htif-event-on-fault`) must each fail the run with their own
#      assertion.
#
# Linux x86-64 only. Prerequisites are the pinned SystemC installation, CMake,
# a C++17 compiler and the RISC-V cross toolchain. A missing prerequisite is a
# failure, never a skip: the CI job that runs this script is the acceptance
# evidence for the unit, so an absent tool means the check did not run.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd -P)"

CMAKECMD="${CMAKE:-cmake}"
CXXCMD="${CXX:-c++}"
SYSTEMC_ROOT="${SYSTEMC_HOME:-${SYSTEMC_ROOT:-/opt/systemc}}"
OUTDIR="${PROJECT_DIR}/target/systemc-smoke"
# The guest suite validates its output directory, so it must sit directly
# below the repository target directory.
ELF_DIR="${PROJECT_DIR}/target/systemc-smoke-elves"
MAX_CYCLES="${SYSTEMC_SMOKE_MAX_CYCLES:-100000}"
# Every guest run is bounded twice: the smoke's own turn limit (the C ABI
# counterpart of the CLI's --max-cycles) and this wall-clock timeout, so neither a
# guest loop nor a host defect can hang CI.
RUN_TIMEOUT="${SYSTEMC_SMOKE_TIMEOUT:-300}"

# `atomic_tohost_exit` and `fd_fma` under their declared profiles (dev-plan
# D10): the suite leaves the atomic guest on the default rv64imafdc profile and
# binds the F/D guest to rv64imafd. The profile is passed explicitly here so the
# comparison does not depend on a default that may be re-selected.
GUESTS=(
    "atomic_tohost_exit rv64imafdc rv64a"
    "fd_fma rv64imafd rv64ifd"
)
# One started slot cannot reach any guest's terminal path, so the early stop is
# genuinely before a run result and not an accident of a long grant.
EARLY_STOP_TURNS=1

fail() {
    echo -e "${RED}error: $*${NC}" >&2
    exit 2
}

# capture OUTPUT_VARIABLE STATUS_VARIABLE COMMAND...
#   Runs COMMAND, captures its combined output and exit status without
#   tripping `set -e` for an expected failure.
capture() {
    local output_variable="$1"
    local status_variable="$2"
    shift 2
    local output
    local status
    set +e
    output="$("$@" 2>&1)"
    status=$?
    set -e
    printf -v "${output_variable}" '%s' "${output}"
    printf -v "${status_variable}" '%s' "${status}"
}

# The CLI's exit-code line, or an empty string.
cli_exit_code() {
    printf '%s\n' "$1" | sed -n 's/^Exit Code:  *\([0-9][0-9]*\)$/\1/p' | head -1
}

# The smoke's own exit-code line, or an empty string.
smoke_exit_code() {
    printf '%s\n' "$1" | sed -n 's/^systemc-smoke: exit exit_code=\([0-9][0-9]*\)$/\1/p' | head -1
}

# The blocking-transaction count the RAM target served, or an empty string.
ram_b_transport_count() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: ram b_transport=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# The Machine grant count of the terminal run, or an empty string.
smoke_grant_count() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: result state=.* grants=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# The completed-turn count of the terminal run, or an empty string. A terminal
# exit stops on its own turn, so equal counts prove both selections published
# the exit value at the same instruction (htif D8).
smoke_turn_count() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: result state=.* turns=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# The direct-memory invalidation count the initiator forwarded, or an empty
# string.
ram_direct_mem_invalidations() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: ram .* direct_mem_invalidations=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# The number of invalidations the driver scheduled, or an empty string.
ram_direct_mem_scheduled() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: ram .* direct_mem_scheduled=\([0-9][0-9]*\)$/\1/p' | head -1
}

# The number of grants whose kernel clock equalled `committed_end_fs`, or an
# empty string.
smoke_clock_checks() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: kernel clock matched committed_end_fs after \([0-9][0-9]*\) grant(s)$/\1/p' | head -1
}

# The AT BEGIN_REQ count the RAM target received, or an empty string (nb D8).
ram_begin_req_count() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: ram .* begin_req=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# The AT applied-access count the RAM target reported, or an empty string.
ram_applied_access_count() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: ram .* applied_accesses=\([0-9][0-9]*\) .*$/\1/p' | head -1
}

# One field of the host's exit totals line, or an empty string (nb D10).
smoke_totals_field() {
    printf '%s\n' "$1" \
        | awk -v key="$2" '/^systemc-smoke: totals / { for (i = 1; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) { print pair[2]; exit } } }'
}

# One field of the host's AT retirement line, or an empty string (nb D9).
smoke_at_field() {
    printf '%s\n' "$1" \
        | awk -v key="$2" '/^systemc-smoke: AT / { for (i = 1; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) { print pair[2]; exit } } }'
}

# One field of the host UART target's summary line, or an empty string
# (device D9).
uart_field() {
    printf '%s\n' "$1" \
        | awk -v key="$2" '/^systemc-smoke: uart / { for (i = 1; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) { print pair[2]; exit } } }'
}

# The byte sequence the host UART target echoed to this host's console, in its
# escaped form, or an empty string (device D9). This is the target's own console
# channel, so it observes a transmitted byte without reading its counters.
uart_console_sequence() {
    printf '%s\n' "$1" | sed -n 's/^uart-console: \(.*\)$/\1/p' | tr -d '\n'
}

# One field of the host HTIF target's summary line, or an empty string
# (htif D8).
htif_field() {
    printf '%s\n' "$1" \
        | awk -v key="$2" '/^systemc-smoke: htif / { for (i = 1; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) { print pair[2]; exit } } }'
}

# The accepted HTIF value sequence, one hexadecimal word per write, joined by
# single spaces (htif D8).
htif_value_sequence() {
    printf '%s\n' "$1" \
        | sed -n 's/^htif-value: \(0x[0-9a-f][0-9a-f]*\)$/\1/p' | tr '\n' ' ' | sed 's/ *$//'
}

# The byte sequence the host HTIF target rendered from the standard console
# device, in its escaped form (htif D8).
htif_console_sequence() {
    printf '%s\n' "$1" | sed -n 's/^htif-console: \(.*\)$/\1/p' | tr -d '\n'
}

# The status the host's reset probe received while an AT transaction was still
# open, or an empty string when the host skipped the probe (nb D9).
smoke_reset_status() {
    printf '%s\n' "$1" \
        | sed -n 's/^systemc-smoke: reset with an open AT transaction returned status \(-*[0-9][0-9]*\)$/\1/p' | head -1
}

# Sums one field of a --grant-trace file.
trace_sum() {
    awk -v key="$2" '{ for (i = 1; i <= NF; i++) { split($i, pair, "="); if (pair[1] == key) sum += pair[2] } } END { printf "%.0f\n", sum }' "$1"
}

# Checks one grant trace: every line, host elapsed <= consumed, and each
# committed end equals the previous end plus the consumed amount (nb D10).
trace_check() {
    awk '
        function field(name, i, pair) {
            for (i = 1; i <= NF; i++) {
                split($i, pair, "=")
                if (pair[1] == name) return pair[2]
            }
            return ""
        }
        $1 == "grant" {
            consumed = field("consumed_fs")
            ended = field("committed_end_fs")
            host = field("host_elapsed_fs")
            if (consumed == "" || ended == "" || host == "") exit 1
            if (host + 0 > consumed + 0) exit 2
            if (ended + 0 != previous + consumed + 0) exit 3
            previous = ended + 0
            lines++
        }
        END { if (lines == 0) exit 4 }
    ' "$1"
}

# Sets the LAST_* variables from one successful --nb pass and checks the
# common U3 observables: native exit code, byte-identical commit log, a
# non-zero BEGIN_REQ count equal to the applied count, and one clock check per
# grant.
require_nb_pass() {
    local guest="$1"
    local label="$2"
    local output="$3"
    local status="$4"
    local expected_exit="$5"
    local log="$6"
    local native_log="$7"

    [ "${status}" -ne 124 ] || fail "${guest}: the ${label} run timed out after ${RUN_TIMEOUT}s"
    if [ "${status}" -ne 0 ]; then
        printf '%s\n' "${output}"
        fail "${guest}: the ${label} run exited with status ${status}"
    fi
    if ! printf '%s\n' "${output}" | grep -q '^systemc-smoke: non-blocking --nb-path '; then
        printf '%s\n' "${output}"
        fail "${guest}: the ${label} run did not report its non-blocking selection"
    fi
    if ! printf '%s\n' "${output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${output}"
        fail "${guest}: the ${label} run did not report a terminal ruscv-sim result"
    fi
    LAST_EXIT="$(smoke_exit_code "${output}")"
    [ -n "${LAST_EXIT}" ] || fail "${guest}: the ${label} run reported no exit code"
    [ "${LAST_EXIT}" = "${expected_exit}" ] \
        || fail "${guest}: the ${label} exit code ${LAST_EXIT} differs from native ${expected_exit}"
    [ -s "${log}" ] || fail "${guest}: the ${label} commit log is empty"
    if ! cmp -s "${native_log}" "${log}"; then
        diff -u "${native_log}" "${log}" | head -20 || true
        fail "${guest}: the ${label} commit log differs from native"
    fi
    LAST_GRANTS="$(smoke_grant_count "${output}")"
    LAST_CHECKS="$(smoke_clock_checks "${output}")"
    LAST_BEGIN="$(ram_begin_req_count "${output}")"
    LAST_APPLIED="$(ram_applied_access_count "${output}")"
    [ -n "${LAST_GRANTS}" ] || fail "${guest}: the ${label} run reported no grant count"
    [ -n "${LAST_CHECKS}" ] || fail "${guest}: the ${label} run reported no clock-check count"
    [ -n "${LAST_BEGIN}" ] || fail "${guest}: the ${label} run reported no BEGIN_REQ count"
    [ -n "${LAST_APPLIED}" ] || fail "${guest}: the ${label} run reported no applied-access count"
    [ "${LAST_BEGIN}" -gt 0 ] \
        || fail "${guest}: the ${label} run reported no BEGIN_REQ phase"
    [ "${LAST_BEGIN}" -eq "${LAST_APPLIED}" ] \
        || fail "${guest}: the ${label} run received ${LAST_BEGIN} BEGIN_REQ phase(s) but applied ${LAST_APPLIED} access(es)"
    [ "${LAST_CHECKS}" -eq "${LAST_GRANTS}" ] \
        || fail "${guest}: the ${label} run checked the kernel clock after ${LAST_CHECKS} of its ${LAST_GRANTS} grant(s)"
    LAST_CONSUMED="$(smoke_totals_field "${output}" consumed_fs)"
    LAST_HOST="$(smoke_totals_field "${output}" host_elapsed_fs)"
    LAST_DELAY="$(smoke_totals_field "${output}" transport_delay_fs)"
    [ -n "${LAST_CONSUMED}" ] || fail "${guest}: the ${label} run reported no consumed total"
    [ -n "${LAST_HOST}" ] || fail "${guest}: the ${label} run reported no host elapsed total"
    [ -n "${LAST_DELAY}" ] || fail "${guest}: the ${label} run reported no transport delay total"
    # nb D9: a normal pass completes every transaction in time, so it ignores no
    # late phase.
    LAST_LATE="$(smoke_at_field "${output}" late_phases)"
    [ -n "${LAST_LATE}" ] || fail "${guest}: the ${label} run reported no late-phase count"
    [ "${LAST_LATE}" -eq 0 ] \
        || fail "${guest}: the ${label} run ignored ${LAST_LATE} late phase(s); a passing pass reports zero"
}

echo -e "${BLUE}== checking the SystemC smoke prerequisites${NC}"
for tool in "${CMAKECMD}" "${CXXCMD}" "${RISCV_PREFIX:-riscv64-unknown-elf-}as" \
    "${RISCV_PREFIX:-riscv64-unknown-elf-}ld"; do
    command -v "${tool}" >/dev/null 2>&1 \
        || fail "required tool not found: ${tool}"
done
[ -d "${SYSTEMC_ROOT}" ] \
    || fail "no SystemC installation at ${SYSTEMC_ROOT}; set SYSTEMC_HOME to the pinned SystemC 3.0.2 prefix"
[ -d "${SYSTEMC_ROOT}/include/sysc" ] \
    || fail "${SYSTEMC_ROOT} does not look like a SystemC installation (no include/sysc)"

echo "  SystemC prefix: ${SYSTEMC_ROOT}"
echo "  C++ compiler:   $("${CXXCMD}" --version | head -1)"
echo "  CMake:          $("${CMAKECMD}" --version | head -1)"

case "${CARGO_TARGET_DIR:-${PROJECT_DIR}/target}" in
    /*) CARGO_TARGET_ABS="${CARGO_TARGET_DIR:-${PROJECT_DIR}/target}" ;;
    *) CARGO_TARGET_ABS="${PROJECT_DIR}/${CARGO_TARGET_DIR:-target}" ;;
esac
LIBRARY="${CARGO_TARGET_ABS}/debug/libruscv_sim.a"
RUSCV_BIN="${CARGO_TARGET_ABS}/release/ruscv-sim"

rm -rf "${OUTDIR}"
mkdir -p "${OUTDIR}/logs"
BUILD_DIR="${OUTDIR}/build"

echo -e "${BLUE}== building the ruscv-sim C ABI static library${NC}"
(
    cd "${PROJECT_DIR}"
    cargo rustc --lib --features capi --crate-type staticlib
)
[ -f "${LIBRARY}" ] || fail "expected static library not found at ${LIBRARY}"

echo -e "${BLUE}== assembling the guest suite${NC}"
if ! RISCV_TEST_OUTDIR="${ELF_DIR}" "${SCRIPT_DIR}/compile_riscv_tests.sh"; then
    fail "the guest suite did not assemble"
fi

echo -e "${BLUE}== building the native ruscv-sim binary${NC}"
(
    cd "${PROJECT_DIR}"
    CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
        cargo build --release --all-features --target-dir "${CARGO_TARGET_ABS}"
)
[ -x "${RUSCV_BIN}" ] || fail "simulator binary not found: ${RUSCV_BIN}"

echo -e "${BLUE}== building the SystemC host${NC}"
"${CMAKECMD}" -S "${PROJECT_DIR}/integration/systemc" -B "${BUILD_DIR}" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_PREFIX_PATH="${SYSTEMC_ROOT}" \
    -DRUSCV_SIM_LIBRARY="${LIBRARY}"
"${CMAKECMD}" --build "${BUILD_DIR}" --parallel "$(nproc)"
SMOKE_BIN="${BUILD_DIR}/systemc_smoke"
[ -x "${SMOKE_BIN}" ] || fail "SystemC smoke binary not found at ${SMOKE_BIN}"

# The number of scheduled invalidations for `--dmi-invalidate-every`: the
# driver invalidates every grant after each Nth grant, so the forwarded count
# is grants/N.
INVALIDATE_EVERY=2

# The two quantum selections every guest runs under (quantum D13/U5): `1` syncs
# at every turn boundary, so a grant ends after one turn, and `1024` lets a
# RAM-only stretch commit once per quantum. The fault-option controls run under
# the larger one, where a grant spans several turns.
QUANTUMS=(1 1024)
FAULT_QUANTUM=1024

# The AT latency every non-blocking pass uses (nb D10): 1.5 Platform ticks, so
# every AT access exercises the round-up and the grant-end wait.
NB_LATENCY=1500000
NB_PATHS=(accepted updated early annotated completed)

# Each guest's native exit code, for the later pass comparisons.
declare -A NATIVE_EXITS=()
# The parity traces and values the non-blocking passes compare against (nb D10).
declare -A PARITY_TRACES=()
declare -A DIRECT_TRACES=()
declare -A PARITY_CONSUMED=()
declare -A NB_NON_DIRECT_BEGIN=()

compared=0
for entry in "${GUESTS[@]}"; do
    read -r guest profile directory <<<"${entry}"
    elf="${ELF_DIR}/${directory}/${guest}.elf"
    [ -f "${elf}" ] || fail "guest not found: ${elf}"
    echo -e "${YELLOW}== ${guest} under --isa ${profile}${NC}"

    native_log="${OUTDIR}/logs/${guest}.native.log"
    capture native_output native_status \
        timeout "${RUN_TIMEOUT}" "${RUSCV_BIN}" run "${elf}" --isa "${profile}" \
        --max-cycles "${MAX_CYCLES}" --log-commits "${native_log}"
    [ "${native_status}" -ne 124 ] \
        || fail "${guest}: the native run timed out after ${RUN_TIMEOUT}s"
    native_exit="$(cli_exit_code "${native_output}")"
    if [ "${native_status}" -ne 0 ] || [ -z "${native_exit}" ]; then
        printf '%s\n' "${native_output}"
        fail "${guest}: the native run did not report an exit code"
    fi
    if ! printf '%s\n' "${native_output}" | grep -q '^Status:     SUCCESS$'; then
        printf '%s\n' "${native_output}"
        fail "${guest}: the native run did not succeed"
    fi
    [ -s "${native_log}" ] || fail "${guest}: the native commit log is empty"
    echo "  native: exit status ${native_status}, guest exit code ${native_exit}"
    NATIVE_EXITS[${guest}]="${native_exit}"

    smoke_log="${OUTDIR}/logs/${guest}.systemc.log"
    smoke_trace="${OUTDIR}/logs/${guest}.systemc.trace"
    capture smoke_output smoke_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${smoke_log}" \
        --grant-trace "${smoke_trace}"
    [ "${smoke_status}" -ne 124 ] \
        || fail "${guest}: the SystemC run timed out after ${RUN_TIMEOUT}s"
    smoke_exit="$(smoke_exit_code "${smoke_output}")"
    transactions="$(ram_b_transport_count "${smoke_output}")"
    if [ "${smoke_status}" -ne 0 ]; then
        printf '%s\n' "${smoke_output}"
        fail "${guest}: the SystemC run exited with status ${smoke_status}"
    fi
    if ! printf '%s\n' "${smoke_output}" | grep -q "^systemc-smoke: SystemC 3\.0\.2"; then
        printf '%s\n' "${smoke_output}"
        fail "${guest}: the run did not use a SystemC 3.0.2 kernel"
    fi
    if ! printf '%s\n' "${smoke_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${smoke_output}"
        fail "${guest}: the SystemC run did not report a terminal ruscv-sim result"
    fi
    [ -n "${smoke_exit}" ] || fail "${guest}: the SystemC run reported no exit code"
    [ -n "${transactions}" ] || fail "${guest}: the RAM target reported no transaction count"
    if [ "${transactions}" -eq 0 ]; then
        fail "${guest}: the RAM target served no blocking transaction"
    fi
    # device D9: with no --uart-target the in-tree Rust device serves the UART
    # window, so a guest that never selects a host target must report zero host
    # device transactions.
    if ! printf '%s\n' "${smoke_output}" | grep -q '^systemc-smoke: uart target=in-tree$'; then
        printf '%s\n' "${smoke_output}"
        fail "${guest}: the default UART target is not the in-tree device"
    fi
    uart_transactions="$(uart_field "${smoke_output}" b_transport)"
    [ -n "${uart_transactions}" ] \
        || fail "${guest}: the run reported no host UART device transaction count"
    if [ "${uart_transactions}" -ne 0 ]; then
        printf '%s\n' "${smoke_output}"
        fail "${guest}: the in-tree selection reported ${uart_transactions} host device transaction(s); it must report zero"
    fi
    echo "  systemc: exit status ${smoke_status}, guest exit code ${smoke_exit}, b_transport ${transactions}"

    if [ "${smoke_exit}" != "${native_exit}" ]; then
        fail "${guest}: SystemC exit code ${smoke_exit} differs from native ${native_exit}"
    fi
    [ -s "${smoke_log}" ] || fail "${guest}: the SystemC commit log is empty"
    if ! cmp -s "${native_log}" "${smoke_log}"; then
        diff -u "${native_log}" "${smoke_log}" | head -20 || true
        fail "${guest}: the SystemC commit log differs from native"
    fi
    trace_check "${smoke_trace}" \
        || fail "${guest}: the parity grant trace is not well formed"
    PARITY_TRACES[${guest}]="${smoke_trace}"
    PARITY_CONSUMED[${guest}]="$(trace_sum "${smoke_trace}" consumed_fs)"
    compared=$((compared + 1))
    echo -e "  ${GREEN}[PASS]${NC} exit code and commit log match native"

    # The direct-memory pass (direct-memory D13/U5): the same guest runs with
    # the table's direct-memory query offered, so hinted accesses copy through
    # the target's granted chunk pointers instead of `b_transport`. The exit
    # code and commit log must match native exactly, and the RAM target's
    # `b_transport` count must stay strictly below the non-direct pass: a run
    # that never uses a granted pointer fails here.
    direct_log="${OUTDIR}/logs/${guest}.systemc.dmi.log"
    direct_trace="${OUTDIR}/logs/${guest}.systemc.dmi.trace"
    capture direct_output direct_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${direct_log}" --dmi \
        --grant-trace "${direct_trace}"
    [ "${direct_status}" -ne 124 ] \
        || fail "${guest}: the direct-memory run timed out after ${RUN_TIMEOUT}s"
    direct_exit="$(smoke_exit_code "${direct_output}")"
    direct_transactions="$(ram_b_transport_count "${direct_output}")"
    if [ "${direct_status}" -ne 0 ]; then
        printf '%s\n' "${direct_output}"
        fail "${guest}: the direct-memory run exited with status ${direct_status}"
    fi
    if ! printf '%s\n' "${direct_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${direct_output}"
        fail "${guest}: the direct-memory run did not report a terminal ruscv-sim result"
    fi
    [ -n "${direct_exit}" ] || fail "${guest}: the direct-memory run reported no exit code"
    [ -n "${direct_transactions}" ] \
        || fail "${guest}: the direct-memory run reported no transaction count"
    if [ "${direct_exit}" != "${native_exit}" ]; then
        printf '%s\n' "${direct_output}"
        fail "${guest}: direct-memory exit code ${direct_exit} differs from native ${native_exit}"
    fi
    [ -s "${direct_log}" ] || fail "${guest}: the direct-memory commit log is empty"
    if ! cmp -s "${native_log}" "${direct_log}"; then
        diff -u "${native_log}" "${direct_log}" | head -20 || true
        fail "${guest}: the direct-memory commit log differs from native"
    fi
    if [ "${direct_transactions}" -ge "${transactions}" ]; then
        printf '%s\n' "${direct_output}"
        fail "${guest}: direct memory served ${direct_transactions} b_transport call(s), not strictly below the non-direct pass's ${transactions}; no granted pointer was ever used"
    fi
    trace_check "${direct_trace}" \
        || fail "${guest}: the direct-memory parity grant trace is not well formed"
    DIRECT_TRACES[${guest}]="${direct_trace}"
    echo -e "  ${GREEN}[PASS]${NC} --dmi matches native with b_transport ${direct_transactions} < ${transactions}"

    # The quantum passes (quantum D13/U5). Both guests run at `--quantum 1` and
    # `--quantum 1024`, with and without `--dmi`: a quantum defers only when the
    # Machine commits the local offset, so every pass keeps the native exit code
    # and a byte-identical commit log. The binary compares the kernel clock with
    # `committed_end_fs` after every grant and fails the run on a mismatch, and
    # the grant and clock-check counts must agree, so a passing run proves every
    # grant was checked. `--quantum 1` syncs at every turn boundary and a counted
    # sync ends its grant, so the same guest must take strictly more grants there
    # than at `--quantum 1024`.
    for direct in 0 1; do
        direct_flag=()
        direct_suffix=""
        if [ "${direct}" -eq 1 ]; then
            direct_flag=(--dmi)
            direct_suffix=".dmi"
            direct_label=" --dmi"
        else
            direct_label=""
        fi
        grants_by_quantum=()
        for quantum in "${QUANTUMS[@]}"; do
            quantum_log="${OUTDIR}/logs/${guest}.systemc.q${quantum}${direct_suffix}.log"
            capture quantum_output quantum_status \
                timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
                --max-turns "${MAX_CYCLES}" --log-commits "${quantum_log}" \
                --quantum "${quantum}" ${direct_flag[@]+"${direct_flag[@]}"}
            [ "${quantum_status}" -ne 124 ] \
                || fail "${guest}: the --quantum ${quantum}${direct_label} run timed out after ${RUN_TIMEOUT}s"
            quantum_exit="$(smoke_exit_code "${quantum_output}")"
            quantum_grants="$(smoke_grant_count "${quantum_output}")"
            quantum_checks="$(smoke_clock_checks "${quantum_output}")"
            if [ "${quantum_status}" -ne 0 ]; then
                printf '%s\n' "${quantum_output}"
                fail "${guest}: the --quantum ${quantum}${direct_label} run exited with status ${quantum_status}"
            fi
            if ! printf '%s\n' "${quantum_output}" | grep -q '^systemc-smoke: result state=exited '; then
                printf '%s\n' "${quantum_output}"
                fail "${guest}: the --quantum ${quantum}${direct_label} run did not report a terminal ruscv-sim result"
            fi
            if ! printf '%s\n' "${quantum_output}" \
                | grep -q "^systemc-smoke: running .* --quantum ${quantum}$"; then
                printf '%s\n' "${quantum_output}"
                fail "${guest}: the --quantum ${quantum}${direct_label} run did not report its quantum selection"
            fi
            [ -n "${quantum_exit}" ] \
                || fail "${guest}: the --quantum ${quantum}${direct_label} run reported no exit code"
            [ -n "${quantum_grants}" ] \
                || fail "${guest}: the --quantum ${quantum}${direct_label} run reported no grant count"
            [ -n "${quantum_checks}" ] \
                || fail "${guest}: the --quantum ${quantum}${direct_label} run reported no clock-check count"
            if [ "${quantum_checks}" -ne "${quantum_grants}" ]; then
                printf '%s\n' "${quantum_output}"
                fail "${guest}: the --quantum ${quantum}${direct_label} run checked the kernel clock after ${quantum_checks} of its ${quantum_grants} grant(s)"
            fi
            if [ "${quantum_exit}" != "${native_exit}" ]; then
                printf '%s\n' "${quantum_output}"
                fail "${guest}: the --quantum ${quantum}${direct_label} exit code ${quantum_exit} differs from native ${native_exit}"
            fi
            [ -s "${quantum_log}" ] \
                || fail "${guest}: the --quantum ${quantum}${direct_label} commit log is empty"
            if ! cmp -s "${native_log}" "${quantum_log}"; then
                diff -u "${native_log}" "${quantum_log}" | head -20 || true
                fail "${guest}: the --quantum ${quantum}${direct_label} commit log differs from native"
            fi
            grants_by_quantum+=("${quantum_grants}")
            echo "  quantum ${quantum}${direct_label}: exit ${quantum_exit}, grants ${quantum_grants}, clock checks ${quantum_checks}"
        done
        if [ "${grants_by_quantum[1]}" -ge "${grants_by_quantum[0]}" ]; then
            fail "${guest}: --quantum 1024${direct_label} took ${grants_by_quantum[1]} grant(s), not strictly below --quantum 1's ${grants_by_quantum[0]}"
        fi
        echo -e "  ${GREEN}[PASS]${NC} --quantum 1024${direct_label} took ${grants_by_quantum[1]} grant(s) < --quantum 1's ${grants_by_quantum[0]} with the same commit log"
    done

    # The non-blocking AT passes (nb D7/D8/D10). Both guests run every return
    # path at lambda = 0 and lambda = 1500000 fs. At lambda = 0 the grant trace
    # must equal the parity trace byte for byte, so the same consumed and
    # committed-end amounts and grant count hold at every grant and the host
    # elapsed total is zero. At lambda > 0 the three D10 sums must hold from the
    # host's exit totals, and every grant trace line must keep host elapsed
    # below the consumed amount.
    for latency in 0 "${NB_LATENCY}"; do
        for path in "${NB_PATHS[@]}"; do
            label="--nb --nb-path ${path} --nb-latency ${latency}"
            nb_log="${OUTDIR}/logs/${guest}.systemc.nb.${path}.${latency}.log"
            nb_trace="${OUTDIR}/logs/${guest}.systemc.nb.${path}.${latency}.trace"
            capture nb_output nb_status \
                timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
                --max-turns "${MAX_CYCLES}" --log-commits "${nb_log}" \
                --nb --nb-latency "${latency}" --nb-path "${path}" \
                --grant-trace "${nb_trace}"
            require_nb_pass "${guest}" "${label}" "${nb_output}" "${nb_status}" \
                "${native_exit}" "${nb_log}" "${native_log}"
            trace_check "${nb_trace}" \
                || fail "${guest}: the ${label} grant trace is not well formed"
            if [ "${path}" = "accepted" ] && [ "${latency}" -eq 0 ]; then
                NB_NON_DIRECT_BEGIN[${guest}]="${LAST_BEGIN}"
            fi
            if [ "${latency}" -eq 0 ]; then
                [ "${LAST_HOST}" -eq 0 ] \
                    || fail "${guest}: the ${label} run reported host elapsed ${LAST_HOST} fs, not zero"
                [ "${LAST_DELAY}" -eq 0 ] \
                    || fail "${guest}: the ${label} run reported transport delay ${LAST_DELAY} fs, not zero"
                if ! cmp -s "${PARITY_TRACES[${guest}]}" "${nb_trace}"; then
                    diff -u "${PARITY_TRACES[${guest}]}" "${nb_trace}" | head -20 || true
                    fail "${guest}: the ${label} grant trace differs from the parity run"
                fi
                echo -e "  ${GREEN}[PASS]${NC} ${label}: ${LAST_BEGIN} BEGIN_REQ phase(s), trace equals parity"
            else
                expected_host=$((LAST_BEGIN * latency))
                expected_delay=$((LAST_BEGIN * latency))
                expected_consumed=$((PARITY_CONSUMED[${guest}] + LAST_BEGIN * 2 * 1000000))
                [ "${LAST_HOST}" -eq "${expected_host}" ] \
                    || fail "${guest}: the ${label} host elapsed sum ${LAST_HOST} is not ${expected_host}"
                [ "${LAST_DELAY}" -eq "${expected_delay}" ] \
                    || fail "${guest}: the ${label} transport delay ${LAST_DELAY} is not ${expected_delay}"
                [ "${LAST_CONSUMED}" -eq "${expected_consumed}" ] \
                    || fail "${guest}: the ${label} consumed sum ${LAST_CONSUMED} is not parity plus 2 ticks per access (${expected_consumed})"
                [ "$(trace_sum "${nb_trace}" host_elapsed_fs)" -eq "${LAST_HOST}" ] \
                    || fail "${guest}: the ${label} trace host elapsed sum disagrees with the exit total"
                [ "$(trace_sum "${nb_trace}" consumed_fs)" -eq "${LAST_CONSUMED}" ] \
                    || fail "${guest}: the ${label} trace consumed sum disagrees with the exit total"
                echo -e "  ${GREEN}[PASS]${NC} ${label}: ${LAST_BEGIN} BEGIN_REQ phase(s), host ${LAST_HOST} fs, delay ${LAST_DELAY} fs, consumed ${LAST_CONSUMED} fs"
            fi
        done
    done

    # nb D7/D10: the direct-memory AT passes at lambda = 0 must equal the --dmi
    # parity trace. The lambda > 0 accepted pass must serve strictly fewer AT
    # transactions than the same guest's non-direct --nb pass, because granted
    # pointers bypass the target callback.
    for path in accepted completed; do
        label="--nb --dmi --nb-path ${path} --nb-latency 0"
        nb_log="${OUTDIR}/logs/${guest}.systemc.nb.dmi.${path}.log"
        nb_trace="${OUTDIR}/logs/${guest}.systemc.nb.dmi.${path}.trace"
        capture nb_output nb_status \
            timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
            --max-turns "${MAX_CYCLES}" --log-commits "${nb_log}" --dmi \
            --nb --nb-path "${path}" --grant-trace "${nb_trace}"
        require_nb_pass "${guest}" "${label}" "${nb_output}" "${nb_status}" \
            "${native_exit}" "${nb_log}" "${native_log}"
        trace_check "${nb_trace}" \
            || fail "${guest}: the ${label} grant trace is not well formed"
        if ! cmp -s "${DIRECT_TRACES[${guest}]}" "${nb_trace}"; then
            diff -u "${DIRECT_TRACES[${guest}]}" "${nb_trace}" | head -20 || true
            fail "${guest}: the ${label} grant trace differs from the --dmi parity run"
        fi
        echo -e "  ${GREEN}[PASS]${NC} ${label}: ${LAST_BEGIN} BEGIN_REQ phase(s), trace equals the --dmi parity run"
    done

    label="--nb --dmi --nb-path accepted --nb-latency ${NB_LATENCY}"
    nb_log="${OUTDIR}/logs/${guest}.systemc.nb.dmi.accepted.${NB_LATENCY}.log"
    nb_trace="${OUTDIR}/logs/${guest}.systemc.nb.dmi.accepted.${NB_LATENCY}.trace"
    capture nb_output nb_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${nb_log}" --dmi \
        --nb --nb-path accepted --nb-latency "${NB_LATENCY}" \
        --grant-trace "${nb_trace}"
    require_nb_pass "${guest}" "${label}" "${nb_output}" "${nb_status}" \
        "${native_exit}" "${nb_log}" "${native_log}"
    trace_check "${nb_trace}" \
        || fail "${guest}: the ${label} grant trace is not well formed"
    [ "${LAST_BEGIN}" -lt "${NB_NON_DIRECT_BEGIN[${guest}]}" ] \
        || fail "${guest}: the ${label} served ${LAST_BEGIN} BEGIN_REQ phase(s), not strictly below the non-direct ${NB_NON_DIRECT_BEGIN[${guest}]}"
    expected_delay=$((LAST_BEGIN * NB_LATENCY))
    [ "${LAST_DELAY}" -eq "${expected_delay}" ] \
        || fail "${guest}: the ${label} transport delay ${LAST_DELAY} is not ${expected_delay}"
    echo -e "  ${GREEN}[PASS]${NC} ${label}: ${LAST_BEGIN} BEGIN_REQ phase(s) < non-direct ${NB_NON_DIRECT_BEGIN[${guest}]}"
done

# The scheduled-invalidation pass (direct-memory D8/U5): every grant is
# invalidated after each second Machine grant through the target's backward
# call and `ruscv_machine_invalidate_direct_mem`, so the next access re-queries
# and re-grants. The commit log must stay unchanged, which is the contract's
# "same facts after invalidation" rule. The scheduled count must equal
# grants/N exactly, and the forwarded count must be at least that: every
# scheduled invalidation is forwarded, and the handle's own pre-reset
# invalidate-all adds one more, so anything below the scheduled count proves a
# dropped forward. A one-turn grant budget makes the run span several grants,
# so the schedule fires more than once.
echo -e "${YELLOW}== --dmi-invalidate-every ${INVALIDATE_EVERY} (commit log must be unchanged)${NC}"
read -r invalidation_guest invalidation_profile invalidation_directory <<<"${GUESTS[0]}"
invalidation_elf="${ELF_DIR}/${invalidation_directory}/${invalidation_guest}.elf"
invalidation_log="${OUTDIR}/logs/${invalidation_guest}.systemc.invalidate.log"
capture invalidation_output invalidation_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${invalidation_elf}" \
    --isa "${invalidation_profile}" --max-turns "${MAX_CYCLES}" \
    --log-commits "${invalidation_log}" --dmi \
    --turn-budget 1 --dmi-invalidate-every "${INVALIDATE_EVERY}"
[ "${invalidation_status}" -ne 124 ] \
    || fail "the --dmi-invalidate-every run timed out after ${RUN_TIMEOUT}s"
invalidation_exit="$(smoke_exit_code "${invalidation_output}")"
invalidations="$(ram_direct_mem_invalidations "${invalidation_output}")"
scheduled="$(ram_direct_mem_scheduled "${invalidation_output}")"
invalidation_grants="$(smoke_grant_count "${invalidation_output}")"
if [ "${invalidation_status}" -ne 0 ]; then
    printf '%s\n' "${invalidation_output}"
    fail "the --dmi-invalidate-every run exited with status ${invalidation_status}"
fi
if ! printf '%s\n' "${invalidation_output}" | grep -q '^systemc-smoke: result state=exited '; then
    printf '%s\n' "${invalidation_output}"
    fail "the --dmi-invalidate-every run did not report a terminal ruscv-sim result"
fi
[ -n "${invalidations}" ] \
    || fail "the --dmi-invalidate-every run reported no invalidation count"
[ -n "${scheduled}" ] \
    || fail "the --dmi-invalidate-every run reported no scheduled count"
[ -n "${invalidation_grants}" ] \
    || fail "the --dmi-invalidate-every run reported no grant count"
expected_invalidations=$((invalidation_grants / INVALIDATE_EVERY))
if [ "${expected_invalidations}" -eq 0 ]; then
    printf '%s\n' "${invalidation_output}"
    fail "the --dmi-invalidate-every ${INVALIDATE_EVERY} run never scheduled an invalidation (${invalidation_grants} grant(s))"
fi
if [ "${scheduled}" -ne "${expected_invalidations}" ]; then
    printf '%s\n' "${invalidation_output}"
    fail "--dmi-invalidate-every ${INVALIDATE_EVERY}: ${invalidation_grants} grant(s) should schedule ${expected_invalidations} invalidation(s), but ${scheduled} were scheduled"
fi
if [ "${invalidations}" -lt "${scheduled}" ]; then
    printf '%s\n' "${invalidation_output}"
    fail "--dmi-invalidate-every ${INVALIDATE_EVERY}: ${scheduled} scheduled invalidation(s), but only ${invalidations} were forwarded to the ABI"
fi
native_log="${OUTDIR}/logs/${invalidation_guest}.native.log"
if [ "${invalidation_exit}" != "${NATIVE_EXITS[${invalidation_guest}]}" ]; then
    printf '%s\n' "${invalidation_output}"
    fail "the --dmi-invalidate-every run's exit code ${invalidation_exit} differs from native ${NATIVE_EXITS[${invalidation_guest}]}"
fi
if ! cmp -s "${native_log}" "${invalidation_log}"; then
    diff -u "${native_log}" "${invalidation_log}" | head -20 || true
    fail "the --dmi-invalidate-every commit log differs from native"
fi
echo -e "  ${GREEN}[PASS]${NC} ${scheduled} scheduled invalidation(s) left the commit log unchanged"

# The early-`sc_stop` control. The kernel stops after one small grant, long
# before any guest can reach a terminal result, so the host must report that a
# stop is not a run result and must exit non-zero.
echo -e "${YELLOW}== early sc_stop control (must not pass)${NC}"
read -r early_guest early_profile early_directory <<<"${GUESTS[0]}"
early_elf="${ELF_DIR}/${early_directory}/${early_guest}.elf"
capture early_output early_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${early_elf}" --isa "${early_profile}" \
    --stop-after-grants 1 --turn-budget "${EARLY_STOP_TURNS}"
[ "${early_status}" -ne 124 ] || fail "the early sc_stop control timed out"
if [ "${early_status}" -eq 0 ]; then
    printf '%s\n' "${early_output}"
    fail "the early sc_stop control passed; a host stop before a terminal result must not"
fi
if ! printf '%s\n' "${early_output}" \
    | grep -q '^systemc-smoke: host stop after 1 grant(s) before a terminal ruscv-sim result$'; then
    printf '%s\n' "${early_output}"
    fail "the early sc_stop control did not report an early host stop"
fi
if printf '%s\n' "${early_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${early_output}"
    fail "the early sc_stop control reported a terminal ruscv-sim result"
fi
if [ -n "$(smoke_exit_code "${early_output}")" ]; then
    printf '%s\n' "${early_output}"
    fail "the early sc_stop control reported a guest exit code"
fi
echo -e "  ${GREEN}[PASS]${NC} the early stop exited ${early_status} without a run result"

# The two quantum fault-option controls (quantum §4). The mutation runner cannot
# own them: it builds only Rust (`cargo test --offline --all-features`) and D14
# keeps `--all-features` free of a C++ toolchain, so each is an option of this
# binary in the style of `--stop-after-grants` instead. Both run under
# `--quantum 1024`, where a grant spans several turns and every turn after the
# first carries a non-zero local offset, so both faults are reachable.
# `main.cpp` prints `exit_code` for every terminal state, so neither assertion
# tests for its absence: each requires `result state=failed`, the C ABI's
# classification of the failed grant and its own named diagnostic.
read -r fault_guest fault_profile fault_directory <<<"${GUESTS[0]}"
fault_elf="${ELF_DIR}/${fault_directory}/${fault_guest}.elf"

# --fault-keep-offset: the initiator reports the full delay argument, offset
# included, as an added delay. It exceeds this target's declared
# per-transaction maximum of zero, so the grant fails as a host failure rather
# than an unknown completion, and the diagnostic must name that maximum.
echo -e "${YELLOW}== --fault-keep-offset under --quantum ${FAULT_QUANTUM} (must not pass)${NC}"
capture keep_output keep_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --quantum "${FAULT_QUANTUM}" --fault-keep-offset
[ "${keep_status}" -ne 124 ] || fail "the --fault-keep-offset control timed out"
if [ "${keep_status}" -eq 0 ]; then
    printf '%s\n' "${keep_output}"
    fail "the --fault-keep-offset control passed; a reported offset above the declared maximum must fail"
fi
if ! printf '%s\n' "${keep_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${keep_output}"
    fail "the --fault-keep-offset control did not report a failed run"
fi
if ! printf '%s\n' "${keep_output}" \
    | grep -q 'above its declared per-transaction maximum of 0 fs'; then
    printf '%s\n' "${keep_output}"
    fail "the --fault-keep-offset control's diagnostic did not name the exceeded declared maximum"
fi
if ! printf '%s\n' "${keep_output}" | grep -q '^systemc-smoke: failed run facts uncertain=0$'; then
    printf '%s\n' "${keep_output}"
    fail "the --fault-keep-offset control was not classified as a host failure"
fi
echo -e "  ${GREEN}[PASS]${NC} the exceeded declared maximum failed the run (status ${keep_status})"

# --fault-decrease-delay: the target lowers the delay below the offset it
# received. A delay below the offset cannot be represented as an added delay,
# and the target may already have committed the write, so the initiator reports
# the transaction as an unknown completion, which the boundary's `uncertain`
# flag records.
echo -e "${YELLOW}== --fault-decrease-delay under --quantum ${FAULT_QUANTUM} (must not pass)${NC}"
capture decrease_output decrease_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --quantum "${FAULT_QUANTUM}" --fault-decrease-delay
[ "${decrease_status}" -ne 124 ] || fail "the --fault-decrease-delay control timed out"
if [ "${decrease_status}" -eq 0 ]; then
    printf '%s\n' "${decrease_output}"
    fail "the --fault-decrease-delay control passed; a delay below the offset must fail"
fi
if ! printf '%s\n' "${decrease_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${decrease_output}"
    fail "the --fault-decrease-delay control did not report a failed run"
fi
if ! printf '%s\n' "${decrease_output}" \
    | grep -q 'could not establish the outcome of '; then
    printf '%s\n' "${decrease_output}"
    fail "the --fault-decrease-delay control's diagnostic did not name the unestablished outcome"
fi
if ! printf '%s\n' "${decrease_output}" | grep -q '^systemc-smoke: failed run facts uncertain=1$'; then
    printf '%s\n' "${decrease_output}"
    fail "the --fault-decrease-delay control was not classified as an unknown completion"
fi
echo -e "  ${GREEN}[PASS]${NC} the delay below the offset failed the run as an unknown completion (status ${decrease_status})"

# The five non-blocking fault-option controls (nb §4). They are options of this
# binary for the same reason as the quantum ones: the mutation runner builds
# only Rust, and D11 keeps `--all-features` free of a C++ toolchain. Each runs in
# its stated mode with its own assertion, and a run that passes with one set is a
# smoke failure.
#
# `--fault-late-response` delays the run's first AT access's final phase to one
# tick past the declared maximum. That access is the guest's first fetch, so the
# initiator retires it at its timeout as an unknown completion, ignores and
# counts the late phase, and drains before destroy. The reset probe in front of
# the drain must be refused while the target still holds the transaction; the
# `completed` path releases it on the forward path, so it skips the probe.
#
# Each assertion reads one obligation of D9: the failed boundary's `uncertain`
# class, the diagnostic's own words, the exact timeout the retirement waited,
# the single late phase it counted, the drain's retained `END_RESP` and that
# phase's host time, the withheld reset acknowledgement and the target's
# released state.
#
#   run_late_response PATH EXPECTED_RESET EXPECTED_END_RESP
#     PATH                accepted | early | completed
#     EXPECTED_RESET      required (the target withholds it) | skipped
#     EXPECTED_END_RESP   forward END_RESP phases the drain must send
run_late_response() {
    local path="$1"
    local expected_reset="$2"
    local expected_end_resp="$3"
    local label="--nb --nb-latency ${NB_LATENCY} --nb-path ${path} --fault-late-response"
    # `capture` owns locals named `output` and `status`, so the result
    # variables must use other names.
    local late_output
    local late_status
    capture late_output late_status timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" \
        --isa "${fault_profile}" --max-turns "${MAX_CYCLES}" \
        --nb --nb-latency "${NB_LATENCY}" --nb-path "${path}" --fault-late-response
    [ "${late_status}" -ne 124 ] || fail "the ${label} control timed out"
    if [ "${late_status}" -eq 0 ]; then
        printf '%s\n' "${late_output}"
        fail "the ${label} control passed; a late response must retire the transaction"
    fi
    if ! printf '%s\n' "${late_output}" | grep -q '^systemc-smoke: result state=failed '; then
        printf '%s\n' "${late_output}"
        fail "the ${label} control did not report a failed run"
    fi
    if ! printf '%s\n' "${late_output}" | grep -q '^systemc-smoke: failed run facts uncertain=1$'; then
        printf '%s\n' "${late_output}"
        fail "the ${label} control was not retired as an unknown completion"
    fi
    if ! printf '%s\n' "${late_output}" | grep -q 'could not establish the outcome of '; then
        printf '%s\n' "${late_output}"
        fail "the ${label} control's diagnostic did not say the target could not establish the outcome"
    fi
    local late
    local retirements
    local end_resp
    local end_resp_fs
    local host
    local delay
    local open
    local begin
    local applied
    late="$(smoke_at_field "${late_output}" late_phases)"
    retirements="$(smoke_at_field "${late_output}" retirements)"
    end_resp="$(smoke_at_field "${late_output}" drain_end_resp)"
    end_resp_fs="$(smoke_at_field "${late_output}" drain_end_resp_fs)"
    host="$(smoke_totals_field "${late_output}" host_elapsed_fs)"
    delay="$(smoke_totals_field "${late_output}" transport_delay_fs)"
    open="$(smoke_at_field "${late_output}" open_transaction)"
    begin="$(ram_begin_req_count "${late_output}")"
    applied="$(ram_applied_access_count "${late_output}")"
    [ -n "${late}" ] || fail "the ${label} control reported no late-phase count"
    [ -n "${retirements}" ] || fail "the ${label} control reported no retirement count"
    [ -n "${end_resp}" ] || fail "the ${label} control reported no drain count"
    [ -n "${end_resp_fs}" ] || fail "the ${label} control reported no drain host time"
    [ -n "${host}" ] || fail "the ${label} control reported no host elapsed total"
    [ -n "${delay}" ] || fail "the ${label} control reported no transport delay total"
    # nb D9: each of the three late forms is counted once, not once per
    # detection site.  Each control injects exactly one late final phase.
    [ "${late}" -eq 1 ] \
        || fail "the ${label} control ignored ${late} late phase(s), not the one it injected"
    [ "${retirements}" -eq 1 ] \
        || fail "the ${label} control retired ${retirements} transaction(s), not one"
    # nb D9: the transaction is not complete at the timeout, so it is retired at
    # exactly `max_delay_fs + 1` fs, and the ABI charges and totals that time.
    # The failed first fetch is the run's only counted transaction.
    local expected_timeout=$((NB_LATENCY + 1))
    [ "${host}" -eq "${expected_timeout}" ] \
        || fail "the ${label} control waited ${host} fs, not the ${expected_timeout} fs timeout"
    [ "${delay}" -eq "${expected_timeout}" ] \
        || fail "the ${label} control charged ${delay} fs, not the ${expected_timeout} fs timeout"
    [ "${open}" = "0" ] \
        || fail "the ${label} control left a transaction open at the target after its drain"
    [ -n "${begin}" ] && [ "${begin}" = "${applied}" ] \
        || fail "the ${label} control applied ${applied} of its ${begin} BEGIN_REQ phase(s)"
    [ "${end_resp}" -eq "${expected_end_resp}" ] \
        || fail "the ${label} control sent ${end_resp} drain END_RESP phase(s), not ${expected_end_resp}"
    # nb D9: the drain sends the retained transaction's `END_RESP` no earlier
    # than its `BEGIN_RESP` phase's effective time, which the target annotated
    # as `lambda` plus one Platform tick for this control. The host waits to
    # that time, so it sends the phase exactly then; a drain that sends it early
    # closes the target's transaction before its own phase takes effect.
    local expected_end_resp_fs=0
    if [ "${expected_end_resp}" -eq 1 ]; then
        expected_end_resp_fs=$((NB_LATENCY + 1000000))
    fi
    [ "${end_resp_fs}" -eq "${expected_end_resp_fs}" ] \
        || fail "the ${label} control sent END_RESP at ${end_resp_fs} fs, not ${expected_end_resp_fs} fs"
    if [ "${expected_reset}" = "required" ]; then
        local reset
        reset="$(smoke_reset_status "${late_output}")"
        [ -n "${reset}" ] \
            || { printf '%s\n' "${late_output}"; fail "the ${label} control never probed the transport reset"; }
        [ "${reset}" -ne 0 ] \
            || fail "the ${label} control's transport reset was acknowledged while a transaction was open"
        echo -e "  ${GREEN}[PASS]${NC} ${label}: late phase counted, drain released (status ${late_status}, reset ${reset})"
    else
        if printf '%s\n' "${late_output}" | grep -q '^systemc-smoke: reset with an open AT transaction '; then
            printf '%s\n' "${late_output}"
            fail "the ${label} control probed the reset although the target holds no transaction"
        fi
        if ! printf '%s\n' "${late_output}" \
            | grep -q '^systemc-smoke: reset probe skipped: the target holds no open AT transaction$'; then
            printf '%s\n' "${late_output}"
            fail "the ${label} control did not report the skipped reset probe"
        fi
        echo -e "  ${GREEN}[PASS]${NC} ${label}: late completion counted, storage released at once (status ${late_status})"
    fi
}

echo -e "${YELLOW}== --fault-late-response on accepted (must not pass)${NC}"
run_late_response accepted required 0
echo -e "${YELLOW}== --fault-late-response on early (must not pass)${NC}"
run_late_response early required 1
echo -e "${YELLOW}== --fault-late-response on completed (must not pass)${NC}"
run_late_response completed skipped 0

# --fault-uncounted-elapsed: the initiator reports `e` = 0 while it reports `d`
# as the host time that passed, so the grant returns no host elapsed time and the
# kernel clock ends beyond the Machine cursor. The host must fail the run with
# that mismatch, not with a run result.
echo -e "${YELLOW}== --fault-uncounted-elapsed (must not pass)${NC}"
capture uncounted_output uncounted_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --nb --nb-latency "${NB_LATENCY}" \
    --nb-path accepted --fault-uncounted-elapsed
[ "${uncounted_status}" -ne 124 ] || fail "the --fault-uncounted-elapsed control timed out"
if [ "${uncounted_status}" -eq 0 ]; then
    printf '%s\n' "${uncounted_output}"
    fail "the --fault-uncounted-elapsed control passed; unreported host time must part the clocks"
fi
if printf '%s\n' "${uncounted_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${uncounted_output}"
    fail "the --fault-uncounted-elapsed control reported a run result instead of a host failure"
fi
if ! printf '%s\n' "${uncounted_output}" | grep -q 'does not match the Machine cursor'; then
    printf '%s\n' "${uncounted_output}"
    fail "the --fault-uncounted-elapsed control's diagnostic did not name the kernel clock mismatch"
fi
echo -e "  ${GREEN}[PASS]${NC} the unaccounted host time failed the run (status ${uncounted_status})"

# --fault-short-delay: the initiator reports `d` one femtosecond below `e`, so a
# terminal response fails D4's elapsed-above-delay rule. The failed boundary
# keeps a known outcome, so its `uncertain` flag must be 0.
echo -e "${YELLOW}== --fault-short-delay (must not pass)${NC}"
capture short_output short_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --nb --nb-latency "${NB_LATENCY}" \
    --nb-path accepted --fault-short-delay
[ "${short_status}" -ne 124 ] || fail "the --fault-short-delay control timed out"
if [ "${short_status}" -eq 0 ]; then
    printf '%s\n' "${short_output}"
    fail "the --fault-short-delay control passed; a delay below the host elapsed time must fail"
fi
if ! printf '%s\n' "${short_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${short_output}"
    fail "the --fault-short-delay control did not report a failed run"
fi
if ! printf '%s\n' "${short_output}" | grep -q '^systemc-smoke: failed run facts uncertain=0$'; then
    printf '%s\n' "${short_output}"
    fail "the --fault-short-delay control was not classified as a host failure"
fi
if ! printf '%s\n' "${short_output}" \
    | grep -q 'host elapsed time above its [0-9][0-9]* fs response delay'; then
    printf '%s\n' "${short_output}"
    fail "the --fault-short-delay control's diagnostic did not name the elapsed time above the delay"
fi
echo -e "  ${GREEN}[PASS]${NC} the short delay failed the run as a host failure (status ${short_status})"

# --fault-wait-blocking: the target waits one tick in `b_transport`, whose
# request carries no `may_suspend`, so the initiator's measured host time trips
# D4's unmarked-elapsed rule.
echo -e "${YELLOW}== --fault-wait-blocking (must not pass)${NC}"
capture blocking_wait_output blocking_wait_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --fault-wait-blocking
[ "${blocking_wait_status}" -ne 124 ] || fail "the --fault-wait-blocking control timed out"
if [ "${blocking_wait_status}" -eq 0 ]; then
    printf '%s\n' "${blocking_wait_output}"
    fail "the --fault-wait-blocking control passed; a blocking wait must fail"
fi
if ! printf '%s\n' "${blocking_wait_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${blocking_wait_output}"
    fail "the --fault-wait-blocking control did not report a failed run"
fi
if ! printf '%s\n' "${blocking_wait_output}" | grep -q '^systemc-smoke: failed run facts uncertain=0$'; then
    printf '%s\n' "${blocking_wait_output}"
    fail "the --fault-wait-blocking control was not classified as a host failure"
fi
if ! printf '%s\n' "${blocking_wait_output}" \
    | grep -q 'host elapsed time on a request that may not suspend'; then
    printf '%s\n' "${blocking_wait_output}"
    fail "the --fault-wait-blocking control's diagnostic did not name the request that may not suspend"
fi
echo -e "  ${GREEN}[PASS]${NC} the blocking wait failed the run as a host failure (status ${blocking_wait_status})"

# --fault-wait-debug: the target waits one tick in `transport_dbg`, so image
# installation must fail and the host must report the failed ELF load.
echo -e "${YELLOW}== --fault-wait-debug (must not pass)${NC}"
capture debug_wait_output debug_wait_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --max-turns "${MAX_CYCLES}" --fault-wait-debug
[ "${debug_wait_status}" -ne 124 ] || fail "the --fault-wait-debug control timed out"
if [ "${debug_wait_status}" -eq 0 ]; then
    printf '%s\n' "${debug_wait_output}"
    fail "the --fault-wait-debug control passed; a debug wait must fail image installation"
fi
if ! printf '%s\n' "${debug_wait_output}" \
    | grep -q '^systemc-smoke: ruscv_machine_load_elf failed with status '; then
    printf '%s\n' "${debug_wait_output}"
    fail "the --fault-wait-debug control did not report a failed ruscv_machine_load_elf"
fi
if printf '%s\n' "${debug_wait_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${debug_wait_output}"
    fail "the --fault-wait-debug control reported a run result after the failed installation"
fi
echo -e "  ${GREEN}[PASS]${NC} the debug wait failed image installation (status ${debug_wait_status})"

# nb D7: --nb with a non-zero quantum is refused by ruscv_machine_create. The
# host reports the create status as a host failure, not as a run result.
echo -e "${YELLOW}== --nb --quantum ${FAULT_QUANTUM} (must be refused by create)${NC}"
capture nb_quantum_output nb_quantum_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" --isa "${fault_profile}" \
    --nb --quantum "${FAULT_QUANTUM}"
[ "${nb_quantum_status}" -ne 124 ] || fail "the --nb --quantum control timed out"
if [ "${nb_quantum_status}" -eq 0 ]; then
    printf '%s\n' "${nb_quantum_output}"
    fail "the --nb --quantum control passed; a suspending table with a quantum must be refused"
fi
if ! printf '%s\n' "${nb_quantum_output}" \
    | grep -q '^systemc-smoke: ruscv_machine_create failed with status 8$'; then
    printf '%s\n' "${nb_quantum_output}"
    fail "the --nb --quantum control was not refused by ruscv_machine_create as RUSCV_ERROR_CONFIG"
fi
if printf '%s\n' "${nb_quantum_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${nb_quantum_output}"
    fail "the --nb --quantum control reported a run result after the create refusal"
fi
echo -e "  ${GREEN}[PASS]${NC} --nb --quantum ${FAULT_QUANTUM} was refused at create"

# ---- device U4: the SystemC UART target (device D9) ----
#
# `hello` is the guest the contract's device parity is stated on (device D7):
# the suite already requires `Hello!` from it on every route, and the host UART
# target must transmit exactly `Hello!\n`. The profile is passed explicitly so
# the comparison does not depend on the CLI's default.
UART_GUEST=hello
UART_PROFILE=rv64imafdc
UART_ELF="${ELF_DIR}/rv64i/${UART_GUEST}.elf"
[ -f "${UART_ELF}" ] || fail "guest not found: ${UART_ELF}"

echo -e "${YELLOW}== ${UART_GUEST}: the native baseline (device D7)${NC}"
UART_NATIVE_LOG="${OUTDIR}/logs/${UART_GUEST}.native.log"
capture uart_native_output uart_native_status \
    timeout "${RUN_TIMEOUT}" "${RUSCV_BIN}" run "${UART_ELF}" --isa "${UART_PROFILE}" \
    --max-cycles "${MAX_CYCLES}" --log-commits "${UART_NATIVE_LOG}"
[ "${uart_native_status}" -ne 124 ] \
    || fail "${UART_GUEST}: the native run timed out after ${RUN_TIMEOUT}s"
UART_NATIVE_EXIT="$(cli_exit_code "${uart_native_output}")"
if [ "${uart_native_status}" -ne 0 ] || [ -z "${UART_NATIVE_EXIT}" ]; then
    printf '%s\n' "${uart_native_output}"
    fail "${UART_GUEST}: the native run did not report an exit code"
fi
[ -s "${UART_NATIVE_LOG}" ] || fail "${UART_GUEST}: the native commit log is empty"
if ! printf '%s\n' "${uart_native_output}" | grep -q 'Hello!'; then
    printf '%s\n' "${uart_native_output}"
    fail "${UART_GUEST}: the native run did not print Hello!"
fi
echo "  native: exit status ${uart_native_status}, guest exit code ${UART_NATIVE_EXIT}"

# require_uart_pass LABEL TARGET LOG [OPTIONS...]
#   Runs one passing UART-target pass and checks every fact device D9 owes it:
#   the native exit code, a byte-identical commit log, a terminal result, a
#   transaction count equal to the read plus write count, and zero host device
#   transactions for the in-tree selection. Sets LAST_* to the run's facts.
require_uart_pass() {
    local label="$1"
    local target="$2"
    local log="$3"
    shift 3
    local pass_output
    local pass_status
    local fact
    capture pass_output pass_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${UART_ELF}" --isa "${UART_PROFILE}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${log}" --uart-target "${target}" "$@"
    [ "${pass_status}" -ne 124 ] || fail "${label}: the run timed out after ${RUN_TIMEOUT}s"
    if [ "${pass_status}" -ne 0 ]; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run exited with status ${pass_status}"
    fi
    if ! printf '%s\n' "${pass_output}" | grep -q "^systemc-smoke: uart target=${target}$"; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run did not report its --uart-target ${target} selection"
    fi
    if ! printf '%s\n' "${pass_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run did not report a terminal ruscv-sim result"
    fi
    LAST_EXIT="$(smoke_exit_code "${pass_output}")"
    [ -n "${LAST_EXIT}" ] || fail "${label}: the run reported no exit code"
    [ "${LAST_EXIT}" = "${UART_NATIVE_EXIT}" ] \
        || fail "${label}: exit code ${LAST_EXIT} differs from native ${UART_NATIVE_EXIT}"
    [ -s "${log}" ] || fail "${label}: the commit log is empty"
    if ! cmp -s "${UART_NATIVE_LOG}" "${log}"; then
        diff -u "${UART_NATIVE_LOG}" "${log}" | head -20 || true
        fail "${label}: the commit log differs from native"
    fi
    LAST_GRANTS="$(smoke_grant_count "${pass_output}")"
    LAST_CONSUMED="$(smoke_totals_field "${pass_output}" consumed_fs)"
    LAST_TX="$(uart_field "${pass_output}" b_transport)"
    LAST_READS="$(uart_field "${pass_output}" reads)"
    LAST_WRITES="$(uart_field "${pass_output}" writes)"
    LAST_TRANSMITTED="$(uart_field "${pass_output}" transmitted)"
    LAST_BYTES="$(uart_field "${pass_output}" bytes)"
    LAST_CONSOLE="$(uart_console_sequence "${pass_output}")"
    for fact in "grants:${LAST_GRANTS}" "consumed:${LAST_CONSUMED}" \
        "b_transport:${LAST_TX}" "reads:${LAST_READS}" "writes:${LAST_WRITES}" \
        "transmitted:${LAST_TRANSMITTED}" "bytes:${LAST_BYTES}"; do
        [ -n "${fact#*:}" ] || fail "${label}: the run reported no ${fact%%:*} fact"
    done
    [ "${LAST_TX}" -eq $((LAST_READS + LAST_WRITES)) ] \
        || fail "${label}: the host UART served ${LAST_TX} device transaction(s), not its ${LAST_READS} read(s) plus ${LAST_WRITES} write(s)"
    if [ "${target}" = "in-tree" ]; then
        [ "${LAST_TX}" -eq 0 ] \
            || fail "${label}: the in-tree selection reported ${LAST_TX} host device transaction(s); it must report zero"
    fi
    LAST_OUTPUT="${pass_output}"
}

# `hello` runs under both selections at the same options. The in-tree pass keeps
# the Rust device and reports zero host device transactions; the host pass serves
# the same window through the C ABI's device table and must transmit exactly
# `Hello!\n`, with every architectural fact equal to the matching in-tree pass
# (device D9).
echo -e "${YELLOW}== ${UART_GUEST}: --uart-target in-tree and host (device D9)${NC}"
for variant in "baseline:" "dmi:--dmi" "nb:--nb" "quantum:--quantum ${FAULT_QUANTUM}"; do
    name="${variant%%:*}"
    flags="${variant#*:}"
    variant_options=()
    if [ -n "${flags}" ]; then
        read -r -a variant_options <<<"${flags}"
    fi
    require_uart_pass "${UART_GUEST} --uart-target in-tree (${name})" in-tree \
        "${OUTDIR}/logs/${UART_GUEST}.uart-in-tree.${name}.log" \
        ${variant_options[@]+"${variant_options[@]}"}
    in_tree_grants="${LAST_GRANTS}"
    in_tree_consumed="${LAST_CONSUMED}"
    [ "${LAST_TRANSMITTED}" -eq 0 ] \
        || fail "${UART_GUEST}: the in-tree (${name}) pass transmitted ${LAST_TRANSMITTED} host byte(s); the Rust device reports none here"
    [ -z "${LAST_CONSOLE}" ] \
        || fail "${UART_GUEST}: the in-tree (${name}) pass echoed console byte(s) ${LAST_CONSOLE}; the in-tree selection serves no host UART"
    require_uart_pass "${UART_GUEST} --uart-target host (${name})" host \
        "${OUTDIR}/logs/${UART_GUEST}.uart-host.${name}.log" \
        ${variant_options[@]+"${variant_options[@]}"}
    [ "${LAST_TX}" -gt 0 ] \
        || fail "${UART_GUEST}: the host (${name}) pass served no device transaction"
    [ "${LAST_TRANSMITTED}" -eq 7 ] \
        || fail "${UART_GUEST}: the host (${name}) pass transmitted ${LAST_TRANSMITTED} byte(s), not the guest's seven"
    [ "${LAST_BYTES}" = '"Hello!\n"' ] \
        || fail "${UART_GUEST}: the host (${name}) pass transmitted ${LAST_BYTES}, not exactly Hello!\\n"
    [ "${LAST_CONSOLE}" = 'Hello!\n' ] \
        || fail "${UART_GUEST}: the host (${name}) pass echoed console byte(s) ${LAST_CONSOLE}, not exactly Hello!\\n"
    [ "${LAST_GRANTS}" = "${in_tree_grants}" ] \
        || fail "${UART_GUEST}: the host (${name}) pass took ${LAST_GRANTS} grant(s), not the in-tree pass's ${in_tree_grants}"
    [ "${LAST_CONSUMED}" = "${in_tree_consumed}" ] \
        || fail "${UART_GUEST}: the host (${name}) pass consumed ${LAST_CONSUMED} fs, not the in-tree pass's ${in_tree_consumed} fs"
    echo -e "  ${GREEN}[PASS]${NC} ${name}: host device transactions ${LAST_TX} = ${LAST_READS} read(s) + ${LAST_WRITES} write(s), ${LAST_TRANSMITTED} byte(s)"
done

# device D7: a second host run of the same guest produces the same bytes, commit
# log and exit code, so the sequence is the Hart's instruction sequence and not a
# scheduling accident.
require_uart_pass "${UART_GUEST} --uart-target host (repeat)" host \
    "${OUTDIR}/logs/${UART_GUEST}.uart-host.repeat.log"
[ "${LAST_BYTES}" = '"Hello!\n"' ] \
    || fail "${UART_GUEST}: the repeated host pass transmitted ${LAST_BYTES}, not exactly Hello!\\n"
[ "${LAST_CONSOLE}" = 'Hello!\n' ] \
    || fail "${UART_GUEST}: the repeated host pass echoed console byte(s) ${LAST_CONSOLE}, not exactly Hello!\\n"
if ! cmp -s "${OUTDIR}/logs/${UART_GUEST}.uart-host.baseline.log" \
    "${OUTDIR}/logs/${UART_GUEST}.uart-host.repeat.log"; then
    fail "${UART_GUEST}: the repeated host pass's commit log differs from the first"
fi
echo -e "  ${GREEN}[PASS]${NC} a second host run produced identical bytes, commit log and exit code"

# The two existing guests under --uart-target host: neither touches the UART, so
# both must reproduce their facts with zero device transactions (device D9).
echo -e "${YELLOW}== the existing guests under --uart-target host (device D9)${NC}"
for entry in "${GUESTS[@]}"; do
    read -r guest profile directory <<<"${entry}"
    elf="${ELF_DIR}/${directory}/${guest}.elf"
    uart_host_log="${OUTDIR}/logs/${guest}.systemc.uart-host.log"
    capture uart_host_output uart_host_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${uart_host_log}" --uart-target host
    [ "${uart_host_status}" -ne 124 ] \
        || fail "${guest}: the --uart-target host run timed out after ${RUN_TIMEOUT}s"
    if [ "${uart_host_status}" -ne 0 ]; then
        printf '%s\n' "${uart_host_output}"
        fail "${guest}: the --uart-target host run exited with status ${uart_host_status}"
    fi
    if ! printf '%s\n' "${uart_host_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${uart_host_output}"
        fail "${guest}: the --uart-target host run did not report a terminal ruscv-sim result"
    fi
    uart_host_exit="$(smoke_exit_code "${uart_host_output}")"
    [ "${uart_host_exit}" = "${NATIVE_EXITS[${guest}]}" ] \
        || fail "${guest}: the --uart-target host exit code ${uart_host_exit} differs from native ${NATIVE_EXITS[${guest}]}"
    if ! cmp -s "${OUTDIR}/logs/${guest}.native.log" "${uart_host_log}"; then
        diff -u "${OUTDIR}/logs/${guest}.native.log" "${uart_host_log}" | head -20 || true
        fail "${guest}: the --uart-target host commit log differs from native"
    fi
    uart_host_transactions="$(uart_field "${uart_host_output}" b_transport)"
    [ -n "${uart_host_transactions}" ] \
        || fail "${guest}: the --uart-target host run reported no device transaction count"
    [ "${uart_host_transactions}" -eq 0 ] \
        || fail "${guest}: the --uart-target host run served ${uart_host_transactions} device transaction(s) although the guest never touches the UART"
    echo -e "  ${GREEN}[PASS]${NC} ${guest}: native exit code and commit log with zero device transactions"
done

# device §4: --fault-uart-no-lsr answers LSR without the transmit-empty bit, so
# `hello` never leaves its wait and exhausts its cycle bound instead of passing.
echo -e "${YELLOW}== --fault-uart-no-lsr (must not pass)${NC}"
capture no_lsr_output no_lsr_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${UART_ELF}" --isa "${UART_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --uart-target host --fault-uart-no-lsr
[ "${no_lsr_status}" -ne 124 ] || fail "the --fault-uart-no-lsr control timed out"
if [ "${no_lsr_status}" -eq 0 ]; then
    printf '%s\n' "${no_lsr_output}"
    fail "the --fault-uart-no-lsr control passed; an LSR without the transmit-empty bit must fail the run"
fi
if ! printf '%s\n' "${no_lsr_output}" \
    | grep -q "exceeded the ${MAX_CYCLES} turn limit"; then
    printf '%s\n' "${no_lsr_output}"
    fail "the --fault-uart-no-lsr control did not exhaust the turn limit"
fi
if printf '%s\n' "${no_lsr_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${no_lsr_output}"
    fail "the --fault-uart-no-lsr control reported a run result after the exhausted bound"
fi
no_lsr_transmitted="$(uart_field "${no_lsr_output}" transmitted)"
[ -n "${no_lsr_transmitted}" ] \
    || fail "the --fault-uart-no-lsr control reported no transmitted count"
[ "${no_lsr_transmitted}" -eq 0 ] \
    || fail "the --fault-uart-no-lsr control transmitted a byte; the guest never left its wait"
echo -e "  ${GREEN}[PASS]${NC} the missing transmit-empty bit exhausted the turn limit (status ${no_lsr_status})"

# device §4: --fault-uart-wait waits one Platform tick inside `b_transport` and
# reports the interval as host elapsed time, which device D4 forbids: host time
# may not pass inside a device callback.
echo -e "${YELLOW}== --fault-uart-wait (must not pass)${NC}"
capture uart_wait_output uart_wait_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${UART_ELF}" --isa "${UART_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --uart-target host --fault-uart-wait
[ "${uart_wait_status}" -ne 124 ] || fail "the --fault-uart-wait control timed out"
if [ "${uart_wait_status}" -eq 0 ]; then
    printf '%s\n' "${uart_wait_output}"
    fail "the --fault-uart-wait control passed; host time inside a device callback must fail"
fi
if ! printf '%s\n' "${uart_wait_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${uart_wait_output}"
    fail "the --fault-uart-wait control did not report a failed run"
fi
if ! printf '%s\n' "${uart_wait_output}" \
    | grep -q 'host elapsed time for .* on the UART window'; then
    printf '%s\n' "${uart_wait_output}"
    fail "the --fault-uart-wait control's diagnostic did not name host elapsed time on a device transaction"
fi
if ! printf '%s\n' "${uart_wait_output}" \
    | grep -q 'host time may not pass inside a device callback'; then
    printf '%s\n' "${uart_wait_output}"
    fail "the --fault-uart-wait control's diagnostic did not name the device callback rule"
fi
echo -e "  ${GREEN}[PASS]${NC} the device wait failed the run as a protocol failure (status ${uart_wait_status})"

# device §4: --fault-uart-event-on-fault reports a transmit with a fault status,
# which device D4 forbids, so no byte may reach the host's console.
echo -e "${YELLOW}== --fault-uart-event-on-fault (must not pass)${NC}"
capture uart_event_output uart_event_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${UART_ELF}" --isa "${UART_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --uart-target host --fault-uart-event-on-fault
[ "${uart_event_status}" -ne 124 ] || fail "the --fault-uart-event-on-fault control timed out"
if [ "${uart_event_status}" -eq 0 ]; then
    printf '%s\n' "${uart_event_output}"
    fail "the --fault-uart-event-on-fault control passed; an event on a fault status must fail"
fi
if ! printf '%s\n' "${uart_event_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${uart_event_output}"
    fail "the --fault-uart-event-on-fault control did not report a failed run"
fi
if ! printf '%s\n' "${uart_event_output}" \
    | grep -q 'only an accepted access publishes an event'; then
    printf '%s\n' "${uart_event_output}"
    fail "the --fault-uart-event-on-fault control did not fail as a protocol failure of that access"
fi
uart_event_transmitted="$(uart_field "${uart_event_output}" transmitted)"
[ -n "${uart_event_transmitted}" ] \
    || fail "the --fault-uart-event-on-fault control reported no transmitted count"
[ "${uart_event_transmitted}" -eq 0 ] \
    || fail "the --fault-uart-event-on-fault control transmitted a byte"
uart_event_console="$(uart_console_sequence "${uart_event_output}")"
[ -z "${uart_event_console}" ] \
    || fail "the --fault-uart-event-on-fault control let console byte(s) ${uart_event_console} reach the host's console"
echo -e "  ${GREEN}[PASS]${NC} the fault-status event failed the access and transmitted nothing (status ${uart_event_status})"

# nb D7/D8: the non-blocking selections are usage errors in the wrong mode,
# and the fault options keep their existing --quantum requirement.
echo -e "${YELLOW}== non-blocking usage errors (must not pass)${NC}"
check_usage_error() {
    local label="$1"
    local expected="$2"
    shift 2
    local usage_output
    local usage_status
    capture usage_output usage_status timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${fault_elf}" \
        --isa "${fault_profile}" "$@"
    [ "${usage_status}" -ne 124 ] || fail "${label} timed out"
    [ "${usage_status}" -eq 2 ] || {
        printf '%s\n' "${usage_output}"
        fail "${label} exited ${usage_status}, not the usage status 2"
    }
    if ! printf '%s\n' "${usage_output}" | grep -q "${expected}"; then
        printf '%s\n' "${usage_output}"
        fail "${label} did not report: ${expected}"
    fi
    echo -e "  ${GREEN}[PASS]${NC} ${label}"
}
check_usage_error "--nb-latency without --nb" "require --nb" --nb-latency 0
check_usage_error "--nb-path without --nb" "require --nb" --nb-path accepted
check_usage_error "--nb-latency above 10^15 fs" "must be at most 1000000000000000 fs" \
    --nb --nb-latency 1000000000000001
check_usage_error "--fault-keep-offset without --quantum" "require --quantum" \
    --fault-keep-offset
check_usage_error "--fault-decrease-delay without --quantum" "require --quantum" \
    --fault-decrease-delay
# nb §4: each non-blocking fault option is a usage error in the wrong mode.
check_usage_error "--fault-late-response without --nb" "require --nb" \
    --fault-late-response
check_usage_error "--fault-uncounted-elapsed without --nb" "require --nb" \
    --fault-uncounted-elapsed
check_usage_error "--fault-short-delay without --nb" "require --nb" \
    --fault-short-delay
check_usage_error "--fault-wait-blocking with --nb" "must not be combined with --nb" \
    --fault-wait-blocking --nb
check_usage_error "--fault-wait-debug with --nb" "must not be combined with --nb" \
    --fault-wait-debug --nb
check_usage_error "two fault options" "taken one at a time" \
    --fault-wait-blocking --fault-wait-debug
# device D9: the UART target selection and its three fault options are usage
# errors in the wrong mode.
check_usage_error "--uart-target with an unknown value" "must be in-tree or host" \
    --uart-target native
check_usage_error "--fault-uart-no-lsr without --uart-target host" \
    "require --uart-target host" --fault-uart-no-lsr
check_usage_error "--fault-uart-wait without --uart-target host" \
    "require --uart-target host" --fault-uart-wait
check_usage_error "--fault-uart-event-on-fault without --uart-target host" \
    "require --uart-target host" --fault-uart-event-on-fault
check_usage_error "a UART fault option with an in-tree selection" \
    "require --uart-target host" --uart-target in-tree --fault-uart-wait
# htif D8: the HTIF target selection and its three fault options are usage
# errors in the wrong mode.
check_usage_error "--htif-target with an unknown value" "must be in-tree or host" \
    --htif-target endpoint
check_usage_error "--fault-htif-drop-event without --htif-target host" \
    "require --htif-target host" --fault-htif-drop-event
check_usage_error "--fault-htif-wait without --htif-target host" \
    "require --htif-target host" --fault-htif-wait
check_usage_error "--fault-htif-event-on-fault without --htif-target host" \
    "require --htif-target host" --fault-htif-event-on-fault
check_usage_error "an HTIF fault option with an in-tree selection" \
    "require --htif-target host" --htif-target in-tree --fault-htif-wait

# ---- HTIF U3: the SystemC HTIF target (htif D8) ----
#
# `htif_u3_console` is the contract's guest: it reads the endpoint, exits with
# code 1 if the answer is not zero, writes three console bytes, writes the
# standard syscall exit word for code 0 and self-loops. Its value sequence is
# the three console words and the exit word; the console channel is the host
# target's own rendering of the standard console device, so it observes the
# guest's bytes without reading the counters. The profile is passed explicitly
# so the comparison does not depend on the default.
#
# The C ABI exposes no per-value sink for an HTIF write: `config.htif_write` has
# no C entry and only the exit decode has a host-visible effect.  The smoke
# therefore proves the exit word's publication through the run stop (the host
# pass stops on the same turn, with the same exit code, as its in-tree parity
# run, and the value sequence's last word is the exit word that code decodes),
# a dropped exit event through the run that reaches its turn bound with no exit
# fact, and a refused write through the protocol failure and its absence from
# the target's accepted report sequence.  The console words are the values the
# target reported for the guest's console commands, fixed by the byte-identical
# commit log.  The publication rule for a refused write and for a non-exit write
# is Rust-level (`src/blocking/device.rs`, proven in `src/machine/tests.rs`);
# this host cannot observe it, and a sink-level console assertion would need a
# new ABI entry, which htif D7 keeps append-only and U3 does not add.
HTIF_GUEST=htif_u3_console
HTIF_PROFILE=rv64imafdc
HTIF_ELF="${ELF_DIR}/rv64i/${HTIF_GUEST}.elf"
[ -f "${HTIF_ELF}" ] || fail "guest not found: ${HTIF_ELF}"
# The guest renders exactly these three bytes.
HTIF_CONSOLE='Hi\n'
# device 1, command 1 and the three payload bytes, then the code-0 exit word.
HTIF_CONSOLE_VALUES='0x0101000000000048 0x0101000000000069 0x010100000000000a'
HTIF_EXIT_VALUE='0x0000000000000001'
HTIF_VALUE_SEQUENCE="${HTIF_CONSOLE_VALUES} ${HTIF_EXIT_VALUE}"

require_htif_pass() {
    local label="$1"
    local target="$2"
    local log="$3"
    shift 3
    local pass_output
    local pass_status
    local fact
    capture pass_output pass_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${HTIF_ELF}" --isa "${HTIF_PROFILE}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${log}" --htif-target "${target}" "$@"
    [ "${pass_status}" -ne 124 ] || fail "${label}: the run timed out after ${RUN_TIMEOUT}s"
    if [ "${pass_status}" -ne 0 ]; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run exited with status ${pass_status}"
    fi
    if ! printf '%s\n' "${pass_output}" | grep -q "^systemc-smoke: htif target=${target}$"; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run did not report its --htif-target ${target} selection"
    fi
    if ! printf '%s\n' "${pass_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${pass_output}"
        fail "${label}: the run did not report a terminal ruscv-sim result"
    fi
    LAST_EXIT="$(smoke_exit_code "${pass_output}")"
    [ -n "${LAST_EXIT}" ] || fail "${label}: the run reported no exit code"
    [ "${LAST_EXIT}" = "${HTIF_NATIVE_EXIT}" ] \
        || fail "${label}: exit code ${LAST_EXIT} differs from native ${HTIF_NATIVE_EXIT}"
    [ -s "${log}" ] || fail "${label}: the commit log is empty"
    if ! cmp -s "${HTIF_NATIVE_LOG}" "${log}"; then
        diff -u "${HTIF_NATIVE_LOG}" "${log}" | head -20 || true
        fail "${label}: the commit log differs from native"
    fi
    LAST_GRANTS="$(smoke_grant_count "${pass_output}")"
    LAST_CONSUMED="$(smoke_totals_field "${pass_output}" consumed_fs)"
    LAST_TURNS="$(smoke_turn_count "${pass_output}")"
    LAST_TX="$(htif_field "${pass_output}" b_transport)"
    LAST_READS="$(htif_field "${pass_output}" reads)"
    LAST_WRITES="$(htif_field "${pass_output}" writes)"
    LAST_VALUES="$(htif_field "${pass_output}" values)"
    LAST_RECEIVED="$(htif_field "${pass_output}" received)"
    LAST_CONSOLE="$(htif_field "${pass_output}" console)"
    LAST_SEQUENCE="$(htif_value_sequence "${pass_output}")"
    LAST_CONSOLE_LINES="$(htif_console_sequence "${pass_output}")"
    for fact in "grants:${LAST_GRANTS}" "consumed:${LAST_CONSUMED}" \
        "turns:${LAST_TURNS}" "b_transport:${LAST_TX}" "reads:${LAST_READS}" \
        "writes:${LAST_WRITES}" "values:${LAST_VALUES}" \
        "received:${LAST_RECEIVED}" "console:${LAST_CONSOLE}"; do
        [ -n "${fact#*:}" ] || fail "${label}: the run reported no ${fact%%:*} fact"
    done
    [ "${LAST_TX}" -eq $((LAST_READS + LAST_WRITES)) ] \
        || fail "${label}: the host HTIF served ${LAST_TX} device transaction(s), not its ${LAST_READS} read(s) plus ${LAST_WRITES} write(s)"
    # htif D3: the endpoint never receives a byte from the host.
    [ "${LAST_RECEIVED}" -eq 0 ] \
        || fail "${label}: the host HTIF target received ${LAST_RECEIVED} byte(s); the endpoint has no host-to-guest direction"
    if [ "${target}" = "in-tree" ]; then
        [ "${LAST_TX}" -eq 0 ] \
            || fail "${label}: the in-tree selection reported ${LAST_TX} host HTIF transaction(s); it must report zero"
        [ "${LAST_VALUES}" -eq 0 ] \
            || fail "${label}: the in-tree selection published ${LAST_VALUES} host HTIF value(s); it must report zero"
        [ "${LAST_CONSOLE}" = '""' ] \
            || fail "${label}: the in-tree selection reported console byte(s) ${LAST_CONSOLE}; the in-tree endpoint renders none"
        [ -z "${LAST_CONSOLE_LINES}" ] \
            || fail "${label}: the in-tree selection rendered console byte(s) ${LAST_CONSOLE_LINES}; the in-tree endpoint renders none"
    else
        [ "${LAST_TX}" -gt 0 ] \
            || fail "${label}: the host selection served no device transaction"
        [ "${LAST_VALUES}" -eq 4 ] \
            || fail "${label}: the host selection published ${LAST_VALUES} value(s), not the guest's four"
        [ "${LAST_SEQUENCE}" = "${HTIF_VALUE_SEQUENCE}" ] \
            || fail "${label}: the host selection published values ${LAST_SEQUENCE}, not the guest's three console words and exit word"
        # The exit word is the one value whose publication the ABI can observe:
        # the run stopped as an exit on its own turn, so the sequence's last word
        # must be the exit word the terminal code decodes.
        [ "${LAST_SEQUENCE##* }" = "${HTIF_EXIT_VALUE}" ] \
            || fail "${label}: the host selection's last published value is not the exit word the run stopped on"
        [ "${LAST_CONSOLE}" = "\"${HTIF_CONSOLE}\"" ] \
            || fail "${label}: the host selection rendered console byte(s) ${LAST_CONSOLE}, not exactly Hi\\n"
        [ "${LAST_CONSOLE_LINES}" = "${HTIF_CONSOLE}" ] \
            || fail "${label}: the host selection rendered console byte(s) ${LAST_CONSOLE_LINES}, not exactly Hi\\n"
    fi
    LAST_OUTPUT="${pass_output}"
}

echo -e "${YELLOW}== ${HTIF_GUEST}: the native baseline (htif D8)${NC}"
HTIF_NATIVE_LOG="${OUTDIR}/logs/${HTIF_GUEST}.native.log"
capture htif_native_output htif_native_status \
    timeout "${RUN_TIMEOUT}" "${RUSCV_BIN}" run "${HTIF_ELF}" --isa "${HTIF_PROFILE}" \
    --max-cycles "${MAX_CYCLES}" --log-commits "${HTIF_NATIVE_LOG}"
[ "${htif_native_status}" -ne 124 ] \
    || fail "${HTIF_GUEST}: the native run timed out after ${RUN_TIMEOUT}s"
HTIF_NATIVE_EXIT="$(cli_exit_code "${htif_native_output}")"
if [ "${htif_native_status}" -ne 0 ] || [ -z "${HTIF_NATIVE_EXIT}" ]; then
    printf '%s\n' "${htif_native_output}"
    fail "${HTIF_GUEST}: the native run did not report an exit code"
fi
if ! printf '%s\n' "${htif_native_output}" | grep -q '^Status:     SUCCESS$'; then
    printf '%s\n' "${htif_native_output}"
    fail "${HTIF_GUEST}: the native run did not succeed"
fi
[ -s "${HTIF_NATIVE_LOG}" ] || fail "${HTIF_GUEST}: the native commit log is empty"
echo "  native: exit status ${htif_native_status}, guest exit code ${HTIF_NATIVE_EXIT}"

# `htif_u3_console` runs under both selections at the same options. The in-tree
# pass keeps the Rust endpoint and reports zero host HTIF transactions; the host
# pass serves the same window through the C ABI's device table and must publish
# the guest's three console words and exit word, render exactly `Hi\n`, and keep
# the matching in-tree pass's grant, consumed and turn facts, so both selections
# stop on the same instruction (htif D8).
echo -e "${YELLOW}== ${HTIF_GUEST}: --htif-target in-tree and host (htif D8)${NC}"
for variant in "baseline:" "dmi:--dmi" "nb:--nb" "quantum:--quantum ${FAULT_QUANTUM}"; do
    name="${variant%%:*}"
    flags="${variant#*:}"
    variant_options=()
    if [ -n "${flags}" ]; then
        read -r -a variant_options <<<"${flags}"
    fi
    require_htif_pass "${HTIF_GUEST} --htif-target in-tree (${name})" in-tree \
        "${OUTDIR}/logs/${HTIF_GUEST}.htif-in-tree.${name}.log" \
        ${variant_options[@]+"${variant_options[@]}"}
    in_tree_grants="${LAST_GRANTS}"
    in_tree_consumed="${LAST_CONSUMED}"
    in_tree_turns="${LAST_TURNS}"
    require_htif_pass "${HTIF_GUEST} --htif-target host (${name})" host \
        "${OUTDIR}/logs/${HTIF_GUEST}.htif-host.${name}.log" \
        ${variant_options[@]+"${variant_options[@]}"}
    [ "${LAST_GRANTS}" = "${in_tree_grants}" ] \
        || fail "${HTIF_GUEST}: the host (${name}) pass took ${LAST_GRANTS} grant(s), not the in-tree pass's ${in_tree_grants}"
    [ "${LAST_CONSUMED}" = "${in_tree_consumed}" ] \
        || fail "${HTIF_GUEST}: the host (${name}) pass consumed ${LAST_CONSUMED} fs, not the in-tree pass's ${in_tree_consumed} fs"
    [ "${LAST_TURNS}" = "${in_tree_turns}" ] \
        || fail "${HTIF_GUEST}: the host (${name}) pass stopped after ${LAST_TURNS} turn(s), not the in-tree pass's ${in_tree_turns}"
    if [ "${name}" = "baseline" ]; then
        HTIF_BASELINE_GRANTS="${LAST_GRANTS}"
        HTIF_BASELINE_CONSUMED="${LAST_CONSUMED}"
        HTIF_BASELINE_TURNS="${LAST_TURNS}"
        HTIF_BASELINE_TX="${LAST_TX}"
        HTIF_BASELINE_READS="${LAST_READS}"
        HTIF_BASELINE_WRITES="${LAST_WRITES}"
        HTIF_BASELINE_VALUES="${LAST_VALUES}"
        HTIF_BASELINE_RECEIVED="${LAST_RECEIVED}"
        HTIF_BASELINE_SEQUENCE="${LAST_SEQUENCE}"
        HTIF_BASELINE_CONSOLE_LINES="${LAST_CONSOLE_LINES}"
    fi
    echo -e "  ${GREEN}[PASS]${NC} ${name}: host HTIF transactions ${LAST_TX} = ${LAST_READS} read(s) + ${LAST_WRITES} write(s), values ${LAST_VALUES}, console ${LAST_CONSOLE}"
done

# htif D8: a second host run reports the same transaction, read, write, value
# and console facts and stops on the same turn, so the sequence is the Hart's
# instruction sequence and not a scheduling accident.
require_htif_pass "${HTIF_GUEST} --htif-target host (repeat)" host \
    "${OUTDIR}/logs/${HTIF_GUEST}.htif-host.repeat.log"
[ "${LAST_GRANTS}" = "${HTIF_BASELINE_GRANTS}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass took ${LAST_GRANTS} grant(s), not the first pass's ${HTIF_BASELINE_GRANTS}"
[ "${LAST_CONSUMED}" = "${HTIF_BASELINE_CONSUMED}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass consumed ${LAST_CONSUMED} fs, not the first pass's ${HTIF_BASELINE_CONSUMED} fs"
[ "${LAST_TURNS}" = "${HTIF_BASELINE_TURNS}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass stopped after ${LAST_TURNS} turn(s), not the first pass's ${HTIF_BASELINE_TURNS}"
[ "${LAST_TX}" = "${HTIF_BASELINE_TX}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass served ${LAST_TX} transaction(s), not the first pass's ${HTIF_BASELINE_TX}"
[ "${LAST_READS}" = "${HTIF_BASELINE_READS}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass served ${LAST_READS} read(s), not the first pass's ${HTIF_BASELINE_READS}"
[ "${LAST_WRITES}" = "${HTIF_BASELINE_WRITES}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass served ${LAST_WRITES} write(s), not the first pass's ${HTIF_BASELINE_WRITES}"
[ "${LAST_VALUES}" = "${HTIF_BASELINE_VALUES}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass published ${LAST_VALUES} value(s), not the first pass's ${HTIF_BASELINE_VALUES}"
[ "${LAST_RECEIVED}" = "${HTIF_BASELINE_RECEIVED}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass received ${LAST_RECEIVED} byte(s), not the first pass's ${HTIF_BASELINE_RECEIVED}"
[ "${LAST_SEQUENCE}" = "${HTIF_BASELINE_SEQUENCE}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass published values ${LAST_SEQUENCE}, not the first pass's ${HTIF_BASELINE_SEQUENCE}"
[ "${LAST_CONSOLE_LINES}" = "${HTIF_BASELINE_CONSOLE_LINES}" ] \
    || fail "${HTIF_GUEST}: the repeated host pass rendered console byte(s) ${LAST_CONSOLE_LINES}, not the first pass's ${HTIF_BASELINE_CONSOLE_LINES}"
if ! cmp -s "${OUTDIR}/logs/${HTIF_GUEST}.htif-host.baseline.log" \
    "${OUTDIR}/logs/${HTIF_GUEST}.htif-host.repeat.log"; then
    fail "${HTIF_GUEST}: the repeated host pass's commit log differs from the first"
fi
echo -e "  ${GREEN}[PASS]${NC} a second host run reported identical values, console, transactions and facts"

# The two existing guests under --htif-target host: neither touches the
# endpoint, so both must reproduce their facts with zero HTIF transactions
# (htif D8).
echo -e "${YELLOW}== the existing guests under --htif-target host (htif D8)${NC}"
for entry in "${GUESTS[@]}"; do
    read -r guest profile directory <<<"${entry}"
    elf="${ELF_DIR}/${directory}/${guest}.elf"
    htif_host_log="${OUTDIR}/logs/${guest}.systemc.htif-host.log"
    capture htif_host_output htif_host_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${htif_host_log}" --htif-target host
    [ "${htif_host_status}" -ne 124 ] \
        || fail "${guest}: the --htif-target host run timed out after ${RUN_TIMEOUT}s"
    if [ "${htif_host_status}" -ne 0 ]; then
        printf '%s\n' "${htif_host_output}"
        fail "${guest}: the --htif-target host run exited with status ${htif_host_status}"
    fi
    if ! printf '%s\n' "${htif_host_output}" | grep -q '^systemc-smoke: result state=exited '; then
        printf '%s\n' "${htif_host_output}"
        fail "${guest}: the --htif-target host run did not report a terminal ruscv-sim result"
    fi
    htif_host_exit="$(smoke_exit_code "${htif_host_output}")"
    [ "${htif_host_exit}" = "${NATIVE_EXITS[${guest}]}" ] \
        || fail "${guest}: the --htif-target host exit code ${htif_host_exit} differs from native ${NATIVE_EXITS[${guest}]}"
    if ! cmp -s "${OUTDIR}/logs/${guest}.native.log" "${htif_host_log}"; then
        diff -u "${OUTDIR}/logs/${guest}.native.log" "${htif_host_log}" | head -20 || true
        fail "${guest}: the --htif-target host commit log differs from native"
    fi
    htif_host_transactions="$(htif_field "${htif_host_output}" b_transport)"
    [ -n "${htif_host_transactions}" ] \
        || fail "${guest}: the --htif-target host run reported no HTIF transaction count"
    [ "${htif_host_transactions}" -eq 0 ] \
        || fail "${guest}: the --htif-target host run served ${htif_host_transactions} HTIF transaction(s) although the guest never touches the endpoint"
    htif_host_values="$(htif_field "${htif_host_output}" values)"
    [ -n "${htif_host_values}" ] \
        || fail "${guest}: the --htif-target host run reported no HTIF value count"
    [ "${htif_host_values}" -eq 0 ] \
        || fail "${guest}: the --htif-target host run published ${htif_host_values} HTIF value(s) although the guest never touches the endpoint"
    echo -e "  ${GREEN}[PASS]${NC} ${guest}: native exit code and commit log with zero HTIF transactions"
done

# htif §4: --fault-htif-drop-event answers the exit word's write with
# RUSCV_TLM_OK and no event, while the three console writes keep their events.
# No exit fact exists, so the guest's self-loop exhausts the turn bound.  The
# run stop is the ABI's observable sink effect, so the exhausted bound proves
# that no exit fact reached it; the three console values are the target's
# accepted reports, which its own summary and the byte-identical commit log
# fix.
echo -e "${YELLOW}== --fault-htif-drop-event (must not pass)${NC}"
capture drop_event_output drop_event_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${HTIF_ELF}" --isa "${HTIF_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --htif-target host --fault-htif-drop-event
[ "${drop_event_status}" -ne 124 ] || fail "the --fault-htif-drop-event control timed out"
if [ "${drop_event_status}" -eq 0 ]; then
    printf '%s\n' "${drop_event_output}"
    fail "the --fault-htif-drop-event control passed; a dropped exit event must exhaust the turn bound"
fi
if ! printf '%s\n' "${drop_event_output}" \
    | grep -q "exceeded the ${MAX_CYCLES} turn limit"; then
    printf '%s\n' "${drop_event_output}"
    fail "the --fault-htif-drop-event control did not exhaust the turn limit"
fi
if printf '%s\n' "${drop_event_output}" | grep -q '^systemc-smoke: result state='; then
    printf '%s\n' "${drop_event_output}"
    fail "the --fault-htif-drop-event control reported a run result after the exhausted bound"
fi
drop_event_values="$(htif_field "${drop_event_output}" values)"
[ -n "${drop_event_values}" ] \
    || fail "the --fault-htif-drop-event control reported no HTIF value count"
[ "${drop_event_values}" -eq 3 ] \
    || fail "the --fault-htif-drop-event control published ${drop_event_values} value(s), not the three console values"
drop_event_sequence="$(htif_value_sequence "${drop_event_output}")"
[ "${drop_event_sequence}" = "${HTIF_CONSOLE_VALUES}" ] \
    || fail "the --fault-htif-drop-event control published values ${drop_event_sequence}, not exactly the three console words"
drop_event_console="$(htif_console_sequence "${drop_event_output}")"
[ "${drop_event_console}" = "${HTIF_CONSOLE}" ] \
    || fail "the --fault-htif-drop-event control rendered console byte(s) ${drop_event_console}, not exactly Hi\\n"
echo -e "  ${GREEN}[PASS]${NC} the dropped exit event kept the console sequence and exhausted the turn limit (status ${drop_event_status})"

# htif §4: --fault-htif-wait waits one Platform tick inside `b_transport` and
# reports the interval as host elapsed time, which htif D9 forbids: host time may
# not pass inside a device callback. The guest's own read is the first access, so
# no value reaches the sink.
echo -e "${YELLOW}== --fault-htif-wait (must not pass)${NC}"
capture htif_wait_output htif_wait_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${HTIF_ELF}" --isa "${HTIF_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --htif-target host --fault-htif-wait
[ "${htif_wait_status}" -ne 124 ] || fail "the --fault-htif-wait control timed out"
if [ "${htif_wait_status}" -eq 0 ]; then
    printf '%s\n' "${htif_wait_output}"
    fail "the --fault-htif-wait control passed; host time inside a device callback must fail"
fi
if ! printf '%s\n' "${htif_wait_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${htif_wait_output}"
    fail "the --fault-htif-wait control did not report a failed run"
fi
if ! printf '%s\n' "${htif_wait_output}" \
    | grep -q 'host elapsed time for .* on the HTIF window'; then
    printf '%s\n' "${htif_wait_output}"
    fail "the --fault-htif-wait control's diagnostic did not name host elapsed time on the HTIF window"
fi
if ! printf '%s\n' "${htif_wait_output}" \
    | grep -q 'host time may not pass inside a device callback'; then
    printf '%s\n' "${htif_wait_output}"
    fail "the --fault-htif-wait control's diagnostic did not name the device callback rule"
fi
htif_wait_values="$(htif_field "${htif_wait_output}" values)"
[ -n "${htif_wait_values}" ] \
    || fail "the --fault-htif-wait control reported no HTIF value count"
[ "${htif_wait_values}" -eq 0 ] \
    || fail "the --fault-htif-wait control published ${htif_wait_values} value(s); no write runs after the failed first access"
htif_wait_console="$(htif_console_sequence "${htif_wait_output}")"
[ -z "${htif_wait_console}" ] \
    || fail "the --fault-htif-wait control rendered console byte(s) ${htif_wait_console}"
echo -e "  ${GREEN}[PASS]${NC} the device wait failed the run as a protocol failure (status ${htif_wait_status})"

# htif §4: --fault-htif-event-on-fault reports the exit write's event with a
# fault status, which htif D4 forbids: only an accepted access publishes an
# event.  The run must fail as a protocol failure of that access, and the
# refused write must be absent from the target's accepted report sequence.  The
# ABI's failure classification runs before its exit decode, so this smoke does
# not observe the refused write's sink; that publication rule is the Rust
# validation rule (proven in `src/machine/tests.rs`) and is not claimed here.
echo -e "${YELLOW}== --fault-htif-event-on-fault (must not pass)${NC}"
capture htif_event_output htif_event_status \
    timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${HTIF_ELF}" --isa "${HTIF_PROFILE}" \
    --max-turns "${MAX_CYCLES}" --htif-target host --fault-htif-event-on-fault
[ "${htif_event_status}" -ne 124 ] || fail "the --fault-htif-event-on-fault control timed out"
if [ "${htif_event_status}" -eq 0 ]; then
    printf '%s\n' "${htif_event_output}"
    fail "the --fault-htif-event-on-fault control passed; an event on a fault status must fail"
fi
if ! printf '%s\n' "${htif_event_output}" | grep -q '^systemc-smoke: result state=failed '; then
    printf '%s\n' "${htif_event_output}"
    fail "the --fault-htif-event-on-fault control did not report a failed run"
fi
if ! printf '%s\n' "${htif_event_output}" \
    | grep -q 'only an accepted access publishes an event'; then
    printf '%s\n' "${htif_event_output}"
    fail "the --fault-htif-event-on-fault control did not fail as a protocol failure of that access"
fi
htif_event_values="$(htif_field "${htif_event_output}" values)"
[ -n "${htif_event_values}" ] \
    || fail "the --fault-htif-event-on-fault control reported no HTIF value count"
[ "${htif_event_values}" -eq 3 ] \
    || fail "the --fault-htif-event-on-fault control published ${htif_event_values} value(s), not the three console values"
htif_event_sequence="$(htif_value_sequence "${htif_event_output}")"
[ "${htif_event_sequence}" = "${HTIF_CONSOLE_VALUES}" ] \
    || fail "the --fault-htif-event-on-fault control published values ${htif_event_sequence}, not exactly the three console words"
htif_event_console="$(htif_console_sequence "${htif_event_output}")"
[ "${htif_event_console}" = "${HTIF_CONSOLE}" ] \
    || fail "the --fault-htif-event-on-fault control rendered console byte(s) ${htif_event_console}, not exactly Hi\\n"
echo -e "  ${GREEN}[PASS]${NC} the fault-status event failed the access; the refused write is absent from the accepted sequence (status ${htif_event_status})"

printf '\n============================================\n'
printf '  SystemC smoke summary\n'
printf '============================================\n'
printf '  Guests compared:      %s\n' "${compared}"
printf '  SystemC kernel:       3.0.2 (pinned)\n'
printf '  Native comparison:    exit code and commit log per guest\n'
printf '  Direct memory:        --dmi exit/log parity, b_transport strictly below\n'
printf '  Invalidation:         --dmi-invalidate-every %s, unchanged commit log\n' \
    "${INVALIDATE_EVERY}"
printf '  Quantum passes:       --quantum %s and %s, with and without --dmi\n' \
    "${QUANTUMS[0]}" "${QUANTUMS[1]}"
printf '  AT passes:            five paths at lambda 0 and %s fs, with --dmi parity\n' \
    "${NB_LATENCY}"
printf '  Refusals:             --nb --quantum and the non-blocking usage errors\n'
printf '  Fault controls:       the exceeded maximum, the unknown completion, the\n'
printf '                        five non-blocking options and the debug wait\n'
printf '  UART target:          --uart-target in-tree|host, exactly Hello!\\n per\n'
printf '                        host pass and zero host device transactions in-tree\n'
printf '  UART fault controls:  --fault-uart-no-lsr, --fault-uart-wait and\n'
printf '                        --fault-uart-event-on-fault, each refused\n'
printf '  HTIF target:          --htif-target in-tree|host, the guest value and\n'
printf '                        console sequences and the matching in-tree facts\n'
printf '  HTIF fault controls:  --fault-htif-drop-event, --fault-htif-wait and\n'
printf '                        --fault-htif-event-on-fault, each refused\n'
printf '  Early sc_stop:        refused, as required\n\n'
echo -e "${GREEN}[PASS]${NC} the pinned SystemC host matched native for every smoke guest"
