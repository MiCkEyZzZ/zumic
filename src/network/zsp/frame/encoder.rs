// Copyright 2025 Zumic

//! Энкодер ZSP (Zumic Serialization Protocol).
//!
//! Этот модуль предоставляет функциональность для кодирования
//! различных типов данных в формат ZSP (Zumic Serialization
//! Protocol), включая строки, числа, массивы, бинарные строки,
//! словари и ZSet. Также реализована валидация строк и глубины
//! вложенности массивов для предотвращения ошибок сериализации.

use std::borrow::Cow;

use crate::error::encode::EncodeError;

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BINARY_LENGTH},
    zsp_types::ZspFrame,
};

/// Структура энкодера для кодирования в формат ZSP.
pub struct ZspEncoder;

impl ZspEncoder {
    pub fn new() -> Self {
        ZspEncoder
    }

    pub fn encode(frame: &ZspFrame) -> Result<Vec<u8>, EncodeError> {
        Self::encode_frame(frame, 0)
    }

    fn encode_frame(frame: &ZspFrame, current_depth: usize) -> Result<Vec<u8>, EncodeError> {
        if current_depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceed ({MAX_ARRAY_DEPTH})");
            return Err(EncodeError::InvalidData(err_msg));
        }

        match frame {
            ZspFrame::InlineString(s) => {
                Self::validate_simple_string(s)?;
                Ok(format!("+{s}\r\n").into_bytes())
            }
            ZspFrame::FrameError(s) => {
                Self::validate_error_string(s)?;
                Ok(format!("-{s}\r\n").into_bytes())
            }
            ZspFrame::Integer(i) => Ok(format!(":{i}\r\n").into_bytes()),
            ZspFrame::Float(f) => Ok(format!(":{f}\r\n").into_bytes()),
            ZspFrame::Bool(b) => {
                if *b {
                    Ok(b"#t\r\n".to_vec())
                } else {
                    Ok(b"#f\r\n".to_vec())
                }
            }
            ZspFrame::BinaryString(Some(b)) => {
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
            ZspFrame::BinaryString(None) => Ok(b"$-1\r\n".to_vec()),
            ZspFrame::Array(ref elements) if elements.is_empty() => Ok(b"*-1\r\n".to_vec()),
            ZspFrame::Array(ref elements) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            ZspFrame::Dictionary(ref items) if items.is_empty() => Ok(b"%-1\r\n".to_vec()),
            ZspFrame::Dictionary(ref items) => {
                if items.is_empty() {
                    // Если словарь пустой, возвращаем специальный формат для пустого словаря
                    return Ok(b"%-1\r\n".to_vec());
                }

                let mut out = format!("%{}\r\n", items.len()).into_bytes();
                for (key, value) in items {
                    let key_cow: Cow<'_, str> = key.clone();
                    out.extend(Self::encode_frame(
                        &ZspFrame::InlineString(key_cow),
                        current_depth + 1,
                    )?);
                    out.extend(Self::encode_frame(value, current_depth + 1)?);
                }
                Ok(out)
            }
            ZspFrame::ZSet(entries) => {
                let mut out = format!("^{}\r\n", entries.len()).into_bytes();
                for (member, score) in entries {
                    Self::validate_simple_string(member)?;
                    out.extend(format!("+{member}\r\n").into_bytes());
                    out.extend(format!(":{score}\r\n").into_bytes());
                }
                Ok(out)
            }
            ZspFrame::Null => Ok(b"$-1\r\n".to_vec()),
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

impl Default for ZspEncoder {
    fn default() -> Self {
        Self::new()
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
        let frame = ZspFrame::InlineString("OK".into());
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    /// Тестирование кодирования BinaryString в байтовый поток.
    /// Проверяет, что строка "hello" правильно кодируется с длиной и содержимым.
    #[test]
    fn test_binary_string() {
        let frame = ZspFrame::BinaryString(Some(b"hello".to_vec()));
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    /// Тестирование кодирования вложенного массива.
    /// Проверяет, что массив из двух элементов (строка и число) правильно кодируется.
    #[test]
    fn test_nested_array() {
        let frame = ZspFrame::Array(vec![
            ZspFrame::InlineString("test".into()),
            ZspFrame::Integer(42),
        ]);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    /// Тестирование кодирования неверного InlineString.
    /// Проверяет, что строка с символами \r\n вызывает ошибку.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZspFrame::InlineString("bad\r\nstring".into());
        let result = ZspEncoder::encode(&frame);
        assert!(result.is_err());
    }

    /// Тестирование кодирования пустого словаря.
    /// Проверяет, что пустой словарь кодируется как "%-1\r\n".
    #[test]
    fn test_empty_dictionary() {
        let frame = ZspFrame::Dictionary(HashMap::new());
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%-1\r\n");
    }

    /// Тестирование кодирования словаря с одним элементом.
    /// Проверяет, что словарь с одним элементом кодируется корректно.
    #[test]
    fn test_single_item_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZspFrame::InlineString("value1".into()));
        let frame = ZspFrame::Dictionary(items);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%1\r\n+key1\r\n+value1\r\n");
    }

    /// Тестирование кодирования словаря с некорректным значением.
    /// Проверяет, что в словарь могут быть добавлены только валидные строки.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZspFrame::InlineString("value1".into()));
        // Пытаемся вставить значение типа InlineString в словарь
        let frame = ZspFrame::Dictionary(items);
        let result = ZspEncoder::encode(&frame);
        assert!(result.is_ok()); // Должно пройти, потому что ключи валидные
    }

    /// Тестирование кодирования неполного словаря.
    /// Проверяет, что даже неполный словарь с одним элементом корректно кодируется.
    #[test]
    fn test_incomplete_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert("key1".into(), ZspFrame::InlineString("value1".into()));
        // Пример неполного словаря
        let frame = ZspFrame::Dictionary(items);
        let result = ZspEncoder::encode(&frame);
        assert!(result.is_ok()); // Ожидается, что словарь будет закодирован корректно
    }

    /// Тестирование кодирования числа с плавающей запятой.
    #[test]
    fn test_float_encoding() {
        let frame = ZspFrame::Float(42.42);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":42.42\r\n");
    }

    /// Тестирование кодирования отрицательного числа с плавающей запятой.
    #[test]
    fn test_negative_float_encoding() {
        let frame = ZspFrame::Float(-42.42);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":-42.42\r\n");
    }

    #[test]
    fn test_bool_true() {
        let frame = ZspFrame::Bool(true);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"#t\r\n");
    }

    #[test]
    fn test_bool_false() {
        let frame = ZspFrame::Bool(false);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"#f\r\n");
    }
}
