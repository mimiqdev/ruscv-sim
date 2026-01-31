//! Cache simulation benchmarks
//!
//! This module benchmarks cache behavior and cache effects on simulation performance.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::Rng;

/// Cache parameters
#[derive(Debug, Clone)]
pub struct CacheParams {
    pub size: usize,      // Cache size in bytes
    pub line_size: usize, // Cache line size in bytes
    pub associativity: usize,
    pub latency: u32, // Cache hit latency in cycles
}

impl Default for CacheParams {
    fn default() -> Self {
        Self {
            size: 32 * 1024,  // 32KB
            line_size: 64,    // 64 bytes
            associativity: 4, // 4-way set associative
            latency: 4,       // 4 cycles
        }
    }
}

/// Simple cache simulator
#[derive(Debug)]
pub struct SimpleCache {
    params: CacheParams,
    sets: Vec<Vec<u64>>, // tag -> address tag
    pub hits: u64,
    pub misses: u64,
    pub accesses: u64,
}

impl SimpleCache {
    pub fn new(params: CacheParams) -> Self {
        let num_sets = params.size / (params.line_size * params.associativity);
        Self {
            params,
            sets: vec![Vec::new(); num_sets],
            hits: 0,
            misses: 0,
            accesses: 0,
        }
    }

    /// Access the cache
    pub fn access(&mut self, addr: u64) -> u32 {
        self.accesses += 1;
        let set_index = ((addr / self.params.line_size as u64) as usize) % self.sets.len();
        let tag = addr / (self.params.line_size * self.sets.len()) as u64;

        let set = &mut self.sets[set_index];

        // Check if tag is in set (LRU replacement)
        if let Some(pos) = set.iter().position(|&t| t == tag) {
            self.hits += 1;
            // Move to front (LRU)
            set.remove(pos);
            set.insert(0, tag);
            self.params.latency
        } else {
            self.misses += 1;
            // Insert new tag
            if set.len() >= self.params.associativity {
                set.pop(); // Evict LRU
            }
            set.insert(0, tag);
            // Miss penalty: larger than hit latency
            self.params.latency * 10
        }
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.accesses as f64
        }
    }

    /// Reset cache state
    pub fn reset(&mut self) {
        for set in &mut self.sets {
            set.clear();
        }
        self.hits = 0;
        self.misses = 0;
        self.accesses = 0;
    }
}

/// Memory access pattern: sequential
pub fn generate_sequential_accesses(count: usize, base: u64, stride: usize) -> Vec<u64> {
    (0..count).map(|i| base + (i * stride) as u64).collect()
}

/// Memory access pattern: random
pub fn generate_random_accesses(count: usize, base: u64, range: usize) -> Vec<u64> {
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|_| base + rng.gen::<usize>() % range as u64)
        .collect()
}

/// Memory access pattern: stride
pub fn generate_stride_accesses(count: usize, base: u64, stride: usize) -> Vec<u64> {
    let mut accesses = Vec::with_capacity(count);
    let mut addr = base;
    for _ in 0..count {
        accesses.push(addr);
        addr = (addr + stride as u64) & 0xFFFF; // Wrap around
    }
    accesses
}

/// Memory access pattern: working set (hot/cold)
pub fn generate_working_set_accesses(count: usize, hot_size: usize, cold_size: usize) -> Vec<u64> {
    let mut rng = rand::thread_rng();
    let mut accesses = Vec::with_capacity(count);

    for _ in 0..count {
        if rng.gen::<f64>() < 0.9 {
            // 90% hot accesses
            accesses.push(rng.gen::<u64>() % hot_size as u64);
        } else {
            // 10% cold accesses
            accesses.push((hot_size + rng.gen::<usize>() % cold_size) as u64);
        }
    }
    accesses
}

/// Benchmark: Sequential access pattern
fn bench_cache_sequential(c: &mut Criterion) {
    let cache = SimpleCache::new(CacheParams {
        size: 32 * 1024,
        line_size: 64,
        associativity: 4,
        latency: 4,
        ..Default::default()
    });
    let accesses = generate_sequential_accesses(10000, 0x1000, 8);

    c.bench_function("cache_sequential_10000", |b| {
        b.iter(|| {
            let mut cache = cache.clone();
            let mut total_latency = 0;
            for &addr in &accesses {
                total_latency += cache.access(addr);
            }
            black_box((total_latency, cache.hit_rate()));
        })
    });
}

/// Benchmark: Random access pattern
fn bench_cache_random(c: &mut Criterion) {
    let cache = SimpleCache::new(CacheParams {
        size: 32 * 1024,
        line_size: 64,
        associativity: 4,
        latency: 4,
        ..Default::default()
    });
    let accesses = generate_random_accesses(10000, 0x1000, 64 * 1024);

    c.bench_function("cache_random_10000", |b| {
        b.iter(|| {
            let mut cache = cache.clone();
            let mut total_latency = 0;
            for &addr in &accesses {
                total_latency += cache.access(addr);
            }
            black_box((total_latency, cache.hit_rate()));
        })
    });
}

/// Benchmark: Stride access pattern
fn bench_cache_stride(c: &mut Criterion) {
    let strides = [8, 64, 256, 1024];

    for stride in strides {
        let cache = SimpleCache::new(Default::default());
        let accesses = generate_stride_accesses(10000, 0x1000, stride);
        let name = format!("cache_stride_{}", stride);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut cache = cache.clone();
                let mut total_latency = 0;
                for &addr in &accesses {
                    total_latency += cache.access(addr);
                }
                black_box((total_latency, cache.hit_rate()));
            })
        });
    }
}

/// Benchmark: Working set access pattern
fn bench_cache_working_set(c: &mut Criterion) {
    let cache = SimpleCache::new(CacheParams {
        size: 8 * 1024, // Smaller cache
        line_size: 64,
        associativity: 4,
        latency: 4,
        ..Default::default()
    });
    let accesses = generate_working_set_accesses(10000, 512, 8192);

    c.bench_function("cache_working_set", |b| {
        b.iter(|| {
            let mut cache = cache.clone();
            let mut total_latency = 0;
            for &addr in &accesses {
                total_latency += cache.access(addr);
            }
            black_box((total_latency, cache.hit_rate()));
        })
    });
}

/// Benchmark: Cache size impact on hit rate
fn bench_cache_size_impact(c: &mut Criterion) {
    let sizes = [4 * 1024, 16 * 1024, 64 * 1024, 256 * 1024];
    let accesses = generate_random_accesses(10000, 0x1000, 64 * 1024);

    for size in sizes {
        let cache = SimpleCache::new(CacheParams {
            size,
            line_size: 64,
            associativity: 4,
            latency: 4,
            ..Default::default()
        });
        let name = format!("cache_size_{}", size / 1024);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut cache = cache.clone();
                let mut total_latency = 0;
                for &addr in &accesses {
                    total_latency += cache.access(addr);
                }
                black_box((total_latency, cache.hit_rate()));
            })
        });
    }
}

/// Benchmark: Cache line size impact
fn bench_cache_line_size_impact(c: &mut Criterion) {
    let line_sizes = [16, 32, 64, 128];
    let accesses = generate_sequential_accesses(10000, 0x1000, 8);

    for line_size in line_sizes {
        let cache = SimpleCache::new(CacheParams {
            size: 32 * 1024,
            line_size,
            associativity: 4,
            latency: 4,
            ..Default::default()
        });
        let name = format!("cache_line_size_{}", line_size);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut cache = cache.clone();
                let mut total_latency = 0;
                for &addr in &accesses {
                    total_latency += cache.access(addr);
                }
                black_box((total_latency, cache.hit_rate()));
            })
        });
    }
}

/// Benchmark: Cache associativity impact
fn bench_cache_associativity_impact(c: &mut Criterion) {
    let associativities = [1, 2, 4, 8];
    let accesses = generate_random_accesses(10000, 0x1000, 16 * 1024);

    for assoc in associativities {
        let cache = SimpleCache::new(CacheParams {
            size: 32 * 1024,
            line_size: 64,
            associativity: assoc,
            latency: 4,
            ..Default::default()
        });
        let name = format!("cache_assoc_{}", assoc);

        c.bench_function(&name, |b| {
            b.iter(|| {
                let mut cache = cache.clone();
                let mut total_latency = 0;
                for &addr in &accesses {
                    total_latency += cache.access(addr);
                }
                black_box((total_latency, cache.hit_rate()));
            })
        });
    }
}

/// Benchmark: Cache throughput
fn bench_cache_throughput(c: &mut Criterion) {
    c.bench_function("cache_throughput_100000", |b| {
        b.iter(|| {
            let mut cache = SimpleCache::new(Default::default());
            let mut rng = rand::thread_rng();
            let mut total_latency = 0;

            for _ in 0..100000 {
                let addr = rng.gen::<u64>() % (64 * 1024);
                total_latency += cache.access(addr);
            }
            black_box(total_latency);
        })
    });
}

/// Benchmark: Cache miss penalty
fn bench_cache_miss_penalty(c: &mut Criterion) {
    c.bench_function("cache_miss_penalty", |b| {
        b.iter(|| {
            let mut cache = SimpleCache::new(CacheParams {
                size: 4 * 1024, // Small cache to force misses
                line_size: 64,
                associativity: 1,
                latency: 4,
                ..Default::default()
            });
            let accesses = generate_random_accesses(1000, 0x1000, 1 * 1024 * 1024);

            let mut total_latency = 0;
            for &addr in &accesses {
                total_latency += cache.access(addr);
            }

            black_box((total_latency, cache.misses));
        })
    });
}

/// Benchmark: Multi-level cache simulation
fn bench_multilevel_cache(c: &mut Criterion) {
    c.bench_function("multilevel_cache", |b| {
        b.iter(|| {
            // L1 cache: small, fast
            let mut l1 = SimpleCache::new(CacheParams {
                size: 16 * 1024,
                line_size: 64,
                associativity: 4,
                latency: 4,
                ..Default::default()
            });

            // L2 cache: larger, slower
            let mut l2 = SimpleCache::new(CacheParams {
                size: 256 * 1024,
                line_size: 64,
                associativity: 8,
                latency: 12,
                ..Default::default()
            });

            let accesses = generate_random_accesses(10000, 0x1000, 1024 * 1024);
            let mut total_latency = 0;

            for &addr in &accesses {
                // Check L1 first
                let l1_latency = l1.access(addr);
                if l1.hits == 0 && l1.misses == 1 {
                    // L1 miss - check L2
                    let l2_latency = l2.access(addr);
                    total_latency += l1_latency + l2_latency;
                } else {
                    total_latency += l1_latency;
                }
            }

            black_box(total_latency);
        })
    });
}

criterion_group!(
    cache_simulation,
    bench_cache_sequential,
    bench_cache_random,
    bench_cache_stride,
    bench_cache_working_set,
    bench_cache_size_impact,
    bench_cache_line_size_impact,
    bench_cache_associativity_impact,
    bench_cache_throughput,
    bench_cache_miss_penalty,
    bench_multilevel_cache
);

criterion_main!(cache_simulation);
