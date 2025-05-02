use criterion::{criterion_group, criterion_main, Criterion};

use zumic::{
    CommandExecute, {InMemoryStore, StorageEngine},
    {ZAddCommand, ZCardCommand, ZRangeCommand, ZRemCommand, ZScoreCommand},
};

fn bench_zadd(c: &mut Criterion) {
    let mut group = c.benchmark_group("ZAddCommand");
    group.sample_size(100);
    group.bench_function("insert 100 members", |b| {
        b.iter(|| {
            let mut engine = StorageEngine::InMemory(InMemoryStore::new());
            for i in 0..100 {
                let cmd = ZAddCommand {
                    key: "myzset".to_string(),
                    member: format!("member{i}"),
                    score: i as f64,
                };
                let _ = cmd.execute(&mut engine);
            }
        });
    });
    group.finish();
}

fn bench_zrem(c: &mut Criterion) {
    let mut group = c.benchmark_group("ZRemCommand");
    group.sample_size(100);
    group.bench_function("remove 100 members", |b| {
        b.iter(|| {
            let mut engine = StorageEngine::InMemory(InMemoryStore::new());
            for i in 0..100 {
                let cmd = ZAddCommand {
                    key: "myzset".to_string(),
                    member: format!("member{i}"),
                    score: i as f64,
                };
                let _ = cmd.execute(&mut engine);
            }
            for i in 0..100 {
                let cmd = ZRemCommand {
                    key: "myzset".to_string(),
                    member: format!("member{i}"),
                };
                let _ = cmd.execute(&mut engine);
            }
        });
    });
    group.finish();
}

fn bench_zscore(c: &mut Criterion) {
    let mut group = c.benchmark_group("ZScoreCommand");
    group.sample_size(100);
    group.bench_function("get score 100 times", |b| {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        for i in 0..100 {
            let cmd = ZAddCommand {
                key: "myzset".to_string(),
                member: format!("member{i}"),
                score: i as f64,
            };
            let _ = cmd.execute(&mut engine);
        }
        b.iter(|| {
            for i in 0..100 {
                let cmd = ZScoreCommand {
                    key: "myzset".to_string(),
                    member: format!("member{i}"),
                };
                let _ = cmd.execute(&mut engine);
            }
        });
    });
    group.finish();
}

fn bench_zrange(c: &mut Criterion) {
    let mut group = c.benchmark_group("ZRangeCommand");
    group.sample_size(100);
    group.bench_function("range 100..200", |b| {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        for i in 0..300 {
            let cmd = ZAddCommand {
                key: "myzset".to_string(),
                member: format!("member{i}"),
                score: i as f64,
            };
            let _ = cmd.execute(&mut engine);
        }
        let cmd = ZRangeCommand {
            key: "myzset".to_string(),
            start: 100,
            stop: 200,
        };
        b.iter(|| {
            let _ = cmd.execute(&mut engine);
        });
    });
    group.finish();
}

fn bench_zcard(c: &mut Criterion) {
    let mut group = c.benchmark_group("ZCardCommand");
    group.sample_size(100);
    group.bench_function("get cardinality", |b| {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        for i in 0..300 {
            let cmd = ZAddCommand {
                key: "myzset".to_string(),
                member: format!("member{i}"),
                score: i as f64,
            };
            let _ = cmd.execute(&mut engine);
        }
        let cmd = ZCardCommand {
            key: "myzset".to_string(),
        };
        b.iter(|| {
            let _ = cmd.execute(&mut engine);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_zadd,
    bench_zrem,
    bench_zscore,
    bench_zrange,
    bench_zcard,
);
criterion_main!(benches);
