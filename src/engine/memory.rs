use std::sync::Arc;

use dashmap::DashMap;

use super::storage::Storage;
use crate::{
    database::{ArcBytes, Value},
    error::{StoreError, StoreResult},
};

pub struct InMemoryStore {
    pub data: Arc<DashMap<ArcBytes, Value>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for InMemoryStore {
    fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()> {
        self.data.insert(key, value);
        Ok(())
    }
    fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>> {
        Ok(self.data.get(&key).map(|entry| entry.clone()))
    }
    fn del(&self, key: ArcBytes) -> StoreResult<i64> {
        if self.data.remove(&key).is_some() {
            Ok(1)
        } else {
            Ok(0)
        }
    }
    fn mset(&mut self, entries: Vec<(ArcBytes, Value)>) -> StoreResult<()> {
        for (key, value) in entries {
            self.data.insert(key, value);
        }
        Ok(())
    }
    fn mget(&self, keys: &[ArcBytes]) -> StoreResult<Vec<Option<Value>>> {
        let result = keys
            .iter()
            .map(|key| self.data.get(key).map(|entry| entry.clone()))
            .collect();
        Ok(result)
    }
    fn rename(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<()> {
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to, value);
            Ok(())
        } else {
            Err(StoreError::KeyNotFound)
        }
    }
    fn renamenx(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<bool> {
        if self.data.contains_key(&to) {
            return Ok(false);
        }
        if let Some((_, value)) = self.data.remove(&from) {
            self.data.insert(to, value);
            Ok(true)
        } else {
            Err(StoreError::KeyNotFound)
        }
    }
    fn flushdb(&mut self) -> StoreResult<()> {
        self.data.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::types::Value;

    fn key(data: &str) -> ArcBytes {
        ArcBytes::from(data.as_bytes().to_vec())
    }

    /// Basic test to verify that a value can be set and then retrieved.
    #[test]
    fn test_set_and_get() {
        let mut store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(ArcBytes::from_str("world"));

        store.set(k.clone(), v.clone()).unwrap();
        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Ensures that setting a value twice for the same key overwrites the old one.
    #[test]
    fn test_overwrite_value() {
        let mut store = InMemoryStore::new();
        let k = key("overwrite");
        let v1 = Value::Str(ArcBytes::from_str("one"));
        let v2 = Value::Str(ArcBytes::from_str("two"));

        store.set(k.clone(), v1.clone()).unwrap();
        store.set(k.clone(), v2.clone()).unwrap();

        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, Some(v2));
    }

    /// Ensures that a key can be deleted and is no longer accessible.
    #[test]
    fn test_delete() {
        let mut store = InMemoryStore::new();
        let k = key("key_to_delete");
        let v = Value::Str(ArcBytes::from_str("some_value"));

        store.set(k.clone(), v).unwrap();
        store.del(k.clone()).unwrap();

        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, None);
    }

    /// Ensures that querying a non-existent key returns None.
    #[test]
    fn test_get_nonexistent_key() {
        let mut store = InMemoryStore::new();
        let got = store.get(key("missing")).unwrap();
        assert_eq!(got, None);
    }

    /// Ensures that deleting a non-existent key does not result in an error.
    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // deleting a non-existent key should not cause an error
        assert!(store.del(key("nope")).is_ok());
    }

    /// Tests bulk set and bulk get functionality.
    /// Verifies that existing and non-existing keys behave as expected.
    #[test]
    fn test_mset_and_mget() {
        let mut store = InMemoryStore::new();
        let entries = vec![
            (key("key1"), Value::Int(1)),
            (key("key2"), Value::Int(2)),
            (key("key3"), Value::Int(3)),
        ];
        store.mset(entries.clone()).unwrap();

        let keys = vec![key("key1"), key("key2"), key("key3"), key("missing")];
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

    /// Tests renaming an existing key to a new key.
    #[test]
    fn test_rename() {
        let mut store = InMemoryStore::new();
        store.set(key("old"), Value::Int(123)).unwrap();

        store.rename(key("old"), key("new")).unwrap();
        assert!(store.get(key("old")).unwrap().is_none());
        assert_eq!(store.get(key("new")).unwrap(), Some(Value::Int(123)));
    }

    /// Ensures that renaming a non-existent key fails with KeyNotFound.
    #[test]
    fn test_rename_nonexistent_key() {
        let mut store = InMemoryStore::new();
        let result = store.rename(key("does_not_exist"), key("whatever"));
        assert!(matches!(result, Err(StoreError::KeyNotFound)));
    }

    /// Tests renaming a key only if the target does not already exist (`renamenx`).
    #[test]
    fn test_renamenx_success() {
        let mut store = InMemoryStore::new();
        store
            .set(key("old"), Value::Str(ArcBytes::from_str("val")))
            .unwrap();

        let ok = store.renamenx(key("old"), key("new")).unwrap();
        assert!(ok);
        assert!(store.get(key("old")).unwrap().is_none());
        assert_eq!(
            store.get(key("new")).unwrap(),
            Some(Value::Str(ArcBytes::from_str("val")))
        );
    }

    /// Verifies that `renamenx` fails if the destination key already exists.
    #[test]
    fn test_renamenx_existing_target() {
        let mut store = InMemoryStore::new();
        store.set(key("old"), Value::Int(1)).unwrap();
        store.set(key("new"), Value::Int(2)).unwrap();

        let ok = store.renamenx(key("old"), key("new")).unwrap();
        assert!(!ok); // should return false
        assert_eq!(store.get(key("old")).unwrap(), Some(Value::Int(1)));
        assert_eq!(store.get(key("new")).unwrap(), Some(Value::Int(2)));
    }

    /// Ensures that `flushdb` removes all keys and values from the store.
    #[test]
    fn test_flushdb() {
        let mut store = InMemoryStore::new();
        store.set(key("one"), Value::Int(1)).unwrap();
        store.set(key("two"), Value::Int(2)).unwrap();

        store.flushdb().unwrap();

        assert!(store.get(key("one")).unwrap().is_none());
        assert!(store.get(key("two")).unwrap().is_none());
    }
}
