use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::IntSet;

fn create_small_set() -> IntSet {
    let mut set = IntSet::new();
    for i in 0..100 {
        set.insert(i);
    }
    set
}

fn create_medium_set() -> IntSet {
    let mut set = IntSet::new();
    for i in 0..10_000 {
        set.insert(i);
    }
    set
}

fn create_i32_set() -> IntSet {
    let mut set = IntSet::new();
    let base = i16::MAX as i64 + 1000;
    for i in 0..10_000 {
        set.insert(base + i);
    }
    set
}

fn create_i64_set() -> IntSet {
    let mut set = IntSet::new();
    let base = i32::MAX as i64 + 1000;
    for i in 0..10_000 {
        set.insert(base + i);
    }
    set
}

fn bench_iter_next(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_next");

    // i16 encoding
    let set = create_small_set();
    group.bench_function("i16", |b| {
        b.iter(|| {
            let mut iter = set.iter();
            black_box(iter.next());
        });
    });

    // i32 encoding
    let set = create_i32_set();
    group.bench_function("i32", |b| {
        b.iter(|| {
            let mut iter = set.iter();
            black_box(iter.next());
        });
    });

    // i64 encoding
    let set = create_i64_set();
    group.bench_function("i64", |b| {
        b.iter(|| {
            let mut iter = set.iter();
            black_box(iter.next());
        });
    });

    group.finish();
}

fn bench_iter_full_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_full_scan");
    group.measurement_time(Duration::from_secs(10));

    for size in [100, 1_000, 10_000, 100_000].iter() {
        let mut set = IntSet::new();
        for i in 0..*size {
            set.insert(i);
        }

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let sum: i64 = set.iter().sum();
                black_box(sum);
            });
        });
    }

    group.finish();
}

fn bench_iter_vs_old_implementation(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_comparison");
    let set = create_medium_set();

    // Новая zero-copy реализация
    group.bench_function("zero_copy", |b| {
        b.iter(|| {
            let sum: i64 = set.iter().sum();
            black_box(sum);
        });
    });

    // Старая реализация (с клонированием) для сравнения
    group.bench_function("old_clone_based", |b| {
        b.iter(|| {
            // Эмуляция старой реализации: collect в Vec + iter
            let values: Vec<i64> = set.iter().collect();
            let sum: i64 = values.iter().sum();
            black_box(sum);
        });
    });

    group.finish();
}

fn bench_rev_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("rev_iter");
    let set = create_medium_set();

    group.bench_function("forward", |b| {
        b.iter(|| {
            let sum: i64 = set.iter().sum();
            black_box(sum);
        });
    });

    group.bench_function("reverse", |b| {
        b.iter(|| {
            let sum: i64 = set.rev_iter().sum();
            black_box(sum);
        });
    });

    group.finish();
}

fn bench_rev_iter_next(c: &mut Criterion) {
    let set = create_small_set();

    c.bench_function("rev_iter_next", |b| {
        b.iter(|| {
            let mut iter = set.rev_iter();
            black_box(iter.next());
        });
    });
}

fn bench_iter_range_setup(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_range_setup");
    let set = create_medium_set();

    // Разные размеры диапазонов
    for range_size in [10, 100, 1_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(range_size),
            range_size,
            |b, &size| {
                b.iter(|| {
                    let iter = set.iter_range(0, size);
                    black_box(iter);
                });
            },
        );
    }

    group.finish();
}

fn bench_iter_range_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_range_scan");
    let set = create_medium_set();

    for range_size in [10, 100, 1_000, 5_000].iter() {
        group.throughput(Throughput::Elements(*range_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(range_size),
            range_size,
            |b, &size| {
                b.iter(|| {
                    let sum: i64 = set.iter_range(0, size).sum();
                    black_box(sum);
                });
            },
        );
    }

    group.finish();
}

fn bench_iter_range_positions(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_range_positions");
    let set = create_medium_set();

    // Range в начале
    group.bench_function("start", |b| {
        b.iter(|| {
            let sum: i64 = set.iter_range(0, 1000).sum();
            black_box(sum);
        });
    });

    // Range в середине
    group.bench_function("middle", |b| {
        b.iter(|| {
            let sum: i64 = set.iter_range(4500, 5500).sum();
            black_box(sum);
        });
    });

    // Range в конце
    group.bench_function("end", |b| {
        b.iter(|| {
            let sum: i64 = set.iter_range(9000, 10000).sum();
            black_box(sum);
        });
    });

    group.finish();
}

fn bench_iter_range_reverse(c: &mut Criterion) {
    let set = create_medium_set();

    c.bench_function("iter_range_reverse", |b| {
        b.iter(|| {
            let sum: i64 = set.iter_range(0, 1000).rev().sum();
            black_box(sum);
        });
    });
}

fn bench_iter_len(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_exact_size");

    for size in [100, 10_000, 100_000].iter() {
        let mut set = IntSet::new();
        for i in 0..*size {
            set.insert(i);
        }

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let iter = set.iter();
                black_box(iter.len());
            });
        });
    }

    group.finish();
}

fn bench_iter_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("iter_allocations");
    group.measurement_time(Duration::from_secs(5));

    let set = create_medium_set();

    // Проверяем, что нет аллокаций при итерации
    group.bench_function("full_iteration", |b| {
        b.iter(|| {
            // В идеале здесь должен быть allocation profiler
            // Но можем хотя бы измерить общее время
            for value in set.iter() {
                black_box(value);
            }
        });
    });

    // Сравнение с Vec (который требует allocation)
    group.bench_function("vec_clone_baseline", |b| {
        b.iter(|| {
            let vec: Vec<i64> = set.iter().collect();
            for value in vec {
                black_box(value);
            }
        });
    });

    group.finish();
}

fn bench_comparison_btreeset(c: &mut Criterion) {
    use std::collections::BTreeSet;

    let mut group = c.benchmark_group("comparison_btreeset");
    group.measurement_time(Duration::from_secs(10));

    let size = 10_000;

    // IntSet iteration
    let mut intset = IntSet::new();
    for i in 0..size {
        intset.insert(i);
    }

    group.throughput(Throughput::Elements(size as u64));
    group.bench_function("intset", |b| {
        b.iter(|| {
            let sum: i64 = intset.iter().sum();
            black_box(sum);
        });
    });

    // BTreeSet iteration
    let mut btree = BTreeSet::new();
    for i in 0..size {
        btree.insert(i);
    }

    group.bench_function("btreeset", |b| {
        b.iter(|| {
            let sum: i64 = btree.iter().sum();
            black_box(sum);
        });
    });

    group.finish();
}

fn bench_comparison_vec(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison_vec");

    let size = 10_000;

    // IntSet
    let mut intset = IntSet::new();
    for i in 0..size {
        intset.insert(i);
    }

    group.throughput(Throughput::Elements(size as u64));
    group.bench_function("intset", |b| {
        b.iter(|| {
            let sum: i64 = intset.iter().sum();
            black_box(sum);
        });
    });

    // Plain sorted Vec<i64>
    let vec: Vec<i64> = (0..size).collect();

    group.bench_function("vec_i64", |b| {
        b.iter(|| {
            let sum: i64 = vec.iter().sum();
            black_box(sum);
        });
    });

    group.finish();
}

fn bench_empty_set(c: &mut Criterion) {
    let set = IntSet::new();

    c.bench_function("iter_empty_set", |b| {
        b.iter(|| {
            let count = set.iter().count();
            black_box(count);
        });
    });
}

fn bench_single_element(c: &mut Criterion) {
    let mut set = IntSet::new();
    set.insert(42);

    c.bench_function("iter_single_element", |b| {
        b.iter(|| {
            let sum: i64 = set.iter().sum();
            black_box(sum);
        });
    });
}

fn bench_range_empty_result(c: &mut Criterion) {
    let set = create_medium_set();

    c.bench_function("iter_range_empty", |b| {
        b.iter(|| {
            // Range за пределами данных
            let count = set.iter_range(20_000, 30_000).count();
            black_box(count);
        });
    });
}

criterion_group!(
    benches,
    bench_iter_next,
    bench_iter_full_scan,
    bench_iter_vs_old_implementation,
    bench_rev_iter,
    bench_rev_iter_next,
    bench_iter_range_setup,
    bench_iter_range_scan,
    bench_iter_range_positions,
    bench_iter_range_reverse,
    bench_iter_len,
    bench_iter_allocations,
    bench_comparison_btreeset,
    bench_comparison_vec,
    bench_empty_set,
    bench_single_element,
    bench_range_empty_result,
);

criterion_main!(benches);
