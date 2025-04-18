//! `ArcBytes` — это обёртка вокруг `Arc<Bytes>`, предназначенная для эффективного,
//! неизменяемого совместного использования данных байтов между потоками.
//!
//! Она предоставляет удобные методы для работы с срезами байтов (`[u8]`),
//! преобразования строк, нарезки, сериализации и операций сравнения.

use std::{
    cmp::Ordering,
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ops::Deref,
    str::{from_utf8, Utf8Error},
    sync::Arc,
};

use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize};

/// Объект-обёртка для неизменяемого буфера байтов с подсчётом ссылок.
///
/// `ArcBytes` инкапсулирует `Arc<Bytes>`, что позволяет эффективно клонировать и
/// разделять бинарные данные без лишнего копирования. Поддерживает удобные преобразования,
/// интерпретацию в UTF‑8 и базовые операции над срезами.
#[derive(Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct ArcBytes(Arc<Bytes>);

impl ArcBytes {
    /// Создаёт новый экземпляр `ArcBytes` из `Vec<u8>`.
    #[inline(always)]
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Self(Arc::new(Bytes::from(vec)))
    }

    /// Создаёт новый `ArcBytes` из строки (UTF‑8).
    pub fn from_str(s: &str) -> Self {
        Self(Arc::new(Bytes::copy_from_slice(s.as_bytes())))
    }

    /// Создаёт новый `ArcBytes` из статического (константного) среза байтов без копирования.
    pub fn from_static(slice: &'static [u8]) -> Self {
        Self(Arc::new(Bytes::from_static(slice)))
    }

    /// Возвращает длину среза байтов.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Клонирует срез байтов.
    #[inline(always)]
    pub fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    /// Возвращает `true`, если срез байтов пуст.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Возвращает срез сохранённых байтов.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    /// Возвращает срез сохранённых байтов.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_slice()
    }

    /// Преобразует сохранённые байты в `Vec<u8>`.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Пытается интерпретировать внутренние байты как строку UTF‑8.
    #[inline(always)]
    pub fn as_str(&self) -> Option<&str> {
        from_utf8(self.as_slice()).ok()
    }

    /// Преобразует данные в строку, возвращая ошибку, если данные не являются корректным UTF‑8.
    pub fn expect_utf8(&self) -> Result<&str, Utf8Error> {
        from_utf8(self.as_slice())
    }

    /// Возвращает изменяемую ссылку на внутренние данные, если владеете единственной копией.
    pub fn make_mut(&mut self) -> &mut Bytes {
        Arc::make_mut(&mut self.0)
    }

    /// Пытается извлечь внутренний `Bytes` без копирования, если `Arc` имеет единственнное владение.
    pub fn try_unwrap(self) -> Result<Bytes, Arc<Bytes>> {
        Arc::try_unwrap(self.0)
    }

    /// Проверяет, начинается ли хранимые данные с указанного префикса.
    pub fn starts_with(&self, prefix: &[u8]) -> bool {
        self.as_slice().starts_with(prefix)
    }

    /// Проверяет, заканчиваются ли хранимые данные указанным суффиксом.
    pub fn ends_with(&self, suffix: &[u8]) -> bool {
        self.as_slice().ends_with(suffix)
    }

    /// Возвращает новый `ArcBytes`, являющийся срезом исходного по заданному диапазону.
    pub fn slice(&self, range: impl std::ops::RangeBounds<usize>) -> Self {
        Self(Arc::new(self.0.slice(range)))
    }

    /// Возвращает внутренний `Arc<Bytes>` без копирования.
    #[inline(always)]
    pub fn into_inner(self) -> Arc<Bytes> {
        self.0
    }

    /// Возвращает внутренний `Bytes` без копирования, если `Arc` имеет единственное владение.
    #[inline(always)]
    pub fn into_bytes(self) -> Bytes {
        Arc::try_unwrap(self.0).unwrap_or_else(|arc| (*arc).clone())
    }
}

impl Serialize for ArcBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Сериализуем как массив байтов.
        serializer.serialize_bytes(self.as_slice())
    }
}

impl<'de> Deserialize<'de> for ArcBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Десериализуем в Vec<u8> и сразу создаем ArcBytes.
        let vec = <Vec<u8>>::deserialize(deserializer)?;
        Ok(ArcBytes(Arc::new(Bytes::from(vec))))
    }
}

impl Default for ArcBytes {
    fn default() -> Self {
        Self(Arc::new(Bytes::new()))
    }
}

impl Deref for ArcBytes {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl AsRef<[u8]> for ArcBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Display for ArcBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match from_utf8(self.as_slice()) {
            Ok(s) => write!(f, "{s}"),
            Err(_) => write!(f, "<invalid utf-8: {} bytes>", self.len()),
        }
    }
}

impl fmt::Debug for ArcBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"")?;
        for &b in self.as_slice() {
            match b {
                b'\n' => write!(f, "\\n")?,
                b'\r' => write!(f, "\\r")?,
                b'\t' => write!(f, "\\t")?,
                b'\"' => write!(f, "\\\"")?,
                b'\\' => write!(f, "\\\\")?,
                b if b.is_ascii_graphic() || b == b' ' => write!(f, "{}", b as char)?,
                _ => write!(f, "\\x{:02x}", b)?,
            }
        }
        write!(f, "\"")
    }
}

impl From<&str> for ArcBytes {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl From<Vec<u8>> for ArcBytes {
    fn from(vec: Vec<u8>) -> Self {
        Self::from_vec(vec)
    }
}

impl From<&[u8]> for ArcBytes {
    fn from(slice: &[u8]) -> Self {
        Self(Arc::new(Bytes::copy_from_slice(slice)))
    }
}

impl From<Bytes> for ArcBytes {
    fn from(bytes: Bytes) -> Self {
        Self(Arc::new(bytes))
    }
}

impl From<String> for ArcBytes {
    fn from(s: String) -> Self {
        Self::from_vec(s.into_bytes())
    }
}

impl Hash for ArcBytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl PartialOrd for ArcBytes {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ArcBytes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_slice().cmp(&other.as_slice())
    }
}

impl PartialEq<[u8]> for ArcBytes {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_slice() == other
    }
}

impl PartialEq<str> for ArcBytes {
    fn eq(&self, other: &str) -> bool {
        self.as_slice() == other.as_bytes()
    }
}

impl PartialEq<Vec<u8>> for ArcBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl PartialEq<Arc<[u8]>> for ArcBytes {
    fn eq(&self, other: &Arc<[u8]>) -> bool {
        self.as_slice() == other.as_ref()
    }
}

impl std::borrow::Borrow<[u8]> for ArcBytes {
    fn borrow(&self) -> &[u8] {
        self.as_slice()
    }
}

impl TryFrom<ArcBytes> for String {
    type Error = Utf8Error;
    fn try_from(value: ArcBytes) -> Result<Self, Self::Error> {
        Ok(std::str::from_utf8(&value)?.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::ArcBytes;

    /// Проверяет, что метод `from_str` корректно создаёт объект `ArcBytes` из строки,
    /// а метод `as_str` возвращает строку, совпадающую с исходным значением.
    #[test]
    fn test_from_str_and_as_str() {
        let s = "hello world";
        let ab = ArcBytes::from_str(s);
        assert_eq!(ab.as_str(), Some(s));
    }

    /// Проверяет, что метод `from_vec` создаёт объект `ArcBytes` из вектора байтов,
    /// а метод `to_vec` возвращает исходный вектор байтов.
    #[test]
    fn test_from_vec_and_to_vec() {
        let v = b"hello".to_vec();
        let ab = ArcBytes::from_vec(v.clone());
        assert_eq!(ab.to_vec(), v);
    }

    /// Проверяет корректность работы методов `len` и `is_empty`.
    /// Метод `len` должен возвращать правильную длину данных,
    /// а `is_empty` – корректно определять, пуст ли объект.
    #[test]
    fn test_len_and_is_empty() {
        let ab = ArcBytes::from_str("hello");
        assert_eq!(ab.len(), 5);
        assert!(!ab.is_empty());

        let empty = ArcBytes::from_str("");
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    /// Проверяет, что реализация `Display` корректно выводит строковое представление
    /// объекта `ArcBytes`, если данные являются валидными UTF-8.
    #[test]
    fn test_display_valid_utf8() {
        let ab = ArcBytes::from_str("test");
        assert_eq!(format!("{}", ab), "test");
    }

    /// Проверяет, что реализация `Display` корректно обрабатывает ситуацию,
    /// когда данные `ArcBytes` содержат невалидный UTF-8, выводя `<invalid utf-8>`.
    #[test]
    fn test_display_invalid_utf8() {
        let ab = ArcBytes::from_vec(vec![0xff, 0xfe, 0xfd]);
        assert_eq!(format!("{}", ab), "<invalid utf-8>");
    }

    /// Проверяет, что объект `ArcBytes` корректно сериализуется и десериализуется
    /// с помощью `serde`. После преобразования в JSON-строку и обратно,
    /// исходный и полученный объекты должны совпадать.
    #[test]
    fn test_serde_serialize_deserialize() {
        use serde_json;
        let ab = ArcBytes::from_str("serde test");
        let json = serde_json::to_string(&ab).unwrap();
        let deserialized: ArcBytes = serde_json::from_str(&json).unwrap();
        assert_eq!(ab, deserialized);
        assert_eq!(deserialized.as_str(), Some("serde test"));
    }

    /// Проверяет, что объект `ArcBytes` корректно реализует трейты `Deref`
    /// и может использоваться как срез байтов (`&[u8]`).
    #[test]
    fn test_deref_trait() {
        let ab = ArcBytes::from_str("abc");
        assert_eq!(&ab[..], b"abc");
    }

    /// Проверяет, что объект `ArcBytes` корректно реализует трейд `AsRef`
    /// и может быть преобразован в ссылку на срез байтов.
    #[test]
    fn test_as_ref_trait() {
        let ab = ArcBytes::from_str("abc");
        let r: &[u8] = ab.as_ref();
        assert_eq!(r, b"abc");
    }

    /// Проверяет корректность работы метода `slice` для объекта `ArcBytes`.
    #[test]
    fn test_slice_operations() {
        let data = ArcBytes::from(b"hello world".as_ref());
        let slice = data.slice(6..);
        assert_eq!(slice.as_slice(), b"world");
    }
}
