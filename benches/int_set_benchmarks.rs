use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::database::int_set::IntSet;

fn bench_insert_small(c: &mut Criterion) {
    let input: Vec<i64> = (0..1000).collect();

    c.bench_function("insert small (i16 range)", |b| {
        b.iter(|| {
            let mut set = black_box(IntSet::new());
            for &val in &input {
                set.insert(black_box(val));
            }
        });
    });
}

fn bench_insert_large(c: &mut Criterion) {
    let input: Vec<i64> = (0..1000).map(|x| x as i64 * 1_000_000_000).collect();

    c.bench_function("insert large (i64 range)", |b| {
        b.iter(|| {
            let mut set = black_box(IntSet::new());
            for &val in &input {
                set.insert(black_box(val));
            }
        });
    });
}

fn bench_contains_hit(c: &mut Criterion) {
    let mut set = IntSet::new();
    for i in 0..10_000 {
        set.insert(i);
    }

    let set = black_box(set);

    c.bench_function("contains hit", |b| {
        b.iter(|| {
            black_box(set.contains(5000));
        });
    });
}

fn bench_contains_miss(c: &mut Criterion) {
    let mut set = IntSet::new();
    for i in 0..10_000 {
        set.insert(i);
    }

    let set = black_box(set);

    c.bench_function("contains miss", |b| {
        b.iter(|| {
            black_box(set.contains(20_000));
        });
    });
}

fn bench_remove(c: &mut Criterion) {
    let input: Vec<i64> = (0..1000).collect();

    c.bench_function("remove", |b| {
        b.iter(|| {
            let mut set = IntSet::new();
            for &val in &input {
                set.insert(val);
            }
            for &val in &input {
                set.remove(black_box(val));
            }
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(500);
    targets =
        bench_insert_small,
        bench_insert_large,
        bench_contains_hit,
        bench_contains_miss,
        bench_remove
);
criterion_main!(benches);
