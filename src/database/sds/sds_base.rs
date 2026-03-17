use std::{
    borrow::Borrow,
    cmp::Ordering,
    convert::TryFrom,
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
    str::{from_utf8, Utf8Error},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone)]
enum Repr {
    Inline { len: u8, buf: [u8; Sds::INLINE_CAP] },
    Heap { buf: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct Sds(Repr);

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl Sds {
    /// Максимальная длина строки для inline-хранения.
    pub const INLINE_CAP: usize = std::mem::size_of::<usize>() * 3 - 1;

    /// Создаёт `Sds` из вектора байт, выбирая `inline` или `heap` в зависимости
    /// от длины.
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
            Sds(Repr::Heap { buf: vec })
        }
    }

    /// Создаёт `Sds` из байтов среза, копируя данные.
    #[inline]
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
            Sds(Repr::Heap {
                buf: slice.to_vec(),
            })
        }
    }

    /// Создаёт `Sds` из `&str`.
    #[allow(clippy::should_implement_trait)]
    #[inline]
    pub fn from_str(s: &str) -> Self {
        Self::from_bytes(s.as_bytes())
    }

    #[inline]
    pub fn from_string(s: String) -> Self {
        let vec = s.into_bytes();

        if vec.len() <= Self::INLINE_CAP {
            Self::from_vec(vec)
        } else {
            Sds(Repr::Heap { buf: vec })
        }
    }

    /// Создаёт пустую `Sds` с предвыделенной ёмкостью.
    ///
    /// * `cap <= INLINE_CAP` → пустая inline-строка, аллокации нет.
    /// * `cap > INLINE_CAP`  → выделяет heap-буфер ёмкостью `cap`.
    ///
    /// Полезно для сценариев с известным максимальным размером строки,
    /// когда нужно избежать повторных реаллокаций.
    ///
    /// # Примеры
    ///
    /// ```rust
    /// use zumic::Sds;
    ///
    /// let mut s = Sds::with_capacity(128);
    /// assert!(s.is_empty());
    /// assert!(s.capacity() >= 128);
    ///
    /// s.append(b"hello");
    /// assert_eq!(s.as_slice(), b"hello");
    /// ```
    pub fn with_capacity(cap: usize) -> Self {
        if cap <= Self::INLINE_CAP {
            Self::default()
        } else {
            Sds(Repr::Heap {
                buf: Vec::with_capacity(cap),
            })
        }
    }

    /// Создаёт строку из `n` копий байта `byte`.
    ///
    /// * `n == 0` → пустая inline-строка.
    /// * `n <= INLINE_CAP` → inline без аллокации.
    /// * `n > INLINE_CAP`  → heap.
    ///
    /// # Примеры
    ///
    /// ```rust
    /// use zumic::Sds;
    ///
    /// assert_eq!(Sds::repeat(b'0', 5).as_slice(), b"00000");
    /// assert!(Sds::repeat(b'x', 0).is_empty());
    /// ```
    pub fn repeat(
        byte: u8,
        n: usize,
    ) -> Self {
        if n == 0 {
            return Self::default();
        }

        if n <= Self::INLINE_CAP {
            let mut buf = [0u8; Self::INLINE_CAP];

            buf[..n].fill(byte);

            Sds(Repr::Inline { len: n as u8, buf })
        } else {
            Sds(Repr::Heap { buf: vec![byte; n] })
        }
    }

    /// Создаёт `Sds` из байтового среза, заменяя невалидные UTF-8
    /// последовательности символом замены `U+FFFD` (`\u{FFFD}`).
    ///
    /// Аналог [`String::from_utf8_lossy`].
    /// Результирующая строка гарантированно является валидным UTF-8.
    ///
    /// # Примеры
    ///
    /// ```rust
    /// use zumic::Sds;
    ///
    /// let s = Sds::from_utf8_lossy(b"hello\xff");
    /// assert!(s.as_str().is_ok());
    /// assert!(s.as_str().unwrap().contains('\u{FFFD}'));
    /// ```
    pub fn from_utf8_lossy(bytes: &[u8]) -> Self {
        // `from_utf8_lossy` возвращает `Cow<str>`:
        // - `Borrowed` если весь ввод валидный UTF-8 (zero-copy путь)
        // - `Owned`    если были замены (одна аллокация)
        let cow = String::from_utf8_lossy(bytes);

        Self::from_bytes(cow.as_bytes())
    }

    /// Возвращает сырой указатель на начало буфера.
    ///
    /// Это low-level API для FFI и высокопроизводительных операций.
    ///
    /// Гарантии:
    /// - указатель валиден пока существует данный `Sds`
    /// - указатель становится недействительным после любых операций realloc
    /// - указатель может использоваться только для чтения
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.as_slice().as_ptr()
    }

    /// Возвращает изменяемый сырой указатель на начало буфера.
    ///
    /// Безопасно изменять только диапазон `[0, len())`.
    /// Изменение capacity или выход за границы вызывает UB.
    ///
    /// Это low-level API.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_slice().as_mut_ptr()
    }

    /// Возвращает содержимое строки как срез байт.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            Repr::Inline { len, buf } => &buf[..*len as usize],
            Repr::Heap { buf } => buf.as_slice(),
        }
    }

    /// Псевдоним для [`as_slice`](Sds::as_slice).
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }

    /// Возвращает изменяемый срез текущего содержимого строки.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match &mut self.0 {
            Repr::Inline { len, buf } => &mut buf[..*len as usize],
            Repr::Heap { buf } => buf.as_mut_slice(),
        }
    }

    /// Возвращает текущую длину строки в байтах.
    #[inline]
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Inline { len, .. } => *len as usize,
            Repr::Heap { buf } => buf.len(),
        }
    }

    /// Возвращает `true`, если строка пустая.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Возвращает текущую ёмкость буфера.
    #[inline]
    pub fn capacity(&self) -> usize {
        match &self.0 {
            Repr::Inline { .. } => Self::INLINE_CAP,
            Repr::Heap { buf, .. } => buf.capacity(),
        }
    }

    /// Возвращает `true`, если строка хранится на стеке (inline).
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self.0, Repr::Inline { .. })
    }

    /// Резервирует место для как минимум `additional` дополнительных байт.
    pub fn reserve(
        &mut self,
        additional: usize,
    ) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                let required = cur_len + additional;

                if required <= Self::INLINE_CAP {
                    return;
                }

                let mut vec = Vec::with_capacity(required);
                vec.extend_from_slice(&buf[..cur_len]);

                self.0 = Repr::Heap { buf: vec };
            }
            Repr::Heap { buf } => buf.reserve(additional),
        }
    }

    /// Очищает содержимое строки, устанавливая длину в 0.
    pub fn clear(&mut self) {
        match &mut self.0 {
            Repr::Inline { len, .. } => *len = 0,
            Repr::Heap { buf } => buf.clear(),
        }
    }

    /// Добавляет один байт в конец строки.
    #[inline(always)]
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
                    let mut vec = Vec::with_capacity(cur_len + 1);

                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.push(byte);

                    self.0 = Repr::Heap { buf: vec };
                }
            }
            Repr::Heap { buf } => buf.push(byte),
        }
    }

    /// Добавляет байтовую строку в конец текущей строки.
    #[inline(always)]
    pub fn append(
        &mut self,
        other: &[u8],
    ) {
        match &mut self.0 {
            Repr::Inline { len, buf } => {
                let cur_len = *len as usize;
                let total = cur_len + other.len();

                if total <= Self::INLINE_CAP {
                    buf[cur_len..total].copy_from_slice(other);
                    *len = total as u8;
                } else {
                    let mut vec = Vec::with_capacity(total);

                    vec.extend_from_slice(&buf[..cur_len]);
                    vec.extend_from_slice(other);

                    self.0 = Repr::Heap { buf: vec };
                }
            }
            Repr::Heap { buf } => {
                if buf.capacity() - buf.len() < other.len() {
                    buf.reserve(other.len());
                }
                buf.extend_from_slice(other);
            }
        }
    }

    /// Обрезает строку до `new_len` байт.
    pub fn truncate(
        &mut self,
        new_len: usize,
    ) {
        match &mut self.0 {
            Repr::Inline { len, .. } => {
                if new_len < *len as usize {
                    *len = new_len as u8;
                }
            }
            Repr::Heap { buf } => {
                if new_len < buf.len() {
                    buf.truncate(new_len);
                }
            }
        }

        self.inline_downgrade();
    }

    /// Возвращает копию подстроки в диапазоне `[start, end)`.
    pub fn slice_range(
        &self,
        start: usize,
        end: usize,
    ) -> Self {
        assert!(
            start <= end && end <= self.len(),
            "Sds::slice_range: invalid range [{start}, {end} for len {}]",
            self.len()
        );

        Self::from_bytes(&self.as_slice()[start..end])
    }

    /// Преобразует байтовое представление строки в `&str`, если она валидна
    /// как UTF-8.
    #[inline]
    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        from_utf8(self.as_slice())
    }

    /// Преобразует heap-строку обратно в inline, если длина позволяет.
    fn inline_downgrade(&mut self) {
        if let Repr::Heap { buf } = &self.0 {
            if buf.len() <= Self::INLINE_CAP {
                let len = buf.len();
                let mut inline_buf = [0u8; Self::INLINE_CAP];

                inline_buf[..len].copy_from_slice(&buf[..len]);

                self.0 = Repr::Inline {
                    len: len as u8,
                    buf: inline_buf,
                }
            }
        }
    }

    /// Проверяет внутренние иварианты структуры.
    #[cfg(debug_assertions)]
    pub fn debug_assert_invariants(&self) {
        match &self.0 {
            Repr::Inline { len, buf } => {
                assert!(
                    (*len as usize) <= Self::INLINE_CAP,
                    "Sds invariant violation: Inline len ({}) > INLINE_CAP ({})",
                    len,
                    Self::INLINE_CAP
                );
                // Проверяем, что as_slice не выходит за пределы buf.
                let _ = &buf[..*len as usize];
            }
            Repr::Heap { buf } => {
                // len() и capacity() полностью управляются Vec — нечего
                // дополнительно проверять, кроме согласованности самого Vec.
                assert!(
                    buf.len() <= buf.capacity(),
                    "Sds invariant violation: Heap buf.len() ({}) > buf.capacity() ({})",
                    buf.len(),
                    buf.capacity()
                );
                // Длинная строка не должна помещаться в inline
                // (иначе inline_downgrade не был вызван).
                // Это предупреждение, а не жёсткая гарантия: после reserve()
                // строка может оказаться в heap с len <= INLINE_CAP.
                // Поэтому здесь намеренно нет assert.
            }
        }
    }

    /// No-op в release-сборке.
    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn debug_assert_invariants(&self) {}
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для Sds
////////////////////////////////////////////////////////////////////////////////

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

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for Sds {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl AsRef<[u8]> for Sds {
    /// Позволяет передавать `&Sds` везде, где ожидается `&[u8]`.
    ///
    /// `AsRef<str>` намеренно не реализован: `Sds` может содержать невалидный
    /// UTF-8. Используйте [`Sds::as_str()`] для явной проверки.
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Borrow<[u8]> for Sds {
    /// Позволяет использовать `Sds` как ключ в `HashMap<Sds, V>` и выполнять
    /// поиск по `&[u8]` без создания нового `Sds`
    ///
    /// Контракт `Borrow` гарантирован: `Hash` и `Eq` для `Sds` и `[u8]`
    /// дают одинаковый результат, потому что оба основаны на `as_slice()`.
    ///
    /// # Пример
    ///
    /// ```rust
    /// use std::collections::HashMap;
    ///
    /// use zumic::Sds;
    ///
    /// let mut map: HashMap<Sds, u32> = HashMap::new();
    /// map.insert("key".into(), 42);
    ///
    /// // Поиск по &[u8] — аллокаций нет:
    /// assert_eq!(map.get(b"key".as_ref()), Some(&42));
    /// ```
    #[inline]
    fn borrow(&self) -> &[u8] {
        self.as_slice()
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

impl From<&[u8]> for Sds {
    #[inline]
    fn from(slice: &[u8]) -> Self {
        Sds::from_bytes(slice)
    }
}

impl From<&str> for Sds {
    #[inline]
    fn from(s: &str) -> Self {
        Sds::from_str(s)
    }
}

impl From<String> for Sds {
    #[inline]
    fn from(s: String) -> Self {
        Sds::from_string(s)
    }
}

impl From<Vec<u8>> for Sds {
    #[inline]
    fn from(v: Vec<u8>) -> Self {
        Sds::from_vec(v)
    }
}

impl From<Sds> for Vec<u8> {
    #[inline]
    fn from(s: Sds) -> Self {
        match s.0 {
            Repr::Inline { len, buf } => buf[..len as usize].to_vec(),
            Repr::Heap { buf } => buf,
        }
    }
}

impl FromIterator<u8> for Sds {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut s = Sds::default();

        for byte in iter {
            s.push(byte);
        }

        s
    }
}

impl Extend<u8> for Sds {
    fn extend<T: IntoIterator<Item = u8>>(
        &mut self,
        iter: T,
    ) {
        for byte in iter {
            self.push(byte);
        }
    }
}

impl<'a> Extend<&'a u8> for Sds {
    fn extend<T: IntoIterator<Item = &'a u8>>(
        &mut self,
        iter: T,
    ) {
        for &byte in iter {
            self.push(byte);
        }
    }
}

impl Serialize for Sds {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_slice())
    }
}

impl<'de> Deserialize<'de> for Sds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        Ok(Sds::from_vec(bytes))
    }
}

impl std::str::FromStr for Sds {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Sds::from_str(s))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        hash::DefaultHasher,
    };

    use super::*;

    fn inline_max() -> String {
        "x".repeat(Sds::INLINE_CAP)
    }

    fn heap_min() -> String {
        "x".repeat(Sds::INLINE_CAP + 1)
    }

    #[test]
    fn test_inline_creation_from_str() {
        let s = Sds::from_str("hello");

        assert_eq!(s.len(), 5);
        assert_eq!(s.as_slice(), b"hello");
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_heap_creation_from_str() {
        let long = "this is a long string exceeding the inline cap";
        let s = Sds::from_str(long);

        assert_eq!(s.len(), long.len());
        assert_eq!(s.as_slice(), long.as_bytes());
        assert!(!s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_push_within_inline() {
        let mut s = Sds::from_str("12345");

        s.push(b'6');
        assert_eq!(s.as_slice(), b"123456");
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_push_exceeding_inline() {
        let mut s = Sds::from_str("a".repeat(Sds::INLINE_CAP).as_str());

        s.push(b'x');

        assert!(!s.is_inline());
        assert_eq!(s.len(), Sds::INLINE_CAP + 1);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_append_within_inline() {
        let mut s = Sds::from_str("123");

        s.append(b"456");

        assert_eq!(s.as_slice(), b"123456");
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_append_exceeding_inline() {
        let mut s = Sds::from_str("hello");

        s.append(b" world this is too long for sure");

        assert!(!s.is_inline());
        assert_eq!(s.as_str().unwrap(), "hello world this is too long for sure");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_clear() {
        let mut s = Sds::from_str("hello");

        s.clear();

        assert_eq!(s.len(), 0);
        assert!(s.is_empty());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_clear_heap_preserves_capacity() {
        let long = "a".repeat(Sds::INLINE_CAP + 10);
        let mut s = Sds::from_str(&long);
        let cap_before = s.capacity();

        s.clear();

        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert_eq!(s.capacity(), cap_before);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_truncate() {
        let mut s = Sds::from_str("hello world");

        s.truncate(5);

        assert_eq!(s.as_str().unwrap(), "hello");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_truncate_to_inline() {
        let mut s = Sds::from_str("a very very long string indeed");

        assert!(!s.is_inline());

        s.truncate(5);

        assert!(s.is_inline());
        assert_eq!(s.as_str().unwrap(), "a ver");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_truncate_noop_when_new_len_ge_current() {
        let mut s = Sds::from_str("hello");

        s.truncate(100);

        assert_eq!(s.as_str().unwrap(), "hello");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_slice_range() {
        let s = Sds::from_str("abcdefg");

        let sliced = s.slice_range(2, 5);

        assert_eq!(sliced.as_slice(), b"cde");

        sliced.debug_assert_invariants();
    }

    #[test]
    fn test_check_slice_range() {
        let s = Sds::from_str("Hello, world!");

        assert_eq!(s.slice_range(0, 5).as_str().unwrap(), "Hello");
        assert_eq!(s.slice_range(7, 12).as_str().unwrap(), "world");
    }

    #[test]
    fn test_display_valid_utf8() {
        let s = Sds::from_str("test");

        assert_eq!(format!("{s}"), "test");
    }

    #[test]
    fn test_equality_and_ordering() {
        let a = Sds::from_str("abc");
        let b = Sds::from_str("abc");
        let c = Sds::from_str("def");

        assert_eq!(a, b);
        assert!(a < c);
    }

    #[test]
    fn test_try_from_valid_utf8() {
        let s = Sds::from_str("hello");
        let string: String = s.try_into().unwrap();

        assert_eq!(string, "hello");
    }

    #[test]
    fn test_hashing_consistency() {
        let a = Sds::from_str("foo");
        let b = Sds::from_str("foo");

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        a.hash(&mut hasher1);
        b.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_invalid_utf8() {
        let s = Sds::from_vec(vec![0x80, 0x80, 0x80]);

        assert!(s.as_str().is_err());
        s.debug_assert_invariants();
    }

    #[test]
    fn test_reserve() {
        let mut s = Sds::from_str("Hello");

        s.reserve(10);

        assert!(s.capacity() >= 15);
        assert_eq!(s.len(), 5);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_deref() {
        let s = Sds::from_str("Hello, world!");
        let slice: &[u8] = &s;

        assert_eq!(slice, b"Hello, world!");
    }

    #[test]
    fn test_deref_mut() {
        let mut s = Sds::from_str("Hello");
        let slice: &mut [u8] = &mut s;

        slice[0] = b'J';

        assert_eq!(s.as_str().unwrap(), "Jello");
    }

    #[test]
    fn test_push_integrity() {
        let mut s = Sds::from_str("Rust");

        s.push(b'!');

        assert_eq!(s.as_str().unwrap(), "Rust!");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_empty_string_is_inline() {
        let s = Sds::default();

        assert!(s.is_inline());
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_exact_inline_cap_stays_inline() {
        let data = "x".repeat(Sds::INLINE_CAP);
        let s = Sds::from_str(&data);

        assert!(
            s.is_inline(),
            "A string of length INLINE_CAP should be inline"
        );
        assert_eq!(s.len(), Sds::INLINE_CAP);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_one_over_inline_cap_goes_heap() {
        let data = "x".repeat(Sds::INLINE_CAP + 1);
        let s = Sds::from_str(&data);

        assert!(
            !s.is_inline(),
            "A string of length INLINE_CAP + 1 should be heap-allocated"
        );
        assert_eq!(s.len(), Sds::INLINE_CAP + 1);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_push_at_inline_boundary_promotes_to_heap() {
        let mut s = Sds::from_str(&"a".repeat(Sds::INLINE_CAP));

        assert!(s.is_inline());

        s.push(b'b');

        assert!(
            !s.is_inline(),
            "Pushing onto a full inline buffer should switch Sds to heap"
        );
        assert_eq!(s.len(), Sds::INLINE_CAP + 1);
        assert_eq!(s.as_slice()[Sds::INLINE_CAP], b'b');

        s.debug_assert_invariants();
    }

    #[test]
    fn test_append_empty_is_noop() {
        let mut s = Sds::from_str("foo");
        let len_before = s.len();
        let repr_inline_before = s.is_inline();

        s.append(b"");

        assert_eq!(s.len(), len_before);
        assert_eq!(s.is_inline(), repr_inline_before);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_truncate_to_zero() {
        let mut s = Sds::from_str("hello");

        s.truncate(0);

        assert_eq!(s.len(), 0);
        assert!(s.is_empty());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_heap_truncate_to_inline_cap_exact() {
        let long = "a".repeat(Sds::INLINE_CAP + 5);
        let mut s = Sds::from_str(&long);

        assert!(!s.is_inline());

        s.truncate(Sds::INLINE_CAP);

        assert!(
            s.is_inline(),
            "After truncating to INLINE_CAP, Sds should be inline"
        );
        assert_eq!(s.len(), Sds::INLINE_CAP);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_slice_range_empty() {
        let s = Sds::from_str("hello");
        let empty = s.slice_range(0, 0);

        assert!(empty.is_empty());
        assert!(empty.is_inline());

        empty.debug_assert_invariants();
    }

    #[test]
    fn test_slice_range_full() {
        let s = Sds::from_str("hello");
        let full = s.slice_range(0, s.len());

        assert_eq!(full.as_slice(), b"hello");

        full.debug_assert_invariants();
    }

    #[test]
    fn test_from_vec_empty() {
        let s = Sds::from_vec(vec![]);

        assert!(s.is_inline());
        assert_eq!(s.len(), 0);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_vec_exact_inline_cap() {
        let vec: Vec<u8> = (0u8..Sds::INLINE_CAP as u8).collect();
        let s = Sds::from_vec(vec.clone());

        assert!(s.is_inline());
        assert_eq!(s.as_slice(), vec.as_slice());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_string_long() {
        let original = "a".repeat(Sds::INLINE_CAP + 1);
        let s: Sds = original.clone().into();

        assert!(!s.is_inline());
        assert_eq!(s.as_str().unwrap(), original.as_str());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_string_short() {
        let s: Sds = String::from("hi").into();

        assert!(s.is_inline());
        assert_eq!(s.as_str().unwrap(), "hi");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_into_vec_inline() {
        let s = Sds::from_str("hello");
        let v: Vec<u8> = s.into();

        assert_eq!(v, b"hello");
    }

    #[test]
    fn test_into_vec_heap() {
        let data = "a".repeat(Sds::INLINE_CAP + 5);
        let s = Sds::from_str(&data);
        let v: Vec<u8> = s.into();

        assert_eq!(v.len(), Sds::INLINE_CAP + 5);
    }

    #[test]
    fn test_many_pushes_invariants() {
        let mut s = Sds::default();

        for i in 0u8..=200 {
            s.push(i);
            s.debug_assert_invariants();
        }

        assert_eq!(s.len(), 201);
        assert_eq!(s.as_slice()[0], 0u8);
        assert_eq!(s.as_slice()[200], 200u8);
    }

    #[test]
    fn test_clear_heap_stays_heap() {
        let long = "a".repeat(Sds::INLINE_CAP + 1);
        let mut s = Sds::from_str(&long);

        assert!(!s.is_inline());

        s.clear();

        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert!(s.capacity() > 0);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_inline_cap_value() {
        let expected = std::mem::size_of::<usize>() * 3 - 1;

        assert_eq!(
            Sds::INLINE_CAP,
            expected,
            "INLINE_CAP must be equal to size_of::<usize>() * 3 - 1 = {expected}"
        );
    }

    #[test]
    fn test_with_capacity_zero_is_empty_inline() {
        let s = Sds::with_capacity(0);

        assert!(s.is_inline());
        assert!(s.is_empty());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_with_capacity_inline_range_stays_inline() {
        let s = Sds::with_capacity(Sds::INLINE_CAP);

        assert!(s.is_inline());
        assert!(s.is_empty());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_with_capacity_over_inline_allocates_heap() {
        let s = Sds::with_capacity(Sds::INLINE_CAP + 1);

        assert!(!s.is_inline(), "cap > INLINE_CAP should yield heap");
        assert!(s.is_empty());
        assert!(s.capacity() > Sds::INLINE_CAP);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_with_capacity_large() {
        let s = Sds::with_capacity(1024);

        assert!(!s.is_inline());
        assert!(s.capacity() >= 1024);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_with_capacity_no_realloc_within_limit() {
        // Если данные вписываются в зарезервированный capacity, append не должен
        // вызывать реаллокацию.
        let cap = 64;
        let mut s = Sds::with_capacity(cap);
        let cap_initial = s.capacity();

        s.append(&vec![b'a'; cap / 2]);

        assert_eq!(s.capacity(), cap_initial);

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_zero_is_empty_inline() {
        let s = Sds::repeat(b'x', 0);

        assert!(s.is_empty());
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_one_byte() {
        let s = Sds::repeat(b'Z', 1);

        assert_eq!(s.as_slice(), b"Z");
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_inline_cap() {
        let s = Sds::repeat(b'0', Sds::INLINE_CAP);

        assert!(s.is_inline());
        assert_eq!(s.len(), Sds::INLINE_CAP);
        assert!(s.as_slice().iter().all(|&b| b == b'0'));

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_over_inline_cap_is_heap() {
        let n = Sds::INLINE_CAP + 1;
        let s = Sds::repeat(b'1', n);

        assert!(!s.is_inline());
        assert_eq!(s.len(), n);
        assert!(s.as_slice().iter().all(|&b| b == b'1'));

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_large() {
        let s = Sds::repeat(b'\xff', 1000);

        assert_eq!(s.len(), 1000);
        assert!(s.as_slice().iter().all(|&b| b == 0xff));

        s.debug_assert_invariants();
    }

    #[test]
    fn test_repeat_null_byte() {
        let s = Sds::repeat(b'\0', 5);

        assert_eq!(s.as_slice(), b"\0\0\0\0\0");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_valid_ascii_unchanged() {
        let s = Sds::from_utf8_lossy(b"hello world");

        assert_eq!(s.as_slice(), b"hello world");
        assert!(s.as_str().is_ok());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_empty() {
        let s = Sds::from_utf8_lossy(b"");

        assert!(s.is_empty());
        assert!(s.is_inline());

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_single_invalid_byte_becomes_replacement() {
        let s = Sds::from_utf8_lossy(b"\xff");
        let text = s.as_str().expect("must be valid UTF-8");

        assert_eq!(text, "\u{FFFD}");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_mixed_replaces_only_invalid() {
        let s = Sds::from_utf8_lossy(b"ok\xff\xfe!");
        let text = s.as_str().expect("must be valid UTF-8");

        assert!(text.starts_with("ok"));
        assert!(text.ends_with('!'));
        assert!(text.contains('\u{FFFD}'));

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_all_invalid() {
        let s = Sds::from_utf8_lossy(&[0x80u8; 3]);
        let text = s.as_str().expect("must be valid UTF-8");

        assert!(text.chars().all(|c| c == '\u{FFFD}'));

        s.debug_assert_invariants();
    }

    #[test]
    fn test_from_utf8_lossy_valid_multibyte_unchanged() {
        let input = "Hello, world!".as_bytes();
        let s = Sds::from_utf8_lossy(input);

        assert_eq!(s.as_str().unwrap(), "Hello, world!");

        s.debug_assert_invariants();
    }

    #[test]
    fn test_as_ref_equals_as_slice() {
        let s = Sds::from_str("hello");
        let via_as_ref: &[u8] = s.as_ref();

        assert_eq!(via_as_ref, s.as_slice());
    }

    #[test]
    fn test_as_ref_works_in_generic_function() {
        fn byte_len(b: impl AsRef<[u8]>) -> usize {
            b.as_ref().len()
        }

        assert_eq!(byte_len(Sds::from_str("hi")), 2);
        assert_eq!(byte_len(Sds::from_str(&heap_min())), Sds::INLINE_CAP + 1);
    }

    #[test]
    fn test_as_ref_usable_for_binary_search() {
        let s = Sds::from_bytes(b"abcde");
        let r: &[u8] = s.as_ref();

        assert_eq!(r.binary_search(&b'c'), Ok(2));
    }

    #[test]
    fn test_borrow_enables_hashmap_lookup_by_slice() {
        let mut map: HashMap<Sds, u32> = HashMap::new();

        map.insert("alpha".into(), 1);
        map.insert("beta".into(), 2);

        // Поиск по &[u8] - без создания Sds
        assert_eq!(map.get(b"alpha".as_ref()), Some(&1));
        assert_eq!(map.get(b"beta".as_ref()), Some(&2));
        assert_eq!(map.get(b"gamma".as_ref()), None);
    }

    #[test]
    fn borrow_enables_hashset_contains_by_slice() {
        let mut set: HashSet<Sds> = HashSet::new();

        set.insert("one".into());
        set.insert("two".into());

        assert!(set.contains(b"one".as_ref()));
        assert!(!set.contains(b"three".as_ref()));
    }

    #[test]
    fn test_borrow_hash_equals_slice_hash() {
        // Контракт Borrow: hash(Sds) == hash(&[u8]) для одних данных.
        let s = Sds::from_str("test");
        let mut h1 = DefaultHasher::new();

        s.hash(&mut h1);

        let mut h2 = DefaultHasher::new();
        let borrowed: &[u8] = s.borrow();

        borrowed.hash(&mut h2);

        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn test_borrow_eq_consistent_with_slice() {
        let s = Sds::from_str("hello");
        let borrowed: &[u8] = s.borrow();

        assert_eq!(borrowed, b"hello");
    }

    #[test]
    fn test_borrow_works_for_heap_string() {
        let mut map: HashMap<Sds, &str> = HashMap::new();

        map.insert(heap_min().into(), "large");

        let key = "x".repeat(Sds::INLINE_CAP + 1);

        assert_eq!(map.get(key.as_bytes()), Some(&"large"));
    }

    #[test]
    fn test_roundtrip_binary_data_preserved() {
        // Всего 256 байт значений должны пройти без изменений.
        let orig: Vec<u8> = (0u8..=255).collect();
        let s = Sds::from_bytes(&orig);
        let back: Vec<u8> = s.into();

        assert_eq!(orig, back);
    }

    #[test]
    fn test_roundtrip_at_online_heap_boundary() {
        // Строка длиной ровно INLINE_CAP - должна быть inline после roundtrip.
        let orig = "y".repeat(Sds::INLINE_CAP);
        let s: Sds = orig.as_str().into();

        assert!(s.is_inline());

        let back: Vec<u8> = s.into();

        assert_eq!(back, orig.as_bytes());
    }

    #[test]
    fn test_extend_promotes_inline_to_heap() {
        let mut s = Sds::from_str(&inline_max());

        assert!(s.is_inline());

        s.extend(b"XYZ".iter().copied());

        assert!(!s.is_inline());
        assert_eq!(s.len(), Sds::INLINE_CAP + 3);

        s.debug_assert_invariants();
    }
}
