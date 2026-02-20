use std::{collections::hash_map::DefaultHasher, hash::BuildHasherDefault, hint::black_box};

use ahash::RandomState as AHashState;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rustc_hash::FxBuildHasher;
use zumic::Dict;

type DictAHash = Dict<u64, u64, AHashState>;
type DictFx = Dict<u64, u64, FxBuildHasher>;
type DictDefault = Dict<u64, u64, BuildHasherDefault<DefaultHasher>>;

const SIZES: &[usize] = &[100, 1_000, 10_000];

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_insert");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        group.bench_with_input(BenchmarkId::new("ahash", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictAHash::new();
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                d
            });
        });

        group.bench_with_input(BenchmarkId::new("fxhash", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictFx::with_hasher(FxBuildHasher);
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                d
            });
        });

        group.bench_with_input(BenchmarkId::new("default_hasher", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictDefault::with_hasher(BuildHasherDefault::default());
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                d
            });
        });
    }

    group.finish();
}

fn bench_insert_preallocated(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_insert_preallocated");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        group.bench_with_input(BenchmarkId::new("ahash", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictAHash::with_capacity(n);
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });

        group.bench_with_input(BenchmarkId::new("fxhash", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictFx::with_capacity_and_hasher(n, FxBuildHasher);
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });

        group.bench_with_input(BenchmarkId::new("default_hasher", n), &n, |b, &n| {
            b.iter(|| {
                let mut d = DictDefault::with_capacity_and_hasher(n, BuildHasherDefault::default());
                for i in 0..n as u64 {
                    d.insert(black_box(i), black_box(i));
                }
                black_box(d)
            });
        });
    }

    group.finish();
}

fn bench_get_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_get_hit");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        let mut ahash_d = DictAHash::new();
        let mut fx_d = DictFx::with_hasher(FxBuildHasher);
        let mut def_d = DictDefault::with_hasher(BuildHasherDefault::default());
        for i in 0..n as u64 {
            ahash_d.insert(i, i);
            fx_d.insert(i, i);
            def_d.insert(i, i);
        }

        group.bench_with_input(BenchmarkId::new("ahash", n), &n, |b, &n| {
            b.iter(|| {
                for i in 0..n as u64 {
                    black_box(ahash_d.get(black_box(&i)));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("fxhash", n), &n, |b, &n| {
            b.iter(|| {
                for i in 0..n as u64 {
                    black_box(fx_d.get(black_box(&i)));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("default_hasher", n), &n, |b, &n| {
            b.iter(|| {
                for i in 0..n as u64 {
                    black_box(def_d.get(black_box(&i)));
                }
            });
        });
    }

    group.finish();
}

fn bench_get_hit_random(c: &mut Criterion) {
    use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

    let mut group = c.benchmark_group("dict_get_hit_random");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        let mut keys: Vec<u64> = (0..n as u64).collect();
        let mut rng = StdRng::seed_from_u64(42);
        keys.shuffle(&mut rng);

        let mut ahash_d = DictAHash::with_capacity(n);
        let mut fx_d = DictFx::with_capacity_and_hasher(n, FxBuildHasher);

        for &k in &keys {
            ahash_d.insert(k, k);
            fx_d.insert(k, k);
        }

        group.bench_function(&format!("ahash/{}", n), |b| {
            b.iter(|| {
                for k in &keys {
                    black_box(ahash_d.get(black_box(k)));
                }
            });
        });

        group.bench_function(&format!("fxhash/{}", n), |b| {
            b.iter(|| {
                for k in &keys {
                    black_box(fx_d.get(black_box(k)));
                }
            });
        });
    }

    group.finish();
}

fn bench_get_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_get_miss");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    let mut ahash_d = DictAHash::new();
    let mut fx_d = DictFx::with_hasher(FxBuildHasher);
    let mut def_d = DictDefault::with_hasher(BuildHasherDefault::default());
    for i in 0..N as u64 {
        ahash_d.insert(i, i);
        fx_d.insert(i, i);
        def_d.insert(i, i);
    }

    group.bench_function("ahash", |b| {
        b.iter(|| {
            for i in N as u64..2 * N as u64 {
                black_box(ahash_d.get(black_box(&i)));
            }
        });
    });

    group.bench_function("fxhash", |b| {
        b.iter(|| {
            for i in N as u64..2 * N as u64 {
                black_box(fx_d.get(black_box(&i)));
            }
        });
    });

    group.bench_function("default_hasher", |b| {
        b.iter(|| {
            for i in N as u64..2 * N as u64 {
                black_box(def_d.get(black_box(&i)));
            }
        });
    });

    group.finish();
}

fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_remove");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("ahash", |b| {
        b.iter_batched(
            || {
                let mut d = DictAHash::new();
                for i in 0..N as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.remove(black_box(&i)));
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("fxhash", |b| {
        b.iter_batched(
            || {
                let mut d = DictFx::with_hasher(FxBuildHasher);
                for i in 0..N as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.remove(black_box(&i)));
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("default_hasher", |b| {
        b.iter_batched(
            || {
                let mut d = DictDefault::with_hasher(BuildHasherDefault::default());
                for i in 0..N as u64 {
                    d.insert(i, i);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    black_box(d.remove(black_box(&i)));
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_iter");

    for &n in SIZES {
        group.throughput(Throughput::Elements(n as u64));

        let mut d = DictAHash::with_capacity(n);
        for i in 0..n as u64 {
            d.insert(i, i);
        }

        group.bench_function(&format!("ahash/{}", n), |b| {
            b.iter(|| {
                for (k, v) in d.iter() {
                    black_box((k, v));
                }
            });
        });
    }

    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("dict_mixed_80r_20w");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    let base: Vec<u64> = (0..N as u64 / 2).collect();

    group.bench_function("ahash", |b| {
        b.iter_batched(
            || {
                let mut d = DictAHash::new();
                for &k in &base {
                    d.insert(k, k);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    if i % 5 == 0 {
                        d.insert(black_box(i + N as u64), black_box(i));
                    } else {
                        black_box(d.get(black_box(&(i % (N as u64 / 2)))));
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("fxhash", |b| {
        b.iter_batched(
            || {
                let mut d = DictFx::with_hasher(FxBuildHasher);
                for &k in &base {
                    d.insert(k, k);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    if i % 5 == 0 {
                        d.insert(black_box(i + N as u64), black_box(i));
                    } else {
                        black_box(d.get(black_box(&(i % (N as u64 / 2)))));
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("default_hasher", |b| {
        b.iter_batched(
            || {
                let mut d = DictDefault::with_hasher(BuildHasherDefault::default());
                for &k in &base {
                    d.insert(k, k);
                }
                d
            },
            |mut d| {
                for i in 0..N as u64 {
                    if i % 5 == 0 {
                        d.insert(black_box(i + N as u64), black_box(i));
                    } else {
                        black_box(d.get(black_box(&(i % (N as u64 / 2)))));
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_vs_std_hashmap(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("dict_vs_hashmap_insert_1000");
    const N: usize = 1_000;
    group.throughput(Throughput::Elements(N as u64));

    group.bench_function("Dict<ahash>", |b| {
        b.iter(|| {
            let mut d = DictAHash::new();
            for i in 0..N as u64 {
                d.insert(black_box(i), black_box(i));
            }
            d
        });
    });

    group.bench_function("HashMap<ahash>", |b| {
        b.iter(|| {
            let mut m: HashMap<u64, u64, AHashState> = HashMap::with_hasher(AHashState::new());
            for i in 0..N as u64 {
                m.insert(black_box(i), black_box(i));
            }
            m
        });
    });

    group.bench_function("HashMap<ahash>/preallocated", |b| {
        b.iter(|| {
            let mut m: HashMap<u64, u64, AHashState> =
                HashMap::with_capacity_and_hasher(N, AHashState::new());
            for i in 0..N as u64 {
                m.insert(black_box(i), black_box(i));
            }
            black_box(m)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_insert,
    bench_insert_preallocated,
    bench_get_hit,
    bench_get_hit_random,
    bench_get_miss,
    bench_remove,
    bench_iter,
    bench_mixed_workload,
    bench_vs_std_hashmap,
);
criterion_main!(benches);
