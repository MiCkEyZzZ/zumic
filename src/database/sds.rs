//! Модуль "Умная динамическая строка" (Sds)
//!
//! Этот модуль реализует эффективную структуру данных для хранения строк,
//! которая использует стек (stack) для коротких строк и кучу (heap) для длинных,
//! обеспечивая компактность и высокую производительность.
//! Структура автоматически переключается между двумя режимами хранения в зависимости от длины строки.

use std::{
    cmp::Ordering,
    convert::TryFrom,
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    str::{from_utf8, Utf8Error},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Представление строки: в стеке (короткая) или в куче (длинная).
#[derive(Debug, Clone)]
enum Repr {
    /// Короткая строка, хранимая напрямую в стеке.
    Inline { len: u8, buf: [u8; Sds::INLINE_CAP] },
    /// Длинная строка, хранимая в куче.
    Heap { buf: Vec<u8>, len: usize },
}

/// Основная структура умной строки.
#[derive(Debug, Clone)]
pub struct Sds(Repr);

// Реализация преобразования из &str в Sds через трейт FromStr.
impl std::str::FromStr for Sds {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Sds::from_str(s))
    }
}

impl Sds {
    /// Максимальный размер строки, при котором используется стековое представление.
    pub const INLINE_CAP: usize = 22;

    /// Создаёт Sds из вектора байт, выбирая стек или кучу в зависимости от размера.
    #[inline(always)]
    pub fn from_vec(vec: Vec<u8>) -> Self {
        let len = vec.len();
        if len <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..len].copy_from_slice(&vec);
            Sds(Repr::Inline {
                len: len as u8,
                buf,
            })
        } else {
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    /// Создаёт Sds из байтов, копируя их при необходимости.
    pub fn from_bytes<B: AsRef<[u8]>>(bytes: B) -> Self {
        let slice = bytes.as_ref();
        if slice.len() <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..slice.len()].copy_from_slice(slice);
            Sds(Repr::Inline {
                len: slice.len() as u8,
                buf,
            })
        } else {
            let vec = slice.to_vec();
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    /// Создаёт строку из &str, автоматически определяя способ хранения.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        if bytes.len() <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..bytes.len()].copy_from_slice(bytes);
            Sds(Repr::Inline {
                len: bytes.len() as u8,
                buf,
            })
        } else {
            let vec = bytes.to_vec();
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    /// Возвращает содержимое строки как срез байт.
    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, buf } => &buf[..*len as usize],
            Repr::Heap { buf, len } => &buf[..*len],
        }
    }

    /// Возвращает байтовое представление строки (аналог `as_slice`).
    pub fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }

    /// Возвращает изменяемый срез байт.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.0 {
            Repr::Inline { len, buf } => &mut buf[..*len as usize],
            Repr::Heap { buf, len } => &mut buf[..*len],
        }
    }

    /// Возвращает текущую длину строки.
    #[inline(always)]
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Inline { len, .. } => *len as usize,
            Repr::Heap { len, .. } => *len,
        }
    }

    /// Проверяет, пуста ли строка.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Возвращает текущую ёмкость буфера (полезно только для кучи).
    pub fn capacity(&self) -> usize {
        match &self.0 {
            Repr::Inline { .. } => Self::INLINE_CAP,
            Repr::Heap { buf, .. } => buf.capacity(),
        }
    }

    /// Резервирует место для дополнительных байт.
    pub fn reserve(
        &mut self,
        additional: usize,
    ) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if cur_len + additional <= Self::INLINE_CAP {
                    return;
                }
                let mut vec = Vec::with_capacity((cur_len + additional).next_power_of_two());
                vec.extend_from_slice(&buf[..cur_len]);
                self.0 = Repr::Heap {
                    len: cur_len,
                    buf: vec,
                };
            }
            Repr::Heap { buf, .. } => buf.reserve(additional),
        }
    }

    /// Очищает содержимое строки (длина = 0).
    pub fn clear(&mut self) {
        match &mut self.0 {
            Repr::Inline { len, .. } => *len = 0,
            Repr::Heap { len, .. } => *len = 0,
        }
    }

    /// Добавляет один байт в конец строки.
    pub fn push(
        &mut self,
        byte: u8,
    ) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if cur_len < Self::INLINE_CAP {
                    buf[cur_len] = byte;
                    *len += 1;
                } else {
                    let mut vec = Vec::with_capacity((cur_len + 1).next_power_of_two());
                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.push(byte);
                    self.0 = Repr::Heap {
                        len: vec.len(),
                        buf: vec,
                    };
                }
            }
            Repr::Heap { buf, len } => {
                if *len < buf.len() {
                    buf[*len] = byte;
                } else {
                    buf.push(byte);
                }
                *len += 1;
            }
        }
    }

    /// Добавляет байтовую строку в конец текущей строки.
    pub fn append(
        &mut self,
        other: &[u8],
    ) {
        let total = self.len() + other.len();
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                if total <= Self::INLINE_CAP {
                    buf[cur_len..total].copy_from_slice(other);
                    *len = total as u8;
                } else {
                    let mut vec = Vec::with_capacity(total.next_power_of_two());
                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.extend_from_slice(other);
                    self.0 = Repr::Heap {
                        len: vec.len(),
                        buf: vec,
                    };
                }
            }
            Repr::Heap { buf, len } => {
                let cur_len = *len;
                let needed = cur_len + other.len();

                if buf.capacity() < needed {
                    buf.reserve((needed - buf.len()).next_power_of_two());
                }

                if buf.len() < needed {
                    buf.extend_from_slice(other);
                } else {
                    buf[cur_len..needed].copy_from_slice(other);
                }

                *len = needed;
            }
        }
    }

    /// Обрезает строку до указанной длины.
    pub fn truncate(
        &mut self,
        new_len: usize,
    ) {
        match &mut self.0 {
            Repr::Inline { len, .. } => {
                *len = new_len.min(*len as usize) as u8;
            }
            Repr::Heap { len, .. } => {
                *len = new_len.min(*len);
            }
        }
        self.inline_downgrade();
    }

    /// Возвращает срез строки в указанном диапазоне.
    pub fn slice_range(
        &self,
        start: usize,
        end: usize,
    ) -> Self {
        assert!(start <= end && end <= self.len(), "invalid slice range");
        let slice = &self.as_slice()[start..end];

        if slice.len() <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];
            buf[..slice.len()].copy_from_slice(slice);
            Sds(Repr::Inline {
                len: slice.len() as u8,
                buf,
            })
        } else {
            let mut vec = Vec::with_capacity(slice.len());
            vec.extend_from_slice(slice);
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }

    /// Преобразует строку обратно в стековое представление, если она стала достаточно короткой.
    fn inline_downgrade(&mut self) {
        if let Repr::Heap { buf, len } = &self.0 {
            if *len <= Self::INLINE_CAP {
                let mut inline_buf = [0u8; Self::INLINE_CAP];
                inline_buf[..*len].copy_from_slice(&buf[..*len]);
                self.0 = Repr::Inline {
                    len: *len as u8,
                    buf: inline_buf,
                }
            }
        }
    }

    /// Преобразует байтовое представление строки в `&str`, если она валидна как UTF-8.
    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        from_utf8(self.as_slice())
    }
}

impl Default for Sds {
    fn default() -> Self {
        Sds(Repr::Inline {
            len: 0,
            buf: [0u8; Sds::INLINE_CAP],
        })
    }
}

impl Deref for Sds {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for Sds {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl Display for Sds {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self.as_str() {
            Ok(s) => write!(f, "{s}"),
            Err(_) => write!(f, "{:?}", self.as_slice()),
        }
    }
}

impl Hash for Sds {
    fn hash<H: Hasher>(
        &self,
        state: &mut H,
    ) {
        self.as_slice().hash(state);
    }
}

impl PartialEq for Sds {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Sds {}

impl PartialOrd for Sds {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sds {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl TryFrom<Sds> for String {
    type Error = Utf8Error;
    fn try_from(value: Sds) -> Result<Self, Self::Error> {
        value.as_str().map(|s| s.to_string())
    }
}

impl Serialize for Sds {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.as_slice())
    }
}

impl<'de> Deserialize<'de> for Sds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        Ok(Sds::from_vec(bytes))
    }
}

impl From<&[u8]> for Sds {
    fn from(slice: &[u8]) -> Self {
        if slice.len() <= Sds::INLINE_CAP {
            let mut buf = [0u8; Sds::INLINE_CAP];
            buf[..slice.len()].copy_from_slice(slice);
            Sds(Repr::Inline {
                len: slice.len() as u8,
                buf,
            })
        } else {
            let mut vec = Vec::with_capacity(slice.len());
            vec.extend_from_slice(slice);
            let len = vec.len();
            Sds(Repr::Heap { buf: vec, len })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест проверяет создание строки, которая помещается в стековое представление.
    #[test]
    fn test_inline_creation_from_str() {
        let s = Sds::from_str("hello");
        assert_eq!(s.len(), 5);
        assert_eq!(s.as_slice(), b"hello");
        assert!(matches!(s.0, Repr::Inline { .. }));
    }

    /// Тест проверяет создание строки, которая превышает лимит стека и переходит в кучу.
    #[test]
    fn test_heap_creation_from_str() {
        let long = "this is a long string exceeding the inline cap";
        let s = Sds::from_str(long);
        assert_eq!(s.len(), long.len());
        assert_eq!(s.as_slice(), long.as_bytes());
        assert!(matches!(s.0, Repr::Heap { .. }));
    }

    /// Тест проверяет добавление одного байта, помещающегося в стековое представление.
    #[test]
    fn test_push_within_inline() {
        let mut s = Sds::from_str("12345");
        s.push(b'6');
        assert_eq!(s.as_slice(), b"123456");
        assert!(matches!(s.0, Repr::Inline { .. }));
    }

    /// Тест проверяет переход от стека к куче при добавлении байта, превышающего лимит.
    #[test]
    fn test_push_exceeding_inline() {
        let mut s = Sds::from_str("a".repeat(Sds::INLINE_CAP).as_str());
        s.push(b'x');
        assert!(matches!(s.0, Repr::Heap { .. }));
        assert_eq!(s.len(), Sds::INLINE_CAP + 1);
    }

    /// Тест проверяет добавление байтов, не превышающее стековое представление.
    #[test]
    fn test_append_within_inline() {
        let mut s = Sds::from_str("123");
        s.append(b"456");
        assert_eq!(s.as_slice(), b"123456");
        assert!(matches!(s.0, Repr::Inline { .. }));
    }

    /// Тест проверяет добавление байтов, вызывающее переход к кучи.
    #[test]
    fn test_append_exceeding_inline() {
        let mut s = Sds::from_str("hello");
        s.append(b" world this is too long");
        assert!(matches!(s.0, Repr::Heap { .. }));
        assert_eq!(s.as_str().unwrap(), "hello world this is too long");
    }

    /// Тест проверяет очистку содержимого строки.
    #[test]
    fn test_clear() {
        let mut s = Sds::from_str("hello");
        s.clear();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    /// Тест проверяет усечение строки до заданной длины.
    #[test]
    fn test_truncate() {
        let mut s = Sds::from_str("hello world");
        s.truncate(5);
        assert_eq!(s.as_str().unwrap(), "hello");
    }

    /// Тест проверяет, что усечение переводит строку из кучи обратно в стек при возможности.
    #[test]
    fn test_truncate_to_inline() {
        let mut s = Sds::from_str("a very very long string indeed");
        assert!(matches!(s.0, Repr::Heap { .. }));
        s.truncate(5);
        assert!(matches!(s.0, Repr::Inline { .. }));
        assert_eq!(s.as_str().unwrap(), "a ver");
    }

    /// Тест проверяет извлечение подстроки в заданном диапазоне.
    #[test]
    fn test_slice_range() {
        let s = Sds::from_str("abcdefg");
        let sliced = s.slice_range(2, 5);
        assert_eq!(sliced.as_slice(), b"cde");
    }

    /// Тест проверяет корректный вывод строки, если она валидна как UTF-8.
    #[test]
    fn test_display_valid_utf8() {
        let s = Sds::from_str("test");
        assert_eq!(format!("{s}"), "test");
    }

    /// Тест сравнения строк на равенство и порядок.
    #[test]
    fn test_equality_and_ordering() {
        let a = Sds::from_str("abc");
        let b = Sds::from_str("abc");
        let c = Sds::from_str("def");
        assert_eq!(a, b);
        assert!(a < c);
    }

    /// Тест корректного преобразования строки в `String`, если это допустимая UTF-8.
    #[test]
    fn test_try_from_valid_utf8() {
        let s = Sds::from_str("hello");
        let string: String = s.try_into().unwrap();
        assert_eq!(string, "hello");
    }

    /// Тест на совпадение хэшей для одинаковых строк.
    #[test]
    fn test_hashing_consistency() {
        use std::collections::hash_map::DefaultHasher;
        let a = Sds::from_str("foo");
        let b = Sds::from_str("foo");
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        a.hash(&mut hasher1);
        b.hash(&mut hasher2);
        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    /// Тест на корректную работу срезов.
    #[test]
    fn test_check_slice_range() {
        let s = Sds::from_str("Hello, world!");
        let sliced = s.slice_range(0, 5); // Ожидается "Hello"
        assert_eq!(sliced.as_str().unwrap(), "Hello");

        let sliced = s.slice_range(7, 12); // Ожидается "world"
        assert_eq!(sliced.as_str().unwrap(), "world");
    }

    /// Тест, что недопустимая строка не может быть преобразована в UTF-8.
    #[test]
    fn test_invalid_utf8() {
        let invalid_bytes = vec![0x80, 0x80, 0x80]; // Невалидные байты UTF-8
        let s = Sds::from_vec(invalid_bytes);
        assert!(s.as_str().is_err()); // Ожидается ошибка при преобразовании
    }

    /// Тест выделения памяти при добавлении данных.
    #[test]
    fn test_reserve() {
        let mut s = Sds::from_str("Hello");
        s.reserve(10); // Резервируем дополнительную память
        assert!(s.capacity() >= 15);
        assert_eq!(s.len(), 5);
    }

    /// Тест на реализацию Deref для Sds.
    #[test]
    fn test_deref() {
        let s = Sds::from_str("Hello, world!");
        let slice: &[u8] = &s; // Использование Deref
        assert_eq!(slice, b"Hello, world!");
    }

    /// Тест на реализацию DerefMut для Sds.
    #[test]
    fn test_deref_mut() {
        let mut s = Sds::from_str("Hello");
        let slice: &mut [u8] = &mut s; // Использование DerefMut
        slice[0] = b'J'; // Изменяем первый символ
        assert_eq!(s.as_str().unwrap(), "Jello");
    }

    /// Тест, что метод `push` не портит строку.
    #[test]
    fn test_push_integrity() {
        let mut s = Sds::from_str("Rust");
        s.push(b'!');
        assert_eq!(s.as_str().unwrap(), "Rust!");
    }
}
