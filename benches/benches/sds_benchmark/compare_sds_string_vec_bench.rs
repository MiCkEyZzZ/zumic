use std::{hint::black_box, time::Duration};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::Sds;

fn make_ascii(size: usize) -> String {
    "a".repeat(size)
}

/// Добавлять фрагмент данных многократно, пока не будет достигнуто значение
/// `target_len`.
fn append_to_target_string(
    s: &mut String,
    chunk: &str,
    target_len: usize,
) {
    while s.len() < target_len {
        s.push_str(chunk);
    }
}

fn append_to_target_vec(
    v: &mut Vec<u8>,
    chunk: &[u8],
    target_len: usize,
) {
    while v.len() < target_len {
        v.extend_from_slice(chunk);
    }
}

fn append_to_target_sds(
    s: &mut Sds,
    chunk: &[u8],
    target_len: usize,
) {
    while s.len() < target_len {
        s.append(chunk);
    }
}

pub fn bench_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("sds_vs_string_vec");
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(80);

    // Determine sizes: short should be < INLINE_CAP
    let inline_cap = Sds::INLINE_CAP;
    let short = std::cmp::max(1, inline_cap.saturating_sub(4)); // well under inline cap
    let medium = 64usize;
    let long = 1024usize;
    let very_long = 4096usize;

    let sizes = [short, medium, long, very_long];

    // --- Creation from &str / &[u8] ---
    for &size in &sizes {
        let input = make_ascii(size);
        let id_base = format!("create/{}B", size);

        group.throughput(Throughput::Bytes(size as u64));

        // String creation
        group.bench_with_input(
            BenchmarkId::new("String::from", &id_base),
            &input,
            |b, s| {
                b.iter(|| {
                    let x = black_box(s.to_string());
                    black_box(x);
                })
            },
        );

        // Vec<u8> creation
        group.bench_with_input(BenchmarkId::new("Vec::from", &id_base), &input, |b, s| {
            b.iter(|| {
                let x = black_box(s.as_bytes().to_vec());
                black_box(x);
            })
        });

        // Sds creation
        group.bench_with_input(
            BenchmarkId::new("Sds::from_str", &id_base),
            &input,
            |b, s| {
                b.iter(|| {
                    let x = black_box(Sds::from_str(s));
                    black_box(x);
                })
            },
        );
    }

    // --- Append to target size (amortized growth; tests inline->heap transition)
    // --- use a small chunk to simulate incremental appends
    for &size in &sizes {
        let chunk = "abcdefghijklmnop"; // 16 bytes chunk (valid UTF-8)
        let chunk_bytes = chunk.as_bytes();
        let id_base = format!("append_to_{}B", size);

        group.throughput(Throughput::Bytes(size as u64));

        // String append
        group.bench_with_input(
            BenchmarkId::new("String::append", &id_base),
            &size,
            |b, &target| {
                b.iter_batched(
                    || String::new(),
                    |mut s| {
                        append_to_target_string(&mut s, chunk, target);
                        black_box(s);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );

        // Vec append
        group.bench_with_input(
            BenchmarkId::new("Vec::append", &id_base),
            &size,
            |b, &target| {
                b.iter_batched(
                    || Vec::<u8>::new(),
                    |mut v| {
                        append_to_target_vec(&mut v, chunk_bytes, target);
                        black_box(v);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );

        // Sds append
        group.bench_with_input(
            BenchmarkId::new("Sds::append", &id_base),
            &size,
            |b, &target| {
                b.iter_batched(
                    || Sds::default(),
                    |mut s| {
                        append_to_target_sds(&mut s, chunk_bytes, target);
                        black_box(s);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }

    // --- Clone existing container (warm state) ---
    for &size in &sizes {
        let payload = make_ascii(size);

        // prepare bases
        let base_string = payload.clone();
        let base_vec = payload.as_bytes().to_vec();
        let base_sds = Sds::from_str(&payload);

        let id_base = format!("clone/{}B", size);
        group.throughput(Throughput::Elements(1));

        group.bench_with_input(
            BenchmarkId::new("String::clone", &id_base),
            &base_string,
            |b, s| {
                b.iter(|| {
                    let c = black_box(s.clone());
                    black_box(c);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Vec::clone", &id_base),
            &base_vec,
            |b, v| {
                b.iter(|| {
                    let c = black_box(v.clone());
                    black_box(c);
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Sds::clone", &id_base),
            &base_sds,
            |b, s| {
                b.iter(|| {
                    let c = black_box(s.clone());
                    black_box(c);
                })
            },
        );
    }

    // --- Read-only traversal (sum bytes) ---
    for &size in &sizes {
        let payload = make_ascii(size);
        let s_string = payload.clone();
        let s_vec = payload.as_bytes().to_vec();
        let s_sds = Sds::from_str(&payload);

        let id_base = format!("read_sum/{}B", size);
        group.throughput(Throughput::Bytes(size as u64));

        // String
        group.bench_with_input(
            BenchmarkId::new("String::read_sum", &id_base),
            &s_string,
            |b, s| {
                b.iter(|| {
                    let mut sum: u64 = 0;
                    for ch in s.as_bytes().iter() {
                        sum = sum.wrapping_add(*ch as u64);
                    }
                    black_box(sum);
                })
            },
        );

        // Vec
        group.bench_with_input(
            BenchmarkId::new("Vec::read_sum", &id_base),
            &s_vec,
            |b, v| {
                b.iter(|| {
                    let mut sum: u64 = 0;
                    for &b in v.iter() {
                        sum = sum.wrapping_add(b as u64);
                    }
                    black_box(sum);
                })
            },
        );

        // Sds
        group.bench_with_input(
            BenchmarkId::new("Sds::read_sum", &id_base),
            &s_sds,
            |b, s| {
                b.iter(|| {
                    let mut sum: u64 = 0;
                    for &b in s.as_slice().iter() {
                        sum = sum.wrapping_add(b as u64);
                    }
                    black_box(sum);
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compare);
criterion_main!(benches);
