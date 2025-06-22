use std::{collections::HashMap, hint::black_box};

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::Dict;

fn bench_insert(c: &mut Criterion) {
    c.bench_function("insert 10_000 (Dict)", |b| {
        b.iter(|| {
            let mut d = Dict::new();
            for i in 0..10_000 {
                d.insert(black_box(i), black_box(i));
            }
        });
    });

    c.bench_function("insert 10_000 (HashMap)", |b| {
        b.iter(|| {
            let mut d = HashMap::new();
            for i in 0..10_000 {
                d.insert(black_box(i), black_box(i));
            }
        });
    });
}

fn bench_get(c: &mut Criterion) {
    let mut dict = Dict::new();
    for i in 0..10_000 {
        dict.insert(i, i);
    }

    let mut hashmap = HashMap::new();
    for i in 0..10_000 {
        hashmap.insert(i, i);
    }

    c.bench_function("get 10_000 (Dict)", |b| {
        b.iter(|| {
            for i in 0..10_000 {
                black_box(dict.get(&i));
            }
        });
    });

    c.bench_function("get 10_000 (HashMap)", |b| {
        b.iter(|| {
            for i in 0..10_000 {
                black_box(hashmap.get(&i));
            }
        });
    });
}

fn bench_remove(c: &mut Criterion) {
    c.bench_function("insert + remove 10_000 (Dict)", |b| {
        b.iter(|| {
            let mut d = Dict::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            for i in 0..10_000 {
                d.remove(&i);
            }
        });
    });

    c.bench_function("insert + remove 10_000 (HashMap)", |b| {
        b.iter(|| {
            let mut d = HashMap::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            for i in 0..10_000 {
                d.remove(&i);
            }
        });
    });
}

fn bench_iter(c: &mut Criterion) {
    let mut dict = Dict::new();
    for i in 0..10_000 {
        dict.insert(i, i);
    }

    let mut hashmap = HashMap::new();
    for i in 0..10_000 {
        hashmap.insert(i, i);
    }

    c.bench_function("iteration over 10_000 (Dict)", |b| {
        b.iter(|| {
            for (_k, _v) in dict.iter() {
                black_box((_k, _v));
            }
        });
    });

    c.bench_function("iteration over 10_000 (HashMap)", |b| {
        b.iter(|| {
            for (_k, _v) in hashmap.iter() {
                black_box((_k, _v));
            }
        });
    });
}

fn bench_clear(c: &mut Criterion) {
    c.bench_function("clear after insert 10_000 (Dict)", |b| {
        b.iter(|| {
            let mut d = Dict::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            d.clear();
        });
    });

    c.bench_function("clear after insert 10_000 (HashMap)", |b| {
        b.iter(|| {
            let mut d = HashMap::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            d.clear();
        });
    });
}

fn bench_reuse(c: &mut Criterion) {
    c.bench_function("reuse after clear (Dict)", |b| {
        b.iter(|| {
            let mut d = Dict::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            d.clear();
            for i in 0..10_000 {
                d.insert(i, i);
            }
        });
    });

    c.bench_function("reuse after clear (HashMap)", |b| {
        b.iter(|| {
            let mut d = HashMap::new();
            for i in 0..10_000 {
                d.insert(i, i);
            }
            d.clear();
            for i in 0..10_000 {
                d.insert(i, i);
            }
        });
    });
}

criterion_group!(
    dict_benches,
    bench_insert,
    bench_get,
    bench_remove,
    bench_iter,
    bench_clear,
    bench_reuse
);
criterion_main!(dict_benches);
