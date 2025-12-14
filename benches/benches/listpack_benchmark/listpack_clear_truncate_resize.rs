use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::ListPack;

fn bench_listpack_clear(c: &mut Criterion) {
    c.bench_function("listpack_clear_10000", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0u16..10_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.clear();
            black_box(&lp);
        });
    });
}

fn bench_listpack_truncate_half(c: &mut Criterion) {
    c.bench_function("listpack_truncate_half_10000_to_5000", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0u16..10_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.truncate(5_000);
            black_box(&lp);
        });
    });
}

fn bench_listpack_truncate_to_zero(c: &mut Criterion) {
    c.bench_function("listpack_truncate_10000_to_0", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0u16..10_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.truncate(0);
            black_box(&lp);
        });
    });
}

fn bench_listpack_resize_grow(c: &mut Criterion) {
    c.bench_function("listpack_resize_grow_0_to_10000", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();
            lp.resize(10_000, b"x");
            black_box(&lp);
        });
    });
}

fn bench_listpack_resize_shrink(c: &mut Criterion) {
    c.bench_function("listpack_resize_shrink_10000_to_100", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0u16..10_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.resize(100, b"x");
            black_box(&lp);
        });
    });
}

fn bench_listpack_resize_noop(c: &mut Criterion) {
    c.bench_function("listpack_resize_noop_1000", |b| {
        b.iter(|| {
            let mut lp = ListPack::new();

            for i in 0u16..1_000 {
                lp.push_back(&i.to_le_bytes());
            }

            lp.resize(1_000, b"x");
            black_box(&lp);
        });
    });
}

criterion_group!(
    listpack_clear_truncate_resize,
    bench_listpack_clear,
    bench_listpack_truncate_half,
    bench_listpack_truncate_to_zero,
    bench_listpack_resize_grow,
    bench_listpack_resize_shrink,
    bench_listpack_resize_noop
);

criterion_main!(listpack_clear_truncate_resize);
