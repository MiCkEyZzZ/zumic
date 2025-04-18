use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use zumic::ArcBytes;

fn bench_arcbytes_from_vec(c: &mut Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("ArcBytes::from_vec", |b| {
        b.iter(|| {
            let ab = ArcBytes::from_vec(black_box(data.clone()));
            black_box(ab);
        });
    });
}

fn bench_arcbytes_clone(c: &mut Criterion) {
    let ab = ArcBytes::from_vec(vec![0u8; 1024]);
    c.bench_function("ArcBytes::clone", |b| {
        b.iter(|| {
            let cloned = black_box(&ab).clone();
            black_box(cloned);
        });
    });
}

fn bench_arcbytes_as_slice(c: &mut Criterion) {
    let ab = ArcBytes::from_vec(vec![0u8; 1024]);
    c.bench_function("ArcBytes::as_slice", |b| {
        b.iter(|| {
            black_box(ab.as_slice());
        });
    });
}

fn bench_arcbytes_as_str(c: &mut Criterion) {
    let ab = ArcBytes::from_vec("hello world".as_bytes().to_vec());
    c.bench_function("ArcBytes::as_str", |b| {
        b.iter(|| {
            black_box(ab.as_str().unwrap());
        });
    });
}

fn bench_arcbytes_slice(c: &mut Criterion) {
    let ab = ArcBytes::from_vec(vec![0u8; 1024]);
    c.bench_function("ArcBytes::slice", |b| {
        b.iter(|| {
            let sliced = black_box(ab.slice(100..900));
            black_box(sliced);
        });
    });
}

fn bench_arcbytes_eq(c: &mut Criterion) {
    let a = ArcBytes::from_vec(vec![1, 2, 3, 4, 5]);
    let b = ArcBytes::from_vec(vec![1, 2, 3, 4, 5]);
    c.bench_function("ArcBytes::eq", |bch| {
        bch.iter(|| black_box(a == b));
    });
}

fn bench_arcbytes_hash(c: &mut Criterion) {
    let a = ArcBytes::from_vec(vec![1, 2, 3, 4, 5]);
    c.bench_function("ArcBytes::hash", |bch| {
        bch.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(a.hash(&mut hasher));
            black_box(hasher.finish());
        });
    });
}

fn bench_arcbytes_into_bytes(c: &mut Criterion) {
    let ab = ArcBytes::from_vec(vec![0u8; 512]);
    c.bench_function("ArcBytes::into_bytes", |b| {
        b.iter(|| {
            let _ = black_box(ab.clone()).into_bytes();
        });
    });
}

fn bench_arcbytes_into_inner(c: &mut Criterion) {
    let ab = ArcBytes::from_vec(vec![0u8; 512]);
    c.bench_function("ArcBytes::into_inner", |b| {
        b.iter(|| {
            let _ = black_box(ab.clone()).into_inner();
        });
    });
}

fn bench_bytes_from_vec(c: &mut Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("Bytes::from", |b| {
        b.iter(|| {
            let bts = Bytes::from(black_box(data.clone()));
            black_box(bts);
        });
    });
}

fn bench_bytes_clone(c: &mut Criterion) {
    let bts = Bytes::from(vec![0u8; 1024]);
    c.bench_function("Bytes::clone", |b| {
        b.iter(|| {
            let cloned = black_box(&bts).clone();
            black_box(cloned);
        });
    });
}

fn bench_bytes_slice(c: &mut Criterion) {
    let bts = Bytes::from(vec![0u8; 1024]);
    c.bench_function("Bytes::slice", |b| {
        b.iter(|| {
            let sliced = black_box(&bts).slice(100..900);
            black_box(sliced);
        });
    });
}

fn bench_vec_clone(c: &mut Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("Vec<u8>::clone", |b| {
        b.iter(|| {
            let v = black_box(&data).clone();
            black_box(v);
        });
    });
}

criterion_group!(
    benches,
    bench_arcbytes_from_vec,
    bench_arcbytes_clone,
    bench_arcbytes_as_slice,
    bench_arcbytes_as_str,
    bench_arcbytes_slice,
    bench_arcbytes_eq,
    bench_arcbytes_hash,
    bench_arcbytes_into_bytes,
    bench_arcbytes_into_inner,
    bench_bytes_from_vec,
    bench_bytes_clone,
    bench_bytes_slice,
    bench_vec_clone
);
criterion_main!(benches);
