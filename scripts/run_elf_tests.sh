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
if ! source "${SCRIPT_DIR}/riscv_elf_result.sh"; then
    echo -e "${RED}Error: cannot load result classification helper${NC}" >&2
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

# TLM U4 (dev-plan D10) and direct-memory U3 (direct-memory D14): every guest
# also runs through the blocking TLM RAM route and through that route with
# direct memory enabled, each with its declared profile. These parity guests
# write --log-commits on every pass, and every file must be byte-identical to
# the native one, so neither the TLM route nor a granted host pointer may
# change a retired instruction.
PARITY_GUESTS=(hello s5_timer s5_external atomic_tohost_exit fd_fma)
LOG_DIR="${OUTDIR}/commit-logs"
rm -rf "${LOG_DIR}"
mkdir -p "${LOG_DIR}"

is_parity_guest() {
    local candidate="$1"
    local guest
    for guest in "${PARITY_GUESTS[@]}"; do
        if [ "${candidate}" = "${guest}" ]; then
            return 0
        fi
    done
    return 1
}

# classify_pass NAME STATUS OUTPUT
#   Returns 0 when one public-CLI pass meets the guest's success contract.
#   Otherwise prints the reason and returns 1. Both transports use this, so a
#   TLM failure is classified exactly like a native one and never skipped.
classify_pass() {
    local name="$1"
    local status="$2"
    local output="$3"
    local reason
    # s5_wfi_none has no future platform event. D4 says that run stops with
    # NoProgress, timed_out clear, and a non-zero status. That is the pass.
    if [ "${name}" = "s5_wfi_none" ]; then
        if [ "${status}" -eq 0 ] \
            || ! printf '%s\n' "${output}" | grep -q 'Error:      NoProgress' \
            || printf '%s\n' "${output}" | grep -q 'TIMEOUT'; then
            echo "s5_wfi_none must stop with NoProgress and not time out"
            return 1
        fi
        return 0
    fi
    # Exit 0 alone is not success: the host truncates the status to 8 bits.
    if ! reason=$(riscv_classify_cli_run "${status}" "${output}"); then
        echo "${reason}"
        return 1
    fi
    if [ "${name}" = "hello" ] && ! printf '%s\n' "${output}" | grep -q 'Hello!'; then
        echo "hello output did not contain Hello!"
        return 1
    fi
    return 0
}

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

# Preserve the historical RV64I/RV64M case order and append the RV64A A8
# guests; existing case identities and numbers stay stable.
mapfile -t ELF_FILES < <(
    find "${OUTDIR}/rv64i" "${OUTDIR}/rv64m" -type f -name '*.elf' -print | sort
    find "${OUTDIR}/rv64a" -type f -name '*.elf' -print | sort
    find "${OUTDIR}/rv64ifd" -type f -name '*.elf' -print | sort
)
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

for required in lrsc_loop amo_w amo_d aq_rl_set sc_after_store_failure atomic_near_ram_end atomic_tohost_exit; do
    if [ ! -f "${OUTDIR}/rv64a/${required}.elf" ]; then
        echo -e "${RED}Error: required A8 atomic guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

for required in s6_ebreak s6_debug_csr; do
    if [ ! -f "${OUTDIR}/rv64i/${required}.elf" ]; then
        echo -e "${RED}Error: required Stage 6 guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

# Zca U4 compressed guests are required members of the same suite.
for required in zca_ctrl_flow zca_link_values zca_straddle_fetch zca_ebreak_trap zca_hints; do
    if [ ! -f "${OUTDIR}/rv64i/${required}.elf" ]; then
        echo -e "${RED}Error: required Zca guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

# Direct-memory U3 guests are required members of the same suite: the
# self-modifying-code guest and the page-table guest are the images the third
# pass must carry, and tests/direct_mem_u3_guests.rs proves the direct path
# with the same builds.
for required in direct_mem_smc_fence_i direct_mem_sv39_pt_store; do
    if [ ! -f "${OUTDIR}/rv64i/${required}.elf" ]; then
        echo -e "${RED}Error: required direct-memory guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

# F/D U5 guests and the U6 FP attach guest are required members of the same
# suite: the rv64imafd profile for the fd_* names, and rv64imafdc for
# zcd_transfer and fdc_u6_attach.
for required in fd_rounding fd_flags fd_nanbox fd_fma fd_convert fd_fs_state fd_zcd_absent \
    zcd_transfer fdc_u6_attach; do
    if [ ! -f "${OUTDIR}/rv64ifd/${required}.elf" ]; then
        echo -e "${RED}Error: required F/D guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

# Stage 4 guests and the Stage 5 U1 through U4 guests live under rv64i and
# are linked with linker-s4.ld. The suite fails if any of them is missing.
for required in \
    s4_ad_observation s4_counters s4_csr_policy s4_delegation s4_envcfg \
    s4_fp_absent s4_mapped_rw s4_mprv s4_permission_fault s4_pmp_csr \
    s4_privilege_transition s4_sfence_vma s4_sret_vectored \
    s4_supervisor_interrupt s4_tvm_tsr_tw s4_walk_fault \
    s5_counters s5_software s5_timer s5_external \
    s5_wfi_event s5_wfi_masked s5_wfi_none
do
    if [ ! -f "${OUTDIR}/rv64i/${required}.elf" ]; then
        echo -e "${RED}Error: required guest missing: ${required}.elf${NC}" >&2
        exit 2
    fi
done

# hello belongs to the TLM U4 parity set, so its absence is a suite failure
# rather than a silently smaller comparison.
if [ ! -f "${OUTDIR}/rv64i/hello.elf" ]; then
    echo -e "${RED}Error: required TLM parity guest missing: hello.elf${NC}" >&2
    exit 2
fi

echo "Found ${#ELF_FILES[@]} fresh ELF cases"

passed=0
failed=0
parity_checked=0
direct_checked=0
number=0
for elf in "${ELF_FILES[@]}"; do
    number=$((number + 1))
    name="$(basename "${elf}" .elf)"
    echo -e "${YELLOW}Test ${number}/${#ELF_FILES[@]}: ${name}.elf${NC}"
    echo "  File: ${elf}"
    # Every guest declares its profile (RV64GC contract D4): guests whose
    # expectations depend on the profile run under their explicit --isa,
    # profile-independent guests declare the default with no --isa. The
    # s4_* guests assert the rv64ima misa, FS=0 or IALIGN=32 rules, so the
    # whole s4_* set is bound to rv64ima explicitly; Zca guests select
    # rv64imac, F/D guests rv64imafd, and the Zcd and F/D/C attach guests
    # rv64imafdc.
    isa_args=( )
    case "${name}" in
        s4_*) isa_args=(--isa rv64ima) ;;
        zca_*) isa_args=(--isa rv64imac) ;;
        fd_*) isa_args=(--isa rv64imafd) ;;
        zcd_*|fdc_*) isa_args=(--isa rv64imafdc) ;;
        # Profile-independent guests run under the default with no --isa.
        *) isa_args=( ) ;;
    esac
    if ! check_entry "${elf}"; then
        failed=$((failed + 1))
        continue
    fi

    # The parity set captures commit logs from all three passes; every other
    # guest still runs three times but needs no log files.
    native_log=""
    tlm_log=""
    direct_log=""
    native_log_args=( )
    tlm_log_args=( )
    direct_log_args=( )
    if is_parity_guest "${name}"; then
        native_log="${LOG_DIR}/${name}.native.log"
        tlm_log="${LOG_DIR}/${name}.tlm.log"
        direct_log="${LOG_DIR}/${name}.direct.log"
        native_log_args=(--log-commits "${native_log}")
        tlm_log_args=(--log-commits "${tlm_log}")
        direct_log_args=(--log-commits "${direct_log}")
    fi

    output=$("${RUSCV_BIN}" run "${elf}" --max-cycles "${MAX_CYCLES}" "${isa_args[@]}" "${native_log_args[@]}" 2>&1)
    status=$?
    printf '%s\n' "${output}"
    if ! reason=$(classify_pass "${name}" "${status}" "${output}"); then
        echo -e "${RED}  [FAIL] ${reason}${NC}"
        failed=$((failed + 1))
        continue
    fi
    if [ "${name}" = "s5_wfi_none" ]; then
        echo -e "${GREEN}  [PASS]${NC} NoProgress, timed_out clear"
    else
        echo -e "${GREEN}  [PASS]${NC} CLI exit status 0, ${RISCV_CLI_SUCCESS_LINE}"
    fi

    # D10: the same guest, profile and cycle bound through the blocking TLM
    # RAM route. A failure here is a real defect, never a skip.
    tlm_output=$("${RUSCV_BIN}" run "${elf}" --max-cycles "${MAX_CYCLES}" "${isa_args[@]}" --transport tlm "${tlm_log_args[@]}" 2>&1)
    tlm_status=$?
    if ! reason=$(classify_pass "${name}" "${tlm_status}" "${tlm_output}"); then
        printf '%s\n' "${tlm_output}"
        echo -e "${RED}  [FAIL] --transport tlm: ${reason}${NC}"
        failed=$((failed + 1))
        continue
    fi

    if [ -n "${native_log}" ]; then
        if [ ! -s "${native_log}" ] || [ ! -s "${tlm_log}" ]; then
            echo -e "${RED}  [FAIL] commit log missing or empty for ${name}${NC}"
            failed=$((failed + 1))
            continue
        fi
        if ! cmp -s "${native_log}" "${tlm_log}"; then
            echo -e "${RED}  [FAIL] ${name}: --transport tlm commit log differs from native${NC}"
            diff -u "${native_log}" "${tlm_log}" | head -20 || true
            failed=$((failed + 1))
            continue
        fi
        parity_checked=$((parity_checked + 1))
        echo -e "${GREEN}  [PASS]${NC} --transport tlm, commit log byte-identical to native"
    else
        echo -e "${GREEN}  [PASS]${NC} --transport tlm"
    fi

    # direct-memory U3 (direct-memory D14): the same guest, profile and cycle
    # bound a third time with the direct-memory selection. A failure here is a
    # real defect, never a skip.
    direct_output=$("${RUSCV_BIN}" run "${elf}" --max-cycles "${MAX_CYCLES}" "${isa_args[@]}" --transport tlm --dmi "${direct_log_args[@]}" 2>&1)
    direct_status=$?
    if ! reason=$(classify_pass "${name}" "${direct_status}" "${direct_output}"); then
        printf '%s\n' "${direct_output}"
        echo -e "${RED}  [FAIL] --transport tlm --dmi: ${reason}${NC}"
        failed=$((failed + 1))
        continue
    fi

    if [ -n "${native_log}" ]; then
        if [ ! -s "${direct_log}" ]; then
            echo -e "${RED}  [FAIL] --transport tlm --dmi commit log missing or empty for ${name}${NC}"
            failed=$((failed + 1))
            continue
        fi
        if ! cmp -s "${native_log}" "${direct_log}"; then
            echo -e "${RED}  [FAIL] ${name}: --transport tlm --dmi commit log differs from native${NC}"
            diff -u "${native_log}" "${direct_log}" | head -20 || true
            failed=$((failed + 1))
            continue
        fi
        direct_checked=$((direct_checked + 1))
        echo -e "${GREEN}  [PASS]${NC} --transport tlm --dmi, commit log byte-identical to native"
    else
        echo -e "${GREEN}  [PASS]${NC} --transport tlm --dmi"
    fi
    passed=$((passed + 1))
done

printf '\n============================================\n'
printf '  Fresh RISC-V ELF execution summary\n'
printf '============================================\n'
printf '  Total:  %s\n' "$((passed + failed))"
printf '  Passed: %s\n' "${passed}"
printf '  Failed: %s\n' "${failed}"
printf '  Cases:  %s\n' "${#ELF_FILES[@]}"
printf '  TLM parity logs compared: %s/%s\n' "${parity_checked}" "${#PARITY_GUESTS[@]}"
printf '  Direct-memory parity logs compared: %s/%s\n' "${direct_checked}" "${#PARITY_GUESTS[@]}"
printf '\n'

if [ "${failed}" -ne 0 ]; then
    echo -e "${RED}[FAILED]${NC} one or more public CLI ELF cases failed"
    exit 1
fi

# A parity guest that never reached the comparisons above must fail the suite
# instead of silently shrinking the byte-identical set.
if [ "${parity_checked}" -ne "${#PARITY_GUESTS[@]}" ]; then
    echo -e "${RED}[FAILED]${NC} TLM parity comparison covered ${parity_checked} of ${#PARITY_GUESTS[@]} guests"
    exit 1
fi
if [ "${direct_checked}" -ne "${#PARITY_GUESTS[@]}" ]; then
    echo -e "${RED}[FAILED]${NC} direct-memory parity comparison covered ${direct_checked} of ${#PARITY_GUESTS[@]} guests"
    exit 1
fi

echo -e "${GREEN}[PASS]${NC} all ${passed} fresh ELF cases passed through the public CLI on all three passes"
exit 0
