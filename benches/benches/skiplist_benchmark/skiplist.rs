use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::SkipList;

fn make_sample_keys(
    n: usize,
    seed: u64,
) -> Vec<i64> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut v = Vec::with_capacity(n);

    for _ in 0..n {
        v.push(rng.gen_range(i64::MIN..=i64::MAX));
    }

    v
}

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("skiplist_insert");
    for &n in &[1_000usize, 10_000, 50_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                // Каждый прогон новый список
                let mut sl: SkipList<i64, i64> = SkipList::new();
                let keys = make_sample_keys(n, 42);
                for k in keys.iter() {
                    sl.insert(*k, *k);
                }
                // предотвратить оптимизацию away
                black_box(sl);
            })
        });
    }
    group.finish();
}

fn bench_search_hit_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("skiplist_search");
    for &n in &[1_000usize, 10_000, 50_000] {
        // Подготовка: заполнить список один раз
        let mut sl: SkipList<i64, i64> = SkipList::new();
        let keys = make_sample_keys(n, 123);
        for k in &keys {
            sl.insert(*k, *k);
        }

        // Сгенерировать набор ключей для успешных и неуспешных поисков
        let hits = keys.clone();
        let misses = make_sample_keys(n, 9999);

        group.bench_with_input(BenchmarkId::new("search_hit", n), &n, |b, &_n| {
            b.iter(|| {
                for k in hits.iter().take(1000) {
                    // batch 1000 per iter
                    black_box(sl.search(&k));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("search_miss", n), &n, |b, &_n| {
            b.iter(|| {
                for k in misses.iter().take(1000) {
                    black_box(sl.search(&k));
                }
            })
        });
    }
    group.finish();
}

fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("skiplist_remove");
    for &n in &[1_000usize, 10_000usize, 50_000usize] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut sl: SkipList<i64, i64> = SkipList::new();
                let keys = make_sample_keys(n, 777);
                for k in &keys {
                    sl.insert(*k, *k);
                }
                // теперь удаляем все
                for k in &keys {
                    black_box(sl.remove(k));
                }
            })
        });
    }
    group.finish();
}

fn bench_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("skiplist_iterate");
    for &n in &[1_000usize, 10_000usize, 50_000usize] {
        let mut sl: SkipList<i64, i64> = SkipList::new();
        let keys = make_sample_keys(n, 2026);
        for k in &keys {
            sl.insert(*k, *k);
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &_n| {
            b.iter(|| {
                // Клонирование/итерирование — в зависимости от API
                for (k, v) in sl.iter() {
                    black_box((k, v));
                }
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_insert,
    bench_search_hit_miss,
    bench_remove,
    bench_iterate
);
criterion_main!(benches);
