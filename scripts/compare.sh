#!/bin/bash
#
# compare.sh - Compare ruscv-sim and Spike commit logs
#
# Usage:
#   ./compare.sh                    # Use default ELF (hello.elf)
#   ./compare.sh <elf_file>         # Use specified ELF
#   ./compare.sh <elf> <spike_log> <ruscv_log>
#
# This script:
#   1. Runs Spike to generate a commit log
#   2. Runs ruscv-sim to generate a commit log
#   3. Calls log-compare.py to compare the results
#   4. Outputs differences (if any)

set -e  # Exit on error

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default paths
RUSCV_SIM="${RUSCV_SIM:-$PROJECT_DIR/target/release/ruscv-sim}"
SPIKE_BIN="${SPIKE_BIN:-spike}"

# Colors for output (if terminal supports it)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    NC=''
fi

print_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] [ELF_FILE]

Compare ruscv-sim and Spike commit logs for a RISC-V ELF program.

Arguments:
  ELF_FILE           Path to RISC-V ELF file (default: hello.elf in project dir)

Options:
  -s, --spike-log    Path for Spike log output (default: spike_compare.log)
  -r, --ruscv-log    Path for ruscv-sim log output (default: ruscv_compare.log)
  -o, --output       Output file for comparison results (default: stdout)
  -a, --start-addr   Only compare instructions at/above this address (hex, e.g., 0x80000000)
  -v, --verbose      Enable verbose output
  -j, --json         Output comparison in JSON format
  -h, --help         Show this help message

Environment Variables:
  RUSCVSIM           Path to ruscv-sim binary (default: \$PROJECT_DIR/target/release/ruscv-sim)
  SPIKE              Path to Spike binary (default: spike)

Examples:
  $(basename "$0") hello.elf
  $(basename "$0") tests/fib.elf -v
  $(basename "$0") tests/fib.elf -a 0x80000000  # Only compare ELF entry point onwards
  $(basename "$0") --json -o diff.json hello.elf

EOF
}

# Parse arguments
SPIKE_LOG="spike_compare.log"
RUSCV_LOG="ruscv_compare.log"
OUTPUT=""
VERBOSE=""
JSON_OUTPUT=""
ELF_FILE="$PROJECT_DIR/hello.elf"

while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--spike-log)
            SPIKE_LOG="$2"
            shift 2
            ;;
        -r|--ruscv-log)
            RUSCV_LOG="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT="$2"
            shift 2
            ;;
        -a|--start-addr)
            START_ADDR="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE="--verbose"
            shift
            ;;
        -j|--json)
            JSON_OUTPUT="--json"
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
            ELF_FILE="$1"
            shift
            ;;
    esac
done

# Build start-addr argument for comparison
START_ADDR_ARG=""
if [ -n "$START_ADDR" ]; then
    START_ADDR_ARG="--start-addr $START_ADDR"
    print_info "Filtering instructions below: $START_ADDR"
fi

# Validate ELF file
if [ ! -f "$ELF_FILE" ]; then
    print_error "ELF file not found: $ELF_FILE"
    exit 1
fi

print_info "Starting comparison..."
print_info "ELF file: $ELF_FILE"
print_info "Spike log: $SPIKE_LOG"
print_info "ruscv-sim log: $RUSCV_LOG"

# Check if ruscv-sim exists
if [ ! -x "$RUSCV_SIM" ]; then
    print_error "ruscv-sim not found or not executable: $RUSCV_SIM"
    print_info "Build it with: cd $PROJECT_DIR && cargo build --release"
    exit 1
fi

# Check if Spike exists
if ! command -v "$SPIKE_BIN" &> /dev/null; then
    print_error "Spike not found. Please install Spike RISC-V ISA Simulator."
    print_info "On Ubuntu/Debian: apt install riscv-tools"
    print_info "Or build from source: https://github.com/riscv-software-src/riscv-isa-sim"
    exit 1
fi

# Clean up old logs
rm -f "$SPIKE_LOG" "$RUSCV_LOG"

print_info "Running Spike..."
# Run Spike with --log-commits
# Spike outputs to stderr by default, redirect to file
# Use timeout to prevent hanging on UART I/O
if timeout 30 "$SPIKE_BIN" --log-commits "$ELF_FILE" 2> "$SPIKE_LOG" > /dev/null; then
    print_success "Spike log generated: $(wc -l < "$SPIKE_LOG" 2>/dev/null || echo 0) lines"
else
    EXIT_CODE=$?
    if [ $EXIT_CODE -eq 124 ]; then
        print_info "Spike timed out (30s) - this is expected for programs waiting on UART"
    fi
    # Check if we got any log output anyway
    if [ -f "$SPIKE_LOG" ] && [ -s "$SPIKE_LOG" ]; then
        print_success "Spike log generated: $(wc -l < "$SPIKE_LOG") lines"
    else
        print_error "Spike failed to generate log"
        cat "$SPIKE_LOG" 2>/dev/null || true
        exit 1
    fi
fi

print_info "Running ruscv-sim..."
# Run ruscv-sim with --log-commits
if "$RUSCV_SIM" run "$ELF_FILE" --log-commits "$RUSCV_LOG" > /dev/null 2>&1; then
    print_success "ruscv-sim log generated: $(wc -l < "$RUSCV_LOG") lines"
else
    print_error "ruscv-sim execution failed"
    exit 1
fi

# Show sample logs for verification
if [ -n "$VERBOSE" ]; then
    echo ""
    echo "=== Sample from Spike log (first 5 lines) ==="
    head -5 "$SPIKE_LOG"
    echo ""
    echo "=== Sample from ruscv-sim log (first 5 lines) ==="
    head -5 "$RUSCV_LOG"
    echo ""
fi

# Run comparison
echo ""
print_info "Comparing logs..."

COMPARE_CMD="python3 $SCRIPT_DIR/log-compare.py $VERBOSE $JSON_OUTPUT $START_ADDR_ARG $SPIKE_LOG $RUSCV_LOG"

if [ -n "$OUTPUT" ]; then
    if eval "$COMPARE_CMD" > "$OUTPUT"; then
        print_success "Comparison complete. Results written to: $OUTPUT"
    else
        print_error "Comparison found differences. See $OUTPUT for details"
    fi
else
    eval "$COMPARE_CMD"
    EXIT_CODE=$?
    if [ $EXIT_CODE -eq 0 ]; then
        echo ""
        print_success "No differences found! ✅"
    else
        echo ""
        print_error "Differences found! See above for details ❌"
    fi
    exit $EXIT_CODE
fi

exit 0
