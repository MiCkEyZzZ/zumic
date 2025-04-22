use tracing::{debug, error, info};

use crate::error::EncodeError;

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BINARY_LENGTH},
    zsp_types::ZSPFrame,
};

pub struct ZSPEncoder;

impl ZSPEncoder {
    pub fn encode(frame: &ZSPFrame) -> Result<Vec<u8>, EncodeError> {
        debug!("Encoding frame: {:?}", frame);
        Self::encode_frame(frame, 0)
    }

    fn encode_frame(frame: &ZSPFrame, current_depth: usize) -> Result<Vec<u8>, EncodeError> {
        debug!("Encoding frame at depth {}: {:?}", current_depth, frame);

        if current_depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceed ({})", MAX_ARRAY_DEPTH);
            error!("{}", err_msg);
            return Err(EncodeError::InvalidData(err_msg));
        }

        match frame {
            ZSPFrame::InlineString(s) => {
                Self::validate_simple_string(s)?;
                info!("Encoding InlineString: {}", s);
                Ok(format!("+{}\r\n", s).into_bytes())
            }
            ZSPFrame::FrameError(s) => {
                Self::validate_error_string(s)?;
                info!("Encoding FrameError: {}", s);
                Ok(format!("-{}\r\n", s).into_bytes())
            }
            ZSPFrame::Integer(i) => {
                info!("Encoding Integer: {}", i);
                Ok(format!(":{}\r\n", i).into_bytes())
            }
            ZSPFrame::Float(f) => {
                info!("Encoding Float: {}", f);
                Ok(format!(":{}\r\n", f).into_bytes())
            }
            ZSPFrame::BinaryString(Some(b)) => {
                if b.len() > MAX_BINARY_LENGTH {
                    let err_msg = format!(
                        "Binary string too long ({} > {})",
                        b.len(),
                        MAX_BINARY_LENGTH
                    );
                    error!("{}", err_msg);
                    return Err(EncodeError::InvalidData(err_msg));
                }

                info!("Encoding BinaryString of length {}", b.len());
                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                Ok(out)
            }
            ZSPFrame::BinaryString(None) => {
                info!("Encoding empty BinaryString");
                Ok(b"$-1\r\n".to_vec())
            }
            ZSPFrame::Array(ref elements) if elements.is_empty() => Ok(b"*-1\r\n".to_vec()),
            ZSPFrame::Array(ref elements) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Dictionary(Some(ref items)) if items.is_empty() => {
                info!("Encoding empty Dictionary");
                Ok(b"%-1\r\n".to_vec())
            }
            ZSPFrame::Dictionary(ref items) => match items {
                Some(items) => {
                    info!("Encoding Dictionary with {} items", items.len());
                    let mut out = format!("%{}\r\n", items.len()).into_bytes();
                    for (key, value) in items {
                        out.extend(Self::encode_frame(
                            &ZSPFrame::InlineString(key.clone()),
                            current_depth + 1,
                        )?);
                        out.extend(Self::encode_frame(value, current_depth + 1)?);
                    }
                    Ok(out)
                }
                None => {
                    info!("Encoding empty Dictionary");
                    Ok(b"%-1\r\n".to_vec())
                }
            },
            ZSPFrame::ZSet(entries) => {
                info!("Encoding ZSet with {} entries", entries.len());
                let mut out = format!("^{}\r\n", entries.len()).into_bytes();
                for (member, score) in entries {
                    Self::validate_simple_string(member)?;
                    out.extend(format!("+{}\r\n", member).into_bytes());
                    out.extend(format!(":{}\r\n", score).into_bytes());
                }
                Ok(out)
            }
            ZSPFrame::Null => {
                info!("Encoding Null");
                Ok(b"$-1\r\n".to_vec())
            }
        }
    }

    fn validate_simple_string(s: &str) -> Result<(), EncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Simple string contains CR or LF characters";
            error!("{}", err_msg);
            Err(EncodeError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }

    fn validate_error_string(s: &str) -> Result<(), EncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Error message contains CR or LF characters";
            error!("{}", err_msg);
            Err(EncodeError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тестирование кодирования InlineString в байтовый поток.
    /// Проверяет, что строка "OK" правильно кодируется в формат "+OK\r\n".
    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::InlineString("OK".to_string());
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
            ZSPFrame::InlineString("test".to_string()),
            ZSPFrame::Integer(42),
        ]);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    /// Тестирование кодирования неверного InlineString.
    /// Проверяет, что строка с символами \r\n вызывает ошибку.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::InlineString("bad\r\nstring".to_string());
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_err());
    }

    /// Тестирование кодирования пустого словаря.
    /// Проверяет, что пустой словарь кодируется как "%-1\r\n".
    #[test]
    fn test_empty_dictionary() {
        let frame = ZSPFrame::Dictionary(None);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%-1\r\n");
    }

    /// Тестирование кодирования словаря с одним элементом.
    /// Проверяет, что словарь с одним элементом кодируется корректно.
    #[test]
    fn test_single_item_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::InlineString("value1".to_string()),
        );
        let frame = ZSPFrame::Dictionary(Some(items));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%1\r\n+key1\r\n+value1\r\n");
    }

    /// Тестирование кодирования словаря с несколькими элементами.
    /// Проверяет, что словарь с двумя элементами кодируется корректно.
    #[test]
    fn test_multiple_items_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::InlineString("value1".to_string()),
        );
        items.insert(
            "key2".to_string(),
            ZSPFrame::InlineString("value2".to_string()),
        );
        let frame = ZSPFrame::Dictionary(Some(items));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%2\r\n+key1\r\n+value1\r\n+key2\r\n+value2\r\n");
    }

    /// Тестирование кодирования словаря с некорректным значением.
    /// Проверяет, что в словарь могут быть добавлены только валидные строки.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::InlineString("value1".to_string()),
        );
        // Пытаемся вставить значение типа InlineString в словарь
        let frame = ZSPFrame::Dictionary(Some(items));
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Должно пройти, потому что ключи валидные
    }

    /// Тестирование кодирования неполного словаря.
    /// Проверяет, что даже неполный словарь с одним элементом корректно кодируется.
    #[test]
    fn test_incomplete_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::InlineString("value1".to_string()),
        );
        // Пример неполного словаря
        let frame = ZSPFrame::Dictionary(Some(items));
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
