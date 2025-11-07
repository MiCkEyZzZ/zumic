use std::{collections::HashSet, hint::black_box, io::Cursor};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::{
    engine::{read_dump, read_value_with_version, write_dump, write_value, FormatVersion},
    Sds, SmartHash, Value,
};

// Генераторы тестовых данных
fn generate_string(size: usize) -> Value {
    let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    Value::Str(Sds::from_vec(data))
}

fn generate_hash(entries: usize) -> Value {
    let mut map = SmartHash::new();
    for i in 0..entries {
        map.insert(
            Sds::from_str(&format!("field_{}", i)),
            Sds::from_str(&format!("value_{}", i)),
        );
    }
    Value::Hash(map)
}

fn generate_set(entries: usize) -> Value {
    let mut set = HashSet::new();
    for i in 0..entries {
        set.insert(Sds::from_str(&format!("member_{}", i)));
    }
    Value::Set(set)
}

fn generate_array(size: usize) -> Value {
    let items: Vec<Value> = (0..size).map(|i| Value::Int(i as i64)).collect();
    Value::Array(items)
}

// Benchmark encode/decode для разных типов
fn bench_value_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("value_roundtrip");

    // String - разные размеры
    for size in [64, 1024, 1024 * 16, 1024 * 64].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("string", size), size, |b, &size| {
            let value = generate_string(size);
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(&value)).unwrap();
                let mut cursor = Cursor::new(buf);
                let _decoded =
                    read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
            });
        });
    }

    // Hash - разные размеры
    for entries in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("hash", entries), entries, |b, &entries| {
            let value = generate_hash(entries);
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(&value)).unwrap();
                let mut cursor = Cursor::new(buf);
                let _decoded =
                    read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
            });
        });
    }

    // Set - разные размеры
    for entries in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("set", entries), entries, |b, &entries| {
            let value = generate_set(entries);
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(&value)).unwrap();
                let mut cursor = Cursor::new(buf);
                let _decoded =
                    read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
            });
        });
    }

    // Array - разные размеры
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("array", size), size, |b, &size| {
            let value = generate_array(size);
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(&value)).unwrap();
                let mut cursor = Cursor::new(buf);
                let _decoded =
                    read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
            });
        });
    }

    group.finish();
}

// Benchmark только encode
fn bench_encode_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_only");

    let string_64k = generate_string(64 * 1024);
    let hash_1000 = generate_hash(1000);
    let array_1000 = generate_array(1000);

    group.bench_function("string_64k", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&string_64k)).unwrap();
        });
    });

    group.bench_function("hash_1000", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&hash_1000)).unwrap();
        });
    });

    group.bench_function("array_1000", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&array_1000)).unwrap();
        });
    });

    group.finish();
}

// Benchmark только decode
fn bench_decode_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_only");

    // Подготовка данных
    let mut string_buf = Vec::new();
    write_value(&mut string_buf, &generate_string(64 * 1024)).unwrap();

    let mut hash_buf = Vec::new();
    write_value(&mut hash_buf, &generate_hash(1000)).unwrap();

    let mut array_buf = Vec::new();
    write_value(&mut array_buf, &generate_array(1000)).unwrap();

    group.bench_function("string_64k", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&string_buf));
            let _decoded = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        });
    });

    group.bench_function("hash_1000", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&hash_buf));
            let _decoded = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        });
    });

    group.bench_function("array_1000", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&array_buf));
            let _decoded = read_value_with_version(&mut cursor, FormatVersion::current()).unwrap();
        });
    });

    group.finish();
}

// Benchmark compression - с сжатием и без
fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");

    // Большая строка которая хорошо сжимается
    let large_compressible = generate_string(128 * 1024);

    group.throughput(Throughput::Bytes(128 * 1024));
    group.bench_function("with_compression", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&large_compressible)).unwrap();
        });
    });

    group.finish();
}

// Benchmark dump операций
fn bench_dump_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("dump_operations");

    // Генерируем дамп с разным количеством записей
    for count in [100, 1000, 10000].iter() {
        let items: Vec<(Sds, Value)> = (0..*count)
            .map(|i| (Sds::from_str(&format!("key_{}", i)), Value::Int(i as i64)))
            .collect();

        group.bench_with_input(BenchmarkId::new("write_dump", count), count, |b, _| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_dump(&mut buf, black_box(items.clone()).into_iter()).unwrap();
            });
        });

        // Подготовка для read
        let mut dump_buf = Vec::new();
        write_dump(&mut dump_buf, items.into_iter()).unwrap();

        group.bench_with_input(BenchmarkId::new("read_dump", count), count, |b, _| {
            b.iter(|| {
                let mut cursor = Cursor::new(black_box(&dump_buf));
                let _items = read_dump(&mut cursor).unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_value_roundtrip,
    bench_encode_only,
    bench_decode_only,
    bench_compression,
    bench_dump_operations
);
criterion_main!(benches);
