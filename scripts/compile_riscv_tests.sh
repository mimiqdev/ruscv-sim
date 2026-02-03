#!/bin/bash
# Compile RISC-V ELF test programs
# Builds add.elf, fib.elf, hello.elf from assembly source files

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_DIR="${SCRIPT_DIR}/../tests/riscv-tests"
OUTDIR="${TESTS_DIR}"

# RISC-V toolchain prefix
RISCV_PREFIX="${RISCV_PREFIX:-riscv64-unknown-elf-}"
AS="${RISCV_PREFIX}as"
LD="${RISCV_PREFIX}ld"

# Assembly and linking flags
ASFLAGS="-march=rv64ima -mabi=lp64"
LDSCRIPT="-T${TESTS_DIR}/linker.ld"

# Source files
SOURCES="add.S fib.S hello.S"

# Function to check if toolchain exists
check_toolchain() {
    if ! command -v "${AS}" &> /dev/null; then
        echo -e "${RED}Error: RISC-V toolchain not found${NC}"
        echo "Expected: ${AS}"
        echo ""
        echo "Please install the RISC-V toolchain:"
        echo "  Ubuntu/Debian: sudo apt-get install gcc-riscv64-unknown-elf"
        echo "  Or download from: https://github.com/riscv-collab/riscv-gnu-toolchain"
        echo ""
        echo "You can also set RISCV_PREFIX to use a different prefix:"
        echo "  export RISCV_PREFIX=riscv64-linux-gnu-"
        exit 1
    fi
    
    if ! command -v "${LD}" &> /dev/null; then
        echo -e "${RED}Error: RISC-V linker not found${NC}"
        echo "Expected: ${LD}"
        exit 1
    fi
    
    echo -e "${GREEN}[OK]${NC} RISC-V toolchain found"
    echo "  Assembler: $(${AS} --version | head -1)"
    echo "  Linker: $(${LD} --version | head -1)"
}

# Function to compile a single test
compile_test() {
    local src="$1"
    local name="${src%.S}"
    local obj="${OUTDIR}/${name}.o"
    local elf="${OUTDIR}/${name}.elf"
    
    echo -e "${BLUE}Compiling ${src}...${NC}"
    
    # Assemble
    echo "  AS  ${src} -> ${name}.o"
    if ! "${AS}" ${ASFLAGS} "${TESTS_DIR}/${src}" -o "${obj}"; then
        echo -e "${RED}  [FAIL]${NC} Assembly failed for ${src}"
        return 1
    fi
    
    # Link
    echo "  LD  ${name}.o -> ${name}.elf"
    if ! "${LD}" ${LDSCRIPT} "${obj}" -o "${elf}"; then
        echo -e "${RED}  [FAIL]${NC} Link failed for ${src}"
        rm -f "${obj}"
        return 1
    fi
    
    # Clean up object file
    rm -f "${obj}"
    
    echo -e "${GREEN}  [OK]${NC} Created ${name}.elf"
    return 0
}

# Main
echo "============================================"
echo "  Compiling RISC-V Test Programs"
echo "============================================"
echo ""

# Check toolchain
echo -e "${YELLOW}Checking RISC-V toolchain...${NC}"
check_toolchain
echo ""

# Check linker script exists
if [ ! -f "${TESTS_DIR}/linker.ld" ]; then
    echo -e "${RED}Error: Linker script not found: ${TESTS_DIR}/linker.ld${NC}"
    exit 1
fi

# Compile each test
echo -e "${YELLOW}Compiling test programs...${NC}"
TESTS_COMPILED=0
TESTS_FAILED=0

for src in ${SOURCES}; do
    if [ -f "${TESTS_DIR}/${src}" ]; then
        if compile_test "${src}"; then
            TESTS_COMPILED=$((TESTS_COMPILED + 1))
        else
            echo -e "${RED}  [FAIL] Failed to compile ${src}${NC}"
            TESTS_FAILED=$((TESTS_FAILED + 1))
        fi
    else
        echo -e "${YELLOW}[WARN] Source file not found: ${src}${NC}"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
    echo ""
done

# Summary
echo "============================================"
echo "  Compilation Summary"
echo "============================================"
echo -e "  Compiled: ${GREEN}${TESTS_COMPILED}${NC}"
echo -e "  Failed:   ${RED}${TESTS_FAILED}${NC}"
echo ""

if [ ${TESTS_FAILED} -eq 0 ]; then
    echo -e "${GREEN}[SUCCESS]${NC} All tests compiled successfully!"
    echo ""
    echo "Generated ELF files:"
    for src in ${SOURCES}; do
        name="${src%.S}"
        elf="${OUTDIR}/${name}.elf"
        if [ -f "${elf}" ]; then
            echo "  - ${elf}"
        fi
    done
    exit 0
else
    echo -e "${RED}[FAILED]${NC} Some tests failed to compile."
    exit 1
fi
