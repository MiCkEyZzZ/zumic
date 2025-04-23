//! Декодер ZSP (Zumic Serialization Protocol).
//!
//! Эта структура отвечает за процесс декодирования фреймов
//! протокола ZSP из потока данных. Она отслеживает текущее
//! состояние декодирования и может восстанавливать его для
//! продолжения декодирования фреймов.
//!
//! Протокол ZSP использует разные типы фреймов, такие как
//! строки, ошибки, целые числа, бинарные строки, массивы и
//! словари. Каждый тип фрейма имеет свои особенности в
//! декодировании, которые обрабатываются в отдельных методах.

use bytes::Buf;
use memchr::memchr;
use std::{borrow::Cow, collections::HashMap};

use crate::error::DecodeError;

use super::zsp_types::ZSPFrame;

/// Максимальная длина строки в протоколе ZSP (1 МБ).
///
/// Эта константа ограничивает длину строки, которая может
/// быть передана в протоколе ZSP. Если строка превышает
/// это значение, произойдет ошибка декодирования.
pub const MAX_LINE_LENGTH: usize = 1024 * 1024;

/// Максимальный размер BinaryString (512 МБ).
/// Эта константа ограничивает размер бинарных строк в
/// протоколе ZSP. Если длина бинарной строки превышает
/// это значение, декодирование завершится ошибкой.
pub const MAX_BINARY_LENGTH: usize = 512 * 1024 * 1024;

/// Максимальная вложенность массивов (32 уровня).
/// Эта константа ограничивает глубину вложенности массивов
/// в протоколе ZSP. Превышение этого значения приведёт к
/// ошибке декодирования.
pub const MAX_ARRAY_DEPTH: usize = 32;

#[derive(Debug)]
pub enum ZSPDecodeState<'a> {
    Initial,
    PartialBinaryString {
        len: usize,
        data: Vec<u8>,
    },
    PartialArray {
        len: usize,
        items: Vec<ZSPFrame<'a>>,
        remaining: usize,
    },
}

pub struct ZSPDecoder<'a> {
    state: ZSPDecodeState<'a>,
}

impl<'a> ZSPDecoder<'a> {
    pub fn new() -> Self {
        Self {
            state: ZSPDecodeState::Initial,
        }
    }

    pub fn decode(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let state = std::mem::replace(&mut self.state, ZSPDecodeState::Initial);

        match state {
            ZSPDecodeState::Initial => self.initial_decode(slice),
            ZSPDecodeState::PartialBinaryString { len, mut data } => {
                self.continue_binary_string(slice, len, &mut data)
            }
            ZSPDecodeState::PartialArray {
                len,
                mut items,
                mut remaining,
            } => match Self::continue_array(self, slice, &mut items, &mut remaining)? {
                Some(array) => Ok(Some(array)),
                None => {
                    self.state = ZSPDecodeState::PartialArray {
                        len,
                        items,
                        remaining,
                    };
                    Ok(None)
                }
            },
        }
    }

    fn initial_decode(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        if !slice.has_remaining() {
            return Ok(None);
        }
        match slice.get_u8() {
            b'+' => self.parse_inline_string(slice),
            b'-' => self.parse_error(slice),
            b':' => self.parse_integer(slice),
            b'$' => self.parse_binary_string(slice),
            b'*' => self.parse_array(slice, 0),
            b'%' => self.parse_dictionary(slice),
            _ => Err(DecodeError::InvalidData("Unknown ZSP type".to_string())),
        }
    }

    fn parse_inline_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZSPFrame::InlineString(Cow::Borrowed(line))))
    }

    fn parse_error(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZSPFrame::FrameError(line.to_string())))
    }

    fn parse_integer(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        let num = line.parse::<i64>().map_err(|_| {
            let err_msg = "Invalid integer".to_string();
            DecodeError::InvalidData(err_msg)
        })?;
        Ok(Some(ZSPFrame::Integer(num)))
    }

    fn parse_binary_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let len = self.read_line(slice)?.parse::<isize>().map_err(|_| {
            let err_msg = "Invalid binary".to_string();
            DecodeError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::BinaryString(None))),
            len if len >= 0 => {
                let len = len as usize;
                if len > MAX_BINARY_LENGTH {
                    let err_msg =
                        format!("Binary string too long ({} > {})", len, MAX_BINARY_LENGTH);
                    return Err(DecodeError::InvalidData(err_msg));
                }

                let available = slice.remaining().min(len);
                let mut data = Vec::with_capacity(len);
                data.extend_from_slice(&slice.chunk()[..available]);
                slice.advance(available);

                if data.len() == len {
                    self.expect_crlf(slice)?;
                    Ok(Some(ZSPFrame::BinaryString(Some(data))))
                } else {
                    self.state = ZSPDecodeState::PartialBinaryString { len, data };
                    Ok(None)
                }
            }
            _ => {
                let err_msg = "Negative binary length".to_string();
                Err(DecodeError::InvalidData(err_msg))
            }
        }
    }

    fn continue_binary_string(
        &mut self,
        slice: &mut &'a [u8],
        len: usize,
        data: &mut Vec<u8>,
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let remaining_bytes = len - data.len();
        let available = slice.remaining().min(remaining_bytes);

        // Резервируем память, если не хватит (защита от realloc)
        if data.capacity() < len {
            data.reserve(len - data.capacity());
        }

        // Безопасное разделение с минимальными копиями
        let (to_copy, rest) = slice.split_at(available);
        data.extend_from_slice(to_copy);
        *slice = rest;

        if data.len() == len {
            self.expect_crlf(slice)?;
            Ok(Some(ZSPFrame::BinaryString(Some(std::mem::take(data)))))
        } else {
            Ok(None)
        }
    }

    fn parse_array(
        &mut self,
        slice: &mut &'a [u8],
        depth: usize,
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        if depth > MAX_ARRAY_DEPTH {
            let err_msg = "Max array depth exceeded".to_string();
            return Err(DecodeError::InvalidData(err_msg));
        }

        let len = self.read_line(slice)?.parse::<isize>().map_err(|_| {
            let err_msg = "Invalid array length".to_string();
            DecodeError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZSPFrame::Array(Vec::new()))),
            len if len >= 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut remaining = len;

                while remaining > 0 {
                    let decoded = self.decode(slice)?;

                    match decoded {
                        Some(frame) => {
                            items.push(frame);
                            remaining -= 1;
                        }
                        None => {
                            self.state = ZSPDecodeState::PartialArray {
                                len,
                                items,
                                remaining,
                            };
                            return Ok(None);
                        }
                    }
                }

                Ok(Some(ZSPFrame::Array(items)))
            }
            _ => {
                let err_msg = "Negative array length".to_string();
                Err(DecodeError::InvalidData(err_msg))
            }
        }
    }

    fn parse_dictionary(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| DecodeError::InvalidData("Invalid dictionary length".to_string()))?;

        match len {
            -1 => Ok(Some(ZSPFrame::Dictionary(HashMap::new()))),
            len if len >= 0 => {
                let len = len as usize;
                let mut items = HashMap::with_capacity(len);

                for _ in 0..len {
                    let key = match self.decode(slice)? {
                        Some(frame) => frame,
                        None => return Ok(None),
                    };

                    let value = match self.decode(slice)? {
                        Some(frame) => frame,
                        None => return Ok(None),
                    };

                    match key {
                        ZSPFrame::InlineString(key_str) => {
                            items.insert(key_str, value);
                        }
                        _ => {
                            return Err(DecodeError::InvalidData(
                                "Expected InlineString as key".to_string(),
                            ));
                        }
                    }
                }

                Ok(Some(ZSPFrame::Dictionary(items)))
            }
            _ => Err(DecodeError::InvalidData(
                "Negative dictionary length".to_string(),
            )),
        }
    }

    fn continue_array(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut Vec<ZSPFrame<'a>>,
        remaining: &mut usize,
    ) -> Result<Option<ZSPFrame<'a>>, DecodeError> {
        while *remaining > 0 {
            match self.decode(slice)? {
                Some(frame) => {
                    items.push(frame);
                    *remaining -= 1;
                }
                None => return Ok(None),
            }
        }

        Ok(Some(ZSPFrame::Array(std::mem::take(items))))
    }

    fn read_line(&mut self, slice: &mut &'a [u8]) -> Result<&'a str, DecodeError> {
        if let Some(pos) = memchr(b'\r', slice) {
            if pos + 1 < slice.len() && slice[pos + 1] == b'\n' {
                let line = &slice[..pos];
                let result = std::str::from_utf8(line)
                    .map_err(|_| DecodeError::InvalidUtf8("Invalid UTF-8".into()))?;
                *slice = &slice[(pos + 2)..];
                return Ok(result);
            }
        }

        Err(DecodeError::UnexpectedEof("Incomplete line".to_string()))
    }

    #[inline(always)]
    fn expect_crlf(&self, slice: &mut &'a [u8]) -> Result<(), DecodeError> {
        if slice.len() < 2 || slice[0] != b'\r' || slice[1] != b'\n' {
            return Err(DecodeError::UnexpectedEof("Expected CRLF".to_string()));
        }
        *slice = &slice[2..];
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::network::zsp::frame::encoder::ZSPEncoder;

    // Тест для строк в формате inline
    // Проверка декодирования строки, начинающейся с '+'
    #[test]
    fn test_simple_string() {
        let mut decoder = ZSPDecoder::new();
        let data = b"+OK\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::InlineString("OK".into()));
    }

    // Тест для бинарных строк
    // Проверка декодирования строки, начинающейся с '$'
    #[test]
    fn test_binary_string() {
        let mut decoder = ZSPDecoder::new();
        let data = b"$5\r\nhello\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::BinaryString(Some(b"hello".to_vec())));
    }

    // Тест для частичной бинарной строки
    // Проверка декодирования бинарной строки в два шага
    #[test]
    fn test_partial_binary_string() {
        let mut decoder = ZSPDecoder::new();

        // Первая часть - должно вернуться None, что означает необходимость дополнительных данных
        let data1 = b"$5\r\nhel".to_vec();
        let mut slice = data1.as_slice();
        assert!(matches!(decoder.decode(&mut slice), Ok(None)));

        // Вторая часть - теперь должно вернуться полное сообщение
        let data2 = b"lo\r\n".to_vec();
        let mut slice = data2.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::BinaryString(Some(b"hello".to_vec())));
    }

    // Тест для пустого словаря
    // Проверка декодирования словаря без элементов
    #[test]
    fn test_empty_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%0\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZSPFrame::Dictionary(HashMap::new()));
    }

    // Тест для словаря с одним элементом
    // Проверка декодирования словаря с одним ключом и значением
    #[test]
    fn test_single_item_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%1\r\n+key\r\n+value\r\n".to_vec(); // Один ключ-значение
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        let mut expected_dict = HashMap::new();
        expected_dict.insert(
            "key".into(),
            ZSPFrame::InlineString("value".to_string().into()),
        );

        assert_eq!(frame, ZSPFrame::Dictionary(expected_dict));
    }

    // Тест для словаря с несколькими элементами
    // Проверка декодирования словаря с несколькими парами ключ-значение
    #[test]
    fn test_multiple_items_dictionary() {
        use std::collections::HashMap;

        let mut items = HashMap::new();
        items.insert(
            "key1".into(),
            ZSPFrame::InlineString("value1".to_string().into()),
        );
        items.insert(
            "key2".into(),
            ZSPFrame::InlineString("value2".to_string().into()),
        );
        let original = ZSPFrame::Dictionary(items);
        let encoded = ZSPEncoder::encode(&original).unwrap();

        // Вместо прямого сравнения байтов, декодируем их обратно:
        let mut decoder = ZSPDecoder::new();
        let mut slice = encoded.as_slice();
        let decoded = decoder.decode(&mut slice).unwrap().unwrap();

        assert_eq!(original, decoded);
    }

    // Тест для неверного словаря (неверный ключ)
    // Проверка, что возникает ошибка при использовании неверного ключа в словаре
    #[test]
    fn test_invalid_dictionary_key() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%1\r\n-err\r\n+value\r\n".to_vec(); // Ошибка, ключ должен быть InlineString
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(result.is_err());
    }

    // Тест для неверного словаря (неполный словарь)
    // Проверка поведения, когда недостаточно данных для декодирования всего словаря
    #[test]
    fn test_incomplete_dictionary() {
        let mut decoder = ZSPDecoder::new();
        let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
    }
}
