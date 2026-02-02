//! Property-based tests для HyperLogLog
//!
//! Эти тесты генерируют тысячи случайных HLL структур и проверяют
//! что математические свойства выполняются во всех случаях.

use proptest::prelude::*;
use zumic::{database::DEFAULT_SPARSE_THRESHOLD, Hll};

/// Basic proptest setting - number of iterations and other parameters.
const PROPTEST_CASES: u32 = 1000;
const PROPTEST_MAX_SHRINK_ITERS: u32 = 10000;

// ============================================================================
// ГЕНЕРАТОРЫ
// ============================================================================

/// Генератор случайных элементов для добавления в HLL
fn element_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..100)
}

/// Генератор вектора элементов
fn element_vec_strategy() -> impl Strategy<Value = Vec<Vec<u8>>> {
    prop::collection::vec(element_strategy(), 0..500)
}

/// Генератор HLL с малым количеством элементов (sparse)
fn sparse_hll_strategy() -> impl Strategy<Value = Hll> {
    element_vec_strategy().prop_map(|elements| {
        let mut hll = Hll::new();
        for elem in elements.iter().take(100) {
            // Ограничиваем до 100 элементов для гарантии sparse
            hll.add(elem);
        }
        hll
    })
}

/// Генератор HLL с большим количеством элементов (dense)
fn dense_hll_strategy() -> impl Strategy<Value = Hll> {
    element_vec_strategy().prop_map(|elements| {
        let mut hll = Hll::with_threshold(100); // Низкий порог для быстрой конверсии
        for elem in elements {
            hll.add(&elem);
        }
        hll
    })
}

/// Генератор произвольного HLL (может быть sparse или dense)
fn any_hll_strategy() -> impl Strategy<Value = Hll> {
    prop_oneof![sparse_hll_strategy(), dense_hll_strategy(),]
}

/// Генератор пары HLL для тестирования merge
fn hll_pair_strategy() -> impl Strategy<Value = (Hll, Hll)> {
    (any_hll_strategy(), any_hll_strategy())
}

/// Генератор тройки HLL для тестирования ассоциативности
fn hll_triple_strategy() -> impl Strategy<Value = (Hll, Hll, Hll)> {
    (any_hll_strategy(), any_hll_strategy(), any_hll_strategy())
}

/// Генератор HLL с заданным порогом
fn hll_with_threshold_strategy() -> impl Strategy<Value = Hll> {
    (100usize..10000, element_vec_strategy()).prop_map(|(threshold, elements)| {
        let mut hll = Hll::with_threshold(threshold);
        for elem in elements {
            hll.add(&elem);
        }
        hll
    })
}

// ============================================================================
// PROPERTY TESTS
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PROPTEST_CASES,
        max_shrink_iters: PROPTEST_MAX_SHRINK_ITERS,
        .. ProptestConfig::default()
    })]

    // ------------------------------------------------------------------------
    // ИДЕМПОТЕНТНОСТЬ: add(x) == add(x, x, x, ...)
    // ------------------------------------------------------------------------

    /// Добавление одного и того же элемента N раз не должно менять кардинальность
    #[test]
    fn add_idempotence(element in element_strategy(), n in 1usize..100) {
        let mut hll1 = Hll::new();
        let mut hll2 = Hll::new();

        // Добавляем один раз
        hll1.add(&element);

        // Добавляем N раз
        for _ in 0..n {
            hll2.add(&element);
        }

        let est1 = hll1.estimate_cardinality();
        let est2 = hll2.estimate_cardinality();

        prop_assert!(
            (est1 - est2).abs() < 1.0,
            "Idempotence violated: single add = {}, {} adds = {}",
            est1, n, est2
        );
    }

    /// Порог sparse→dense не должен влиять на семантику merge
    #[test]
    fn threshold_does_not_affect_merge_semantics(
        hll1 in hll_with_threshold_strategy(),
        hll2 in hll_with_threshold_strategy(),
    ) {
        let mut merged1 = hll1.clone();
        merged1.merge(&hll2);

        let mut merged2 = hll2.clone();
        merged2.merge(&hll1);

        let est1 = merged1.estimate_cardinality();
        let est2 = merged2.estimate_cardinality();

        prop_assert!(
            (est1 - est2).abs() < 5.0,
            "Threshold affected merge result: {} vs {}",
            est1, est2
        );
    }

    /// Идемпотентность для массива элементов
    #[test]
    fn add_multiple_idempotence(elements in element_vec_strategy()) {
        let mut hll1 = Hll::new();
        let mut hll2 = Hll::new();

        // Добавляем каждый элемент один раз
        for elem in &elements {
            hll1.add(elem);
        }

        // Добавляем каждый элемент несколько раз
        for _ in 0..5 {
            for elem in &elements {
                hll2.add(elem);
            }
        }

        let est1 = hll1.estimate_cardinality();
        let est2 = hll2.estimate_cardinality();

        prop_assert!(
            (est1 - est2).abs() < 10.0,
            "Multiple idempotence violated: {} vs {}",
            est1, est2
        );
    }

    // ------------------------------------------------------------------------
    // КОММУТАТИВНОСТЬ: merge(A, B) == merge(B, A)
    // ------------------------------------------------------------------------

    /// Порядок merge не должен влиять на результат
    #[test]
    fn merge_commutativity((hll_a, hll_b) in hll_pair_strategy()) {
        let mut ab = hll_a.clone();
        ab.merge(&hll_b);

        let mut ba = hll_b.clone();
        ba.merge(&hll_a);

        let est_ab = ab.estimate_cardinality();
        let est_ba = ba.estimate_cardinality();

        prop_assert!(
            (est_ab - est_ba).abs() < 1.0,
            "Commutativity violated: merge(A,B) = {}, merge(B,A) = {}",
            est_ab, est_ba
        );
    }

    /// Коммутативность для sparse HLL
    #[test]
    fn sparse_merge_commutativity(
        elements_a in prop::collection::vec(element_strategy(), 10..50),
        elements_b in prop::collection::vec(element_strategy(), 10..50)
    ) {
        let mut hll_a = Hll::new();
        let mut hll_b = Hll::new();

        for elem in &elements_a {
            hll_a.add(elem);
        }
        for elem in &elements_b {
            hll_b.add(elem);
        }

        // Оба должны быть sparse
        prop_assert!(hll_a.is_sparse() && hll_b.is_sparse());

        let mut ab = hll_a.clone();
        ab.merge(&hll_b);

        let mut ba = hll_b.clone();
        ba.merge(&hll_a);

        let est_ab = ab.estimate_cardinality();
        let est_ba = ba.estimate_cardinality();

        prop_assert!(
            (est_ab - est_ba).abs() < 1.0,
            "Sparse commutativity violated: {} vs {}",
            est_ab, est_ba
        );
    }

    // ------------------------------------------------------------------------
    // АССОЦИАТИВНОСТЬ: merge(merge(A,B), C) == merge(A, merge(B,C))
    // ------------------------------------------------------------------------

    /// Группировка merge не должна влиять на результат
    #[test]
    fn merge_associativity((hll_a, hll_b, hll_c) in hll_triple_strategy()) {
        // (A ∪ B) ∪ C
        let mut left = hll_a.clone();
        left.merge(&hll_b);
        left.merge(&hll_c);

        // A ∪ (B ∪ C)
        let mut right = hll_a.clone();
        let mut bc = hll_b.clone();
        bc.merge(&hll_c);
        right.merge(&bc);

        let est_left = left.estimate_cardinality();
        let est_right = right.estimate_cardinality();

        prop_assert!(
            (est_left - est_right).abs() < 5.0,
            "Associativity violated: ((A∪B)∪C) = {}, (A∪(B∪C)) = {}",
            est_left, est_right
        );
    }

    // ------------------------------------------------------------------------
    // МОНОТОННОСТЬ: добавление элементов увеличивает/сохраняет кардинальность
    // ------------------------------------------------------------------------

    /// Кардинальность не должна уменьшаться при добавлении элементов
    #[test]
    fn cardinality_monotonicity(elements in element_vec_strategy()) {
        let mut hll = Hll::new();
        let mut prev_cardinality = 0.0;

        for elem in elements {
            hll.add(&elem);
            let curr_cardinality = hll.estimate_cardinality();

            prop_assert!(
                curr_cardinality >= prev_cardinality - 1.0, // -1.0 для учёта погрешности
                "Monotonicity violated: {} < {} after adding element",
                curr_cardinality, prev_cardinality
            );

            prev_cardinality = curr_cardinality;
        }
    }

    /// Merge не должен уменьшать кардинальность
    #[test]
    fn merge_monotonicity((hll_a, hll_b) in hll_pair_strategy()) {
        let est_a = hll_a.estimate_cardinality();
        let est_b = hll_b.estimate_cardinality();

        let mut merged = hll_a.clone();
        merged.merge(&hll_b);
        let est_merged = merged.estimate_cardinality();

        prop_assert!(
            est_merged >= est_a - 1.0 && est_merged >= est_b - 1.0,
            "Merge monotonicity violated: A = {}, B = {}, merge(A,B) = {}",
            est_a, est_b, est_merged
        );
    }

    // ------------------------------------------------------------------------
    // ТОЧНОСТЬ: оценка должна быть близка к истинному значению
    // ------------------------------------------------------------------------

    /// Точность для известного количества уникальных элементов
    #[test]
    fn cardinality_accuracy(n in 10usize..1000) {
        let mut hll = Hll::new();

        // Добавляем n уникальных элементов
        for i in 0..n {
            hll.add(format!("unique_{}", i).as_bytes());
        }

        let estimate = hll.estimate_cardinality();
        let error = (estimate - n as f64).abs() / n as f64;

        // Стандартная погрешность HLL: ~1.04/sqrt(m) ≈ 0.81% для m=16384
        // Проверяем, что ошибка в пределах 5%
        prop_assert!(
            error < 0.05,
            "Accuracy violated: expected {}, got {} (error {:.2}%)",
            n, estimate, error * 100.0
        );
    }

    // ------------------------------------------------------------------------
    // СЕРИАЛИЗАЦИЯ: roundtrip должен сохранять кардинальность
    // ------------------------------------------------------------------------

    /// Сериализация и десериализация не должна менять кардинальность
    #[test]
    fn serialization_preserves_cardinality(hll in any_hll_strategy()) {
        let original_estimate = hll.estimate_cardinality();
        let original_is_sparse = hll.is_sparse();

        // Сериализация
        let serialized = bincode::serialize(&hll)
            .map_err(|e| TestCaseError::fail(format!("Serialization failed: {}", e)))?;

        // Десериализация
        let deserialized: Hll = bincode::deserialize(&serialized)
            .map_err(|e| TestCaseError::fail(format!("Deserialization failed: {}", e)))?;

        let deserialized_estimate = deserialized.estimate_cardinality();
        let deserialized_is_sparse = deserialized.is_sparse();

        prop_assert_eq!(
            original_is_sparse, deserialized_is_sparse,
            "Encoding type changed during serialization"
        );

        prop_assert!(
            (original_estimate - deserialized_estimate).abs() < 0.01,
            "Cardinality changed during serialization: {} -> {}",
            original_estimate, deserialized_estimate
        );
    }

    /// Сериализация sparse HLL
    #[test]
    fn sparse_serialization_roundtrip(hll in sparse_hll_strategy()) {
        prop_assert!(hll.is_sparse(), "Expected sparse HLL");

        let original_estimate = hll.estimate_cardinality();

        let serialized = bincode::serialize(&hll)
            .map_err(|e| TestCaseError::fail(format!("Serialization failed: {}", e)))?;

        let deserialized: Hll = bincode::deserialize(&serialized)
            .map_err(|e| TestCaseError::fail(format!("Deserialization failed: {}", e)))?;

        prop_assert!(deserialized.is_sparse(), "Deserialized HLL should be sparse");

        prop_assert!(
            (original_estimate - deserialized.estimate_cardinality()).abs() < 0.01,
            "Sparse cardinality changed: {} -> {}",
            original_estimate, deserialized.estimate_cardinality()
        );
    }

    /// Сериализация dense HLL
    #[test]
    fn dense_serialization_roundtrip(hll in dense_hll_strategy()) {
        let original_estimate = hll.estimate_cardinality();
        let original_is_sparse = hll.is_sparse();

        let serialized = bincode::serialize(&hll)
            .map_err(|e| TestCaseError::fail(format!("Serialization failed: {}", e)))?;

        let deserialized: Hll = bincode::deserialize(&serialized)
            .map_err(|e| TestCaseError::fail(format!("Deserialization failed: {}", e)))?;

        prop_assert_eq!(
            original_is_sparse, deserialized.is_sparse(),
            "Encoding type changed during serialization"
        );

        prop_assert!(
            (original_estimate - deserialized.estimate_cardinality()).abs() < 0.01,
            "Dense cardinality changed: {} -> {}",
            original_estimate, deserialized.estimate_cardinality()
        );
    }

    // ------------------------------------------------------------------------
    // КОНВЕРСИЯ: sparse→dense должна сохранять кардинальность
    // ------------------------------------------------------------------------

    /// Конверсия sparse→dense не должна менять кардинальность
    #[test]
    fn sparse_to_dense_conversion_preserves_cardinality(
        elements in prop::collection::vec(element_strategy(), 50..200)
    ) {
        let mut hll = Hll::new();

        for elem in &elements {
            hll.add(elem);
        }

        let sparse_estimate = hll.estimate_cardinality();
        prop_assert!(hll.is_sparse(), "Expected sparse before conversion");

        // Принудительная конверсия
        hll.convert_to_dense();

        let dense_estimate = hll.estimate_cardinality();
        prop_assert!(!hll.is_sparse(), "Expected dense after conversion");

        prop_assert!(
            (sparse_estimate - dense_estimate).abs() < 1.0,
            "Conversion changed cardinality: {} -> {}",
            sparse_estimate, dense_estimate
        );
    }

    /// Автоматическая конверсия при превышении порога
    #[test]
    fn automatic_conversion_at_threshold(threshold in 100usize..1000) {
        let mut hll = Hll::with_threshold(threshold);

        // Добавляем элементы до превышения порога
        for i in 0..(threshold + 500) {
            hll.add(format!("elem_{}", i).as_bytes());
        }

        // Должен быть dense после превышения порога
        prop_assert!(
            !hll.is_sparse(),
            "HLL should convert to dense after threshold {}",
            threshold
        );
    }

    // ------------------------------------------------------------------------
    // THRESHOLD: различные пороги должны работать корректно
    // ------------------------------------------------------------------------

    /// HLL с разными порогами должны давать одинаковую кардинальность
    #[test]
    fn threshold_independence(
        elements in element_vec_strategy(),
        threshold1 in 100usize..5000,
        threshold2 in 100usize..5000
    ) {
        let mut hll1 = Hll::with_threshold(threshold1);
        let mut hll2 = Hll::with_threshold(threshold2);

        for elem in elements {
            hll1.add(&elem);
            hll2.add(&elem);
        }

        let est1 = hll1.estimate_cardinality();
        let est2 = hll2.estimate_cardinality();

        prop_assert!(
            (est1 - est2).abs() < 5.0,
            "Threshold affects cardinality: threshold {} = {}, threshold {} = {}",
            threshold1, est1, threshold2, est2
        );
    }

    // ------------------------------------------------------------------------
    // ПУСТЫЕ И ГРАНИЧНЫЕ СЛУЧАИ
    // ------------------------------------------------------------------------

    /// Пустой HLL должен давать кардинальность 0
    #[test]
    fn empty_hll_zero_cardinality(_unit in any::<()>()) {
        let hll = Hll::new();
        let estimate = hll.estimate_cardinality();

        prop_assert_eq!(
            estimate, 0.0,
            "Empty HLL should have cardinality 0, got {}",
            estimate
        );
    }

    /// Один элемент должен давать кардинальность ≈1
    #[test]
    fn single_element_cardinality(element in element_strategy()) {
        let mut hll = Hll::new();
        hll.add(&element);

        let estimate = hll.estimate_cardinality();

        prop_assert!(
            (0.5..=5.0).contains(&estimate),
            "Single element cardinality should be ~1, got {}",
            estimate
        );
    }

    // ------------------------------------------------------------------------
    // MERGE С САМИМ СОБОЙ
    // ------------------------------------------------------------------------

    /// Merge HLL с самим собой не должен менять кардинальность
    #[test]
    fn merge_with_self_idempotence(hll in any_hll_strategy()) {
        let original_estimate = hll.estimate_cardinality();

        let mut merged = hll.clone();
        merged.merge(&hll);

        let merged_estimate = merged.estimate_cardinality();

        prop_assert!(
            (original_estimate - merged_estimate).abs() < 1.0,
            "Merge with self changed cardinality: {} -> {}",
            original_estimate, merged_estimate
        );
    }

    // ------------------------------------------------------------------------
    // СТАТИСТИКА
    // ------------------------------------------------------------------------

    /// Статистика должна быть согласованной
    #[test]
    fn stats_consistency(hll in any_hll_strategy()) {
        let stats = hll.stats();

        // is_sparse должен соответствовать методу is_sparse()
        prop_assert_eq!(
            stats.is_sparse, hll.is_sparse(),
            "stats.is_sparse inconsistent with is_sparse()"
        );

        // Кардинальность должна совпадать
        let estimate = hll.estimate_cardinality();
        prop_assert!(
            (stats.cardinality - estimate).abs() < 0.01,
            "stats.cardinality inconsistent: stats = {}, estimate() = {}",
            stats.cardinality, estimate
        );

        // Memory bytes для sparse должна быть меньше, чем для dense
        if stats.is_sparse {
            prop_assert!(
                stats.memory_bytes < 12288,
                "Sparse memory should be < 12KB, got {}",
                stats.memory_bytes
            );
        }
    }
}

// ============================================================================
// ДОПОЛНИТЕЛЬНЫЕ UNIT ТЕСТЫ ДЛЯ EDGE CASES
// ============================================================================

#[cfg(test)]
mod edge_cases {
    use zumic::database::HllEncoding;

    use super::*;

    #[test]
    fn test_default_threshold() {
        let mut hll = Hll::new();

        match &mut hll.encoding {
            HllEncoding::Sparse(sparse) => {
                for idx in 0..=DEFAULT_SPARSE_THRESHOLD {
                    sparse.set_register(idx as u16, 1);
                }
            }
            _ => panic!("Expected sparse HLL initially"),
        }

        hll.convert_to_dense();

        assert!(
            !hll.is_sparse(),
            "Should convert after exceeding DEFAULT_SPARSE_THRESHOLD registers"
        );
    }

    #[test]
    fn test_merge_empty_hlls() {
        let mut hll1 = Hll::new();
        let hll2 = Hll::new();

        hll1.merge(&hll2);

        assert_eq!(hll1.estimate_cardinality(), 0.0);
        assert!(hll1.is_sparse());
    }

    #[test]
    fn test_merge_with_empty() {
        let mut hll1 = Hll::new();
        for i in 0..100 {
            hll1.add(format!("elem_{}", i).as_bytes());
        }

        let est_before = hll1.estimate_cardinality();
        let empty = Hll::new();

        hll1.merge(&empty);

        let est_after = hll1.estimate_cardinality();

        assert!(
            (est_before - est_after).abs() < 0.1,
            "Merge with empty changed cardinality"
        );
    }

    #[test]
    fn test_repeated_conversions() {
        let mut hll = Hll::new();

        for i in 0..100 {
            hll.add(format!("elem_{}", i).as_bytes());
        }

        let est_sparse = hll.estimate_cardinality();

        // Первая конверсия
        hll.convert_to_dense();
        let est_dense1 = hll.estimate_cardinality();

        // Повторная конверсия (не должна ничего менять)
        hll.convert_to_dense();
        let est_dense2 = hll.estimate_cardinality();

        assert!((est_sparse - est_dense1).abs() < 1.0);
        assert_eq!(est_dense1, est_dense2);
    }
}
