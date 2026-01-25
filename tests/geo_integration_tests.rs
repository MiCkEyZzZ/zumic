use zumic::{
    haversine_distance, Direction, GeoPoint, GeoSet, Geohash, GeohashPrecision, RadiusOptions,
};

#[test]
fn test_restaurant_search_scenario() {
    let mut restaurants = GeoSet::new();

    // Добавляем рестораны в центре Кунгура
    restaurants.add("Pizza Place".into(), 57.4500, 57.4498);
    restaurants.add("Burger Joint".into(), 57.4505, 57.4502);
    restaurants.add("Sushi Bar".into(), 57.4495, 57.4505);
    restaurants.add("Italian Bistro".into(), 57.4502, 57.4508);
    restaurants.add("Thai Restaurant".into(), 57.4498, 57.4495);

    // Пользователь находится примерно в центре города
    let user_lon = 57.4500;
    let user_lat = 57.4498;

    // Поиск в радиусе 500м
    let nearby = restaurants.radius(user_lon, user_lat, 500.0);

    assert!(!nearby.is_empty(), "No nearby restaurants were found");
    assert!(nearby.iter().any(|(name, _)| name == "Pizza Place"));

    // Все результаты должны быть в пределах радиуса
    for (name, dist) in &nearby {
        assert!(
            *dist <= 500.0,
            "{} is {}m away, expected <= 500m",
            name,
            dist
        );
    }
}

#[test]
fn test_geohash_neighbor_coverage() {
    let mut gs = GeoSet::new();

    // Создаём сетку
    for i in 0..10 {
        for j in 0..10 {
            let lon = i as f64 * 0.01;
            let lat = j as f64 * 0.01;
            gs.add(format!("P_{}_{}", i, j), lon, lat);
        }
    }

    // Поиск на границе между ячейками
    let opts = RadiusOptions {
        use_geohash: true,
        geohash_precision: Some(GeohashPrecision::High),
        include_neighbors: true,
    };

    let results_with_neighbors = gs.radius_with_options(0.045, 0.045, 1000.0, opts.clone());

    let opts_no_neighbors = RadiusOptions {
        include_neighbors: false,
        ..opts
    };

    let results_without = gs.radius_with_options(0.045, 0.045, 1000.0, opts_no_neighbors);

    // С соседям должно найтись больше точек
    assert!(results_with_neighbors.len() >= results_without.len());
}

#[test]
fn test_geohash_accuracy_by_precision() {
    let original = GeoPoint {
        lon: 13.361389,
        lat: 38.115556,
    };

    let test_cases = vec![
        (GeohashPrecision::VeryLow, 20_000.0),
        (GeohashPrecision::Low, 5_000.0),
        (GeohashPrecision::Medium, 1_500.0),
        (GeohashPrecision::High, 200.0),
        (GeohashPrecision::VeryHigh, 50.0),
        (GeohashPrecision::UltraHigh, 10.5),
    ];

    for (precision, max_error_m) in test_cases {
        let gh = Geohash::encode(original, precision);
        let decoded = gh.decode();

        let error = haversine_distance(original, decoded);

        assert!(
            error < max_error_m,
            "Precision {:?}: error {}m exceeds max {}m",
            precision,
            error,
            max_error_m
        );
    }
}

#[test]
fn test_geohash_neighbor_directions() {
    let center = Geohash::encode(GeoPoint { lon: 0.0, lat: 0.0 }, GeohashPrecision::Medium);
    let north = center.neighbor(Direction::North);
    let south = center.neighbor(Direction::South);
    let east = center.neighbor(Direction::East);
    let west = center.neighbor(Direction::West);

    let center_point = center.decode();
    let north_point = north.decode();
    let south_point = south.decode();
    let east_point = east.decode();
    let west_point = west.decode();

    // North должен быть севернее
    assert!(north_point.lat > center_point.lat);

    // South должен быть южнее
    assert!(south_point.lat < center_point.lat);

    // East должен быть восточнее
    assert!(east_point.lon > center_point.lon);

    // West должен быть западнее
    assert!(west_point.lon < center_point.lon);
}

#[test]
fn test_geohash_hierarchy() {
    let gh = Geohash::encode(
        GeoPoint {
            lon: 13.4,
            lat: 52.5,
        },
        GeohashPrecision::High,
    );

    // Parent должен быть менее точным
    let parent = gh.parent().unwrap();
    assert_eq!(parent.precision(), gh.precision() - 1);
    assert!(gh.has_prefix(parent.as_str()));

    // Children должны иметь этот geohash как префикс
    let children = parent.children();
    assert_eq!(children.len(), 32);

    // Текущий gh должен быть среди детей
    assert!(children.iter().any(|c| c.hash == gh.hash));

    // Все дети должны иметь parent как префикс
    for child in &children {
        assert!(child.has_prefix(parent.as_str()));
        assert_eq!(child.precision(), parent.precision() + 1);
    }
}

#[test]
fn test_large_dataset_performance() {
    let mut gs = GeoSet::new();

    // Добавляем 10 тыс. точек
    for i in 0..10_000 {
        let lon = ((i % 100) as f64) * 0.1 - 5.0; // [-5, 4.9] ок
        let lat = ((i / 100) % 100) as f64 * 0.1 - 5.0; // теперь lat ∈ [-5, 4.9], все валидно
        gs.add(format!("P{}", i), lon, lat);
    }

    assert_eq!(gs.len(), 10_000);

    // R-дерево должно быть разумной мощностью
    let stats = gs.index_stats();
    assert!(stats.depth < 15, "Tree too deep: {}", stats.depth);

    // Geohash index должен иметь адекватное распределение
    let gh_stats = gs.geohash_stats();
    assert!(gh_stats.bucket_count > 100);
    assert!(gh_stats.avg_bucket_size < 200.0);

    // Запрос Radius должен работать быстро
    let results = gs.radius_with_options(
        0.0,
        0.0,
        50_000.0,
        RadiusOptions {
            use_geohash: false,
            geohash_precision: None,
            include_neighbors: false,
        },
    );
    assert!(!results.is_empty());
}

#[test]
fn test_query_consistency() {
    let mut gs = GeoSet::new();

    // Создаем набор данных
    for i in 0..1_000 {
        let lon = (i % 50) as f64 * 0.02;
        let lat = (i / 50) as f64 * 0.02;
        gs.add(format!("P{}", i), lon, lat);
    }

    let center_lon = 0.5;
    let center_lat = 0.5;
    let radius = 50_000.0;

    // Только R-дерево
    let opts_rtree = RadiusOptions {
        use_geohash: false,
        geohash_precision: None,
        include_neighbors: false,
    };
    let mut results_rtree = gs.radius_with_options(center_lon, center_lat, radius, opts_rtree);
    results_rtree.sort_by(|a, b| a.0.cmp(&b.0));

    // Геохеш + R-дерево
    let opts_geohash = RadiusOptions {
        use_geohash: false,
        geohash_precision: Some(GeohashPrecision::from_radius(radius)),
        include_neighbors: false,
    };
    let mut results_geohash = gs.radius_with_options(center_lon, center_lat, radius, opts_geohash);
    results_geohash.sort_by(|a, b| a.0.cmp(&b.0));

    // Результаты должны быть идентичными
    assert_eq!(results_rtree.len(), results_geohash.len());

    for (rt, gh) in results_rtree.iter().zip(results_geohash.iter()) {
        assert_eq!(rt.0, gh.0, "Member names don't match");
        assert!(
            (rt.1 - gh.1).abs() < 0.01,
            "Distance mismatch for {}: {} vs {}",
            rt.0,
            rt.1,
            gh.1
        );
    }
}

#[test]
fn test_edge_coordinates() {
    let mut gs = GeoSet::new();

    // Примерные координаты Пермского края, Россия
    // Северо-запад: 57.0, 52.0
    // Юго-восток: 58.0, 58.0
    // Центр: 57.5, 55.5
    let test_cases = vec![
        ("Northwest", 52.0, 58.0),
        ("Northeast", 58.0, 58.0),
        ("Southwest", 52.0, 57.0),
        ("Southeast", 58.0, 57.0),
        ("Center", 55.5, 57.5),
    ];

    for (name, lon, lat) in test_cases {
        gs.add(name.into(), lon, lat);

        // Должны уметь извлечь обратно
        let retrieved = gs.get(name).unwrap();
        assert!(
            (retrieved.lon - lon).abs() < 0.01,
            "{} longitude mismatch",
            name
        );
        assert!(
            (retrieved.lat - lat).abs() < 0.01,
            "{} latitude mismatch",
            name
        );

        // Geohash должен работать
        let gh = gs.get_geohash(name, GeohashPrecision::High).unwrap();
        let decoded = gh.decode();

        let error = haversine_distance(retrieved, decoded);
        assert!(error < 1000.0, "Large error at edge {}: {}m", name, error);
    }
}

#[test]
fn test_geohash_prefix_search() {
    let mut gs = GeoSet::new();

    // Добавляем точки в районе Кунгура
    gs.add("P1".into(), 57.44, 56.99); // центр города
    gs.add("P2".into(), 57.45, 56.995);
    gs.add("P3".into(), 57.46, 56.992);

    // Далёкая точка (Пермь)
    gs.add("P4".into(), 56.0, 58.0);

    // Получаем geohash для одной точки
    let gh1 = gs.get_geohash("P1", GeohashPrecision::Medium).unwrap();
    let prefix = gh1.prefix(4);

    // Точки в Кунгуре должны иметь общий префикс
    let gh2 = gs.get_geohash("P2", GeohashPrecision::Medium).unwrap();
    assert!(gh2.has_prefix(&prefix));

    let gh3 = gs.get_geohash("P3", GeohashPrecision::Medium).unwrap();
    assert!(gh3.has_prefix(&prefix));

    // Далёкая точка Пермь не должна иметь общий префикс
    let gh4 = gs.get_geohash("P4", GeohashPrecision::Medium).unwrap();
    assert!(!gh4.has_prefix(&prefix));
}
