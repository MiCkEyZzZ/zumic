use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::database::bitmap::Bitmap; // поправь путь под свой crate

fn bench_set_bit(c: &mut Criterion) {
    let mut bitmap = Bitmap::with_capacity(10_000);
    c.bench_function("set_bit 10k", |b| {
        b.iter(|| {
            for i in 0..10_000 {
                bitmap.set_bit(i, true);
            }
        })
    });
}

fn bench_get_bit(c: &mut Criterion) {
    let mut bitmap = Bitmap::with_capacity(10_000);
    for i in 0..10_000 {
        bitmap.set_bit(i, i.is_multiple_of(2));
    }

    c.bench_function("get_bit 10k", |b| {
        b.iter(|| {
            let mut count = 0;
            for i in 0..10_000 {
                if bitmap.get_bit(i) {
                    count += 1;
                }
            }
            black_box(count);
        })
    });
}

fn bench_bitcount(c: &mut Criterion) {
    let mut bitmap = Bitmap::with_capacity(1_000_000);
    for i in (0..1_000_000).step_by(3) {
        bitmap.set_bit(i, true);
    }

    c.bench_function("bitcount 1M", |b| {
        b.iter(|| {
            let count = bitmap.bitcount(0, 1_000_000);
            black_box(count);
        })
    });
}

fn bench_bitop_or(c: &mut Criterion) {
    let mut a = Bitmap::with_capacity(100_000);
    let mut b = Bitmap::with_capacity(100_000);
    for i in (0..100_000).step_by(2) {
        a.set_bit(i, true);
    }
    for i in (1..100_000).step_by(2) {
        b.set_bit(i, true);
    }

    c.bench_function("bitwise or 100k", |bch| {
        bch.iter(|| {
            let _ = &a | &b;
        })
    });
}

criterion_group!(
    benches,
    bench_set_bit,
    bench_get_bit,
    bench_bitcount,
    bench_bitop_or
);
criterion_main!(benches);
