use criterion::{black_box, criterion_group, criterion_main, Criterion};

use zumic::{
    command::list::{
        LLenCommand, LPopCommand, LPushCommand, LRangeCommand, RPopCommand, RPushCommand,
    },
    engine::{memory::InMemoryStore, store::StorageEngine},
    CommandExecute,
};

fn bench_lpush(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    c.bench_function("LPushCommand - insert 100 items to head", |b| {
        b.iter(|| {
            for i in 0..100 {
                let cmd = LPushCommand {
                    key: "mylist".into(),
                    value: format!("val{i}"),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_rpush(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    c.bench_function("RPushCommand - insert 100 items to tail", |b| {
        b.iter(|| {
            for i in 0..100 {
                let cmd = RPushCommand {
                    key: "mylist".into(),
                    value: format!("val{i}"),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_lpop(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..100 {
        let cmd = LPushCommand {
            key: "mylist".into(),
            value: format!("val{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("LPopCommand - pop 100 items from head", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let cmd = LPopCommand {
                    key: "mylist".into(),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_rpop(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..100 {
        let cmd = RPushCommand {
            key: "mylist".into(),
            value: format!("val{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("RPopCommand - pop 100 items from tail", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let cmd = RPopCommand {
                    key: "mylist".into(),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_llen(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..100 {
        let cmd = LPushCommand {
            key: "mylist".into(),
            value: format!("val{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("LLenCommand - get length", |b| {
        b.iter(|| {
            let cmd = LLenCommand {
                key: "mylist".into(),
            };
            let _ = cmd.execute(black_box(&mut store));
        });
    });
}

fn bench_lrange(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..100 {
        let cmd = RPushCommand {
            key: "mylist".into(),
            value: format!("val{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("LRangeCommand - range [100..200]", |b| {
        b.iter(|| {
            let cmd = LRangeCommand {
                key: "mylist".into(),
                start: 100,
                stop: 200,
            };
            let _ = cmd.execute(black_box(&mut store));
        });
    });
}

criterion_group!(
    benches,
    bench_lpush,
    bench_rpush,
    bench_lpop,
    bench_rpop,
    bench_llen,
    bench_lrange
);
criterion_main!(benches);
