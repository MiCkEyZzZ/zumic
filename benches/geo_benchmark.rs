//! Benchmarks для GeoSet и R-tree spatial index.
//!
//! Запуск: `cargo bench --bench geo_benchmark`

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use zumic::database::geo::{GeoEntry, GeoPoint, GeoSet};

/// Генерирует случайные GeoEntry для тестирования.
fn generate_entries(count: usize) -> Vec<GeoEntry> {
    use std::{collections::hash_map::RandomState, hash::BuildHasher};

    let hasher_builder = RandomState::new();
    (0..count)
        .map(|i| {
            let hash = hasher_builder.hash_one(i);

            let lon = ((hash % 36000) as f64 / 100.0) - 180.0;
            let lat = (((hash >> 32) % 18000) as f64 / 100.0) - 90.0;

            GeoEntry {
                member: format!("point_{i}"),
                point: GeoPoint { lon, lat },
                score: 0,
            }
        })
        .collect()
}

/// Benchmark: sequential insert vs bulk load.
fn bench_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("insertion");

    for size in [100, 1000, 10000] {
        let entries = generate_entries(size);

        // Sequential insert
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut gs = GeoSet::new();
                    for entry in entries {
                        gs.add(entry.member.clone(), entry.point.lon, entry.point.lat);
                    }
                    black_box(gs)
                });
            },
        );

        // Bulk load
        group.bench_with_input(
            BenchmarkId::new("bulk_load", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let gs = GeoSet::from_entries(entries.clone());
                    black_box(gs)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: radius queries на различных размерах dataset.
fn bench_radius_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("radius_query");

    for size in [1000, 10000, 100000] {
        let entries = generate_entries(size);
        let gs = GeoSet::from_entries(entries);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("10km_radius", size), &gs, |b, gs| {
            b.iter(|| {
                let results = gs.radius(0.0, 0.0, 10_000.0);
                black_box(results)
            });
        });

        group.bench_with_input(BenchmarkId::new("100km_radius", size), &gs, |b, gs| {
            b.iter(|| {
                let results = gs.radius(0.0, 0.0, 100_000.0);
                black_box(results)
            });
        });
    }

    group.finish();
}

/// Benchmark: k-NN queries.
fn bench_knn_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("knn_query");

    for size in [1000, 10000, 100000] {
        let entries = generate_entries(size);
        let gs = GeoSet::from_entries(entries);

        for k in [1, 10, 100] {
            group.throughput(Throughput::Elements(size as u64));
            group.bench_with_input(BenchmarkId::new(format!("{k}-NN"), size), &gs, |b, gs| {
                b.iter(|| {
                    let results = gs.nearest(0.0, 0.0, k);
                    black_box(results)
                });
            });
        }
    }

    group.finish();
}

/// Benchmark: point lookup by member name.
fn bench_get_operation(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_operation");

    for size in [1000, 10000, 100000] {
        let entries = generate_entries(size);
        let gs = GeoSet::from_entries(entries.clone());
        let target_member = &entries[size / 2].member;

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("get", size),
            &(gs, target_member),
            |b, (gs, member)| {
                b.iter(|| {
                    let result = gs.get(member);
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: distance calculation between two points.
fn bench_distance_calc(c: &mut Criterion) {
    let mut group = c.benchmark_group("distance");

    let entries = generate_entries(1000);
    let gs = GeoSet::from_entries(entries.clone());
    let m1 = &entries[100].member;
    let m2 = &entries[500].member;

    group.bench_function("haversine_dist", |b| {
        b.iter(|| {
            let dist = gs.dist(m1, m2);
            black_box(dist)
        });
    });

    group.finish();
}

/// Benchmark: index rebuild performance.
fn bench_index_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_rebuild");

    for size in [1000, 10000] {
        let entries = generate_entries(size);

        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("rebuild", size), &entries, |b, entries| {
            b.iter_batched(
                || {
                    let mut gs = GeoSet::from_entries(entries.clone());
                    // Симулируем обновления
                    for (i, _) in entries.iter().enumerate().take(100) {
                        gs.add(
                            format!("point_{i}"),
                            entries[i].point.lon + 0.1,
                            entries[i].point.lat + 0.1,
                        );
                    }
                    gs
                },
                |mut gs| {
                    gs.rebuild_index();
                    black_box(gs)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark: memory overhead R-tree vs flat array.
fn bench_memory_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");

    for size in [1000, 10000, 100000] {
        let entries = generate_entries(size);

        // Измеряем размер flat array
        let flat_size = std::mem::size_of::<GeoEntry>() * size;

        group.bench_with_input(
            BenchmarkId::new("allocation", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let gs = GeoSet::from_entries(entries.clone());
                    let stats = gs.index_stats();
                    black_box((gs, stats))
                });
            },
        );

        // Выводим статистику
        let gs = GeoSet::from_entries(entries);
        let stats = gs.index_stats();
        println!(
            "\nSize: {} points, Tree depth: {}, Nodes: {}, Leaves: {}",
            size, stats.depth, stats.node_count, stats.leaf_count
        );
        println!(
            "Flat array: {} bytes, Estimated overhead: ~{}%",
            flat_size,
            ((stats.node_count * 200) as f64 / flat_size as f64 * 100.0) as usize
        );
    }

    group.finish();
}

/// Benchmark: concurrent read performance.
#[cfg(feature = "concurrent")]
#[allow(dead_code)]
fn bench_concurrent_reads(c: &mut Criterion) {
    use std::{sync::Arc, thread};

    let mut group = c.benchmark_group("concurrent_reads");

    let entries = generate_entries(10000);
    let gs = Arc::new(GeoSet::from_entries(entries));

    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("radius_query", num_threads),
            &num_threads,
            |b, &threads| {
                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|_| {
                            let gs_clone = Arc::clone(&gs);
                            thread::spawn(move || {
                                for _ in 0..100 {
                                    let results = gs_clone.radius(0.0, 0.0, 50_000.0);
                                    black_box(results);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

#[cfg(feature = "concurrent")]
criterion_group!(
    benches,
    bench_insertion,
    bench_radius_query,
    bench_knn_query,
    bench_get_operation,
    bench_distance_calc,
    bench_index_rebuild,
    bench_memory_overhead,
);

#[cfg(feature = "concurrent")]
criterion_main!(benches);
