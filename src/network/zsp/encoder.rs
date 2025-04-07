use std::io::{self, Error, ErrorKind};

use super::{
    decoder::{MAX_ARRAY_DEPTH, MAX_BULK_LENGTH},
    types::ZSPFrame,
};

pub struct ZSPEncoder;

impl ZSPEncoder {}

impl ZSPEncoder {
    pub fn encode(frame: &ZSPFrame) -> io::Result<Vec<u8>> {
        Self::encode_frame(frame, 0)
    }
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
        }
    }
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

    #[test]
    fn test_simple_string() {
        let frame = ZSPFrame::SimpleString("OK".to_string());
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"+OK\r\n");
    }

    #[test]
    fn test_builk_string() {
        let frame = ZSPFrame::BulkString(Some(b"hello".to_vec()));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_nested_array() {
        let frame = ZSPFrame::Array(Some(vec![
            ZSPFrame::SimpleString("test".to_string()),
            ZSPFrame::Integer(42),
        ]));
        let encoded = ZSPEncoder::encode(&frame).unwrap();
        assert_eq!(encoded, b"*2\r\n+test\r\n:42\r\n");
    }

    #[test]
    fn test_invalid_simple_string() {
        let frame = ZSPFrame::SimpleString("bad\r\nstring".to_string());
        let result = ZSPEncoder::encode(&frame);
        assert!(result.is_err());
    }
}
