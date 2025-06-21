use bytes::Bytes;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::Message;

// Предполагаем, что ваш crate называется `pubsub` и там лежит модуль message

fn bench_new_string_vec(c: &mut Criterion) {
    let payload = vec![42u8; 1024];
    c.bench_function("Message::new(&str, Vec<u8>)", |b| {
        b.iter(|| {
            // black_box чтобы предотвратить оптимизацию
            let _ = Message::new(black_box("channel"), black_box(payload.clone()));
        })
    });
}

fn bench_new_static(c: &mut Criterion) {
    c.bench_function("Message::from_static", |b| {
        b.iter(|| {
            let _ = Message::from_static(black_box("static_channel"), black_box(b"static_payload"));
        })
    });
}

fn bench_clone_small(c: &mut Criterion) {
    let msg = Message::new("chan", Bytes::from_static(b"x"));
    c.bench_function("Message::clone small", |b| {
        b.iter(|| {
            let _ = black_box(msg.clone());
        })
    });
}

fn bench_clone_large(c: &mut Criterion) {
    // payload ~1MB
    let big = vec![1u8; 1_000_000];
    let msg = Message::new("bigchan", Bytes::from(big.clone()));
    c.bench_function("Message::clone large", |b| {
        b.iter(|| {
            let _ = black_box(msg.clone());
        })
    });
}

criterion_group!(
    benches,
    bench_new_string_vec,
    bench_new_static,
    bench_clone_small,
    bench_clone_large,
);
criterion_main!(benches);
