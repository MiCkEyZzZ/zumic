use std::fs::remove_file;

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::NamedTempFile;
use zumic::{
    engine::aof::{AofOp, SyncPolicy},
    AofLog,
};

/// Benchmark append_set performance for varying key/value sizes and sync policies.
fn bench_append_set(c: &mut Criterion) {
    let policies = [SyncPolicy::Always, SyncPolicy::EverySec, SyncPolicy::No];
    for &policy in &policies {
        let mut group = c.benchmark_group(format!("append_set/{:?}", policy));
        for size in [10usize, 100, 1000, 10_000].iter() {
            group.bench_with_input(format!("size_{}", size), size, |b, &s| {
                let temp = NamedTempFile::new().unwrap();
                let path = temp.path().to_path_buf();
                let mut log = AofLog::open(&path, policy).unwrap();
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
}

/// Benchmark append_del performance for varying key sizes and sync policies.
fn bench_append_del(c: &mut Criterion) {
    let policies = [SyncPolicy::Always, SyncPolicy::EverySec, SyncPolicy::No];
    for &policy in &policies {
        let mut group = c.benchmark_group(format!("append_del/{:?}", policy));
        for size in [10usize, 100, 1000, 10_000].iter() {
            group.bench_with_input(format!("size_{}", size), size, |b, &s| {
                let temp = NamedTempFile::new().unwrap();
                let path = temp.path().to_path_buf();
                let mut log = AofLog::open(&path, policy).unwrap();
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
}

/// Benchmark replay performance for N entries and sync policies.
fn bench_replay(c: &mut Criterion) {
    let policies = [SyncPolicy::Always, SyncPolicy::EverySec, SyncPolicy::No];
    for &policy in &policies {
        let mut group = c.benchmark_group(format!("replay/{:?}", policy));
        for count in [100usize, 1_000, 10_000].iter() {
            group.bench_with_input(format!("count_{}", count), count, |b, &n| {
                let temp = NamedTempFile::new().unwrap();
                let path = temp.path().to_path_buf();
                {
                    let mut log = AofLog::open(&path, policy).unwrap();
                    for i in 0..n {
                        let key = format!("key{}", i).into_bytes();
                        let value = format!("value{}", i).into_bytes();
                        log.append_set(&key, &value).unwrap();
                    }
                }
                let mut log = AofLog::open(&path, policy).unwrap();
                b.iter(|| {
                    log.replay(|op, _key, _val| assert!(matches!(op, AofOp::Set)))
                        .unwrap();
                });
                let _ = remove_file(path);
            });
        }
        group.finish();
    }
}

/// Benchmark rewrite performance for N entries.
fn bench_rewrite(c: &mut Criterion) {
    let mut group = c.benchmark_group("rewrite");
    for count in [100usize, 1_000, 10_000].iter() {
        group.bench_with_input(format!("count_{}", count), count, |b, &n| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();

            // Запись начального AOF с множеством set и del
            {
                let mut log = AofLog::open(&path, SyncPolicy::No).unwrap();
                for i in 0..n {
                    let key = format!("key{}", i).into_bytes();
                    let value = format!("value{}", i).into_bytes();
                    log.append_set(&key, &value).unwrap();
                    // Для некоторых ключей делаем DEL, чтобы имитировать "устаревание"
                    if i % 5 == 0 {
                        log.append_del(&key).unwrap();
                    }
                }
            }

            // Собираем "живое" состояние
            let mut live_map = std::collections::HashMap::new();
            {
                let mut log = AofLog::open(&path, SyncPolicy::No).unwrap();
                log.replay(|op, key, val| match op {
                    AofOp::Set => {
                        live_map.insert(key, val.unwrap());
                    }
                    AofOp::Del => {
                        live_map.remove(&key);
                    }
                })
                .unwrap();
            }

            // Измеряем rewrite
            b.iter(|| {
                let mut log = AofLog::open(&path, SyncPolicy::No).unwrap();
                log.rewrite(&path, live_map.clone().into_iter()).unwrap();
            });

            let _ = remove_file(path);
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_append_set,
    bench_append_del,
    bench_replay,
    bench_rewrite
);
criterion_main!(benches);
