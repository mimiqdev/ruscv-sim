//! Execute benchmark suite
//!
//! Measures instruction execution latency for various RISC-V operations

#![allow(clippy::unusual_byte_groupings)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruscv_sim::core::CoreState;
use ruscv_sim::decode::InstructionDecoder;
use ruscv_sim::execute::Executor;
use ruscv_sim::memory::SimpleMemory;
use std::sync::{Arc, Mutex};

fn setup_core_state() -> CoreState {
    let mut state = CoreState::default();
    // Initialize some registers with test values
    state.regs[1] = 100;
    state.regs[2] = 200;
    state.regs[3] = 300;
    state.regs[4] = 0x1000;
    state.pc = 0;
    state
}

fn execute_arithmetic_ops(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();
    let mut executor = Executor::new();
    let mem = Arc::new(Mutex::new(SimpleMemory::new(8192)));

    let mut group = c.benchmark_group("execute_arithmetic");

    // ADD x5, x1, x2
    let add_inst = 0b0000000_00010_00001_000_00101_0110011u32;
    let decoded_add = decoder.decode(add_inst).unwrap();

    group.bench_function("ADD", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_add), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    // SUB x5, x2, x1
    let sub_inst = 0b0100000_00001_00010_000_00101_0110011u32;
    let decoded_sub = decoder.decode(sub_inst).unwrap();

    group.bench_function("SUB", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_sub), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    // SLL x5, x1, x2
    let sll_inst = 0b0000000_00010_00001_001_00101_0110011u32;
    let decoded_sll = decoder.decode(sll_inst).unwrap();

    group.bench_function("SLL", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_sll), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    group.finish();
}

fn execute_immediate_ops(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();
    let mut executor = Executor::new();
    let mem = Arc::new(Mutex::new(SimpleMemory::new(8192)));

    let mut group = c.benchmark_group("execute_immediate");

    // ADDI x5, x1, 42
    let addi_inst = 0b000000101010_00001_000_00101_0010011u32;
    let decoded_addi = decoder.decode(addi_inst).unwrap();

    group.bench_function("ADDI", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_addi), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    // SLTI x5, x1, 150
    let slti_inst = 0b000010010110_00001_010_00101_0010011u32;
    let decoded_slti = decoder.decode(slti_inst).unwrap();

    group.bench_function("SLTI", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_slti), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    // XORI x5, x1, 0xFF
    let xori_inst = 0b000011111111_00001_100_00101_0010011u32;
    let decoded_xori = decoder.decode(xori_inst).unwrap();

    group.bench_function("XORI", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            executor
                .execute(black_box(&decoded_xori), &mut state, &mut *mem_guard)
                .unwrap();
        })
    });

    group.finish();
}

fn execute_instruction_mix(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();
    let mut executor = Executor::new();
    let mem = Arc::new(Mutex::new(SimpleMemory::new(8192)));

    // Mixed instruction sequence
    let instructions = [
        0b0000000_00010_00001_000_00101_0110011u32, // ADD
        0b000000101010_00001_000_00101_0010011u32,  // ADDI
        0b0100000_00001_00010_000_00101_0110011u32, // SUB
        0b000011111111_00001_100_00101_0010011u32,  // XORI
        0b0000000_00010_00001_001_00101_0110011u32, // SLL
    ];

    let decoded: Vec<_> = instructions
        .iter()
        .map(|&inst| decoder.decode(inst).unwrap())
        .collect();

    c.bench_function("execute_mixed_sequence", |b| {
        b.iter(|| {
            let mut state = setup_core_state();
            let mut mem_guard = mem.lock().unwrap();
            for inst in &decoded {
                executor
                    .execute(black_box(inst), &mut state, &mut *mem_guard)
                    .unwrap();
            }
        })
    });
}

fn execute_cpi_simulation(c: &mut Criterion) {
    let decoder = InstructionDecoder::new();
    let mut executor = Executor::new();
    let mem = Arc::new(Mutex::new(SimpleMemory::new(8192)));

    let mut group = c.benchmark_group("cpi_simulation");
    group.sample_size(50);

    for inst_count in [100, 1000].iter() {
        // Generate a realistic instruction mix
        let mut instructions = Vec::with_capacity(*inst_count);
        for i in 0..*inst_count {
            let inst = match i % 8 {
                0 => 0b0000000_00010_00001_000_00101_0110011u32, // ADD (25%)
                1 | 2 => 0b000000101010_00001_000_00101_0010011u32, // ADDI (25%)
                3 => 0b0100000_00001_00010_000_00101_0110011u32, // SUB (12.5%)
                4 => 0b000011111111_00001_100_00101_0010011u32,  // XORI (12.5%)
                5 => 0b0000000_00010_00001_001_00101_0110011u32, // SLL (12.5%)
                6 => 0b0000000_00010_00001_111_00101_0110011u32, // AND (12.5%)
                _ => 0b0000000_00010_00001_110_00101_0110011u32, // OR (12.5%)
            };
            instructions.push(inst);
        }

        let decoded: Vec<_> = instructions
            .iter()
            .map(|&inst| decoder.decode(inst).unwrap())
            .collect();

        group.bench_with_input(
            BenchmarkId::new("instructions", inst_count),
            inst_count,
            |b, _| {
                b.iter(|| {
                    let mut state = setup_core_state();
                    let mut mem_guard = mem.lock().unwrap();
                    for inst in &decoded {
                        executor
                            .execute(black_box(inst), &mut state, &mut *mem_guard)
                            .unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    execute_arithmetic_ops,
    execute_immediate_ops,
    execute_instruction_mix,
    execute_cpi_simulation
);
criterion_main!(benches);
