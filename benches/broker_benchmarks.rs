use std::hint::black_box;

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{Broker, Subscription};

fn bench_subscribe(c: &mut Criterion) {
    let broker = Broker::new(100);
    c.bench_function("broker_subscribe", |b| {
        b.iter(|| {
            let _sub = black_box(broker.subscribe("chan"));
        })
    });
}

fn bench_unsubscribe_all(c: &mut Criterion) {
    let broker = Broker::new(100);
    // предварительно создаём канал
    let _ = broker.subscribe("chan");
    c.bench_function("broker_unsubscribe_all", |b| {
        b.iter(|| {
            broker.unsubscribe_all("chan");
            black_box(())
        });
    });
}

fn bench_publish_0_sub(c: &mut Criterion) {
    let broker = Broker::new(100);
    c.bench_function("publish_0_subs", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
        })
    });
}

fn bench_publish_1_sub(c: &mut Criterion) {
    let broker = Broker::new(100);
    let _sub = broker.subscribe("chan");
    c.bench_function("publish_1_sub", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
        })
    });
}

fn bench_publish_10_sub(c: &mut Criterion) {
    let broker = Broker::new(100);
    let _subs: Vec<Subscription> = (0..10).map(|_| broker.subscribe("chan")).collect();
    c.bench_function("publish_10_subs", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
        })
    });
}

fn bench_publish_100_sub(c: &mut Criterion) {
    let broker = Broker::new(100);
    let _subs: Vec<Subscription> = (0..100).map(|_| broker.subscribe("chan")).collect();
    c.bench_function("publish_100_subs", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
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
