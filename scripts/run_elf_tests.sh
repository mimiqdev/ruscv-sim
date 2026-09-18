#!/bin/bash
# Freshly build and run every project-authored RISC-V ELF through the public CLI.
#
# By default this script invokes compile_riscv_tests.sh first.  Set
# RISCV_TEST_SKIP_BUILD=1 only when a separately recorded fresh manifest is
# intentionally being consumed.  Missing cross-tools are reported as SKIP with
# exit status 77, never as a passing zero.

set -u

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd -P)"
TESTS_DIR="${PROJECT_DIR}/tests/bare-metal-riscv-test"

if ! source "${SCRIPT_DIR}/riscv_elf_paths.sh"; then
    echo -e "${RED}Error: cannot load path validation helper${NC}" >&2
    exit 2
fi
OUTDIR_REQUESTED="${RISCV_TEST_OUTDIR:-target/riscv-elf-tests}"
if ! OUTDIR="$(riscv_validate_output_dir "${PROJECT_DIR}" "${OUTDIR_REQUESTED}")"; then
    exit 2
fi

RISCV_PREFIX="${RISCV_PREFIX:-riscv64-unknown-elf-}"
MAX_CYCLES="${RISCV_TEST_MAX_CYCLES:-100000}"

if [ "${RISCV_TEST_SKIP_BUILD:-0}" != "1" ]; then
    "${SCRIPT_DIR}/compile_riscv_tests.sh"
    compile_status=$?
    if [ "${compile_status}" -ne 0 ]; then
        # 77 is an explicit unavailable-toolchain result; all other failures
        # are real build failures.
        exit "${compile_status}"
    fi
fi

manifest="${OUTDIR}/manifest.txt"
if [ ! -f "${manifest}" ]; then
    echo -e "${RED}Error: no fresh guest manifest at ${manifest}${NC}" >&2
    echo "Run without RISCV_TEST_SKIP_BUILD=1 to assemble/link first." >&2
    exit 2
fi

# Use the cross-binutils tools to confirm that the actual ELF entry is the
# linker-selected _start symbol.  This prevents a source/ELF mismatch from
# being hidden by a successful process exit.
READELF="${RISCV_PREFIX}readelf"
NM="${RISCV_PREFIX}nm"
if ! command -v "${READELF}" >/dev/null 2>&1 || ! command -v "${NM}" >/dev/null 2>&1; then
    echo -e "${RED}Error: entry-point verification tools unavailable (${READELF}, ${NM})${NC}" >&2
    exit 2
fi

normalize_hex() {
    local value="$1"
    value="${value#0x}"
    value="${value#0X}"
    value="${value#0x}"
    printf '%s' "${value}" | tr '[:upper:]' '[:lower:]' | sed 's/^0*//' | sed 's/^$/0/'
}

check_entry() {
    local elf="$1"
    local entry start
    entry=$("${READELF}" -h "${elf}" | awk '/Entry point address:/ {print $NF; exit}')
    start=$("${NM}" -n "${elf}" | awk '$3 == "_start" {print $1; exit}')
    if [ -z "${entry}" ] || [ -z "${start}" ]; then
        echo -e "${RED}  [FAIL] cannot determine entry/_start for ${elf}${NC}" >&2
        return 1
    fi
    if [ "$(normalize_hex "${entry}")" != "$(normalize_hex "${start}")" ]; then
        echo -e "${RED}  [FAIL] entry ${entry} != _start ${start} for ${elf}${NC}" >&2
        return 1
    fi
    echo "  Entry: ${entry} (_start ${start})"
    return 0
}

# An explicit RUSCV_SIM_BIN is a caller-owned override: the caller is
# responsible for its freshness.  The default path always invokes Cargo so a
# repeated verification run cannot silently reuse an obsolete simulator.
if [ -n "${RUSCV_SIM_BIN:-}" ]; then
    RUSCV_BIN="${RUSCV_SIM_BIN}"
else
    CARGO_TARGET_VALUE="${CARGO_TARGET_DIR:-${PROJECT_DIR}/target/a6-elf-cargo}"
    case "${CARGO_TARGET_VALUE}" in
        /*) CARGO_TARGET_ABS="${CARGO_TARGET_VALUE}" ;;
        *) CARGO_TARGET_ABS="${PROJECT_DIR}/${CARGO_TARGET_VALUE}" ;;
    esac
    RUSCV_BIN="${CARGO_TARGET_ABS}/release/ruscv-sim"
    echo -e "${BLUE}Building ruscv-sim in ${CARGO_TARGET_ABS}${NC}"
    if ! (cd "${PROJECT_DIR}" && \
        CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
        cargo build --release --all-features --target-dir "${CARGO_TARGET_ABS}"); then
        echo -e "${RED}Error: cargo build --release failed${NC}" >&2
        exit 1
    fi
fi

if [ ! -x "${RUSCV_BIN}" ]; then
    echo -e "${RED}Error: simulator binary not found: ${RUSCV_BIN}${NC}" >&2
    exit 2
fi

echo -e "${GREEN}[OK]${NC} Simulator: ${RUSCV_BIN}"
echo -e "${GREEN}[OK]${NC} Guest manifest: ${manifest}"
grep -E '^(source_head|source_count|compiled_count)=' "${manifest}" || true

mapfile -t ELF_FILES < <(find "${OUTDIR}" -type f -name '*.elf' -print | sort)
if [ "${#ELF_FILES[@]}" -eq 0 ]; then
    echo -e "${RED}Error: no ELF files found in ${OUTDIR}${NC}" >&2
    exit 2
fi

# Task 4's five guests are required members of the same suite.
for required in trap_ecall trap_illegal trap_ebreak trap_vectored trap_mret_priv; do
    if [ ! -f "${OUTDIR}/rv64i/${required}.elf" ]; then
        echo -e "${RED}Error: required A6 guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

echo "Found ${#ELF_FILES[@]} fresh ELF cases"

passed=0
failed=0
number=0
for elf in "${ELF_FILES[@]}"; do
    number=$((number + 1))
    name="$(basename "${elf}" .elf)"
    echo -e "${YELLOW}Test ${number}/${#ELF_FILES[@]}: ${name}.elf${NC}"
    echo "  File: ${elf}"
    if ! check_entry "${elf}"; then
        failed=$((failed + 1))
        continue
    fi

    output=$("${RUSCV_BIN}" run "${elf}" --max-cycles "${MAX_CYCLES}" 2>&1)
    status=$?
    printf '%s\n' "${output}"
    if [ "${status}" -ne 0 ]; then
        echo -e "${RED}  [FAIL] CLI exit status ${status}, expected 0${NC}"
        failed=$((failed + 1))
        continue
    fi
    if [ "${name}" = "hello" ] && ! printf '%s\n' "${output}" | grep -q 'Hello!'; then
        echo -e "${RED}  [FAIL] hello output did not contain Hello!${NC}"
        failed=$((failed + 1))
        continue
    fi
    echo -e "${GREEN}  [PASS]${NC} CLI exit status 0"
    passed=$((passed + 1))
done

printf '\n============================================\n'
printf '  Fresh RISC-V ELF execution summary\n'
printf '============================================\n'
printf '  Total:  %s\n' "$((passed + failed))"
printf '  Passed: %s\n' "${passed}"
printf '  Failed: %s\n' "${failed}"
printf '  Cases:  %s\n' "${#ELF_FILES[@]}"
printf '\n'

if [ "${failed}" -ne 0 ]; then
    echo -e "${RED}[FAILED]${NC} one or more public CLI ELF cases failed"
    exit 1
fi

echo -e "${GREEN}[PASS]${NC} all ${passed} fresh ELF cases passed through the public CLI"
exit 0
