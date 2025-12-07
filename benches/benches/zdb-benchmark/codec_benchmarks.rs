use std::{collections::HashSet, hint::black_box};

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, PlotConfiguration, Throughput,
};
use ordered_float::OrderedFloat;
use zumic::{
    database::Bitmap,
    engine::{read_dump, read_value, write_dump, write_value, TAG_STR},
    Dict, Hll, Sds, SkipList, SmartHash, Value, DENSE_SIZE,
};

// ============================================================================
// Helper functions для создания тестовых данных
// ============================================================================

fn create_string_value(size: usize) -> Value {
    Value::Str(Sds::from_vec(vec![b'a'; size]))
}

fn create_int_value() -> Value {
    Value::Int(42)
}

fn create_float_value() -> Value {
    Value::Float(std::f64::consts::PI)
}

fn create_bool_value() -> Value {
    Value::Bool(true)
}

fn create_null_value() -> Value {
    Value::Null
}

fn create_hash_value(entries: usize) -> Value {
    let mut map = SmartHash::new();
    for i in 0..entries {
        let key = Sds::from_vec(format!("key_{i}").into_bytes());
        let val = Sds::from_vec(format!("value_{i}").into_bytes());
        map.insert(key, val);
    }
    Value::Hash(map)
}

fn create_zset_value(entries: usize) -> Value {
    let mut dict = Dict::new();
    let mut sorted = SkipList::new();

    for i in 0..entries {
        let key = Sds::from_vec(format!("member_{i}").into_bytes());
        let score = i as f64;
        dict.insert(key.clone(), score);
        sorted.insert(OrderedFloat(score), key);
    }

    Value::ZSet { dict, sorted }
}

fn create_set_value(entries: usize) -> Value {
    let mut set = HashSet::new();
    for i in 0..entries {
        let member = Sds::from_vec(format!("member_{i}").into_bytes());
        set.insert(member);
    }
    Value::Set(set)
}

fn create_hll_value() -> Value {
    let mut data = [0u8; DENSE_SIZE];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }
    Value::HyperLogLog(Box::new(Hll { data }))
}

fn create_array_value(size: usize) -> Value {
    let mut items = Vec::with_capacity(size);
    for i in 0..size {
        items.push(Value::Int(i as i64));
    }
    Value::Array(items)
}

fn create_bitmap_value(size: usize) -> Value {
    let mut bm = Bitmap::new();
    bm.bytes = vec![0xFF; size];
    Value::Bitmap(bm)
}

// ============================================================================
// Benchmarks: Encode/Decode для каждого типа Value
// ============================================================================

fn bench_encode_decode_primitives(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode/primitives");

    // String (разные размеры)
    for size in [10, 100, 1000, 10_000].iter() {
        let value = create_string_value(*size);
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::new("string/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(
            BenchmarkId::new("string/decode", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_value(&mut cursor).unwrap());
                });
            },
        );
    }

    // Int
    let int_value = create_int_value();
    group.bench_function("int/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&int_value)).unwrap();
            black_box(buf);
        });
    });

    let mut int_encoded = Vec::new();
    write_value(&mut int_encoded, &int_value).unwrap();
    group.bench_function("int/decode", |b| {
        b.iter(|| {
            let mut cursor = std::io::Cursor::new(black_box(&int_encoded));
            black_box(read_value(&mut cursor).unwrap());
        });
    });

    // Float
    let float_value = create_float_value();
    group.bench_function("float/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&float_value)).unwrap();
            black_box(buf);
        });
    });

    // Bool
    let bool_value = create_bool_value();
    group.bench_function("bool/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&bool_value)).unwrap();
            black_box(buf);
        });
    });

    // Null
    let null_value = create_null_value();
    group.bench_function("null/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&null_value)).unwrap();
            black_box(buf);
        });
    });

    group.finish();
}

fn bench_encode_decode_collections(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode/collections");
    group.sample_size(50); // Уменьшаем для больших структур

    // Hash (разные размеры)
    for size in [10, 100, 1000, 10_000].iter() {
        let value = create_hash_value(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("hash/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(
            BenchmarkId::new("hash/decode", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_value(&mut cursor).unwrap());
                });
            },
        );
    }

    // ZSet
    for size in [10, 100, 1000, 10_000].iter() {
        let value = create_zset_value(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("zset/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(
            BenchmarkId::new("zset/decode", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_value(&mut cursor).unwrap());
                });
            },
        );
    }

    // Set
    for size in [10, 100, 1000, 10_000].iter() {
        let value = create_set_value(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("set/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(BenchmarkId::new("set/decode", size), &encoded, |b, data| {
            b.iter(|| {
                let mut cursor = std::io::Cursor::new(black_box(data));
                black_box(read_value(&mut cursor).unwrap());
            });
        });
    }

    group.finish();
}

fn bench_encode_decode_complex(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode/complex");
    group.sample_size(20);

    // HyperLogLog
    let hll_value = create_hll_value();
    group.bench_function("hll/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&hll_value)).unwrap();
            black_box(buf);
        });
    });

    let mut hll_encoded = Vec::new();
    write_value(&mut hll_encoded, &hll_value).unwrap();
    group.bench_function("hll/decode", |b| {
        b.iter(|| {
            let mut cursor = std::io::Cursor::new(black_box(&hll_encoded));
            black_box(read_value(&mut cursor).unwrap());
        });
    });

    // Array
    for size in [10, 100, 1000].iter() {
        let value = create_array_value(*size);
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("array/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(
            BenchmarkId::new("array/decode", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_value(&mut cursor).unwrap());
                });
            },
        );
    }

    // Bitmap
    for size in [100, 1000, 10_000].iter() {
        let value = create_bitmap_value(*size);
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::new("bitmap/encode", size), &value, |b, v| {
            b.iter(|| {
                let mut buf = Vec::new();
                write_value(&mut buf, black_box(v)).unwrap();
                black_box(buf);
            });
        });

        let mut encoded = Vec::new();
        write_value(&mut encoded, &value).unwrap();

        group.bench_with_input(
            BenchmarkId::new("bitmap/decode", size),
            &encoded,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_value(&mut cursor).unwrap());
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmarks: Compression levels
// ============================================================================

fn bench_compression_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");
    group.sample_size(20);
    group.plot_config(PlotConfiguration::default());

    // Создаём большой повторяющийся контент (хорошо сжимается)
    let compressible_data = create_string_value(100_000);

    // Создаём случайный контент (плохо сжимается)
    let random_data = {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        Value::Str(Sds::from_vec(data))
    };

    for data_type in ["compressible", "random"].iter() {
        let value = if *data_type == "compressible" {
            &compressible_data
        } else {
            &random_data
        };

        group.throughput(Throughput::Bytes(100_000));

        // Без сжатия (baseline)
        group.bench_with_input(
            BenchmarkId::new("no_compression", data_type),
            value,
            |b, v| {
                b.iter(|| {
                    let mut buf = Vec::new();
                    // Используем внутреннюю функцию без автосжатия
                    buf.push(TAG_STR);
                    let bytes = match v {
                        Value::Str(s) => s.as_bytes(),
                        _ => unreachable!(),
                    };
                    buf.extend(&(bytes.len() as u32).to_be_bytes());
                    buf.extend(bytes);
                    black_box(buf);
                });
            },
        );

        // Со сжатием (автоматическое, level по умолчанию)
        group.bench_with_input(
            BenchmarkId::new("with_compression", data_type),
            value,
            |b, v| {
                b.iter(|| {
                    let mut buf = Vec::new();
                    write_value(&mut buf, black_box(v)).unwrap();
                    black_box(buf);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmarks: Full dump operations
// ============================================================================

fn bench_dump_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("dump_operations");
    group.sample_size(20);

    for num_entries in [100, 1000, 10_000].iter() {
        // Создаём тестовый дамп
        let mut items = Vec::new();
        for i in 0..*num_entries {
            let key = Sds::from_vec(format!("key_{}", i).into_bytes());
            let value = create_hash_value(10); // Небольшие хеши
            items.push((key, value));
        }

        group.throughput(Throughput::Elements(*num_entries as u64));

        // Write dump
        group.bench_with_input(
            BenchmarkId::new("write_dump", num_entries),
            &items,
            |b, data| {
                b.iter(|| {
                    let mut buf = Vec::new();
                    write_dump(&mut buf, black_box(data.clone()).into_iter()).unwrap();
                    black_box(buf);
                });
            },
        );

        // Read dump
        let mut dump_data = Vec::new();
        write_dump(&mut dump_data, items.clone().into_iter()).unwrap();

        group.bench_with_input(
            BenchmarkId::new("read_dump", num_entries),
            &dump_data,
            |b, data| {
                b.iter(|| {
                    let mut cursor = std::io::Cursor::new(black_box(data));
                    black_box(read_dump(&mut cursor).unwrap());
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmarks: Memory usage (измеряем косвенно через throughput)
// ============================================================================

fn bench_memory_intensive(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_intensive");
    group.sample_size(10); // Ещё меньше для больших структур

    // Очень большой Hash
    let huge_hash = create_hash_value(100_000);
    group.bench_function("huge_hash/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&huge_hash)).unwrap();
            black_box(buf);
        });
    });

    // Очень большой ZSet
    let huge_zset = create_zset_value(100_000);
    group.bench_function("huge_zset/encode", |b| {
        b.iter(|| {
            let mut buf = Vec::new();
            write_value(&mut buf, black_box(&huge_zset)).unwrap();
            black_box(buf);
        });
    });

    group.finish();
}

// ============================================================================
// Criterion configuration
// ============================================================================

criterion_group!(
    benches,
    bench_encode_decode_primitives,
    bench_encode_decode_collections,
    bench_encode_decode_complex,
    bench_compression_levels,
    bench_dump_operations,
    bench_memory_intensive,
);

criterion_main!(benches);
