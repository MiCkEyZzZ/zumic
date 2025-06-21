use std::fs::remove_file;

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::NamedTempFile;

use zumic::{InPersistentStore, Sds, Storage, Value};

fn bench_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistent_set");
    for size in [10usize, 100, 1000].iter() {
        group.bench_with_input(format!("size_{size}"), size, |b, &s| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            let store = InPersistentStore::new(&path).unwrap();
            let key = Sds::from_bytes(vec![b'k'; s]);
            let value = Value::Str(Sds::from_bytes(vec![b'v'; s]));
            b.iter(|| {
                store.set(&key, value.clone()).unwrap();
            });
            drop(store);
            let _ = remove_file(path);
        });
    }
    group.finish();
}

fn bench_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistent_get");
    let temp = NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let store = InPersistentStore::new(&path).unwrap();
    let key = Sds::from_str("key");
    let value = Value::Str(Sds::from_str("value"));
    store.set(&key, value.clone()).unwrap();
    group.bench_function("get_existing", |b| {
        b.iter(|| {
            let _ = store.get(&key).unwrap();
        });
    });
    group.finish();
}

fn bench_del(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistent_del");
    for size in [10, 100].iter() {
        group.bench_with_input(format!("size_{size}"), size, |b, &s| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            let store = InPersistentStore::new(&path).unwrap();
            let key = Sds::from_bytes(vec![b'k'; s]);
            let val = Value::Str(Sds::from_bytes(vec![b'v'; s]));
            store.set(&key, val).unwrap();
            b.iter(|| {
                store.del(&key).unwrap();
                store
                    .set(&key, Value::Str(Sds::from_bytes(vec![b'v'; s])))
                    .unwrap();
            });
            let _ = remove_file(path);
        });
    }
    group.finish();
}

fn bench_mset(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistent_mset");
    for count in [10usize, 100, 1000].iter() {
        group.bench_with_input(format!("count_{count}"), count, |b, &n| {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path().to_path_buf();
            let store = InPersistentStore::new(&path).unwrap();
            let entries: Vec<_> = (0..n)
                .map(|i| {
                    (
                        Sds::from_str(&format!("key{i}")),
                        Value::Str(Sds::from_str(&format!("val{i}"))),
                    )
                })
                .collect();
            b.iter(|| {
                store
                    .mset(
                        entries
                            .iter()
                            .map(|(k, v)| (k, v.clone()))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            });
            let _ = remove_file(path);
        });
    }
    group.finish();
}

criterion_group!(benches, bench_set, bench_get, bench_del, bench_mset);
criterion_main!(benches);
