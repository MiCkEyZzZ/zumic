use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::Dict;

const SIZES: &[usize] = &[256, 1_024, 8_192, 65_536];
const BASE_SHRINK_N: usize = 4_096;
const RESERVE_BASE: usize = 512;
const RESERVE_EXTRA: usize = 1_024;
const REMOVE_WAVE: usize = 512;
const REMOVE_WAVES: usize = 6;

fn bench_insert_with_rehash(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/insert_no_prealloc");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut d: Dict<u64, u64> = Dict::new();
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });
    }

    group.finish();
}

fn bench_insert_with_capacity(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/insert_with_capacity");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut d: Dict<u64, u64> = Dict::with_capacity(n);
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });
    }

    group.finish();
}

fn bench_reserve_vs_capacity(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/reserve_vs_capacity");

    group.bench_function("reserve_after_insert", |b| {
        b.iter_batched(
            || {
                let mut d: Dict<u64, u64> = Dict::new();
                for i in 0..RESERVE_BASE as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                d.reserve(black_box(RESERVE_EXTRA));
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("with_capacity_upfront", |b| {
        b.iter_batched(
            || Dict::with_capacity(RESERVE_BASE + RESERVE_EXTRA),
            |mut d| {
                for i in 0..RESERVE_BASE as u64 {
                    d.insert(i, i);
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_shrink_to_fit(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/shrink_to_fit");
    let keep_ratios: &[f64] = &[0.05, 0.10, 0.25];

    for &ratio in keep_ratios {
        let keep = (BASE_SHRINK_N as f64 * ratio) as usize;
        let label = format!("keep_{:.0}pct", ratio * 100.0);

        group.bench_with_input(
            BenchmarkId::new("shrink_to_fit", &label),
            &keep,
            |b, &keep| {
                b.iter_batched(
                    || {
                        let mut d: Dict<u64, u64> = Dict::new();
                        for i in 0..BASE_SHRINK_N as u64 {
                            d.insert(i, i);
                        }
                        for i in keep as u64..BASE_SHRINK_N as u64 {
                            d.remove(&i);
                        }
                        d
                    },
                    |mut d| {
                        d.shrink_to_fit();
                        black_box(d)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_remove_with_auto_shrink(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/remove_cost");
    const N: usize = 1_024;

    group.bench_function("remove_all_with_shrink", |b| {
        b.iter_batched(
            || {
                let mut d: Dict<u64, u64> = Dict::new();
                for i in 0..N as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.remove(black_box(&i)));
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_wave_insert_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/wave_insert_remove");
    group.throughput(Throughput::Elements((REMOVE_WAVE * REMOVE_WAVES) as u64));

    group.bench_function("waves", |b| {
        b.iter(|| {
            let mut d: Dict<u64, u64> = Dict::new();
            for wave in 0..REMOVE_WAVES as u64 {
                let base = wave * REMOVE_WAVE as u64;
                for i in base..base + REMOVE_WAVE as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                if wave > 0 {
                    let prev = (wave - 1) * REMOVE_WAVE as u64;
                    for i in prev..prev + REMOVE_WAVE as u64 {
                        d.remove(black_box(&i));
                    }
                }
            }
            black_box(d)
        });
    });

    group.bench_function("waves_with_reserve", |b| {
        b.iter(|| {
            let mut d: Dict<u64, u64> = Dict::with_capacity(REMOVE_WAVE);
            for wave in 0..REMOVE_WAVES as u64 {
                d.reserve(REMOVE_WAVE);
                let base = wave * REMOVE_WAVE as u64;
                for i in base..base + REMOVE_WAVE as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                if wave > 0 {
                    let prev = (wave - 1) * REMOVE_WAVE as u64;
                    for i in prev..prev + REMOVE_WAVE as u64 {
                        d.remove(black_box(&i));
                    }
                }
            }
            black_box(d)
        });
    });

    group.finish();
}

fn bench_rehash_trigger_point(c: &mut Criterion) {
    let mut group = c.benchmark_group("rehash/trigger_point");

    for k in 1..=6usize {
        let n = 1usize << (k + 1);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("inserts_until_rehash", n), &n, |b, &n| {
            b.iter(|| {
                let mut d: Dict<u64, u64> = Dict::new();
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_insert_with_rehash,
    bench_insert_with_capacity,
    bench_reserve_vs_capacity,
    bench_shrink_to_fit,
    bench_remove_with_auto_shrink,
    bench_wave_insert_remove,
    bench_rehash_trigger_point,
);
criterion_main!(benches);
