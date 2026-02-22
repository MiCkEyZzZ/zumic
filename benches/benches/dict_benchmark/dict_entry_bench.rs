use std::{collections::HashMap, hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::Dict;

const SIZES: &[usize] = &[100, 1_000, 10_000, 100_000];

fn bench_or_insert_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry/or_insert_hit");
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        let mut base: Dict<u64, u64> = Dict::new();
        for i in 0..n as u64 {
            base.insert(i, i);
        }

        group.bench_with_input(BenchmarkId::new("entry_or_insert", n), &n, |b, &_n| {
            b.iter_batched(
                || base.clone(),
                |mut d| {
                    for i in 0..n as u64 {
                        let v = d.entry(black_box(i)).or_insert(0);
                        black_box(v);
                    }
                    black_box(d)
                },
                criterion::BatchSize::SmallInput,
            );
        });

        // Zumic Dict get_then_insert
        group.bench_with_input(BenchmarkId::new("get_then_insert", n), &n, |b, &_n| {
            b.iter_batched(
                || base.clone(),
                |mut d| {
                    for i in 0..n as u64 {
                        if let Some(v) = d.get(black_box(&i)) {
                            black_box(v);
                        } else {
                            d.insert(i, 0);
                        }
                    }
                    black_box(d)
                },
                criterion::BatchSize::SmallInput,
            );
        });

        let mut base_std: HashMap<u64, u64> = HashMap::new();
        for i in 0..n as u64 {
            base_std.insert(i, i);
        }

        group.bench_with_input(
            BenchmarkId::new("std_hashmap_entry_or_insert", n),
            &n,
            |b, &_n| {
                b.iter_batched(
                    || base_std.clone(),
                    |mut m| {
                        for i in 0..n as u64 {
                            let v = m.entry(black_box(i)).or_insert(0);
                            black_box(v);
                        }
                        black_box(m)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_or_insert_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry/or_insert_miss");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(60);

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        let mut rng = StdRng::seed_from_u64(0xC0FFEE);
        let keys: Vec<u64> = (0..n).map(|_| rng.gen::<u64>()).collect();

        group.bench_with_input(BenchmarkId::new("entry_or_insert", n), &n, |b, &_n| {
            b.iter_batched(
                || Dict::<u64, u64>::new(),
                |mut d| {
                    for &k in &keys {
                        d.entry(black_box(k))
                            .or_insert(black_box(k.wrapping_mul(2)));
                    }
                    black_box(d)
                },
                criterion::BatchSize::SmallInput,
            );
        });

        group.bench_with_input(BenchmarkId::new("get_then_insert", n), &n, |b, &_n| {
            b.iter_batched(
                || Dict::<u64, u64>::new(),
                |mut d| {
                    for &k in &keys {
                        if d.get(black_box(&k)).is_none() {
                            d.insert(k, black_box(k.wrapping_mul(2)));
                        }
                    }
                    black_box(d)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_counter_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry/counter_pattern");
    const N: usize = 1_000;

    let keys: Vec<u64> = (0..N as u64 * 5).map(|i| i % (N as u64 / 2)).collect();

    group.throughput(Throughput::Elements(keys.len() as u64));

    group.bench_function("entry_and_modify_or_insert", |b| {
        b.iter_batched(
            || Dict::<u64, u64>::new(),
            |mut d| {
                for &k in &keys {
                    d.entry(black_box(k)).and_modify(|v| *v += 1).or_insert(1);
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("get_mut_or_insert", |b| {
        b.iter_batched(
            || Dict::<u64, u64>::new(),
            |mut d| {
                for &k in &keys {
                    if let Some(v) = d.get_mut(black_box(&k)) {
                        *v += 1;
                    } else {
                        d.insert(k, 1);
                    }
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    let keys_collide: Vec<u64> = (0u64..(N as u64 * 5)).map(|i| i % 16).collect();
    group.bench_function("counter_pattern_collisions", |b| {
        b.iter_batched(
            || Dict::<u64, u64>::new(),
            |mut d| {
                for &k in &keys_collide {
                    d.entry(black_box(k)).and_modify(|v| *v += 1).or_insert(1);
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_remove_via_entry(c: &mut Criterion) {
    use zumic::database::dict::entry::Entry;
    let mut group = c.benchmark_group("entry/remove");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("entry_remove", |b| {
        b.iter_batched(
            || {
                let mut d = Dict::<u64, u64>::new();
                for i in 0..N as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    if let Entry::Occupied(e) = d.entry(black_box(i)) {
                        black_box(e.remove());
                    }
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("dict_remove", |b| {
        b.iter_batched(
            || {
                let mut d = Dict::<u64, u64>::new();
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

fn bench_or_default(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry/or_default");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("or_default", |b| {
        b.iter_batched(
            || Dict::<u64, u64>::new(),
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.entry(black_box(i)).or_default());
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("or_insert_default", |b| {
        b.iter_batched(
            || Dict::<u64, u64>::new(),
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.entry(black_box(i)).or_insert(u64::default()));
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_mixed_hit_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("entry/mixed_70hit_30miss");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    let mut d_base = Dict::<u64, u64>::new();
    for i in 0..700u64 {
        d_base.insert(i, i);
    }

    group.bench_function("entry_or_insert", |b| {
        b.iter_batched(
            || d_base.clone(),
            |mut d| {
                for i in 0..N as u64 {
                    let v = d.entry(black_box(i)).or_insert(black_box(i * 2));
                    black_box(v);
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("get_then_insert", |b| {
        b.iter_batched(
            || d_base.clone(),
            |mut d| {
                for i in 0..N as u64 {
                    if let Some(v) = d.get(black_box(&i)) {
                        black_box(v);
                    } else {
                        d.insert(i, black_box(i * 2));
                    }
                }
                black_box(d)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_or_insert_hit,
    bench_or_insert_miss,
    bench_counter_pattern,
    bench_remove_via_entry,
    bench_or_default,
    bench_mixed_hit_miss,
);
criterion_main!(benches);
