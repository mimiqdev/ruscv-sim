//! Pipeline simulation benchmarks
//!
//! This module benchmarks pipeline stage performance and pipeline hazards.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::Rng;
use std::collections::VecDeque;

/// Pipeline stages
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PipelineStage {
    Fetch,
    Decode,
    Execute,
    Memory,
    WriteBack,
}

/// Instruction in the pipeline
#[derive(Debug, Clone, Copy)]
pub struct PipelineInstruction {
    pub pc: u32,
    pub stage: PipelineStage,
    pub opcode: u8,
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
    pub imm: u32,
}

impl Default for PipelineInstruction {
    fn default() -> Self {
        Self {
            pc: 0,
            stage: PipelineStage::Fetch,
            opcode: 0,
            rd: 0,
            rs1: 0,
            rs2: 0,
            imm: 0,
        }
    }
}

/// Simple 5-stage pipeline simulator
#[derive(Debug)]
pub struct SimplePipeline {
    stages: [Option<PipelineInstruction>; 5],
    pub cycles: u64,
    pub instructions_completed: u64,
    pub stalls: u64,
    pub flushes: u64,
}

impl Default for SimplePipeline {
    fn default() -> Self {
        Self {
            stages: [None; 5],
            cycles: 0,
            instructions_completed: 0,
            stalls: 0,
            flushes: 0,
        }
    }
}

impl SimplePipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clock cycle - advance pipeline by one stage
    pub fn tick(&mut self) {
        self.cycles += 1;

        // Write back stage
        if let Some(_instr) = self.stages[4].take() {
            self.instructions_completed += 1;
        }

        // Memory stage
        self.stages[4] = self.stages[3].take();

        // Execute stage
        self.stages[3] = self.stages[2].take();

        // Decode stage
        self.stages[2] = self.stages[1].take();

        // Fetch stage
        self.stages[1] = self.stages[0].take();
    }

    /// Fetch a new instruction
    pub fn fetch(&mut self, instr: PipelineInstruction) {
        self.stages[0] = Some(instr);
    }

    /// Check for structural hazards
    pub fn has_structural_hazard(&self) -> bool {
        // Simplified: no structural hazards in this model
        false
    }

    /// Check for data hazards (RAW, WAR, WAW)
    pub fn detect_data_hazard(&self, instr: &PipelineInstruction) -> bool {
        for stage in &self.stages {
            if let Some(staged) = stage {
                // RAW hazard check
                if instr.rs1 == staged.rd || instr.rs2 == staged.rd {
                    return true;
                }
            }
        }
        false
    }

    /// Insert a stall for a data hazard
    pub fn insert_stall(&mut self) {
        self.stalls += 1;
        // In a real pipeline, we would bubble the stage
    }

    /// Flush pipeline due to mispredicted branch
    pub fn flush(&mut self) {
        self.flushes += 1;
        // Clear all stages except writeback
        self.stages[0] = None;
        self.stages[1] = None;
        self.stages[2] = None;
        self.stages[3] = None;
    }
}

/// Generate a sequence of random instructions
pub fn generate_instruction_sequence(count: usize) -> Vec<PipelineInstruction> {
    let mut instrs = Vec::with_capacity(count);
    for i in 0..count {
        instrs.push(PipelineInstruction {
            pc: (i as u32) * 4,
            stage: PipelineStage::Fetch,
            opcode: (i % 32) as u8,
            rd: (i % 32) as u8,
            rs1: (i % 32) as u8,
            rs2: ((i + 1) % 32) as u8,
            imm: (i * 100) as u32,
        });
    }
    instrs
}

/// Generate instruction sequence with data hazards
pub fn generate_hazard_sequence(count: usize, hazard_rate: f64) -> Vec<PipelineInstruction> {
    let mut rng = rand::thread_rng();
    let mut instrs = Vec::with_capacity(count);

    for i in 0..count {
        let has_hazard = rng.gen::<f64>() < hazard_rate;
        let rs1 = if has_hazard && i > 0 {
            instrs[i - 1].rd
        } else {
            (i % 32) as u8
        };

        instrs.push(PipelineInstruction {
            pc: (i as u32) * 4,
            stage: PipelineStage::Fetch,
            opcode: (i % 32) as u8,
            rd: (i % 32) as u8,
            rs1,
            rs2: ((i + 1) % 32) as u8,
            imm: (i * 100) as u32,
        });
    }

    instrs
}

/// Benchmark: Pipeline throughput (instructions per cycle)
fn bench_pipeline_throughput(c: &mut Criterion) {
    let instrs = generate_instruction_sequence(1000);

    c.bench_function("pipeline_throughput_1000", |b| {
        b.iter(|| {
            let mut pipeline = SimplePipeline::new();
            let mut completed = 0;
            let mut pc = 0;

            // Simulate for enough cycles to complete all instructions
            for cycle in 0..(instrs.len() + 5) {
                if pc < instrs.len() {
                    pipeline.fetch(instrs[pc].clone());
                    pc += 1;
                }
                pipeline.tick();
                completed = pipeline.instructions_completed;
            }
            black_box(completed);
        })
    });
}

/// Benchmark: Pipeline with data hazards
fn bench_pipeline_with_hazards(c: &mut Criterion) {
    let hazard_rates = [0.0, 0.1, 0.25, 0.5];

    for rate in hazard_rates {
        let instrs = generate_hazard_sequence(1000, rate);
        let name = format!("pipeline_hazards_{}", (rate * 100.0) as u32);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut pipeline = SimplePipeline::new();
                let mut completed = 0;
                let mut pc = 0;

                for cycle in 0..(instrs.len() + 20) {
                    if pc < instrs.len() {
                        if !pipeline.detect_data_hazard(&instrs[pc]) {
                            pipeline.fetch(instrs[pc].clone());
                            pc += 1;
                        } else {
                            pipeline.insert_stall();
                        }
                    }
                    pipeline.tick();
                    completed = pipeline.instructions_completed;
                }
                black_box((completed, pipeline.stalls));
            })
        });
    }
}

/// Benchmark: Pipeline CPI (cycles per instruction)
fn bench_pipeline_cpi(c: &mut Criterion) {
    let counts = [100, 500, 1000, 5000];

    for count in counts {
        let instrs = generate_instruction_sequence(count);
        let name = format!("pipeline_cpi_{}", count);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut pipeline = SimplePipeline::new();
                let mut pc = 0;

                while pipeline.instructions_completed < count as u64 {
                    if pc < instrs.len() {
                        pipeline.fetch(instrs[pc].clone());
                        pc += 1;
                    }
                    pipeline.tick();
                }

                let cpi = pipeline.cycles as f64 / count as f64;
                black_box(cpi);
            })
        });
    }
}

/// Benchmark: Pipeline stall overhead
fn bench_pipeline_stalls(c: &mut Criterion) {
    c.bench_function("pipeline_stalls_overhead", |b| {
        b.iter(|| {
            let mut pipeline = SimplePipeline::new();
            let mut rng = rand::thread_rng();

            for i in 0..1000 {
                let stall_count = if rng.gen::<f64>() < 0.3 { 1 } else { 0 };
                for _ in 0..stall_count {
                    pipeline.insert_stall();
                }
                pipeline.tick();
            }
            black_box(pipeline.stalls);
        })
    });
}

/// Benchmark: Branch misprediction penalty
fn bench_branch_penalty(c: &mut Criterion) {
    c.bench_function("branch_mispredict_penalty", |b| {
        b.iter(|| {
            let mut pipeline = SimplePipeline::new();
            let penalty = 3; // Typical penalty for 5-stage pipeline

            // Simulate branch misprediction
            for i in 0..100 {
                pipeline.fetch(PipelineInstruction {
                    pc: (i as u32) * 4,
                    stage: PipelineStage::Fetch,
                    opcode: 0x63, // Branch opcode
                    ..Default::default()
                });
                pipeline.tick();
            }

            // Simulate misprediction and flush
            for _ in 0..penalty {
                pipeline.tick();
            }
            pipeline.flush();

            black_box(pipeline.flushes);
        })
    });
}

/// Benchmark: Pipeline depth impact
fn bench_pipeline_depth(c: &mut Criterion) {
    c.bench_function("pipeline_depth_scaling", |b| {
        b.iter(|| {
            let mut pipeline = SimplePipeline::new();

            // Simulate deeper pipeline with more stages
            for _depth in 0..10 {
                pipeline.tick();
            }

            black_box(pipeline.cycles);
        })
    });
}

/// Benchmark: Out-of-order execution simulation (simplified)
fn bench_out_of_order(c: &mut Criterion) {
    c.bench_function("out_of_order_simulation", |b| {
        b.iter(|| {
            let mut reorder_buffer: VecDeque<PipelineInstruction> = VecDeque::new();
            let mut reservation_stations = Vec::new();
            let mut completed = 0;

            let instrs = generate_instruction_sequence(1000);

            for instr in instrs {
                // Issue stage
                if reservation_stations.len() < 16 {
                    reservation_stations.push(instr);
                }

                // Execute ready instructions
                let mut executed = Vec::new();
                for (i, _rs) in reservation_stations.iter_mut().enumerate() {
                    // Simplified: all instructions are ready
                    executed.push(i);
                }

                // Remove executed instructions
                for i in executed.iter().rev() {
                    reorder_buffer.push_back(reservation_stations.remove(*i));
                    completed += 1;
                }

                // Retire completed instructions
                while let Some(instr) = reorder_buffer.pop_front() {
                    black_box(instr);
                    break;
                }
            }

            black_box(completed);
        })
    });
}

criterion_group!(
    pipeline_simulation,
    bench_pipeline_throughput,
    bench_pipeline_with_hazards,
    bench_pipeline_cpi,
    bench_pipeline_stalls,
    bench_branch_penalty,
    bench_pipeline_depth,
    bench_out_of_order
);

criterion_main!(pipeline_simulation);
