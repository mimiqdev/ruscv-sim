#!/usr/bin/env bash
# Stage 6 U5. One OpenOCD 0.12.0 + GDB session performs every §7 item 5 row
# against `run --rbb-port --halted`, then the same session against Spike.
# Two discovery mutations for that session follow: dmstatus.version 4 and
# datacount 2.
#
# Program-buffer U4 adds a second session that forces OpenOCD's program-buffer
# memory path by enabling no other method. It transfers four adjacent values at
# 8, 4, 2, and 1 bytes, reads every address back through abstract Access
# Memory, faults on an unmapped store, and recovers. Spike runs the same
# session with matching capacity, implicit EBREAK, and datacount, plus two
# capacity controls that must make the session fail closed.
#
# System-bus U4 adds a third session that forces OpenOCD's system-bus memory
# path the same way. It transfers four adjacent values at all four supported
# widths, reads the bulk array back through the system bus, faults on an
# unmapped store with sberror 2, recovers, reads every address back through
# abstract Access Memory, and records the sbcs profile and the sberror
# taxonomy over raw DMI. Spike runs the same session with --dm-sba=64. Two
# raw-DMI rows are the pre-identified pinned-Spike deviations: Spike reports
# sberror 3, not the specification's 4, for an unsupported sbaccess size, and
# performs no alignment check, so its misaligned word load traps as a bad
# address (sberror 2).
#
# F/D U6 adds one rv64imafdc session under contract D12. The debugger writes
# f0-f31 and fcsr through DM abstract access while mstatus.FS is Off, reads
# every value back, and continues. The guest verifies that FS is still Off,
# sets FS, checks the written values, and parks on an FLD, an FADD.D and an
# FSW in turn; the debugger breaks on each, single-steps it, and reads
# mstatus.FS before and after. Pinned Spike runs the same image with
# --isa=rv64imafdc_zicsr. The only named difference is the connect PC:
# Spike halts in its boot ROM, this profile at the ELF entry.
#
# RV64GC U3 binds every session to its profile under contract D7/D3. The
# abstract, program-buffer and system-bus sessions (and the two negative
# controls) run the simulator under explicit `--isa rv64ima`, the Zca session
# under `--isa rv64imac`, and the F/D session with no `--isa` at all: since the
# RV64GC U1 default flip it runs under the rv64imafdc default and proves it
# (contract D7). Every
# Spike side keeps its existing --isa string, and no named result changes.
#
# Direct-memory U3 adds a session under `run --transport tlm --dmi` with the
# system bus as the debugger's forced memory path: the debugger reads the word
# at `s6_break`, plants a software breakpoint on it, resumes, hits it, removes
# it, reads the restored word, and then re-executes it to the tohost exit. The
# inserted and restored bytes are read back through the same forced path, so a
# shadow copy in the debugger fails, and the run after removal only completes
# if the fetch returned the restored instruction (direct-memory D11). Spike has
# no direct-memory route, so this session has no Spike pair; the guest-visible
# facts it checks are the ones the system-bus session already compares with
# Spike.
#
# Named outputs are the S6, U4, Z5, U3DMI and U6 lines in the GDB logs. The
# comparisons are those values, not a hash. dmstatus.version is not one of them. dcsr is
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
GDB_SYSBUS_SCRIPT="${ROOT}/scripts/debug/s6-u4-sysbus.gdb"
GDB_DIRECT_MEM_SCRIPT="${ROOT}/scripts/debug/direct-mem-u3-dmi.gdb"
GDB_ZCA_SCRIPT="${ROOT}/scripts/debug/zca-u5-session.gdb"
GDB_FD_SCRIPT="${ROOT}/scripts/debug/fd-u6-session.gdb"
GUEST_SRC="${ROOT}/tests/bare-metal-riscv-test/rv64i/s6_u5_attach.S"
ZCA_GUEST_SRC="${ROOT}/tests/bare-metal-riscv-test/rv64i/zca_u5_attach.S"
FD_GUEST_SRC="${ROOT}/tests/bare-metal-riscv-test/rv64ifd/fdc_u6_attach.S"
LINKER="${ROOT}/tests/bare-metal-riscv-test/linker.ld"
LINKER_S4="${ROOT}/tests/bare-metal-riscv-test/linker-s4.ld"

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

# Zca U5. The compressed guest is a separate image because it needs the
# rv64imac profile. The compressed session is mandatory, so a binutils that
# cannot assemble the image fails the run instead of silently skipping it.
if ! "${AS}" -march=rv64imac_zicsr_zifencei -mabi=lp64 "${ZCA_GUEST_SRC}" -o "${WORK}/zca-guest.o" \
    2>"${WORK}/zca-assemble.err"; then
    echo "failed to assemble the compressed guest (riscv64 binutils with Zca is required)" >&2
    cat "${WORK}/zca-assemble.err" >&2 || true
    exit 1
fi
"${LD}" -T "${LINKER}" "${WORK}/zca-guest.o" -o "${WORK}/zca-guest.elf"

zca_symbol() {
    local name="$1"
    "${NM}" "${WORK}/zca-guest.elf" | awk -v name="${name}" '$3 == name { print $1; exit }'
}

ZCA_ENTRY="$(zca_symbol _start)"
ZCA_LEN2="$(zca_symbol zca_len2)"
ZCA_LEN4="$(zca_symbol zca_len4)"
ZCA_LINK="$(zca_symbol zca_link)"
ZCA_SECOND_LINK="$(zca_symbol zca_second_link)"
ZCA_HINT="$(zca_symbol zca_hint)"
ZCA_SPIN="$(zca_symbol zca_spin)"
ZCA_EXIT="$(zca_symbol zca_exit)"
if [[ -z "${ZCA_ENTRY}" || -z "${ZCA_LEN2}" || -z "${ZCA_LEN4}" \
    || -z "${ZCA_LINK}" || -z "${ZCA_SECOND_LINK}" || -z "${ZCA_HINT}" || -z "${ZCA_SPIN}" || -z "${ZCA_EXIT}" ]]; then
    echo "compressed guest is missing one of its fixed labels" >&2
    exit 1
fi
# The fixed instruction mix is a guest invariant: the 2-byte instruction
# and, per contract D10, the 4-byte breakpoint target both start at
# pc % 4 == 2 in this image.
if (( 0x${ZCA_LEN2} % 4 != 2 )); then
    echo "zca_len2 (0x${ZCA_LEN2}) is not at pc % 4 == 2" >&2
    exit 1
fi
if (( 0x${ZCA_LEN4} % 4 != 2 )); then
    echo "zca_len4 (0x${ZCA_LEN4}) is not at pc % 4 == 2" >&2
    exit 1
fi

# F/D U6. The FP session image is a separate rv64imafdc build because the
# same source is also a fresh-suite guest under that profile. Both the
# session and the suite require this image, so a binutils without F/D/C
# fails the run instead of skipping it.
if ! "${AS}" -march=rv64imafdc_zicsr_zifencei -mabi=lp64 "${FD_GUEST_SRC}" -o "${WORK}/fdc-guest.o" \
    2>"${WORK}/fdc-assemble.err"; then
    echo "failed to assemble the rv64imafdc FP session guest (riscv64 binutils with F/D/C is required)" >&2
    cat "${WORK}/fdc-assemble.err" >&2 || true
    exit 1
fi
"${LD}" -T "${LINKER_S4}" "${WORK}/fdc-guest.o" -o "${WORK}/fdc-guest.elf"

fd_symbol() {
    local name="$1"
    "${NM}" "${WORK}/fdc-guest.elf" | awk -v name="${name}" '$3 == name { print $1; exit }'
}

FD_ENTRY="$(fd_symbol _start)"
FD_LOAD="$(fd_symbol fd_u6_bp_load)"
FD_ADD="$(fd_symbol fd_u6_bp_add)"
FD_STORE="$(fd_symbol fd_u6_bp_store)"
if [[ -z "${FD_ENTRY}" || -z "${FD_LOAD}" || -z "${FD_ADD}" || -z "${FD_STORE}" ]]; then
    echo "FP session guest is missing one of its fixed labels" >&2
    exit 1
fi
# The guest keeps the three FP stops 4-byte aligned and 4-byte long. A
# misaligned stop would need a multi-word breakpoint write, which the pinned
# Spike refuses.
for address in "${FD_LOAD}" "${FD_ADD}" "${FD_STORE}"; do
    if (( 0x${address} % 4 != 0 )); then
        echo "FP session stop at 0x${address} is not 4-byte aligned" >&2
        exit 1
    fi
done

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

# mstatus.FS (bits 14:13) and SD (bit 63). Bash arithmetic is two's
# complement, so the shifts stay correct when bit 63 is set.
fs_field() {
    local value
    value="$(norm "$1")"
    printf '%x' "$(( (0x${value} >> 13) & 3 ))"
}

sd_field() {
    local value
    value="$(norm "$1")"
    printf '%x' "$(( (0x${value} >> 63) & 1 ))"
}

# The U6 guest writes f_i = i + 0.5, exact in binary64. This returns the
# expected raw bits so the readback is checked against the intended value,
# not only against the other target.
fd_expected_bits() {
    python3 - "$1" <<'PY'
import struct
import sys
value = float(sys.argv[1])
print(f"{struct.unpack('<Q', struct.pack('<d', value))[0]:x}")
PY
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
# and takes the remote_bitbang port. Logs stay in ${WORK}/$1. GDB loads
# ${WORK}/guest.elf unless the caller sets SESSION_ELF for a different image.
# Remaining arguments go to OpenOCD after the config file.
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
    local gdb_elf="${SESSION_ELF:-${WORK}/guest.elf}"

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
        -ex "file ${gdb_elf}" \
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
        echo "${name}: GDB did not reach [${ready}]" >&2
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
        echo "${name}: GDB log has no [${ready}] (gdb status ${gdb_status})" >&2
        cat "${gdb_log}" >&2 || true
        cat "${openocd_log}" >&2 || true
        return 1
    fi
    # Wait for OpenOCD to notice the socket drop. It either logs the
    # disconnect or, when the scan hits the dead socket between handlers,
    # aborts (Debug Spec sessions end by the target exiting first, so the
    # SIGABRT after the named outputs is that known 0.12.0 shutdown race).
    local tries
    for ((tries = 0; tries < 50; tries++)); do
        if grep -E -q 'Connection reset by peer|Broken pipe|remote_bitbang_fill_buf' "${openocd_log}" "${dir}/openocd.stdout"; then
            break
        fi
        if ! kill -0 "${openocd_pid}" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done
    if ! grep -E -q 'Connection reset by peer|Broken pipe|remote_bitbang_fill_buf' "${openocd_log}" "${dir}/openocd.stdout" \
        && kill -0 "${openocd_pid}" 2>/dev/null; then
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

# The system-bus session's named outputs, in the order the session prints
# them. The raw-DMI rows join their keys with underscores and are folded in
# by extract_dmi_named below.
u4_sysbus_keys() {
    printf 'connect bulk64 width32 width16 width8 sysbus_readback fault recovery s0 s1 halt_pc reset_pc'
    local array index
    for array in s6_pb64 s6_pb32 s6_pb16 s6_pb8; do
        for index in 0 1 2 3; do
            printf ' %s_%s' "${array}" "${index}"
        done
    done
}

# The raw-DMI rows print as U4SBCS_* and U4SBERROR_*: GDB's monitor path
# drops the connection on spaced output, so those echo lines cannot carry
# the "U4 key" shape. Fold them into the named file under their session
# keys.
extract_dmi_named() {
    local named="$1" gdb_log="$2" pair
    local value
    for pair in sbcs_profile:U4SBCS_PROFILE sberror_misaligned:U4SBERROR_MISALIGNED \
        sberror_badaddr:U4SBERROR_BADADDR sberror_unsupported:U4SBERROR_UNSUPPORTED; do
        value="$(awk -F= -v key="${pair##*:}" '$1 == key { print substr($0, index($0, "=") + 1); exit }' "${gdb_log}")"
        if [[ -z "${value}" ]]; then
            echo "U4 raw-DMI: missing ${pair##*:}" >&2
            cat "${gdb_log}" >&2 || true
            return 1
        fi
        printf '%s=%s\n' "${pair%%:*}" "${value}" >>"${named}"
    done
}

# The forced system-bus session. OpenOCD has no other memory-access method,
# so a transfer it reports here cannot have used abstract memory or the
# program buffer.
run_sysbus_session() {
    local name="$1"
    local named="$3"
    start_session "${name}" "$2" "${GDB_SYSBUS_SCRIPT}" 'U4 reset_pc=' \
        -d3 -c "riscv set_mem_access sysbus" || return 1
    local keys=()
    read -r -a keys <<<"$(u4_sysbus_keys)"
    extract_named "${named}" "${WORK}/${name}/gdb.log" U4 raw "${keys[@]}" || return 1
    extract_dmi_named "${named}" "${WORK}/${name}/gdb.log" || return 1
    assert_sysbus_session "${name}" "${named}" || return 1
    assert_sysbus_dmi "${name}" "${named}" || return 1
    assert_sysbus_evidence "${name}" "${WORK}/${name}" || return 1
    echo "${name}: system-bus session ok (target exit 0)"
}

# Each readback value is checked at its own symbol-derived address against
# the constants the GDB script wrote, so a repeated value, a stale value, or
# a wrong increment fails. The raw-DMI profile row must advertise the
# contract's D2 profile.
assert_sysbus_session() {
    local name="$1" named="$2"
    if [[ "$(field "${named}" connect)" != "ok" ]]; then
        echo "${name}: system-bus session did not connect" >&2
        return 1
    fi
    local key
    for key in bulk64 width32 width16 width8 recovery; do
        if [[ "$(field "${named}" "${key}")" != "written" ]]; then
            echo "${name}: the ${key} transfer did not complete" >&2
            return 1
        fi
    done
    if [[ "$(field "${named}" sysbus_readback)" != "0x1111111111111111" ]]; then
        echo "${name}: the system-bus read of the bulk array is $(field "${named}" sysbus_readback)" >&2
        return 1
    fi
    if [[ "$(field "${named}" fault)" != "refused" ]]; then
        echo "${name}: an unmapped system-bus write was not refused" >&2
        return 1
    fi
    if [[ "$(field "${named}" s0)" != "0x5a5a5a5a5a5a5a5a" \
        || "$(field "${named}" s1)" != "0xa5a5a5a5a5a5a5a5" ]]; then
        echo "${name}: a hart register changed during system-bus access" >&2
        return 1
    fi
    # 0x2004080f is the D2 reset profile (sbversion 1, sbasize 64, widths
    # 8-64, sbaccess 2). The asserted value reads sbaccess 0: this row is
    # taken at the end of the session, after OpenOCD's last word access, so
    # it also pins that session residue. The contract bits are the high
    # groups; the sbaccess field is OpenOCD state, and an OpenOCD bump may
    # legitimately change it without a simulator regression.
    if [[ "$(field "${named}" sbcs_profile)" != "0x2000080f" ]]; then
        echo "${name}: the sbcs profile is $(field "${named}" sbcs_profile), expected 0x2000080f" >&2
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

# The sberror taxonomy over raw DMI. The profile row is checked with the
# session; these two rows carry the pre-identified pinned-Spike deviations:
# the contract follows the specification on both, and the expectation names
# Spike's behavior where the targets differ.
assert_sysbus_dmi() {
    local name="$1" named="$2"
    if [[ "${name}" == spike-* ]]; then
        assert_eq "${name}" "$(field "${named}" sberror_misaligned)" "2" "sberror for a misaligned word" \
            || return 1
        assert_eq "${name}" "$(field "${named}" sberror_unsupported)" "3" "sberror for an unsupported sbaccess" \
            || return 1
    else
        assert_eq "${name}" "$(field "${named}" sberror_misaligned)" "3" "sberror for a misaligned word" \
            || return 1
        assert_eq "${name}" "$(field "${named}" sberror_unsupported)" "4" "sberror for an unsupported sbaccess" \
            || return 1
    fi
    assert_eq "${name}" "$(field "${named}" sberror_badaddr)" "2" "sberror for an unmapped word" || return 1
}

# The OpenOCD log must record a system-bus burst for every written address,
# the 64-bit value markers of the bulk-transfer readback, a single failed
# write for the fault word with sberror 2 at that address, no abstract or
# program-buffer transfer before the readback switch, and an abstract read
# for every checked address after it. GDB hands the bulk transfer to OpenOCD
# as one buffer, so the 64-bit array is asserted at its base: OpenOCD bursts
# it whole on this simulator and per 8-byte word on Spike, and both chunk
# the element stores of the narrower arrays.
assert_sysbus_evidence() {
    local name="$1" dir="$2"
    local log="${dir}/openocd.log"
    if ! grep -aq "transferring burst starting at address $(printf '0x%x' "0x$(symbol s6_pb64)")" "${log}"; then
        echo "${name}: OpenOCD log lacks a system-bus burst for the bulk array" >&2
        return 1
    fi
    local symbol width index address
    while read -r symbol width; do
        local base
        base="$(symbol "${symbol}")"
        for index in 0 1 2 3; do
            address="$(printf '0x%x' "$((0x${base} + index * width))")"
            if ! grep -aq "transferring burst starting at address ${address}" "${log}"; then
                echo "${name}: OpenOCD log lacks a system-bus burst at ${address}" >&2
                return 1
            fi
        done
    done <<'TABLE'
s6_pb32 4
s6_pb16 2
s6_pb8 1
TABLE
    local bulk
    bulk="$(printf '0x%x' "0x$(symbol s6_pb64)")"
    for pattern in \
        'Succeeded to write memory via system bus' \
        'Succeeded to read memory via system bus' \
        "M[${bulk}] reads 0x11111111" \
        "M[$(printf '0x%x' "$((0x${bulk#0x} + 4))")] reads 0x11111111" \
        'System bus access failed with sberror=2 (sbaddress=0x7ffffff0)' \
        'Failed to write memory (addr=0x7ffffff0)'
    do
        # Fixed strings: the address rows contain bracket expressions that
        # a regex grep would read as character classes.
        if ! grep -aqF -- "${pattern}" "${log}"; then
            echo "${name}: OpenOCD log lacks [${pattern}]" >&2
            return 1
        fi
    done
    # 1 bulk transfer + 4+4+4 element stores at 4/2/1 bytes + 1 recovery =
    # 14 successful sysbus writes; the fault write is the single failure
    # asserted separately.
    if [[ "$(grep -ac -- 'Succeeded to write memory via system bus' "${log}")" != 14 ]]; then
        echo "${name}: the successful system-bus write count is not 14" >&2
        return 1
    fi
    if [[ "$(grep -ac -- 'Failed to write memory via system bus' "${log}")" != 1 ]]; then
        echo "${name}: the deliberate system-bus failure is not the only one" >&2
        return 1
    fi
    # The two-word program buffer is disabled in this session; no transfer
    # may use it.
    if grep -aq 'write_memory_progbuf' "${log}"; then
        echo "${name}: a transfer used the disabled program buffer" >&2
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
            echo "${name}: a transfer succeeded through abstract memory before the switch" >&2
            return 1
        fi
    done < <(grep -an -- 'Succeeded to write memory via abstract access' "${log}" | cut -d: -f1)
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

# The direct-memory U3 named outputs, in the order the session prints them.
direct_mem_keys() {
    printf 'connect original inserted restored break_pc a2 resume'
}

# The direct-memory session (direct-memory D11). OpenOCD has no memory-access
# method other than the system bus, so the breakpoint insert and its removal
# are both system-bus writes, and the hart's fetch of them is the same fetch
# the direct-memory grant serves.
run_direct_mem_session() {
    local name="$1" named="$3"
    start_session "${name}" "$2" "${GDB_DIRECT_MEM_SCRIPT}" 'U3DMI resume=ok' \
        -d3 -c "riscv set_mem_access sysbus" || return 1
    local keys=()
    read -r -a keys <<<"$(direct_mem_keys)"
    extract_named "${named}" "${WORK}/${name}/gdb.log" U3DMI raw "${keys[@]}" || return 1
    assert_direct_mem_session "${name}" "${named}" || return 1
    assert_direct_mem_evidence "${name}" "${WORK}/${name}" || return 1
    echo "${name}: direct-memory session ok (target exit 0)"
}

# The guest's `s6_break` is `addi a2, a2, 1`; the debugger must put EBREAK
# there, remove it, and leave the original word behind.
assert_direct_mem_session() {
    local name="$1" named="$2"
    assert_eq "${name}" "$(field "${named}" connect)" "ok" "connect" || return 1
    assert_eq "${name}" "$(field "${named}" original)" "0x00160613" \
        "the word at s6_break" || return 1
    assert_eq "${name}" "$(field "${named}" inserted)" "0x00100073" \
        "the inserted breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" break_pc)" "${BREAK_AT}" "breakpoint PC" || return 1
    assert_eq "${name}" "$(field "${named}" a2)" "0" "a2 at the breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" restored)" "$(field "${named}" original)" \
        "the restored word" || return 1
    assert_eq "${name}" "$(field "${named}" resume)" "ok" "resume after removal" || return 1
}

# Both breakpoint writes must be system-bus writes, and neither the program
# buffer nor abstract access may have carried them.
assert_direct_mem_evidence() {
    local name="$1" dir="$2"
    local log="${dir}/openocd.log"
    local site writes
    site="$(printf '0x%x' "0x$(symbol s6_break)")"
    if ! grep -aqF -- "M[${site}] reads 0x00100073" "${log}"; then
        echo "${name}: OpenOCD log has no system-bus read of the planted EBREAK" >&2
        return 1
    fi
    writes="$(grep -ac -- 'Succeeded to write memory via system bus' "${log}")"
    if [[ "${writes}" != 2 ]]; then
        echo "${name}: the breakpoint insert and removal are not two system-bus writes (${writes})" >&2
        return 1
    fi
    if grep -aq 'write_memory_progbuf' "${log}"; then
        echo "${name}: a transfer used the disabled program buffer" >&2
        return 1
    fi
    if grep -aq 'Succeeded to write memory via abstract access' "${log}"; then
        echo "${name}: a transfer used the disabled abstract path" >&2
        return 1
    fi
}

# The Zca U5 named outputs, in the order the session prints them.
zca_u5_keys() {
    printf 'connect pc gpr dcsr break2_pc break2_dpc break2_dcsr break2_a0 break2_a1'
    printf ' step2_pc step2_a0 step2_a1 break4_pc break4_dpc break4_dcsr break4_s0'
    printf ' step4_pc step4_s0 link_pc link_s0 hint_before_a1 hint_pc hint_after_a1'
    printf ' halt_pc halt_dpc reset_pc'
}

# The compressed session. Contract D10: the same target and debugger path as
# the abstract session, against a mixed-length rv64imac guest. OpenOCD's
# -d3 log carries the breakpoint writes, so the evidence check can prove the
# 2-byte instruction received the 2-byte C.EBREAK write and that a 4-byte
# breakpoint was added at the 4-byte instruction.
run_zca_session() {
    local name="$1"
    local named="$3"
    local SESSION_ELF="${WORK}/zca-guest.elf"
    start_session "${name}" "$2" "${GDB_ZCA_SCRIPT}" 'Z5 reset_pc=' -d3 || return 1
    local keys=()
    read -r -a keys <<<"$(zca_u5_keys)"
    extract_named "${named}" "${WORK}/${name}/gdb.log" Z5 norm "${keys[@]}" || return 1
    append_dcsr_fields "${named}" || return 1
    assert_zca_session "${name}" "${named}" || return 1
    assert_zca_evidence "${name}" "${WORK}/${name}" || return 1
    echo "${name}: compressed session ok (target exit 0)"
}

# Exact stops of the fixed guest. A wrong width, a wrong link value, or a
# stale PC fails here. The dcsr cause after each breakpoint is 1 (EBREAK),
# which is the C.EBREAK path for the 2-byte stop.
assert_zca_session() {
    local name="$1" named="$2"
    assert_eq "${name}" "$(field "${named}" connect)" "ok" "connect" || return 1
    if [[ "${name}" == "spike-zca" ]]; then
        assert_eq "${name}" "$(field "${named}" pc)" "1000" "connect PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" pc)" "${ZCA_ENTRY}" "connect PC" || return 1
    fi
    assert_eq "${name}" "$(field "${named}" gpr)" "0" "connect GPR" || return 1

    local len2 len4 hint
    len2="$(printf '%x' "$((0x${ZCA_LEN2}))")"
    len4="$(printf '%x' "$((0x${ZCA_LEN4}))")"
    hint="$(printf '%x' "$((0x${ZCA_HINT}))")"

    assert_eq "${name}" "$(field "${named}" break2_pc)" "${len2}" "2-byte breakpoint PC" || return 1
    assert_eq "${name}" "$(field "${named}" break2_dpc)" "${len2}" "2-byte breakpoint dpc" || return 1
    assert_eq "${name}" "$(field "${named}" break2_a0)" "0x1f" "a0 at the 2-byte breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" break2_a1)" "1" "a1 at the 2-byte breakpoint" || return 1
    # C.EBREAK is EBREAK (contract D8): dcsr.cause is 1 at the 2-byte stop.
    assert_eq "${name}" "$(( (0x$(field "${named}" break2_dcsr) >> 6) & 7 ))" "1" \
        "dcsr.cause at the 2-byte breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" step2_pc)" "$(printf '%x' "$((0x${ZCA_LEN2} + 2))")" \
        "single-step PC over the 2-byte instruction" || return 1
    assert_eq "${name}" "$(field "${named}" step2_a0)" "0x20" "a0 after stepping the 2-byte instruction" \
        || return 1
    assert_eq "${name}" "$(field "${named}" step2_a1)" "7" "a1 survived the 2-byte step" || return 1

    assert_eq "${name}" "$(field "${named}" break4_pc)" "${len4}" "4-byte breakpoint PC" || return 1
    assert_eq "${name}" "$(field "${named}" break4_dpc)" "${len4}" "4-byte breakpoint dpc" || return 1
    assert_eq "${name}" "$(field "${named}" break4_s0)" "0" "s0 before the link" || return 1
    assert_eq "${name}" "$(( (0x$(field "${named}" break4_dcsr) >> 6) & 7 ))" "1" \
        "dcsr.cause at the 4-byte breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" step4_pc)" "${ZCA_LINK}" \
        "single-step PC over the 4-byte instruction at pc % 4 == 2" || return 1
    assert_eq "${name}" "$(field "${named}" step4_s0)" "0" \
        "s0 is untouched by the 4-byte jump" || return 1

    assert_eq "${name}" "$(field "${named}" link_pc)" "${hint}" \
        "single-step PC over the 4-byte link" || return 1
    assert_eq "${name}" "$(field "${named}" link_s0)" "${ZCA_SECOND_LINK}" \
        "4-byte link value" || return 1
    assert_eq "${name}" "$(field "${named}" hint_before_a1)" "7" "a1 before the HINT" || return 1
    assert_eq "${name}" "$(field "${named}" hint_pc)" "$(printf '%x' "$((0x${ZCA_HINT} + 4))")" \
        "single-step PC over the HINT" || return 1
    assert_eq "${name}" "$(field "${named}" hint_after_a1)" "7" "HINT left a1 unchanged" || return 1

    assert_eq "${name}" "$(field "${named}" halt_pc)" "${ZCA_SPIN}" "halt PC" || return 1
    assert_eq "${name}" "$(field "${named}" halt_dpc)" "${ZCA_SPIN}" "halt dpc" || return 1
    if [[ "${name}" == "spike-zca" ]]; then
        assert_eq "${name}" "$(field "${named}" reset_pc)" "1000" "reset PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" reset_pc)" "${ZCA_ENTRY}" "reset PC" || return 1
    fi
    assert_eq "${name}" "$(field "${named}" dcsr_debugver)" "4" "dcsr.debugver" || return 1
    assert_eq "${name}" "$(field "${named}" dcsr_prv)" "3" "dcsr.prv" || return 1
}

# The debugger writes the breakpoint itself: a 2-byte software breakpoint
# where the guest has a 2-byte instruction, which only a compressed-capable
# target accepts as C.EBREAK, and a 4-byte software breakpoint where the guest
# has a 4-byte instruction at pc % 4 == 2. The successful "added software
# breakpoint" row is the authoritative evidence for both, because the pinned
# Spike refuses a multi-word abstract write: it reaches the 4-byte breakpoint
# through an aligned 8-byte read-modify-write, while this profile writes the
# two 2-byte parcels directly. Both end with the same breakpoint in memory.
assert_zca_evidence() {
    local name="$1" dir="$2"
    local log="${dir}/openocd.log"
    local len2 len4
    len2="$(printf '0x%x' "0x$(zca_symbol zca_len2)")"
    len4="$(printf '0x%x' "0x$(zca_symbol zca_len4)")"
    if ! grep -aqF "added software breakpoint at ${len2} of length 0x00000002" "${log}"; then
        echo "${name}: no 2-byte software breakpoint at ${len2}" >&2
        return 1
    fi
    if ! grep -aqF "added software breakpoint at ${len4} of length 0x00000004" "${log}"; then
        echo "${name}: no 4-byte software breakpoint at ${len4}" >&2
        return 1
    fi
    if ! grep -aqF "writing 1 words of 2 bytes from ${len2}" "${log}"; then
        echo "${name}: no 2-byte C.EBREAK write at ${len2}" >&2
        return 1
    fi
    if grep -aq 'Failed to write 2-byte breakpoint instruction' "${log}" \
        || grep -aq 'Failed to write 4-byte breakpoint instruction' "${log}"; then
        echo "${name}: a breakpoint write was rejected" >&2
        return 1
    fi
    if ! grep -aq 'Examined RISC-V core' "${log}"; then
        echo "${name}: OpenOCD did not examine the compressed hart" >&2
        return 1
    fi
}

# The D10 comparison: the same named behavior on both targets. Spike runs its
# existing ISA string plus `c` (rv64imac_zicsr). Only the reset entry
# point and the dcsr fields the abstract session already documents differ.
# The per-stop dcsr rows differ only in those same D5 bits, so they are
# compared with stopcount, stoptime, and mprven masked out.
compare_zca_named() {
    local left="$1" right="$2"
    local keys=()
    read -r -a keys <<<"$(zca_u5_keys)"
    local key a b
    for key in "${keys[@]}"; do
        case "${key}" in
            pc|reset_pc|dcsr|break2_dcsr|break4_dcsr) continue ;;
        esac
        a="$(field "${left}" "${key}")"
        b="$(field "${right}" "${key}")"
        if [[ "${a}" != "${b}" ]]; then
            echo "Spike comparison mismatch on ${key}: simulator ${a}, spike ${b}" >&2
            return 1
        fi
        printf 'match %s=%s\n' "${key}" "${a}"
    done
    for key in break2_dcsr break4_dcsr; do
        a="$(zca_dcsr_common "$(field "${left}" "${key}")")"
        b="$(zca_dcsr_common "$(field "${right}" "${key}")")"
        if [[ "${a}" != "${b}" ]]; then
            echo "Spike comparison mismatch on ${key} without the D5 bits: simulator ${a}, spike ${b}" >&2
            return 1
        fi
        printf 'match %s=%s (D5 bits masked)\n' "${key}" "${a}"
    done
    for key in dcsr_debugver dcsr_ebreakm dcsr_ebreaks dcsr_ebreaku dcsr_step dcsr_prv \
        dcsr_common; do
        a="$(field "${left}" "${key}")"
        b="$(field "${right}" "${key}")"
        if [[ "${a}" != "${b}" ]]; then
            echo "Spike comparison mismatch on ${key}: simulator ${a}, spike ${b}" >&2
            return 1
        fi
        printf 'match %s=%s\n' "${key}" "${a}"
    done
    note_divergence "${left}" "${right}" pc "$(norm "${ZCA_ENTRY}")" 1000 \
        "Spike --halted stops at its boot ROM (0x1000). This profile has no boot ROM and stops at the ELF entry. The GDB script then writes PC to _start before the guest operations."
    note_divergence "${left}" "${right}" reset_pc "$(norm "${ZCA_ENTRY}")" 1000 \
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

# The dcsr value with the D5-only bits removed, for the per-stop rows. cause,
# debugver, ebreak* and prv stay because both targets must agree on them.
zca_dcsr_common() {
    python3 - "$1" <<'PY'
import sys
value = int(sys.argv[1], 16)
print(hex(value & ~((1 << 10) | (1 << 9) | (1 << 4))))
PY
}

# The U6 named outputs, in the order the session prints them.
fd_u6_keys() {
    printf 'connect pc mstatus fcsr_init f0_init fcsr mstatus_writes'
    local index
    for ((index = 0; index < 32; index++)); do
        printf ' f%s' "${index}"
    done
    printf ' load_pc load_mstatus load_fcsr load_f0 load_f31'
    printf ' load_step_pc load_step_mstatus load_f8'
    printf ' add_pc add_mstatus add_step_pc add_step_mstatus add_f10'
    printf ' store_pc store_mstatus store_step_pc store_step_mstatus store_word'
    printf ' fcsr_end continue_sent'
}

# The rv64imafdc FP session. The guest verifies the debugger-written FP
# state, then the three stops prove FS and the FP effect of an FP load, an
# FP arithmetic instruction and an FP store through one debugger path.
run_fd_session() {
    local name="$1"
    local named="$3"
    local SESSION_ELF="${WORK}/fdc-guest.elf"
    start_session "${name}" "$2" "${GDB_FD_SCRIPT}" 'U6 continue_sent=' -d3 || return 1
    local keys=()
    read -r -a keys <<<"$(fd_u6_keys)"
    extract_named "${named}" "${WORK}/${name}/gdb.log" U6 norm "${keys[@]}" || return 1
    assert_fd_session "${name}" "${named}" || return 1
    assert_fd_evidence "${name}" "${WORK}/${name}" || return 1
    echo "${name}: FP session ok (target exit 0)"
}

# Every named value the session produced. f0-f31 are i + 0.5, so a stale,
# repeated or shifted register fails here as well as in the guest.
assert_fd_session() {
    local name="$1" named="$2"
    local index
    assert_eq "${name}" "$(field "${named}" connect)" "ok" "connect" || return 1
    if [[ "${name}" == spike-* ]]; then
        assert_eq "${name}" "$(field "${named}" pc)" "1000" "connect PC" || return 1
    else
        assert_eq "${name}" "$(field "${named}" pc)" "${FD_ENTRY}" "connect PC" || return 1
    fi
    assert_eq "${name}" "$(fs_field "$(field "${named}" mstatus)")" "0" "FS at connect" \
        || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" mstatus)")" "0" "SD at connect" \
        || return 1
    assert_eq "${name}" "$(field "${named}" fcsr_init)" "0" "fcsr at connect" || return 1
    assert_eq "${name}" "$(field "${named}" f0_init)" "0" "f0 at connect" || return 1
    for ((index = 0; index < 32; index++)); do
        assert_eq "${name}" "$(field "${named}" "f${index}")" \
            "$(fd_expected_bits "${index}.5")" "f${index} readback" || return 1
    done
    assert_eq "${name}" "$(field "${named}" fcsr)" "7b" "fcsr after the debugger write" \
        || return 1
    # Contract D10: abstract FPR and fcsr access leaves FS unchanged. The
    # guest checks the same FS Off state from the hart before it sets FS.
    assert_eq "${name}" "$(fs_field "$(field "${named}" mstatus_writes)")" "0" \
        "FS after the abstract FP writes" || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" mstatus_writes)")" "0" \
        "SD after the abstract FP writes" || return 1

    assert_eq "${name}" "$(field "${named}" load_pc)" "${FD_LOAD}" "FLD breakpoint PC" \
        || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" load_mstatus)")" "2" \
        "FS before the FLD" || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" load_mstatus)")" "0" \
        "SD before the FLD" || return 1
    assert_eq "${name}" "$(field "${named}" load_fcsr)" "7b" "fcsr at the FLD breakpoint" \
        || return 1
    assert_eq "${name}" "$(field "${named}" load_f0)" "$(fd_expected_bits 0.5)" \
        "f0 read at the FLD breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" load_f31)" "$(fd_expected_bits 31.5)" \
        "f31 read at the FLD breakpoint" || return 1
    assert_eq "${name}" "$(field "${named}" load_step_pc)" \
        "$(printf '%x' "$((0x${FD_LOAD} + 4))")" "PC after stepping the FLD" || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" load_step_mstatus)")" "3" \
        "FS after the FLD" || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" load_step_mstatus)")" "1" \
        "SD after the FLD" || return 1
    assert_eq "${name}" "$(field "${named}" load_f8)" "4008000000000000" \
        "f8 after the FLD" || return 1

    assert_eq "${name}" "$(field "${named}" add_pc)" "${FD_ADD}" "FADD.D breakpoint PC" \
        || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" add_mstatus)")" "2" \
        "FS before the FADD.D" || return 1
    assert_eq "${name}" "$(field "${named}" add_step_pc)" \
        "$(printf '%x' "$((0x${FD_ADD} + 4))")" "PC after stepping the FADD.D" || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" add_step_mstatus)")" "3" \
        "FS after the FADD.D" || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" add_step_mstatus)")" "1" \
        "SD after the FADD.D" || return 1
    assert_eq "${name}" "$(field "${named}" add_f10)" "4029000000000000" \
        "f10 after the FADD.D" || return 1

    assert_eq "${name}" "$(field "${named}" store_pc)" "${FD_STORE}" "FSW breakpoint PC" \
        || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" store_mstatus)")" "2" \
        "FS before the FSW" || return 1
    assert_eq "${name}" "$(field "${named}" store_step_pc)" \
        "$(printf '%x' "$((0x${FD_STORE} + 4))")" "PC after stepping the FSW" || return 1
    assert_eq "${name}" "$(fs_field "$(field "${named}" store_step_mstatus)")" "2" \
        "FS after the FSW" || return 1
    assert_eq "${name}" "$(sd_field "$(field "${named}" store_step_mstatus)")" "0" \
        "SD after the FSW" || return 1
    assert_eq "${name}" "$(field "${named}" store_word)" "3fc00000" \
        "word stored by the FSW" || return 1
    assert_eq "${name}" "$(field "${named}" fcsr_end)" "7b" "fcsr after the FP instructions" \
        || return 1
    assert_eq "${name}" "$(field "${named}" continue_sent)" "ok" "final continue" \
        || return 1
}

# The OpenOCD 0.12.0 log must show the FP profile's misa, a 4-byte software
# breakpoint at each FP stop, a written value for f0/f31 and fcsr, a fresh DM
# read of every written FPR, and no fallback from abstract FPR access to the
# program buffer. OpenOCD caches FPRs after a write; the FPR read rows prove
# the session's `monitor halt` invalidated that cache and every readback came
# from the target.
assert_fd_evidence() {
    local name="$1" dir="$2"
    local log="${dir}/openocd.log"
    if ! grep -aqF -- 'misa=0x800000000014112d' "${log}"; then
        echo "${name}: OpenOCD did not report the rv64imafdc misa" >&2
        return 1
    fi
    local address hex
    for address in "${FD_LOAD}" "${FD_ADD}" "${FD_STORE}"; do
        hex="$(printf '0x%x' "0x${address}")"
        if ! grep -aqF -- "added software breakpoint at ${hex} of length 0x00000004" "${log}"; then
            echo "${name}: no 4-byte software breakpoint at ${hex}" >&2
            return 1
        fi
    done
    local pattern
    for pattern in \
        'register_write_direct(): {0} fpr0 <- 0x3fe0000000000000' \
        'register_write_direct(): {0} fpr31 <- 0x403f800000000000' \
        'write 0x000000000000007b to fcsr' \
        'wrote 0x7b to fcsr'
    do
        if ! grep -aqF -- "${pattern}" "${log}"; then
            echo "${name}: OpenOCD log lacks [${pattern}]" >&2
            return 1
        fi
    done
    local index bits register
    for ((index = 0; index < 32; index++)); do
        bits="$(fd_expected_bits "${index}.5")"
        if (( index == 0 )); then
            register="fpr0"
        elif (( index == 31 )); then
            register="fpr31"
        else
            register="f${index}"
        fi
        if ! grep -aqF -- "register_read_direct(): {0} ${register} = 0x${bits}" "${log}"; then
            echo "${name}: no fresh DM read of f${index} (0x${bits})" >&2
            return 1
        fi
    done
    if grep -aq 'Disabling abstract command reads from FPRs' "${log}" \
        || grep -aq 'Disabling abstract command writes to FPRs' "${log}"; then
        echo "${name}: OpenOCD fell back from abstract FPR access" >&2
        return 1
    fi
}

# The D12 comparison: every named value matches except the connect PC, which
# is the same boot-ROM difference the other sessions document.
compare_fd_named() {
    local left="$1" right="$2"
    local keys=()
    read -r -a keys <<<"$(fd_u6_keys)"
    local key a b
    for key in "${keys[@]}"; do
        if [[ "${key}" == "pc" ]]; then
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
    note_divergence "${left}" "${right}" pc "$(norm "${FD_ENTRY}")" 1000 \
        "Spike --halted stops at its boot ROM (0x1000). This profile has no boot ROM and stops at the ELF entry. The GDB script then writes PC to _start before the guest operations."
}

ruscv_cmd() {
    local port="$1"
    exec "${RUSCV_SIM}" run "${WORK}/guest.elf" --isa rv64ima \
        --rbb-port "${port}" --halted --max-cycles 100000000
}

# The Zca U5 target: the compressed guest under the rv64imac profile.
ruscv_zca_cmd() {
    local port="$1"
    exec "${RUSCV_SIM}" run "${WORK}/zca-guest.elf" --isa rv64imac \
        --rbb-port "${port}" --halted --max-cycles 100000000
}

# The direct-memory U3 target: the same attach guest with the same rv64ima
# session profile, on the direct-memory route (`--dmi` requires `--transport
# tlm`).
ruscv_direct_mem_cmd() {
    local port="$1"
    exec "${RUSCV_SIM}" run "${WORK}/guest.elf" --isa rv64ima \
        --transport tlm --dmi \
        --rbb-port "${port}" --halted --max-cycles 100000000
}

# The F/D U6 target: the FP session guest under the rv64imafdc profile.
# This is the D7 default-proof session: the default is rv64imafdc since the
# RV64GC U1 flip, so the F/D session runs with no --isa and proves the
# default (contract D7). The Spike side keeps its explicit profile string.
FDC_ISA_ARGS=()
ruscv_fdc_cmd() {
    local port="$1"
    exec "${RUSCV_SIM}" run "${WORK}/fdc-guest.elf" "${FDC_ISA_ARGS[@]+"${FDC_ISA_ARGS[@]}"}" \
        --rbb-port "${port}" --halted --max-cycles 100000000
}

spike_available() {
    command -v spike >/dev/null 2>&1 || return 1
    local help
    help="$(spike --help 2>&1 || true)"
    grep -q -- '--dm-datacount' <<<"${help}" \
        && grep -q -- '--halted' <<<"${help}" \
        && grep -q -- '--dm-progsize' <<<"${help}" \
        && grep -q -- '--dm-no-impebreak' <<<"${help}" \
        && grep -q -- '--dm-sba' <<<"${help}"
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

# The U4 system-bus comparison target: --dm-sba=64 enables Spike's SBA block
# at the contract's sbasize. progbufsize stays 0 because this session never
# uses the program buffer, and datacount stays at this profile's four.
spike_sysbus_cmd() {
    local port="$1"
    exec spike --halted \
        --isa=rv64ima_zicsr \
        --priv=msu \
        --pc="0x${ENTRY}" \
        --dm-progsize=0 \
        --dm-no-impebreak \
        --dm-datacount=4 \
        --dm-sba=64 \
        --rbb-port="${port}" \
        "${WORK}/guest.elf"
}

# The Zca U5 comparison target: the compressed guest under the rv64imac
# profile. Spike's grammar keeps single-letter extensions before the
# underscore, so its existing rv64ima_zicsr string plus `c` is rv64imac_zicsr.
spike_zca_cmd() {
    local port="$1"
    exec spike --halted \
        --isa=rv64imac_zicsr \
        --priv=msu \
        --pc="0x${ZCA_ENTRY}" \
        --dm-progsize=0 \
        --dm-no-impebreak \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/zca-guest.elf"
}

# The F/D U6 comparison target: D12's rv64imafdc_zicsr ISA string and the
# same debug-module shape as the other sessions.
spike_fdc_cmd() {
    local port="$1"
    exec spike --halted \
        --isa=rv64imafdc_zicsr \
        --priv=msu \
        --pc="0x${FD_ENTRY}" \
        --dm-progsize=0 \
        --dm-no-impebreak \
        --dm-datacount=4 \
        --rbb-port="${port}" \
        "${WORK}/fdc-guest.elf"
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
        "${RUSCV_SIM}" run "${WORK}/guest.elf" --isa rv64ima \
        --rbb-port "${port}" --halted --max-cycles 100000000 \
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
        "${RUSCV_SIM}" run "${WORK}/guest.elf" --isa rv64ima \
        --rbb-port "${port}" --halted --max-cycles 100000000 \
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

# The U4 system-bus comparison: the same addresses, values, and sberror
# taxonomy on both targets. reset_pc differs for the same reason as the
# abstract session. The two raw-DMI rows are the contract's pre-identified
# pinned-Spike deviations; compare_progbuf_named's equality loop would fail
# them, so they are compared as named divergences here.
compare_sysbus_named() {
    local left="$1" right="$2"
    local keys=()
    read -r -a keys <<<"$(u4_sysbus_keys)"
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
    note_divergence "${left}" "${right}" sberror_misaligned 3 2 \
        "The contract follows the specification: a misaligned system-bus access is not performed and reports sberror 3. The pinned Spike performs no alignment check, so its misaligned word load traps in the memory system and lands as sberror 2."
    note_divergence "${left}" "${right}" sberror_unsupported 4 3 \
        "The contract follows the specification: an unsupported sbaccess size reports sberror 4. The pinned Spike reports 3 for it."
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
    if kill -0 "${target_pid}" 2>/dev/null; then
        kill "${target_pid}" 2>/dev/null || true
        wait "${target_pid}" 2>/dev/null || true
    fi
    if kill -0 "${openocd_pid}" 2>/dev/null; then
        kill "${openocd_pid}" 2>/dev/null || true
        wait "${openocd_pid}" 2>/dev/null || true
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
run_sysbus_session ruscv-sysbus ruscv_cmd "${WORK}/ruscv-sysbus.named"
run_direct_mem_session ruscv-direct-mem ruscv_direct_mem_cmd "${WORK}/ruscv-direct-mem.named"
run_zca_session ruscv-zca ruscv_zca_cmd "${WORK}/ruscv-zca.named"
run_fd_session ruscv-fd ruscv_fdc_cmd "${WORK}/ruscv-fd.named"
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
    run_sysbus_session spike-sysbus spike_sysbus_cmd "${WORK}/spike-sysbus.named"
    compare_sysbus_named "${WORK}/ruscv-sysbus.named" "${WORK}/spike-sysbus.named" \
        | tee "${WORK}/sysbus-comparison.txt"
    run_zca_session spike-zca spike_zca_cmd "${WORK}/spike-zca.named"
    compare_zca_named "${WORK}/ruscv-zca.named" "${WORK}/spike-zca.named" \
        | tee "${WORK}/zca-comparison.txt"
    run_fd_session spike-fd spike_fdc_cmd "${WORK}/spike-fd.named"
    compare_fd_named "${WORK}/ruscv-fd.named" "${WORK}/spike-fd.named" \
        | tee "${WORK}/fd-comparison.txt"
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
    tar -C "${WORK}" -cf - --exclude='guest.o' --exclude='guest.elf' \
        --exclude='zca-guest.o' --exclude='zca-guest.elf' \
        --exclude='fdc-guest.o' --exclude='fdc-guest.elf' . \
        | tar -C "${EVIDENCE}" -xf -
    echo "evidence copied to ${EVIDENCE}"
fi

echo "debug-attach ok"
