use std::{collections::HashMap, io::Cursor};

use bytes::Buf;
use tracing::{error, info};

use super::{errors::ZSPError, zsp_types::ZSPFrame};

/// Maximum string length (1mb).
pub const MAX_LINE_LENGTH: usize = 1024 * 1024;
/// Maximum BulkString size (512mb).
pub const MAX_BULK_LENGTH: usize = 512 * 1024 * 1024;
/// Maximum nesting of arrays (32 levels).
pub const MAX_ARRAY_DEPTH: usize = 32;

#[derive(Debug)]
pub enum ZSPDecodeState {
    /// Начальное состояние.
    Initial,
    /// Состояние, когда BulkString прочитан не полностью.
    PartialBulkString { len: usize, data: Vec<u8> },
    /// Состояние, когда Array читается не полностью.
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

    pub fn decode(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>, ZSPError> {
        let state = std::mem::replace(&mut self.state, ZSPDecodeState::Initial);

        match state {
            ZSPDecodeState::Initial => {
                if !buf.has_remaining() {
                    info!("No data left to decode.");
                    return Ok(None);
                }
                // В зависимости от первого байта вызываем соответствующий метод парсинга.
                match buf.get_u8() {
                    b'+' => self.parse_simple_string(buf),
                    b'-' => self.parse_error(buf),
                    b':' => self.parse_integer(buf),
                    b'$' => self.parse_bulk_string(buf),
                    b'*' => self.parse_array(buf, 0),
                    b'%' => self.parse_dictionary(buf),
                    _ => {
                        let err_msg = format!("Unknown ZSP type at byte {}", buf.position() - 1);
                        error!("{}", err_msg);
                        Err(ZSPError::InvalidData(err_msg))
                    }
                }
            }
            ZSPDecodeState::PartialBulkString { len, mut data } => {
                let result = self.continue_bulk_string(buf, len, &mut data);
                // Если данные всё ещё неполные, сохраняем состояние.
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

    // --- Методы для парсинга отдельных типов фреймов ---

    /// Парсит SimpleString, читаемый до CRLF.
    fn parse_simple_string(
        &mut self,
        buf: &mut Cursor<&[u8]>,
    ) -> Result<Option<ZSPFrame>, ZSPError> {
        let line = self.read_line(buf)?;
        info!("Parsed simple string: {}", line);
        Ok(Some(ZSPFrame::SimpleString(line)))
    }

    /// Парсит Error-фрейм.
    fn parse_error(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>, ZSPError> {
        let line = self.read_line(buf)?;
        info!("Parsed error: {}", line);
        Ok(Some(ZSPFrame::FrameError(line)))
    }

    /// Парсит Integer-фрейм.
    fn parse_integer(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>, ZSPError> {
        let line = self.read_line(buf)?;
        let num = line.parse::<i64>().map_err(|_| {
            let err_msg = format!("Invalid integer at byte {}", buf.position());
            error!("{}", err_msg);
            ZSPError::InvalidData(err_msg)
        })?;
        info!("Parsed integer: {}", num);
        Ok(Some(ZSPFrame::Integer(num)))
    }

    fn parse_bulk_string(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>, ZSPError> {
        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            let err_msg = format!("Invalid bulk length at byte {}", buf.position());
            error!("{}", err_msg);
            ZSPError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::BulkString(None))), // Null bulk string
            len if len >= 0 => {
                let len = len as usize;
                if len > MAX_BULK_LENGTH {
                    let err_msg = format!("Bulk string too long ({} > {})", len, MAX_BULK_LENGTH);
                    error!("{}", err_msg);
                    return Err(ZSPError::InvalidData(err_msg));
                }

                // Читаем доступное количество байт
                let available = buf.remaining().min(len);
                let mut data = Vec::with_capacity(len);
                data.extend_from_slice(&buf.chunk()[..available]);
                buf.advance(available);

                if data.len() == len {
                    // Если данные полные, проверяем завершающий CRLF
                    self.expect_crlf(buf)?;
                    info!("Parsed bulk string of length {}", len);
                    Ok(Some(ZSPFrame::BulkString(Some(data))))
                } else {
                    // Если данных недостаточно, сохраняем состояние
                    self.state = ZSPDecodeState::PartialBulkString { len, data };
                    Ok(None)
                }
            }
            _ => {
                let err_msg = format!("Negative bulk length at byte {}", buf.position());
                error!("{}", err_msg);
                Err(ZSPError::InvalidData(err_msg))
            }
        }
    }

    /// Продолжает чтение BulkString, если данные были неполными.
    fn continue_bulk_string(
        &mut self,
        buf: &mut Cursor<&[u8]>,
        len: usize,
        data: &mut Vec<u8>,
    ) -> Result<Option<ZSPFrame>, ZSPError> {
        let remaining_bytes = len - data.len();
        let available = buf.remaining().min(remaining_bytes);
        data.extend_from_slice(&buf.chunk()[..available]);
        buf.advance(available);

        if data.len() == len {
            self.expect_crlf(buf)?;
            info!("Completed parsing bulk string.");
            Ok(Some(ZSPFrame::BulkString(Some(std::mem::take(data)))))
        } else {
            Ok(None)
        }
    }

    fn parse_array(
        &mut self,
        buf: &mut Cursor<&[u8]>,
        depth: usize,
    ) -> Result<Option<ZSPFrame>, ZSPError> {
        if depth > MAX_ARRAY_DEPTH {
            let err_msg = format!("Max array depth exceeded at byte {}", buf.position());
            error!("{}", err_msg);
            return Err(ZSPError::InvalidData(err_msg));
        }

        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            let err_msg = format!("Invalid array length at byte {}", buf.position());
            error!("{}", err_msg);
            ZSPError::InvalidData(err_msg)
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

                info!("Parsed array with {} elements.", items.len());
                Ok(Some(ZSPFrame::Array(Some(items))))
            }
            _ => {
                let err_msg = format!("Negative array length at byte {}", buf.position());
                error!("{}", err_msg);
                Err(ZSPError::InvalidData(err_msg))
            }
        }
    }

    fn parse_dictionary(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>, ZSPError> {
        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            let err_msg = format!("Invalid dictionary length at byte {}", buf.position());
            error!("{}", err_msg);
            ZSPError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::Dictionary(None))), // Null dictionary
            len if len >= 0 => {
                let len = len as usize;
                let mut items = HashMap::new();

                for _ in 0..len {
                    // Читаем ключ
                    let key_opt = self.decode(buf)?;
                    if key_opt.is_none() {
                        return Ok(None); // Возвращаем Ok(None), если нет данных для ключа
                    }
                    let key = key_opt.unwrap();

                    // Читаем значение
                    let value_opt = self.decode(buf)?;
                    if value_opt.is_none() {
                        return Ok(None); // Возвращаем Ok(None), если нет данных для значения
                    }
                    let value = value_opt.unwrap();

                    // Ключ должен быть SimpleString
                    if let ZSPFrame::SimpleString(key_str) = key {
                        items.insert(key_str, value);
                    } else {
                        let err_msg =
                            format!("Expected SimpleString as key at byte {}", buf.position());
                        error!("{}", err_msg);
                        return Err(ZSPError::InvalidData(err_msg));
                    }
                }

                info!("Parsed dictionary with {} items.", items.len());
                Ok(Some(ZSPFrame::Dictionary(Some(items))))
            }
            _ => {
                let err_msg = format!("Negative dictionary length at byte {}", buf.position());
                error!("{}", err_msg);
                Err(ZSPError::InvalidData(err_msg))
            }
        }
    }

    /// Продолжает чтение Array-фрейма, если данные были неполными.
    fn continue_array(
        &mut self,
        buf: &mut Cursor<&[u8]>,
        len: usize,
        items: &mut Vec<ZSPFrame>,
        remaining: usize,
    ) -> Result<Option<ZSPFrame>, ZSPError> {
        let mut remaining = remaining;
        while remaining > 0 {
            if let Some(frame) = self.decode(buf)? {
                items.push(frame);
                remaining -= 1;
            } else {
                self.state = ZSPDecodeState::PartialArray {
                    len,
                    items: std::mem::take(items),
                    remaining,
                };
                return Ok(None);
            }
        }

        self.state = ZSPDecodeState::Initial;
        Ok(Some(ZSPFrame::Array(Some(std::mem::take(items)))))
    }

    // --- Вспомогательные методы ---

    fn read_line(&mut self, buf: &mut Cursor<&[u8]>) -> Result<String, ZSPError> {
        let start_pos = buf.position();
        let mut line = Vec::new();

        while buf.has_remaining() && line.len() < MAX_LINE_LENGTH {
            let b = buf.get_u8();
            if b == b'\r' {
                if buf.get_u8() == b'\n' {
                    return String::from_utf8(line).map_err(|_| {
                        let err_msg = format!("Invalid UTF-8 sequence at byte {}", start_pos);
                        error!("{}", err_msg);
                        ZSPError::InvalidData(err_msg)
                    });
                } else {
                    let err_msg = format!("Expected \\n after \\r at byte {}", buf.position());
                    error!("{}", err_msg);
                    return Err(ZSPError::InvalidData(err_msg));
                }
            }
            line.push(b);
        }

        if line.len() >= MAX_LINE_LENGTH {
            Err(ZSPError::InvalidData(format!(
                "Line to long (max {} bytes)",
                MAX_LINE_LENGTH
            )))
        } else {
            Err(ZSPError::UnexpectedEof(format!(
                "Incomplete line at byte {}",
                start_pos
            )))
        }
    }

    /// Проверяет, что следующие два байта представляют собой CRLF.
    fn expect_crlf(&mut self, buf: &mut Cursor<&[u8]>) -> Result<(), ZSPError> {
        if buf.remaining() < 2 {
            let err_msg = format!("Expected CRLF at byte {}", buf.position());
            error!("{}", err_msg);
            return Err(ZSPError::UnexpectedEof(err_msg));
        }
        if buf.get_u8() != b'\r' || buf.get_u8() != b'\n' {
            let err_msg = format!("Invalid CRLF sequence at byte {}", buf.position());
            error!("{}", err_msg);
            return Err(ZSPError::UnexpectedEof(err_msg));
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::network::zsp::frame::encoder::ZSPEncoder;

    // Тест для простых строк
    // Проверяет декодирование строки, начинающейся с '+'
    #[test]
    fn test_simple_string() {
        let mut decoder = ZSPDecoder::new();
        let data = b"+OK\r\n".to_vec();
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::SimpleString("OK".to_string()));
    }

    // Тест для булк-строк
    // Проверяет декодирование строки, начинающейся с '$'
    #[test]
    fn test_bulk_string() {
        let mut decoder = ZSPDecoder::new();
        let data = b"$5\r\nhello\r\n".to_vec();
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::BulkString(Some(b"hello".to_vec())));
    }

    // Тест для частичной булк-строки
    // Проверяет декодирование булк-строки в два этапа
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

    // Тест для пустого словаря
    // Проверяет декодирование словаря без элементов
    #[test]
    fn test_empty_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%0\r\n".to_vec();
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::Dictionary(Some(HashMap::new())));
    }

    // Тест для словаря с одним элементом
    // Проверяет декодирование словаря с одним ключом и значением
    #[test]
    fn test_single_item_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%1\r\n+key\r\n+value\r\n".to_vec(); // Один ключ-значение
        let mut cursor = Cursor::new(data.as_slice());
        let frame = decoder.decode(&mut cursor).unwrap().unwrap();

        let mut expected_dict = HashMap::new();
        expected_dict.insert(
            "key".to_string(),
            ZSPFrame::SimpleString("value".to_string()),
        );

        assert_eq!(frame, ZSPFrame::Dictionary(Some(expected_dict)));
    }

    // Тест для словаря с несколькими элементами
    // Проверяет декодирование словаря с несколькими парами ключ-значение
    #[test]
    fn test_multiple_items_dictionary() {
        use std::collections::HashMap;

        let mut items = HashMap::new();
        items.insert(
            "key1".to_string(),
            ZSPFrame::SimpleString("value1".to_string()),
        );
        items.insert(
            "key2".to_string(),
            ZSPFrame::SimpleString("value2".to_string()),
        );
        let original = ZSPFrame::Dictionary(Some(items));
        let encoded = ZSPEncoder::encode(&original).unwrap();

        // Вместо прямого сравнения байтов, декодируем обратно:
        let mut decoder = ZSPDecoder::new();
        let mut cursor = std::io::Cursor::new(encoded.as_slice());
        let decoded = decoder.decode(&mut cursor).unwrap().unwrap();

        assert_eq!(original, decoded);
    }

    // Тест для некорректного словаря (некорректный ключ)
    // Проверяет, что происходит ошибка при попытке использовать некорректный ключ в словаре
    #[test]
    fn test_invalid_dictionary_key() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%1\r\n-err\r\n+value\r\n".to_vec(); // Ошибка, ключ должен быть SimpleString
        let mut cursor = Cursor::new(data.as_slice());
        let result = decoder.decode(&mut cursor);
        assert!(result.is_err());
    }

    // Тест для некорректного словаря (неполный словарь)
    // Проверяет поведение при недостаточности данных для декодирования всего словаря
    #[test]
    fn test_incomplete_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
        let mut cursor = Cursor::new(data.as_slice());
        let result = decoder.decode(&mut cursor);
        assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
    }
}
