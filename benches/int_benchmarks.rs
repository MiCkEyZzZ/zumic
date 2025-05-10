use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    command::{DecrByCommand, DecrCommand, IncrByCommand, IncrCommand},
    database::{Sds, Value},
    engine::{engine::StorageEngine, memory::InMemoryStore},
    CommandExecute,
};

fn key(data: &str) -> Sds {
    Sds::from(data.as_bytes())
}

fn int_commands_benchmark(c: &mut Criterion) {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());
    store.set(&key("key"), Value::Int(0)).unwrap();

    c.bench_function("incr command", |b| {
        b.iter(|| {
            let cmd = IncrCommand {
                key: "key".to_string(),
            };
            cmd.execute(&mut store).unwrap();
        })
    });

    c.bench_function("incrby command", |b| {
        b.iter(|| {
            let cmd = IncrByCommand {
                key:       "key".to_string(),
                increment: 5,
            };
            cmd.execute(&mut store).unwrap();
        })
    });

    c.bench_function("decr command", |b| {
        b.iter(|| {
            let cmd = DecrCommand {
                key: "key".to_string(),
            };
            cmd.execute(&mut store).unwrap();
        })
    });

    c.bench_function("decrby command", |b| {
        b.iter(|| {
            let cmd = DecrByCommand {
                key:       "key".to_string(),
                decrement: 5,
            };
            cmd.execute(&mut store).unwrap();
        })
    });
}

criterion_group!(benches, int_commands_benchmark);
criterion_main!(benches);
