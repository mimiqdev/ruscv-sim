#!/bin/bash
# Run RISC-V ELF tests using ruscv-sim simulator
# Auto-discovers all .elf files in tests/bare-metal-riscv-test/rv64i/



# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TESTS_DIR="${PROJECT_DIR}/tests/bare-metal-riscv-test"

# Find ruscv-sim binary
find_ruscv_sim() {
    # Check release build first, then debug
    if [ -f "${PROJECT_DIR}/target/release/ruscv-sim" ]; then
        echo "${PROJECT_DIR}/target/release/ruscv-sim"
    elif [ -f "${PROJECT_DIR}/target/debug/ruscv-sim" ]; then
        echo "${PROJECT_DIR}/target/debug/ruscv-sim"
    else
        echo ""
    fi
}

RUSCV_BIN="$(find_ruscv_sim)"

# Function to check if ELF file exists
check_elf() {
    local elf="$1"
    if [ ! -f "${elf}" ]; then
        echo -e "${RED}Error: ELF file not found: ${elf}${NC}"
        echo "Run compile_riscv_tests.sh first to build the test programs."
        return 1
    fi
    return 0
}

# Function to check if ruscv-sim is available
check_simulator() {
    if [ -z "${RUSCV_BIN}" ] || [ ! -f "${RUSCV_BIN}" ]; then
        echo -e "${BLUE}Building ruscv-sim...${NC}"
        cd "${PROJECT_DIR}"
        if ! cargo build --release; then
            echo -e "${RED}Error: cargo build --release failed${NC}"
            return 1
        fi
        RUSCV_BIN="$(find_ruscv_sim)"
    fi
    
    if [ -z "${RUSCV_BIN}" ] || [ ! -f "${RUSCV_BIN}" ]; then
        echo -e "${RED}Error: ruscv-sim binary not found${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}[OK]${NC} Simulator: ${RUSCV_BIN}"
}

# Function to run a single test
run_test() {
    local name="$1"
    local elf="$2"
    local expected_exit="$3"
    local description="$4"
    
    echo -e "${BLUE}Running test: ${name}${NC}"
    echo "  File: ${elf}"
    echo "  Expected exit code: ${expected_exit}"
    echo "  Description: ${description}"
    echo ""
    
    # Run the simulator
    local output
    local exit_status=0
    
    # Run simulator and capture output
    output=$("${RUSCV_BIN}" run "${elf}" --max-cycles 100000 2>&1) || exit_status=$?
    
    echo "$output"
    echo ""
    
    if [ $exit_status -eq "$expected_exit" ]; then
        echo -e "${GREEN}  [PASS]${NC} Exit code = ${exit_status} (expected ${expected_exit})"
        return 0
    else
        echo -e "${RED}  [FAIL]${NC} Exit code = ${exit_status} (expected ${expected_exit})"
        return 1
    fi
}

# Function to run hello test with output verification
run_hello_test() {
    local name="$1"
    local elf="$2"
    local expected_exit="$3"
    local description="$4"
    
    echo -e "${BLUE}Running test: ${name}${NC}"
    echo "  File: ${elf}"
    echo "  Expected exit code: ${expected_exit}"
    echo "  Description: ${description}"
    echo ""
    
    # Run the simulator
    local output
    local exit_status=0
    
    # Run simulator and capture output
    output=$("${RUSCV_BIN}" run "${elf}" --max-cycles 100000 2>&1) || exit_status=$?
    
    echo "$output"
    echo ""
    
    # Check exit code
    if [ $exit_status -ne "$expected_exit" ]; then
        echo -e "${RED}  [FAIL]${NC} Exit code = ${exit_status} (expected ${expected_exit})"
        return 1
    fi
    
    # Check output contains "Hello!"
    if echo "$output" | grep -q "Hello!"; then
        echo -e "${GREEN}  [PASS]${NC} Exit code = ${exit_status}, output contains 'Hello!'"
        return 0
    else
        echo -e "${RED}  [FAIL]${NC} Output does not contain 'Hello!'"
        return 1
    fi
}

# Main
echo "============================================"
echo "  RISC-V ELF Test Runner"
echo "============================================"
echo ""

# Check simulator
echo -e "${YELLOW}Checking simulator...${NC}"
check_simulator
echo ""

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0
TEST_NUMBER=0

# Find all .elf files in rv64i directory
echo -e "${YELLOW}Discovering tests...${NC}"
ELF_FILES=("${TESTS_DIR}"/rv64i/*.elf)

if [ ${#ELF_FILES[@]} -eq 0 ] || [ ! -f "${ELF_FILES[0]}" ]; then
    echo -e "${RED}Error: No .elf files found in ${TESTS_DIR}/rv64i/${NC}"
    echo "Run compile_riscv_tests.sh first to build the test programs."
    exit 1
fi

echo "Found ${#ELF_FILES[@]} test(s)"
echo ""

# Run each test
for elf_file in "${ELF_FILES[@]}"; do
    # Skip if not a file (handles case when glob doesn't match)
    [ -f "$elf_file" ] || continue
    
    # Extract test name from filename
    test_name=$(basename "$elf_file" .elf)
    TEST_NUMBER=$((TEST_NUMBER + 1))
    
    echo -e "${YELLOW}Test ${TEST_NUMBER}: ${test_name}.elf${NC}"
    echo "--------------------------------------------"
    
    if check_elf "$elf_file"; then
        # Use special validation for hello test
        if [ "$test_name" = "hello" ]; then
            if run_hello_test "$test_name" "$elf_file" "0" "UART output test"; then
                TESTS_PASSED=$((TESTS_PASSED + 1))
            else
                TESTS_FAILED=$((TESTS_FAILED + 1))
            fi
        else
            if run_test "$test_name" "$elf_file" "0" "RV64I instruction test"; then
                TESTS_PASSED=$((TESTS_PASSED + 1))
            else
                TESTS_FAILED=$((TESTS_FAILED + 1))
            fi
        fi
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
    echo ""
done

# Summary
echo "============================================"
echo "  Test Summary"
echo "============================================"
echo -e "  Total:  ${YELLOW}$((TESTS_PASSED + TESTS_FAILED))${NC}"
echo -e "  Passed: ${GREEN}${TESTS_PASSED}${NC}"
echo -e "  Failed: ${RED}${TESTS_FAILED}${NC}"
echo ""

if [ ${TESTS_FAILED} -eq 0 ]; then
    echo -e "${GREEN}[SUCCESS]${NC} All tests passed!"
    exit 0
else
    echo -e "${RED}[FAILED]${NC} Some tests failed."
    exit 1
fi
