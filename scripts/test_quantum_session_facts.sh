#!/bin/bash
# Regression tests for the quantum debug-attach fact checks (quantum U3,
# quantum D6/D8/D10).
#
# The live session needs OpenOCD, GDB and a RISC-V toolchain, so these cases
# exercise the same `quantum_session_facts` function directly with crafted fact
# files. They prove the rejection paths the session relies on, most importantly
# a wrapped (negative) lateness, which signed shell arithmetic would otherwise
# compare as a small number and accept.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

if ! source "${SCRIPT_DIR}/debug/quantum_session_facts.sh"; then
    echo "[FAIL] cannot load the quantum session fact checks" >&2
    exit 2
fi

failures=0
temporary="$(mktemp -d "${TMPDIR:-/tmp}/quantum-session-facts.XXXXXX")"
trap 'rm -rf -- "${temporary}"' EXIT

report() {
    local syncs="${1:-1}"
    printf 'Cycles:     128\n'
    printf 'Quantum:    1024 ticks, %s syncs, latency bound 1023 ticks (reduced accuracy)\n' "${syncs}"
}

# write_named FILE KEY=VALUE...
write_named() {
    local file="$1"
    shift
    : >"${file}"
    local row
    for row in "$@"; do
        printf '%s\n' "${row}" >>"${file}"
    done
}

# A valid session: the compare is armed 64 ticks past the sample, the halt sits
# inside the quantum, and the trap is one timer interrupt with a 10-tick
# lateness (trap_time = 1 + compare + lateness).
valid_facts() {
    local file="$1" lateness="$2" trap_time="$3"
    write_named "${file}" \
        "connect=ok" \
        "mtime_halted=0" \
        "halted=ok" \
        "mtime_break=11" \
        "arm=3" \
        "compare=43" \
        "mcause=8000000000000007" \
        "mepc=80000044" \
        "traps=1" \
        "trap_time=${trap_time}" \
        "lateness=${lateness}" \
        "resume=ok"
}

assert_accept() {
    local label="$1" named="$2" target="$3"
    if quantum_session_facts "case" "${named}" "${target}" "80000044"; then
        echo "[PASS] ${label}: accepted"
    else
        echo "[FAIL] ${label}: expected acceptance" >&2
        failures=$((failures + 1))
    fi
}

assert_reject() {
    local label="$1" named="$2" target="$3" expected="$4"
    local reason
    if reason="$(quantum_session_facts "case" "${named}" "${target}" "80000044" 2>&1)"; then
        echo "[FAIL] ${label}: expected rejection, but the facts were accepted" >&2
        failures=$((failures + 1))
        return
    fi
    case "${reason}" in
        *"${expected}"*)
            echo "[PASS] ${label}: rejected (${expected})"
            ;;
        *)
            echo "[FAIL] ${label}: rejected for the wrong reason: ${reason}" >&2
            failures=$((failures + 1))
            ;;
    esac
}

good_target="${temporary}/good.target"
report 1 >"${good_target}"
good="${temporary}/good.named"
valid_facts "${good}" "a" "4e"
assert_accept "a 10-tick lateness inside the bound" "${good}" "${good_target}"

# A zero lateness is legal: D8 allows a sync that lands exactly on the compare.
zero="${temporary}/zero.named"
valid_facts "${zero}" "0" "44"
assert_accept "a coincident (zero) lateness" "${zero}" "${good_target}"

# The negative control this file exists for: a wrapped subtraction. The guest
# reports 0xffffffffffffffff, which signed arithmetic reads as -1.
underflow="${temporary}/underflow.named"
valid_facts "${underflow}" "ffffffffffffffff" "44"
assert_reject "a wrapped (negative) lateness" "${underflow}" "${good_target}" \
    "is not a Platform tick count"

# One tick above `Q - 1 + t` with a consistent accounting row.
over="${temporary}/over.named"
valid_facts "${over}" "400" "444"
assert_reject "a lateness above the D8 bound" "${over}" "${good_target}" \
    "outside the D8 bound"

# The guest's arithmetic must agree with its own trap time.
mismatch="${temporary}/mismatch.named"
valid_facts "${mismatch}" "a" "44"
assert_reject "an accounting mismatch" "${mismatch}" "${good_target}" \
    "does not match lateness"

# A trap that is not the machine timer interrupt, or not the spin.
cause="${temporary}/cause.named"
valid_facts "${cause}" "a" "4e"
sed -i 's/^mcause=.*/mcause=8000000000000005/' "${cause}"
assert_reject "a non-timer trap" "${cause}" "${good_target}" \
    "expected the machine timer interrupt"

epc="${temporary}/epc.named"
valid_facts "${epc}" "a" "4e"
sed -i 's/^mepc=.*/mepc=80000048/' "${epc}"
assert_reject "a trap outside the spin" "${epc}" "${good_target}" \
    "expected the spin at"

# Exactly one recognition.
twice="${temporary}/twice.named"
valid_facts "${twice}" "a" "4e"
sed -i 's/^traps=.*/traps=2/' "${twice}"
assert_reject "two recognitions" "${twice}" "${good_target}" \
    "expected 1"

# The halt must be inside the quantum, after arming and before the compare.
late_halt="${temporary}/late-halt.named"
valid_facts "${late_halt}" "a" "4e"
sed -i 's/^mtime_break=.*/mtime_break=43/' "${late_halt}"
assert_reject "a halt at the compare" "${late_halt}" "${good_target}" \
    "is outside"

# A wholly absent row is not a zero: a missing arming sample must fail.
missing="${temporary}/missing.named"
valid_facts "${missing}" "a" "4e"
sed -i '/^arm=/d' "${missing}"
assert_reject "a missing field" "${missing}" "${good_target}" \
    "the arming sample is not a Platform tick value"

missing_lateness="${temporary}/missing-lateness.named"
valid_facts "${missing_lateness}" "a" "4e"
sed -i '/^lateness=/d' "${missing_lateness}"
assert_reject "a missing lateness field" "${missing_lateness}" "${good_target}" \
    "the recorded lateness is not a Platform tick count"

# The withheld report line: the session must see the same D8 bound.
no_line="${temporary}/no-line.target"
printf 'Cycles:     128\n' >"${no_line}"
assert_reject "a report with no quantum line" "${good}" "${no_line}" \
    "carries no quantum line"

wrong_bound="${temporary}/wrong-bound.target"
report 1 | sed 's/latency bound 1023 ticks/latency bound 1000 ticks/' >"${wrong_bound}"
assert_reject "a report with another bound" "${good}" "${wrong_bound}" \
    "does not name the D8 bound"

if [ "${failures}" -ne 0 ]; then
    echo "[FAILED] ${failures} quantum session fact case(s)" >&2
    exit 1
fi

echo "[PASS] all quantum session fact cases"
