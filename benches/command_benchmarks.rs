use criterion::{criterion_group, criterion_main, Criterion};
use zumic::{
    command::{DelCommand, GetCommand, MGetCommand, MSetCommand, SetCommand},
    database::{Sds, Value},
    engine::{engine::StorageEngine, memory::InMemoryStore},
    CommandExecute,
};

fn bench_set_command(c: &mut Criterion) {
    c.bench_function("set command", |b| {
        b.iter(|| {
            let mut store = StorageEngine::InMemory(InMemoryStore::new());
            let cmd = SetCommand {
                key: "test_key".to_string(),
                value: Value::Str(Sds::from_str("value")),
            };
            cmd.execute(&mut store).unwrap();
        });
    });
}

fn bench_get_command(c: &mut Criterion) {
    c.bench_function("get command", |b| {
        b.iter(|| {
            let mut store = StorageEngine::InMemory(InMemoryStore::new());
            let set_cmd = SetCommand {
                key: "test_key".to_string(),
                value: Value::Str(Sds::from_str("test_value")),
            };
            set_cmd.execute(&mut store).unwrap();

            let get_cmd = GetCommand {
                key: "test_key".to_string(),
            };
            get_cmd.execute(&mut store).unwrap();
        })
    });
}

fn bench_del_command(c: &mut Criterion) {
    c.bench_function("del command", |b| {
        b.iter(|| {
            let mut store = StorageEngine::InMemory(InMemoryStore::new());
            let set_cmd = SetCommand {
                key: "test_key".to_string(),
                value: Value::Str(Sds::from_str("test_value")),
            };
            set_cmd.execute(&mut store).unwrap();

            let del_cmd = DelCommand {
                key: "test_key".to_string(),
            };
            del_cmd.execute(&mut store).unwrap();
        })
    });
}

fn bench_mset_command(c: &mut Criterion) {
    c.bench_function("mset command", |b| {
        b.iter(|| {
            let mut store = StorageEngine::InMemory(InMemoryStore::new());
            let mset_cmd = MSetCommand {
                entries: vec![
                    ("key1".to_string(), Value::Str(Sds::from_str("value1"))),
                    ("key2".to_string(), Value::Str(Sds::from_str("value2"))),
                ],
            };
            mset_cmd.execute(&mut store).unwrap();
        })
    });
}

fn bench_mget_command(c: &mut Criterion) {
    c.bench_function("mget command", |b| {
        b.iter(|| {
            let mut store = StorageEngine::InMemory(InMemoryStore::new());
            let mset_cmd = MSetCommand {
                entries: vec![
                    ("key1".to_string(), Value::Str(Sds::from_str("value1"))),
                    ("key2".to_string(), Value::Str(Sds::from_str("value2"))),
                ],
            };
            mset_cmd.execute(&mut store).unwrap();

            let mget_cmd = MGetCommand {
                keys: vec!["key1".to_string(), "key2".to_string()],
            };
            mget_cmd.execute(&mut store).unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_set_command,
    bench_get_command,
    bench_del_command,
    bench_mset_command,
    bench_mget_command
);

criterion_main!(benches);
