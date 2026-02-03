#!/bin/bash
# Test script for RISC-V ELF loader and executor
# Tests the ruscv-sim ELF loading and execution pipeline

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_DIR="${SCRIPT_DIR}/riscv-tests"
RUSCV_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Build the ruscv-sim if needed
echo -e "${BLUE}Building ruscv-sim...${NC}"
cd "${RUSCV_DIR}"
cargo build --release 2>/dev/null || cargo build

RUSCV_BIN="${RUSCV_DIR}/target/debug/ruscv-sim"

# Check if ELF files exist
check_elf() {
    local elf="${TESTS_DIR}/$1"
    if [ ! -f "$elf" ]; then
        echo -e "${YELLOW}Warning: $elf not found. Run 'make' in tests/riscv-tests/ first.${NC}"
        return 1
    fi
    return 0
}

# Run a test and check result
run_test() {
    local name="$1"
    local elf="$2"
    local expected_exit="$3"
    local description="$4"

    echo -e "${BLUE}Running test: $name${NC}"
    echo "  File: $elf"
    echo "  Expected exit code: $expected_exit"
    echo "  Description: $description"
    echo ""

    # Run the simulator
    local output
    if output=$("${RUSCV_BIN}" run "$elf" --max-cycles 100000 2>&1); then
        echo "$output"
        
        # Check exit code (look for "exit_code" in output)
        local exit_code
        exit_code=$(echo "$output" | grep -oP 'exit_code: \K\d+' | head -1)
        
        if [ -z "$exit_code" ]; then
            echo -e "${YELLOW}  Warning: Could not parse exit code${NC}"
            return 1
        fi
        
        if [ "$exit_code" = "$expected_exit" ]; then
            echo -e "${GREEN}  PASS: Exit code = $exit_code (expected $expected_exit)${NC}"
            return 0
        else
            echo -e "${RED}  FAIL: Exit code = $exit_code (expected $expected_exit)${NC}"
            return 1
        fi
    else
        echo -e "${RED}  ERROR: Simulation failed${NC}"
        echo "$output"
        return 1
    fi
}

# Summary
TESTS_PASSED=0
TESTS_FAILED=0

echo ""
echo "============================================"
echo "  RISC-V ELF Loader and Executor Tests"
echo "============================================"
echo ""

# Test 1: add.elf (1+2+...+10 = 55)
echo -e "${YELLOW}Test 1: add.elf (Sum 1 to 10)${NC}"
echo "----------------------------------------"
if check_elf "add.elf"; then
    if run_test "add" "${TESTS_DIR}/add.elf" "0" "Calculate sum 1+2+...+10 = 55"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi
else
    ((TESTS_FAILED++))
fi
echo ""

# Test 2: fib.elf (Fibonacci F10 = 55)
echo -e "${YELLOW}Test 2: fib.elf (Fibonacci Sequence)${NC}"
echo "----------------------------------------"
if check_elf "fib.elf"; then
    if run_test "fib" "${TESTS_DIR}/fib.elf" "0" "Calculate Fibonacci F10 = 55, expect pass (exit 0)"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi
else
    ((TESTS_FAILED++))
fi
echo ""

# Test 3: hello.elf (UART output)
echo -e "${YELLOW}Test 3: hello.elf (UART Hello World)${NC}"
echo "----------------------------------------"
if check_elf "hello.elf"; then
    if run_test "hello" "${TESTS_DIR}/hello.elf" "0" "Output 'Hello!' via UART, exit 0"; then
        ((TESTS_PASSED++))
    else
        ((TESTS_FAILED++))
    fi
else
    ((TESTS_FAILED++))
fi
echo ""

# Summary
echo "============================================"
echo "  Test Summary"
echo "============================================"
echo -e "  Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "  Failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
