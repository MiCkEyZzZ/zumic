use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::{
    command::{CommandExecute, HDelCommand, HGetAllCommand, HGetCommand, HSetCommand},
    engine::{engine::StorageEngine, memory::InMemoryStore},
};

fn setup_store_with_hash() -> StorageEngine {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());

    // Инициализируем несколько хешей с полями
    for i in 0..1000 {
        for j in 0..10 {
            let cmd = HSetCommand {
                key: format!("hash_{i}"),
                field: format!("field_{j}"),
                value: format!("value_{i}_{j}"),
            };
            cmd.execute(&mut store).unwrap();
        }
    }

    store
}

fn bench_hset(c: &mut Criterion) {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());
    let cmd = HSetCommand {
        key: "hash".into(),
        field: "field1".into(),
        value: "value1".into(),
    };

    c.bench_function("HSet", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

fn bench_hget(c: &mut Criterion) {
    let mut store = setup_store_with_hash();
    let cmd = HGetCommand {
        key: "hash_0".into(),
        field: "field_0".into(),
    };

    c.bench_function("HGet", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

fn bench_hdel(c: &mut Criterion) {
    let mut store = setup_store_with_hash();
    let cmd = HDelCommand {
        key: "hash_0".into(),
        field: "field_0".into(),
    };

    c.bench_function("HDel", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

fn bench_hgetall(c: &mut Criterion) {
    let mut store = setup_store_with_hash();
    let cmd = HGetAllCommand {
        key: "hash_0".into(),
    };

    c.bench_function("HGetAll", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

criterion_group!(benches, bench_hset, bench_hget, bench_hdel, bench_hgetall);
criterion_main!(benches);
