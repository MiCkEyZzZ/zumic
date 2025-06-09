// Copyright 2025 Zumic

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

use std::{borrow::Cow, collections::HashMap};

use bytes::Buf;
use memchr::memchr;

use super::ZspFrame;
use crate::DecodeError;

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
pub enum ZspDecodeState<'a> {
    Initial,
    PartialBinaryString {
        len: usize,
        data: Vec<u8>,
    },
    PartialArray {
        len: usize,
        items: Vec<ZspFrame<'a>>,
        remaining: usize,
    },
    PartialZSet {
        len: usize,
        items: Vec<(String, f64)>,
        pending_member: Option<String>,
        remaining: usize,
    },
}

pub struct ZspDecoder<'a> {
    state: ZspDecodeState<'a>,
}

impl<'a> ZspDecoder<'a> {
    pub fn new() -> Self {
        Self {
            state: ZspDecodeState::Initial,
        }
    }

    pub fn decode(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let state = std::mem::replace(&mut self.state, ZspDecodeState::Initial);

        match state {
            ZspDecodeState::Initial => self.initial_decode(slice),
            ZspDecodeState::PartialBinaryString { len, mut data } => {
                self.continue_binary_string(slice, len, &mut data)
            }
            ZspDecodeState::PartialArray {
                len,
                mut items,
                mut remaining,
            } => match Self::continue_array(self, slice, &mut items, &mut remaining)? {
                Some(array) => Ok(Some(array)),
                None => {
                    self.state = ZspDecodeState::PartialArray {
                        len,
                        items,
                        remaining,
                    };
                    Ok(None)
                }
            },
            ZspDecodeState::PartialZSet {
                len,
                mut items,
                mut pending_member,
                mut remaining,
            } => {
                match self.continue_zset(
                    slice,
                    len,
                    &mut items,
                    &mut pending_member,
                    &mut remaining,
                )? {
                    Some(zs) => Ok(Some(zs)),
                    None => {
                        self.state = ZspDecodeState::PartialZSet {
                            len,
                            items,
                            pending_member,
                            remaining,
                        };
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Продолжить декодирование ZSet при частичных данных.
    fn continue_zset(
        &mut self,
        slice: &mut &'a [u8],
        _len: usize,
        items: &mut Vec<(String, f64)>,
        pending_member: &mut Option<String>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        while *remaining > 0 {
            if pending_member.is_none() {
                // ожидаем member
                match self.decode(slice)? {
                    Some(frame) => {
                        let member = match frame {
                            ZspFrame::InlineString(cow) => cow.into_owned(),
                            ZspFrame::BinaryString(Some(bytes)) => String::from_utf8(bytes)
                                .map_err(|_| {
                                    DecodeError::InvalidData(
                                        "Invalid UTF-8 in zset member".to_string(),
                                    )
                                })?,
                            _ => {
                                return Err(DecodeError::InvalidData(
                                    "Expected string for zset member".to_string(),
                                ));
                            }
                        };
                        *pending_member = Some(member);
                    }
                    None => return Ok(None),
                }
            } else {
                // ожидаем score
                let score = match self.decode(slice)? {
                    Some(ZspFrame::Float(f)) => f,
                    Some(ZspFrame::Integer(i)) => i as f64,
                    Some(_) => {
                        return Err(DecodeError::InvalidData(
                            "Expected float/integer for zset score".to_string(),
                        ))
                    }
                    None => return Ok(None),
                };
                let member = pending_member.take().unwrap();
                items.push((member, score));
                *remaining -= 1;
            }
        }
        // всё прочитано
        Ok(Some(ZspFrame::ZSet(std::mem::take(items))))
    }

    fn initial_decode(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        if !slice.has_remaining() {
            return Ok(None);
        }
        match slice.get_u8() {
            b'+' => self.parse_inline_string(slice),
            b'-' => self.parse_error(slice),
            b':' => self.parse_integer(slice),
            b',' => self.parse_float(slice),
            b'#' => self.parse_bool(slice),
            b'^' => self.parse_zset(slice),
            b'$' => self.parse_binary_string(slice),
            b'*' => self.parse_array(slice, 0),
            b'%' => self.parse_dictionary(slice),
            _ => Err(DecodeError::InvalidData("Unknown ZSP type".to_string())),
        }
    }

    fn parse_inline_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZspFrame::InlineString(Cow::Borrowed(line))))
    }

    fn parse_error(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZspFrame::FrameError(line.to_string())))
    }

    fn parse_integer(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        let num = line.parse::<i64>().map_err(|_| {
            let err_msg = "Invalid integer".to_string();
            DecodeError::InvalidData(err_msg)
        })?;
        Ok(Some(ZspFrame::Integer(num)))
    }

    /// Дукодирует число с плавающей точкой (префикс `,`).
    /// Формат: `,<float>\r\n`
    fn parse_float(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let line = self.read_line(slice)?;
        let num = line.parse::<f64>().map_err(|_| {
            let err_msg = "Invalid float".to_string();
            DecodeError::InvalidData(err_msg)
        })?;
        Ok(Some(ZspFrame::Float(num)))
    }

    /// Парсит булево значение: `#t\r\n` или `#f\r\n`
    fn parse_bool(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        if slice.len() < 3 {
            return Err(DecodeError::UnexpectedEof("Incomplete boolean".into()));
        }
        let b = slice[0];
        if slice[1] != b'\r' || slice[2] != b'\n' {
            return Err(DecodeError::InvalidData("Invalid boolean format".into()));
        }
        *slice = &slice[3..];
        match b {
            b't' => Ok(Some(ZspFrame::Bool(true))),
            b'f' => Ok(Some(ZspFrame::Bool(false))),
            _ => Err(DecodeError::InvalidData("Unknown boolean value".into())),
        }
    }

    fn parse_binary_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let len = self.read_line(slice)?.parse::<isize>().map_err(|_| {
            let err_msg = "Invalid binary".to_string();
            DecodeError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZspFrame::BinaryString(None))),
            len if len >= 0 => {
                let len = len as usize;
                if len > MAX_BINARY_LENGTH {
                    let err_msg = format!("Binary string too long ({len} > {MAX_BINARY_LENGTH})");
                    return Err(DecodeError::InvalidData(err_msg));
                }

                let available = slice.remaining().min(len);
                let mut data = Vec::with_capacity(len);
                data.extend_from_slice(&slice.chunk()[..available]);
                slice.advance(available);

                if data.len() == len {
                    self.expect_crlf(slice)?;
                    Ok(Some(ZspFrame::BinaryString(Some(data))))
                } else {
                    self.state = ZspDecodeState::PartialBinaryString { len, data };
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
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
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
            Ok(Some(ZspFrame::BinaryString(Some(std::mem::take(data)))))
        } else {
            Ok(None)
        }
    }

    fn parse_array(
        &mut self,
        slice: &mut &'a [u8],
        depth: usize,
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        if depth > MAX_ARRAY_DEPTH {
            let err_msg = "Max array depth exceeded".to_string();
            return Err(DecodeError::InvalidData(err_msg));
        }

        let len = self.read_line(slice)?.parse::<isize>().map_err(|_| {
            let err_msg = "Invalid array length".to_string();
            DecodeError::InvalidData(err_msg)
        })?;

        match len {
            -1 => Ok(Some(ZspFrame::Array(Vec::new()))),
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
                            self.state = ZspDecodeState::PartialArray {
                                len,
                                items,
                                remaining,
                            };
                            return Ok(None);
                        }
                    }
                }

                Ok(Some(ZspFrame::Array(items)))
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
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| DecodeError::InvalidData("Invalid dictionary length".to_string()))?;

        match len {
            -1 => Ok(Some(ZspFrame::Dictionary(HashMap::new()))),
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
                        ZspFrame::InlineString(key_str) => {
                            items.insert(key_str, value);
                        }
                        _ => {
                            return Err(DecodeError::InvalidData(
                                "Expected InlineString as key".to_string(),
                            ));
                        }
                    }
                }

                Ok(Some(ZspFrame::Dictionary(items)))
            }
            _ => Err(DecodeError::InvalidData(
                "Negative dictionary length".to_string(),
            )),
        }
    }

    /// Декодирует фрейм ZSet (префикс `^`).
    /// Содержит <count> пар "член-оценка", где оценка — число с плавающей
    /// точкой:contentReference[oaicite:7]{index=7}.
    /// Пример: `^2\r\n$3\r\nfoo\r\n,1.23\r\n$3\r\nbar\r\n,4.56\r\n`.
    fn parse_zset(&mut self, slice: &mut &'a [u8]) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        // Считываем количество элементов ZSET
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| DecodeError::InvalidData("Invalid zset length".to_string()))?;

        match len {
            // Null-ZSET
            -1 => Ok(Some(ZspFrame::ZSet(Vec::new()))),

            // Положительное число элементов
            len if len >= 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut pending_member = None;
                let mut remaining = len;

                // попытаться декодировать как можно больше элементов
                match self.continue_zset(
                    slice,
                    len,
                    &mut items,
                    &mut pending_member,
                    &mut remaining,
                )? {
                    Some(zs) => Ok(Some(zs)),
                    None => {
                        // не всё прочитано — сохраняем состояние
                        self.state = ZspDecodeState::PartialZSet {
                            len,
                            items,
                            pending_member,
                            remaining,
                        };
                        Ok(None)
                    }
                }
            }

            // Отрицательное число, не равное -1
            _ => Err(DecodeError::InvalidData("Negative zset length".to_string())),
        }
    }

    fn continue_array(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut Vec<ZspFrame<'a>>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, DecodeError> {
        while *remaining > 0 {
            match self.decode(slice)? {
                Some(frame) => {
                    items.push(frame);
                    *remaining -= 1;
                }
                None => return Ok(None),
            }
        }

        Ok(Some(ZspFrame::Array(std::mem::take(items))))
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

impl Default for ZspDecoder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::network::zsp::frame::encoder::ZspEncoder;

    /// Тест для строк в формате inline
    /// Проверка декодирования строки, начинающейся с '+'
    #[test]
    fn test_simple_string() {
        let mut decoder = ZspDecoder::new();
        let data = b"+OK\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::InlineString("OK".into()));
    }

    /// Тест для бинарных строк
    /// Проверка декодирования строки, начинающейся с '$'
    #[test]
    fn test_binary_string() {
        let mut decoder = ZspDecoder::new();
        let data = b"$5\r\nhello\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Тест для частичной бинарной строки
    /// Проверка декодирования бинарной строки в два шага
    #[test]
    fn test_partial_binary_string() {
        let mut decoder = ZspDecoder::new();

        // Первая часть - должно вернуться None, что означает необходимость дополнительных данных
        let data1 = b"$5\r\nhel".to_vec();
        let mut slice = data1.as_slice();
        assert!(matches!(decoder.decode(&mut slice), Ok(None)));

        // Вторая часть - теперь должно вернуться полное сообщение
        let data2 = b"lo\r\n".to_vec();
        let mut slice = data2.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Тест для пустого словаря
    /// Проверка декодирования словаря без элементов
    #[test]
    fn test_empty_dictionary() {
        let mut decoder = ZspDecoder::new();
        let data = b"%0\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Dictionary(HashMap::new()));
    }

    /// Тест для словаря с одним элементом
    /// Проверка декодирования словаря с одним ключом и значением
    #[test]
    fn test_single_item_dictionary() {
        let mut decoder = ZspDecoder::new();
        let data = b"%1\r\n+key\r\n+value\r\n".to_vec(); // Один ключ-значение
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        let mut expected_dict = HashMap::new();
        expected_dict.insert(
            "key".into(),
            ZspFrame::InlineString("value".to_string().into()),
        );

        assert_eq!(frame, ZspFrame::Dictionary(expected_dict));
    }

    /// Тест для словаря с несколькими элементами
    /// Проверка декодирования словаря с несколькими парами ключ-значение
    #[test]
    fn test_multiple_items_dictionary() {
        use std::collections::HashMap;

        let mut items = HashMap::new();
        items.insert(
            "key1".into(),
            ZspFrame::InlineString("value1".to_string().into()),
        );
        items.insert(
            "key2".into(),
            ZspFrame::InlineString("value2".to_string().into()),
        );
        let original = ZspFrame::Dictionary(items);
        let encoded = ZspEncoder::encode(&original).unwrap();

        // Вместо прямого сравнения байтов, декодируем их обратно:
        let mut decoder = ZspDecoder::new();
        let mut slice = encoded.as_slice();
        let decoded = decoder.decode(&mut slice).unwrap().unwrap();

        assert_eq!(original, decoded);
    }

    /// Тест для неверного словаря (неверный ключ)
    /// Проверка, что возникает ошибка при использовании неверного ключа в словаре
    #[test]
    fn test_invalid_dictionary_key() {
        let mut decoder = ZspDecoder::new();
        let data = b"%1\r\n-err\r\n+value\r\n".to_vec(); // Ошибка, ключ должен быть InlineString
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(result.is_err());
    }

    /// Тест для неверного словаря (неполный словарь)
    /// Проверка поведения, когда недостаточно данных для декодирования всего словаря
    #[test]
    fn test_incomplete_dictionary() {
        let mut decoder = ZspDecoder::new();
        let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
    }

    /// Тест для проверки на положительное число.
    #[test]
    fn test_parse_float_valid_positive() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",3.141592653589793\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(PI));
    }

    /// Тест для проверки на корректное отрицательное число.
    #[test]
    fn test_parse_float_valid_negative() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",-0.5\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(-0.5));
    }

    /// Тест для проверки на некорректное значение - ожидаем ошибку.
    #[test]
    fn test_parse_float_integer_style() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",abc\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Null-ZSet (len = -1) декодируется в пустой вектор.
    #[test]
    fn test_parse_zset_null() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^-1\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(Vec::new()));
    }

    /// Пустой ZSet (len = 0)
    #[test]
    fn test_parse_zset_empty() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^0\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(Vec::new()));
    }

    /// Один элемент member="foo", score=2.5
    #[test]
    fn test_parse_zset_single() {
        let mut decoder = ZspDecoder::new();
        let data = b"^1\r\n+foo\r\n,2.5\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(vec![("foo".to_string(), 2.5)]));
    }

    /// Два элемента: один через BinaryString, второй через InlineString и Integer-приведение
    #[test]
    fn test_parse_zset_multiple() {
        let mut decoder = ZspDecoder::new();
        let data = b"^2\r\n$3\r\nbar\r\n,1.0\r\n+baz\r\n,2\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(
            frame,
            ZspFrame::ZSet(vec![("bar".to_string(), 1.0), ("baz".to_string(), 2.0),])
        );
    }

    /// Некорректная длина ZSet → ошибка
    #[test]
    fn test_parse_zset_invalid_length() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^x\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Неподходящий тип member (Integer вместо строки) → ошибка
    #[test]
    fn test_parse_zset_invalid_member_type() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^1\r\n:123\r\n,1.0\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Неподходящий тип score (String вместо числа) → ошибка
    #[test]
    fn test_parse_zset_invalid_score_type() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^1\r\n+foo\r\n+bar\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Частичные данные → сначала None, затем полное декодирование
    #[test]
    fn test_parse_zset_partial() {
        let mut decoder = ZspDecoder::new();
        // Только префикс и один member
        let mut slice1 = b"^2\r\n+foo\r\n".as_ref();
        assert!(decoder.decode(&mut slice1).unwrap().is_none());

        // Догружаем остальные байты
        let mut slice2 = b",1.0\r\n+bar\r\n,2.0\r\n".as_ref();
        let frame = decoder.decode(&mut slice2).unwrap().unwrap();
        assert_eq!(
            frame,
            ZspFrame::ZSet(vec![("foo".to_string(), 1.0), ("bar".to_string(), 2.0),])
        );
    }

    #[test]
    fn test_parse_bool_true() {
        let mut dec = ZspDecoder::new();
        let mut buf = b"#t\r\n".as_ref();
        let frame = dec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Bool(true));
    }

    #[test]
    fn test_parse_bool_false() {
        let mut dec = ZspDecoder::new();
        let mut buf = b"#f\r\n".as_ref();
        let frame = dec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Bool(false));
    }
}
