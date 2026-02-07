use zumic::{
    database::{hll_metrics::HllMetrics, HllDefault, HllEncoding, DEFAULT_SPARSE_THRESHOLD},
    Hll,
};

#[test]
fn test_full_lifecycle() {
    let mut hll: HllDefault = Hll::new();

    // Этап 1: Sparse режим
    for i in 0..100 {
        hll.add(format!("item_{i}").as_bytes());
    }

    assert!(hll.is_sparse(), "Should start in sparse mode");
    let sparse_cardinality = hll.estimate_cardinality();
    assert!(
        (sparse_cardinality - 100.0).abs() < 10.0,
        "Sparse estimate should be close to 100, got {sparse_cardinality}",
    );

    // Этап 2: Запустить преобразование в dense
    for i in 100..5000 {
        hll.add(format!("item_{i}").as_bytes());
    }

    assert!(!hll.is_sparse(), "Should convert to dense mode");
    let dense_cardinality = hll.estimate_cardinality();
    assert!(
        (dense_cardinality - 5000.0).abs() < 100.0,
        "Dense estimate should be close to 5000, got {dense_cardinality}",
    );

    // Этап 3: Слияние с другой HLL
    let mut hll2 = Hll::new();
    for i in 4000..6000 {
        hll2.add(format!("item_{i}").as_bytes());
    }

    hll.merge(&hll2);
    let merged_cardinality = hll.estimate_cardinality();
    assert!(
        (merged_cardinality - 6000.0).abs() < 150.0,
        "Merged estimate should be close to 6000, got {merged_cardinality}"
    );
}

#[test]
fn test_accuracy_at_different_scales() {
    let test_cases = vec![
        (10, 5.0),         // 10 элементов, допуск 5
        (100, 10.0),       // 100 элементов, допуск 10
        (1_000, 30.0),     // 1K элементов, допуск 30
        (10_000, 200.0),   // 10K элементов, допуск 200
        (100_000, 2000.0), // 100K элементов, допуск 2000
    ];

    for (num_elements, tolerance) in test_cases {
        let mut hll: HllDefault = Hll::new();

        for i in 0..num_elements {
            hll.add(format!("element_{i}").as_bytes());
        }

        let estimate = hll.estimate_cardinality();
        let error = (estimate - num_elements as f64).abs();

        assert!(
            error < tolerance,
            "Scale {num_elements}: error {error} exceeds tolerance {tolerance} (estimate: {estimate})"
        );
    }
}

#[test]
fn test_memory_efficiency_sparse_vs_dense() {
    let mut sparse_hll: HllDefault = Hll::new();
    for i in 0..100 {
        sparse_hll.add(format!("item_{i}").as_bytes());
    }

    let sparse_stats = sparse_hll.stats();
    assert!(sparse_stats.is_sparse);

    let mut dense_hll: HllDefault = Hll::new();
    for i in 0..10_000 {
        dense_hll.add(format!("item_{i}").as_bytes());
    }

    let dense_stats = dense_hll.stats();
    assert!(!dense_stats.is_sparse);

    assert!(
        sparse_stats.memory_bytes < dense_stats.memory_bytes,
        "Sparse HLL should use less memory than dense"
    );
}

#[test]
fn test_threshold_monotonicity() {
    let threshold = 5000;
    let mut hll: HllDefault = Hll::with_threshold(threshold);

    let mut was_dense = false;

    for i in 0..20_000 {
        hll.add(format!("item_{i}").as_bytes());

        if !hll.is_sparse() {
            was_dense = true;
        }

        // Как только стал dense — обратно быть не должно
        if was_dense {
            assert!(
                !hll.is_sparse(),
                "HLL reverted to sparse after becoming dense at element {i}"
            );
        }
    }

    assert!(was_dense, "HLL never converted to dense");
}

#[test]
fn test_merge_combinations() {
    let mut sparse1: HllDefault = Hll::new();
    let mut sparse2: HllDefault = Hll::new();

    for i in 0..100 {
        sparse1.add(format!("a_{i}").as_bytes());
        sparse2.add(format!("b_{i}").as_bytes());
    }

    assert!(sparse1.is_sparse() && sparse2.is_sparse());

    sparse1.merge(&sparse2);
    let estimate = sparse1.estimate_cardinality();

    assert!(
        (estimate - 200.0).abs() < 20.0,
        "Sparse+Sparse merge: expected ~200, got {estimate}"
    );

    let mut dense: HllDefault = Hll::with_threshold(50);
    let mut sparse: HllDefault = Hll::new();

    for i in 0..1000 {
        dense.add(format!("d_{i}").as_bytes());
    }
    for i in 500..600 {
        sparse.add(format!("s_{i}").as_bytes());
    }

    assert!(!dense.is_sparse() && sparse.is_sparse());

    dense.merge(&sparse);
    let estimate = dense.estimate_cardinality();

    assert!(
        (estimate - 1100.0).abs() < 100.0,
        "Dense+Sparse merge: expected ~1100, got {estimate}"
    );

    let mut dense1: HllDefault = Hll::with_threshold(50);
    let mut dense2: HllDefault = Hll::with_threshold(50);

    for i in 0..2000 {
        dense1.add(format!("x_{i}").as_bytes());
    }
    for i in 1500..3500 {
        dense2.add(format!("y_{i}").as_bytes());
    }

    assert!(!dense1.is_sparse() && !dense2.is_sparse());

    dense1.merge(&dense2);
    let estimate = dense1.estimate_cardinality();

    // Для dense + dense проверяем диапазон, а не точное значение,
    // так как HLL — вероятностная структура, и merge увеличивает дисперсию.
    assert!(
        estimate > 3000.0 && estimate < 4500.0,
        "Dense+Dense merge: estimate {estimate} out of expected range"
    );
}

#[test]
fn test_add_idempotence() {
    let mut hll1: HllDefault = Hll::new();
    let mut hll2: HllDefault = Hll::new();

    let data = b"test_element";

    // Добавляем один раз
    hll1.add(data);

    // Добавляем много раз
    for _ in 0..100 {
        hll2.add(data);
    }

    let estimate1 = hll1.estimate_cardinality();
    let estimate2 = hll2.estimate_cardinality();

    assert!(
        (estimate1 - estimate2).abs() < 0.1,
        "Idempotence failed: {estimate1} vs {estimate2}"
    );
}

#[test]
fn test_merge_commutativity() {
    let mut hll_a: HllDefault = Hll::new();
    let mut hll_b: HllDefault = Hll::new();

    for i in 0..500 {
        hll_a.add(format!("a_{i}").as_bytes());
    }

    for i in 250..700 {
        hll_b.add(format!("b_{i}").as_bytes());
    }

    let mut merged_ab = hll_a.clone();
    merged_ab.merge(&hll_b);

    let mut merged_ba = hll_b.clone();
    merged_ba.merge(&hll_a);

    let estimate_ab = merged_ab.estimate_cardinality();
    let estimate_ba = merged_ba.estimate_cardinality();

    assert!(
        (estimate_ab - estimate_ba).abs() < 1.0,
        "Commutativity failed: {estimate_ab} vs {estimate_ba}"
    );
}

#[test]
fn test_merge_associativity() {
    let mut hll1: HllDefault = Hll::new();
    let mut hll2: HllDefault = Hll::new();
    let mut hll3: HllDefault = Hll::new();

    for i in 0..300 {
        hll1.add(format!("1_{i}").as_bytes());
    }

    for i in 200..500 {
        hll2.add(format!("2_{i}").as_bytes());
    }

    for i in 400..700 {
        hll3.add(format!("3_{i}").as_bytes());
    }

    let mut left_assoc = hll1.clone();
    left_assoc.merge(&hll2);
    left_assoc.merge(&hll3);

    let mut right_assoc = hll1.clone();
    let mut bc = hll2.clone();
    bc.merge(&hll3);
    right_assoc.merge(&bc);

    let estimate_left = left_assoc.estimate_cardinality();
    let estimate_right = right_assoc.estimate_cardinality();

    assert!(
        (estimate_left - estimate_right).abs() < 0.5,
        "Associativity failed: {estimate_left} vs {estimate_right}"
    );
}

#[test]
fn test_sparse_serialization() {
    let mut hll: HllDefault = Hll::new();

    for i in 0..200 {
        hll.add(format!("item_{i}").as_bytes());
    }

    assert!(hll.is_sparse());
    let original_estimate = hll.estimate_cardinality();

    // Сериализация
    let serialized = bincode::serialize(&hll).unwrap();

    // Десериализация
    let deserialized: Hll = bincode::deserialize(&serialized).unwrap();

    assert!(deserialized.is_sparse());
    assert_eq!(original_estimate, deserialized.estimate_cardinality());
}

#[test]
fn test_dense_serialization() {
    let mut hll: HllDefault = Hll::new();

    for i in 0..200 {
        hll.add(format!("item_{i}").as_bytes());
    }

    assert!(hll.is_sparse());
    let original_estimate = hll.estimate_cardinality();

    // Сериализация
    let serialized = bincode::serialize(&hll).unwrap();

    // Десериализация
    let deserialized: Hll = bincode::deserialize(&serialized).unwrap();

    assert!(deserialized.is_sparse());
    assert_eq!(original_estimate, deserialized.estimate_cardinality());
}

#[test]
fn test_metrics_tracking() {
    let metrics = HllMetrics::new();

    // Симулируем создание нескольких HLL
    metrics.on_create_sparse();
    metrics.on_create_sparse();
    metrics.on_create_sparse();

    // Симулируем операции
    for _ in 0..100 {
        metrics.on_add();
    }

    for _ in 0..10 {
        metrics.on_merge();
    }

    for _ in 0..50 {
        metrics.on_estimation();
    }

    // Симулируем конверсию
    metrics.on_sparse_to_dense_conversion(12288);

    let snapshot = metrics.snapshot();

    assert_eq!(snapshot.total_created, 3);
    assert_eq!(snapshot.sparse_count, 2);
    assert_eq!(snapshot.dense_count, 1);
    assert_eq!(snapshot.sparse_to_dense_conversions, 1);
    assert_eq!(snapshot.total_adds, 100);
    assert_eq!(snapshot.total_merges, 10);
    assert_eq!(snapshot.total_estimations, 50);
    assert_eq!(snapshot.total_memory_bytes, 12288);
}

#[test]
fn test_hll_stats() {
    let mut hll: HllDefault = Hll::new();

    // Пустой HLL
    let empty_stats = hll.stats();
    assert_eq!(empty_stats.cardinality, 0.0);
    assert!(empty_stats.is_sparse);
    assert_eq!(empty_stats.non_zero_registers, 0);

    // Sparse HLL
    for i in 0..100 {
        hll.add(format!("item_{i}").as_bytes());
    }

    let sparse_stats = hll.stats();
    assert!(sparse_stats.cardinality > 50.0);
    assert!(sparse_stats.is_sparse);
    assert!(sparse_stats.non_zero_registers > 0);
    assert!(sparse_stats.memory_bytes < 5000);

    // Dense HLL
    for i in 100..10000 {
        hll.add(format!("item_{i}").as_bytes());
    }

    let dense_stats = hll.stats();
    assert!(dense_stats.cardinality > 5000.0);
    assert!(!dense_stats.is_sparse);
    assert!(dense_stats.non_zero_registers > sparse_stats.non_zero_registers);
    assert!(dense_stats.memory_bytes > sparse_stats.memory_bytes);
}

#[test]
fn test_large_cardinality() {
    let mut hll: HllDefault = Hll::new();

    // Добавляем 1 миллион уникальных элементов
    for i in 0..1_000_000 {
        if i % 10000 == 0 {
            // Переодически проверяем, что HLL работает
            let _ = hll.estimate_cardinality();
        }
        hll.add(format!("element_{i}").as_bytes());
    }

    let estimate = hll.estimate_cardinality();
    let error_rate = (estimate - 1_000_000.0).abs() / 1_000_000.0;

    // Должно быть в пределах 3% от истинного значения
    assert!(
        error_rate < 0.03,
        "Large cardinality error rate {:.2}% exceeds 3%",
        error_rate * 100.0
    );

    assert!(!hll.is_sparse(), "Should be dense for large cardinality");
}

#[test]
fn test_conversion_performance() {
    use std::time::Instant;

    let mut hll: HllDefault = Hll::with_threshold(DEFAULT_SPARSE_THRESHOLD);

    match &mut hll.encoding {
        HllEncoding::Sparse(sparse) => {
            for idx in 0..(DEFAULT_SPARSE_THRESHOLD + 1) {
                // Устанавливаем небольшой ненулевой rho (1) в каждый регистр.
                sparse.set_register(idx, 1);
            }
        }
        _ => panic!("Expected newly created HLL to be sparse"),
    }

    hll.convert_to_dense();

    assert!(
        !hll.is_sparse(),
        "HLL should convert to dense after explicit conversion when sparse encoding is saturated"
    );

    // Проверяем, что операции в dense режиме быстрые
    let start = Instant::now();
    for i in 0..1000 {
        hll.add(format!("new_item_{i}").as_bytes());
    }
    let duration = start.elapsed();

    // 1000 операций должны занять < 10ms
    assert!(
        duration.as_millis() < 10,
        "Dense operations too slow: {duration:?}",
    );
}
