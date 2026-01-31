//! RV64D Performance Benchmark Suite
//!
//! Sprint 7 performance benchmarks for double-precision floating-point operations.
//!
//! Timing requirements:
//! - FADD.D: <60ns per operation
//! - FDIV.D: <300ns per operation

#![allow(clippy::unusual_byte_groupings)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionFormat, Opcode};
use ruscv_sim::execute::{exec_fadd_d, exec_fdiv_d, exec_fmul_d, exec_fsqrt_d, exec_fsub_d};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::memory::SimpleMemory;

/// Setup core state with double-precision FPU registers initialized
fn setup_fpu_state() -> CoreState {
    let mut state = CoreState::default();
    // Initialize FPU registers with test values (as f64 bits)
    state
        .fpr
        .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));
    state
        .fpr
        .write(2, Fpr::from_bits(std::f64::consts::E.to_bits()));
    state.fpr.write(3, Fpr::from_bits(2.0f64.to_bits()));
    state.fpr.write(4, Fpr::from_bits(16.0f64.to_bits()));
    state
        .fpr
        .write(5, Fpr::from_bits(std::f64::consts::PI.to_bits()));
    state
}

/// Create decoded instruction for D-extension arithmetic
fn create_d_arith_decoded(funct7: u8, rs1: u8, rs2: u8, rd: u8) -> DecodedInstruction {
    DecodedInstruction {
        raw: 0,
        format: InstructionFormat::RType,
        opcode: Opcode::OpFp,
        funct3: None,
        funct7: Some(funct7),
        rs1: Some(rs1),
        rs2: Some(rs2),
        rs3: None,
        rd: Some(rd),
        imm: None,
        branch_taken: false,
    }
}

/// Benchmark FADD.D instruction
/// Target: <60ns per operation
fn bench_fadd_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_fadd");

    // FADD.D: funct7=0000001 (0x01)
    let decoded = create_d_arith_decoded(0x01, 1, 2, 10);

    group.bench_function("FADD.D", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);
            exec_fadd_d(black_box(&decoded), &mut state, &mut mem).unwrap();
        })
    });

    // Benchmark with various operand combinations
    let operand_pairs: [(f64, f64); 5] = [
        (1.0, 2.0),
        (std::f64::consts::PI, std::f64::consts::E),
        (1e10, 1e-10),
        (f64::MAX / 2.0, f64::MAX / 4.0),
        (1e-300, 1e-300),
    ];

    for (i, (a, b)) in operand_pairs.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("operands", i),
            &(a, b),
            |bench, &(a, b)| {
                bench.iter(|| {
                    let mut state = CoreState::default();
                    state.fpr.write(1, Fpr::from_bits(a.to_bits()));
                    state.fpr.write(2, Fpr::from_bits(b.to_bits()));
                    let mut mem = SimpleMemory::new(0x1000);
                    exec_fadd_d(black_box(&decoded), &mut state, &mut mem).unwrap();
                })
            },
        );
    }

    group.finish();
}

/// Benchmark FDIV.D instruction
/// Target: <300ns per operation
fn bench_fdiv_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_fdiv");

    // FDIV.D: funct7=0001101 (0x0D) with D bit set = 0x2D (but actual encoding uses 0x0D)
    // According to execute/mod.rs: (0x0C, 0, true) => exec_fdiv_d
    // funct7 & 0x1F = 0x0C, so funct7 = 0x0C | 0x20 = 0x2C for D extension
    let decoded = create_d_arith_decoded(0x2C, 1, 3, 10);

    group.bench_function("FDIV.D", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);
            exec_fdiv_d(black_box(&decoded), &mut state, &mut mem).unwrap();
        })
    });

    // Benchmark division with various operand types
    let operand_pairs: [(f64, f64); 5] = [
        (10.0, 2.0),                                 // Simple division
        (std::f64::consts::PI, std::f64::consts::E), // Irrational numbers
        (1.0, 3.0),                                  // Repeating decimal
        (1e100, 1e50),                               // Large numbers
        (1e-100, 1e-50),                             // Small numbers
    ];

    for (i, (a, b)) in operand_pairs.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("operands", i),
            &(a, b),
            |bench, &(a, b)| {
                bench.iter(|| {
                    let mut state = CoreState::default();
                    state.fpr.write(1, Fpr::from_bits(a.to_bits()));
                    state.fpr.write(3, Fpr::from_bits(b.to_bits()));
                    let mut mem = SimpleMemory::new(0x1000);
                    exec_fdiv_d(black_box(&decoded), &mut state, &mut mem).unwrap();
                })
            },
        );
    }

    group.finish();
}

/// Benchmark FSUB.D instruction
fn bench_fsub_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_fsub");

    // FSUB.D: funct7=0000101 (0x05) with D bit set = 0x25
    let decoded = create_d_arith_decoded(0x25, 1, 2, 10);

    group.bench_function("FSUB.D", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);
            exec_fsub_d(black_box(&decoded), &mut state, &mut mem).unwrap();
        })
    });

    group.finish();
}

/// Benchmark FMUL.D instruction
fn bench_fmul_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_fmul");

    // FMUL.D: funct7=0001001 (0x09) with D bit set = 0x29
    let decoded = create_d_arith_decoded(0x29, 1, 2, 10);

    group.bench_function("FMUL.D", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);
            exec_fmul_d(black_box(&decoded), &mut state, &mut mem).unwrap();
        })
    });

    group.finish();
}

/// Benchmark FSQRT.D instruction
fn bench_fsqrt_d(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_fsqrt");

    // FSQRT.D: funct7=0101101 (0x2D) with D bit set = 0x4D
    let decoded = create_d_arith_decoded(0x4D, 4, 0, 10);

    group.bench_function("FSQRT.D", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);
            exec_fsqrt_d(black_box(&decoded), &mut state, &mut mem).unwrap();
        })
    });

    // Benchmark square root with various operand sizes
    let operands: [f64; 5] = [
        4.0,                  // Perfect square
        2.0,                  // Irrational result
        1e10,                 // Large number
        1e-10,                // Small number
        std::f64::consts::PI, // Irrational input
    ];

    for (i, val) in operands.iter().enumerate() {
        group.bench_with_input(BenchmarkId::new("operand", i), val, |bench, &val| {
            bench.iter(|| {
                let mut state = CoreState::default();
                state.fpr.write(4, Fpr::from_bits(val.to_bits()));
                let mut mem = SimpleMemory::new(0x1000);
                exec_fsqrt_d(black_box(&decoded), &mut state, &mut mem).unwrap();
            })
        });
    }

    group.finish();
}

/// Mixed RV64D operations benchmark
fn bench_rv64d_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_mixed");
    group.sample_size(50);

    // Create various D-extension instructions
    let fadd_decoded = create_d_arith_decoded(0x21, 1, 2, 10);
    let fsub_decoded = create_d_arith_decoded(0x25, 10, 3, 11);
    let fmul_decoded = create_d_arith_decoded(0x29, 11, 4, 12);
    let fdiv_decoded = create_d_arith_decoded(0x2D, 12, 5, 13);

    // Simulate a realistic FPU-intensive workload
    group.bench_function("mixed_sequence", |b| {
        b.iter(|| {
            let mut state = setup_fpu_state();
            let mut mem = SimpleMemory::new(0x1000);

            // Execute a chain of operations
            exec_fadd_d(black_box(&fadd_decoded), &mut state, &mut mem).unwrap();
            exec_fsub_d(black_box(&fsub_decoded), &mut state, &mut mem).unwrap();
            exec_fmul_d(black_box(&fmul_decoded), &mut state, &mut mem).unwrap();
            exec_fdiv_d(black_box(&fdiv_decoded), &mut state, &mut mem).unwrap();
        })
    });

    // Benchmark instruction throughput (multiple iterations)
    for inst_count in [10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("fadd_throughput", inst_count),
            inst_count,
            |bench, &count| {
                bench.iter(|| {
                    let mut state = setup_fpu_state();
                    let mut mem = SimpleMemory::new(0x1000);
                    for _ in 0..count {
                        exec_fadd_d(black_box(&fadd_decoded), &mut state, &mut mem).unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("fdiv_throughput", inst_count),
            inst_count,
            |bench, &count| {
                bench.iter(|| {
                    let mut state = setup_fpu_state();
                    let mut mem = SimpleMemory::new(0x1000);
                    for _ in 0..count {
                        exec_fdiv_d(black_box(&fdiv_decoded), &mut state, &mut mem).unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

/// Special values benchmark (NaN, Infinity, Denormalized)
fn bench_rv64d_special_values(c: &mut Criterion) {
    let mut group = c.benchmark_group("rv64d_special");

    let fadd_decoded = create_d_arith_decoded(0x21, 1, 2, 10);
    let fdiv_decoded = create_d_arith_decoded(0x2D, 1, 2, 10);

    // Benchmark operations with special IEEE 754 values
    let special_pairs: [(&str, f64, f64); 4] = [
        ("infinity", f64::INFINITY, 1.0),
        ("neg_infinity", f64::NEG_INFINITY, 1.0),
        ("nan", f64::NAN, 1.0),
        ("denormalized", 1e-310, 1e-310),
    ];

    for (name, a, b) in special_pairs.iter() {
        group.bench_with_input(BenchmarkId::new("fadd", name), &(a, b), |bench, &(a, b)| {
            bench.iter(|| {
                let mut state = CoreState::default();
                state.fpr.write(1, Fpr::from_bits(a.to_bits()));
                state.fpr.write(2, Fpr::from_bits(b.to_bits()));
                let mut mem = SimpleMemory::new(0x1000);
                exec_fadd_d(black_box(&fadd_decoded), &mut state, &mut mem).unwrap();
            })
        });

        if *name != "nan" {
            // Skip NaN for division (produces NaN which is expected)
            group.bench_with_input(BenchmarkId::new("fdiv", name), &(a, b), |bench, &(a, b)| {
                bench.iter(|| {
                    let mut state = CoreState::default();
                    state.fpr.write(1, Fpr::from_bits(a.to_bits()));
                    state.fpr.write(2, Fpr::from_bits(b.to_bits()));
                    let mut mem = SimpleMemory::new(0x1000);
                    exec_fdiv_d(black_box(&fdiv_decoded), &mut state, &mut mem).unwrap();
                })
            });
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_fadd_d,
    bench_fdiv_d,
    bench_fsub_d,
    bench_fmul_d,
    bench_fsqrt_d,
    bench_rv64d_mixed,
    bench_rv64d_special_values,
);
criterion_main!(benches);
