//! Benchmark suite для Bitmap SIMD operations
//!
//! Запуск: cargo bench --bench bitmap_benches
//!
//! Measures:
//! - Throughput (ops/sec)
//! - Latency (ns per operation)
//! - Performance across different bitmap sizes
//! - Comparison of all bitcount strategies

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::database::{bitmap::Bitmap, bitmap_simd::BitcountStrategy};

/// Benchmark bitcount performance for different strategies and sizes
fn bench_bitcount_strategies(c: &mut Criterion) {
    let sizes = vec![
        ("16B", 16),
        ("256B", 256),
        ("1KB", 1024),
        ("4KB", 4096),
        ("64KB", 64 * 1024),
        ("1MB", 1024 * 1024),
        ("10MB", 10 * 1024 * 1024),
    ];

    for (name, size) in sizes {
        let mut group = c.benchmark_group(format!("bitcount/{}", name));
        group.throughput(Throughput::Bytes(size as u64));

        // Create test bitmap with ~50% density
        let mut bitmap = Bitmap::with_capacity(size * 8);
        for i in 0..size * 8 {
            if i % 2 == 0 {
                bitmap.set_bit(i, true);
            }
        }

        // Используем move-замыкания, чтобы захватить bitmap (владение/мутабельный
        // binding), а не bench_with_input (который передаёт &T).
        {
            let bm = bitmap.clone(); // если Bitmap: Clone — использование clone безопасно; иначе уберите `.clone()`
                                     // и используйте `let mut bm = bitmap;` (перенесёт владение)
            group.bench_function("lookup_table", move |b| {
                b.iter(|| {
                    black_box(bm.bitcount_with_strategy(0, size * 8, BitcountStrategy::LookupTable))
                })
            });
        }

        {
            let bm = bitmap.clone();
            group.bench_function("popcnt", move |b| {
                b.iter(|| {
                    black_box(bm.bitcount_with_strategy(0, size * 8, BitcountStrategy::Popcnt))
                })
            });
        }

        {
            let bm = bitmap.clone();
            group.bench_function("avx2", move |b| {
                b.iter(|| black_box(bm.bitcount_with_strategy(0, size * 8, BitcountStrategy::Avx2)))
            });
        }

        {
            let bm = bitmap.clone();
            group.bench_function("avx512", move |b| {
                b.iter(|| {
                    black_box(bm.bitcount_with_strategy(0, size * 8, BitcountStrategy::Avx512))
                })
            });
        }

        {
            let bm = bitmap; // последнее использование, перемещаем оригинал
            group.bench_function("auto", move |b| {
                b.iter(|| black_box(bm.bitcount(0, size * 8)))
            });
        }

        group.finish();
    }
}

/// Benchmark different bitmap densities
fn bench_bitcount_density(c: &mut Criterion) {
    let size = 1024 * 1024; // 1MB
    let densities = vec![1, 10, 25, 50, 75, 90, 99];

    let mut group = c.benchmark_group("bitcount_density");
    group.throughput(Throughput::Bytes(size as u64));

    for density in densities {
        let mut bitmap = Bitmap::with_capacity(size * 8);
        for i in 0..size * 8 {
            if (i * 100) % (size * 8) < (size * 8 * density / 100) {
                bitmap.set_bit(i, true);
            }
        }

        // Вместо bench_with_input (который передаёт &bitmap) используем move-замыкание,
        // которое захватывает bitmap как mutable владение и может вызывать методы,
        // требующие &mut.
        let id = BenchmarkId::from_parameter(format!("{}%", density));
        let bm = bitmap; // захватываем bitmap в замыкании
        group.bench_function(id, move |b| b.iter(|| black_box(bm.bitcount(0, size * 8))));
    }

    group.finish();
}

/// Benchmark set_bit operation
fn bench_set_bit(c: &mut Criterion) {
    let mut group = c.benchmark_group("set_bit");

    group.bench_function("sequential", |b| {
        // Не возвращаем ссылку на захваченную переменную — возвращаем `()` через
        // black_box
        let mut bitmap = Bitmap::new();
        let mut counter = 0;
        b.iter(|| {
            bitmap.set_bit(counter, true);
            counter = (counter + 1) % 10000;
            black_box(()) // ничего не возвращаем, чтобы не "утекала" ссылка
        })
    });

    group.bench_function("random_small", |b| {
        // аналогично: не возвращаем ссылку
        let mut bitmap = Bitmap::with_capacity(1000);
        b.iter(|| {
            let pos = fastrand::usize(..1000);
            bitmap.set_bit(pos, true);
            black_box(())
        })
    });

    group.bench_function("expansion", |b| {
        b.iter(|| {
            let mut bitmap = Bitmap::new();
            bitmap.set_bit(100000, true);
            black_box(bitmap) // здесь мы создаём bitmap внутри итерации и
                              // передаём его в black_box — это валидно
        })
    });

    group.finish();
}

/// Benchmark get_bit operation
fn bench_get_bit(c: &mut Criterion) {
    let mut bitmap = Bitmap::with_capacity(10000);
    for i in 0..10000 {
        if i % 2 == 0 {
            bitmap.set_bit(i, true);
        }
    }

    let mut group = c.benchmark_group("get_bit");

    group.bench_function("sequential", |b| {
        let mut counter = 0;
        b.iter(|| {
            let result = bitmap.get_bit(counter);
            counter = (counter + 1) % 10000;
            black_box(result)
        })
    });

    group.bench_function("random", |b| {
        b.iter(|| {
            let pos = fastrand::usize(..10000);
            black_box(bitmap.get_bit(pos))
        })
    });

    group.finish();
}

/// Benchmark bitwise operations
fn bench_bitwise_ops(c: &mut Criterion) {
    let size = 64 * 1024; // 64KB
    let mut bitmap_a = Bitmap::with_capacity(size * 8);
    let mut bitmap_b = Bitmap::with_capacity(size * 8);

    for i in 0..size * 8 {
        if i % 2 == 0 {
            bitmap_a.set_bit(i, true);
        }
        if i % 3 == 0 {
            bitmap_b.set_bit(i, true);
        }
    }

    let mut group = c.benchmark_group("bitwise_ops");
    group.throughput(Throughput::Bytes(size as u64));

    group.bench_function("and", |b| b.iter(|| black_box(&bitmap_a & &bitmap_b)));

    group.bench_function("or", |b| b.iter(|| black_box(&bitmap_a | &bitmap_b)));

    group.bench_function("xor", |b| b.iter(|| black_box(&bitmap_a ^ &bitmap_b)));

    group.bench_function("not", |b| b.iter(|| black_box(!&bitmap_a)));

    group.finish();
}

/// Benchmark aligned vs unaligned memory access
fn bench_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("alignment");

    // Aligned bitmap (size divisible by 64)
    let mut aligned = Bitmap::with_capacity(64 * 1024 * 8);
    for i in 0..64 * 1024 * 8 {
        if i % 2 == 0 {
            aligned.set_bit(i, true);
        }
    }

    // Unaligned bitmap (odd size)
    let mut unaligned = Bitmap::with_capacity(65535);
    for i in 0..65535 {
        if i % 2 == 0 {
            unaligned.set_bit(i, true);
        }
    }

    group.bench_function("aligned_64KB", |b| {
        b.iter(|| black_box(aligned.bitcount(0, 64 * 1024 * 8)))
    });

    group.bench_function("unaligned_64KB", |b| {
        b.iter(|| black_box(unaligned.bitcount(0, 65535)))
    });

    group.finish();
}

/// Benchmark real-world patterns
fn bench_real_world_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("real_world");

    // Sparse bitmap (< 1% density)
    let mut sparse = Bitmap::with_capacity(1024 * 1024 * 8);
    for i in (0..1024 * 1024 * 8).step_by(1000) {
        sparse.set_bit(i, true);
    }

    // Dense bitmap (> 99% density)
    let mut dense = Bitmap::with_capacity(1024 * 1024 * 8);
    for i in 0..1024 * 1024 * 8 {
        dense.set_bit(i, true);
    }
    for i in (0..1024 * 1024 * 8).step_by(1000) {
        dense.set_bit(i, false);
    }

    // Clustered pattern
    let mut clustered = Bitmap::with_capacity(1024 * 1024 * 8);
    for cluster in 0..1000 {
        let start = cluster * 10000;
        for i in start..start + 100 {
            if i < 1024 * 1024 * 8 {
                clustered.set_bit(i, true);
            }
        }
    }

    group.bench_function("sparse_1pct", |b| {
        b.iter(|| black_box(sparse.bitcount_all()))
    });

    group.bench_function("dense_99pct", |b| {
        b.iter(|| black_box(dense.bitcount_all()))
    });

    group.bench_function("clustered", |b| {
        b.iter(|| black_box(clustered.bitcount_all()))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_bitcount_strategies,
    bench_bitcount_density,
    bench_set_bit,
    bench_get_bit,
    bench_bitwise_ops,
    bench_alignment,
    bench_real_world_patterns,
);

criterion_main!(benches);
