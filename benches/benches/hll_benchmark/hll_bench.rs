use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use zumic::{
    database::{HllDefault, MurmurHasher, SipHasher, XxHasher},
    Hll,
};

const N_SPARSE: usize = 100;
const N_DENSE: usize = 50_000;
const N_MERGE: usize = 10_000;

// -------------------------------------------------
// add(): sparse
// -------------------------------------------------
fn bench_add_sparse(c: &mut Criterion) {
    c.bench_function("hll/add_sparse", |b| {
        b.iter_batched(
            || HllDefault::new(),
            |mut hll| {
                for i in 0..N_SPARSE {
                    hll.add(format!("sparse_{i}").as_bytes());
                }
            },
            BatchSize::SmallInput,
        )
    });
}

// ------------------------------------------------------------
// add(): dense
// ------------------------------------------------------------
fn bench_add_dense(c: &mut Criterion) {
    c.bench_function("hll/add_dense", |b| {
        b.iter_batched(
            || {
                let mut hll = Hll::<14>::with_threshold(10);
                // заранее загоняем в dense
                for i in 0..1000 {
                    hll.add(format!("warmup_{i}").as_bytes());
                }
                hll
            },
            |mut hll| {
                for i in 0..N_DENSE {
                    hll.add(format!("dense_{i}").as_bytes());
                }
            },
            BatchSize::SmallInput,
        )
    });
}

// ------------------------------------------------------------
// sparse -> dense conversion cost
// ------------------------------------------------------------
fn bench_sparse_to_dense(c: &mut Criterion) {
    c.bench_function("hll/sparse_to_dense", |b| {
        b.iter(|| {
            let mut hll = Hll::<14>::with_threshold(200);
            for i in 0..1000 {
                hll.add(format!("convert_{i}").as_bytes());
            }
        })
    });
}

// ------------------------------------------------------------
// merge(): dense + dense
// ------------------------------------------------------------
fn bench_merge_dense_dense(c: &mut Criterion) {
    c.bench_function("hll/merge_dense_dense", |b| {
        b.iter_batched(
            || {
                let mut a = Hll::<14>::with_threshold(10);
                let mut b = Hll::<14>::with_threshold(10);

                for i in 0..N_MERGE {
                    a.add(format!("a_{i}").as_bytes());
                    b.add(format!("b_{i}").as_bytes());
                }

                (a, b)
            },
            |(mut a, b)| {
                a.merge(&b);
            },
            BatchSize::SmallInput,
        )
    });
}

// ------------------------------------------------------------
// estimate_cardinality()
// ------------------------------------------------------------
fn bench_estimate(c: &mut Criterion) {
    c.bench_function("hll/estimate", |b| {
        let mut hll = Hll::<14>::with_threshold(10);
        for i in 0..N_DENSE {
            hll.add(format!("item_{i}").as_bytes());
        }

        b.iter(|| {
            let _ = hll.estimate_cardinality();
        });
    });
}

// ------------------------------------------------------------
// hasher comparison (add only)
// ------------------------------------------------------------
fn bench_hashers(c: &mut Criterion) {
    let mut group = c.benchmark_group("hll/add_dense_hashers");

    group.bench_function("murmur", |b| {
        b.iter_batched(
            || Hll::<14, MurmurHasher>::with_threshold(10),
            |mut hll| {
                for i in 0..N_DENSE {
                    hll.add(format!("m_{i}").as_bytes());
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("xxhash", |b| {
        b.iter_batched(
            || Hll::<14, XxHasher>::with_threshold(10),
            |mut hll| {
                for i in 0..N_DENSE {
                    hll.add(format!("x_{i}").as_bytes());
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("siphash", |b| {
        b.iter_batched(
            || Hll::<14, SipHasher>::with_threshold(10),
            |mut hll| {
                for i in 0..N_DENSE {
                    hll.add(format!("s_{i}").as_bytes());
                }
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    hll_benches,
    bench_add_sparse,
    bench_add_dense,
    bench_sparse_to_dense,
    bench_merge_dense_dense,
    bench_estimate,
    bench_hashers,
);

criterion_main!(hll_benches);
