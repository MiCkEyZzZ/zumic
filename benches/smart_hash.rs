use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use zumic::{ArcBytes, SmartHash};

fn prepare_kv_pairs(n: usize) -> Vec<(ArcBytes, ArcBytes)> {
    (0..n)
        .map(|i| {
            (
                ArcBytes::from(i.to_string().as_bytes()),
                ArcBytes::from_str("value"),
            )
        })
        .collect()
}

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("SmartHash Insert");

    for &size in [8, 32, 64, 128, 1024].iter() {
        let data = prepare_kv_pairs(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter_batched(
                || SmartHash::new(),
                |mut sh| {
                    for (k, v) in data {
                        sh.insert(k.clone(), v.clone());
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("SmartHash Get");

    for &size in [8, 32, 64, 128, 1024].iter() {
        let data = prepare_kv_pairs(size);
        let mut sh = SmartHash::new();
        for (k, v) in &data {
            sh.insert(k.clone(), v.clone());
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), &sh, |b, sh| {
            let keys: Vec<_> = sh.keys();
            b.iter(|| {
                for key in &keys {
                    let _ = sh.clone().get(key); // клонируем для иммутабельности
                }
            });
        });
    }

    group.finish();
}

fn bench_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("SmartHash Delete");

    for &size in [8, 32, 64, 128, 1024].iter() {
        let data = prepare_kv_pairs(size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter_batched(
                || {
                    let mut sh = SmartHash::new();
                    for (k, v) in data {
                        sh.insert(k.clone(), v.clone());
                    }
                    sh
                },
                |mut sh| {
                    for (k, _) in data {
                        let _ = sh.remove(k);
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("SmartHash Iter");

    for &size in [8, 32, 64, 128, 1024].iter() {
        let data = prepare_kv_pairs(size);
        let mut sh = SmartHash::new();
        for (k, v) in &data {
            sh.insert(k.clone(), v.clone());
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), &sh, |b, sh| {
            b.iter(|| {
                for (k, v) in sh.clone().iter() {
                    criterion::black_box((k, v));
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_insert, bench_get, bench_delete, bench_iter);
criterion_main!(benches);
