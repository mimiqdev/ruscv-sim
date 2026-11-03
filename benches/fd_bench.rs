//! F/D instruction-execution benchmarks (contract U3a).
//!
//! Each sample decodes a real instruction word and executes it through the
//! single Hart FP engine, so the timing covers the architectural wrapper
//! (NaN-boxing, `fcsr` accrual, FS marking) around the U2 arithmetic core.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruscv_sim::core::CoreState;
use ruscv_sim::decode::{DecodedInstruction, InstructionDecoder};
use ruscv_sim::fpu::Fpr;
use ruscv_sim::isa::fd;
use ruscv_sim::memory::SimpleMemory;

fn op_fp(funct7: u32, rs2: u32, rs1: u32, rm: u32, rd: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (rm << 12) | (rd << 7) | 0b101_0011
}

/// FMADD.D fd, fs1, fs2, fs3
fn fmadd_d(rd: u32, rs1: u32, rs2: u32, rs3: u32) -> u32 {
    (rs3 << 27) | (1 << 25) | (rs2 << 20) | (rs1 << 15) | (rd << 7) | 0b100_0011
}

fn decode(raw: u32) -> DecodedInstruction {
    InstructionDecoder::new().decode(raw).unwrap()
}

/// NaN-box a single-precision bit pattern, as every 32-bit FPR write does.
fn boxed_single(bits: u32) -> u64 {
    ruscv_sim::fpu::NAN_BOX_MASK | u64::from(bits)
}

fn state_with(a: u64, b: u64) -> CoreState {
    let mut state = CoreState::default();
    state.fpr.write(1, Fpr::from_bits(a));
    state.fpr.write(2, Fpr::from_bits(b));
    state
}

fn run(decoded: &DecodedInstruction, a: u64, b: u64) {
    let mut state = state_with(a, b);
    let mut mem = SimpleMemory::new(0x1000);
    fd::execute(black_box(decoded), &mut state, &mut mem).unwrap();
}

fn arith_group(c: &mut Criterion, name: &str, raw: u32, operand: u64) {
    let decoded = decode(raw);
    c.bench_function(name, |b| b.iter(|| run(&decoded, operand, operand)));
}

fn bench_binary_formats(c: &mut Criterion) {
    arith_group(
        c,
        "fadd_s",
        op_fp(0x00, 2, 1, 0, 10),
        boxed_single(1.5f32.to_bits()),
    );
    arith_group(c, "fadd_d", op_fp(0x01, 2, 1, 0, 10), 1.5f64.to_bits());
    arith_group(c, "fmul_d", op_fp(0x09, 2, 1, 0, 10), 1.25f64.to_bits());
    arith_group(c, "fdiv_d", op_fp(0x0d, 2, 1, 0, 10), 3.5f64.to_bits());
    arith_group(c, "fsqrt_d", op_fp(0x2d, 0, 1, 0, 10), 2.0f64.to_bits());
    arith_group(c, "fmadd_d", fmadd_d(10, 1, 2, 1), 1.5f64.to_bits());
}

fn bench_operands(c: &mut Criterion) {
    let decoded = decode(op_fp(0x0d, 2, 1, 0, 10));
    let mut group = c.benchmark_group("fdiv_d_operands");
    for (index, (a, b)) in [
        (1.0f64, 3.0f64),
        (1e300, 1e-300),
        (1e-300, 1e300),
        (std::f64::consts::PI, std::f64::consts::E),
    ]
    .into_iter()
    .enumerate()
    {
        group.bench_with_input(
            BenchmarkId::new("operands", index),
            &(a, b),
            |bench, pair| bench.iter(|| run(&decoded, pair.0.to_bits(), pair.1.to_bits())),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_binary_formats, bench_operands);
criterion_main!(benches);
