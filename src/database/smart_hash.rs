//! SmartHash is an adaptive hash-like structure optimized for small and
//! medium-sized collections.
//!
//! It uses two internal representations:
//! - `Zip`: a compact `Vec<(ArcBytes, ArcBytes)>` for small datasets,
//! - `Map`: a `HashMap<ArcBytes, ArcBytes>` for large datasets.
//!
//! The structure automatically switches between these representations
//! based on the number of elements for performance and memory efficiency.

use std::{
    collections::{hash_map, HashMap},
    slice,
};

use serde::{Deserialize, Serialize};

use super::ArcBytes;

/// The threshold at which `SmartHash` switches from `Zip` to `Map`.
const THRESHOLD: usize = 32;

/// An adaptive key-value structure with automatic representation switching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartHash {
    /// Compact representation using a Vec of key-value pairs.
    Zip(Vec<(ArcBytes, ArcBytes)>),
    /// HashMap representation for faster access on large datasets.
    Map(HashMap<ArcBytes, ArcBytes>),
}

impl SmartHash {
    /// Creates a new empty `SmartHash` using `Zip` representation.
    pub fn new() -> Self {
        SmartHash::Zip(Vec::new())
    }
    /// Returns the number of elements in the structure.
    pub fn len(&self) -> usize {
        match self {
            SmartHash::Zip(v) => v.len(),
            SmartHash::Map(v) => v.len(),
        }
    }
    /// Returns `true` if the structure contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Inserts or updates the given `key` with `value`.
    ///
    /// Automatically switches to `Map` if element count exceeds threshold.
    pub fn hset(&mut self, key: ArcBytes, value: ArcBytes) {
        match self {
            SmartHash::Zip(vec) => {
                if let Some((_, v)) = vec.iter_mut().find(|(k, _)| k == &key) {
                    *v = value;
                    return;
                }
                vec.push((key, value));
                if vec.len() > THRESHOLD {
                    let mut map = HashMap::with_capacity(vec.len());
                    for (k, v) in vec.drain(..) {
                        map.insert(k, v);
                    }
                    *self = SmartHash::Map(map);
                }
            }
            SmartHash::Map(map) => {
                map.insert(key, value);
                if map.len() < THRESHOLD / 2 {
                    let mut vec = Vec::with_capacity(map.len());
                    for (k, v) in map.drain() {
                        vec.push((k, v));
                    }
                    *self = SmartHash::Zip(vec);
                }
            }
        }
    }
    /// Returns a reference to the value corresponding to the key.
    pub fn hget(&self, key: &ArcBytes) -> Option<&ArcBytes> {
        match self {
            SmartHash::Zip(vec) => vec.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            SmartHash::Map(map) => map.get(key),
        }
    }
    /// Removes the value corresponding to the key.
    ///
    /// Returns `true` if the key was present and removed.
    /// May downgrade to `Zip` if size falls below half of the threshold.
    pub fn hdel(&mut self, key: &ArcBytes) -> bool {
        let removed = match self {
            SmartHash::Zip(vec) => {
                if let Some(pos) = vec.iter().position(|(k, _)| k == key) {
                    vec.remove(pos);
                    true
                } else {
                    false
                }
            }
            SmartHash::Map(map) => map.remove(key).is_some(),
        };
        if removed {
            if let SmartHash::Map(map) = self {
                if map.len() < THRESHOLD / 2 {
                    let mut vec = Vec::with_capacity(map.len());
                    for (k, v) in map.drain() {
                        vec.push((k, v));
                    }
                    *self = SmartHash::Zip(vec);
                }
            }
        }
        removed
    }
    /// Returns an iterator over the key-value pairs.
    pub fn iter(&self) -> SmartHashIter<'_> {
        match self {
            SmartHash::Zip(vec) => SmartHashIter::Zip(vec.iter()),
            SmartHash::Map(map) => SmartHashIter::Map(map.iter()),
        }
    }
}

impl Default for SmartHash {
    fn default() -> Self {
        SmartHash::new()
    }
}

impl FromIterator<(ArcBytes, ArcBytes)> for SmartHash {
    fn from_iter<I: IntoIterator<Item = (ArcBytes, ArcBytes)>>(iter: I) -> Self {
        let mut sh = SmartHash::new();
        for (k, v) in iter {
            sh.hset(k, v);
        }
        sh
    }
}

impl Extend<(ArcBytes, ArcBytes)> for SmartHash {
    fn extend<I: IntoIterator<Item = (ArcBytes, ArcBytes)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.hset(k, v);
        }
    }
}

/// Iterator over the elements of a `SmartHash`.
pub enum SmartHashIter<'a> {
    /// Iterator over compact `Zip` representation.
    Zip(slice::Iter<'a, (ArcBytes, ArcBytes)>),
    /// Iterator over `Map` representation.
    Map(hash_map::Iter<'a, ArcBytes, ArcBytes>),
}

impl<'a> Iterator for SmartHashIter<'a> {
    type Item = (&'a ArcBytes, &'a ArcBytes);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SmartHashIter::Zip(iter) => iter.next().map(|(k, v)| (k, v)),
            SmartHashIter::Map(iter) => iter.next(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that a value inserted with `hset` can be retrieved using `hget`.
    #[test]
    fn test_hset_hget() {
        let key = ArcBytes::from_str("key1");
        let value = ArcBytes::from_str("value1");

        let mut smart_hash = SmartHash::new();
        smart_hash.hset(key.clone(), value.clone());
        assert_eq!(smart_hash.hget(&key), Some(&value));
    }

    /// Tests that a key can be deleted using `hdel` and becomes inaccessible.
    #[test]
    fn test_hdel() {
        let key = ArcBytes::from_str("key1");
        let value = ArcBytes::from_str("value1");

        let mut smart_hash = SmartHash::new();
        smart_hash.hset(key.clone(), value.clone());
        let removed = smart_hash.hdel(&key);
        assert!(removed);
        assert!(smart_hash.hget(&key).is_none());
    }

    /// Tests that the internal representation automatically switches to Map
    /// after inserting more entries than the threshold.
    #[test]
    fn test_auto_convert_to_map() {
        let mut smart_hash = SmartHash::new();
        // Insert more elements than the THRESHOLD threshold to trigger the conversion.
        for i in 0..(THRESHOLD + 1) {
            let key = ArcBytes::from_str(&format!("key{}", i));
            let value = ArcBytes::from_str(&format!("value{}", i));
            smart_hash.hset(key, value);
        }

        // Check that the internal representation is now a Map.
        match smart_hash {
            SmartHash::Map(_) => {}
            _ => panic!(
                "Ожидалось, что внутреннее представление будет Map после превышения THRESHOLD"
            ),
        }
    }

    /// Tests iteration over entries returns all key-value pairs.
    #[test]
    fn test_iter() {
        let mut smart_hash = SmartHash::new();
        let pairs = vec![
            (ArcBytes::from_str("a"), ArcBytes::from_str("1")),
            (ArcBytes::from_str("b"), ArcBytes::from_str("2")),
        ];
        for (k, v) in pairs.clone() {
            smart_hash.hset(k, v);
        }
        let collected: Vec<(&ArcBytes, &ArcBytes)> = smart_hash.iter().collect();
        // Check that both elements are present (order is not guaranteed)
        assert_eq!(collected.len(), 2);
    }

    /// Tests `len` and `is_empty` behave as expected during insertions.
    #[test]
    fn test_len_and_empty() {
        let mut sh = SmartHash::new();
        assert!(sh.is_empty());
        assert_eq!(sh.len(), 0);
        sh.hset(ArcBytes::from_str("a"), ArcBytes::from_str("1"));
        assert!(!sh.is_empty());
        assert_eq!(sh.len(), 1);
    }

    /// Tests `hset`, `hget`, `hdel` and downgrade from Map to Zip if size drops below threshold.
    #[test]
    fn test_hset_hget_hdel_and_downgrade() {
        let mut sh = SmartHash::new();
        // Add THRESHOLD+1 to go to Map
        for i in 0..(THRESHOLD + 1) {
            let k = ArcBytes::from_str(&format!("k{i}"));
            let v = ArcBytes::from_str(&format!("v{i}"));
            sh.hset(k.clone(), v.clone());
            assert_eq!(sh.hget(&k), Some(&v));
        }
        // We made sure that inside Map
        matches!(sh, SmartHash::Map(_));

        // Remove all but one so that map.len() == 1 < THRESHOLD/2
        for i in 0..THRESHOLD {
            let k = ArcBytes::from_str(&format!("k{i}"));
            assert!(sh.hdel(&k));
        }
        // Should go back to Zip
        matches!(sh, SmartHash::Zip(_));
        assert_eq!(sh.len(), 1);
    }

    /// Tests that iteration order does not affect correctness by sorting entries before comparing.
    #[test]
    fn test_iter_order_independent() {
        let mut sh = SmartHash::new();
        let pairs = vec![
            (ArcBytes::from_str("x"), ArcBytes::from_str("10")),
            (ArcBytes::from_str("y"), ArcBytes::from_str("20")),
            (ArcBytes::from_str("z"), ArcBytes::from_str("30")),
        ];
        // Using Extend
        sh.extend(pairs.clone());
        let mut got: Vec<_> = sh.iter().collect();
        // Sort by key for stability
        got.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut expected = pairs.clone();
        expected.sort_by(|(a, _), (b, _)| a.cmp(b));
        let expected_refs: Vec<_> = expected.iter().map(|(k, v)| (k, v)).collect();
        assert_eq!(got, expected_refs);
    }

    /// Tests that `FromIterator` correctly builds a SmartHash with all entries accessible.
    #[test]
    fn test_from_iterator() {
        let pairs = vec![
            (ArcBytes::from_str("foo"), ArcBytes::from_str("bar")),
            (ArcBytes::from_str("baz"), ArcBytes::from_str("qux")),
        ];
        let sh: SmartHash = pairs.clone().into_iter().collect();
        assert_eq!(sh.len(), 2);
        for (k, v) in pairs {
            assert_eq!(sh.hget(&k), Some(&v));
        }
    }
}
