#!/bin/bash
# Assemble and link the project-authored RISC-V ELF suite.
#
# The output directory is deliberately outside the source tree.  Every
# invocation removes that directory before assembling, so an ELF from an older
# checkout cannot be mistaken for evidence for the current sources.

set -u

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TESTS_DIR="${PROJECT_DIR}/tests/bare-metal-riscv-test"
OUTDIR="${RISCV_TEST_OUTDIR:-${PROJECT_DIR}/target/riscv-elf-tests}"

RISCV_PREFIX="${RISCV_PREFIX:-riscv64-unknown-elf-}"
AS="${RISCV_PREFIX}as"
LD="${RISCV_PREFIX}ld"

# Refuse an accidental source-tree or filesystem-root cleanup.  The default is
# a disposable target subdirectory and callers may select another disposable
# directory with RISCV_TEST_OUTDIR.
case "${OUTDIR}" in
    ""|"/"|"${PROJECT_DIR}"|"${TESTS_DIR}")
        echo -e "${RED}Error: unsafe RISCV_TEST_OUTDIR: ${OUTDIR}${NC}" >&2
        exit 2
        ;;
esac

check_toolchain() {
    if ! command -v "${AS}" >/dev/null 2>&1 || ! command -v "${LD}" >/dev/null 2>&1; then
        echo -e "${YELLOW}[SKIP] RISC-V assembler/linker unavailable${NC}"
        echo "  expected assembler: ${AS}"
        echo "  expected linker:    ${LD}"
        echo "  install binutils/gcc or set RISCV_PREFIX; exit status 77 means unavailable"
        return 77
    fi
    echo -e "${GREEN}[OK]${NC} RISC-V toolchain found"
    echo "  Assembler: $(${AS} --version | head -1)"
    echo "  Linker:    $(${LD} --version | head -1)"
    return 0
}

check_toolchain
check_toolchain_status=$?
if [ "${check_toolchain_status}" -ne 0 ]; then
    exit "${check_toolchain_status}"
fi

if [ ! -f "${TESTS_DIR}/linker.ld" ]; then
    echo -e "${RED}Error: linker script not found: ${TESTS_DIR}/linker.ld${NC}" >&2
    exit 2
fi

# Fresh means fresh: do not reuse a previous source/HEAD's generated ELF.
rm -rf "${OUTDIR}"
mkdir -p "${OUTDIR}"

mapfile -t SOURCES < <(find "${TESTS_DIR}/rv64i" "${TESTS_DIR}/rv64m" -type f -name '*.S' -print | sort)
if [ "${#SOURCES[@]}" -eq 0 ]; then
    echo -e "${RED}Error: no RISC-V assembly sources found${NC}" >&2
    exit 2
fi

compiled=0
failed=0
for source in "${SOURCES[@]}"; do
    relative="${source#"${TESTS_DIR}/"}"
    name="${relative%.S}"
    object="${OUTDIR}/${name}.o"
    elf="${OUTDIR}/${name}.elf"
    mkdir -p "$(dirname "${object}")"

    echo -e "${BLUE}Compiling ${relative}${NC}"
    if ! "${AS}" -march=rv64ima_zicsr -mabi=lp64 "${source}" -o "${object}"; then
        echo -e "${RED}  [FAIL] assembly: ${relative}${NC}" >&2
        failed=$((failed + 1))
        continue
    fi
    if ! "${LD}" -T "${TESTS_DIR}/linker.ld" "${object}" -o "${elf}"; then
        echo -e "${RED}  [FAIL] link: ${relative}${NC}" >&2
        failed=$((failed + 1))
        rm -f "${object}"
        continue
    fi
    rm -f "${object}"
    echo -e "${GREEN}  [OK]${NC} ${elf}"
    compiled=$((compiled + 1))
done

source_head="${RISCV_SOURCE_HEAD:-unknown}"
if [ "${source_head}" = "unknown" ] && source_head_value=$(git -C "${PROJECT_DIR}" rev-parse HEAD 2>/dev/null); then
    source_head="${source_head_value}"
fi
manifest="${OUTDIR}/manifest.txt"
{
    printf 'source_head=%s\n' "${source_head}"
    printf 'source_dir=%s\n' "${TESTS_DIR}"
    printf 'output_dir=%s\n' "${OUTDIR}"
    printf 'toolchain_prefix=%s\n' "${RISCV_PREFIX}"
    printf 'assembler=%s\n' "${AS}"
    printf 'linker=%s\n' "${LD}"
    printf 'source_count=%s\n' "${#SOURCES[@]}"
    printf 'compiled_count=%s\n' "${compiled}"
    printf 'failed_count=%s\n' "${failed}"
} >"${manifest}"

printf '\n============================================\n'
printf '  Fresh RISC-V ELF compilation summary\n'
printf '============================================\n'
printf '  Source HEAD: %s\n' "${source_head}"
printf '  Sources:     %s\n' "${#SOURCES[@]}"
printf '  Compiled:    %s\n' "${compiled}"
printf '  Failed:      %s\n' "${failed}"
printf '  Output:      %s\n' "${OUTDIR}"
printf '\n'

if [ "${failed}" -ne 0 ] || [ "${compiled}" -ne "${#SOURCES[@]}" ]; then
    echo -e "${RED}[FAILED]${NC} guest compilation did not produce every ELF"
    exit 1
fi

echo -e "${GREEN}[PASS]${NC} fresh assembly/link completed for ${compiled} guest cases"
exit 0
