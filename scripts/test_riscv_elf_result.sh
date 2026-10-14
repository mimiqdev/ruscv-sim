#!/bin/bash
# Regression tests for the ELF suite's pass/fail classification.
#
# A guest that reports FAILED must be a failure even when the process exits 0
# (for example a guest code 0x10000400, whose low byte is 0).

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

if ! source "${SCRIPT_DIR}/riscv_elf_result.sh"; then
    echo "[FAIL] cannot load result classification helper" >&2
    exit 2
fi

failures=0

result_block() {
    local code="$1"
    local status="$2"
    printf '\n========== Execution Result ==========\n'
    printf 'Exit Code:  %s\n' "${code}"
    printf 'Cycles:     12\n'
    printf 'Final PC:   0x0000000080000010\n'
    printf 'Status:     %s\n' "${status}"
    printf '=====================================\n'
}

assert_pass() {
    local label="$1"
    local reason
    if reason=$(riscv_classify_cli_run "$2" "$3"); then
        echo "[PASS] ${label}: classified as pass"
    else
        echo "[FAIL] ${label}: expected pass, got failure: ${reason}" >&2
        failures=$((failures + 1))
    fi
}

assert_fail() {
    local label="$1"
    local expected="$4"
    local reason
    if reason=$(riscv_classify_cli_run "$2" "$3"); then
        echo "[FAIL] ${label}: failing run classified as pass" >&2
        failures=$((failures + 1))
    elif [[ "${reason}" != *"${expected}"* ]]; then
        echo "[FAIL] ${label}: expected '${expected}', got: ${reason}" >&2
        failures=$((failures + 1))
    else
        echo "[PASS] ${label}: failure (${reason})"
    fi
}

assert_pass "exit 0 with SUCCESS" 0 "$(result_block 0 SUCCESS)"
assert_pass "guest UART text before SUCCESS" 0 "Hello!
$(result_block 0 SUCCESS)"

assert_fail "exit 0 with FAILED (code 0x10000400)" 0 \
    "$(result_block 268436480 FAILED)" "reported: Status:     FAILED"
assert_fail "exit 0 with TIMEOUT" 0 "$(result_block 1 TIMEOUT)" "reported: Status:     TIMEOUT"
assert_fail "exit 0 without a Status line" 0 "Hello!" "no Status line"
assert_fail "exit 0 with empty output" 0 "" "no Status line"
assert_fail "exit 0 with SUCCESS and FAILED lines" 0 \
    "$(result_block 0 SUCCESS)
$(result_block 3 FAILED)" "reported:"
assert_fail "SUCCESS line not at line start" 0 \
    "guest says Status:     SUCCESS" "no Status line"
assert_fail "nonzero exit with SUCCESS" 1 "$(result_block 0 SUCCESS)" "CLI exit status 1"
assert_fail "nonzero exit with FAILED" 3 "$(result_block 3 FAILED)" "CLI exit status 3"

# End to end through a stand-in simulator: a guest that exits 0 while printing
# FAILED is counted as a failure by the same code path the suite uses.
temporary="$(mktemp -d "${TMPDIR:-/tmp}/ruscv-elf-result.XXXXXX")"
trap 'rm -rf -- "${temporary}"' EXIT
fake="${temporary}/fake-sim"
cat > "${fake}" <<'FAKE'
#!/bin/bash
printf '\n========== Execution Result ==========\n'
printf 'Exit Code:  268436480\n'
printf 'Status:     FAILED\n'
exit 0
FAKE
chmod +x "${fake}"
output=$("${fake}" run guest.elf 2>&1)
status=$?
assert_fail "stand-in simulator exits 0 with FAILED" "${status}" "${output}" "reported: Status:     FAILED"

if [ "${failures}" -ne 0 ]; then
    echo "[FAILED] ${failures} ELF result classification case(s)" >&2
    exit 1
fi

echo "[PASS] all ELF result classification cases"
