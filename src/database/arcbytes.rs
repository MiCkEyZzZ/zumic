use std::{
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ops::Deref,
    str::from_utf8,
    sync::Arc,
};

use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcBytes(Arc<Bytes>);

impl ArcBytes {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Self(Arc::new(Bytes::from(vec)))
    }
    pub fn from_str(s: &str) -> Self {
        Self(Arc::new(Bytes::copy_from_slice(s.as_bytes())))
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
    pub fn into_bytes(self) -> Bytes {
        Arc::try_unwrap(self.0).unwrap_or_else(|arc| arc.as_ref().clone())
    }
    pub fn into_arc(self) -> Arc<Bytes> {
        self.0
    }
    pub fn starts_with(&self, prefix: &[u8]) -> bool {
        self.as_slice().starts_with(prefix)
    }
    pub fn ends_with(&self, suffix: &[u8]) -> bool {
        self.as_slice().ends_with(suffix)
    }
    pub fn slice(&self, range: impl std::ops::RangeBounds<usize>) -> Self {
        let bytes = self.0.slice(range);
        Self(Arc::new(bytes))
    }
}

impl Serialize for ArcBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0[..])
    }
}

impl<'de> Deserialize<'de> for ArcBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        Ok(ArcBytes(Arc::new(Bytes::from(bytes))))
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
        &self.0[..]
    }
}

impl AsRef<[u8]> for ArcBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Display for ArcBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match from_utf8(&self.0) {
            Ok(s) => write!(f, "{}", s),
            Err(_) => write!(f, "<invalid utf-8>"),
        }
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
        self.0.hash(state);
    }
}

impl PartialOrd for ArcBytes {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for ArcBytes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
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

impl std::borrow::Borrow<[u8]> for ArcBytes {
    fn borrow(&self) -> &[u8] {
        self.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::ArcBytes;

    // Checks that the `from_str` method correctly creates an `ArcBytes`
    // object from a string and that the `as_str` method returns a string
    // that matches the original value.
    #[test]
    fn test_from_str_and_as_str() {
        let s = "hello world";
        let ab = ArcBytes::from_str(s);
        assert_eq!(ab.as_str(), Some(s));
    }

    // Checks that the `from_vec` method creates an `ArcBytes` object from
    // a vector of bytes, and that the `to_vec` method converts it back to
    // the original vector of bytes.
    #[test]
    fn test_from_vec_and_to_vec() {
        let v = b"hello".to_vec();
        let ab = ArcBytes::from_vec(v.clone());
        assert_eq!(ab.to_vec(), v);
    }

    // Checks that the `len` and `is_empty` methods work correctly. The `len`
    // method should return the correct data length, and `is_empty` should
    // correctly determine whether the object is empty.
    #[test]
    fn test_len_and_is_empty() {
        let ab = ArcBytes::from_str("hello");
        assert_eq!(ab.len(), 5);
        assert!(!ab.is_empty());

        let empty = ArcBytes::from_str("");
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    // Checks that the `Display` method correctly displays the string representation
    // of the `ArcBytes` object, if the data can be interpreted as valid UTF-8.
    #[test]
    fn test_display_valid_utf8() {
        let ab = ArcBytes::from_str("test");
        assert_eq!(format!("{}", ab), "test");
    }

    // Checks that the `Display` method correctly handles the situation when the
    // `ArcBytes` data contains invalid UTF-8, by outputing `<invalid utf-8>`.
    #[test]
    fn test_display_invalid_utf8() {
        let invalid = ArcBytes::from_vec(vec![0xff, 0xfe, 0xfd]);
        assert_eq!(format!("{}", invalid), "<invalid utf-8>");
    }

    // Checks whether the `ArcBytes` object is serialized and deserialized correctly
    // using `serde`. The method must convert the object to a JSON string and back,
    // and the original and deserialized objects must be equal.
    #[test]
    fn test_serde_serialize_deserialize() {
        use serde_json;

        let ab = ArcBytes::from_str("serde test");
        let json = serde_json::to_string(&ab).unwrap();
        let deserialized: ArcBytes = serde_json::from_str(&json).unwrap();

        assert_eq!(ab, deserialized);
        assert_eq!(deserialized.as_str(), Some("serde test"));
    }

    // Checks that the `ArcBytes` object correctly implements the `Deref` trait and can
    // be used as a byte slice (`&[u8]`).
    #[test]
    fn test_deref_trait() {
        let ab = ArcBytes::from_str("abc");
        assert_eq!(&ab[..], b"abc");
    }

    // Checks that the `ArcBytes` object correctly implements the `AsRes` trait and can be
    // converted to a byte slice reference.
    #[test]
    fn test_as_ref_trait() {
        let ab = ArcBytes::from_str("abc");
        let r: &[u8] = ab.as_ref();
        assert_eq!(r, b"abc");
    }

    // Checks that the `slice` method correctly operation on an `ArcBytes` object.
    #[test]
    fn test_slice_operations() {
        let data = ArcBytes::from(b"hello world".as_ref());
        let slice = data.slice(6..);
        assert_eq!(slice.as_slice(), b"world");
    }
}
