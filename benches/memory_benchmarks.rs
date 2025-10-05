use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{InMemoryStore, Sds, Storage, Value};

/// Генерация ключей для бенчмарков.
fn key(data: &str) -> Sds {
    Sds::from(data.as_bytes())
}

/// Бенчмарк для вставки элементов в хранилище.
fn bench_set(c: &mut Criterion) {
    c.bench_function("set", |b| {
        b.iter(|| {
            let store = InMemoryStore::new();
            let k = key("hello");
            let v = Value::Str(Sds::from_str("world"));
            store.set(&k, v).unwrap(); // Теперь метод set доступен
        })
    });
}

/// Бенчмарк для получения элемента по ключу.
fn bench_get(c: &mut Criterion) {
    c.bench_function("get", |b| {
        let store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));
        store.set(&k, v.clone()).unwrap();

        b.iter(|| {
            let got = store.get(&k).unwrap();
            black_box(got);
        })
    });
}

/// Бенчмарк для удаления элемента по ключу.
fn bench_del(c: &mut Criterion) {
    c.bench_function("del", |b| {
        let store = InMemoryStore::new();
        let k = key("key_to_delete");
        let v = Value::Str(Sds::from_str("value"));
        store.set(&k, v.clone()).unwrap();

        b.iter(|| {
            store.del(&k).unwrap();
        })
    });
}

/// Бенчмарк для массовой вставки.
fn bench_mset(c: &mut Criterion) {
    c.bench_function("mset", |b| {
        let store = InMemoryStore::new();

        // Создаём все ключи заранее и храним их в векторе
        let keys: Vec<Sds> = (0..1000).map(|i| key(&format!("key_{i}"))).collect();

        let entries: Vec<(&Sds, Value)> = keys
            .iter()
            .map(|key| {
                (
                    key, // Теперь мы передаем ссылки на уже созданные ключи
                    Value::Str(Sds::from_str(&format!("value_{key}"))),
                )
            })
            .collect();

        b.iter(|| {
            store.mset(entries.clone()).unwrap();
        })
    });
}

/// Бенчмарк для массового получения значений.
fn bench_mget(c: &mut Criterion) {
    c.bench_function("mget", |b| {
        let store = InMemoryStore::new();
        let keys: Vec<Sds> = (0..1000).map(|i| key(&format!("key_{i}"))).collect();
        let values: Vec<Value> = keys
            .iter()
            .map(|k| Value::Str(Sds::from_str(&format!("value_{k}"))))
            .collect();

        // Заполняем хранилище.
        for (key, value) in keys.iter().zip(values.iter()) {
            store.set(key, value.clone()).unwrap();
        }

        // Передаем срез ссылок на элементы ключей
        b.iter(|| {
            let result = store.mget(&keys.iter().collect::<Vec<_>>()).unwrap();
            black_box(result);
        })
    });
}

criterion_group!(benches, bench_set, bench_get, bench_del, bench_mset, bench_mget);

criterion_main!(benches);
