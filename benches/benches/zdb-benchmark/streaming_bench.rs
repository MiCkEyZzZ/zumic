use std::{hint::black_box, io::Cursor};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::{
    engine::{
        read_dump, write_dump, write_stream, CallbackHandler, CollectHandler, CountHandler,
        FilterHandler, StreamReader, StreamingParser,
    },
    Sds, SmartHash, Value,
};

fn create_test_dataset(num_entries: usize) -> Vec<(Sds, Value)> {
    let mut items = Vec::with_capacity(num_entries);

    for i in 0..num_entries {
        let key = Sds::from_vec(format!("key_{i:06}").into_bytes());

        // Чередуем типы значений для реалистичности
        let value = match i % 5 {
            0 => Value::Int(i as i64),
            1 => Value::Str(Sds::from_vec(format!("value_{i}").into_bytes())),
            2 => {
                let mut map = SmartHash::new();
                for j in 0..10 {
                    map.insert(
                        Sds::from_vec(format!("field_{j}").into_bytes()),
                        Sds::from_vec(format!("val_{j}").into_bytes()),
                    );
                }
                Value::Hash(map)
            }
            3 => Value::Float(i as f64 * 1.5),
            _ => Value::Bool(i % 2 == 0),
        };

        items.push((key, value));
    }

    items
}

fn prepare_dump_and_stream(num_entries: usize) -> (Vec<u8>, Vec<u8>) {
    // Генерируем данные отдельно для dump и для stream — это гарантирует, что
    // write_dump и write_stream получают корректные входные данные независимо.
    let items_for_dump = create_test_dataset(num_entries);
    let items_for_stream = create_test_dataset(num_entries);

    let mut dump_data = Vec::new();
    write_dump(&mut dump_data, items_for_dump.into_iter()).expect("write_dump failed");

    let mut stream_data = Vec::new();
    write_stream(&mut stream_data, items_for_stream.into_iter()).expect("write_stream failed");

    // Sanity checks: quick round-trip to catch format/CRC issues before the bench
    // run
    {
        let mut cur = Cursor::new(&dump_data);
        let items = read_dump(&mut cur).expect("sanity read_dump failed");
        assert_eq!(
            items.len(),
            num_entries,
            "sanity: dump entry count mismatch"
        );
    }
    {
        // For stream we use StreamingParser + CountHandler quick check
        let cur = Cursor::new(&stream_data);
        let mut parser = StreamingParser::new(cur).expect("sanity StreamingParser::new failed");
        let mut handler = CountHandler::new();
        parser
            .parse(&mut handler)
            .expect("sanity streaming parse failed");
        // handler.total_entries() -> u64, сравниваем с num_entries (usize)
        assert_eq!(
            handler.total_entries() as usize,
            num_entries,
            "sanity: stream entry count mismatch"
        );
    }

    (dump_data, stream_data)
}

fn bench_streaming_vs_buffered(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_vs_buffered");
    // уменьшил sample_size — при больших бенчах это ускоряет прогон; подгоняйте по
    // необходимости
    group.sample_size(20);

    for &num_entries in &[100usize, 1_000, 10_000, 50_000] {
        group.throughput(Throughput::Elements(num_entries as u64));

        // Подготавливаем оба формата и делаем sanity-checks внутри функции
        let (dump_data, stream_data) = prepare_dump_and_stream(num_entries);

        // Buffered (legacy read_dump) - читает всё в память сразу
        group.bench_with_input(
            BenchmarkId::new("buffered/read_dump", num_entries),
            &dump_data,
            |b, data| {
                b.iter(|| {
                    let mut cursor = Cursor::new(black_box(data));
                    // read_dump возвращает Vec<(Sds, Value)> — это и сравниваем/бенчим
                    black_box(read_dump(&mut cursor).unwrap());
                });
            },
        );

        // Streaming with CollectHandler (функционально эквивалентно read_dump но
        // потоково)
        group.bench_with_input(
            BenchmarkId::new("streaming/collect", num_entries),
            &stream_data,
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

        // Streaming with CountHandler (только счётчик, не загружает данные)
        group.bench_with_input(
            BenchmarkId::new("streaming/count_only", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let mut parser = StreamingParser::new(cursor).unwrap();
                    let mut handler = CountHandler::new();
                    parser.parse(&mut handler).unwrap();
                    black_box(handler.total_entries());
                });
            },
        );

        // Streaming with FilterHandler (загружает только ~10% записей)
        group.bench_with_input(
            BenchmarkId::new("streaming/filter_10pct", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let mut parser = StreamingParser::new(cursor).unwrap();
                    let mut handler = FilterHandler::new(|key: &Sds| {
                        // Фильтруем только ключи, оканчивающиеся на 0
                        key.as_bytes().last() == Some(&b'0')
                    });
                    parser.parse(&mut handler).unwrap();
                    black_box(handler.into_items());
                });
            },
        );

        // Streaming with CallbackHandler (обработка на лету без аллокаций)
        group.bench_with_input(
            BenchmarkId::new("streaming/callback_noalloc", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let mut parser = StreamingParser::new(cursor).unwrap();
                    let mut count = 0usize;
                    let mut handler = CallbackHandler::new(|_key, _value| {
                        // NB: не делаем heap-allocs внутри колбека
                        count += 1;
                        Ok(())
                    });
                    parser.parse(&mut handler).unwrap();
                    black_box(count);
                });
            },
        );
    }

    group.finish();
}

fn bench_stream_reader_iterator(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_reader");
    group.sample_size(20);

    for &num_entries in &[100usize, 1_000, 10_000] {
        group.throughput(Throughput::Elements(num_entries as u64));

        // Тут нужен только stream формат
        let (_dump_data, stream_data) = prepare_dump_and_stream(num_entries);

        // StreamReader с collect (загружает всё в Vec)
        group.bench_with_input(
            BenchmarkId::new("iterator/collect_all", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let reader = StreamReader::new(cursor).unwrap();
                    let items: Vec<_> = reader.map(|r| r.unwrap()).collect();
                    black_box(items);
                });
            },
        );

        // StreamReader с filter и collect (фильтрация в итераторе)
        group.bench_with_input(
            BenchmarkId::new("iterator/filter_collect", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let reader = StreamReader::new(cursor).unwrap();
                    let items: Vec<_> = reader
                        .filter_map(|r| r.ok())
                        .filter(|(key, _)| key.as_bytes().last() == Some(&b'0'))
                        .collect();
                    black_box(items);
                });
            },
        );

        // StreamReader с count (только подсчёт, без аллокаций)
        group.bench_with_input(
            BenchmarkId::new("iterator/count_only", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let reader = StreamReader::new(cursor).unwrap();
                    let count = reader.count();
                    black_box(count);
                });
            },
        );

        // StreamReader с for_each (обработка на лету)
        group.bench_with_input(
            BenchmarkId::new("iterator/for_each", num_entries),
            &stream_data,
            |b, data| {
                b.iter(|| {
                    let cursor = Cursor::new(black_box(data));
                    let reader = StreamReader::new(cursor).unwrap();
                    let mut sum = 0i64;
                    reader.for_each(|r| {
                        if let Ok((_, Value::Int(i))) = r {
                            sum += i;
                        }
                    });
                    black_box(sum);
                });
            },
        );
    }

    group.finish();
}

fn bench_memory_pressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pressure");
    group.sample_size(10);

    // Большой дамп с большими значениями (используем 10_000 записей как пример)
    let num_entries = 10_000usize;
    let (dump_data, stream_data) = prepare_dump_and_stream(num_entries);

    group.throughput(Throughput::Bytes(dump_data.len() as u64));

    // Buffered - загружает весь дамп в память
    group.bench_function("large_dump/buffered", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&dump_data));
            black_box(read_dump(&mut cursor).unwrap());
        });
    });

    // Streaming - обрабатывает порциями
    group.bench_function("large_dump/streaming_count", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&stream_data));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = CountHandler::new();
            parser.parse(&mut handler).unwrap();
            black_box(handler.total_entries());
        });
    });

    // Streaming с частичной загрузкой (пример ~1%)
    group.bench_function("large_dump/streaming_partial", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&stream_data));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = FilterHandler::new(|key: &Sds| {
                // Загружаем только 1% данных: ключи заканчиваются на "00"
                let bytes = key.as_bytes();
                bytes.last() == Some(&b'0')
                    && bytes.get(bytes.len().saturating_sub(2)) == Some(&b'0')
            });
            parser.parse(&mut handler).unwrap();
            black_box(handler.into_items());
        });
    });

    group.finish();
}

fn bench_parse_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_overhead");
    group.sample_size(50);

    // Маленький дамп для измерения overhead парсера
    let num_entries = 10usize;
    let (dump_data, stream_data) = prepare_dump_and_stream(num_entries);

    // Baseline: read_dump
    group.bench_function("small/buffered", |b| {
        b.iter(|| {
            let mut cursor = Cursor::new(black_box(&dump_data));
            black_box(read_dump(&mut cursor).unwrap());
        });
    });

    // StreamingParser overhead
    group.bench_function("small/streaming", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&stream_data));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = CollectHandler::new();
            parser.parse(&mut handler).unwrap();
            black_box(handler.into_items());
        });
    });

    // StreamReader overhead
    group.bench_function("small/reader", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&stream_data));
            let reader = StreamReader::new(cursor).unwrap();
            let items: Vec<_> = reader.map(|r| r.unwrap()).collect();
            black_box(items);
        });
    });

    group.finish();
}

fn bench_data_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_patterns");
    group.sample_size(20);

    let num_entries = 1000;

    // Pattern 1: Все Int
    let int_items: Vec<_> = (0..num_entries)
        .map(|i| {
            (
                Sds::from_vec(format!("key_{i}").into_bytes()),
                Value::Int(i as i64),
            )
        })
        .collect();

    let mut int_stream = Vec::new();
    write_stream(&mut int_stream, int_items.into_iter()).unwrap();

    group.bench_function("all_ints/streaming", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&int_stream));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = CollectHandler::new();
            parser.parse(&mut handler).unwrap();
            black_box(handler.into_items());
        });
    });

    // Pattern 2: Все Hash
    let hash_items: Vec<_> = (0..num_entries)
        .map(|i| {
            let mut map = SmartHash::new();
            for j in 0..20 {
                map.insert(
                    Sds::from_vec(format!("f_{}", j).into_bytes()),
                    Sds::from_vec(format!("v_{}_{}", i, j).into_bytes()),
                );
            }
            (
                Sds::from_vec(format!("key_{}", i).into_bytes()),
                Value::Hash(map),
            )
        })
        .collect();

    let mut hash_stream = Vec::new();
    write_stream(&mut hash_stream, hash_items.into_iter()).unwrap();

    group.bench_function("all_hashes/streaming", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&hash_stream));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = CollectHandler::new();
            parser.parse(&mut handler).unwrap();
            black_box(handler.into_items());
        });
    });

    // Pattern 3: Смешанные типы
    let mixed_items = create_test_dataset(num_entries);
    let mut mixed_stream = Vec::new();
    write_stream(&mut mixed_stream, mixed_items.into_iter()).unwrap();

    group.bench_function("mixed_types/streaming", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&mixed_stream));
            let mut parser = StreamingParser::new(cursor).unwrap();
            let mut handler = CollectHandler::new();
            parser.parse(&mut handler).unwrap();
            black_box(handler.into_items());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_streaming_vs_buffered,
    bench_stream_reader_iterator,
    bench_memory_pressure,
    bench_parse_overhead,
    bench_data_patterns,
);
criterion_main!(benches);
