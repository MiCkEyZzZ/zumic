use std::hint::black_box;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{Broker, MessagePayload, Subscriber};

fn bench_subscribe(c: &mut Criterion) {
    let broker = Broker::new();
    c.bench_function("subscribe", |b| {
        b.iter(|| {
            // тратим всю работу подписки
            let _sub = black_box(broker.subscribe("chan"));
        })
    });
}

fn bench_unsubscribe(c: &mut Criterion) {
    let broker = Broker::new();
    c.bench_function("unsubscribe", |b| {
        b.iter(|| {
            let sub = broker.subscribe("chan").unwrap();
            drop(sub);
            black_box(());
        });
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
    // создаём 10 подписок заранее
    let _subs: Vec<Subscriber> = (0..10).map(|_| broker.subscribe("chan").unwrap()).collect();
    c.bench_function("publish_10_sub", |b| {
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
    // создаём 100 подписок заранее
    let _subs: Vec<Subscriber> = (0..100)
        .map(|_| broker.subscribe("chan").unwrap())
        .collect();
    c.bench_function("publish_100_sub", |b| {
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
    bench_unsubscribe,
    bench_publish_1_sub,
    bench_publish_10_sub,
    bench_publish_100_sub
);
criterion_main!(benches);
