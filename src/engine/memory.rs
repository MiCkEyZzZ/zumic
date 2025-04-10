use std::sync::Arc;

use dashmap::DashMap;

use super::storage::Storage;
use crate::{
    database::{types::Value, ArcBytes},
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
    fn keys(&self, pattern: &str) -> StoreResult<Vec<ArcBytes>> {
        let wildcard = pattern == "*";
        let result = self
            .data
            .iter()
            .filter_map(|entry| {
                let key_str = String::from_utf8_lossy(&entry.key());
                if wildcard || key_str.contains(pattern) {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
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

    #[test]
    fn test_set_and_get() {
        let mut store = InMemoryStore::new();
        let k = key("hello");
        let v = Value::Str(ArcBytes::from_str("world"));

        store.set(k.clone(), v.clone()).unwrap();
        let got = store.get(k.clone()).unwrap();
        assert_eq!(got, Some(v));
    }

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

    #[test]
    fn test_get_nonexistent_key() {
        let mut store = InMemoryStore::new();
        let got = store.get(key("missing")).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let store = InMemoryStore::new();
        // deleting несуществующего ключа не должен вызывать ошибку
        assert!(store.del(key("nope")).is_ok());
    }
}
