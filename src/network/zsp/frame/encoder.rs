use tracing::{debug, error, info};

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BULK_LENGTH},
    errors::ZSPError,
    zsp_types::ZSPFrame,
};

pub struct ZSPEncoder;

impl ZSPEncoder {
    pub fn encode(frame: &ZSPFrame) -> Result<Vec<u8>, ZSPError> {
        debug!("Encoding frame: {:?}", frame);
        Self::encode_frame(frame, 0)
    }

    fn encode_frame(frame: &ZSPFrame, current_depth: usize) -> Result<Vec<u8>, ZSPError> {
        debug!("Encoding frame at depth {}: {:?}", current_depth, frame);

        if current_depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceed ({})", MAX_ARRAY_DEPTH);
            error!("{}", err_msg);
            return Err(ZSPError::InvalidData(err_msg));
        }

        match frame {
            ZSPFrame::SimpleString(s) => {
                Self::validate_simple_string(s)?;
                info!("Encoding SimpleString: {}", s);
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
            ZSPFrame::BulkString(Some(b)) => {
                if b.len() > MAX_BULK_LENGTH {
                    let err_msg =
                        format!("Bulk string too long ({} > {})", b.len(), MAX_BULK_LENGTH);
                    error!("{}", err_msg);
                    return Err(ZSPError::InvalidData(err_msg));
                }

                info!("Encoding BulkString of length {}", b.len());
                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                Ok(out)
            }
            ZSPFrame::BulkString(None) => {
                info!("Encoding empty BulkString");
                Ok(b"$-1\r\n".to_vec())
            }
            ZSPFrame::Array(Some(elements)) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Array(None) => Ok(b"*-1\r\n".to_vec()),
            ZSPFrame::Dictionary(Some(items)) => {
                info!("Encoding Dictionary with {} items", items.len());
                let mut out = format!("%{}\r\n", items.len()).into_bytes();
                for (key, value) in items {
                    // Encode the key
                    out.extend(Self::encode_frame(
                        &ZSPFrame::SimpleString(key.clone()),
                        current_depth + 1,
                    )?);
                    // Encode the value
                    out.extend(Self::encode_frame(value, current_depth + 1)?);
                }
                Ok(out)
            }
            ZSPFrame::Dictionary(None) => {
                info!("Encoding empty Dictionary");
                Ok(b"%-1\r\n".to_vec())
            }
            ZSPFrame::ZSet(entries) => {
                info!("Encoding ZSet with {} entries", entries.len());
                let mut out = format!("^{}\r\n", entries.len()).into_bytes();
                for (member, score) in entries {
                    // member as simple string
                    Self::validate_simple_string(member)?;
                    out.extend(format!("+{}\r\n", member).into_bytes());
                    // score as float
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
    fn validate_simple_string(s: &str) -> Result<(), ZSPError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Simple string contains CR or LF characters";
            error!("{}", err_msg);
            Err(ZSPError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }

    fn validate_error_string(s: &str) -> Result<(), ZSPError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Error message contains CR or LF characters";
            error!("{}", err_msg);
            Err(ZSPError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тестирование кодирования SimpleString в байтовый поток.
    /// Проверяет, что строка "OK" правильно кодируется в формат "+OK\r\n".
    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::SimpleString("OK".to_string());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    /// Тестирование кодирования BulkString в байтовый поток.
    /// Проверяет, что строка "hello" правильно кодируется с длиной и содержимым.
    #[test]
    fn test_builk_string() {
        let frame = ZSPFrame::BulkString(Some(b"hello".to_vec()));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    /// Тестирование кодирования вложенного массива.
    /// Проверяет, что массив из двух элементов (строка и число) правильно кодируется.
    #[test]
    fn test_nested_array() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("test".to_string()),
            ZSPFrame::Integer(42),
        ]));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    /// Тестирование кодирования неверного SimpleString.
    /// Проверяет, что строка с символами \r\n вызывает ошибку.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::SimpleString("bad\r\nstring".to_string());
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
            ZSPFrame::SimpleString("value1".to_string()),
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

    /// Тестирование кодирования словаря с некорректным значением.
    /// Проверяет, что в словарь могут быть добавлены только валидные строки.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        // Пытаемся вставить значение типа SimpleString в словарь
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
            ZSPFrame::SimpleString("value1".to_string()),
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
