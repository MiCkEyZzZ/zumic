use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{rngs::StdRng, Rng, SeedableRng};
use zumic::{GeoEntry, GeoPoint, GeoSet, Geohash, GeohashPrecision, RadiusOptions};

/// -----------------------------
/// Utils
/// -----------------------------

fn generate_entries(count: usize) -> Vec<GeoEntry> {
    let mut rng = StdRng::seed_from_u64(42);

    (0..count)
        .map(|i| GeoEntry {
            member: format!("point_{i}"),
            point: GeoPoint {
                lon: rng.gen_range(-180.0..180.0),
                lat: rng.gen_range(-90.0..90.0),
            },
            score: 0,
        })
        .collect()
}

fn generate_clustered_points(
    count: usize,
    clusters: usize,
    seed: u64,
) -> Vec<(String, f64, f64)> {
    let mut rng = StdRng::seed_from_u64(seed);

    let centers: Vec<(f64, f64)> = (0..clusters)
        .map(|_| (rng.gen_range(-180.0..180.0), rng.gen_range(-90.0..90.0)))
        .collect();

    (0..count)
        .map(|i| {
            let (cx, cy) = centers[i % clusters];
            (
                format!("P{i}"),
                (cx + rng.gen_range(-0.1..0.1)).clamp(-180.0, 180.0),
                (cy + rng.gen_range(-0.1..0.1)).clamp(-90.0, 90.0),
            )
        })
        .collect()
}

/// -----------------------------
/// Insert benchmarks
/// -----------------------------

fn bench_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for &size in &[1_000, 10_000] {
        let entries = generate_entries(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    let mut gs = GeoSet::new();
                    for e in entries {
                        gs.add(e.member.clone(), e.point.lon, e.point.lat);
                    }
                    black_box(gs);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("bulk_load", size),
            &entries,
            |b, entries| {
                b.iter(|| {
                    black_box(GeoSet::from_entries(entries.clone()));
                });
            },
        );
    }

    group.finish();
}

/// -----------------------------
/// Get / Dist
/// -----------------------------

fn bench_get_and_dist(c: &mut Criterion) {
    let entries = generate_entries(10_000);
    let gs = GeoSet::from_entries(entries.clone());

    let m1 = &entries[100].member;
    let m2 = &entries[9000].member;

    c.bench_function("get", |b| {
        b.iter(|| black_box(gs.get(black_box(m1))));
    });

    c.bench_function("dist_haversine", |b| {
        b.iter(|| black_box(gs.dist(black_box(m1), black_box(m2))));
    });
}

/// -----------------------------
/// Radius
/// -----------------------------

fn bench_radius(c: &mut Criterion) {
    let mut group = c.benchmark_group("radius");

    let size = 50_000;
    let points = generate_clustered_points(size, 10, 42);
    let mut gs = GeoSet::new();

    for (m, lon, lat) in points {
        gs.add(m, lon, lat);
    }

    for &radius in &[1_000.0, 10_000.0, 100_000.0] {
        let gh_prec = GeohashPrecision::from_radius(radius);

        group.bench_with_input(
            BenchmarkId::new("rtree", radius as u64),
            &radius,
            |b, &r| {
                b.iter(|| {
                    let res = gs.radius_with_options(
                        0.0,
                        0.0,
                        r,
                        RadiusOptions {
                            use_geohash: false,
                            geohash_precision: None,
                            include_neighbors: false,
                        },
                    );
                    black_box(res);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("geohash+rtree", radius as u64),
            &radius,
            |b, &r| {
                b.iter(|| {
                    let res = gs.radius_with_options(
                        0.0,
                        0.0,
                        r,
                        RadiusOptions {
                            use_geohash: true,
                            geohash_precision: Some(gh_prec),
                            include_neighbors: true,
                        },
                    );
                    black_box(res);
                });
            },
        );
    }

    group.finish();
}

/// -----------------------------
/// k-NN
/// -----------------------------

fn bench_knn(c: &mut Criterion) {
    let mut group = c.benchmark_group("knn");

    let size = 50_000;
    let entries = generate_entries(size);
    let gs = GeoSet::from_entries(entries);

    for &k in &[10, 50, 100] {
        group.bench_with_input(BenchmarkId::new("knn", k), &k, |b, &k| {
            b.iter(|| {
                black_box(gs.nearest(0.0, 0.0, k));
            });
        });
    }

    group.finish();
}

/// -----------------------------
/// Geohash
/// -----------------------------

fn bench_geohash(c: &mut Criterion) {
    let point = GeoPoint {
        lon: 13.361389,
        lat: 38.115556,
    };

    for p in [
        GeohashPrecision::Low,
        GeohashPrecision::Medium,
        GeohashPrecision::High,
        GeohashPrecision::VeryHigh,
    ] {
        c.bench_function(&format!("geohash_encode_{:?}", p), |b| {
            b.iter(|| black_box(Geohash::encode(point, p)));
        });
    }
}

/// -----------------------------
/// False positive rate (CORRECT)
/// -----------------------------

fn bench_false_positive(c: &mut Criterion) {
    use std::collections::HashSet;

    let size = 20_000;
    let points = generate_clustered_points(size, 20, 42);
    let mut gs = GeoSet::new();

    for (m, lon, lat) in points {
        gs.add(m, lon, lat);
    }

    c.bench_function("false_positive_rate", |b| {
        b.iter(|| {
            let r = 10_000.0;
            let p = GeohashPrecision::from_radius(r);

            // Только имена
            let gh: HashSet<_> = gs
                .radius_with_options(
                    0.0,
                    0.0,
                    r,
                    RadiusOptions {
                        use_geohash: true,
                        geohash_precision: Some(p),
                        include_neighbors: true,
                    },
                )
                .into_iter()
                .map(|(member, _dist)| member)
                .collect();

            let rt: HashSet<_> = gs
                .radius_with_options(
                    0.0,
                    0.0,
                    r,
                    RadiusOptions {
                        use_geohash: false,
                        geohash_precision: None,
                        include_neighbors: false,
                    },
                )
                .into_iter()
                .map(|(member, _dist)| member)
                .collect();

            let fp = gh.difference(&rt).count() as f64 / gh.len().max(1) as f64;
            black_box(fp);
        });
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(5));
    targets =
        bench_insertion,
        bench_get_and_dist,
        bench_radius,
        bench_knn,
        bench_geohash,
        bench_false_positive
);

criterion_main!(benches);
