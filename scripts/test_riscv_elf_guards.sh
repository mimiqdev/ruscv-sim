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
    local project="$2"
    local path="$3"
    local expected="$4"
    local output
    if output=$(riscv_validate_output_dir "${project}" "${path}" 2>&1); then
        echo "[FAIL] ${label}: accepted unsafe path ${path}" >&2
        failures=$((failures + 1))
    elif [[ "${output}" != *"${expected}"* ]]; then
        echo "[FAIL] ${label}: expected '${expected}', got: ${output}" >&2
        failures=$((failures + 1))
    else
        echo "[PASS] ${label}: rejected ${path} (${expected})"
    fi
}

assert_accepted() {
    local label="$1"
    local project="$2"
    local path="$3"
    local expected="$4"
    local output
    if ! output=$(riscv_validate_output_dir "${project}" "${path}" 2>&1); then
        echo "[FAIL] ${label}: rejected ordinary path ${path}: ${output}" >&2
        failures=$((failures + 1))
    elif [ "${output}" != "${expected}" ]; then
        echo "[FAIL] ${label}: expected canonical path ${expected}, got: ${output}" >&2
        failures=$((failures + 1))
    else
        echo "[PASS] ${label}: accepted ${path}"
    fi
}

# These are protected repository/source paths.  The trailing slash and lexical
# aliases cover the bypasses that an exact-string comparison misses.
assert_rejected "repository with trailing slash" "${PROJECT_DIR}" "${PROJECT_DIR}/" "output must be directly below"
assert_rejected "repository root" "${PROJECT_DIR}" "${PROJECT_DIR}" "output must be directly below"
assert_rejected "bare-metal source directory" "${PROJECT_DIR}" "${PROJECT_DIR}/tests/bare-metal-riscv-test/rv64i" "output must be directly below"
assert_rejected "parent alias into source" "${PROJECT_DIR}" "${PROJECT_DIR}/target/../tests/bare-metal-riscv-test/rv64i" "output must be directly below"
assert_rejected "target root" "${PROJECT_DIR}" "${PROJECT_DIR}/target" "output must be directly below"
assert_rejected "dot output component" "${PROJECT_DIR}" "${PROJECT_DIR}/target/a6-path-guard-probe/." "output must be a named disposable directory"
assert_rejected "dot-dot output component" "${PROJECT_DIR}" "${PROJECT_DIR}/target/a6-path-guard-probe/.." "output must be a named disposable directory"

# An output symlink and a symlinked target root must not be followed during
# cleanup.  Both cases live entirely under a temporary directory.
temporary="$(mktemp -d "${TMPDIR:-/tmp}/ruscv-elf-guards.XXXXXX")"
cleanup() {
    rm -rf -- "${temporary}"
}
trap cleanup EXIT

mkdir -p -- "${temporary}/project/target" "${temporary}/outside"
temporary_project="$(cd -- "${temporary}/project" && pwd -P)"
temporary_outside="$(cd -- "${temporary}/outside" && pwd -P)"
assert_accepted "ordinary temporary output" "${temporary_project}" \
    "${temporary_project}/target/ordinary-output" \
    "${temporary_project}/target/ordinary-output"
ln -s -- "${temporary_outside}" "${temporary_project}/target/output-link"
assert_rejected "output symlink" "${temporary_project}" \
    "${temporary_project}/target/output-link" "output path is a symlink"
ln -s -- "${temporary_project}/target" "${temporary_project}/target/ancestor-link"
assert_rejected "symlinked ancestor" "${temporary_project}" \
    "${temporary_project}/target/ancestor-link/output" "output must be directly below"

mkdir -p -- "${temporary}/symlink-project" "${temporary}/symlink-target"
temporary_symlink_project="$(cd -- "${temporary}/symlink-project" && pwd -P)"
temporary_symlink_target="$(cd -- "${temporary}/symlink-target" && pwd -P)"
ln -s -- "${temporary_symlink_target}" "${temporary_symlink_project}/target"
assert_rejected "target-root symlink" "${temporary_symlink_project}" \
    "${temporary_symlink_project}/target/output" "target root is a symlink"

if [ "${failures}" -ne 0 ]; then
    echo "[FAILED] ${failures} ELF output-path guard case(s)" >&2
    exit 1
fi

echo "[PASS] all ELF output-path guard cases"
