#!/bin/bash
# Regression tests for the disposable ELF output-path guard.
#
# These tests validate protected paths only; they never invoke the cleanup
# operation against the repository and only remove an isolated temporary tree.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd -P)"

if ! source "${SCRIPT_DIR}/riscv_elf_paths.sh"; then
    echo "[FAIL] cannot load path validation helper" >&2
    exit 2
fi

failures=0

assert_rejected() {
    local label="$1"
    local path="$2"
    local output
    if output=$(riscv_validate_output_dir "${PROJECT_DIR}" "${path}" 2>&1); then
        echo "[FAIL] ${label}: accepted unsafe path ${path}" >&2
        failures=$((failures + 1))
    else
        echo "[PASS] ${label}: rejected ${path}"
    fi
}

# These are protected repository/source paths.  The trailing slash and lexical
# aliases cover the bypasses that an exact-string comparison misses.
assert_rejected "repository with trailing slash" "${PROJECT_DIR}/"
assert_rejected "repository root" "${PROJECT_DIR}"
assert_rejected "bare-metal source directory" "${PROJECT_DIR}/tests/bare-metal-riscv-test/rv64i"
assert_rejected "parent alias into source" "${PROJECT_DIR}/target/../tests/bare-metal-riscv-test/rv64i"
assert_rejected "target root" "${PROJECT_DIR}/target"
assert_rejected "dot output component" "${PROJECT_DIR}/target/a6-path-guard-probe/."
assert_rejected "dot-dot output component" "${PROJECT_DIR}/target/a6-path-guard-probe/.."

# An output symlink and a symlinked target root must not be followed during
# cleanup.  Both cases live entirely under a temporary directory.
temporary="$(mktemp -d "${TMPDIR:-/tmp}/ruscv-elf-guards.XXXXXX")"
cleanup() {
    rm -rf -- "${temporary}"
}
trap cleanup EXIT

mkdir -p -- "${temporary}/project/target" "${temporary}/outside"
ln -s -- "${temporary}/outside" "${temporary}/project/target/output-link"
assert_rejected "output symlink" "${temporary}/project/target/output-link"
ln -s -- "${temporary}/project/target" "${temporary}/project/target/ancestor-link"
assert_rejected "symlinked ancestor" "${temporary}/project/target/ancestor-link/output"

mkdir -p -- "${temporary}/symlink-project" "${temporary}/symlink-target"
ln -s -- "${temporary}/symlink-target" "${temporary}/symlink-project/target"
assert_rejected "target-root symlink" "${temporary}/symlink-project/target/output"

if [ "${failures}" -ne 0 ]; then
    echo "[FAILED] ${failures} ELF output-path guard case(s)" >&2
    exit 1
fi

echo "[PASS] all ELF output-path guard cases"
