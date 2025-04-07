use bytes::Buf;
use std::io::{self, Cursor, Result};

use super::types::ZSPFrame;

// --- Константы для безопасности ---
pub const MAX_LINE_LENGTH: usize = 1024 * 1024; // 1 MB
pub const MAX_BULK_LENGTH: usize = 512 * 1024 * 1024; // 512 MB
pub const MAX_ARRAY_DEPTH: usize = 32; // Максимальная вложенность массивов

#[derive(Debug)]
pub enum ZSPDecodeState {
    Initial,
    PartialBulkString {
        len: usize,
        data: Vec<u8>,
    },
    PartialArray {
        len: usize,
        items: Vec<ZSPFrame>,
        remaining: usize,
    },
}

pub struct ZSPDecoder {
    state: ZSPDecodeState,
}

impl ZSPDecoder {
    pub fn new() -> Self {
        Self {
            state: ZSPDecodeState::Initial,
        }
    }

    pub fn decode(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        // Take ownership of the current state, replacing it with Initial
        let state = std::mem::replace(&mut self.state, ZSPDecodeState::Initial);

        match state {
            ZSPDecodeState::Initial => {
                if !buf.has_remaining() {
                    return Ok(None);
                }
                match buf.get_u8() {
                    b'+' => self.parse_simple_string(buf),
                    b'-' => self.parse_error(buf),
                    b':' => self.parse_integer(buf),
                    b'$' => self.parse_bulk_string(buf),
                    b'*' => self.parse_array(buf, 0),
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown ZSP type at byte {}", buf.position() - 1),
                    )),
                }
            }
            ZSPDecodeState::PartialBulkString { len, mut data } => {
                let result = self.continue_bulk_string(buf, len, &mut data);
                // Only update state if we're still partial
                if let Ok(None) = result {
                    self.state = ZSPDecodeState::PartialBulkString { len, data };
                }
                result
            }
            ZSPDecodeState::PartialArray {
                len,
                mut items,
                remaining,
            } => {
                let result = self.continue_array(buf, len, &mut items, remaining);
                // Only update state if we're still partial
                if let Ok(None) = result {
                    self.state = ZSPDecodeState::PartialArray {
                        len,
                        items,
                        remaining,
                    };
                }
                result
            }
        }
    }

    // --- Методы для парсинга фреймов ---
    fn parse_simple_string(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = self.read_line(buf)?;
        Ok(Some(ZSPFrame::SimpleString(line)))
    }

    fn parse_error(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = self.read_line(buf)?;
        Ok(Some(ZSPFrame::Error(line)))
    }

    fn parse_integer(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = self.read_line(buf)?;
        let num = line.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid integer at byte {}", buf.position()),
            )
        })?;
        Ok(Some(ZSPFrame::Integer(num)))
    }

    fn parse_bulk_string(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid bulk length at byte {}", buf.position()),
            )
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::BulkString(None))), // Null bulk string
            len if len >= 0 => {
                let len = len as usize;
                if len > MAX_BULK_LENGTH {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Bulk string too long ({} > {})", len, MAX_BULK_LENGTH),
                    ));
                }

                // Read available bytes immediately
                let available = buf.remaining().min(len);
                let mut data = Vec::with_capacity(len);
                data.extend_from_slice(&buf.chunk()[..available]);
                buf.advance(available);

                if data.len() == len {
                    // Full data read, check CRLF
                    self.expect_crlf(buf)?;
                    Ok(Some(ZSPFrame::BulkString(Some(data))))
                } else {
                    // Save partial data in state
                    self.state = ZSPDecodeState::PartialBulkString { len, data };
                    Ok(None)
                }
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Negative bulk length at byte {}", buf.position()),
            )),
        }
    }

    fn continue_bulk_string(
        &mut self,
        buf: &mut Cursor<&[u8]>,
        len: usize,
        data: &mut Vec<u8>,
    ) -> Result<Option<ZSPFrame>> {
        let remaining_bytes = len - data.len();
        let available = buf.remaining().min(remaining_bytes);
        data.extend_from_slice(&buf.chunk()[..available]);
        buf.advance(available);

        if data.len() == len {
            self.expect_crlf(buf)?;
            Ok(Some(ZSPFrame::BulkString(Some(std::mem::take(data)))))
        } else {
            Ok(None)
        }
    }

    fn parse_array(&mut self, buf: &mut Cursor<&[u8]>, depth: usize) -> Result<Option<ZSPFrame>> {
        if depth > MAX_ARRAY_DEPTH {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Max array depth exceeded at byte {}", buf.position()),
            ));
        }

        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid array length at byte {}", buf.position()),
            )
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::Array(None))), // Null array
            len if len >= 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut remaining = len;

                while remaining > 0 {
                    if let Some(frame) = self.decode(buf)? {
                        items.push(frame);
                        remaining -= 1;
                    } else {
                        self.state = ZSPDecodeState::PartialArray {
                            len,
                            items,
                            remaining,
                        };
                        return Ok(None);
                    }
                }

                Ok(Some(ZSPFrame::Array(Some(items))))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Negative array length at byte {}", buf.position()),
            )),
        }
    }

    fn continue_array(
        &mut self,
        buf: &mut Cursor<&[u8]>,
        len: usize,
        items: &mut Vec<ZSPFrame>,
        remaining: usize,
    ) -> Result<Option<ZSPFrame>> {
        let mut remaining = remaining;
        while remaining > 0 {
            if let Some(frame) = self.decode(buf)? {
                items.push(frame);
                remaining -= 1;
            } else {
                self.state = ZSPDecodeState::PartialArray {
                    len,
                    items: std::mem::take(items), // Take ownership instead of cloning
                    remaining,
                };
                return Ok(None);
            }
        }

        self.state = ZSPDecodeState::Initial;
        Ok(Some(ZSPFrame::Array(Some(std::mem::take(items)))))
    }

    // --- Вспомогательные методы ---
    fn read_line(&mut self, buf: &mut Cursor<&[u8]>) -> Result<String> {
        let start_pos = buf.position();
        let mut line = Vec::new();

        while buf.has_remaining() && line.len() < MAX_LINE_LENGTH {
            let b = buf.get_u8();
            if b == b'\r' {
                if buf.get_u8() == b'\n' {
                    return String::from_utf8(line).map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Invalid UTF-8 at byte {}", start_pos),
                        )
                    });
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Expected \\n after \\r at byte {}", buf.position() - 1),
                    ));
                }
            }
            line.push(b);
        }

        if line.len() >= MAX_LINE_LENGTH {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Line too long (max {} bytes)", MAX_LINE_LENGTH),
            ))
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("Incomplete line at byte {}", start_pos),
            ))
        }
    }

    fn expect_crlf(&mut self, buf: &mut Cursor<&[u8]>) -> Result<()> {
        if buf.remaining() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("Expected CRLF at byte {}", buf.position()),
            ));
        }
        if buf.get_u8() != b'\r' || buf.get_u8() != b'\n' {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Expected CRLF at byte {}", buf.position() - 2),
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
        let mut decoder = ZSPDecoder::new();
        let data = b"+OK\r\n".to_vec();
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::SimpleString("OK".to_string()));
    }

    #[test]
    fn test_bulk_string() {
        let mut decoder = ZSPDecoder::new();
        let data = b"$5\r\nhello\r\n".to_vec();
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(b"hello".to_vec())));
    }

    #[test]
    fn test_partial_bulk_string() {
        let mut decoder = ZSPDecoder::new();

        // First part - should return None indicating more data needed
        let data1 = b"$5\r\nhel".to_vec();
        let mut cursor = Cursor::new(data1.as_slice());
        assert!(matches!(decoder.decode(&mut cursor), Ok(None)));

        // Second part - should now return the complete frame
        let data2 = b"lo\r\n".to_vec();
        let mut cursor = Cursor::new(data2.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(b"hello".to_vec())));
    }
}
