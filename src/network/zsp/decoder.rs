//! # Модуль ZSPDecoder
//! Этот модуль реализует декодер протокола **ZSP (Zumic Serialization Protocol)**.
//! Протокол ZSP поддерживает следующие типы данных:
//! - **SimpleString** – строка, начинающаяся с `+`
//! - **Error** – сообщение об ошибке, начинающееся с `-`
//! - **Integer** – целое число, начинающееся с `:`
//! - **BulkString** – бинарные данные, начинающиеся с `$`
//! - **Array** – массив элементов, начинающийся с `*`
//! - **Dictionary** – словарь (Map), начинающийся с `%`
//!
//! Декодер поддерживает частичное чтение данных (streaming). Если данных недостаточно для полного фрейма, метод `decode` возвращает `Ok(None)`,
//! сигнализируя о необходимости получения дополнительных данных.
//!
//! Для обеспечения безопасности реализованы ограничения на максимальную длину строки (`MAX_LINE_LENGTH`),
//! максимальный размер BulkString (`MAX_BULK_LENGTH`) и максимальную вложенность массивов (`MAX_ARRAY_DEPTH`).

use bytes::Buf;
use std::{
    collections::HashMap,
    io::{self, Cursor, Result},
};

use super::types::ZSPFrame;

/// Максимальная длина строки (1 МБ).
pub const MAX_LINE_LENGTH: usize = 1024 * 1024;
/// Максимальный размер BulkString (512 МБ).
pub const MAX_BULK_LENGTH: usize = 512 * 1024 * 1024;
/// Максимальная вложенность массивов (32 уровня).
pub const MAX_ARRAY_DEPTH: usize = 32;

/// Состояние декодера. Используется для сохранения промежуточного состояния при частичном чтении.
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

/// Основной декодер протокола ZSP.
pub struct ZSPDecoder {
    /// Текущее состояние декодера.
    state: ZSPDecodeState,
}

impl ZSPDecoder {
    /// Создаёт новый экземпляр декодера.
    pub fn new() -> Self {
        Self {
            state: ZSPDecodeState::Initial,
        }
    }

    /// Основной метод декодирования.
    ///
    /// Принимает ссылку на `Cursor` с данными и пытается декодировать один полный фрейм ZSP.
    /// Если данных недостаточно для завершения декодирования, возвращается `Ok(None)`.
    pub fn decode(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        // Извлекаем текущее состояние и сбрасываем его на начальное.
        let state = std::mem::replace(&mut self.state, ZSPDecodeState::Initial);

        match state {
            ZSPDecodeState::Initial => {
                if !buf.has_remaining() {
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
                    _ => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unknown ZSP type at byte {}", buf.position() - 1),
                    )),
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
    fn parse_simple_string(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = self.read_line(buf)?;
        Ok(Some(ZSPFrame::SimpleString(line)))
    }

    /// Парсит Error-фрейм.
    fn parse_error(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let line = self.read_line(buf)?;
        Ok(Some(ZSPFrame::FrameError(line)))
    }

    /// Парсит Integer-фрейм.
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

    /// Парсит BulkString.
    ///
    /// Читает длину строки, затем данные и завершающий CRLF.
    /// Если данных недостаточно, сохраняет состояние и возвращает Ok(None).
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

                // Читаем доступное количество байт
                let available = buf.remaining().min(len);
                let mut data = Vec::with_capacity(len);
                data.extend_from_slice(&buf.chunk()[..available]);
                buf.advance(available);

                if data.len() == len {
                    // Если данные полные, проверяем завершающий CRLF
                    self.expect_crlf(buf)?;
                    Ok(Some(ZSPFrame::BulkString(Some(data))))
                } else {
                    // Если данных недостаточно, сохраняем состояние
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

    /// Продолжает чтение BulkString, если данные были неполными.
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

    /// Парсит Array-фрейм.
    ///
    /// Читает количество элементов, затем последовательно декодирует каждый элемент.
    /// Поддерживается рекурсия до MAX_ARRAY_DEPTH.
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

    /// Парсит Dictionary-фрейм.
    ///
    /// Читает количество пар (ключ-значение). Для каждой пары рекурсивно вызывает `decode` для ключа и значения.
    /// Если данные для ключа или значения неполные, возвращается `Ok(None)`.
    fn parse_dictionary(&mut self, buf: &mut Cursor<&[u8]>) -> Result<Option<ZSPFrame>> {
        let len = self.read_line(buf)?.parse::<isize>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid dictionary length at byte {}", buf.position()),
            )
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
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Expected SimpleString as key at byte {}", buf.position()),
                        ));
                    }
                }

                Ok(Some(ZSPFrame::Dictionary(Some(items))))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Negative dictionary length at byte {}", buf.position()),
            )),
        }
    }

    /// Продолжает чтение Array-фрейма, если данные были неполными.
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

    /// Читает строку до последовательности CRLF.
    ///
    /// Если строка не заканчивается CRLF или неполная, возвращает ошибку или Ok(None).
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

    /// Проверяет, что следующие два байта представляют собой CRLF.
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
    use crate::network::zsp::encoder::ZSPEncoder;

    use super::*;

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
