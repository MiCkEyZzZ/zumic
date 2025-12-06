//! Property-based tests для ZDB кодека
//!
//! Эти тесты генерируют тысячи случайных значений Value и проверяют
//! что encode/decode работает корректно во всех случаях.

use std::io::Cursor;

use proptest::prelude::*;
use zumic::{
    engine::zdb::{read_value, read_value_with_version, write_value, FormatVersion},
    Value,
};

mod generators;
use generators::*;

/// Базовая настройка proptest - количество итераций и другие параметры
const PROPTEST_CASES: u32 = 1000;
const PROPTEST_MAX_SHRINK_ITERS: u32 = 10000;

/// Глубокое сравнение Value с корректной обработкой NaN в Float
fn value_deep_eq(
    a: &Value,
    b: &Value,
) -> bool {
    use Value::*;
    match (a, b) {
        // Спец. случай NaN == NaN
        (Float(f1), Float(f2)) => {
            if f1.is_nan() && f2.is_nan() {
                true
            } else {
                f1 == f2
            }
        }

        (Array(v1), Array(v2)) => {
            v1.len() == v2.len() && v1.iter().zip(v2).all(|(x, y)| value_deep_eq(x, y))
        }

        (Set(s1), Set(s2)) => s1.len() == s2.len() && s1.iter().all(|x| s2.contains(x)),

        (Hash(h1), Hash(h2)) => {
            let e1 = h1.entries();
            let e2 = h2.entries();
            if e1.len() != e2.len() {
                return false;
            }
            e1.into_iter()
                .all(|(k, v)| e2.contains(&(k.clone(), v.clone())))
        }

        (ZSet { dict: d1, .. }, ZSet { dict: d2, .. }) => {
            let e1: Vec<_> = d1.into_iter().collect();
            let e2: Vec<_> = d2.into_iter().collect();
            if e1.len() != e2.len() {
                return false;
            }
            e1.into_iter()
                .all(|(k, s1)| match e2.iter().find(|(kk, _)| *kk == k) {
                    Some((_, s2)) => {
                        if s1.is_nan() && s2.is_nan() {
                            true
                        } else {
                            s1 == *s2
                        }
                    }
                    None => false,
                })
        }

        (Array(..), _) | (Set(..), _) | (Hash(..), _) | (ZSet { .. }, _) => false,

        _ => a == b,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: PROPTEST_CASES,
        max_shrink_iters: PROPTEST_MAX_SHRINK_ITERS,
        .. ProptestConfig::default()
    })]

    /// Главный roundtrip тест: любое Value должно корректно encode -> decode
    #[test]
    fn roundtrip_all_values(value in any_value_strategy()) {
        let mut buffer = Vec::new();

        // Encode
        write_value(&mut buffer, &value)
            .map_err(|e| TestCaseError::fail(format!("Failed to encode value: {e}")))?;

        // Decode
        let mut cursor = Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)
            .map_err(|e| TestCaseError::fail(format!("Failed to decode value: {e}")))?;

        prop_assert!(
            value_deep_eq(&value, &decoded),
            "Roundtrip failed: original != decoded\nleft: {value:?}\nright: {decoded:?}"
        );
    }

    /// Тест совместимости версий: V1 -> V2
    #[test]
    fn cross_version_compatibility_v1_to_v2(value in any_value_strategy()) {
        let mut buffer = Vec::new();

        write_value(&mut buffer, &value)
            .map_err(|e| TestCaseError::fail(format!("Failed to encode: {e}")))?;

        let mut cursor = Cursor::new(&buffer);
        let decoded = read_value_with_version(&mut cursor, FormatVersion::V2, None, 0)
            .map_err(|e| TestCaseError::fail(format!("Failed to decode with V2: {e}")))?;

        prop_assert!(
            value_deep_eq(&value, &decoded),
            "Cross-version compatibility failed\nleft: {value:?}\nright: {decoded:?}"
        );
    }

    /// Тест границ сжатия: значения около MIN_COMPRESSION_SIZE должны правильно обрабатываться
    #[test]
    fn compression_boundary_values(value in compression_boundary_strategy()) {
        let mut buffer = Vec::new();

        write_value(&mut buffer, &value)
            .map_err(|e| TestCaseError::fail(format!("Failed to encode compression boundary value: {e}")))?;

        let mut cursor = Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)
            .map_err(|e| TestCaseError::fail(format!("Failed to decode compression boundary value: {e}")))?;

        prop_assert!(
            value_deep_eq(&value, &decoded),
            "Compression boundary roundtrip failed\nleft: {value:?}\nright: {decoded:?}"
        );
    }

    /// Тест больших коллекций
    #[test]
    fn large_collections_roundtrip(value in large_collection_strategy()) {
        let mut buffer = Vec::new();

        write_value(&mut buffer, &value)
            .map_err(|e| TestCaseError::fail(format!("Failed to encode large collection: {e}")))?;

        let mut cursor = Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)
            .map_err(|e| TestCaseError::fail(format!("Failed to decode large collection: {e}")))?;

        prop_assert!(
            value_deep_eq(&value, &decoded),
            "Large collection roundtrip failed\nleft: {value:?}\nright: {decoded:?}"
        );
    }

    /// Тест edge cases для чисел (NaN, Infinity, граничные значения)
    #[test]
    fn numeric_edge_cases(value in numeric_edge_case_strategy()) {
        let mut buffer = Vec::new();

        write_value(&mut buffer, &value)
            .map_err(|e| TestCaseError::fail(format!("Failed to encode numeric edge case: {e}")))?;

        let mut cursor = Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)
            .map_err(|e| TestCaseError::fail(format!("Failed to decode numeric edge case: {e}")))?;

        // Уже был отдельный NaN-кейс, оставим:
        match (&value, &decoded) {
            (Value::Float(f1), Value::Float(f2)) if f1.is_nan() && f2.is_nan() => {},
            _ => {
                prop_assert!(
                    value_deep_eq(&value, &decoded),
                    "Numeric edge case roundtrip failed\nleft: {value:?}\nright: {decoded:?}"
                );
            }
        }
    }
}

/// Дополнительные unit тесты для специфичных случаев
#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_empty_collections() {
        let test_cases = vec![
            Value::Array(vec![]),
            Value::Set(std::collections::HashSet::new()),
        ];

        for value in test_cases {
            let mut buffer = Vec::new();
            write_value(&mut buffer, &value).unwrap();

            let mut cursor = Cursor::new(&buffer);
            let decoded = read_value(&mut cursor).unwrap();

            assert!(
                value_deep_eq(&value, &decoded),
                "Empty collection roundtrip failed\nleft: {value:?}\nright: {decoded:?}"
            );
        }
    }

    #[test]
    fn test_single_element_collections() {
        let test_cases = vec![Value::Array(vec![Value::Int(42)])];

        for value in test_cases {
            let mut buffer = Vec::new();
            write_value(&mut buffer, &value).unwrap();

            let mut cursor = Cursor::new(&buffer);
            let decoded = read_value(&mut cursor).unwrap();

            assert!(
                value_deep_eq(&value, &decoded),
                "Single element collection roundtrip failed\nleft: {value:?}\nright: {decoded:?}"
            );
        }
    }
}
