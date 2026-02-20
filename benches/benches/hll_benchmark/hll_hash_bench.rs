use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::database::{HllHasher, MurmurHasher, SipHasher, XxHasher};

fn make_payload(
    size: usize,
    seed: u64,
) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..size).map(|_| rng.gen::<u8>()).collect()
}

fn bench_single_hash<H: HllHasher + 'static + Send + Sync>(
    c: &mut Criterion,
    name: &str,
    payload_size: usize,
) {
    let payload = make_payload(payload_size, 0xfeedu64);

    let mut group = c.benchmark_group(format!("hash/single/{}b", payload_size));
    group.throughput(criterion::Throughput::Bytes(payload_size as u64));
    group.bench_function(BenchmarkId::new(name, ""), |b| {
        let hasher = H::default();
        b.iter(|| {
            // black_box чтобы избежать оптимизации
            black_box(hasher.hash_bytes(black_box(&payload)));
        })
    });
    group.finish();
}

fn bench_batch_hash<H: HllHasher + 'static + Send + Sync>(
    c: &mut Criterion,
    name: &str,
    payload_size: usize,
    batch: usize,
) {
    // Подготовка пакетную обработку различных полезных нагрузок, чтобы избежать
    // случайных попаданий в кэш.
    let payloads: Vec<Vec<u8>> = (0..batch)
        .map(|i| make_payload(payload_size, 0xDEAD_0000u64 + i as u64))
        .collect();

    let mut group = c.benchmark_group(format!("hash/batch/{}b_x{}", payload_size, batch));
    group.bench_function(BenchmarkId::new(name, ""), |b| {
        let hasher = H::default();
        b.iter(|| {
            for p in &payloads {
                black_box(hasher.hash_bytes(black_box(p)));
            }
        })
    });
    group.finish();
}

pub fn hashers_comparison(c: &mut Criterion) {
    // Размеры выбраны с учетом типичной рабочей нагрузки HLL:
    // малый — 16 байт (идентификатор, короткая клавиша)
    // средний — 64 байта
    // большой — 1024 байта (стрессовая нагрузка)
    let sizes = [16usize, 64usize, 1024usize];

    // Микростенд для одного вызова для каждого молотка и каждого размера
    for &size in &sizes {
        bench_single_hash::<MurmurHasher>(c, "murmur", size);
        bench_single_hash::<XxHasher>(c, "xxhash", size);
        bench_single_hash::<SipHasher>(c, "siphash", size);
    }

    // Тесты производительности пакетной обработки — моделирование пропускной
    // способности на «горячем» пути
    let batch = 1024usize; // количество хешей за итерацию
    for &size in &[16usize, 64usize] {
        bench_batch_hash::<MurmurHasher>(c, "murmur", size, batch);
        bench_batch_hash::<XxHasher>(c, "xxhash", size, batch);
        bench_batch_hash::<SipHasher>(c, "siphash", size, batch);
    }

    // Дополнительно: небольшой bench для сравнения трех групп для получения точных
    // числовых данных.
    let payload = make_payload(64, 0xBEEF);
    let mut group = c.benchmark_group("hash/compare/64b");
    group.bench_function("murmur", |b| {
        let h = MurmurHasher::default();
        b.iter(|| black_box(h.hash_bytes(black_box(&payload))));
    });
    group.bench_function("xxhash", |b| {
        let h = XxHasher::default();
        b.iter(|| black_box(h.hash_bytes(black_box(&payload))));
    });
    group.bench_function("siphash", |b| {
        let h = SipHasher::default();
        b.iter(|| black_box(h.hash_bytes(black_box(&payload))));
    });
    group.finish();
}

criterion_group!(benches, hashers_comparison);
criterion_main!(benches);
