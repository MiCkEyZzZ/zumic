use std::{hint::black_box, sync::Arc};

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{InClusterStore, InMemoryStore, Sds, Storage, Value}; // замени `your_crate` на имя твоего крейта

fn make_cluster() -> InClusterStore {
    #[allow(clippy::arc_with_non_send_sync)]
    let s1 = Arc::new(InMemoryStore::new());
    #[allow(clippy::arc_with_non_send_sync)]
    let s2 = Arc::new(InMemoryStore::new());
    InClusterStore::new(vec![s1, s2])
}

fn bench_set_get(c: &mut Criterion) {
    let cluster = make_cluster();
    let key = Sds::from_str("key{1}");
    let value = Value::Int(123);

    c.bench_function("cluster_store set", |b| {
        b.iter(|| {
            cluster
                .set(black_box(&key), black_box(value.clone()))
                .unwrap();
        })
    });

    cluster.set(&key, value.clone()).unwrap();

    c.bench_function("cluster_store get", |b| {
        b.iter(|| {
            let _ = cluster.get(black_box(&key)).unwrap();
        })
    });
}

fn bench_mset_mget(c: &mut Criterion) {
    let cluster = make_cluster();

    let keys: Vec<_> = (0..100)
        .map(|i| Sds::from_str(&format!("key{{tag}}{i}")))
        .collect();
    let values: Vec<_> = (0..100).map(Value::Int).collect();
    let entries: Vec<_> = keys
        .iter()
        .zip(values.iter())
        .map(|(k, v)| (k, v.clone()))
        .collect();

    c.bench_function("cluster_store mset 100", |b| {
        b.iter(|| {
            cluster.mset(black_box(entries.clone())).unwrap();
        })
    });

    let key_refs: Vec<&Sds> = keys.iter().collect();
    cluster.mset(entries.clone()).unwrap();

    c.bench_function("cluster_store mget 100", |b| {
        b.iter(|| {
            let _ = cluster.mget(black_box(&key_refs)).unwrap();
        })
    });
}

criterion_group!(benches, bench_set_get, bench_mset_mget);
criterion_main!(benches);
