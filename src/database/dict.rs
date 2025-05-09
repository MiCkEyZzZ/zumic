//! Hash table (Dict) with incremental rehashing.
//!
//! Implementation of a dictionary (associative array) based on a
//! chained hash table with two tables and smooth
//! rehashing without pauses.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

const INITIAL_SIZE: usize = 4;
const REHASH_BATCH: usize = 1;

/// One element in a collision chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
struct Entry<K, V> {
    key: K,
    val: V,
    next: Option<Box<Entry<K, V>>>,
}

/// One hash table: vector of buckets, size mask, and number of used elements.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
struct HashTable<K, V> {
    buckets: Vec<Option<Box<Entry<K, V>>>>,
    size_mask: usize,
    used: usize,
}

/// Main dictionary with two hash tables for rehashing.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Dict<K, V> {
    ht: [HashTable<K, V>; 2],
    rehash_idx: isize, // -1 = no rehashing, otherwise index in ht[0]
}

pub struct DictIter<'a, K, V> {
    tables: [&'a HashTable<K, V>; 2],
    table_idx: usize,
    bucket_idx: usize,
    current_entry: Option<&'a Entry<K, V>>,
}

impl<K, V> Entry<K, V> {
    fn new(key: K, val: V, next: Option<Box<Entry<K, V>>>) -> Box<Self> {
        Box::new(Entry { key, val, next })
    }
}

impl<K, V> HashTable<K, V> {
    /// Creates a table with capacity `cap` (rounded up to the next power of two, at least INITIAL_SIZE).
    fn with_capacity(cap: usize) -> Self {
        let sz = cap.next_power_of_two().max(INITIAL_SIZE);
        let mut buckets = Vec::with_capacity(sz);
        buckets.resize_with(sz, || None);

        HashTable {
            buckets,
            size_mask: sz - 1,
            used: 0,
        }
    }

    /// Resets the table to an empty state.
    fn clear(&mut self) {
        self.buckets.clear();
        self.size_mask = 0;
        self.used = 0;
    }
}

impl<K, V> Default for Dict<K, V>
where
    K: Hash + Eq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Dict<K, V>
where
    K: Hash + Eq,
{
    /// Creates a new empty dictionary.
    pub fn new() -> Self {
        Dict {
            ht: [HashTable::with_capacity(0), HashTable::with_capacity(0)],
            rehash_idx: -1,
        }
    }

    /// Inserts (key, val). If key exists — updates it and returns false.
    pub fn insert(&mut self, key: K, val: V) -> bool {
        self.expand_if_needed();
        self.rehash_step();

        let table_idx = if self.is_rehashing() { 1 } else { 0 };
        let mask = self.ht[table_idx].size_mask;
        let slot = (Self::hash_key(&key) as usize) & mask;

        // check if key already exists
        {
            let mut cur = &mut self.ht[table_idx].buckets[slot];
            while let Some(ref mut e) = cur {
                if e.key == key {
                    e.val = val;
                    return false;
                }
                cur = &mut e.next;
            }
        }

        // insert new entry at the head of the chain
        let next = self.ht[table_idx].buckets[slot].take();
        let new_entry = Entry::new(key, val, next);
        self.ht[table_idx].buckets[slot] = Some(new_entry);
        self.ht[table_idx].used += 1;
        true
    }

    /// Returns `&V` for the given key or None.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.is_rehashing() {
            self.rehash_step();
        }
        for table_idx in 0..=1 {
            if self.ht[table_idx].size_mask == 0 {
                continue;
            }
            let slot = (Self::hash_key(key) as usize) & self.ht[table_idx].size_mask;
            let mut cur = &self.ht[table_idx].buckets[slot];
            while let Some(ref e) = cur {
                if &e.key == key {
                    return Some(&e.val);
                }
                cur = &e.next;
            }
            if !self.is_rehashing() {
                break;
            }
        }
        None
    }

    /// Removes a key. Returns true if the key was removed.
    pub fn remove(&mut self, key: &K) -> bool {
        // perform a rehash step if needed
        if self.is_rehashing() {
            self.rehash_step();
        }

        // try each table
        for table_idx in 0..=1 {
            let table = &mut self.ht[table_idx];
            if table.size_mask == 0 {
                continue;
            }

            let slot = (Self::hash_key(key) as usize) & table.size_mask;

            // extract the whole chain
            let old_chain = std::mem::take(&mut table.buckets[slot]);
            // remove the element from it
            let (new_chain, removed) = Self::remove_from_chain(old_chain, key);
            // restore the chain (possibly without the removed node)
            table.buckets[slot] = new_chain;

            if removed {
                table.used -= 1;
                return true;
            }
            if !self.is_rehashing() {
                break;
            }
        }

        false
    }

    /// Returns total number of elements (across all tables).
    pub fn len(&self) -> usize {
        let mut total = self.ht[0].used;
        if self.is_rehashing() {
            total += self.ht[1].used;
        }
        total
    }

    /// Returns `true` if the dictionary has no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears the dictionary and resets rehashing.
    pub fn clear(&mut self) {
        self.ht[0].clear();
        self.ht[1].clear();
        self.rehash_idx = -1;
    }

    pub fn iter(&self) -> DictIter<K, V> {
        DictIter {
            tables: [&self.ht[0], &self.ht[1]],
            table_idx: 0,
            bucket_idx: 0,
            current_entry: None,
        }
    }

    /// Recursively processes a chain: removes the first node with the key `key`.
    /// Returns (new_chain, was_removed).
    fn remove_from_chain(
        chain: Option<Box<Entry<K, V>>>,
        key: &K,
    ) -> (Option<Box<Entry<K, V>>>, bool) {
        match chain {
            None => (None, false),
            Some(mut boxed) => {
                if &boxed.key == key {
                    // found — skip this node
                    (boxed.next.take(), true)
                } else {
                    // recurse on tail
                    let (next_chain, removed) = Self::remove_from_chain(boxed.next.take(), key);
                    boxed.next = next_chain;
                    (Some(boxed), removed)
                }
            }
        }
    }

    /// Returns true if rehashing is in progress.
    #[inline]
    fn is_rehashing(&self) -> bool {
        self.rehash_idx != -1
    }

    /// Hashes the key to u64.
    fn hash_key(key: &K) -> u64 {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }

    /// Performs one incremental rehash step.
    fn rehash_step(&mut self) {
        if !self.is_rehashing() {
            return;
        }
        for _ in 0..REHASH_BATCH {
            let idx = self.rehash_idx as usize;
            // if we've finished all buckets — complete migration
            if idx >= self.ht[0].buckets.len() {
                self.ht[0] = std::mem::replace(&mut self.ht[1], HashTable::with_capacity(0));
                self.rehash_idx = -1;
                return;
            }
            // move the entire chain from ht[0].buckets[idx]
            let mut entry_opt = self.ht[0].buckets[idx].take();
            while let Some(mut e) = entry_opt {
                entry_opt = e.next.take();
                let h = (Self::hash_key(&e.key) as usize) & self.ht[1].size_mask;
                e.next = self.ht[1].buckets[h].take();
                self.ht[1].buckets[h] = Some(e);
                self.ht[0].used -= 1;
                self.ht[1].used += 1;
            }
            self.rehash_idx += 1;
        }
    }

    /// Starts or expands rehashing if the load factor exceeds 1.
    fn expand_if_needed(&mut self) {
        if self.is_rehashing() {
            return;
        }
        let used = self.ht[0].used;
        let size = self.ht[0].buckets.len();
        if used >= size {
            // начинаем реhash с новой таблицей вдвое больше
            self.ht[1] = HashTable::with_capacity(size * 2);
            self.rehash_idx = 0;
        }
    }
}

impl<'a, K, V> Iterator for DictIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.current_entry.take() {
                self.current_entry = entry.next.as_deref();
                return Some((&entry.key, &entry.val));
            }

            if self.bucket_idx >= self.tables[self.table_idx].buckets.len() {
                if self.table_idx == 0 && self.tables[1].size_mask != 0 {
                    self.table_idx = 1;
                    self.bucket_idx = 0;
                    continue;
                } else {
                    return None;
                }
            }

            self.current_entry = self.tables[self.table_idx].buckets[self.bucket_idx].as_deref();
            self.bucket_idx += 1;
        }
    }
}

impl<'a, K, V> IntoIterator for &'a Dict<K, V>
where
    K: Hash + Eq,
{
    type Item = (&'a K, &'a V);
    type IntoIter = DictIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies basic insertion and key-based value retrieval operations.
    #[test]
    fn basic_insert_get() {
        let mut d = Dict::new();
        assert!(d.insert("a", 1));
        assert!(d.insert("b", 2));
        assert_eq!(d.get(&"a"), Some(&1));
        assert_eq!(d.get(&"b"), Some(&2));
        assert_eq!(d.get(&"c"), None);
        assert!(!d.insert("a", 10));
        assert_eq!(d.get(&"a"), Some(&10));
    }

    /// Verifies value update when reinserting with the same key.
    #[test]
    fn insert_updates_existing_key() {
        let mut d = Dict::new();
        assert!(d.insert("key", 42));
        assert!(!d.insert("key", 100));
        assert_eq!(d.get(&"key"), Some(&100));
    }

    /// Verifies correct removal behavior: value is removed, and repeated removal returns false.
    #[test]
    fn removal() {
        let mut d = Dict::new();
        d.insert("x", 100);
        assert_eq!(d.get(&"x"), Some(&100));
        assert!(d.remove(&"x"));
        assert_eq!(d.get(&"x"), None);
        assert!(!d.remove(&"x"));
    }

    /// Verifies dictionary behavior with many insertions and subsequent access.
    #[test]
    fn rehash_behavior() {
        let mut d = Dict::new();
        for i in 0..100 {
            d.insert(i, i * 10);
        }
        for i in 0..100 {
            assert_eq!(d.get(&i), Some(&(i * 10)));
        }
        assert_eq!(d.len(), 100);
    }

    /// Verifies correct removal of keys during rehashing.
    #[test]
    fn rehash_with_removal() {
        let mut d = Dict::new();
        for i in 0..20 {
            d.insert(i, i);
        }

        for i in 0..10 {
            assert!(d.remove(&i));
        }

        for i in 0..10 {
            assert_eq!(d.get(&i), None);
        }

        for i in 10..20 {
            assert_eq!(d.get(&i), Some(&i));
        }
    }

    /// Verifies that the dictionary is properly cleared.
    #[test]
    fn clear_dict() {
        let mut d = Dict::new();
        d.insert("k", "v");
        d.clear();
        assert_eq!(d.len(), 0);
        assert_eq!(d.get(&"k"), None);
    }

    /// Verifies that the dictionary can be reused after being cleared.
    #[test]
    fn clear_and_reuse() {
        let mut d = Dict::new();
        d.insert("a", 1);
        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.insert("a", 2));
        assert_eq!(d.get(&"a"), Some(&2));
    }

    /// Verifies correct operation of the dictionary iterator.
    #[test]
    fn iteration_work() {
        let mut d = Dict::new();
        d.insert("x", 1);
        d.insert("y", 2);
        d.insert("z", 3);

        let mut seen = vec![];
        for (k, v) in d.iter() {
            seen.push((k, *v));
        }

        seen.sort();
        assert_eq!(seen, vec![(&"x", 1), (&"y", 2), (&"z", 3)]);
    }

    /// Verifies that the iterator over an empty dictionary yields no elements.
    #[test]
    fn empty_iterator() {
        let d: Dict<&str, i32> = Dict::new();
        let mut iter = d.iter();
        assert_eq!(iter.next(), None);
    }
}
