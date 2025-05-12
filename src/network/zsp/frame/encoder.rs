//! Энкодер ZSP (Zumic Serialization Protocol).
//!
//! Этот модуль предоставляет функциональность для кодирования
//! различных типов данных в формат ZSP (Zumic Serialization
//! Protocol), включая строки, числа, массивы, бинарные строки,
//! словари и ZSet. Также реализована валидация строк и глубины
//! вложенности массивов для предотвращения ошибок сериализации.

use std::borrow::Cow;

use super::{
    ZSPFrame, {MAX_ARRAY_DEPTH, MAX_BINARY_LENGTH},
};
use crate::EncodeError;

/// Структура энкодера для кодирования в формат ZSP.
pub struct ZSPEncoder;

impl ZSPEncoder {
    pub fn encode(frame: &ZSPFrame) -> Result<Vec<u8>, EncodeError> {
        Self::encode_frame(frame, 0)
    }

    fn encode_frame(frame: &ZSPFrame, current_depth: usize) -> Result<Vec<u8>, EncodeError> {
        if current_depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceed ({MAX_ARRAY_DEPTH})");
            return Err(EncodeError::InvalidData(err_msg));
        }

        match frame {
            ZSPFrame::InlineString(s) => {
                Self::validate_simple_string(s)?;
                Ok(format!("+{s}\r\n").into_bytes())
            }
            ZSPFrame::FrameError(s) => {
                Self::validate_error_string(s)?;
                Ok(format!("-{s}\r\n").into_bytes())
            }
            ZSPFrame::Integer(i) => Ok(format!(":{i}\r\n").into_bytes()),
            ZSPFrame::Float(f) => Ok(format!(":{f}\r\n").into_bytes()),
            ZSPFrame::BinaryString(Some(b)) => {
                if b.len() > MAX_BINARY_LENGTH {
                    let err_msg = format!(
                        "Binary string too long ({} > {})",
                        b.len(),
                        MAX_BINARY_LENGTH
                    );
                    return Err(EncodeError::InvalidData(err_msg));
                }

                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                Ok(out)
            }
            ZSPFrame::BinaryString(None) => Ok(b"$-1\r\n".to_vec()),
            ZSPFrame::Array(ref elements) if elements.is_empty() => Ok(b"*-1\r\n".to_vec()),
            ZSPFrame::Array(ref elements) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Dictionary(ref items) if items.is_empty() => Ok(b"%-1\r\n".to_vec()),
            ZSPFrame::Dictionary(ref items) => {
                if items.is_empty() {
                    // Если словарь пустой, возвращаем специальный формат для пустого словаря
                    return Ok(b"%-1\r\n".to_vec());
                }

                let mut out = format!("%{}\r\n", items.len()).into_bytes();
                for (key, value) in items {
                    let key_cow: Cow<'_, str> = key.clone();
                    out.extend(Self::encode_frame(
                        &ZSPFrame::InlineString(key_cow),
                        current_depth + 1,
                    )?);
                    out.extend(Self::encode_frame(value, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::ZSet(entries) => {
                let mut out = format!("^{}\r\n", entries.len()).into_bytes();
                for (member, score) in entries {
                    Self::validate_simple_string(member)?;
                    out.extend(format!("+{member}\r\n").into_bytes());
                    out.extend(format!(":{score}\r\n").into_bytes());
                }
                Ok(out)
            }
            ZSPFrame::Null => Ok(b"$-1\r\n".to_vec()),
        }
    }

    fn validate_simple_string(s: &str) -> Result<(), EncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Simple string contains CR or LF characters";
            Err(EncodeError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }

    fn validate_error_string(s: &str) -> Result<(), EncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Error message contains CR or LF characters";
            Err(EncodeError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Тестирование кодирования InlineString в байтовый поток.
    /// Проверяет, что строка "OK" правильно кодируется в формат "+OK\r\n".
    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::InlineString("OK".into());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    /// Тестирование кодирования BinaryString в байтовый поток.
    /// Проверяет, что строка "hello" правильно кодируется с длиной и содержимым.
    #[test]
    fn test_binary_string() {
        let frame = ZSPFrame::BinaryString(Some(b"hello".to_vec()));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    /// Тестирование кодирования вложенного массива.
    /// Проверяет, что массив из двух элементов (строка и число) правильно кодируется.
    #[test]
    fn test_nested_array() {
        let frame = ZSPFrame::Array(vec![
            ZSPFrame::InlineString("test".into()),
            ZSPFrame::Integer(42),
        ]);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    /// Тестирование кодирования неверного InlineString.
    /// Проверяет, что строка с символами \r\n вызывает ошибку.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::InlineString("bad\r\nstring".into());
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_err());
    }

    /// Тестирование кодирования пустого словаря.
    /// Проверяет, что пустой словарь кодируется как "%-1\r\n".
    #[test]
    fn test_empty_dictionary() {
        let frame = ZSPFrame::Dictionary(HashMap::new());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%-1\r\n");
    }

    /// Тестирование кодирования словаря с одним элементом.
    /// Проверяет, что словарь с одним элементом кодируется корректно.
    #[test]
    fn test_single_item_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZSPFrame::InlineString("value1".into()));
        let frame = ZSPFrame::Dictionary(items);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%1\r\n+key1\r\n+value1\r\n");
    }

    /// Тестирование кодирования словаря с несколькими элементами.
    /// Проверяет, что словарь с двумя элементами кодируется корректно.
    #[test]
    fn test_multiple_items_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZSPFrame::InlineString("value1".into()));
        items.insert("key2".into(), ZSPFrame::InlineString("value2".into()));
        let frame = ZSPFrame::Dictionary(items);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%2\r\n+key1\r\n+value1\r\n+key2\r\n+value2\r\n");
    }

    /// Тестирование кодирования словаря с некорректным значением.
    /// Проверяет, что в словарь могут быть добавлены только валидные строки.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZSPFrame::InlineString("value1".into()));
        // Пытаемся вставить значение типа InlineString в словарь
        let frame = ZSPFrame::Dictionary(items);
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Должно пройти, потому что ключи валидные
    }

    /// Тестирование кодирования неполного словаря.
    /// Проверяет, что даже неполный словарь с одним элементом корректно кодируется.
    #[test]
    fn test_incomplete_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZSPFrame::InlineString("value1".into()));
        // Пример неполного словаря
        let frame = ZSPFrame::Dictionary(items);
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Ожидается, что словарь будет закодирован корректно
    }

    /// Тестирование кодирования числа с плавающей запятой.
    #[test]
    fn test_float_encoding() {
        let frame = ZSPFrame::Float(42.42);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":42.42\r\n");
    }

    /// Тестирование кодирования отрицательного числа с плавающей запятой.
    #[test]
    fn test_negative_float_encoding() {
        let frame = ZSPFrame::Float(-42.42);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":-42.42\r\n");
    }
}
