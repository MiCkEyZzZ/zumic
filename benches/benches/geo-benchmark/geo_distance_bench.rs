use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use zumic::{
    database::geo_distance::{
        calculate_distance, euclidean_distance, great_circle_distance, manhattan_distance,
        vincenty_distance, vincenty_distance_ellipsoid, DistanceMethod, DistanceUnit, Ellipsoid,
    },
    haversine_distance, GeoPoint,
};

fn test_points() -> Vec<(GeoPoint, GeoPoint, &'static str)> {
    vec![
        // Малое расстояние (~1км)
        (
            GeoPoint { lon: 0.0, lat: 0.0 },
            GeoPoint {
                lon: 0.01,
                lat: 0.0,
            },
            "1km",
        ),
        // Среднее расстояние (~100км)
        (
            GeoPoint { lon: 0.0, lat: 0.0 },
            GeoPoint { lon: 1.0, lat: 0.0 },
            "100km",
        ),
        // Большое расстояние (~1000км)
        (
            GeoPoint { lon: 0.0, lat: 0.0 },
            GeoPoint {
                lon: 10.0,
                lat: 0.0,
            },
            "1000km",
        ),
        // Межконтинентальное (~10000км)
        (
            GeoPoint { lon: 0.0, lat: 0.0 },
            GeoPoint {
                lon: 100.0,
                lat: 0.0,
            },
            "10000km",
        ),
    ]
}

fn bench_distance_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("distance_methods");

    let points = test_points();

    for (p1, p2, dist_label) in &points {
        // Хаверсинус
        group.bench_with_input(
            BenchmarkId::new("haversine", dist_label),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(haversine_distance(black_box(*p1), black_box(*p2)));
                });
            },
        );

        // Винсенти
        group.bench_with_input(
            BenchmarkId::new("vincenty", dist_label),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(vincenty_distance(black_box(*p1), black_box(*p2)));
                });
            },
        );

        // Большой круг
        group.bench_with_input(
            BenchmarkId::new("great_circle", dist_label),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(great_circle_distance(black_box(*p1), black_box(*p2)));
                });
            },
        );

        // Манхэттен
        group.bench_with_input(
            BenchmarkId::new("manhattan", dist_label),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(manhattan_distance(black_box(*p1), black_box(*p2)));
                });
            },
        );

        // Евклидова
        group.bench_with_input(
            BenchmarkId::new("euclidean", dist_label),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(euclidean_distance(black_box(*p1), black_box(*p2)));
                });
            },
        );
    }

    group.finish();
}

fn bench_unit_conversions(c: &mut Criterion) {
    let mut group = c.benchmark_group("unit_conversions");

    let distance_m = 10_000.0;

    group.bench_function("meters_to_km", |b| {
        b.iter(|| {
            black_box(DistanceUnit::Kilometers.convert_from_meters(black_box(distance_m)));
        });
    });

    group.bench_function("meters_to_miles", |b| {
        b.iter(|| {
            black_box(DistanceUnit::Miles.convert_from_meters(black_box(distance_m)));
        });
    });

    group.bench_function("km_to_meters", |b| {
        b.iter(|| {
            black_box(DistanceUnit::Kilometers.convert_to_meters(black_box(10.0)));
        });
    });

    group.finish();
}

fn bench_unified_calculation(c: &mut Criterion) {
    let p1 = GeoPoint {
        lon: 13.4,
        lat: 52.5,
    };
    let p2 = GeoPoint {
        lon: 14.4,
        lat: 53.5,
    };

    c.bench_function("unified_haversine", |b| {
        b.iter(|| {
            black_box(calculate_distance(
                black_box(p1),
                black_box(p2),
                DistanceMethod::Haversine,
            ));
        });
    });

    c.bench_function("unified_vincenty", |b| {
        b.iter(|| {
            black_box(calculate_distance(
                black_box(p1),
                black_box(p2),
                DistanceMethod::Vincenty,
            ));
        });
    });
}

fn bench_custom_ellipsoids(c: &mut Criterion) {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    let p2 = GeoPoint { lon: 1.0, lat: 0.0 };

    c.bench_function("vincenty_wgs84", |b| {
        b.iter(|| {
            black_box(vincenty_distance_ellipsoid(
                black_box(p1),
                black_box(p2),
                Ellipsoid::WGS84,
            ));
        });
    });

    c.bench_function("vincenty_grs80", |b| {
        b.iter(|| {
            black_box(vincenty_distance_ellipsoid(
                black_box(p1),
                black_box(p2),
                Ellipsoid::GRS80,
            ));
        });
    });
}

fn bench_accuracy_analysis(c: &mut Criterion) {
    // Лондон — Париж: известное расстояние ~343,5 км.
    let london = GeoPoint {
        lon: -0.1278,
        lat: 51.5074,
    };
    let paris = GeoPoint {
        lon: 2.3522,
        lat: 48.8566,
    };
    let reference = 343_500.0; // метры

    c.bench_function("accuracy_haversine_error", |b| {
        b.iter(|| {
            let dist = haversine_distance(black_box(london), black_box(paris));
            let error = (dist - reference).abs();
            black_box(error);
        });
    });

    c.bench_function("accuracy_vincenty_error", |b| {
        b.iter(|| {
            let dist = vincenty_distance(black_box(london), black_box(paris)).unwrap();
            let error = (dist - reference).abs();
            black_box(error);
        });
    });
}

fn bench_method_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");

    // Генерируем точки на разных расстояниях
    let distances_deg = vec![0.01, 0.1, 1.0, 10.0, 50.0];

    for &deg in &distances_deg {
        let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
        let p2 = GeoPoint { lon: deg, lat: 0.0 };

        group.bench_with_input(
            BenchmarkId::new("haversine", format!("{}deg", deg)),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(haversine_distance(p1, p2));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("vincenty", format!("{}deg", deg)),
            &(p1, p2),
            |b, &(p1, p2)| {
                b.iter(|| {
                    black_box(vincenty_distance(p1, p2));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_distance_methods,
    bench_unit_conversions,
    bench_unified_calculation,
    bench_custom_ellipsoids,
    bench_accuracy_analysis,
    bench_method_scaling
);
criterion_main!(benches);
