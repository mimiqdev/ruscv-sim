#!/bin/bash
# Shared pass/fail classification for one public-CLI ELF run.
#
# Source this file; it defines functions only.  A run passes only when the CLI
# exited 0 AND its result block reports exactly one status line, which is
# `Status:     SUCCESS` (see print_result in src/main.rs).  Exit status alone is
# not trusted: a host keeps only the low 8 bits of a process status.

# The exact status line printed by the CLI for a successful run.
RISCV_CLI_SUCCESS_LINE='Status:     SUCCESS'

# riscv_classify_cli_run STATUS OUTPUT
#   Returns 0 for a passing run.  Otherwise prints the reason on stdout and
#   returns 1.
riscv_classify_cli_run() {
    local status="$1"
    local output="$2"
    local status_lines
    if [ "${status}" -ne 0 ]; then
        echo "CLI exit status ${status}, expected 0"
        return 1
    fi
    status_lines="$(printf '%s\n' "${output}" | grep -E '^Status:' || true)"
    if [ -z "${status_lines}" ]; then
        echo "CLI output has no Status line"
        return 1
    fi
    if [ "${status_lines}" != "${RISCV_CLI_SUCCESS_LINE}" ]; then
        echo "CLI exited 0 but reported: $(printf '%s' "${status_lines}" | tr '\n' ' ')"
        return 1
    fi
    return 0
}
