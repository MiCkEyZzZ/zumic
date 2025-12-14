use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use zumic::ListPack;

/// Benchmark для удаления первого элемента (должен быть O(1))
fn bench_remove_first(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_first_pure");

    for &size in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            let mut lp = ListPack::new();
            for i in 0..n {
                lp.push_back(&[i as u8]);
            }

            b.iter(|| {
                let mut lp_copy = lp.clone();
                lp_copy.remove(black_box(0));
                black_box(lp_copy);
            });
        });
    }
}

/// Benchmark для удаления последнего элемента
fn bench_remove_last(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_last");

    for &size in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut lp = ListPack::new();
                for i in 0..n {
                    lp.push_back(&[i as u8]);
                }

                // Удаляем последний элемент
                lp.remove(black_box(n - 1));
                black_box(lp);
            });
        });
    }

    group.finish();
}

/// Benchmark для удаления из начала списка (left-biased)
fn bench_remove_near_start(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_near_start");

    for &size in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut lp = ListPack::new();
                for i in 0..n {
                    lp.push_back(&[i as u8]);
                }

                // Удаляем элемент близко к началу (10% от начала)
                let idx = n / 10;
                lp.remove(black_box(idx));
                black_box(lp);
            });
        });
    }

    group.finish();
}

/// Benchmark для удаления из конца списка (right-biased)
fn bench_remove_near_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_near_end");

    for &size in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut lp = ListPack::new();
                for i in 0..n {
                    lp.push_back(&[i as u8]);
                }

                // Удаляем элемент близко к концу (90% от начала)
                let idx = (n * 9) / 10;
                lp.remove(black_box(idx));
                black_box(lp);
            });
        });
    }

    group.finish();
}

/// Benchmark для удаления из середины списка
fn bench_remove_middle(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_middle");

    for &size in &[100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &n| {
            b.iter(|| {
                let mut lp = ListPack::new();
                for i in 0..n {
                    lp.push_back(&[i as u8]);
                }

                // Удаляем элемент из середины
                let idx = n / 2;
                lp.remove(black_box(idx));
                black_box(lp);
            });
        });
    }

    group.finish();
}

/// Benchmark для множественных последовательных удалений
fn bench_remove_sequential_pattern(c: &mut Criterion) {
    c.bench_function("remove_sequential_every_5th_1000", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            let n = 1000;

            for i in 0..n {
                // Явно указываем тип i, например u32
                let val: u32 = i as u32;
                lp.push_back(&val.to_le_bytes());
            }

            // Удаляем каждый пятый элемент
            let mut i = 0;
            while i < lp.len() {
                if i % 5 == 0 {
                    lp.remove(i);
                } else {
                    i += 1;
                }
            }

            black_box(lp);
        });
    });
}

/// Benchmark для worst-case scenario — удаление всех элементов с начала
fn bench_remove_all_from_start(c: &mut Criterion) {
    c.bench_function("remove_all_from_start_500", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0..500 {
                lp.push_back(&[i as u8]);
            }

            // Удаляем все элементы с начала
            while lp.len() > 0 {
                lp.remove(0);
            }

            black_box(lp);
        });
    });
}

/// Benchmark для best-case scenario — удаление всех элементов с конца
fn bench_remove_all_from_end(c: &mut Criterion) {
    c.bench_function("remove_all_from_end_500", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0..500 {
                lp.push_back(&[i as u8]);
            }

            // Удаляем все элементы с конца
            while lp.len() > 0 {
                lp.remove(lp.len() - 1);
            }

            black_box(lp);
        });
    });
}

criterion_group!(
    remove_benchmarks,
    bench_remove_first,
    bench_remove_last,
    bench_remove_near_start,
    bench_remove_near_end,
    bench_remove_middle,
    bench_remove_sequential_pattern,
    bench_remove_all_from_start,
    bench_remove_all_from_end,
);

criterion_main!(remove_benchmarks);
