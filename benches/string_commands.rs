use criterion::{black_box, criterion_group, criterion_main, Criterion};
use zumic::{
    command::{AppendCommand, CommandExecute, GetRangeCommand, StrLenCommand},
    database::{ArcBytes, Sds, Value},
    engine::{engine::StorageEngine, memory::InMemoryStore},
};

fn setup_store_with_str(key: &str, value: &str) -> StorageEngine {
    let mut store = StorageEngine::InMemory(InMemoryStore::new());
    store
        .set(ArcBytes::from_str(key), Value::Str(Sds::from_str(value)))
        .unwrap();
    store
}

fn bench_strlen(c: &mut Criterion) {
    let mut store = setup_store_with_str("foo", "hello world");
    let cmd = StrLenCommand { key: "foo".into() };

    c.bench_function("StrLen", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

fn bench_append(c: &mut Criterion) {
    let mut store = setup_store_with_str("foo", "hello");
    let cmd = AppendCommand {
        key: "foo".into(),
        value: " world".into(),
    };

    c.bench_function("Append", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

fn bench_getrange(c: &mut Criterion) {
    let mut store = setup_store_with_str("foo", "hello world");
    let cmd = GetRangeCommand {
        key: "foo".into(),
        start: 0,
        end: 5,
    };

    c.bench_function("GetRange", |b| {
        b.iter(|| {
            let _ = cmd.execute(black_box(&mut store));
        })
    });
}

criterion_group!(benches, bench_strlen, bench_append, bench_getrange);
criterion_main!(benches);
