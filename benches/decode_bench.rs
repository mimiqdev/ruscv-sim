//! Decode benchmark suite
//!
//! Measures instruction decode latency for various RISC-V instruction formats

#![allow(clippy::unusual_byte_groupings)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruscv_sim::decode::InstructionDecoder;

fn decode_single_instruction(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();

    let mut group = c.benchmark_group("decode_single");

    // R-type: ADD x1, x2, x3
    let r_type_inst = 0b0000000_00011_00010_000_00001_0110011u32;
    group.bench_with_input(
        BenchmarkId::new("R-type", "ADD"),
        &r_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    // I-type: ADDI x1, x2, 100
    let i_type_inst = 0b000001100100_00010_000_00001_0010011u32;
    group.bench_with_input(
        BenchmarkId::new("I-type", "ADDI"),
        &i_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    // S-type: SW x3, 8(x2)
    let s_type_inst = 0b0000000_00011_00010_010_01000_0100011u32;
    group.bench_with_input(
        BenchmarkId::new("S-type", "SW"),
        &s_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    // B-type: BEQ x1, x2, 16
    let b_type_inst = 0b0000000_00010_00001_000_10000_1100011u32;
    group.bench_with_input(
        BenchmarkId::new("B-type", "BEQ"),
        &b_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    // U-type: LUI x1, 0x12345
    let u_type_inst = 0x12345037u32;
    group.bench_with_input(
        BenchmarkId::new("U-type", "LUI"),
        &u_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    // J-type: JAL x1, 2048
    let j_type_inst = 0b0_0000000001_0_00000000_00001_1101111u32;
    group.bench_with_input(
        BenchmarkId::new("J-type", "JAL"),
        &j_type_inst,
        |b, &inst| b.iter(|| decoder.decode(black_box(inst)).unwrap()),
    );

    group.finish();
}

fn decode_instruction_batch(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();

    // Create a batch of mixed instructions
    let instructions = vec![
        0b0000000_00011_00010_000_00001_0110011u32, // ADD
        0b000001100100_00010_000_00001_0010011u32,  // ADDI
        0b0000000_00011_00010_010_01000_0100011u32, // SW
        0b0000000_00010_00001_000_10000_1100011u32, // BEQ
        0x12345037u32,                              // LUI
        0b0_0000000001_0_00000000_00001_1101111u32, // JAL
    ];

    c.bench_function("decode_batch_mixed", |b| {
        b.iter(|| {
            for &inst in &instructions {
                let _ = decoder.decode(black_box(inst)).unwrap();
            }
        })
    });
}

fn decode_throughput(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();

    let mut group = c.benchmark_group("decode_throughput");

    for size in [10, 100, 1000].iter() {
        // Generate a sequence of mixed instructions
        let mut instructions = Vec::with_capacity(*size);
        for i in 0..*size {
            let inst = match i % 6 {
                0 => 0b0000000_00011_00010_000_00001_0110011u32, // ADD
                1 => 0b000001100100_00010_000_00001_0010011u32,  // ADDI
                2 => 0b0000000_00011_00010_010_01000_0100011u32, // SW
                3 => 0b0000000_00010_00001_000_10000_1100011u32, // BEQ
                4 => 0x12345037u32,                              // LUI
                _ => 0b0_0000000001_0_00000000_00001_1101111u32, // JAL
            };
            instructions.push(inst);
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                for &inst in &instructions {
                    let _ = decoder.decode(black_box(inst)).unwrap();
                }
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    decode_single_instruction,
    decode_instruction_batch,
    decode_throughput
);
criterion_main!(benches);
