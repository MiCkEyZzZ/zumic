use std::sync::Arc;

use dashmap::DashMap;

use super::storage::Storage;
use crate::{
    database::{types::Value, ArcBytes},
    error::StoreResult,
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
    fn delete(&self, key: ArcBytes) -> StoreResult<()> {
        self.data.remove(&key);
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
        store.delete(k.clone()).unwrap();

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
        assert!(store.delete(key("nope")).is_ok());
    }
}
