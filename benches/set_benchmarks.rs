use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{
    command::set::{SAddCommand, SIsMemberCommand, SMembersCommand, SRemCommand},
    engine::{memory::InMemoryStore, store::StorageEngine},
    CommandExecute,
};

fn bench_sadd(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    c.bench_function("SAddCommand - insert 1000 items", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let cmd = SAddCommand {
                    key: "myset".into(),
                    member: format!("member{i}"),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_sismember(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    // Предварительно заполним множество
    for i in 0..1000 {
        let cmd = SAddCommand {
            key: "myset".into(),
            member: format!("member{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("SIsMemberCommand - lookup 1000 existing items", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let cmd = SIsMemberCommand {
                    key: "myset".into(),
                    member: format!("member{i}"),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_srem(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..1000 {
        let cmd = SAddCommand {
            key: "myset".into(),
            member: format!("member{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("SRemCommand - remove 1000 items", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let cmd = SRemCommand {
                    key: "myset".into(),
                    member: format!("member{i}"),
                };
                let _ = cmd.execute(black_box(&mut store));
            }
        });
    });
}

fn bench_smembers(c: &mut Criterion) {
    let mut store = StorageEngine::Memory(InMemoryStore::new());

    for i in 0..1000 {
        let cmd = SAddCommand {
            key: "myset".into(),
            member: format!("member{i}"),
        };
        let _ = cmd.execute(&mut store);
    }

    c.bench_function("SMembersCommand - get all members", |b| {
        b.iter(|| {
            let cmd = SMembersCommand {
                key: "myset".into(),
            };
            let _ = cmd.execute(black_box(&mut store));
        });
    });
}

criterion_group!(
    benches,
    bench_sadd,
    bench_sismember,
    bench_srem,
    bench_smembers
);
criterion_main!(benches);
