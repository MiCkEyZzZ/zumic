use std::{
    fs::File,
    hint::black_box,
    io::{BufReader, Cursor, Read},
    process::{Command, Stdio},
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rdb::{filter::Simple, formatter::Nil, parse};
use zumic::{
    engine::{read_dump, write_dump, write_stream, CollectHandler, StreamingParser},
    Sds, SmartHash, Value,
};

fn create_test_dataset(num_entries: usize) -> Vec<(Sds, Value)> {
    let mut items = Vec::with_capacity(num_entries);

    for i in 0..num_entries {
        let key = Sds::from_vec(format!("key:{i:06}").into_bytes());

        let value = match i % 4 {
            0 => Value::Str(Sds::from_vec(format!("string_value_{i}").into_bytes())),
            1 => Value::Int(i as i64),
            2 => {
                let mut map = SmartHash::new();
                for j in 0..10 {
                    map.insert(
                        Sds::from_vec(format!("field{j}").into_bytes()),
                        Sds::from_vec(format!("val{j}").into_bytes()),
                    );
                }
                Value::Hash(map)
            }
            _ => Value::Float(i as f64 * 1.5),
        };

        items.push((key, value));
    }

    items
}

fn is_redis_available() -> bool {
    Command::new("redis-cli")
        .arg("PING")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn make_redis_rdb_dump(dataset: &[(Sds, Value)]) -> Option<Vec<u8>> {
    if !is_redis_available() {
        return None;
    }

    Command::new("redis-cli").arg("FLUSHALL").status().ok()?;

    for (key, value) in dataset {
        let key_str = String::from_utf8_lossy(&key.to_vec()).to_string();

        match value {
            Value::Str(s) => {
                let s_str = String::from_utf8_lossy(&s.to_vec()).to_string();
                Command::new("redis-cli")
                    .arg("SET")
                    .arg(&key_str)
                    .arg(&s_str)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .ok()?;
            }
            Value::Int(n) => {
                Command::new("redis-cli")
                    .arg("SET")
                    .arg(&key_str)
                    .arg(n.to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .ok()?;
            }
            Value::Hash(map) => {
                let mut map = map.clone();
                for (f, v) in map.iter() {
                    let f_str = String::from_utf8_lossy(&f.to_vec()).to_string();
                    let v_str = String::from_utf8_lossy(&v.to_vec()).to_string();
                    Command::new("redis-cli")
                        .arg("HSET")
                        .arg(&key_str)
                        .arg(&f_str)
                        .arg(&v_str)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .ok()?;
                }
            }
            Value::Float(f) => {
                Command::new("redis-cli")
                    .arg("SET")
                    .arg(&key_str)
                    .arg(f.to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .ok()?;
            }
            _ => {}
        }
    }

    Command::new("redis-cli").arg("SAVE").status().ok()?;

    let output = Command::new("redis-cli")
        .arg("CONFIG")
        .arg("GET")
        .arg("dir")
        .output()
        .ok()?;

    let dir_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = dir_str.lines().collect();
    let redis_dir = if lines.len() >= 2 {
        lines[1]
    } else {
        "/var/lib/redis"
    };

    let dump_path = format!("{redis_dir}/dump.rdb");

    let mut file = File::open(&dump_path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;

    Some(buf)
}

fn bench_zdb_vs_redis_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("zdb_vs_redis");
    group.sample_size(30);

    for &num_entries in &[100usize, 1_000, 10_000] {
        group.throughput(Throughput::Elements(num_entries as u64));

        let items = create_test_dataset(num_entries);

        // ZDB: buffered read_dump
        let mut zdb_dump = Vec::new();
        write_dump(&mut zdb_dump, items.clone().into_iter()).expect("ZDB write_dump failed");

        group.bench_with_input(
            BenchmarkId::new("zdb/buffered", num_entries),
            &zdb_dump,
            |b, data| {
                b.iter(|| {
                    let mut cursor = Cursor::new(black_box(data));
                    black_box(read_dump(&mut cursor).unwrap());
                });
            },
        );

        // ZDB: streaming parser
        let mut zdb_stream = Vec::new();
        write_stream(&mut zdb_stream, items.clone().into_iter()).expect("ZDB write_stream failed");

        group.bench_with_input(
            BenchmarkId::new("zdb/streaming", num_entries),
            &zdb_stream,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let mut parser = StreamingParser::new(cursor).unwrap();
                    let mut handler = CollectHandler::new();
                    parser.parse(&mut handler).unwrap();
                    black_box(handler.into_items());
                });
            },
        );

        // Redis RDB: rdb-rs parser
        if let Some(rdb_dump) = make_redis_rdb_dump(&items) {
            group.bench_with_input(
                BenchmarkId::new("redis/rdb_rs", num_entries),
                &rdb_dump,
                |b, data| {
                    b.iter(|| {
                        let cursor = Cursor::new(black_box(data));
                        let reader = BufReader::new(cursor);
                        parse(reader, Nil::new(None), Simple::new()).expect("RDB parse failed");
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_format_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_overhead");
    group.sample_size(50);

    let items = vec![(Sds::from_vec(b"key".to_vec()), Value::Int(42))];

    let mut zdb_dump = Vec::new();
    write_dump(&mut zdb_dump, items.clone().into_iter()).unwrap();

    group.bench_function("zdb/single_entry", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&zdb_dump));
            black_box(read_dump(&mut cursor).unwrap());
        });
    });

    if let Some(rdb_dump) = make_redis_rdb_dump(&items) {
        group.bench_function("redis/single_entry", |b| {
            b.iter(|| {
                let cursor = Cursor::new(black_box(&rdb_dump));
                let reader = BufReader::new(cursor);
                parse(reader, Nil::new(None), Simple::new()).unwrap();
            });
        });
    }

    group.finish();
}

fn bench_large_values(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_values");
    group.sample_size(20);

    let mut large_hash = SmartHash::new();
    for i in 0..10_000 {
        large_hash.insert(
            Sds::from_vec(format!("f{i}").into_bytes()),
            Sds::from_vec(format!("v{i}").into_bytes()),
        );
    }

    let items = vec![(
        Sds::from_vec(b"large_hash".to_vec()),
        Value::Hash(large_hash),
    )];

    let mut zdb_dump = Vec::new();
    write_dump(&mut zdb_dump, items.clone().into_iter()).unwrap();

    group.throughput(Throughput::Bytes(zdb_dump.len() as u64));

    group.bench_function("zdb/hash_10k_fields", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&zdb_dump));
            black_box(read_dump(&mut cursor).unwrap());
        });
    });

    if let Some(rdb_dump) = make_redis_rdb_dump(&items) {
        group.bench_function("redis/hash_10k_fields", |b| {
            b.iter(|| {
                let cursor = Cursor::new(black_box(&rdb_dump));
                let reader = BufReader::new(cursor);
                parse(reader, Nil::new(None), Simple::new()).unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_zdb_vs_redis_parsing,
    bench_format_overhead,
    bench_large_values,
);
criterion_main!(benches);
