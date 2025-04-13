use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    command::{CommandExecute, DecrByFloatCommand, IncrByFloatCommand, SetFloatCommand},
    database::Value,
    engine::{engine::StorageEngine, memory::InMemoryStore},
};

fn bench_set_float(c: &mut Criterion) {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());

    c.bench_function("set_float command", |b| {
        b.iter(|| {
            let cmd = SetFloatCommand {
                key: "floatkey".to_string(),
                value: 42.42,
            };
            let _ = cmd.execute(&mut store);
        });
    });
}

fn bench_incr_float(c: &mut Criterion) {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());
    store.set("floatkey".into(), Value::Float(10.0)).unwrap();

    c.bench_function("incr_by_float command", |b| {
        b.iter(|| {
            let cmd = IncrByFloatCommand {
                key: "floatkey".to_string(),
                increment: 1.1,
            };
            let _ = cmd.execute(&mut store);
        });
    });
}

fn bench_decr_float(c: &mut Criterion) {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());
    store.set("floatkey".into(), Value::Float(10.0)).unwrap();

    c.bench_function("decr_by_float command", |b| {
        b.iter(|| {
            let cmd = DecrByFloatCommand {
                key: "floatkey".to_string(),
                decrement: 0.9,
            };
            let _ = cmd.execute(&mut store);
        });
    });
}

criterion_group!(
    float_benches,
    bench_set_float,
    bench_incr_float,
    bench_decr_float
);
criterion_main!(float_benches);
