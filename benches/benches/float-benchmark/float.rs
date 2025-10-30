use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_float_add(c: &mut Criterion) {
    c.bench_function("float_add", |b| {
        b.iter(|| {
            let x = black_box(1.2345f64) + black_box(6.7890f64);
            black_box(x);
        })
    });
}

criterion_group!(benches, bench_float_add);
criterion_main!(benches);
