use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use zumic::Sds;

const I64_CASES: &[(&str, i64)] = &[
    ("zero", 0),
    ("one", 1),
    ("minus_one", -1),
    ("small_pos", 42),
    ("small_neg", -42),
    ("thousand", 1_000),
    ("million", 1_000_000),
    ("i64_max", i64::MAX),
    ("i64_min", i64::MIN),
];

const U64_CASES: &[(&str, u64)] = &[
    ("zero", 0),
    ("one", 1),
    ("small", 42),
    ("million", 1_000_000),
    ("u64_max", u64::MAX),
];

const F64_CASES: &[(&str, f64)] = &[
    ("zero", 0.0),
    ("one", 1.0),
    ("minus_one", -1.0),
    ("pi", 3.14),
    ("large", 1e10),
    ("tiny", -1e-5),
    ("precise", 1.23456789012345),
];

fn bench_from_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/from_i64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    for (name, n) in I64_CASES {
        group.bench_with_input(BenchmarkId::new("Sds::from_i64", name), n, |b, &n| {
            b.iter(|| black_box(Sds::from_i64(black_box(n))))
        });

        group.bench_with_input(
            BenchmarkId::new("to_string_then_from_str", name),
            n,
            |b, &n| {
                b.iter(|| {
                    let s = n.to_string();
                    black_box(Sds::from_str(black_box(&s)))
                })
            },
        );
    }

    group.finish();
}

fn bench_from_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/from_u64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    for (name, n) in U64_CASES {
        group.bench_with_input(BenchmarkId::new("Sds::from_u64", name), n, |b, &n| {
            b.iter(|| black_box(Sds::from_u64(black_box(n))))
        });

        group.bench_with_input(
            BenchmarkId::new("to_string_then_from_str", name),
            n,
            |b, &n| {
                b.iter(|| {
                    let s = n.to_string();
                    black_box(Sds::from_str(black_box(&s)))
                })
            },
        );
    }

    group.finish();
}

fn bench_from_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/from_f64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    for (name, n) in F64_CASES {
        group.bench_with_input(BenchmarkId::new("Sds::from_f64", name), n, |b, &n| {
            b.iter(|| black_box(Sds::from_f64(black_box(n))))
        });

        group.bench_with_input(
            BenchmarkId::new("to_string_then_from_str", name),
            n,
            |b, &n| {
                b.iter(|| {
                    let s = n.to_string();
                    black_box(Sds::from_str(black_box(&s)))
                })
            },
        );
    }

    group.finish();
}

fn bench_to_i64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/to_i64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    let values: Vec<(String, Sds)> = I64_CASES
        .iter()
        .map(|(name, n)| ((*name).to_string(), Sds::from_i64(*n)))
        .collect();

    for (name, sds) in &values {
        group.bench_with_input(BenchmarkId::new("Sds::to_i64", name), sds, |b, s| {
            b.iter(|| black_box(s.to_i64().unwrap()))
        });

        group.bench_with_input(BenchmarkId::new("borrowed_str_parse", name), sds, |b, s| {
            b.iter(|| {
                let n = std::str::from_utf8(s.as_slice())
                    .unwrap()
                    .parse::<i64>()
                    .unwrap();
                black_box(n)
            })
        });
    }

    group.finish();
}

fn bench_to_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/to_u64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    let values: Vec<(String, Sds)> = U64_CASES
        .iter()
        .map(|(name, n)| ((*name).to_string(), Sds::from_u64(*n)))
        .collect();

    for (name, sds) in &values {
        group.bench_with_input(BenchmarkId::new("Sds::to_u64", name), sds, |b, s| {
            b.iter(|| black_box(s.to_u64().unwrap()))
        });

        group.bench_with_input(BenchmarkId::new("borrowed_str_parse", name), sds, |b, s| {
            b.iter(|| {
                let n = std::str::from_utf8(s.as_slice())
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                black_box(n)
            })
        });
    }

    group.finish();
}

fn bench_to_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/to_f64");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    let values: Vec<(String, Sds)> = F64_CASES
        .iter()
        .map(|(name, n)| ((*name).to_string(), Sds::from_f64(*n)))
        .collect();

    for (name, sds) in &values {
        group.bench_with_input(BenchmarkId::new("Sds::to_f64", name), sds, |b, s| {
            b.iter(|| black_box(s.to_f64().unwrap()))
        });

        group.bench_with_input(BenchmarkId::new("borrowed_str_parse", name), sds, |b, s| {
            b.iter(|| {
                let n = std::str::from_utf8(s.as_slice())
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                black_box(n)
            })
        });
    }

    group.finish();
}

fn bench_is_integer(c: &mut Criterion) {
    let mut group = c.benchmark_group("numeric/is_integer");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(100);

    let valid: Vec<(String, Sds)> = I64_CASES
        .iter()
        .map(|(name, n)| ((*name).to_string(), Sds::from_i64(*n)))
        .collect();

    let mixed: Vec<(String, Sds)> = vec![
        ("int".into(), Sds::from_str("42")),
        ("float".into(), Sds::from_str("3.14")),
        ("neg".into(), Sds::from_str("-100")),
        ("alpha".into(), Sds::from_str("abc")),
        ("exp".into(), Sds::from_str("1e10")),
        ("zero".into(), Sds::from_str("0")),
        ("plus".into(), Sds::from_str("+7")),
        ("empty_sign".into(), Sds::from_str("-")),
    ];

    for (name, sds) in &valid {
        group.bench_with_input(BenchmarkId::new("Sds::is_integer", name), sds, |b, s| {
            b.iter(|| black_box(s.is_integer()))
        });

        group.bench_with_input(
            BenchmarkId::new("borrowed_parse_is_ok", name),
            sds,
            |b, s| {
                b.iter(|| {
                    let ok = std::str::from_utf8(s.as_slice())
                        .ok()
                        .and_then(|x| x.parse::<i64>().ok())
                        .is_some();
                    black_box(ok)
                })
            },
        );
    }

    for (name, sds) in &mixed {
        group.bench_with_input(
            BenchmarkId::new("Sds::is_integer_mixed", name),
            sds,
            |b, s| b.iter(|| black_box(s.is_integer())),
        );

        group.bench_with_input(
            BenchmarkId::new("borrowed_parse_is_ok_mixed", name),
            sds,
            |b, s| {
                b.iter(|| {
                    let ok = std::str::from_utf8(s.as_slice())
                        .ok()
                        .and_then(|x| x.parse::<i64>().ok())
                        .is_some();
                    black_box(ok)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_from_i64,
    bench_from_u64,
    bench_from_f64,
    bench_to_i64,
    bench_to_u64,
    bench_to_f64,
    bench_is_integer,
);
criterion_main!(benches);
