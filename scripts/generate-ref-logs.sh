#!/bin/bash
#
# generate-ref-logs.sh - Generate reference logs for ELF tests
#
# Usage:
#   ./generate-ref-logs.sh              # Generate for default tests
#   ./generate-ref-logs.sh add.elf      # Generate for specific ELF
#   ./generate-ref-logs.sh add.elf and.elf  # Generate for multiple ELFs
#
# This script:
#   1. Runs Spike with pk (proxy kernel) to get HTIF support
#   2. Truncates logs at tohost write to avoid infinite loop bug
#   3. Outputs .log.ref files suitable for comparison
#
# Generated files are suitable for CI tests using scripts/run-ref-comparison.sh

set -e  # Exit on error

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TESTS_DIR="${PROJECT_DIR}/tests/bare-metal-riscv-test/rv64i"
REFS_DIR="${PROJECT_DIR}/tests/reference-logs"

# Paths
SPIKE_BIN="${SPIKE_BIN:-spike}"
PK_BIN="${PK_BIN:-pk}"

# Colors
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] [ELF_FILES]

Generate reference logs for ELF tests using Spike with pk (proxy kernel).

Arguments:
  ELF_FILES          One or more ELF files (default: use DEFAULT_TESTS)

Options:
  -o, --output-dir   Output directory for .log.ref files (default: $REFS_DIR)
  -v, --verbose      Enable verbose output
  -h, --help         Show this help message

Default Tests:
  add.elf, addi.elf, and.elf, or.elf, xor.elf, fib.elf

Environment Variables:
  SPIKE              Path to Spike binary (default: spike)
  PK                 Path to pk binary (default: pk)

Output:
  Creates .log.ref files that can be compared with ruscv-sim logs
  using scripts/log-compare.py or scripts/run-ref-comparison.sh

Examples:
  $(basename "$0")                          # Generate for default tests
  $(basename "$0") -v add.elf               # Verbose output for add.elf
  $(basename "$0") -o ./my-refs fib.elf     # Custom output directory

NOTE:
  This script uses Spike with pk (proxy kernel) for HTIF support.
  Logs are truncated at tohost write to avoid pk's infinite loop bug
  in Spike. The tohost write occurs when the test program signals
  completion via writing to the HTIF tohost register.

EOF
}

# Default tests
DEFAULT_TESTS=("add.elf" "addi.elf" "and.elf" "or.elf" "xor.elf" "fib.elf")

# Default start address (typical ELF entry point for RISC-V bare metal)
OUTPUT_DIR="$REFS_DIR"
VERBOSE=""
ELF_FILES=()

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -o|--output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE="--verbose"
            shift
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        -*)
            print_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
        *)
            ELF_FILES+=("$1")
            shift
            ;;
    esac
done

# Use default tests if none specified
if [ ${#ELF_FILES[@]} -eq 0 ]; then
    print_info "Using default tests: ${DEFAULT_TESTS[*]}"
    ELF_FILES=("${DEFAULT_TESTS[@]}")
fi

# Check Spike
if ! command -v "$SPIKE_BIN" &> /dev/null; then
    print_error "Spike not found: $SPIKE_BIN"
    print_info "Install Spike or set SPIKE environment variable"
    exit 1
fi

# Check pk
if ! command -v "$PK_BIN" &> /dev/null && ! [ -f "$PK_BIN" ]; then
    print_error "pk not found: $PK_BIN"
    print_info "Install riscv-pk or set PK environment variable"
    exit 1
fi

print_info "Spike binary: $SPIKE_BIN"
print_info "pk binary: $PK_BIN"

# Create output directory
mkdir -p "$OUTPUT_DIR"
print_info "Output directory: $OUTPUT_DIR"
echo ""

# Process each ELF file
SUCCESS_COUNT=0
FAILED_COUNT=0

for elf_name in "${ELF_FILES[@]}"; do
    # Find the ELF file (check tests dir first, then current dir)
    ELF_PATH=""
    if [ -f "$elf_name" ]; then
        ELF_PATH="$elf_name"
    elif [ -f "$TESTS_DIR/$elf_name" ]; then
        ELF_PATH="$TESTS_DIR/$elf_name"
    elif [ -f "$PROJECT_DIR/$elf_name" ]; then
        ELF_PATH="$PROJECT_DIR/$elf_name"
    else
        print_error "ELF file not found: $elf_name"
        FAILED_COUNT=$((FAILED_COUNT + 1))
        continue
    fi

    # Generate output filename
    base_name=$(basename "$elf_name" .elf)
    REF_LOG="${OUTPUT_DIR}/${base_name}.log.ref"

    print_info "Processing: $elf_name -> ${base_name}.log.ref"

    # Run Spike with pk and truncate at tohost write
    # The awk command:
    #   - Keeps only lines where PC is in the test program range (0x80000000 - 0x8000100)
    #   - The test program runs in M-mode (privilege 0) when using pk
    #   - Stops when pk takes over (pk runs at higher addresses in same privilege)
    timeout 10 "$SPIKE_BIN" --log-commits "$PK_BIN" "$ELF_PATH" 2>&1 | \
        awk '/^core   0: 0 0x/ && $4 >= "0x0000000080000000" && $4 < "0x0000000080000100" {print}
             /^core   0: 0 0x/ && $4 >= "0x0000000080000100" {exit}' > "$REF_LOG"

    # Check if we got log output
    if [ -f "$REF_LOG" ] && [ -s "$REF_LOG" ]; then
        LINE_COUNT=$(wc -l < "$REF_LOG")
        print_success "Generated: $REF_LOG ($LINE_COUNT lines)"
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        print_error "Empty or missing log for: $elf_name"
        rm -f "$REF_LOG"
        FAILED_COUNT=$((FAILED_COUNT + 1))
    fi

    if [ -n "$VERBOSE" ] && [ -f "$REF_LOG" ]; then
        echo "  Sample (first 3 lines):"
        head -3 "$REF_LOG" | sed 's/^/    /'
        echo "  Sample (last 3 lines):"
        tail -3 "$REF_LOG" | sed 's/^/    /'
        echo ""
    fi
done

# Summary
echo ""
echo "============================================"
echo "  Summary"
echo "============================================"
echo -e "  Total:  ${YELLOW}$((SUCCESS_COUNT + FAILED_COUNT))${NC}"
echo -e "  Success: ${GREEN}${SUCCESS_COUNT}${NC}"
if [ $FAILED_COUNT -gt 0 ]; then
    echo -e "  Failed: ${RED}${FAILED_COUNT}${NC}"
fi
echo ""

if [ $FAILED_COUNT -eq 0 ]; then
    print_success "All reference logs generated successfully!"
    echo ""
    echo "Reference logs are ready in: $OUTPUT_DIR"
    echo "Use scripts/run-ref-comparison.sh or scripts/compare.sh to compare with ruscv-sim output"
    exit 0
else
    print_error "Some reference logs failed to generate"
    exit 1
fi
