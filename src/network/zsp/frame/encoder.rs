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

    // Test encoding of SimpleString into a byte stream.
    // Checks that the string "OK" is correctly encoded into the format "+OK\r\n".
    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::SimpleString("OK".to_string());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    // Test encoding of BulkString into a byte stream.
    // Checks that the string "hello" is correctly encoded with length and content.
    #[test]
    fn test_builk_string() {
        let frame = ZSPFrame::BulkString(Some(b"hello".to_vec()));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    // Test the encoding of a nested array.
    // Checks that an array of two elements (a string and a number) is correctly encoded.
    #[test]
    fn test_nested_array() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("test".to_string()),
            ZSPFrame::Integer(42),
        ]));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    // Test encoding of an invalid SimpleString.
    // Checks that a string with \r\n characters causes an error.
    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::SimpleString("bad\r\nstring".to_string());
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_err());
    }

    // Test encoding of an empty dictionary.
    // Checks that an empty dictionary is encoded as "%-1\r\n".
    #[test]
    fn test_empty_dictionary() {
        let frame = ZSPFrame::Dictionary(None);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"%-1\r\n");
    }

    // Test encoding of a dictionary with one element.
    // Checks that a dictionary with one element is encoded correctly.
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

    // Test encoding of a dictionary with multiple elements.
    // Checks that a dictionary with two elements is encoded correctly.
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

    // Testing the encoding of a dictionary with an invalid value.
    // Checking that only valid strings can be added to the dictionary.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        // Let's try to insert a SimpleString type value into the dictionary
        let frame = ZSPFrame::Dictionary(Some(items));
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Должен пройти, потому что ключи валидные
    }

    // Testing encoding of an incomplete dictionary.
    // Checking that even an incomplete dictionary with one element is correctly encoded.
    #[test]
    fn test_incomplete_dictionary() {
        let mut items = std::collections::HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        // Example of an incomplete dictionary
        let frame = ZSPFrame::Dictionary(Some(items));
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_ok()); // Expect the dictionary to be encoded correctly
    }

    // Testing Float encoding.
    #[test]
    fn test_float_encoding() {
        let frame = ZSPFrame::Float(42.42);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":42.42\r\n");
    }

    // Test encoding of Float with negative value.
    #[test]
    fn test_negative_float_encoding() {
        let frame = ZSPFrame::Float(-42.42);
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b":-42.42\r\n");
    }
}
