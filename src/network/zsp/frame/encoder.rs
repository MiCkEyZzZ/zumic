//! Энкодер ZSP (Zumic Serialization Protocol).
//!
//! Этот модуль предоставляет функциональность для кодирования
//! различных типов данных в формат ZSP (Zumic Serialization
//! Protocol), включая строки, числа, массивы, бинарные строки,
//! словари и ZSet. Также реализована валидация строк и глубины
//! вложенности массивов для предотвращения ошибок сериализации.

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BINARY_LENGTH},
    zsp_types::ZspFrame,
};
use crate::ZspEncodeError;

/// Структура энкодера для кодирования в формат ZSP.
pub struct ZspEncoder;

impl ZspEncoder {
    pub fn new() -> Self {
        ZspEncoder
    }

    pub fn encode(frame: &ZspFrame) -> Result<Vec<u8>, ZspEncodeError> {
        Self::encode_frame(frame, 0)
    }

    fn encode_frame(
        frame: &ZspFrame,
        current_depth: usize,
    ) -> Result<Vec<u8>, ZspEncodeError> {
        if current_depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceed ({MAX_ARRAY_DEPTH})");
            return Err(ZspEncodeError::InvalidData(err_msg));
        }

        match frame {
            // ZSP: Simple String - +OK\r\n
            ZspFrame::InlineString(s) => {
                Self::validate_simple_string(s)?;
                Ok(format!("+{s}\r\n").into_bytes())
            }
            // ZSP: Error - -ERR message\r\n
            ZspFrame::FrameError(s) => {
                Self::validate_error_string(s)?;
                Ok(format!("-{s}\r\n").into_bytes())
            }
            // ZSP: Integer - :123\r\n
            ZspFrame::Integer(i) => Ok(format!(":{i}\r\n").into_bytes()),
            // ZSP: Double (Float) - ,3.14\r\n
            ZspFrame::Float(f) => {
                // ZSP поддерживает inf и -inf
                if f.is_infinite() {
                    if f.is_sign_positive() {
                        Ok(b",inf\r\n".to_vec())
                    } else {
                        Ok(b",-inf\r\n".to_vec())
                    }
                } else {
                    Ok(format!(",{f:.5}\r\n").into_bytes())
                }
            }
            // ZSP: Boolean - #t\r\n или #f\r\n
            ZspFrame::Bool(b) => {
                if *b {
                    Ok(b"#t\r\n".to_vec())
                } else {
                    Ok(b"#f\r\n".to_vec())
                }
            }
            // ZSP: Binary String - $5\r\nhello\r\n
            ZspFrame::BinaryString(Some(b)) => {
                if b.len() > MAX_BINARY_LENGTH {
                    let err_msg = format!(
                        "Binary string too long ({} > {})",
                        b.len(),
                        MAX_BINARY_LENGTH
                    );
                    return Err(ZspEncodeError::InvalidData(err_msg));
                }

                let mut out = format!("${}\r\n", b.len()).into_bytes();
                out.extend(b);
                out.extend(b"\r\n");
                Ok(out)
            }
            // ZSP: Null Binary String - $-1\r\n или ZSP Null: _\r\n
            ZspFrame::BinaryString(None) => Ok(b"$-1\r\n".to_vec()),
            // ZSP: Array - *2\r\n+hello\r\n:123\r\n
            ZspFrame::Array(ref elements) if elements.is_empty() => Ok(b"*0\r\n".to_vec()),
            ZspFrame::Array(ref elements) => {
                let mut out = format!("*{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            // ZSP: Map (Dictionary) - %2\r\n+key1\r\n+val1\r\n+key2\r\n+val2\r\n
            // ИСПРАВЛЕНО: пустой словарь %0\r\n, ключи как BulkString
            ZspFrame::Dictionary(ref items) => {
                if items.is_empty() {
                    // Если словарь пустой, возвращаем пустой Map это %0\r\n
                    return Ok(b"%0\r\n".to_vec());
                }

                let mut out = format!("%{}\r\n", items.len()).into_bytes();
                for (key, value) in items {
                    let key_bytes = key.as_bytes();
                    out.extend(format!("${}\r\n", key_bytes.len()).into_bytes());
                    out.extend(key_bytes);
                    out.extend(b"\r\n");
                    out.extend(Self::encode_frame(value, current_depth + 1)?);
                }
                Ok(out)
            }
            // ZSP: Set - ~3\r\n+member1\r\n+member2\r\n+member3\r\n
            ZspFrame::Set(ref members) => {
                if members.is_empty() {
                    return Ok(b"~0\r\n".to_vec());
                }

                let mut out = format!("~{}\r\n", members.len()).into_bytes();
                for member in members {
                    out.extend(Self::encode_frame(member, current_depth + 1)?);
                }
                Ok(out)
            }
            // ZSP: Push - >3\r\n+message\r\n+channel\r\n+payload\r\n
            ZspFrame::Push(elements) => {
                let mut out = format!(">{}\r\n", elements.len()).into_bytes();
                for e in elements {
                    out.extend(Self::encode_frame(e, current_depth + 1)?);
                }
                Ok(out)
            }
            // ZSP: ZSet - ^2\r\n+member1\r\n,1.5\r\n+member2\r\n,2.5\r\n
            ZspFrame::ZSet(entries) => {
                let mut out = format!("^{}\r\n", entries.len()).into_bytes();
                for (member, score) in entries {
                    // Член как BulkString (более безопасно чем SimpleString)
                    let member_bytes = member.as_bytes();
                    out.extend(format!("${}\r\n", member_bytes.len()).into_bytes());
                    out.extend(member_bytes);
                    out.extend(b"\r\n");

                    // Счёт как Double
                    out.extend(format!(",{score}\r\n").into_bytes());
                }
                Ok(out)
            }
            // ZSP: Null - _\r\n
            ZspFrame::Null => Ok(b"_\r\n".to_vec()),
        }
    }

    fn validate_simple_string(s: &str) -> Result<(), ZspEncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Simple string contains CR or LF characters";
            Err(ZspEncodeError::InvalidData(err_msg.into()))
        } else {
            Ok(())
        }
    }

    fn validate_error_string(s: &str) -> Result<(), ZspEncodeError> {
        if s.contains('\r') || s.contains('\n') {
            let err_msg = "Error message contains CR or LF characters";
            Err(ZspEncodeError::InvalidData(err_msg.into()))
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
    use std::{
        collections::{HashMap, HashSet},
        f64::consts::PI,
    };

    use super::*;

    #[test]
    fn test_simple_string() {
        let frame = ZspFrame::InlineString("OK".into());
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    #[test]
    fn test_error() {
        let frame = ZspFrame::FrameError("ERR unknown command".into());
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"-ERR unknown command\r\n");
    }

    #[test]
    fn test_integer() {
        let frame = ZspFrame::Integer(42);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b":42\r\n");
    }

    #[test]
    fn test_float() {
        let frame = ZspFrame::Float(PI);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b",3.14159\r\n");
    }

    #[test]
    fn test_float_infinity() {
        let frame = ZspFrame::Float(f64::INFINITY);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b",inf\r\n");

        let frame = ZspFrame::Float(f64::NEG_INFINITY);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b",-inf\r\n");
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

    #[test]
    fn test_binary_string() {
        let frame = ZspFrame::BinaryString(Some(b"hello".to_vec()));
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_null_binary_string() {
        let frame = ZspFrame::BinaryString(None);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"$-1\r\n");
    }

    #[test]
    fn test_null() {
        let frame = ZspFrame::Null;
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"_\r\n");
    }

    #[test]
    fn test_empty_array() {
        let frame = ZspFrame::Array(vec![]);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"*0\r\n");
    }

    #[test]
    fn test_array() {
        let frame = ZspFrame::Array(vec![
            ZspFrame::InlineString("test".into()),
            ZspFrame::Integer(42),
        ]);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    #[test]
    fn test_nested_array() {
        let frame = ZspFrame::Array(vec![
            ZspFrame::Array(vec![ZspFrame::Integer(1), ZspFrame::Integer(2)]),
            ZspFrame::Bool(true),
        ]);
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n*2\r\n:1\r\n:2\r\n#t\r\n");
    }

    #[test]
    fn test_empty_map() {
        let frame = ZspFrame::Dictionary(HashMap::new());
        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%0\r\n");
    }

    #[test]
    fn test_map_with_binary_string_keys() {
        let mut items = HashMap::new();
        items.insert("key1".into(), ZspFrame::InlineString("value1".into()));
        let frame = ZspFrame::Dictionary(items);

        let encoded = ZspEncoder::encode(&frame).unwrap();
        // Ключи теперь BinaryString
        assert_eq!(encoded, b"%1\r\n$4\r\nkey1\r\n+value1\r\n");
    }

    #[test]
    fn test_empty_set() {
        let frame = ZspFrame::Set(HashSet::new());
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"~0\r\n");
    }

    #[test]
    fn test_set() {
        let mut members = HashSet::new();
        members.insert(ZspFrame::InlineString("member1".into()));
        members.insert(ZspFrame::InlineString("member2".into()));
        let frame = ZspFrame::Set(members);

        let encoded = ZspEncoder::encode(&frame).unwrap();
        // Порядок может быть любой из-за HashSet
        assert!(encoded.starts_with(b"~2\r\n"));
    }

    #[test]
    fn test_push() {
        let frame = ZspFrame::Push(vec![
            ZspFrame::InlineString("message".into()),
            ZspFrame::InlineString("channel".into()),
            ZspFrame::BinaryString(Some(b"payload".to_vec())),
        ]);

        let encoded = ZspEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b">3\r\n+message\r\n+channel\r\n$7\r\npayload\r\n");
    }

    #[test]
    fn test_zset_empty() {
        let frame = ZspFrame::ZSet(vec![]);
        assert_eq!(ZspEncoder::encode(&frame).unwrap(), b"^0\r\n");
    }

    #[test]
    fn test_zset() {
        let entries = vec![("member1".to_string(), 1.5), ("member2".to_string(), 2.5)];
        let frame = ZspFrame::ZSet(entries);
        let encoded = ZspEncoder::encode(&frame).unwrap();

        // ZSet с BinaryString для членов (безопаснее)
        assert_eq!(
            encoded,
            b"^2\r\n$7\r\nmember1\r\n,1.5\r\n$7\r\nmember2\r\n,2.5\r\n"
        );
    }

    #[test]
    fn test_invalid_simple_string() {
        let frame = ZspFrame::InlineString("bad\r\nstring".into());
        let result = ZspEncoder::encode(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_error_string() {
        let frame = ZspFrame::FrameError("bad\nerror".into());
        let result = ZspEncoder::encode(&frame);
        assert!(result.is_err());
    }
}
