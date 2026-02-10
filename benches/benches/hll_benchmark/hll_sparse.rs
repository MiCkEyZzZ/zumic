use std::hint::black_box;

use criterion::Criterion;
use zumic::database::HllSparse;

fn bench_sparse_set_get(c: &mut Criterion) {
    const THRESHOLD: usize = 3000;
    let mut sparse = HllSparse::<14>::with_threshold(THRESHOLD);

    c.bench_function("sparse/set_register", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(sparse.set_register(i, 1));
            }
        })
    });

    c.bench_function("sparse/get_register", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(sparse.get_register(i));
            }
        })
    });
}

fn bench_sparse_merge(c: &mut Criterion) {
    let mut s1 = HllSparse::<14>::new();
    let mut s2 = HllSparse::<14>::new();

    for i in 0..500 {
        s1.set_register(i, 1);
        s2.set_register(i + 250, 1);
    }

    c.bench_function("sparse/merge", |b| b.iter(|| black_box(s1.merge(&s2))));
}

criterion::criterion_group!(benches, bench_sparse_set_get, bench_sparse_merge);
criterion::criterion_main!(benches);
