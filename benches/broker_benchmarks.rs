use std::hint::black_box;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{Broker, MessagePayload, Subscriber};

fn bench_subscribe(c: &mut Criterion) {
    let broker = Broker::new();
    c.bench_function("broker_subscribe", |b| {
        b.iter(|| {
            let _sub = black_box(broker.subscribe("chan"));
        })
    });
}

fn bench_unsubscribe_all(c: &mut Criterion) {
    let broker = Broker::new();
    // предварительно создаём канал
    let _ = broker.subscribe("chan");
    c.bench_function("broker_unsubscribe_all", |b| {
        b.iter(|| {
            broker.unsubscribe("chan");
            black_box(())
        });
    });
}

fn bench_publish_0_sub(c: &mut Criterion) {
    let broker = Broker::new();
    c.bench_function("publish_0_subs", |b| {
        b.iter(|| {
            broker
                .publish(
                    "chan",
                    black_box(MessagePayload::Bytes(Bytes::from_static(b"x"))),
                )
                .unwrap();
        })
    });
}

fn bench_publish_1_sub(c: &mut Criterion) {
    let broker = Broker::new();
    let _sub = broker.subscribe("chan");
    c.bench_function("publish_1_sub", |b| {
        b.iter(|| {
            broker
                .publish(
                    "chan",
                    black_box(MessagePayload::Bytes(Bytes::from_static(b"x"))),
                )
                .unwrap();
        })
    });
}

fn bench_publish_10_sub(c: &mut Criterion) {
    let broker = Broker::new();
    let _subs: Vec<Subscriber> = (0..10).map(|_| broker.subscribe("chan").unwrap()).collect();
    c.bench_function("publish_10_subs", |b| {
        b.iter(|| {
            broker
                .publish(
                    "chan",
                    black_box(MessagePayload::Bytes(Bytes::from_static(b"x"))),
                )
                .unwrap();
        })
    });
}

fn bench_publish_100_sub(c: &mut Criterion) {
    let broker = Broker::new();
    let _subs: Vec<Subscriber> = (0..100)
        .map(|_| broker.subscribe("chan").unwrap())
        .collect();
    c.bench_function("publish_100_subs", |b| {
        b.iter(|| {
            broker
                .publish(
                    "chan",
                    black_box(MessagePayload::Bytes(Bytes::from_static(b"x"))),
                )
                .unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_subscribe,
    bench_unsubscribe_all,
    bench_publish_0_sub,
    bench_publish_1_sub,
    bench_publish_10_sub,
    bench_publish_100_sub,
);
criterion_main!(benches);
