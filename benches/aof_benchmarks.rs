use criterion::{criterion_group, criterion_main, Criterion};
use std::fs::remove_file;
use tempfile::NamedTempFile;

use zumic::{
    engine::aof::{AofOp, SyncPolicy},
    AofLog,
};

/// Benchmark append_set performance for varying key/value sizes.
fn bench_append_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("append_set");
    for size in [10usize, 100, 1000, 10_000].iter() {
        group.bench_with_input(format!("size_{}", size), size, |b, &s| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            let mut log = AofLog::open(&path, SyncPolicy::Always).unwrap();
            let key = vec![b'k'; s];
            let value = vec![b'v'; s];
            b.iter(|| {
                log.append_set(&key, &value).unwrap();
            });
            drop(log);
            let _ = remove_file(path);
        });
    }
    group.finish();
}

/// Benchmark append_del performance for varying key sizes.
fn bench_append_del(c: &mut Criterion) {
    let mut group = c.benchmark_group("append_del");
    for size in [10usize, 100, 1000, 10_000].iter() {
        group.bench_with_input(format!("size_{}", size), size, |b, &s| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            let mut log = AofLog::open(&path, SyncPolicy::Always).unwrap();
            let key = vec![b'k'; s];
            b.iter(|| {
                log.append_del(&key).unwrap();
            });
            drop(log);
            let _ = remove_file(path);
        });
    }
    group.finish();
}

/// Benchmark replay performance for N entries.
fn bench_replay(c: &mut Criterion) {
    let mut group = c.benchmark_group("replay");
    for count in [100usize, 1_000, 10_000].iter() {
        group.bench_with_input(format!("count_{}", count), count, |b, &n| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            {
                let mut log = AofLog::open(&path, SyncPolicy::Always).unwrap();
                for i in 0..n {
                    let key = format!("key{}", i).into_bytes();
                    let value = format!("value{}", i).into_bytes();
                    log.append_set(&key, &value).unwrap();
                }
            }
            let mut log = AofLog::open(&path, SyncPolicy::Always).unwrap();
            b.iter(|| {
                // unwrap the Result from replay to avoid unused Result warning
                log.replay(|op, _key, _val| {
                    assert!(matches!(op, AofOp::Set));
                })
                .unwrap();
            });
            let _ = remove_file(path);
        });
    }
    group.finish();
}

criterion_group!(benches, bench_append_set, bench_append_del, bench_replay);
criterion_main!(benches);
