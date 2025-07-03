//! Модуль для сжатия и распаковки блоков данных в ZDB с
//! помощью ZSTD.
//!
//! Содержит утилиты для решения, когда применять сжатие,
//! а также функции для компрессии и декомпрессии.

use std::io;
use zstd::stream::{decode_all, encode_all};

/// Минимальный размер в байтах, при котором стоит применять
/// сжатие.
/// Если длина блока данных меньше этой константы, сжатие не
/// выполняется.
const MIN_COMPRESSION_SIZE: usize = 64;

/// Проверяет, нужно ли пытаться сжать блок данных заданного
/// размера.
///
/// # Аргументы
///
/// * `size` — длина блока данных в байтах.
///
/// # Возвращает
///
/// `true`, если `size >= MIN_COMPRESSION_SIZE`, иначе `false`.
pub fn should_compress(size: usize) -> bool {
    size >= MIN_COMPRESSION_SIZE
}

/// Сжимает переданный срез байтов с помощью алгоритма ZSTD.
///
/// Использует уровень компрессии 3, который обеспечивает баланс
/// между скоростью и степенью сжатия.
///
/// # Аргументы
///
/// * `data` — исходный срез байтов для сжатия.
///
/// # Возвращает
///
/// `Ok(Vec<u8>)` с сжатыми данными или `Err` с ошибкой ввода-вывода.
pub fn compress_block(data: &[u8]) -> io::Result<Vec<u8>> {
    // Уровень сжатия: 3 — баланс между скоростью и размером
    encode_all(data, 3)
}

/// Распаковывает блок данных, сжатых с помощью ZSTD.
///
/// Попытка декомпрессии всегда выполняется, даже если размер блока
/// мал.
///
/// # Аргументы
///
/// * `data` — срез байтов с заранее сжатыми данными.
///
/// # Возвращает
///
/// `Ok(Vec<u8>)` с декомпрессированными данными или `Err` с ошибкой.
pub fn decompress_block(data: &[u8]) -> io::Result<Vec<u8>> {
    decode_all(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет пограничные условия функции `should_compress`:
    /// - возврат `false` для размеров ниже порога;
    /// - возврат `true` для размеров на пороге и выше.
    #[test]
    fn test_should_compress_threshold() {
        // Ниже порога
        assert!(!should_compress(0));
        assert!(!should_compress(MIN_COMPRESSION_SIZE - 1));
        // На пороге и выше
        assert!(should_compress(MIN_COMPRESSION_SIZE));
        assert!(should_compress(MIN_COMPRESSION_SIZE + 1));
    }

    /// Тест проверяет, что сжатие и последующая декомпрессия маленького
    /// блока возвращают исходные данные.
    #[test]
    fn test_compress_decompress_roundtrip_small() {
        let data = b"short data";
        // даже если мы не будем использовать compress_block при write, сама библиотека работает
        let compressed = compress_block(data).expect("compress failed");
        let decompressed = decompress_block(&compressed).expect("decompress failed");
        assert_eq!(&decompressed, data);
    }

    /// Тест проверяет корректность сжатия и декомпрессии для блока данных,
    /// размер которого превосходит порог `MIN_COMPRESSION_SIZE`.
    #[test]
    fn test_compress_decompress_roundtrip_large() {
        // создаём буфер > MIN_COMPRESSION_SIZE
        let data: Vec<u8> = (0..(MIN_COMPRESSION_SIZE * 2))
            .map(|i| (i % 256) as u8)
            .collect();
        assert!(should_compress(data.len()));
        let compressed = compress_block(&data).expect("compress failed");
        // Убедимся, что что-то действительно сжалось (или хотя бы буфер непуст)
        assert!(!compressed.is_empty());
        let decompressed = decompress_block(&compressed).expect("decompress failed");
        assert_eq!(decompressed, data);
    }

    /// Тест проверяет, что при передаче некорректных данных в `decompress_block`
    /// возвращается ошибка с типом `ErrorKind::Other`.
    #[test]
    fn test_decompress_invalid_data() {
        // Некорректные данные приводят к ошибке
        let bad = vec![0u8; 10];
        let err = decompress_block(&bad).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }
}
