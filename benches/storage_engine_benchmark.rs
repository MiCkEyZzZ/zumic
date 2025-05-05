use criterion::{black_box, criterion_group, criterion_main, Criterion};

use zumic::{
    config::settings::{StorageConfig, StorageType},
    Sds, StorageEngine, Value,
};

fn make_engine() -> StorageEngine {
    // инициализируем engine через config
    let cfg = StorageConfig {
        storage_type: StorageType::Memory,
    };
    StorageEngine::initialize(&cfg).unwrap()
}

fn bench_engine_set_get(c: &mut Criterion) {
    let engine = make_engine();
    let key = Sds::from_str("bench_key");
    let value = Value::Int(42);

    c.bench_function("engine.set", |b| {
        b.iter(|| {
            engine
                .set(black_box(&key), black_box(value.clone()))
                .unwrap()
        })
    });

    // подготовим: один раз вставим
    engine.set(&key, value.clone()).unwrap();

    c.bench_function("engine.get", |b| {
        b.iter(|| black_box(engine.get(black_box(&key)).unwrap()))
    });
}

fn bench_engine_mset_mget(c: &mut Criterion) {
    let engine = make_engine();

    // подготовим данные
    let keys: Vec<Sds> = (0..100)
        .map(|i| Sds::from_str(&format!("k{}", i)))
        .collect();
    let vals: Vec<Value> = (0..100).map(Value::Int).collect();
    let entries: Vec<(&Sds, Value)> = keys.iter().zip(vals.iter().cloned()).collect();

    c.bench_function("engine.mset 100", |b| {
        b.iter(|| engine.mset(black_box(entries.clone())).unwrap())
    });

    // сделаем один раз mset
    engine.mset(entries.clone()).unwrap();
    let key_refs: Vec<&Sds> = keys.iter().collect();

    c.bench_function("engine.mget 100", |b| {
        b.iter(|| black_box(engine.mget(black_box(&key_refs)).unwrap()))
    });
}

criterion_group!(benches, bench_engine_set_get, bench_engine_mset_mget);
criterion_main!(benches);
