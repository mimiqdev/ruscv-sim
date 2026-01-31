//! Memory access benchmark suite
//!
//! Measures memory access latency for load/store operations

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};

fn memory_read_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_read_single");

    let mem = SimpleMemory::new(8192);

    group.bench_function("read_byte", |b| {
        b.iter(|| mem.read_byte(black_box(0x100)).unwrap())
    });

    group.bench_function("read_halfword", |b| {
        b.iter(|| {
            mem.read_half(black_box(0x100)).unwrap()
        })
    });

    group.bench_function("read_word", |b| {
        b.iter(|| mem.read_word(black_box(0x100)).unwrap())
    });

    group.finish();
}

fn memory_write_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_write_single");

    group.bench_function("write_byte", |b| {
        b.iter_batched(
            || SimpleMemory::new(8192),
            |mut mem| mem.write_byte(black_box(0x100), black_box(0x42)).unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("write_halfword", |b| {
        b.iter_batched(
            || SimpleMemory::new(8192),
            |mut mem| {
                mem.write_half(black_box(0x100), black_box(0x1234))
                    .unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("write_word", |b| {
        b.iter_batched(
            || SimpleMemory::new(8192),
            |mut mem| {
                mem.write_word(black_box(0x100), black_box(0x12345678))
                    .unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn memory_sequential_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_sequential");

    for count in [10, 100, 1000].iter() {
        let mem = SimpleMemory::new(8192);

        group.bench_with_input(BenchmarkId::new("read_words", count), count, |b, &count| {
            b.iter(|| {
                for i in 0..count {
                    let _ = mem.read_word(black_box((i * 4) as u32)).unwrap();
                }
            })
        });
    }

    for count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("write_words", count),
            count,
            |b, &count| {
                b.iter_batched(
                    || SimpleMemory::new(8192),
                    |mut mem| {
                        for i in 0..count {
                            mem.write_word(black_box((i * 4) as u32), black_box(i as u32))
                                .unwrap();
                        }
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn memory_random_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_random");

    // Generate pseudo-random addresses (deterministic for benchmarking)
    let addresses: Vec<u32> = (0..1000).map(|i| ((i * 97) % 2048) * 4).collect();

    for count in [10, 100, 1000].iter() {
        let mem = SimpleMemory::new(8192);
        let addrs = &addresses[0..*count];

        group.bench_with_input(BenchmarkId::new("read_words", count), count, |b, _| {
            b.iter(|| {
                for &addr in addrs {
                    let _ = mem.read_word(black_box(addr)).unwrap();
                }
            })
        });
    }

    for count in [10, 100, 1000].iter() {
        let addrs = &addresses[0..*count];

        group.bench_with_input(BenchmarkId::new("write_words", count), count, |b, _| {
            b.iter_batched(
                || SimpleMemory::new(8192),
                |mut mem| {
                    for (i, &addr) in addrs.iter().enumerate() {
                        mem.write_word(black_box(addr), black_box(i as u32))
                            .unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn memory_cache_effects(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_cache_effects");

    // Test cache line behavior (assuming 64-byte cache lines)
    let cache_line_size = 64;
    let words_per_line = cache_line_size / 4;

    let mem = SimpleMemory::new(8192);

    // Within same cache line
    group.bench_function("same_cache_line", |b| {
        b.iter(|| {
            for i in 0..words_per_line {
                let _ = mem.read_word(black_box(i * 4)).unwrap();
            }
        })
    });

    // Across cache lines
    group.bench_function("different_cache_lines", |b| {
        b.iter(|| {
            for i in 0..words_per_line {
                let _ = mem.read_word(black_box(i * cache_line_size)).unwrap();
            }
        })
    });

    group.finish();
}

fn memory_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_throughput");
    group.sample_size(50);

    for size_kb in [1, 4, 16].iter() {
        let word_count = (size_kb * 1024) / 4;
        let mem = SimpleMemory::new(size_kb * 1024);

        group.throughput(criterion::Throughput::Bytes((size_kb * 1024) as u64));

        group.bench_with_input(
            BenchmarkId::new("read", format!("{}KB", size_kb)),
            size_kb,
            |b, _| {
                b.iter(|| {
                    for i in 0..word_count {
                        let _ = mem.read_word(black_box((i * 4) as u32)).unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("write", format!("{}KB", size_kb)),
            size_kb,
            |b, _| {
                b.iter_batched(
                    || SimpleMemory::new(size_kb * 1024),
                    |mut mem| {
                        for i in 0..word_count {
                            mem.write_word(black_box((i * 4) as u32), black_box(i as u32))
                                .unwrap();
                        }
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    memory_read_single,
    memory_write_single,
    memory_sequential_access,
    memory_random_access,
    memory_cache_effects,
    memory_throughput
);
criterion_main!(benches);
