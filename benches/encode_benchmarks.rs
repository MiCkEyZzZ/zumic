use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    engine::{write_dump, write_value},
    Sds, Value,
};

fn bench_write_value_small(c: &mut Criterion) {
    let v = Value::Str(Sds::from_str("short"));
    c.bench_function("write_value small", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(64);
            write_value(&mut buf, black_box(&v)).unwrap();
            black_box(buf);
        })
    });
}

fn bench_write_value_large(c: &mut Criterion) {
    // сделаем строку > MIN_COMPRESSION_SIZE
    let s = "x".repeat(128);
    let v = Value::Str(Sds::from_str(&s));
    c.bench_function("write_value large", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(256);
            write_value(&mut buf, black_box(&v)).unwrap();
            black_box(buf);
        })
    });
}

fn bench_write_dump(c: &mut Criterion) {
    // сгенерируем несколько пар
    let items: Vec<(Sds, Value)> = (0..100)
        .map(|i| {
            let key = Sds::from_str(&format!("key{}", i));
            let val = Value::Int(i);
            (key, val)
        })
        .collect();
    c.bench_function("write_dump 100 ints", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_dump(&mut buf, black_box(items.clone().into_iter())).unwrap();
            black_box(buf);
        })
    });
}

criterion_group!(
    benches,
    bench_write_value_small,
    bench_write_value_large,
    bench_write_dump
);
criterion_main!(benches);
