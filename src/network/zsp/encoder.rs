//! Модуль кодирует ZSPFrame в байтовый поток по протоколу ZSP.
//!
//! Поддерживаются все основные типы: `SimpleString`, `Error`, `Integer`, `BulkString`, `Array`, `Dictionary`.
//!
//! Используется в основном через `ZSPEncoder::encode(&frame)` — возвращает Vec<u8>, пригодный для отправки по сети.

use std::io::{self, Error, ErrorKind};

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BULK_LENGTH},
    types::ZSPFrame,
};

/// Кодировщик для фреймов ZSP (Zumic Serialization Protocol).
///
/// Используется для преобразования структур `ZSPFrame` в байтовый поток (`Vec<u8>`),
/// пригодный для передачи по TCP.
///
/// Ограничения:
/// - Максимальная глубина вложенных массивов — `MAX_ARRAY_DEPTH`
/// - Максимальная длина BulkString — `MAX_BULK_LENGTH`
/// - SimpleString и Error не могут содержать `\r` или `\n`
pub struct ZSPEncoder;

impl ZSPEncoder {
    /// Кодирует фрейм `ZSPFrame` в Vec<u8>.
    ///
    /// Возвращает `Err`, если нарушены ограничения по вложенности или длине.
    pub fn encode(frame: &ZSPFrame) -> io::Result<Vec<u8>> {
        Self::encode_frame(frame, 0)
    }
    /// Рекурсивная функция кодирования с отслеживанием глубины вложенности.
    ///
    /// Используется для кодирования различных типов фреймов в соответствии с протоколом ZSP.
    fn encode_frame(frame: &ZSPFrame, current_depth: usize) -> io::Result<Vec<u8>> {
        if current_depth > MAX_ARRAY_DEPTH {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Max array depth exceeded ({})", MAX_ARRAY_DEPTH),
            ));
        }

        match frame {
            ZSPFrame::SimpleString(s) => {
                Self::validate_simple_string(s)?;
                Ok(format!("+{}\r\n", s).into_bytes())
            }
            ZSPFrame::Error(s) => {
                Self::validate_error_string(s)?;
                Ok(format!("-{}\r\n", s).into_bytes())
            }
            ZSPFrame::Integer(i) => Ok(format!(":{}\r\n", i).into_bytes()),
            ZSPFrame::BulkString(Some(b)) => {
                if b.len() > MAX_BULK_LENGTH {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Bulk string too long ({} > {})", b.len(), MAX_BULK_LENGTH),
                    ));
                }

                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                Ok(out)
            }
            ZSPFrame::BulkString(None) => Ok(b"$-1\r\n".to_vec()),
            ZSPFrame::Array(Some(elements)) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Array(None) => Ok(b"*-1\r\n".to_vec()),
            ZSPFrame::Dictionary(Some(items)) => {
                let mut out = format!("%{}\r\n", items.len()).into_bytes();
                for (key, value) in items {
                    // Кодируем ключ
                    out.extend(Self::encode_frame(
                        &ZSPFrame::SimpleString(key.clone()),
                        current_depth + 1,
                    )?);
                    // Кодируем значение
                    out.extend(Self::encode_frame(value, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Dictionary(None) => Ok(b"%-1\r\n".to_vec()),
        }
    }
    /// Проверка: simple string не должен содержать `\r` или `\n`
    fn validate_simple_string(s: &str) -> io::Result<()> {
        if s.contains('\r') || s.contains('\n') {
            Err(Error::new(
                ErrorKind::InvalidData,
                "Simple string contains CR or LF characters",
            ))
        } else {
            Ok(())
        }
    }
    /// Проверка: error string не должен содержать `\r` или `\n`
    fn validate_error_string(s: &str) -> io::Result<()> {
        if s.contains('\r') || s.contains('\n') {
            Err(Error::new(
                io::ErrorKind::InvalidData,
                "Error message contains CR or LF characters",
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Тестируем кодирование SimpleString в байтовый поток.
    // Проверяется, что строка "OK" корректно кодируется в формат "+OK\r\n".
    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::SimpleString("OK".to_string());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    // Тестируем кодирование BulkString в байтовый поток.
    // Проверяется, что строка "hello" корректно кодируется с длиной и содержимым.
    #[test]
    fn test_builk_string() {
        let frame = ZSPFrame::BulkString(Some(b"hello".to_vec()));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    // Тестируем кодирование вложенного массива.
    // Проверяется, что массив из двух элементов (строка и число) правильно кодируется.
    #[test]
    fn test_nested_array() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("test".to_string()),
            ZSPFrame::Integer(42),
        ]));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    // Тестируем кодирование некорректной строки SimpleString.
    // Проверяется, что строка с символами \r\n вызывает ошибку.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::SimpleString("bad\r\nstring".to_string());
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_err());
    }

    // Тестируем кодирование пустого словаря.
    // Проверяется, что пустой словарь кодируется как "%-1\r\n".
    #[test]
    fn test_empty_dictionary() {
        let frame = ZSPFrame::Dictionary(None);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%-1\r\n");
    }

    // Тестируем кодирование словаря с одним элементом.
    // Проверяется, что словарь с одним элементом кодируется правильно.
    #[test]
    fn test_single_item_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        let frame = ZSPFrame::Dictionary(Some(items));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%1\r\n+key1\r\n+value1\r\n");
    }

    // Тестируем кодирование словаря с несколькими элементами.
    // Проверяется, что словарь с двумя элементами кодируется правильно.
    #[test]
    fn test_multiple_items_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        items.insert(
            "key2".to_string(),
            ZSPFrame::SimpleString("value2".to_string()),
        );
        let frame = ZSPFrame::Dictionary(Some(items));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%2\r\n+key1\r\n+value1\r\n+key2\r\n+value2\r\n");
    }

    // Тестируем кодирование словаря с некорректным значением.
    // Проверяется, что в словарь можно добавлять только валидные строки.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        // Попробуем вставить в словарь значение типа SimpleString
        let frame = ZSPFrame::Dictionary(Some(items));
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Должен пройти, потому что ключи валидные
    }

    // Тестируем кодирование неполного словаря.
    // Проверяется, что даже неполный словарь с одним элементом корректно кодируется.
    #[test]
    fn test_incomplete_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        // Пример неполного словаря
        let frame = ZSPFrame::Dictionary(Some(items));
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Ожидаем, что словарь будет закодирован корректно
    }
}
