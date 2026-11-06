#!/bin/bash
# quantum U3 (quantum D6/D8/D10): the fact checks of the live debug-attach
# session under `run --transport tlm --quantum 1024`.
#
# scripts/debug_attach.sh sources this file and calls `quantum_session_facts`
# with the session's named values and target log.
# scripts/test_quantum_session_facts.sh exercises the same function directly
# with crafted fact files, so every rejection below has a control that does not
# need OpenOCD, GDB or a RISC-V toolchain.
#
# The function prints one reason to stderr and returns 1 on a rejected fact, so
# the caller's failure message names the check that failed.

# Strip `0x` and leading zeros, the same normalization the attach script uses.
quantum_norm() {
    local value="${1#0x}"
    value="${value#0X}"
    value="$(printf '%s' "${value}" | tr 'A-F' 'a-f' | sed 's/^0*//')"
    printf '%s' "${value:-0}"
}

# One `key=value` row from a named-values file.
quantum_field() {
    local file="$1" key="$2"
    awk -F= -v key="${key}" '$1 == key { print substr($0, index($0, "=") + 1); exit }' "${file}"
}

# quantum_narrow VALUE
#   Prints VALUE as a decimal integer and fails above 2^63-1, the range bash's
#   signed arithmetic represents exactly. A 64-bit subtraction that underflowed
#   sits at 2^64 - n, so it fails here instead of becoming a small negative
#   number that a `> bound` test would accept (quantum D8: MTIP is never early,
#   so 0 <= R - X).
quantum_narrow() {
    local value
    value="$(quantum_norm "$1")"
    if [[ ! "${value}" =~ ^[0-9a-f]+$ ]]; then
        return 1
    fi
    if (( ${#value} > 16 )); then
        return 1
    fi
    if (( ${#value} == 16 )) && [[ "${value:0:1}" > "7" ]]; then
        return 1
    fi
    printf '%d' "$(( 0x${value} ))"
}

# quantum_session_facts NAME NAMED TARGET SPIN_HEX
#   NAME       the session name, for the failure message
#   NAMED      the named-values file the session wrote
#   TARGET     the target's stdout/stderr log
#   SPIN_HEX   the spin label's address in the session image
#
# Accepts only a single timer recognition that interrupted the spin, a compare
# armed from the guest's own sample, a halt inside the quantum before that
# compare, and a lateness inside the D8 bound. The lateness is never compared
# with an exact value: a debug session's sync points depend on debugger timing
# (quantum D6), so only `0 <= R - X <= Q - 1 + t` is a fact here.
quantum_session_facts() {
    local name="$1" named="$2" target="$3" spin="$4"
    local expected
    local value

    for value in connect halted resume; do
        expected="$(quantum_field "${named}" "${value}")"
        if [[ "${expected}" != "ok" ]]; then
            echo "${name}: ${value} is ${expected:-missing}, expected ok" >&2
            return 1
        fi
    done

    # quantum F9: install leaves committed time at zero, so the system-bus read
    # at the initial halt is zero on a decoupled run too.
    if [[ "$(quantum_field "${named}" mtime_halted)" != "0" ]]; then
        echo "${name}: mtime at the initial halt is $(quantum_field "${named}" mtime_halted), expected 0" >&2
        return 1
    fi

    # quantum D8: the trap is the timer interrupt the guest armed, and it
    # interrupted the spin exactly once.
    if [[ "$(quantum_field "${named}" mcause)" != "8000000000000007" ]]; then
        echo "${name}: mcause is $(quantum_field "${named}" mcause), expected the machine timer interrupt" >&2
        return 1
    fi
    if [[ "$(quantum_norm "$(quantum_field "${named}" mepc)")" != "$(quantum_norm "${spin}")" ]]; then
        echo "${name}: mepc is $(quantum_field "${named}" mepc), expected the spin at ${spin}" >&2
        return 1
    fi
    if [[ "$(quantum_field "${named}" traps)" != "1" ]]; then
        echo "${name}: the guest recorded $(quantum_field "${named}" traps) recognitions, expected 1" >&2
        return 1
    fi

    local arm compare mtime_break trap_time lateness
    if ! arm="$(quantum_narrow "$(quantum_field "${named}" arm)")"; then
        echo "${name}: the arming sample is not a Platform tick value" >&2
        return 1
    fi
    if ! compare="$(quantum_narrow "$(quantum_field "${named}" compare)")"; then
        echo "${name}: the armed compare is not a Platform tick value" >&2
        return 1
    fi
    if ! mtime_break="$(quantum_narrow "$(quantum_field "${named}" mtime_break)")"; then
        echo "${name}: the halted mtime is not a Platform tick value" >&2
        return 1
    fi
    if ! trap_time="$(quantum_narrow "$(quantum_field "${named}" trap_time)")"; then
        echo "${name}: the handler's time is not a Platform tick value" >&2
        return 1
    fi
    if ! lateness="$(quantum_narrow "$(quantum_field "${named}" lateness)")"; then
        echo "${name}: the recorded lateness is not a Platform tick count" >&2
        return 1
    fi

    if (( compare - arm != 64 )); then
        echo "${name}: the guest armed $(( compare - arm )) ticks past its sample, expected 64" >&2
        return 1
    fi
    # The halt is inside the quantum: committed time is at or past the arming
    # sample and still below the compare, so the crossing happens after the
    # resume and not before the debugger lets the Hart run again.
    if (( mtime_break < arm || mtime_break >= compare )); then
        echo "${name}: the halted mtime ${mtime_break} is outside [${arm}, ${compare})" >&2
        return 1
    fi
    # The guest's own accounting: the handler's first `time` is the trap turn
    # plus one, so `trap_time - 1 - compare` is the lateness it stored.
    if (( trap_time - 1 - compare != lateness )); then
        echo "${name}: trap time ${trap_time} does not match lateness ${lateness}" >&2
        return 1
    fi
    # quantum D8/D6: 0 <= R - X <= Q - 1 + t, and never one exact value.
    local bound=1023
    if (( lateness < 0 || lateness > bound )); then
        echo "${name}: the interrupt lateness ${lateness} is outside the D8 bound [0, ${bound}]" >&2
        return 1
    fi

    # quantum D10: the run reports the same bound and the reduced-accuracy
    # request, so the session's `mtime` reads and the report agree.
    if ! grep -qF -- 'Quantum:    1024 ticks' "${target}"; then
        echo "${name}: the target report carries no quantum line" >&2
        return 1
    fi
    if ! grep -qF -- 'latency bound 1023 ticks (reduced accuracy)' "${target}"; then
        echo "${name}: the target report does not name the D8 bound" >&2
        return 1
    fi
    return 0
}
