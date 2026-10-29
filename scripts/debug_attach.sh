#!/usr/bin/env bash
# Stage 6 U5. One OpenOCD 0.12.0 + GDB session performs every §7 item 5 row
# against `run --rbb-port --halted`, then the same session against Spike.
# Two negative mutations follow: dmstatus.version 4, and datacount 2.
#
# Program-buffer U4 adds a second session that forces OpenOCD's program-buffer
# memory path by enabling no other method. It transfers four adjacent values at
# 8, 4, 2, and 1 bytes, reads every address back through abstract Access
# Memory, faults on an unmapped store, and recovers. Spike runs the same
# session with matching capacity, implicit EBREAK, and datacount, plus two
# capacity controls that must make the session fail closed.
#
# Named outputs are the S6 and U4 lines in the GDB logs. The comparisons are
# those values, not a hash. dmstatus.version is not one of them. dcsr is
# compared after setting aside two documented differences: D5 forces
# stopcount, stoptime, and mprven to 1, and this profile's --halted is haltreq
# (cause 3). Spike hardwires the three D5 bits to 0 and enters with cause 5.

set -euo pipefail
# OpenOCD 0.12.0 aborts after the target drops the socket. Without this,
# bash job control prints "Aborted" for that expected SIGABRT.
set +m

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
CFG="${ROOT}/scripts/debug/openocd-remote-bitbang.cfg"
GDB_SCRIPT="${ROOT}/scripts/debug/s6-u5-session.gdb"
GDB_PROGBUF_SCRIPT="${ROOT}/scripts/debug/s6-u4-progbuf.gdb"
GUEST_SRC="${ROOT}/tests/bare-metal-riscv-test/rv64i/s6_u5_attach.S"
LINKER="${ROOT}/tests/bare-metal-riscv-test/linker.ld"

EVIDENCE=""
REQUIRE_SPIKE=0
SKIP_SPIKE=0

usage() {
    echo "usage: scripts/debug_attach.sh [--evidence DIR] [--require-spike] [--skip-spike]" >&2
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --evidence)
            EVIDENCE="${2:-}"
            shift 2
            ;;
        --require-spike) REQUIRE_SPIKE=1; shift ;;
        --skip-spike) SKIP_SPIKE=1; shift ;;
        -h|--help) usage ;;
        *) usage ;;
    esac
done

if ! command -v openocd >/dev/null 2>&1 || ! command -v gdb-multiarch >/dev/null 2>&1; then
    echo "openocd and gdb-multiarch are required" >&2
    exit 1
fi
if ! openocd --version 2>&1 | head -1 | grep -q '0\.12\.0'; then
    echo "OpenOCD 0.12.0 is required; found: $(openocd --version 2>&1 | head -1)" >&2
    exit 1
fi

AS="${RISCV_PREFIX:-riscv64-unknown-elf-}as"
LD="${RISCV_PREFIX:-riscv64-unknown-elf-}ld"
NM="${RISCV_PREFIX:-riscv64-unknown-elf-}nm"
if ! command -v "${AS}" >/dev/null 2>&1 || ! command -v "${LD}" >/dev/null 2>&1 || ! command -v "${NM}" >/dev/null 2>&1; then
    echo "RISC-V binutils are required (${AS}, ${LD}, ${NM})" >&2
    exit 1
fi

if [[ -z "${RUSCV_SIM:-}" ]]; then
    cargo_target="${CARGO_TARGET_DIR:-${ROOT}/target}"
    if [[ "${cargo_target}" != /* ]]; then
        cargo_target="${ROOT}/${cargo_target}"
    fi
    if [[ ! -x "${cargo_target}/debug/ruscv-sim" ]]; then
        cargo_args=(build)
        if [[ "${RUSCV_CARGO_OFFLINE:-}" == 1 ]]; then
            cargo_args+=(--offline)
        fi
        (cd "${ROOT}" && cargo "${cargo_args[@]}")
    fi
    RUSCV_SIM="${cargo_target}/debug/ruscv-sim"
fi
if [[ ! -x "${RUSCV_SIM}" ]]; then
    echo "simulator binary is not executable: ${RUSCV_SIM}" >&2
    exit 1
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/s6-u5.XXXXXX")"
PIDS=()
cleanup() {
    local pid
    for pid in "${PIDS[@]+"${PIDS[@]}"}"; do
        if kill -0 "${pid}" 2>/dev/null; then
            kill "${pid}" 2>/dev/null || true
            wait "${pid}" 2>/dev/null || true
        fi
    done
}
trap cleanup EXIT

"${AS}" -march=rv64ima_zicsr -mabi=lp64 "${GUEST_SRC}" -o "${WORK}/guest.o"
"${LD}" -T "${LINKER}" "${WORK}/guest.o" -o "${WORK}/guest.elf"

symbol() {
    local name="$1"
    "${NM}" "${WORK}/guest.elf" | awk -v name="${name}" '$3 == name { print $1; exit }'
}

ENTRY="$(symbol _start)"
BREAK_AT="$(symbol s6_break)"
SPIN_AT="$(symbol s6_spin)"
if [[ -z "${ENTRY}" || -z "${BREAK_AT}" || -z "${SPIN_AT}" ]]; then
    echo "guest is missing _start, s6_break, or s6_spin" >&2
    exit 1
fi
for array in s6_pb64 s6_pb32 s6_pb16 s6_pb8; do
    if [[ -z "$(symbol "${array}")" ]]; then
        echo "guest is missing ${array}" >&2
        exit 1
    fi
done

norm() {
    local value="${1#0x}"
    value="${value#0X}"
    value="$(printf '%s' "${value}" | tr 'A-F' 'a-f' | sed 's/^0*//')"
    printf '%s' "${value:-0}"
}

# Debug 1.0 dcsr. `dcsr_common` clears the bits this profile and Spike are
# allowed to disagree on: cause (8:6), stopcount (10), stoptime (9), mprven (4).
append_dcsr_fields() {
    local named="$1"
    python3 - "$(field "${named}" dcsr)" >>"${named}" <<'PY'
import sys
raw = int(sys.argv[1], 16)

def bit(n):
    return (raw >> n) & 1

def field(lo, width):
    return (raw >> lo) & ((1 << width) - 1)

clear = (0x7 << 6) | (1 << 10) | (1 << 9) | (1 << 4)
rows = [
    ("dcsr_debugver", field(28, 4)),
    ("dcsr_ebreakm", bit(15)),
    ("dcsr_ebreaks", bit(13)),
    ("dcsr_ebreaku", bit(12)),
    ("dcsr_stopcount", bit(10)),
    ("dcsr_stoptime", bit(9)),
    ("dcsr_cause", field(6, 3)),
    ("dcsr_mprven", bit(4)),
    ("dcsr_step", bit(2)),
    ("dcsr_prv", field(0, 2)),
    ("dcsr_common", raw & ~clear & ((1 << 64) - 1)),
]
for key, value in rows:
    print(f"{key}={value:x}")
PY
}

field() {
    local file="$1" key="$2"
    awk -F= -v key="${key}" '$1 == key { print substr($0, index($0, "=") + 1); exit }' "${file}"
}

free_port() {
    python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

wait_for() {
    local file="$1" pattern="$2" pid="$3" tries="${4:-100}"
    local i
    for ((i = 0; i < tries; i++)); do
        if grep -q "${pattern}" "${file}" 2>/dev/null; then
            return 0
        fi
        if ! kill -0 "${pid}" 2>/dev/null; then
            echo "process ${pid} exited before ${pattern}" >&2
            cat "${file}" >&2 || true
            return 1
        fi
        sleep 0.1
    done
    echo "timed out waiting for ${pattern}" >&2
    cat "${file}" >&2 || true
    return 1
}

# Start one target, attach OpenOCD and GDB, run $3 until its log shows $4,
# and stop GDB once the target exits. $2 is a function that execs the target
# and takes the remote_bitbang port. Logs stay in ${WORK}/$1. Remaining
# arguments go to OpenOCD after the config file.
start_session() {
    local name="$1"
    local starter="$2"
    local gdb_script="$3"
    local ready="$4"
    shift 4
    local openocd_args=("$@")
    local dir="${WORK}/${name}"
    mkdir -p "${dir}"
    local rbb gdb_port
    rbb="$(free_port)"
    gdb_port="$(free_port)"

    local target_log="${dir}/target.log"
    local openocd_log="${dir}/openocd.log"
    local gdb_log="${dir}/gdb.log"

    "${starter}" "${rbb}" >"${target_log}" 2>&1 &
    local target_pid=$!
    PIDS+=("${target_pid}")
    wait_for "${target_log}" "Listening for remote bitbang connection on port" "${target_pid}"

    openocd \
        -l "${openocd_log}" \
        -c "set RBB_PORT ${rbb}" \
        -c "set GDB_PORT ${gdb_port}" \
        -f "${CFG}" \
        "${openocd_args[@]+"${openocd_args[@]}"}" \
        >"${dir}/openocd.stdout" 2>&1 &
    local openocd_pid=$!
    PIDS+=("${openocd_pid}")
    if ! wait_for "${openocd_log}" "Examined RISC-V core" "${openocd_pid}" 150; then
        echo "${name}: OpenOCD did not examine the hart" >&2
        cat "${openocd_log}" >&2 || true
        return 1
    fi

    # GDB does not exit after the guest does. OpenOCD 0.12 keeps scanning
    # DMI once the bitbang socket drops, and `continue` never returns.
    # Stop GDB after the named outputs and the target exit are recorded.
    gdb-multiarch -batch -nx \
        -ex "set pagination off" \
        -ex "set confirm off" \
        -ex "set architecture riscv:rv64" \
        -ex "file ${WORK}/guest.elf" \
        -ex "target remote :${gdb_port}" \
        -x "${gdb_script}" \
        >"${gdb_log}" 2>&1 &
    local gdb_pid=$!
    PIDS+=("${gdb_pid}")

    local tries saw_reset=0
    for ((tries = 0; tries < 900; tries++)); do
        if grep -q "${ready}" "${gdb_log}" 2>/dev/null; then
            saw_reset=1
            break
        fi
        if ! kill -0 "${gdb_pid}" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    if [[ "${saw_reset}" -ne 1 ]]; then
        echo "${name}: GDB did not reach monitor reset halt" >&2
        head -40 "${gdb_log}" >&2 || true
        head -40 "${openocd_log}" >&2 || true
        return 1
    fi

    for ((tries = 0; tries < 100; tries++)); do
        if ! kill -0 "${target_pid}" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    if kill -0 "${target_pid}" 2>/dev/null; then
        echo "${name}: target still running after reset halt" >&2
        cat "${gdb_log}" >&2 || true
        kill "${target_pid}" 2>/dev/null || true
        wait "${target_pid}" 2>/dev/null || true
        return 1
    fi
    sleep 0.3
    if kill -0 "${gdb_pid}" 2>/dev/null; then
        kill "${gdb_pid}" 2>/dev/null || true
        wait "${gdb_pid}" 2>/dev/null || true
    fi
    local gdb_status="stopped-after-exit"
    set +e
    wait "${target_pid}"
    local target_status=$?
    set -e

    # Drop the finished pid so cleanup does not wait on it again.
    local kept=() pid
    for pid in "${PIDS[@]+"${PIDS[@]}"}"; do
        if [[ "${pid}" != "${target_pid}" ]]; then
            kept+=("${pid}")
        fi
    done
    PIDS=("${kept[@]+"${kept[@]}"}")

    printf 'process_status=%s\n' "${target_status}" >>"${target_log}"
    if [[ "${target_status}" -ne 0 ]]; then
        echo "${name}: target exited ${target_status}, expected 0" >&2
        cat "${target_log}" >&2 || true
        cat "${gdb_log}" >&2 || true
        return 1
    fi
    if grep -q 'continue_returned=unexpected' "${gdb_log}"; then
        echo "${name}: continue returned while the target was still there" >&2
        return 1
    fi
    if ! grep -q "${ready}" "${gdb_log}"; then
        echo "${name}: GDB log has no reset PC (gdb status ${gdb_status})" >&2
        cat "${gdb_log}" >&2 || true
        cat "${openocd_log}" >&2 || true
        return 1
    fi
    # Give OpenOCD a moment to notice the socket drop.
    sleep 0.5
    if ! grep -E -q 'Connection reset by peer|Broken pipe|remote_bitbang_fill_buf' "${openocd_log}" "${dir}/openocd.stdout"; then
        echo "${name}: OpenOCD log does not show the target going away" >&2
        cat "${openocd_log}" >&2 || true
        return 1
    fi
    # OpenOCD keeps scanning after the socket drops. Stop it so the saved
    # log ends at the disconnect instead of repeating until the script exits.
    if kill -0 "${openocd_pid}" 2>/dev/null; then
        kill "${openocd_pid}" 2>/dev/null || true
        wait "${openocd_pid}" 2>/dev/null || true
    fi

    echo "${name}: session reached ${ready} (target exit 0, gdb status ${gdb_status})"
}

# Compare one named value with its expectation.
assert_eq() {
    local name="$1" got="$2" expect="$3" label="$4"
    if [[ "$(norm "${got}")" != "$(norm "${expect}")" ]]; then
        echo "${name}: ${label} is ${got}, expected ${expect}" >&2
        return 1
    fi
}

# Read "$prefix key" lines from a GDB log into a named-values file. Values are
# normalized for the S6 session and kept as written for the U4 session, whose
# readbacks carry "value@address".
extract_named() {
    local named="$1" gdb_log="$2" prefix="$3" mode="$4"
    shift 4
    : >"${named}"
    local key raw
    for key in "$@"; do
        raw="$(field "${gdb_log}" "${prefix} ${key}")"
        if [[ -z "${raw}" ]]; then
            echo "${prefix}: missing ${key}" >&2
            cat "${gdb_log}" >&2 || true
            return 1
        fi
        if [[ "${mode}" == "norm" ]]; then
            printf '%s=%s\n' "${key}" "$(norm "${raw}")" >>"${named}"
        else
            printf '%s=%s\n' "${key}" "${raw}" >>"${named}"
        fi
    done
}

# The abstract Access Memory session: discovery, stepping, breakpoint,
# register and RAM access, resume and halt, reset, and public exit.
run_session() {
    local name="$1"
    local named="$3"
    start_session "${name}" "$2" "${GDB_SCRIPT}" 'S6 reset_pc=' || return 1
    extract_named "${named}" "${WORK}/${name}/gdb.log" S6 norm \
        connect pc gpr mstatus dcsr step_pc step_gpr break_pc observe_gpr ram halt_pc reset_pc
    append_dcsr_fields "${named}"

    assert_eq "${name}" "$(field "${named}" connect)" "ok" "connect" || return 1
    if [[ "${name}" == "spike" ]]; then
        assert_eq "${name}" "$(field "${named}" pc)" "1000" "connect PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" pc)" "${ENTRY}" "connect PC" || return 1
    fi
    assert_eq "${name}" "$(field "${named}" gpr)" "0" "connect GPR" || return 1
    assert_eq "${name}" "$(field "${named}" step_gpr)" "42" "single-step GPR" || return 1
    assert_eq "${name}" "$(field "${named}" step_pc)" "$(printf '%x' "$((0x${ENTRY} + 4))")" "single-step PC" || return 1
    assert_eq "${name}" "$(field "${named}" break_pc)" "${BREAK_AT}" "breakpoint PC" || return 1
    assert_eq "${name}" "$(field "${named}" observe_gpr)" "1235" "register after step" || return 1
    assert_eq "${name}" "$(field "${named}" ram)" "1122334455667788" "RAM readback" || return 1
    assert_eq "${name}" "$(field "${named}" halt_pc)" "${SPIN_AT}" "halt PC" || return 1
    if [[ "${name}" == "spike" ]]; then
        assert_eq "${name}" "$(field "${named}" reset_pc)" "1000" "reset PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" reset_pc)" "${ENTRY}" "reset PC" || return 1
    fi

    echo "${name}: session ok (target exit 0)"
}

# The U4 named outputs, in the order the session prints them.
u4_keys() {
    printf 'connect bulk64 width32 width16 width8 fault recovery s0 s1 halt_pc reset_pc'
    local array index
    for array in s6_pb64 s6_pb32 s6_pb16 s6_pb8; do
        for index in 0 1 2 3; do
            printf ' %s_%s' "${array}" "${index}"
        done
    done
}

# The forced program-buffer session. OpenOCD has no other memory-access
# method, so a transfer it reports here cannot have used abstract memory.
run_progbuf_session() {
    local name="$1"
    local named="$3"
    start_session "${name}" "$2" "${GDB_PROGBUF_SCRIPT}" 'U4 reset_pc=' \
        -d3 -c "riscv set_mem_access progbuf" || return 1
    local keys=()
    read -r -a keys <<<"$(u4_keys)"
    extract_named "${named}" "${WORK}/${name}/gdb.log" U4 raw "${keys[@]}" || return 1
    assert_progbuf_session "${name}" "${named}" || return 1
    assert_progbuf_evidence "${name}" "${WORK}/${name}" || return 1
    echo "${name}: program-buffer session ok (target exit 0)"
}

# Each readback value is checked at its own symbol-derived address against the
# constants the GDB script wrote, so a repeated value, a stale value, or a
# wrong increment fails.
assert_progbuf_session() {
    local name="$1" named="$2"
    if [[ "$(field "${named}" connect)" != "ok" ]]; then
        echo "${name}: program-buffer session did not connect" >&2
        return 1
    fi
    local key
    for key in bulk64 width32 width16 width8 recovery; do
        if [[ "$(field "${named}" "${key}")" != "written" ]]; then
            echo "${name}: the ${key} transfer did not complete" >&2
            return 1
        fi
    done
    if [[ "$(field "${named}" fault)" != "refused" ]]; then
        echo "${name}: an unmapped program-buffer store was not refused" >&2
        return 1
    fi
    if [[ "$(field "${named}" s0)" != "0x5a5a5a5a5a5a5a5a" \
        || "$(field "${named}" s1)" != "0xa5a5a5a5a5a5a5a5" ]]; then
        echo "${name}: a program-buffer register was not restored" >&2
        return 1
    fi
    local symbol width values value
    while read -r symbol width values; do
        local base index=0
        base="$(symbol "${symbol}")"
        for value in ${values}; do
            local address want got
            address="$(printf '0x%x' "$((0x${base} + index * width))")"
            want="${value}@${address}"
            got="$(field "${named}" "${symbol}_${index}")"
            if [[ "${got}" != "${want}" ]]; then
                echo "${name}: ${symbol}_${index} is ${got}, expected ${want}" >&2
                return 1
            fi
            index=$((index + 1))
        done
    done <<'TABLE'
s6_pb64 8 0x1111111111111111 0x2222222222222222 0x3333333333333333 0x4444444444444444
s6_pb32 4 0x11223344 0x55667788 0x99aabbcc 0xddeeff00
s6_pb16 2 0x1234 0x5678 0x9abc 0xdef0
s6_pb8 1 0x5a 0xa5 0x3c 0xc3
TABLE
    assert_eq "${name}" "$(field "${named}" halt_pc)" "${SPIN_AT}" "halt PC" || return 1
    if [[ "${name}" == spike-* ]]; then
        assert_eq "${name}" "$(field "${named}" reset_pc)" "1000" "reset PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" reset_pc)" "${ENTRY}" "reset PC" || return 1
    fi
}

# The OpenOCD log must record program-buffer transfers for every width and
# increment, both fences, no successful abstract memory write before the
# readback switch, and an abstract read for every checked address afterwards.
assert_progbuf_evidence() {
    local name="$1" dir="$2"
    local log="${dir}/openocd.log"
    local pattern
    for pattern in \
        'write_memory_progbuf(): writing 4 words of 8 bytes' \
        'write_memory_progbuf(): writing 1 words of 4 bytes' \
        'write_memory_progbuf(): writing 1 words of 2 bytes' \
        'write_memory_progbuf(): writing 1 words of 1 bytes' \
        'DASM(0x00943023)' 'DASM(0x00840413)' \
        'DASM(0x00942023)' 'DASM(0x00440413)' \
        'DASM(0x00941023)' 'DASM(0x00240413)' \
        'DASM(0x00940023)' 'DASM(0x00140413)' \
        'DASM(0x0000100f)' 'DASM(0x0000000f)' \
        'Succeeded to write memory via program buffer'
    do
        if ! grep -aq -- "${pattern}" "${log}"; then
            echo "${name}: OpenOCD log lacks [${pattern}]" >&2
            return 1
        fi
    done
    # The two-word buffer must not carry a program that needs more words.
    if grep -aq 'Unable to insert instruction' "${log}"; then
        echo "${name}: a program needed more words than the buffer holds" >&2
        return 1
    fi
    # The unmapped store is the only program-buffer write that may fail.
    if [[ "$(grep -ac -- 'Failed to write memory via program buffer' "${log}")" != 1 ]]; then
        echo "${name}: the deliberate program-buffer failure is not the only one" >&2
        return 1
    fi
    local switch
    switch="$(grep -an -- 'command - riscv set_mem_access abstract$' "${log}" | head -1 | cut -d: -f1)"
    if [[ -z "${switch}" ]]; then
        echo "${name}: OpenOCD log does not show the switch to abstract memory" >&2
        return 1
    fi
    local line
    while read -r line; do
        if [[ "${line}" -lt "${switch}" ]]; then
            echo "${name}: a write succeeded through abstract memory before the switch" >&2
            return 1
        fi
    done < <(grep -an -- 'Succeeded to write memory via abstract access' "${log}" | cut -d: -f1)
    local symbol width index address
    while read -r symbol width; do
        local base
        base="$(symbol "${symbol}")"
        for index in 0 1 2 3; do
            address="$(printf '0x%x' "$((0x${base} + index * width))")"
            if ! awk -v start="${switch}" \
                -v pattern="read_memory_abstract(): reading 1 words of ${width} bytes from ${address}" \
                'NR > start && index($0, pattern) { found = 1 } END { exit found ? 0 : 1 }' "${log}"; then
                echo "${name}: no abstract read of ${width} bytes at ${address}" >&2
                return 1
            fi
        done
    done <<'TABLE'
s6_pb64 8
s6_pb32 4
s6_pb16 2
s6_pb8 1
TABLE
}

ruscv_cmd() {
    local port="$1"
    exec "${RUSCV_SIM}" run "${WORK}/guest.elf" --rbb-port "${port}" --halted --max-cycles 100000000
}

spike_available() {
    command -v spike >/dev/null 2>&1 || return 1
    local help
    help="$(spike --help 2>&1 || true)"
    grep -q -- '--dm-datacount' <<<"${help}" \
        && grep -q -- '--halted' <<<"${help}" \
        && grep -q -- '--dm-progsize' <<<"${help}" \
        && grep -q -- '--dm-no-impebreak' <<<"${help}"
}

spike_cmd() {
    local port="$1"
    # Spike's default reset PC is its boot ROM at 0x1000. This profile has
    # no boot ROM: --halted stops at the ELF entry. --pc puts Spike on
    # that same instruction. The debug-module flags stay the §7 set.
    exec spike --halted \
        --isa=rv64ima_zicsr \
        --priv=msu \
        --pc="0x${ENTRY}" \
        --dm-progsize=0 \
        --dm-no-impebreak \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/guest.elf"
}

# The U4 comparison target: two writable words, the implicit EBREAK, and
# datacount four, which is this profile's capacity.
spike_progbuf_cmd() {
    local port="$1"
    exec spike --halted \
        --isa=rv64ima_zicsr \
        --priv=msu \
        --pc="0x${ENTRY}" \
        --dm-progsize=2 \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/guest.elf"
}

# One writable word plus the implicit EBREAK is two slots; OpenOCD needs three.
spike_one_word_progbuf() {
    local port="$1"
    exec spike --halted \
        --isa=rv64ima_zicsr \
        --priv=msu \
        --pc="0x${ENTRY}" \
        --dm-progsize=1 \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/guest.elf"
}

# Two writable words without the implicit EBREAK are also two slots.
spike_no_implicit_ebreak() {
    local port="$1"
    exec spike --halted \
        --isa=rv64ima_zicsr \
        --priv=msu \
        --pc="0x${ENTRY}" \
        --dm-progsize=2 \
        --dm-no-impebreak \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/guest.elf"
}

run_negative_version() {
    local dir="${WORK}/negative-version"
    mkdir -p "${dir}"
    local port
    port="$(free_port)"
    RUSCV_S6_DMSTATUS_VERSION=4 \
        "${RUSCV_SIM}" run "${WORK}/guest.elf" --rbb-port "${port}" --halted --max-cycles 100000000 \
        >"${dir}/target.log" 2>&1 &
    local target_pid=$!
    PIDS+=("${target_pid}")
    wait_for "${dir}/target.log" "Listening for remote bitbang connection on port" "${target_pid}"
    if ! grep -q 'verification override RUSCV_S6_DMSTATUS_VERSION=4' "${dir}/target.log"; then
        echo "version mutation did not announce itself" >&2
        return 1
    fi
    local gdb_port
    gdb_port="$(free_port)"
    set +e
    timeout 30 openocd \
        -l "${dir}/openocd.log" \
        -c "set RBB_PORT ${port}" \
        -c "set GDB_PORT ${gdb_port}" \
        -f "${CFG}" \
        >"${dir}/openocd.stdout" 2>&1
    local openocd_status=$?
    set -e
    if [[ "${openocd_status}" -eq 0 ]]; then
        echo "OpenOCD accepted dmstatus.version 4" >&2
        return 1
    fi
    if grep -q 'Examined RISC-V core' "${dir}/openocd.log"; then
        echo "OpenOCD examined a module whose version is 4" >&2
        return 1
    fi
    if ! grep -q 'only supports Debug Module version' "${dir}/openocd.log" "${dir}/openocd.stdout"; then
        echo "OpenOCD did not report a version rejection" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    if kill -0 "${target_pid}" 2>/dev/null; then
        kill "${target_pid}" 2>/dev/null || true
        wait "${target_pid}" 2>/dev/null || true
    fi
    echo "negative version: OpenOCD rejected dmstatus.version 4"
}

run_negative_datacount() {
    local dir="${WORK}/negative-datacount"
    mkdir -p "${dir}"
    local port gdb_port
    port="$(free_port)"
    gdb_port="$(free_port)"
    RUSCV_S6_DATACOUNT=2 \
        "${RUSCV_SIM}" run "${WORK}/guest.elf" --rbb-port "${port}" --halted --max-cycles 100000000 \
        >"${dir}/target.log" 2>&1 &
    local target_pid=$!
    PIDS+=("${target_pid}")
    wait_for "${dir}/target.log" "Listening for remote bitbang connection on port" "${target_pid}"
    if ! grep -q 'verification override RUSCV_S6_DATACOUNT=2' "${dir}/target.log"; then
        echo "datacount mutation did not announce itself" >&2
        return 1
    fi
    # This control proves that datacount 2 hides data2/data3 from abstract
    # Access Memory. The two-word program buffer can also carry the write, so
    # this session restricts memory access to abstract only.
    openocd \
        -l "${dir}/openocd.log" \
        -c "set RBB_PORT ${port}" \
        -c "set GDB_PORT ${gdb_port}" \
        -f "${CFG}" \
        -c "riscv set_mem_access abstract" \
        >"${dir}/openocd.stdout" 2>&1 &
    local openocd_pid=$!
    PIDS+=("${openocd_pid}")
    if ! wait_for "${dir}/openocd.log" "Examined RISC-V core" "${openocd_pid}" 150; then
        echo "datacount mutation: OpenOCD did not examine the hart" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    set +e
    timeout 30 gdb-multiarch -batch -nx \
        -ex "set pagination off" \
        -ex "set confirm off" \
        -ex "file ${WORK}/guest.elf" \
        -ex "target remote :${gdb_port}" \
        -ex "set {unsigned long long}&s6_scratch = 0x1122334455667788" \
        >"${dir}/gdb.log" 2>&1
    set -e
    # GDB 15 exits 0 when OpenOCD rejects the abstract write. The OpenOCD
    # log is the proof: datacount 2 hides data2/data3, so the 64-bit address
    # never lands and Access Memory fails.
    if ! grep -q 'datacount=2' "${dir}/openocd.log"; then
        echo "OpenOCD did not report datacount 2" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    if ! grep -q 'Failed to write memory' "${dir}/openocd.log"; then
        echo "datacount 2 did not fail the memory write" >&2
        cat "${dir}/openocd.log" >&2 || true
        cat "${dir}/gdb.log" >&2 || true
        return 1
    fi
    if ! grep -q 'abstract=failed' "${dir}/openocd.log"; then
        echo "datacount 2 did not fail abstract memory" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    if grep -q '1122334455667788' "${dir}/gdb.log"; then
        echo "GDB read back the RAM value with datacount 2" >&2
        cat "${dir}/gdb.log" >&2 || true
        return 1
    fi
    if kill -0 "${target_pid}" 2>/dev/null; then
        kill "${target_pid}" 2>/dev/null || true
        wait "${target_pid}" 2>/dev/null || true
    fi
    if kill -0 "${openocd_pid}" 2>/dev/null; then
        kill "${openocd_pid}" 2>/dev/null || true
        wait "${openocd_pid}" 2>/dev/null || true
    fi
    echo "negative datacount: abstract memory failed"
}

# The U4 comparison: the same addresses and values on both targets. Only the
# reset PC differs, for the same reason as the abstract session.
compare_progbuf_named() {
    local left="$1" right="$2"
    local keys=()
    read -r -a keys <<<"$(u4_keys)"
    local key a b
    for key in "${keys[@]}"; do
        if [[ "${key}" == "reset_pc" ]]; then
            continue
        fi
        a="$(field "${left}" "${key}")"
        b="$(field "${right}" "${key}")"
        if [[ "${a}" != "${b}" ]]; then
            echo "Spike comparison mismatch on ${key}: simulator ${a}, spike ${b}" >&2
            return 1
        fi
        printf 'match %s=%s\n' "${key}" "${a}"
    done
    note_divergence "${left}" "${right}" reset_pc "$(printf '0x%x' "$((0x${ENTRY}))")" 0x1000 \
        "monitor reset halt returns Spike to its boot ROM. This profile's ndmreset reloads the image and halts at the ELF entry."
}

# A target with two effective program-buffer slots cannot host OpenOCD's
# three-slot memory programs. The forced session must then fail closed:
# OpenOCD skips its only enabled method and reports that the buffer is too
# small, and no write lands through program buffer or abstract memory.
run_negative_progbuf_capacity() {
    local label="$1" starter="$2"
    local dir="${WORK}/${label}"
    mkdir -p "${dir}"
    local port gdb_port
    port="$(free_port)"
    gdb_port="$(free_port)"
    "${starter}" "${port}" >"${dir}/target.log" 2>&1 &
    local target_pid=$!
    PIDS+=("${target_pid}")
    wait_for "${dir}/target.log" "Listening for remote bitbang connection on port" "${target_pid}"
    openocd \
        -d3 \
        -l "${dir}/openocd.log" \
        -c "set RBB_PORT ${port}" \
        -c "set GDB_PORT ${gdb_port}" \
        -f "${CFG}" \
        -c "riscv set_mem_access progbuf" \
        >"${dir}/openocd.stdout" 2>&1 &
    local openocd_pid=$!
    PIDS+=("${openocd_pid}")
    if ! wait_for "${dir}/openocd.log" "Examined RISC-V core" "${openocd_pid}" 150; then
        echo "${label}: OpenOCD did not examine the hart" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    # The forced session's first transfer must now fail.
    set +e
    timeout 30 gdb-multiarch -batch -nx \
        -ex "set pagination off" \
        -ex "set confirm off" \
        -ex "set architecture riscv:rv64" \
        -ex "file ${WORK}/guest.elf" \
        -ex "target remote :${gdb_port}" \
        -ex "set {unsigned int}((char *)&s6_pb32 + 0) = 0x11223344" \
        >"${dir}/gdb.log" 2>&1
    local gdb_status=$?
    set -e
    if [[ "${gdb_status}" -eq 0 ]]; then
        echo "${label}: GDB reported a successful write with two program-buffer slots" >&2
        cat "${dir}/gdb.log" >&2 || true
        return 1
    fi
    if ! grep -aq 'Skipping mem write via progbuf - insufficient progbuf size' "${dir}/openocd.log"; then
        echo "${label}: OpenOCD did not report the missing program-buffer capacity" >&2
        cat "${dir}/openocd.log" >&2 || true
        return 1
    fi
    if grep -aq 'Succeeded to write memory via program buffer' "${dir}/openocd.log"; then
        echo "${label}: a write succeeded through an undersized program buffer" >&2
        return 1
    fi
    if grep -aq 'Succeeded to write memory via abstract access' "${dir}/openocd.log"; then
        echo "${label}: a write fell back to abstract memory" >&2
        return 1
    fi
    if ! grep -aq 'Cannot access memory at address' "${dir}/gdb.log"; then
        echo "${label}: GDB did not report the refused write" >&2
        cat "${dir}/gdb.log" >&2 || true
        return 1
    fi
    echo "${label}: forced program-buffer access failed closed"
}

compare_named() {
    local left="$1" right="$2"
    local key
    for key in connect gpr mstatus step_pc step_gpr break_pc observe_gpr ram halt_pc \
        dcsr_debugver dcsr_ebreakm dcsr_ebreaks dcsr_ebreaku dcsr_step dcsr_prv dcsr_common; do
        local a b
        a="$(field "${left}" "${key}")"
        b="$(field "${right}" "${key}")"
        if [[ "${a}" != "${b}" ]]; then
            echo "Spike comparison mismatch on ${key}: simulator ${a}, spike ${b}" >&2
            echo "simulator dcsr=$(field "${left}" dcsr) spike dcsr=$(field "${right}" dcsr)" >&2
            return 1
        fi
        printf 'match %s=%s\n' "${key}" "${a}"
    done
    note_divergence "${left}" "${right}" pc "$(norm "${ENTRY}")" 1000 \
        "Spike --halted stops at its boot ROM (0x1000). This profile has no boot ROM and stops at the ELF entry. The GDB script then writes PC to _start before the guest operations."
    note_divergence "${left}" "${right}" reset_pc "$(norm "${ENTRY}")" 1000 \
        "monitor reset halt returns Spike to that boot ROM. This profile's ndmreset reloads the image and halts at the ELF entry."
    note_divergence "${left}" "${right}" dcsr_stopcount 1 0 \
        "D5 fixes stopcount to 1. Spike hardwires stopcount to 0."
    note_divergence "${left}" "${right}" dcsr_stoptime 1 0 \
        "D5 fixes stoptime to 1. Spike hardwires stoptime to 0."
    note_divergence "${left}" "${right}" dcsr_mprven 1 0 \
        "D5 fixes mprven to 1. Spike resets mprven to 0 and leaves it writable."
    note_divergence "${left}" "${right}" dcsr_cause 3 5 \
        "This profile's --halted is haltreq (cause 3). Spike --halted uses DCSR_CAUSE_HALT (5), which Debug Spec 1.0 calls resethaltreq. D11 leaves resethaltreq unimplemented."
    echo "dcsr raw: simulator $(field "${left}" dcsr), spike $(field "${right}" dcsr)"
}

# A known pair is a documented divergence. Any other difference fails.
note_divergence() {
    local left="$1" right="$2" key="$3" sim_known="$4" spike_known="$5" note="$6"
    local a b
    a="$(field "${left}" "${key}")"
    b="$(field "${right}" "${key}")"
    if [[ "${a}" == "${b}" ]]; then
        printf 'match %s=%s\n' "${key}" "${a}"
        return 0
    fi
    if [[ "${a}" == "${sim_known}" && "${b}" == "${spike_known}" ]]; then
        printf 'divergence %s: simulator %s, spike %s\n' "${key}" "${a}" "${b}"
        printf '  %s\n' "${note}"
        return 0
    fi
    echo "unexpected ${key}: simulator ${a}, spike ${b}" >&2
    return 1
}

echo "OpenOCD: $(openocd --version 2>&1 | head -1)"
echo "GDB: $(gdb-multiarch --version | head -1)"

run_session ruscv ruscv_cmd "${WORK}/ruscv.named"
run_progbuf_session ruscv-progbuf ruscv_cmd "${WORK}/ruscv-progbuf.named"
run_negative_version
run_negative_datacount

if [[ "${SKIP_SPIKE}" -eq 1 ]]; then
    echo "spike comparison and program-buffer capacity controls skipped"
elif spike_available; then
    echo "Spike: $(command -v spike)"
    run_progbuf_session spike-progbuf spike_progbuf_cmd "${WORK}/spike-progbuf.named"
    compare_progbuf_named "${WORK}/ruscv-progbuf.named" "${WORK}/spike-progbuf.named" \
        | tee "${WORK}/progbuf-comparison.txt"
    run_negative_progbuf_capacity negative-progbufsize spike_one_word_progbuf
    run_negative_progbuf_capacity negative-impebreak spike_no_implicit_ebreak
    run_session spike spike_cmd "${WORK}/spike.named"
    compare_named "${WORK}/ruscv.named" "${WORK}/spike.named" | tee "${WORK}/comparison.txt"
else
    echo "spike with --dm-datacount and --halted is not on PATH" >&2
    if [[ "${REQUIRE_SPIKE}" -eq 1 ]]; then
        exit 1
    fi
    echo "spike comparison skipped"
fi

if [[ -n "${EVIDENCE}" ]]; then
    mkdir -p "${EVIDENCE}"
    # The script rebuilds the guest. Keep logs and named values only.
    tar -C "${WORK}" -cf - --exclude='guest.o' --exclude='guest.elf' . \
        | tar -C "${EVIDENCE}" -xf -
    echo "evidence copied to ${EVIDENCE}"
fi

echo "debug-attach ok"
