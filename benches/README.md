# RISC-V ISS Performance Benchmarks

This directory contains performance benchmarks for the RISC-V ISS simulator.

## Benchmark Suite

### 1. Decode Benchmarks (`decode_bench.rs`)

Measures instruction decode latency:

- **Single instruction decode**: Decode latency for each instruction format (R, I, S, B, U, J)
- **Batch decode**: Mixed instruction sequences
- **Throughput**: Decode performance at scale (10, 100, 1000 instructions)

### 2. Execute Benchmarks (`execute_bench.rs`)

Measures instruction execution latency:

- **Arithmetic operations**: ADD, SUB, SLL, etc.
- **Immediate operations**: ADDI, SLTI, XORI, etc.
- **Mixed sequences**: Realistic instruction mixes
- **CPI simulation**: Cycles Per Instruction metrics

### 3. Memory Benchmarks (`memory_bench.rs`)

Measures memory access latency:

- **Single access**: Read/Write byte, halfword, word
- **Sequential access**: Linear memory access patterns
- **Random access**: Non-sequential memory access
- **Cache effects**: Cache line behavior simulation
- **Throughput**: Memory bandwidth measurements

## Running Benchmarks

### Run all benchmarks
```bash
cargo bench
```

### Run specific benchmark
```bash
cargo bench --bench decode_bench
cargo bench --bench execute_bench
cargo bench --bench memory_bench
```

### Run specific test within benchmark
```bash
cargo bench --bench decode_bench -- decode_single
cargo bench --bench execute_bench -- execute_arithmetic
```

## Benchmark Results

Results are saved to `target/criterion/` with:
- HTML reports in `target/criterion/report/index.html`
- Raw data for trend analysis
- Statistical analysis (mean, median, std dev)

## Performance Metrics

### Baseline Targets (Initial Implementation)

| Metric | Target | Notes |
|--------|--------|-------|
| Decode latency | < 100 ns | Per instruction |
| Execute latency | < 200 ns | Arithmetic ops |
| Memory read | < 50 ns | Single word |
| Memory write | < 100 ns | Single word |
| CPI (simulated) | ~1.0 | Ideal single-cycle |

### Expected Performance Characteristics

1. **Decode**: O(1) - constant time per instruction
2. **Execute**: O(1) - varies by instruction complexity
3. **Memory**: O(1) - simple array-based implementation

## Interpreting Results

### Key Metrics

- **Mean**: Average execution time
- **Std Dev**: Consistency of measurements
- **Throughput**: Operations per second

### Performance Regression

Criterion automatically detects performance changes:
- ✅ Green: No significant change
- ⚠️ Yellow: Small regression detected
- ❌ Red: Significant regression

## Optimization Opportunities

Based on benchmark results, consider:

1. **Decode optimization**: Lookup tables for opcode dispatch
2. **Execute optimization**: Inline hot paths, reduce branching
3. **Memory optimization**: Cache-friendly data structures
4. **Batch processing**: SIMD or parallel decode/execute

## Continuous Benchmarking

Benchmarks should be run:
- Before major refactoring
- After optimization changes
- During sprint reviews
- Before releases

## Future Benchmarks (Completed)

The following benchmarks have been added:
- ✅ Branch prediction performance (`branch_predict_bench.rs`)
- ✅ Pipeline simulation (`pipeline_bench.rs`)
- ✅ Cache simulation (`cache_bench.rs`)

## CI Integration

Note: Benchmarks are NOT run in CI by default (too slow).
Run locally before submitting performance-critical changes.

To run in CI (manual trigger):
```bash
gh workflow run benchmark.yml
```
