//! Расширенные генераторы для более сложных edge cases
//! Добавляется после успешного внедрения базовых property
//! tests.

use ordered_float::OrderedFloat;
use proptest::{
    prelude::{any, prop, Just, Strategy},
    prop_oneof,
};

use zumic::{Dict, Hll, Sds, SkipList, Value, DENSE_SIZE};

mod generators;
use generators::*;

/// Генератор для глубокого вложенных структур.
pub fn deeply_nested_strategy() -> impl Strategy<Value = Value> {
    // Создаёт структуры типа Array[Array[Array[...]]] глубиной до 100
    let leaf = Just(Value::Int(42));

    // макс. глубина, максимум узлов, только один элемент на уровень для макс. глубины.
    leaf.prop_recursive(100, 1000, 1, |inner| Just(Value::Array(vec![inner])))
}

/// Генератор для Unicode edge cases в строках.
pub fn unicode_string_strategy() -> impl Strategy<Value = Sds> {
    prop_oneof![
        // Emoji и специальные символы.
        "[\u{1F600}-\u{1F64F}]{1,10}", // Emoticons
        "[\u{2600}-\u{26FF}]{1,10}",   // Miscellaneous symbols
        "[\u{1F300}-\u{1F5FF}]{1,10}", // Misc symbols and pictographs
        // RTL (Right-to-Left) символы - могут сломать парсинг
        "[\u{0590}-\u{05FF}]{1,20}", // Hebrew
        "[\u{0600}-\u{06FF}]{1,20}", // Arabic
        // Контрольные символы
        "[\u{0000}-\u{001F}]{1,10}", // C0 controls
        "[\u{007F}-\u{009F}]{1,10}", // C1 controls
        // Очень длинные Unicode строки
        "[\u{4E00}-\u{9FFF}]{50,200}", // CJK Unified Ideographs
        // Смешанные направления текста
        "[a-zA-Z\u{0590}-\u{05FF}]{1,50}",
    ]
    .prop_map(|s| Sds::from_str(&s))
}

/// Генератор для патологических ZSet случаев.
pub fn pathological_zset_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // Все элементы с одинаковым score (тест сортировка)
        (
            any::<f64>(),
            prop::collection::vec(sds_strategy(), 10..=100)
        )
            .prop_map(|(score, keys)| {
                let mut dict = Dict::new();
                let mut sorted = SkipList::new();
                for key in keys {
                    dict.insert(key.clone(), score);
                    sorted.insert(OrderedFloat(score), key);
                }
                Value::ZSet { dict, sorted }
            }),
        // Множество с очень близкими scores (тест точности float)
        prop::collection::vec(sds_strategy(), 10..=50).prop_map(|keys| {
            let mut dict = Dict::new();
            let mut sorted = SkipList::new();
            for (i, key) in keys.into_iter().enumerate() {
                // Очень близкие значения, различающиеся на epsilon.
                let score = 1.0 + (i as f64) * f64::EPSILON;
                dict.insert(key.clone(), score);
                sorted.insert(OrderedFloat(score), key);
            }
            Value::ZSet { dict, sorted }
        }),
        // ZSet с NaN scores в разных позициях
        prop::collection::vec(sds_strategy(), 5..=20).prop_map(|keys| {
            let mut dict = Dict::new();
            let mut sorted = SkipList::new();
            for (i, key) in keys.into_iter().enumerate() {
                let score = if i % 3 == 0 { f64::NAN } else { i as f64 };
                dict.insert(key.clone(), score);
                sorted.insert(OrderedFloat(score), key);
            }
            Value::ZSet { dict, sorted }
        }),
    ]
}

/// Генератор для HLL с специфичными паттернами.
pub fn specialized_hll_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // HLL с градиентным паттерном (0, 1, 2, 3, ...)
        Just({
            let mut data = [0u8; DENSE_SIZE];
            for (i, byte) in data.iter_mut().enumerate() {
                *byte = (i % 256) as u8;
            }
            Value::HyperLogLog(Box::new(Hll { data }))
        }),
        // HLL с чередующимся паттерном (0x55, 0xAA, ...)
        Just({
            let mut data = [0u8; DENSE_SIZE];
        }),
    ]
}
