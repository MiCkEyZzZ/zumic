use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::{Broker, PubSubPort, Subscription, SubscriptionPort};

// Подставьте здесь ваше имя крайта и пути к модулям:

fn bench_subscribe(c: &mut Criterion) {
    let broker = Broker::new(100);
    c.bench_function("subscribe", |b| {
        b.iter(|| {
            // тратим всю работу подписки
            let _sub = black_box(broker.subscribe("chan"));
        })
    });
}

fn bench_unsubscribe(c: &mut Criterion) {
    let broker = Broker::new(100);
    c.bench_function("unsubscribe", |b| {
        b.iter(|| {
            // создаём подписку, а потом сразу её отпускаем
            let sub = broker.subscribe("chan");
            black_box(sub.unsubscribe());
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
    // создаём 10 подписок заранее
    let _subs: Vec<Subscription> = (0..10).map(|_| broker.subscribe("chan")).collect();
    c.bench_function("publish_10_sub", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
        })
    });
}

fn bench_publish_100_sub(c: &mut Criterion) {
    let broker = Broker::new(100);
    // создаём 100 подписок заранее
    let _subs: Vec<Subscription> = (0..100).map(|_| broker.subscribe("chan")).collect();
    c.bench_function("publish_100_sub", |b| {
        b.iter(|| {
            broker.publish("chan", black_box(Bytes::from_static(b"x")));
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
