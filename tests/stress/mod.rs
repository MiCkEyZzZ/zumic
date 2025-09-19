//! Стресс-тесты для проверки производительности и
//! стабильности под нагрузкой. Запускаются отдельно
//! от обычных property tests.

use std::{
    io::Cursor,
    time::{Duration, Instant},
};

use proptest::{collection::vec, prop_assert, prop_assert_eq, proptest, test_runner::Config};

use zumic::{
    engine::{read_value, write_value},
    Sds, Value, DENSE_SIZE,
};

mod advanced_generators;
mod generators;
use advanced_generators::*;
use generators::*;

proptest! {
    #![proptest_config(Config {
            cases: 10000,  // Много итераций для стресс-теста
            max_shrink_iters: 100, // Меньше shrinking для скорости
            .. Config::default()
        })]

    /// Стресс-тест: 10 тыс. случаиных roundtrips должны завершиться быстро.
    #[test]
    fn stress_roundtrip_performance(values in vec(any_value_strategy(), 1..=10)) {
        let start = Instant::now();
        let n = values.len();

        // Итерируем по ссылкам, чтобы не перемещать `values`
        for value in &values {
            let mut buffer = Vec::new();
            // write_value принимает &Value — передаём ссылку
            write_value(&mut buffer, value).unwrap();

            let mut cursor = Cursor::new(&buffer);
            let decoded = read_value(&mut cursor).unwrap();

            // value: &Value, decoded: Value -> сравниваем &decoded
            prop_assert_eq!(value, &decoded);
        }

        let elapsed = start.elapsed();
        // Каждый roundtrip должен быть быстрее ~1 ms в среднем
        prop_assert!(
            elapsed < Duration::from_millis(n as u64),
            "Roundtrip too slow: {:?} for {} values",
            elapsed,
            n
        );
    }

    /// Стресс-тест памяти: большие структуры не должны вызывать OOM
    #[test]
    fn stress_memory_usage(value in large_collection_strategy()) {
        // Проверяем что можем кодировать/декодировать большие структуры
        // без превышения разумных лимитов памяти
        let mut buffer = Vec::new();
        write_value(&mut buffer, &value)?;

        // Проверяем что размер закодированных данных разумный
        // (не больше чем 10x от исходного размера из-за сжатия)
        let estimated_size = estimate_value_size(&value);
        prop_assert!(buffer.len() <= estimated_size * 10,
                    "Encoded size {} too large for estimated size {}",
                    buffer.len(), estimated_size);

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)?;
        prop_assert_eq!(value, decoded);
    }

    /// Стресс-тест для глубоко вложенных структур
    #[test]
    fn stress_deep_nesting(value in deeply_nested_strategy()) {
        // Этот тест может найти stack overflow в рекурсивном коде
        let mut buffer = Vec::new();
        write_value(&mut buffer, &value)?;

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = read_value(&mut cursor)?;
        prop_assert_eq!(value, decoded);
    }
}

/// Вспомогательная функция для оценки размера Value в памяти
fn estimate_value_size(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(_) => 1,
        Value::Int(_) => 8,
        Value::Float(_) => 8,
        Value::Str(s) => s.len(),
        Value::List(list) => {
            // QuickList<Sds> — проходимся по элементам, считаем длину строк
            let elems_size: usize = list.iter().map(|sds| sds.len()).sum();
            elems_size + list.len() * std::mem::size_of::<Sds>()
        }
        Value::Array(arr) => arr.iter().map(estimate_value_size).sum::<usize>() + arr.len() * 8,
        Value::Set(set) => set.iter().map(|s| s.len()).sum::<usize>() + set.len() * 8,
        Value::Hash(hash) => {
            // Предполагаем что есть метод для итерации
            hash.entries()
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                + hash.len() * 16
        }
        Value::ZSet { dict, .. } => dict.len() * 32, // Примерная оценка
        Value::HyperLogLog(_) => DENSE_SIZE,
        Value::Bitmap(bmp) => bmp.as_bytes().len(),
        Value::SStream(entries) => entries.len() * 64, // Примерная оценка
    }
}

/// Отдельные unit тесты для специфичных стресс-случаев
#[cfg(test)]
mod stress_unit_tests {
    use zumic::Sds;

    use super::*;

    #[test]
    fn test_maximum_string_length() {
        // Тест с очень длинной строкой (близко к u32::MAX байт)
        // ВНИМАНИЕ: этот тест может занять много памяти!
        let max_reasonable_size = 100 * 1024 * 1024; // 100MB
        let long_string = "a".repeat(max_reasonable_size);
        let value = Value::Str(Sds::from_str(&long_string));

        let mut buffer = Vec::new();
        write_value(&mut buffer, &value).unwrap();

        let mut cursor = std::io::Cursor::new(&buffer);
        let decoded = read_value(&mut cursor).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_empty_collections_stress() {
        // Много пустых коллекций - тест на правильную обработку граничных случаев
        let values = vec![
            Value::Array(vec![]),
            Value::Set(std::collections::HashSet::new()),
            // Добавьте другие пустые коллекции
        ];

        for _ in 0..10000 {
            for value in &values {
                let mut buffer = Vec::new();
                write_value(&mut buffer, value).unwrap();

                let mut cursor = std::io::Cursor::new(&buffer);
                let decoded = read_value(&mut cursor).unwrap();

                assert_eq!(*value, decoded);
            }
        }
    }

    #[test]
    fn test_compression_pathological_cases() {
        // Данные которые плохо сжимаются или становятся больше после сжатия
        let pathological_data = vec![
            // Случайные данные - плохо сжимаются
            (0..1000).map(|i| (i * 17 + 23) as u8).collect::<Vec<_>>(),
            // Данные на границе MIN_COMPRESSION_SIZE
            vec![b'a'; MIN_COMPRESSION_SIZE],
            vec![b'a'; MIN_COMPRESSION_SIZE + 1],
            vec![b'a'; MIN_COMPRESSION_SIZE - 1],
        ];

        for data in pathological_data {
            let value = Value::Str(Sds::from_vec(data));

            let mut buffer = Vec::new();
            write_value(&mut buffer, &value).unwrap();

            let mut cursor = std::io::Cursor::new(&buffer);
            let decoded = read_value(&mut cursor).unwrap();

            assert_eq!(value, decoded);
        }
    }

    #[test]
    #[ignore] // Помечено ignore потому что медленный - запускать вручную
    fn test_endurance_many_iterations() {
        // Тест на выносливость - много итераций для поиска memory leaks
        use std::collections::HashMap;

        let mut stats = HashMap::new();

        for i in 0..100_000 {
            if i % 10_000 == 0 {
                println!("Iteration {}", i);
            }

            let value = Value::Int(i);
            let mut buffer = Vec::new();
            write_value(&mut buffer, &value).unwrap();

            let mut cursor = std::io::Cursor::new(&buffer);
            let decoded = read_value(&mut cursor).unwrap();

            assert_eq!(value, decoded);

            // Собираем статистику размеров
            *stats.entry(buffer.len()).or_insert(0) += 1;
        }

        println!("Size distribution: {:?}", stats);
    }
}

/// Benchmark тесты (требует `cargo +nightly bench`)
#[cfg(test)]
mod benchmarks {
    use zumic::Sds;

    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn benchmark_encode_decode_speed() {
        let test_values = vec![
            Value::Int(42),
            Value::Str(Sds::from_str("hello world")),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            // Добавьте другие типичные значения
        ];

        let iterations = 10_000;

        for value in test_values {
            let start = Instant::now();

            for _ in 0..iterations {
                let mut buffer = Vec::new();
                write_value(&mut buffer, &value).unwrap();

                let mut cursor = std::io::Cursor::new(&buffer);
                let decoded = read_value(&mut cursor).unwrap();

                assert_eq!(value, decoded);
            }

            let elapsed = start.elapsed();
            let per_iter = elapsed / iterations;

            println!(
                "Value type: {:?}, Time per roundtrip: {:?}",
                std::mem::discriminant(&value),
                per_iter
            );

            // Проверяем что каждый roundtrip быстрее 10 микросекунд
            assert!(
                per_iter < Duration::from_micros(10),
                "Roundtrip too slow: {:?}",
                per_iter
            );
        }
    }
}
