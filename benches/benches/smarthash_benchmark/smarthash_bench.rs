use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    CommandExecute, HGetCommand, HIncrByCommand, HIncrByFloatCommand, HSetCommand, InMemoryStore,
    StorageEngine, Value,
};

fn init_store_int() -> StorageEngine {
    let mut store = StorageEngine::Memory(InMemoryStore::new());
    HSetCommand {
        key: "counter".into(),
        entries: vec![("hits".into(), "0".into())],
    }
    .execute(&mut store)
    .unwrap();
    store
}

fn init_store_float() -> StorageEngine {
    let mut store = StorageEngine::Memory(InMemoryStore::new());
    HSetCommand {
        key: "float_counter".into(),
        entries: vec![("hits".into(), "0.0".into())],
    }
    .execute(&mut store)
    .unwrap();
    store
}

fn benchmark_hincrby_atomic(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_increment_int");

    group.bench_function("hincrby_atomic", |b| {
        b.iter(|| {
            let mut store = init_store_int();

            for _ in 0..1000 {
                HIncrByCommand {
                    key: "counter".into(),
                    field: "hits".into(),
                    increment: black_box(1),
                }
                .execute(&mut store)
                .unwrap();
            }
        });
    });

    group.bench_function("hget_hset_manual", |b| {
        b.iter(|| {
            let mut store = init_store_int();

            for _ in 0..1000 {
                let current = HGetCommand {
                    key: "counter".into(),
                    field: "hits".into(),
                }
                .execute(&mut store)
                .unwrap();

                let val: i64 = match current {
                    Value::Str(s) => s.as_str().unwrap_or("0").parse().unwrap_or(0),
                    _ => 0,
                };

                let new_val = val + black_box(1);

                HSetCommand {
                    key: "counter".into(),
                    entries: vec![("hits".into(), new_val.to_string())],
                }
                .execute(&mut store)
                .unwrap();
            }
        });
    });

    group.finish();
}

fn benchmark_hincrbyfloat_atomic(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_increment_float");

    group.bench_function("hincrbyfloat_atomic", |b| {
        b.iter(|| {
            let mut store = init_store_float();

            for _ in 0..1000 {
                HIncrByFloatCommand {
                    key: "float_counter".into(),
                    field: "hits".into(),
                    increment: black_box(1.5),
                }
                .execute(&mut store)
                .unwrap();
            }
        });
    });

    group.bench_function("hget_parse_set_manual_float", |b| {
        b.iter(|| {
            let mut store = init_store_float();

            for _ in 0..1000 {
                let current = HGetCommand {
                    key: "float_counter".into(),
                    field: "hits".into(),
                }
                .execute(&mut store)
                .unwrap();

                let val: f64 = match current {
                    Value::Str(s) => s.as_str().unwrap_or("0.0").parse().unwrap_or(0.0),
                    _ => 0.0,
                };

                let new_val = val + black_box(1.5);

                HSetCommand {
                    key: "float_counter".into(),
                    entries: vec![("hits".into(), new_val.to_string())],
                }
                .execute(&mut store)
                .unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_hincrby_atomic,
    benchmark_hincrbyfloat_atomic
);
criterion_main!(benches);
