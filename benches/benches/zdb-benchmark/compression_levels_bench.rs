//! Детальный бенчмарк ZSTD compression levels 1-22
//!
//! Измеряет:
//! - Скорость сжатия/распаковки для каждого уровня
//! - Compression ratio
//! - Throughput

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::Sds;

fn create_compressible_data(size: usize) -> Vec<u8> {
    let pattern = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
    let mut data = Vec::with_capacity(size);
    while data.len() < size {
        data.extend_from_slice(pattern);
    }
    data.truncate(size);
    data
}

fn create_random_data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((i * 6364136223846793005u64 as usize + 1) >> 32) as u8)
        .collect()
}

fn compress_with_level(
    data: &[u8],
    level: i32,
) -> Vec<u8> {
    zstd::encode_all(data, level).expect("compression failed")
}

fn decompress(data: &[u8]) -> Vec<u8> {
    zstd::decode_all(data).expect("decompression failed")
}

fn bench_zstd_compression_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_levels");
    group.sample_size(50);

    // Тестируем разные размеры данных
    for data_size in [1024, 10_240, 102_400, 1_024_000] {
        group.throughput(Throughput::Bytes(data_size as u64));

        // Compressible data
        let compressible = create_compressible_data(data_size);

        // Random data
        let random = create_random_data(data_size);

        for data_type in ["compressible", "random"] {
            let data = if data_type == "compressible" {
                &compressible
            } else {
                &random
            };

            // Benchmark каждый уровень от 1 до 22
            for level in [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 22] {
                // Compression
                group.bench_with_input(
                    BenchmarkId::new(format!("{data_type}/compress/level_{level}"), data_size),
                    data,
                    |b, d| {
                        b.iter(|| {
                            let compressed = compress_with_level(black_box(d), level);
                            black_box(compressed);
                        });
                    },
                );

                // Decompression (сжимаем заранее)
                let compressed = compress_with_level(data, level);
                group.bench_with_input(
                    BenchmarkId::new(format!("{data_type}/decompress/level_{level}"), data_size),
                    &compressed,
                    |b, d| {
                        b.iter(|| {
                            let decompressed = decompress(black_box(d));
                            black_box(decompressed);
                        });
                    },
                );
            }
        }
    }

    group.finish();
}

fn bench_compression_ratio(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_ratio");
    group.sample_size(20);

    let data_size = 100_000;
    let compressible = create_compressible_data(data_size);
    let random = create_random_data(data_size);

    println!("\n=== Compression Ratio Analysis ===");
    println!("Original size: {data_size} bytes");
    println!(
        "\n{:<6} {:<20} {:<20} {:<15} {:<15}",
        "Level", "Compressible (bytes)", "Random (bytes)", "Comp. Ratio", "Rand. Ratio"
    );
    println!("{:-<80}", "");

    for level in 1..=22 {
        let comp_compressed = compress_with_level(&compressible, level);
        let rand_compressed = compress_with_level(&random, level);

        let comp_ratio = data_size as f64 / comp_compressed.len() as f64;
        let rand_ratio = data_size as f64 / rand_compressed.len() as f64;

        println!(
            "{:<6} {:<20} {:<20} {:<15.2} {:<15.2}",
            level,
            comp_compressed.len(),
            rand_compressed.len(),
            comp_ratio,
            rand_ratio
        );
    }

    group.finish();
}

fn bench_zumic_value_compression(c: &mut Criterion) {
    use zumic::{SmartHash, Value};

    let mut group = c.benchmark_group("zstd_zumic_values");
    group.sample_size(30);

    // Создаём реалистичные Value для бенчмарка
    let mut large_hash = SmartHash::new();
    for i in 0..1000 {
        large_hash.insert(
            Sds::from_vec(format!("field_{i}").into_bytes()),
            Sds::from_vec(format!("value_{i}").into_bytes()),
        );
    }
    let hash_value = Value::Hash(large_hash);

    // Сериализуем Value в bytes
    let mut value_bytes = Vec::new();
    zumic::engine::write_value_no_compress(&mut value_bytes, &hash_value)
        .expect("serialization failed");

    for level in [1, 5, 9, 13, 17, 22] {
        group.bench_with_input(
            BenchmarkId::new("hash_1000_fields/compress", level),
            &level,
            |b, &lvl| {
                b.iter(|| {
                    let compressed = compress_with_level(black_box(&value_bytes), lvl);
                    black_box(compressed);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_zstd_compression_levels,
    bench_compression_ratio,
    bench_zumic_value_compression,
);
criterion_main!(benches);
