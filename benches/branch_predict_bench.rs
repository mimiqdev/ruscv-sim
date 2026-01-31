//! Branch prediction benchmarks
//!
//! This module benchmarks branch prediction simulation performance.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::Rng;

/// Simple branch predictor: always not taken
#[derive(Debug, Default, Clone)]
pub struct AlwaysNotTakenPredictor;

impl Predictor for AlwaysNotTakenPredictor {
    fn predict(&self, _pc: u32) -> bool {
        false
    }

    fn update(&mut self, _pc: u32, _taken: bool) {
        // No state to update
    }
}

/// Simple branch predictor: always taken
#[derive(Debug, Default, Clone)]
pub struct AlwaysTakenPredictor;

impl Predictor for AlwaysTakenPredictor {
    fn predict(&self, _pc: u32) -> bool {
        true
    }

    fn update(&mut self, _pc: u32, _taken: bool) {
        // No state to update
    }
}

/// 1-bit branch predictor
#[derive(Debug, Clone)]
pub struct OneBitPredictor {
    table: Vec<bool>,
    size: usize,
}

impl OneBitPredictor {
    pub fn new(size: usize) -> Self {
        Self {
            table: vec![false; size],
            size,
        }
    }
}

impl Predictor for OneBitPredictor {
    fn predict(&self, pc: u32) -> bool {
        let index = (pc as usize) % self.size;
        self.table[index]
    }

    fn update(&mut self, pc: u32, taken: bool) {
        let index = (pc as usize) % self.size;
        self.table[index] = taken;
    }
}

/// 2-bit saturating counter predictor
#[derive(Debug, Clone)]
pub struct TwoBitPredictor {
    table: Vec<u8>, // 0-3: 0=strongly not taken, 1=weakly not taken, 2=weakly taken, 3=strongly taken
    size: usize,
}

impl TwoBitPredictor {
    pub fn new(size: usize) -> Self {
        Self {
            table: vec![1; size], // Initialize to weakly not taken
            size,
        }
    }
}

impl Predictor for TwoBitPredictor {
    fn predict(&self, pc: u32) -> bool {
        let index = (pc as usize) % self.size;
        self.table[index] >= 2
    }

    fn update(&mut self, pc: u32, taken: bool) {
        let index = (pc as usize) % self.size;
        let current = self.table[index];
        if taken {
            self.table[index] = (current + 1).min(3);
        } else {
            self.table[index] = current.saturating_sub(1);
        }
    }
}

/// Branch instruction with known outcome for testing
#[derive(Debug, Clone)]
pub struct BranchInstruction {
    pc: u32,
    target: u32,
    taken: bool,
}

impl BranchInstruction {
    pub fn new(pc: u32, target: u32, taken: bool) -> Self {
        Self { pc, target, taken }
    }

    pub fn execute<P: Predictor>(&self, predictor: &mut P) {
        let predicted = predictor.predict(self.pc);
        predictor.update(self.pc, self.taken);
        black_box((predicted, self.taken));
    }
}

pub trait Predictor {
    fn predict(&self, pc: u32) -> bool;
    fn update(&mut self, pc: u32, taken: bool);
}

/// Generate a sequence of branch instructions with configurable taken rate
pub fn generate_branch_sequence(count: usize, taken_rate: f64) -> Vec<BranchInstruction> {
    let mut rng = rand::thread_rng();
    let mut branches = Vec::with_capacity(count);

    for i in 0..count {
        let taken = rng.gen::<f64>() < taken_rate;
        let pc = (i as u32) * 4;
        let target = pc + if taken { 0x100 } else { 4 };
        branches.push(BranchInstruction::new(pc, target, taken));
    }

    branches
}

/// Benchmark: Always not taken predictor
fn bench_always_not_taken(c: &mut Criterion) {
    let predictor = AlwaysNotTakenPredictor::default();
    let branches = generate_branch_sequence(1000, 0.5);

    c.bench_function("always_not_taken_1000", |b| {
        b.iter(|| {
            let mut pred = predictor.clone();
            for branch in &branches {
                branch.execute(&mut pred);
            }
        })
    });
}

/// Benchmark: Always taken predictor
fn bench_always_taken(c: &mut Criterion) {
    let predictor = AlwaysTakenPredictor::default();
    let branches = generate_branch_sequence(1000, 0.5);

    c.bench_function("always_taken_1000", |b| {
        b.iter(|| {
            let mut pred = predictor.clone();
            for branch in &branches {
                branch.execute(&mut pred);
            }
        })
    });
}

/// Benchmark: 1-bit predictor
fn bench_one_bit_predictor(c: &mut Criterion) {
    let mut predictor = OneBitPredictor::new(1024);
    let branches = generate_branch_sequence(1000, 0.5);

    c.bench_function("one_bit_predictor_1000", |b| {
        b.iter(|| {
            let mut pred = OneBitPredictor::new(1024);
            for branch in &branches {
                branch.execute(&mut pred);
            }
        })
    });
}

/// Benchmark: 2-bit saturating counter predictor
fn bench_two_bit_predictor(c: &mut Criterion) {
    let branches = generate_branch_sequence(1000, 0.5);

    c.bench_function("two_bit_predictor_1000", |b| {
        b.iter(|| {
            let mut pred = TwoBitPredictor::new(1024);
            for branch in &branches {
                branch.execute(&mut pred);
            }
        })
    });
}

/// Benchmark: Predictor accuracy at different taken rates
fn bench_predictor_accuracy(c: &mut Criterion) {
    let rates = [0.0, 0.25, 0.5, 0.75, 1.0];
    let predictor = TwoBitPredictor::new(1024);

    for rate in rates {
        let branches = generate_branch_sequence(10000, rate);
        let name = format!("two_bit_accuracy_taken_rate_{}", (rate * 100.0) as u32);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut pred = TwoBitPredictor::new(1024);
                let mut correct = 0;
                for branch in &branches {
                    let predicted = pred.predict(branch.pc);
                    pred.update(branch.pc, branch.taken);
                    if predicted == branch.taken {
                        correct += 1;
                    }
                }
                black_box(correct);
            })
        });
    }
}

/// Benchmark: Branch prediction throughput
fn bench_prediction_throughput(c: &mut Criterion) {
    c.bench_function("prediction_throughput_10000", |b| {
        b.iter(|| {
            let mut pred = TwoBitPredictor::new(2048);
            let mut correct = 0;
            for i in 0..10000 {
                let taken = i % 2 == 0;
                let predicted = pred.predict(i * 4);
                pred.update(i * 4, taken);
                if predicted == taken {
                    correct += 1;
                }
            }
            black_box(correct);
        })
    });
}

/// Benchmark: Table size impact on accuracy
fn bench_table_size_impact(c: &mut Criterion) {
    let sizes = [64, 256, 1024, 4096];
    let branches = generate_branch_sequence(10000, 0.5);

    for size in sizes {
        let name = format!("table_size_{}", size);
        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut pred = TwoBitPredictor::new(size);
                let mut correct = 0;
                for branch in &branches {
                    let predicted = pred.predict(branch.pc);
                    pred.update(branch.pc, branch.taken);
                    if predicted == branch.taken {
                        correct += 1;
                    }
                }
                black_box(correct);
            })
        });
    }
}

criterion_group!(
    branch_prediction,
    bench_always_not_taken,
    bench_always_taken,
    bench_one_bit_predictor,
    bench_two_bit_predictor,
    bench_predictor_accuracy,
    bench_prediction_throughput,
    bench_table_size_impact
);

criterion_main!(branch_prediction);
