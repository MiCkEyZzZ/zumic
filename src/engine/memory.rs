use std::sync::Arc;

use dashmap::DashMap;

use crate::{Sds, Storage, StoreError, StoreResult, Value};

/// `InMemoryStore` - a thread-safe key-value store
/// using `DashMap` and `Arc`.
pub struct InMemoryStore {
    pub data: Arc<DashMap<Sds, Value>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }
}

impl Storage for InMemoryStore {
    fn set(&self, key: &Sds, value: Value) -> StoreResult<()> {
        self.data.insert(key.clone(), value);
        Ok(())
    }

    fn get(&self, key: &Sds) -> StoreResult<Option<Value>> {
        Ok(self.data.get(key).map(|entry| entry.value().clone()))
    }

    fn del(&self, key: &Sds) -> StoreResult<i64> {
        if self.data.remove(key).is_some() {
            Ok(1)
        } else {
            Ok(0)
        }
    }

    fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key.clone(), value);
        }
        Ok(())
    }

    fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>> {
        let result = keys
            .iter()
            .map(|key| self.data.get(key).map(|entry| entry.clone()))
            .collect();
        Ok(result)
    }

    fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()> {
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to.clone(), value);
            Ok(())
        } else {
            Err(StoreError::KeyNotFound)
        }
    }

    fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool> {
        if self.data.contains_key(&to) {
            return Ok(false);
        }
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to.clone(), value);
            Ok(true)
        } else {
            Err(StoreError::KeyNotFound)
        }
    }

    fn flushdb(&self) -> StoreResult<()> {
        self.data.clear();
        Ok(())
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Main test to check setting and getting a value.
    /// This test ensures that values can be correctly set and retrieved from the store.
    #[test]
    fn test_set_and_get() {
        let store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        store.set(&k, v.clone()).unwrap();
        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Checks that re-setting the value for the same key
    /// overwrites the old value.
    /// This test ensures that calling `set` on an existing key updates the value.
    #[test]
    fn test_overwrite_value() {
        let store = InMemoryStore::new();
        let k = key("overwrite");

        store.set(&k, Value::Str(Sds::from_str("one"))).unwrap();
        store.set(&k, Value::Str(Sds::from_str("two"))).unwrap();

        let got = store.get(&k).unwrap();
        assert_eq!(got, Some(Value::Str(Sds::from_str("two"))));
    }

    /// Checks that a key can be deleted, and after that, it is inaccessible.
    /// This test ensures that after calling `del` on a key, it is no longer retrievable.
    #[test]
    fn test_delete() {
        let store = InMemoryStore::new();
        let k = key("key_to_delete");
        let v = Value::Str(Sds::from_str("some_value"));

        store.set(&k, v).unwrap();
        store.del(&k).unwrap();

        let got = store.get(&k).unwrap();
        assert_eq!(got, None);
    }

    /// Checks that getting a value for a non-existent key returns None.
    /// This test ensures that attempting to get a key that doesn't exist results in `None`.
    #[test]
    fn test_get_nonexistent_key() {
        let store = InMemoryStore::new();
        let got = store.get(&key("missing")).unwrap();
        assert_eq!(got, None);
    }

    /// Checks that deleting a non-existent key does not result in an error.
    /// This test ensures that calling `del` on a non-existent key doesn't cause an error.
    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // Deleting a non-existent key should not cause an error.
        assert!(store.del(&key("nope")).is_ok());
    }

    /// Tests bulk setting and getting values.
    /// This test checks that multiple key-value pairs can be set and retrieved at once,
    /// and that missing keys return `None`.
    #[test]
    fn test_mset_and_mget() {
        let store = InMemoryStore::new();

        let k1 = key("key1");
        let k2 = key("key2");
        let k3 = key("key3");
        let kmissing = key("missing");

        let entries = vec![
            (&k1, Value::Int(1)),
            (&k2, Value::Int(2)),
            (&k3, Value::Int(3)),
        ];
        store.mset(entries).unwrap();

        let keys: Vec<&Sds> = vec![&k1, &k2, &k3, &kmissing];
        let result = store.mget(&keys).unwrap();

        assert_eq!(
            result,
            vec![
                Some(Value::Int(1)),
                Some(Value::Int(2)),
                Some(Value::Int(3)),
                None
            ]
        );
    }

    /// Checks that renaming an existing key works correctly.
    /// This test ensures that the key is moved to the new name and the old key is no longer accessible.
    #[test]
    fn test_rename() {
        let store = InMemoryStore::new();
        store.set(&key("old"), Value::Int(123)).unwrap();

        store.rename(&key("old"), &key("new")).unwrap();
        assert!(store.get(&key("old")).unwrap().is_none());
        assert_eq!(store.get(&key("new")).unwrap(), Some(Value::Int(123)));
    }

    /// Checks that attempting to rename a non-existent key results in an error
    /// with the KeyNotFound code.
    /// This test ensures that trying to rename a key that doesn't exist returns an error.
    #[test]
    fn test_rename_nonexistent_key() {
        let store = InMemoryStore::new();
        let result = store.rename(&key("does_not_exist"), &key("whatever"));
        assert!(matches!(result, Err(StoreError::KeyNotFound)));
    }

    /// Tests the renamenx method: renaming happens only
    /// if the target key does not exist.
    /// This test ensures that renaming a key only works if the target doesn't already exist.
    #[test]
    fn test_renamenx_success() {
        let store = InMemoryStore::new();
        store
            .set(&key("old"), Value::Str(Sds::from_str("val")))
            .unwrap();

        let ok = store.renamenx(&key("old"), &key("new")).unwrap();
        assert!(ok);
        assert!(store.get(&key("old")).unwrap().is_none());
        assert_eq!(
            store.get(&key("new")).unwrap(),
            Some(Value::Str(Sds::from_str("val")))
        );
    }

    /// Checks that renamenx does not proceed if the target key already exists.
    /// This test ensures that renaming a key fails if the target already exists.
    #[test]
    fn test_renamenx_existing_target() {
        let store = InMemoryStore::new();
        store.set(&key("old"), Value::Int(1)).unwrap();
        store.set(&key("new"), Value::Int(2)).unwrap();

        let ok = store.renamenx(&key("old"), &key("new")).unwrap();
        assert!(!ok); // Expects false since the target key already exists.
        assert_eq!(store.get(&key("old")).unwrap(), Some(Value::Int(1)));
        assert_eq!(store.get(&key("new")).unwrap(), Some(Value::Int(2)));
    }

    /// Checks that the flushdb method clears all keys and values from the store.
    /// This test ensures that calling `flushdb` removes all data from the store.
    #[test]
    fn test_flushdb() {
        let store = InMemoryStore::new();
        store.set(&key("one"), Value::Int(1)).unwrap();
        store.set(&key("two"), Value::Int(2)).unwrap();

        store.flushdb().unwrap();

        assert!(store.get(&key("one")).unwrap().is_none());
        assert!(store.get(&key("two")).unwrap().is_none());
    }

    /// Tests that an empty key is handled correctly.
    /// This test ensures that an empty key can be set and retrieved from the store.
    #[test]
    fn test_empty_key() {
        let store = InMemoryStore::new();
        let empty = key("");
        store.set(&empty, Value::Int(42)).unwrap();
        assert_eq!(store.get(&empty).unwrap(), Some(Value::Int(42)));
    }

    /// Tests handling of very long keys and values.
    /// This test ensures that the store can handle keys and values of arbitrary size.
    #[test]
    fn test_very_long_key_and_value() {
        let store = InMemoryStore::new();
        let long_key = key(&"k".repeat(10_000));
        let long_value = Value::Str(Sds::from("v".repeat(100_000).as_bytes()));

        store.set(&long_key, long_value.clone()).unwrap();
        assert_eq!(store.get(&long_key).unwrap(), Some(long_value));
    }
}
