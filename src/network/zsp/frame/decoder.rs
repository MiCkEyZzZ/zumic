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

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use bytes::Buf;
use memchr::memchr;

use super::zsp_types::ZspFrame;
use crate::ZspDecodeError;

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
    PartialDictionary {
        len: usize,
        items: HashMap<Cow<'a, str>, ZspFrame<'a>>,
        remaining: usize,
        pending_key: Option<Cow<'a, str>>,
    },
    PartialSet {
        len: usize,
        items: HashSet<ZspFrame<'a>>,
        remaining: usize,
    },
    PartialPush {
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

    pub fn decode(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
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
            ZspDecodeState::PartialDictionary {
                len,
                mut items,
                mut remaining,
                mut pending_key,
            } => {
                match self.continue_dictionary(
                    slice,
                    &mut items,
                    &mut remaining,
                    &mut pending_key,
                )? {
                    Some(dict) => Ok(Some(dict)),
                    None => {
                        self.state = ZspDecodeState::PartialDictionary {
                            len,
                            items,
                            remaining,
                            pending_key,
                        };
                        Ok(None)
                    }
                }
            }
            ZspDecodeState::PartialSet {
                len,
                mut items,
                mut remaining,
            } => match self.continue_set(slice, &mut items, &mut remaining)? {
                Some(set) => Ok(Some(set)),
                None => {
                    self.state = ZspDecodeState::PartialSet {
                        len,
                        items,
                        remaining,
                    };
                    Ok(None)
                }
            },
            ZspDecodeState::PartialPush {
                len,
                mut items,
                mut remaining,
            } => match self.continue_push(slice, &mut items, &mut remaining)? {
                Some(push) => Ok(Some(push)),
                None => {
                    self.state = ZspDecodeState::PartialPush {
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

    fn initial_decode(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        if !slice.has_remaining() {
            return Ok(None);
        }
        match slice.get_u8() {
            b'+' => self.parse_inline_string(slice),
            b'-' => self.parse_error(slice),
            b':' => self.parse_integer(slice),
            b',' => self.parse_float(slice),
            b'#' => self.parse_bool(slice),
            b'$' => self.parse_binary_string(slice),
            b'_' => self.parse_null(slice),
            b'*' => self.parse_array(slice, 0),
            b'%' => self.parse_dictionary(slice),
            b'~' => self.parse_set(slice),
            b'>' => self.parse_push(slice),
            b'^' => self.parse_zset(slice),
            _ => Err(ZspDecodeError::InvalidData(
                "Unknown ZSP/RESP3 type".to_string(),
            )),
        }
    }

    fn parse_inline_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZspFrame::InlineString(Cow::Borrowed(line))))
    }

    fn parse_error(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let line = self.read_line(slice)?;
        Ok(Some(ZspFrame::FrameError(line.to_string())))
    }

    fn parse_integer(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let line = self.read_line(slice)?;
        let num = line
            .parse::<i64>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid integer".to_string()))?;
        Ok(Some(ZspFrame::Integer(num)))
    }

    /// Декодирует число с плавающей точкой (префикс `,`).
    /// Формат: `,<float>\r\n`
    fn parse_float(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let line = self.read_line(slice)?;

        // RESP3 поддерживает inf и -inf
        let num = match line {
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            _ => line
                .parse::<f64>()
                .map_err(|_| ZspDecodeError::InvalidData("Invalid float".to_string()))?,
        };

        Ok(Some(ZspFrame::Float(num)))
    }

    /// Парсит булево значение: `#t\r\n` или `#f\r\n`
    fn parse_bool(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        if slice.len() < 3 {
            return Err(ZspDecodeError::UnexpectedEof("Incomplete boolean".into()));
        }
        let b = slice[0];
        if slice[1] != b'\r' || slice[2] != b'\n' {
            return Err(ZspDecodeError::InvalidData("Invalid boolean format".into()));
        }
        *slice = &slice[3..];
        match b {
            b't' => Ok(Some(ZspFrame::Bool(true))),
            b'f' => Ok(Some(ZspFrame::Bool(false))),
            _ => Err(ZspDecodeError::InvalidData("Unknown boolean value".into())),
        }
    }

    fn parse_null(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        if slice.len() < 2 {
            return Err(ZspDecodeError::UnexpectedEof("Incomplete null".into()));
        }
        if slice[0] != b'\r' || slice[1] != b'\n' {
            return Err(ZspDecodeError::InvalidData("Invalid null format".into()));
        }
        *slice = &slice[2..];
        Ok(Some(ZspFrame::Null))
    }

    fn parse_binary_string(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let len = self
            .read_line(slice)?
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid binary".to_string()))?;

        match len {
            -1 => Ok(Some(ZspFrame::BinaryString(None))),
            len if len >= 0 => {
                let len = len as usize;
                if len > MAX_BINARY_LENGTH {
                    let err_msg = format!("Binary string too long ({len} > {MAX_BINARY_LENGTH})");
                    return Err(ZspDecodeError::InvalidData(err_msg));
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
            _ => Err(ZspDecodeError::InvalidData(
                "Negative binary length".to_string(),
            )),
        }
    }

    fn continue_binary_string(
        &mut self,
        slice: &mut &'a [u8],
        len: usize,
        data: &mut Vec<u8>,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let remaining_bytes = len - data.len();
        let available = slice.remaining().min(remaining_bytes);

        if data.capacity() < len {
            data.reserve(len - data.capacity());
        }

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
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        if depth > MAX_ARRAY_DEPTH {
            return Err(ZspDecodeError::InvalidData(
                "Max array depth exceeded".to_string(),
            ));
        }

        let len = self
            .read_line(slice)?
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid array length".to_string()))?;

        match len {
            -1 | 0 => Ok(Some(ZspFrame::Array(Vec::new()))),
            len if len > 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut remaining = len;

                while remaining > 0 {
                    match self.decode(slice)? {
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
            _ => Err(ZspDecodeError::InvalidData(
                "Negative array length".to_string(),
            )),
        }
    }

    fn continue_array(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut Vec<ZspFrame<'a>>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
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

    /// ZSP: Map - %2\r\n$3\r\nkey\r\n$5\r\nvalue\r\n
    fn parse_dictionary(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid dictionary length".to_string()))?;

        match len {
            0 => Ok(Some(ZspFrame::Dictionary(HashMap::new()))),
            len if len > 0 => {
                let len = len as usize;
                let mut items = HashMap::with_capacity(len);
                let mut remaining = len;
                let mut pending_key = None;

                match self.continue_dictionary(
                    slice,
                    &mut items,
                    &mut remaining,
                    &mut pending_key,
                )? {
                    Some(dict) => Ok(Some(dict)),
                    None => {
                        self.state = ZspDecodeState::PartialDictionary {
                            len,
                            items,
                            remaining,
                            pending_key,
                        };
                        Ok(None)
                    }
                }
            }
            _ => Err(ZspDecodeError::InvalidData(
                "Negative dictionary length".to_string(),
            )),
        }
    }

    fn continue_dictionary(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut HashMap<Cow<'a, str>, ZspFrame<'a>>,
        remaining: &mut usize,
        pending_key: &mut Option<Cow<'a, str>>,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        while *remaining > 0 {
            if pending_key.is_none() {
                // Читаем ключ
                match self.decode(slice)? {
                    Some(frame) => {
                        let key = match frame {
                            ZspFrame::BinaryString(Some(bytes)) => String::from_utf8(bytes)
                                .map_err(|_| {
                                    ZspDecodeError::InvalidData("Key not UTF-8".to_string())
                                })?,
                            ZspFrame::InlineString(s) => s.into_owned(),
                            _ => {
                                return Err(ZspDecodeError::InvalidData(
                                    "Expected string key".to_string(),
                                ))
                            }
                        };
                        *pending_key = Some(Cow::Owned(key));
                    }
                    None => return Ok(None),
                }
            } else {
                // Читаем значение
                match self.decode(slice)? {
                    Some(value) => {
                        let key = pending_key.take().unwrap();
                        items.insert(key, value);
                        *remaining -= 1;
                    }
                    None => return Ok(None),
                }
            }
        }
        Ok(Some(ZspFrame::Dictionary(std::mem::take(items))))
    }

    /// ZSP: Set - ~3\r\n+member1\r\n+member2\r\n+member3\r\n
    fn parse_set(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid set length".to_string()))?;

        match len {
            0 => Ok(Some(ZspFrame::Set(HashSet::new()))),
            len if len > 0 => {
                let len = len as usize;
                let mut items = HashSet::with_capacity(len);
                let mut remaining = len;

                match self.continue_set(slice, &mut items, &mut remaining)? {
                    Some(set) => Ok(Some(set)),
                    None => {
                        self.state = ZspDecodeState::PartialSet {
                            len,
                            items,
                            remaining,
                        };
                        Ok(None)
                    }
                }
            }
            _ => Err(ZspDecodeError::InvalidData(
                "Negative set length".to_string(),
            )),
        }
    }

    fn continue_set(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut HashSet<ZspFrame<'a>>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        while *remaining > 0 {
            match self.decode(slice)? {
                Some(frame) => {
                    items.insert(frame);
                    *remaining -= 1;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(ZspFrame::Set(std::mem::take(items))))
    }

    /// ZSP: Push - >3\r\n+message\r\n+channel\r\n$7\r\npayload\r\n
    fn parse_push(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid push length".to_string()))?;

        match len {
            0 => Ok(Some(ZspFrame::Push(Vec::new()))),
            len if len > 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut remaining = len;

                match self.continue_push(slice, &mut items, &mut remaining)? {
                    Some(push) => Ok(Some(push)),
                    None => {
                        self.state = ZspDecodeState::PartialPush {
                            len,
                            items,
                            remaining,
                        };
                        Ok(None)
                    }
                }
            }
            _ => Err(ZspDecodeError::InvalidData(
                "Negative push length".to_string(),
            )),
        }
    }

    fn continue_push(
        &mut self,
        slice: &mut &'a [u8],
        items: &mut Vec<ZspFrame<'a>>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        while *remaining > 0 {
            match self.decode(slice)? {
                Some(frame) => {
                    items.push(frame);
                    *remaining -= 1;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(ZspFrame::Push(std::mem::take(items))))
    }

    /// ZSP: ZSet -
    /// ^2\r\n$7\r\nmember1\r\n,1.5\r\n$7\r\nmember2\r\n,2.5\r\n
    fn parse_zset(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        let len_str = self.read_line(slice)?;
        let len = len_str
            .parse::<isize>()
            .map_err(|_| ZspDecodeError::InvalidData("Invalid zset length".to_string()))?;

        match len {
            -1 | 0 => Ok(Some(ZspFrame::ZSet(Vec::new()))),
            len if len > 0 => {
                let len = len as usize;
                let mut items = Vec::with_capacity(len);
                let mut pending_member = None;
                let mut remaining = len;

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
            _ => Err(ZspDecodeError::InvalidData(
                "Invalid zset length".to_string(),
            )),
        }
    }

    fn continue_zset(
        &mut self,
        slice: &mut &'a [u8],
        _len: usize,
        items: &mut Vec<(String, f64)>,
        pending_member: &mut Option<String>,
        remaining: &mut usize,
    ) -> Result<Option<ZspFrame<'a>>, ZspDecodeError> {
        while *remaining > 0 {
            if pending_member.is_none() {
                // Ожидаем member
                match self.decode(slice)? {
                    Some(frame) => {
                        let member = match frame {
                            ZspFrame::InlineString(cow) => cow.into_owned(),
                            ZspFrame::BinaryString(Some(bytes)) => String::from_utf8(bytes)
                                .map_err(|_| {
                                    ZspDecodeError::InvalidData(
                                        "Invalid UTF-8 in zset member".to_string(),
                                    )
                                })?,
                            _ => {
                                return Err(ZspDecodeError::InvalidData(
                                    "Expected string for zset member".to_string(),
                                ))
                            }
                        };
                        *pending_member = Some(member);
                    }
                    None => return Ok(None),
                }
            } else {
                // Ожидаем score
                let score = match self.decode(slice)? {
                    Some(ZspFrame::Float(f)) => f,
                    Some(ZspFrame::Integer(i)) => i as f64,
                    Some(_) => {
                        return Err(ZspDecodeError::InvalidData(
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
        Ok(Some(ZspFrame::ZSet(std::mem::take(items))))
    }

    fn read_line(
        &mut self,
        slice: &mut &'a [u8],
    ) -> Result<&'a str, ZspDecodeError> {
        if let Some(pos) = memchr(b'\r', slice) {
            if pos + 1 < slice.len() && slice[pos + 1] == b'\n' {
                let line = &slice[..pos];
                let result = std::str::from_utf8(line)
                    .map_err(|_| ZspDecodeError::InvalidUtf8("Invalid UTF-8".into()))?;
                *slice = &slice[(pos + 2)..];
                return Ok(result);
            }
        }
        Err(ZspDecodeError::UnexpectedEof("Incomplete line".to_string()))
    }

    #[inline(always)]
    fn expect_crlf(
        &self,
        slice: &mut &'a [u8],
    ) -> Result<(), ZspDecodeError> {
        if slice.len() < 2 || slice[0] != b'\r' || slice[1] != b'\n' {
            return Err(ZspDecodeError::UnexpectedEof("Expected CRLF".to_string()));
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

    /// Тест проверяет декодирование простой inline-строки с
    /// префиксом '+'.
    #[test]
    fn test_simple_string() {
        let mut decoder = ZspDecoder::new();
        let data = b"+OK\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::InlineString("OK".into()));
    }

    /// Тест проверяет декодирование бинарной строки, начинающей
    /// ся с '$'.
    #[test]
    fn test_binary_string() {
        let mut decoder = ZspDecoder::new();
        let data = b"$5\r\nhello\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Тест проверяет декодирование бинарной строки в два этапа
    /// (partial).
    #[test]
    fn test_partial_binary_string() {
        let mut decoder = ZspDecoder::new();

        // Первая часть - должно вернуться None, что означает необходимость
        // дополнительных данных
        let data1 = b"$5\r\nhel".to_vec();
        let mut slice = data1.as_slice();
        assert!(matches!(decoder.decode(&mut slice), Ok(None)));

        // Вторая часть - теперь должно вернуться полное сообщение
        let data2 = b"lo\r\n".to_vec();
        let mut slice = data2.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::BinaryString(Some(b"hello".to_vec())));
    }

    /// Тест проверяет декодирование пустого словаря.
    #[test]
    fn test_empty_dictionary() {
        let mut decoder = ZspDecoder::new();
        let data = b"%0\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Dictionary(HashMap::new()));
    }

    /// Тест проверяет декодирование словаря с одним элементом.
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

    /// Тест проверяет декодирование словаря с несколькими
    /// элементами.
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

    /// Тест проверяет, что возникает ошибка при неверном ключе
    /// словаря.
    #[test]
    fn test_invalid_dictionary_key() {
        let mut decoder = ZspDecoder::new();
        let data = b"%1\r\n-err\r\n+value\r\n".to_vec(); // Ошибка, ключ должен быть InlineString
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(result.is_err());
    }

    /// Тест проверяет поведение декодера при неполных данных
    /// словаря.
    #[test]
    fn test_incomplete_dictionary() {
        let mut decoder = ZspDecoder::new();
        let data = b"%2\r\n+key1\r\n+value1\r\n".to_vec(); // Недостаточно данных для второго элемента
        let mut slice = data.as_slice();
        let result = decoder.decode(&mut slice);
        assert!(matches!(result, Ok(None))); // Ожидаем Ok(None)
    }

    /// Тест проверяет декодирование положительного числа с
    /// плавающей точкой.
    #[test]
    fn test_parse_float_valid_positive() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",3.141592653589793\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(PI));
    }

    /// Тест проверяет декодирование корректного отрицательного
    /// числа с плавающей точкой.
    #[test]
    fn test_parse_float_valid_negative() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",-0.5\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(-0.5));
    }

    /// Тест проверяет, что при некорректном значении возникает
    /// ошибка.
    #[test]
    fn test_parse_float_integer_style() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",abc\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Тест проверяет декодирование Null-ZSet с длиной -1 как
    /// пустой вектор.
    #[test]
    fn test_parse_zset_null() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^-1\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(Vec::new()));
    }

    /// Тест проверяет декодирование пустого ZSet с длиной 0.
    #[test]
    fn test_parse_zset_empty() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^0\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(Vec::new()));
    }

    /// Тест проверяет декодирование ZSet с одним элементом.
    #[test]
    fn test_parse_zset_single() {
        let mut decoder = ZspDecoder::new();
        let data = b"^1\r\n+foo\r\n,2.5\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::ZSet(vec![("foo".to_string(), 2.5)]));
    }

    /// Тест проверяет декодирование ZSet с несколькими элементами.
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

    /// Тест проверяет, что при некорректной длине ZSet возникает
    /// ошибка.
    #[test]
    fn test_parse_zset_invalid_length() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^x\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Тест проверяет ошибку при неверном типе member в ZSet.
    #[test]
    fn test_parse_zset_invalid_member_type() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^1\r\n:123\r\n,1.0\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Тест проверяет ошибку при неверном типе score в ZSet.
    #[test]
    fn test_parse_zset_invalid_score_type() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"^1\r\n+foo\r\n+bar\r\n".as_ref();
        assert!(decoder.decode(&mut slice).is_err());
    }

    /// Тест проверяет частичное декодирование ZSet с последующим
    /// полным.
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

    /// Тест проверяет корректный разбор (декодирование) булевого
    /// значения `true` из байтового потока.
    /// Проверяет, что строка "#t\r\n" декодируется в
    /// `ZspFrame::Bool(true)`.
    #[test]
    fn test_parse_bool_true() {
        let mut dec = ZspDecoder::new();
        let mut buf = b"#t\r\n".as_ref();
        let frame = dec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Bool(true));
    }

    /// Тест проверяет корректный разбор (декодирование) булевого
    /// значения `false` из байтового потока.
    /// Проверяет, что строка "#f\r\n" декодируется в
    /// `ZspFrame::Bool(false)`.
    #[test]
    fn test_parse_bool_false() {
        let mut dec = ZspDecoder::new();
        let mut buf = b"#f\r\n".as_ref();
        let frame = dec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Bool(false));
    }

    #[test]
    fn test_resp3_null() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b"_\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Null);
    }

    #[test]
    fn test_resp3_map_with_binary_keys() {
        let mut decoder = ZspDecoder::new();
        let data = b"%1\r\n$4\r\nkey1\r\n+value1\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        if let ZspFrame::Dictionary(dict) = frame {
            assert_eq!(dict.len(), 1);
            assert!(dict.contains_key("key1"));
        } else {
            panic!("Expected Dictionary");
        }
    }

    #[test]
    fn test_resp3_set() {
        let mut decoder = ZspDecoder::new();
        let data = b"~2\r\n+member1\r\n+member2\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        if let ZspFrame::Set(set) = frame {
            assert_eq!(set.len(), 2);
        } else {
            panic!("Expected Set");
        }
    }

    #[test]
    fn test_resp3_push() {
        let mut decoder = ZspDecoder::new();
        let data = b">3\r\n+message\r\n+channel\r\n$7\r\npayload\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        if let ZspFrame::Push(items) = frame {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected Push");
        }
    }

    #[test]
    fn test_float_infinity() {
        let mut decoder = ZspDecoder::new();
        let mut slice = b",inf\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(f64::INFINITY));

        let mut slice = b",-inf\r\n".as_ref();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();
        assert_eq!(frame, ZspFrame::Float(f64::NEG_INFINITY));
    }

    #[test]
    fn test_zset_with_binary_strings() {
        let mut decoder = ZspDecoder::new();
        let data = b"^2\r\n$7\r\nmember1\r\n,1.5\r\n$7\r\nmember2\r\n,2.5\r\n".to_vec();
        let mut slice = data.as_slice();
        let frame = decoder.decode(&mut slice).unwrap().unwrap();

        assert_eq!(
            frame,
            ZspFrame::ZSet(vec![
                ("member1".to_string(), 1.5),
                ("member2".to_string(), 2.5),
            ])
        );
    }
}
