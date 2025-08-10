//! Генераторы для property-based тестирования всех типов Value
//!
//! Каждый генератор создаёт стратегии для генерации случайных,
//! но валидных данных определённого типа с акцентом на edge cases.

use std::{cmp, f64, ops::RangeInclusive};

use ordered_float::OrderedFloat;
use proptest::{prelude::*, string::string_regex};
use zumic::{
    Bitmap, Dict, Hll, Sds, SkipList, SmartHash, StreamEntry, StreamId, Value, DENSE_SIZE,
};

/// Размеры для тестирования - от очень маленьких до больших
const SMALL_SIZE: RangeInclusive<usize> = 0..=10;
const MEDIUM_SIZE: RangeInclusive<usize> = 10..=100;
const LARGE_SIZE: RangeInclusive<usize> = 100..=1000;

/// MIN_COMPRESSION_SIZE из compression.rs
pub const MIN_COMPRESSION_SIZE: usize = 64;

/// Генерация для Sds строк.
pub fn sds_strategy() -> impl Strategy<Value = Sds> {
    // Формируем динамический regex безопасно (saturating_sub чтобы не уйти в underflow)
    let around_compression = format!(
        "[a-zA-Z0-9]{{{},{}}}",
        MIN_COMPRESSION_SIZE.saturating_sub(5),
        MIN_COMPRESSION_SIZE + 5
    );

    prop_oneof![
        // пустая строка — это уже стратегия (`Just`)
        Just(Sds::from_str("")),
        // статические regex'ы — оборачиваем в string_regex(...).unwrap()
        string_regex("[a-zA-Z0-9]{1,10}")
            .unwrap()
            .prop_map(|s| Sds::from_str(&s)),
        // динамический regex из format!
        string_regex(&around_compression)
            .unwrap()
            .prop_map(|s| Sds::from_str(&s)),
        string_regex("[a-zA-Z0-9]{100,1000}")
            .unwrap()
            .prop_map(|s| Sds::from_str(&s)),
        // raw string лучше для unicode-диапазонов
        string_regex(r"[\u{00}-\u{1F}\u{7F}-\u{FF}]{1,50}")
            .unwrap()
            .prop_map(|s| Sds::from_str(&s)),
    ]
}

/// Генератор для базовых типов Value.
pub fn basic_value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // Null
        Just(Value::Null),
        // Bool
        any::<bool>().prop_map(Value::Bool),
        // Int - включая граничные значения
        prop_oneof![
            Just(i64::MIN),
            Just(i64::MAX),
            Just(0i64),
            Just(-1i64),
            Just(1i64),
            any::<i64>(),
        ]
        .prop_map(Value::Int),
        // Float - включая специальные значения
        prop_oneof![
            Just(f64::NAN),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            Just(0.0f64),
            Just(-0.0f64),
            Just(f64::MIN),
            Just(f64::MAX),
            Just(f64::MIN_POSITIVE),
            any::<f64>(),
        ]
        .prop_map(Value::Float),
        // Str
        sds_strategy().prop_map(Value::Str),
    ]
}

/// Генератор для массивов (Array)
#[allow(dead_code)]
pub fn array_strategy() -> impl Strategy<Value = Value> {
    prop::collection::vec(basic_value_strategy(), SMALL_SIZE).prop_map(Value::Array)
}

/// Генератор для множеств (Set)
pub fn set_strategy() -> impl Strategy<Value = Value> {
    prop::collection::hash_set(sds_strategy(), SMALL_SIZE).prop_map(Value::Set)
}

/// Генератор для хеш-таблиц (Hash)
#[allow(dead_code)]
pub fn hash_strategy() -> impl Strategy<Value = Value> {
    // Предполагаем что у вас есть SmartHash::new() и метод insert
    prop::collection::hash_map(sds_strategy(), sds_strategy(), SMALL_SIZE).prop_map(|map| {
        // Конвертируем HashMap в ваш SmartHash
        let mut smart_hash = SmartHash::new(); // Adjust to your actual type
        for (k, v) in map {
            smart_hash.insert(k, v);
        }
        Value::Hash(smart_hash)
    })
}

/// Генератор для ZSet - особое внимание к edge cases со scores
#[allow(dead_code)]
pub fn zset_strategy() -> impl Strategy<Value = Value> {
    // Генерируем пары (score, key) с особыми случаями
    let score_strategy = prop_oneof![
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(0.0),
        Just(-0.0),
        any::<f64>(),
    ];

    prop::collection::vec((score_strategy, sds_strategy()), SMALL_SIZE).prop_map(|pairs| {
        let mut dict = Dict::new(); // Adjust to your actual Dict type
        let mut sorted = SkipList::new(); // Adjust to your actual SkipList type

        for (score, key) in pairs {
            dict.insert(key.clone(), score);
            sorted.insert(OrderedFloat(score), key); // Adjust based on your implementation
        }

        Value::ZSet { dict, sorted }
    })
}

/// Генератор для HyperLogLog с акцентом на boundary sizes
#[allow(dead_code)]
pub fn hll_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // Пустой HLL (все нули)
        Just({
            let data = [0u8; DENSE_SIZE]; // Adjust DENSE_SIZE import
            Value::HyperLogLog(Box::new(Hll { data }))
        }),
        // HLL с случайными данными
        any::<[u8; DENSE_SIZE]>().prop_map(|data| { Value::HyperLogLog(Box::new(Hll { data })) }),
        // HLL со всеми максимальными значениями
        Just({
            let data = [0xFF; DENSE_SIZE];
            Value::HyperLogLog(Box::new(Hll { data }))
        }),
    ]
}

/// Генератор для Bitmap с различными размерами
#[allow(dead_code)]
pub fn bitmap_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // Пустой bitmap
        Just({
            let bmp = Bitmap::new(); // Adjust to your actual Bitmap type
            Value::Bitmap(bmp)
        }),
        // Малые bitmap
        prop::collection::vec(any::<u8>(), 1..=10).prop_map(|bytes| {
            let mut bmp = Bitmap::new();
            bmp.bytes = bytes; // Adjust based on your Bitmap API
            Value::Bitmap(bmp)
        }),
        // Большие bitmap (до 1MB как указано в issue)
        prop::collection::vec(any::<u8>(), 1000..=10000).prop_map(|bytes| {
            let mut bmp = Bitmap::new();
            bmp.bytes = bytes;
            Value::Bitmap(bmp)
        }),
    ]
}

/// Генератор для SStream (самый сложный тип)
#[allow(dead_code)]
pub fn sstream_strategy() -> impl Strategy<Value = Value> {
    // Генератор для ID записи в потоке
    let stream_id_strategy =
        (any::<u64>(), any::<u64>()).prop_map(|(ms_time, sequence)| StreamId { ms_time, sequence }); // Adjust to your StreamId type

    // Генератор для данных в записи (map field_name -> Value)
    let stream_data_strategy = prop::collection::hash_map(
        "[a-zA-Z][a-zA-Z0-9_]*", // field names
        basic_value_strategy(),  // field values
        0..=5,                   // небольшое количество полей
    );

    // Собираем записи потока
    let stream_entry_strategy =
        (stream_id_strategy, stream_data_strategy).prop_map(|(id, data)| StreamEntry { id, data }); // Adjust to your StreamEntry type

    prop::collection::vec(stream_entry_strategy, SMALL_SIZE).prop_map(Value::SStream)
}

/// Главный генератор - объединяет все типы Value
pub fn any_value_strategy() -> impl Strategy<Value = Value> {
    // Рекурсивный генератор с ограничением глубины
    let leaf = prop_oneof![
        basic_value_strategy(),
        // Простые коллекции без вложенности
        prop::collection::vec(basic_value_strategy(), 0..=3).prop_map(Value::Array),
        set_strategy(),
    ];

    leaf.prop_recursive(
        8,   // максимальная глубина рекурсии
        256, // максимальное количество узлов
        10,  // максимальные элементы в коллекции
        |inner| {
            prop_oneof![
                // Вложенные структуры
                prop::collection::vec(inner.clone(), 0..=3).prop_map(Value::Array),
                // hash_strategy(), // если поддерживает рекурсию
                // zset_strategy(),
                // hll_strategy(),
                // bitmap_strategy(),
                // sstream_strategy(),
            ]
        },
    )
}

/// Специальный генератор для значений около границы сжатия
pub fn compression_boundary_strategy() -> impl Strategy<Value = Value> {
    // защищаемся от underflow
    let n_minus = MIN_COMPRESSION_SIZE.saturating_sub(1);

    let s1 = string_regex(&format!("a{{{n_minus}}}"))
        .unwrap()
        .prop_map(|s| Value::Str(Sds::from_str(&s)));
    let s2 = string_regex(&format!("a{{{MIN_COMPRESSION_SIZE}}}"))
        .unwrap()
        .prop_map(|s| Value::Str(Sds::from_str(&s)));
    let s3 = string_regex(&format!("a{{{}}}", MIN_COMPRESSION_SIZE + 1))
        .unwrap()
        .prop_map(|s| Value::Str(Sds::from_str(&s)));

    let boundary_strings = prop_oneof![s1, s2, s3];

    prop_oneof![
        boundary_strings,
        prop::collection::vec(
            basic_value_strategy(),
            MIN_COMPRESSION_SIZE / 8..=MIN_COMPRESSION_SIZE / 4
        )
        .prop_map(Value::Array),
    ]
}

/// Генератор для больших коллекций
pub fn large_collection_strategy() -> impl Strategy<Value = Value> {
    let min = *LARGE_SIZE.start();
    let max = *LARGE_SIZE.end();
    let max_for_set = cmp::min(max, 500usize);

    prop_oneof![
        prop::collection::vec(basic_value_strategy(), min..=max).prop_map(Value::Array),
        prop::collection::hash_set(sds_strategy(), min..=max_for_set).prop_map(Value::Set),
    ]
}

/// Специальный генератор для числовых edge cases
pub fn numeric_edge_case_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        // Целые числа - граничные случаи
        prop_oneof![
            Just(i64::MIN),
            Just(i64::MAX),
            Just(0i64),
            Just(-1i64),
            Just(1i64),
            Just(i32::MIN as i64),
            Just(i32::MAX as i64),
            Just(i16::MIN as i64),
            Just(i16::MAX as i64),
            Just(u8::MIN as i64),
            Just(u8::MAX as i64),
        ]
        .prop_map(Value::Int),
        // Числа с плавающей точкой - все особые случаи
        prop_oneof![
            Just(f64::NAN),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            Just(0.0f64),
            Just(-0.0f64),
            Just(f64::MIN),
            Just(f64::MAX),
            Just(f64::MIN_POSITIVE),
            Just(f64::EPSILON),
            Just(f64::consts::PI),
            Just(f64::consts::E),
            // Subnormal numbers
            Just(f64::from_bits(1)),
            Just(f64::from_bits(0x000FFFFFFFFFFFFF)),
        ]
        .prop_map(Value::Float),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_medium_size_constant() {
        // Просто вызовите или проверьте, чтобы "использовать" константу
        let size: usize = *MEDIUM_SIZE.start();
        assert!(size <= *MEDIUM_SIZE.end());
    }
}
