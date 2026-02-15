use std::{hint::black_box, io::Cursor};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::Rng;
use zumic::engine::varint::{read_varint, varint_is_efficient, varint_size, write_varint};

fn encoding_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("encoding");

    // Тестовые значения для каждого размера varint
    let test_cases = vec![
        ("1_byte", 127u32),
        ("2_bytes_low", 128u32),
        ("2_bytes_high", 16383u32),
        ("3_bytes_low", 16384u32),
        ("3_bytes_high", 2097151u32),
        ("4_bytes_low", 2097152u32),
        ("4_bytes_high", 268435455u32),
        ("5_bytes_low", 268435456u32),
        ("5_bytes_high", u32::MAX),
    ];

    for (name, value) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("write_varint", name),
            &value,
            |b, &value| {
                let mut buf = Vec::with_capacity(5);
                b.iter(|| {
                    buf.clear();
                    write_varint(black_box(&mut buf), black_box(value)).unwrap();
                });
            },
        );
    }

    group.finish();
}

fn decoding_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoding");

    let test_cases = vec![
        ("1_byte", 127u32),
        ("2_bytes", 16383u32),
        ("3_bytes", 2097151u32),
        ("4_bytes", 268435455u32),
        ("5_bytes", u32::MAX),
    ];

    // Заранее кодируем значения
    let encoded: Vec<Vec<u8>> = test_cases
        .iter()
        .map(|&(_, value)| {
            let mut buf = Vec::new();
            write_varint(&mut buf, value).unwrap();
            buf
        })
        .collect();

    for ((name, _value), encoded_buf) in test_cases.iter().zip(encoded.iter()) {
        group.bench_with_input(
            BenchmarkId::new("read_varint", name),
            encoded_buf,
            |b, buf| {
                b.iter(|| {
                    let mut cursor = Cursor::new(black_box(buf));
                    black_box(read_varint(black_box(&mut cursor)).unwrap());
                });
            },
        );
    }

    group.finish();
}

fn size_calculation_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("size_calculation");

    let test_cases = vec![
        ("0", 0u32),
        ("1_byte", 127u32),
        ("2_bytes", 16383u32),
        ("3_bytes", 2097151u32),
        ("4_bytes", 268435455u32),
        ("5_bytes", u32::MAX),
    ];

    for (name, value) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("varint_size", name),
            &value,
            |b, &value| {
                b.iter(|| {
                    black_box(varint_size(black_box(value)));
                });
            },
        );
    }

    group.finish();
}

fn efficiency_check_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("efficiency_check");

    let test_cases = vec![
        ("efficient_1", 127u32),
        ("efficient_2", 16383u32),
        ("efficient_3", 2097151u32),
        ("inefficient_4", 2097152u32),
        ("inefficient_5", u32::MAX),
    ];

    for (name, value) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("varint_is_efficient", name),
            &value,
            |b, &value| {
                b.iter(|| {
                    black_box(varint_is_efficient(black_box(value)));
                });
            },
        );
    }

    group.finish();
}

fn roundtrip_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("roundtrip");

    let test_cases = vec![
        ("small", 42u32),
        ("medium", 100_000u32),
        ("large", u32::MAX),
    ];

    for (name, value) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("encode_decode", name),
            &value,
            |b, &value| {
                b.iter(|| {
                    let mut buf = Vec::with_capacity(5);
                    write_varint(&mut buf, black_box(value)).unwrap();
                    let mut cursor = Cursor::new(&buf);
                    let decoded = read_varint(&mut cursor).unwrap();
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

fn batch_encoding_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_operations");

    // Создаем вектор из 1000 случайных чисел разного размера
    let mut rng = rand::thread_rng();
    let values: Vec<u32> = (0..1000)
        .map(|i| {
            match i % 5 {
                0 => rng.gen_range(0..128),               // 1 байт
                1 => rng.gen_range(128..16384),           // 2 байта
                2 => rng.gen_range(16384..2097152),       // 3 байта
                3 => rng.gen_range(2097152..268435456),   // 4 байта
                _ => rng.gen_range(268435456..=u32::MAX), // 5 байт
            }
        })
        .collect();

    group.bench_function("encode_1000_random_values", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(3000); // Примерный размер
            for &value in values.iter() {
                write_varint(&mut buf, black_box(value)).unwrap();
            }
            black_box(buf);
        });
    });

    // Заранее кодируем значения для теста декодирования
    let encoded_bufs: Vec<Vec<u8>> = values
        .iter()
        .map(|&value| {
            let mut buf = Vec::new();
            write_varint(&mut buf, value).unwrap();
            buf
        })
        .collect();

    group.bench_function("decode_1000_random_values", |b| {
        b.iter(|| {
            let mut total = 0u32;
            for buf in encoded_bufs.iter() {
                let mut cursor = Cursor::new(buf);
                total = total.wrapping_add(read_varint(&mut cursor).unwrap());
            }
            black_box(total);
        });
    });

    group.finish();
}

fn comparison_with_fixed_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison");

    let values = vec![
        ("small", 42u32),
        ("medium", 100_000u32),
        ("large", u32::MAX),
    ];

    // Сравнение с обычной записью u32 в little-endian
    for (name, value) in values {
        group.bench_with_input(
            BenchmarkId::new("varint_encode", name),
            &value,
            |b, &value| {
                b.iter(|| {
                    let mut buf = Vec::with_capacity(5);
                    write_varint(&mut buf, black_box(value)).unwrap();
                    black_box(buf);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("fixed_u32_encode", name),
            &value,
            |b, &value| {
                b.iter(|| {
                    let buf = black_box(value).to_le_bytes();
                    black_box(buf);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(1000)
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2));
    targets =
        encoding_benchmark,
        decoding_benchmark,
        size_calculation_benchmark,
        efficiency_check_benchmark,
        roundtrip_benchmark,
        batch_encoding_benchmark,
        comparison_with_fixed_size
);

criterion_main!(benches);
