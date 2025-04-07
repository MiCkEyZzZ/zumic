use std::io::{self, Cursor, Result};

use bytes::Buf;

use super::types::ZSPFrame;

pub struct ZSPDecoder;

impl ZSPDecoder {
    pub fn decode(buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        if !buf.has_remaining() {
            return Ok(None);
        }

        match buf.get_u8() {
            b'+' => Self::parse_simple_string(buf),
            b'-' => Self::parse_error(buf),
            b':' => Self::parse_integer(buf),
            b'$' => Self::parse_bulk_string(buf),
            b'*' => Self::parse_array(buf),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Uknown ZSP type",
            )),
        }
    }
    fn parse_simple_string(buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = Self::read_line(buf)?;
        Ok(Some(ZSPFrame::SimpleString(line)))
    }
    fn parse_error(buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = Self::read_line(buf)?;
        Ok(Some(ZSPFrame::Error(line)))
    }
    fn parse_integer(buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = Self::read_line(buf)?;
        let number = line
            .parse::<i64>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid integer"))?;
        Ok(Some(ZSPFrame::Integer(number)))
    }
    fn parse_bulk_string(buf: &mut Cursor<&[u8]>) -> io::Result<Option<ZSPFrame>> {
        let len = Self::read_line(buf)?
            .parse::<isize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid bulk length"))?;
        if len == -1 {
            return Ok(Some(ZSPFrame::BulkString(None)));
        }
        if len < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Negative bulk length",
            ));
        }
        let len = len as usize;
        if buf.remaining() < len + 2 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Not enough data for bulk string",
            ));
        }
        let mut data = vec![0; len];
        buf.copy_to_slice(&mut data);
        Self::expect_crlf(buf)?;
        Ok(Some(ZSPFrame::BulkString(Some(data))))
    }
    fn parse_array(buf: &mut Cursor<&[u8]>) -> io::Result<Option<ZSPFrame>> {
        let len = Self::read_line(buf)?
            .parse::<isize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid array length"))?;
        if len == -1 {
            return Ok(Some(ZSPFrame::Array(None)));
        }
        let mut items = Vec::with_capacity(len as usize);
        for _ in 0..len {
            if let Some(frame) = Self::decode(buf)? {
                items.push(frame);
            }
        }
        Ok(Some(ZSPFrame::Array(Some(items))))
    }
    fn read_line(buf: &mut Cursor<&[u8]>) -> Result<String> {
        let mut line = Vec::new();
        while buf.has_remaining() {
            let b = buf.get_u8();
            if b == b'\r' {
                if buf.get_u8() == b'\n' {
                    break;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Expected \\n after \\r",
                    ));
                }
            }
            line.push(b);
        }
        Ok(String::from_utf8(line)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))?)
    }
    fn expect_crlf(buf: &mut Cursor<&[u8]>) -> Result<()> {
        if buf.remaining() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Expected CRLF",
            ));
        }
        let cr = buf.get_u8();
        let lf = buf.get_u8();
        if cr == b'\r' && lf == b'\n' {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidData, "Expected CRLF"))
        }
    }
}
