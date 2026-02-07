use zumic::{
    database::geo_distance::{
        calculate_distance, estimate_max_error, euclidean_distance, great_circle_distance,
        manhattan_distance, recommend_method, vincenty_distance, vincenty_distance_ellipsoid,
        DistanceMethod, DistanceUnit, Ellipsoid,
    },
    haversine_distance, GeoPoint,
};

const EPSILON: f64 = 0.01; // 1см

#[test]
fn test_known_distances_accuracy() {
    // Пермь -> Екатеринбург: ~292.244км
    let perm = GeoPoint {
        lon: 56.2347,
        lat: 58.0105,
    };
    let yekaterinburg = GeoPoint {
        lon: 60.6057,
        lat: 56.8389,
    };

    let haversine = haversine_distance(perm, yekaterinburg);
    let vincenty = vincenty_distance(perm, yekaterinburg).unwrap();
    let great_circle = great_circle_distance(perm, yekaterinburg);

    // Все методы должны быть близки к ссылке (~292,244 км)
    let reference_m = 292_244.36073020997_f64;
    assert!((haversine - reference_m).abs() < 5_000.0);
    assert!((vincenty - reference_m).abs() < 1_000.0);
    assert!((great_circle - reference_m).abs() < 5_000.0);

    // Винсент должен быть более тонким — проверяем малую разницу с хаверсинусом
    assert!((vincenty - haversine).abs() < 1_000.0);
}

#[test]
fn test_equatorial_vs_polar() {
    // Экваториальное расстояние
    let eq_p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    let eq_p2 = GeoPoint { lon: 1.0, lat: 0.0 };

    // Полярное расстояние
    let pol_p1 = GeoPoint {
        lon: 0.0,
        lat: 89.0,
    };
    let pol_p2 = GeoPoint {
        lon: 1.0,
        lat: 89.0,
    };

    let eq_dist = vincenty_distance(eq_p1, eq_p2).unwrap();
    let pol_dist = vincenty_distance(pol_p1, pol_p2).unwrap();

    // Полярное расстояние должно быть значительно меньше
    assert!(pol_dist < eq_dist * 0.2);
}

#[test]
fn test_antipodal_points() {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    // Почти антиподальная точка
    let p2 = GeoPoint {
        lon: 179.0,
        lat: 0.0,
    };

    // Хаверсинус должен работать
    let haversine = haversine_distance(p1, p2);
    assert!(haversine > 19_000_000.0); // ~половина окружности Земли

    // Винсенти не может сойтись для точных антиподов, но должен работать для
    // почти-антиподов
    let vincenty = vincenty_distance(p1, p2);
    assert!(vincenty.is_some());
}

#[test]
fn test_very_small_distances() {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    let p2 = GeoPoint {
        lon: 0.000_001,
        lat: 0.000_001,
    }; // ~0,15м

    let haversine = haversine_distance(p1, p2);
    let vincenty = vincenty_distance(p1, p2).unwrap();
    let euclidean = euclidean_distance(p1, p2);

    // Все методы должны быть близки для малых расстояний
    assert!((haversine - vincenty).abs() < 0.001);
    assert!((euclidean - vincenty).abs() < 0.01);
}

#[test]
fn test_zero_distance() {
    let p = GeoPoint {
        lon: 13.4,
        lat: 52.5,
    };

    assert_eq!(haversine_distance(p, p), 0.0);
    assert_eq!(vincenty_distance(p, p).unwrap(), 0.0);
    assert_eq!(manhattan_distance(p, p), 0.0);
    assert_eq!(euclidean_distance(p, p), 0.0);
}

#[test]
fn test_unit_conversion_precision() {
    let meters = 12_345.678;

    // Преобразование туда и обратно
    for unit in [
        DistanceUnit::Kilometers,
        DistanceUnit::Miles,
        DistanceUnit::Feet,
        DistanceUnit::NauticalMiles,
    ] {
        let converted = unit.convert_from_meters(meters);
        let back = unit.convert_to_meters(converted);
        assert!((back - meters).abs() < EPSILON, "Failed for {unit:?}");
    }
}

#[test]
fn test_manhattan_properties() {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };

    // Диагональное движение
    let p2_diag = GeoPoint { lon: 1.0, lat: 1.0 };
    // Движение по оси
    let p2_axis = GeoPoint { lon: 0.0, lat: 1.0 };

    let manhattan_diag = manhattan_distance(p1, p2_diag);
    let manhattan_axis = manhattan_distance(p1, p2_axis);
    let haversine_diag = haversine_distance(p1, p2_diag);

    // Манхэттен всегда >= хаверсинус
    assert!(manhattan_diag >= haversine_diag);

    // Манхэттен для диагонали > для выравнивания по оси (если одинаковая разница
    // широты и долготы)
    assert!(manhattan_diag > manhattan_axis);
}

#[test]
fn test_ellipsoid_differences() {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    let p2 = GeoPoint {
        lon: 10.0,
        lat: 0.0,
    };

    let wgs84 = vincenty_distance_ellipsoid(p1, p2, Ellipsoid::WGS84).unwrap();
    let grs80 = vincenty_distance_ellipsoid(p1, p2, Ellipsoid::GRS80).unwrap();
    let sphere = vincenty_distance_ellipsoid(p1, p2, Ellipsoid::SPHERE).unwrap();

    // WGS84 и GRS80 должны быть практически идентичны
    assert!(
        (wgs84 - grs80).abs() < 1.0,
        "WGS84 vs GRS80 diff = {} m",
        (wgs84 - grs80).abs()
    );

    // Sphere должен заметно отличаться от эллипсоидов
    let diff = (wgs84 - sphere).abs();

    // Для 10° по экватору разница радиусов (~7,1 км) дает ~1,2 км по дуге
    assert!(
        diff > 1_000.0 && diff < 2_000.0,
        "Unexpected WGS84 vs Sphere diff = {diff} m"
    );
}

#[test]
fn test_error_bound_estimates() {
    let dist_10km = 10_000.0;

    let haversine_err = estimate_max_error(DistanceMethod::Haversine, dist_10km);
    let vincenty_err = estimate_max_error(DistanceMethod::Vincenty, dist_10km);
    let euclidean_err = estimate_max_error(DistanceMethod::Euclidean, dist_10km);

    // Винсент должен совершить наименьшую ошибку
    assert!(vincenty_err < haversine_err);
    assert!(vincenty_err < euclidean_err);

    // Ошибка Винсенти должна быть субметровой
    assert!(vincenty_err < 1.0);
}

#[test]
fn test_method_recommendations() {
    // Высокая точность -> Винсент
    let method1 = recommend_method(10_000.0, 0.0001);
    assert_eq!(method1, DistanceMethod::Vincenty);

    // Стандартная точность -> Хаверсинус
    let method2 = recommend_method(100_000.0, 100.0);
    assert_eq!(method2, DistanceMethod::Haversine);

    // Низкая точность, малое расстояние -> Евклидово
    let method3 = recommend_method(50.0, 20.0);
    assert_eq!(method3, DistanceMethod::Euclidean);

    // Очень низкая точность -> Манхэттен
    let method4 = recommend_method(10_000.0, 2000.0);
    assert_eq!(method4, DistanceMethod::Manhattan);
}

#[test]
fn test_cross_meridian() {
    // Пересечение 180° меридиана
    let p1 = GeoPoint {
        lon: 179.0,
        lat: 0.0,
    };
    let p2 = GeoPoint {
        lon: -179.0,
        lat: 0.0,
    };

    let dist = haversine_distance(p1, p2);

    // Должно быть короткое расстояние (~220км), не через весь мир
    assert!(dist < 300_000.0);
}

#[test]
fn test_high_latitudes() {
    let north1 = GeoPoint {
        lon: 0.0,
        lat: 89.0,
    };
    let north2 = GeoPoint {
        lon: 180.0,
        lat: 89.0,
    };

    let haversine = haversine_distance(north1, north2);
    let vincenty = vincenty_distance(north1, north2);

    assert!(vincenty.is_some());

    // Расстояние должно быть малым (близко к полюсу)
    assert!(haversine < 250_000.0);
}

#[test]
fn test_distance_result_metadata() {
    let p1 = GeoPoint { lon: 0.0, lat: 0.0 };
    let p2 = GeoPoint { lon: 1.0, lat: 0.0 };

    let result = calculate_distance(p1, p2, DistanceMethod::Vincenty);

    assert_eq!(result.method, DistanceMethod::Vincenty);
    assert!(result.iterations.is_some());
    assert!(result.error_bound_m.is_some());
    assert!(result.error_bound_m.unwrap() < 0.001);
}

#[test]
fn test_batch_distance_calculations() {
    let origin = GeoPoint { lon: 0.0, lat: 0.0 };

    let points: Vec<GeoPoint> = (0..100)
        .map(|i| GeoPoint {
            lon: (i as f64) * 0.1,
            lat: 0.0,
        })
        .collect();

    // Проверяем, что все методы работают для пакетной обработки
    for &point in &points {
        let _ = haversine_distance(origin, point);
        let _ = vincenty_distance(origin, point);
        let _ = manhattan_distance(origin, point);
    }
}

#[test]
fn test_distance_symmetry() {
    let p1 = GeoPoint {
        lon: 13.4,
        lat: 52.5,
    };
    let p2 = GeoPoint {
        lon: 14.5,
        lat: 53.6,
    };

    // d(A,B) должно равняться d(B,A)
    for method in [
        DistanceMethod::Haversine,
        DistanceMethod::Vincenty,
        DistanceMethod::GreatCircle,
        DistanceMethod::Manhattan,
        DistanceMethod::Euclidean,
    ] {
        let forward = calculate_distance(p1, p2, method).distance_m;
        let backward = calculate_distance(p2, p1, method).distance_m;

        assert!(
            (forward - backward).abs() < EPSILON,
            "Symmetry failed for {method:?}"
        );
    }
}
