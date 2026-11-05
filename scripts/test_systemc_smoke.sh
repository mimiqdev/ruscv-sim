#!/bin/bash
# Build and run the pinned-SystemC smoke (dev-plan D9/D10; direct-memory D13).
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
#      a terminal ruscv-sim result is never a run result.
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

# Each guest's native exit code, for the later pass comparisons.
declare -A NATIVE_EXITS=()

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
    capture smoke_output smoke_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${smoke_log}"
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
    echo "  systemc: exit status ${smoke_status}, guest exit code ${smoke_exit}, b_transport ${transactions}"

    if [ "${smoke_exit}" != "${native_exit}" ]; then
        fail "${guest}: SystemC exit code ${smoke_exit} differs from native ${native_exit}"
    fi
    [ -s "${smoke_log}" ] || fail "${guest}: the SystemC commit log is empty"
    if ! cmp -s "${native_log}" "${smoke_log}"; then
        diff -u "${native_log}" "${smoke_log}" | head -20 || true
        fail "${guest}: the SystemC commit log differs from native"
    fi
    compared=$((compared + 1))
    echo -e "  ${GREEN}[PASS]${NC} exit code and commit log match native"

    # The direct-memory pass (direct-memory D13/U5): the same guest runs with
    # the table's direct-memory query offered, so hinted accesses copy through
    # the target's granted chunk pointers instead of `b_transport`. The exit
    # code and commit log must match native exactly, and the RAM target's
    # `b_transport` count must stay strictly below the non-direct pass: a run
    # that never uses a granted pointer fails here.
    direct_log="${OUTDIR}/logs/${guest}.systemc.dmi.log"
    capture direct_output direct_status \
        timeout "${RUN_TIMEOUT}" "${SMOKE_BIN}" "${elf}" --isa "${profile}" \
        --max-turns "${MAX_CYCLES}" --log-commits "${direct_log}" --dmi
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
    echo -e "  ${GREEN}[PASS]${NC} --dmi matches native with b_transport ${direct_transactions} < ${transactions}"
done

# The scheduled-invalidation pass (direct-memory D8/U5): every grant is
# invalidated after each second Machine grant through the target's backward
# call and `ruscv_machine_invalidate_direct_mem`, so the next access re-queries
# and re-grants. The commit log must stay unchanged, which is the contract's
# "same facts after invalidation" rule. The scheduled count must equal
# grants/N exactly and the forwarded count must cover it, which proves the
# schedule really ran and really reached the handle's registry. A one-turn
# grant budget makes the run span several grants, so the schedule fires more
# than once.
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

printf '\n============================================\n'
printf '  SystemC smoke summary\n'
printf '============================================\n'
printf '  Guests compared:      %s\n' "${compared}"
printf '  SystemC kernel:       3.0.2 (pinned)\n'
printf '  Native comparison:    exit code and commit log per guest\n'
printf '  Direct memory:        --dmi exit/log parity, b_transport strictly below\n'
printf '  Invalidation:         --dmi-invalidate-every %s, unchanged commit log\n' \
    "${INVALIDATE_EVERY}"
printf '  Early sc_stop:        refused, as required\n\n'
echo -e "${GREEN}[PASS]${NC} the pinned SystemC host matched native for every smoke guest"
