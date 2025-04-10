use tracing::info;

use crate::{
    database::{ArcBytes, Value},
    error::StoreResult,
};

use super::{memory::InMemoryStore, storage::Storage};

#[derive(Clone, Debug)]
pub enum StorageType {
    Memory,
    Persistent,
    Clustered,
}

pub enum StorageEngine {
    InMemory(InMemoryStore),
}

impl StorageEngine {
    pub fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()> {
        info!("Setting value for key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.set(key, value),
        }
    }

    pub fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>> {
        info!("Getting value for key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.get(key),
        }
    }

    pub fn del(&mut self, key: ArcBytes) -> StoreResult<i64> {
        info!("Deleting key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.del(key),
        }
    }

    pub fn mset(&mut self, entries: Vec<(ArcBytes, Value)>) -> StoreResult<()> {
        info!("MSET {} leys", entries.len());
        match self {
            StorageEngine::InMemory(store) => store.mset(entries),
        }
    }

    pub fn mget(&self, keys: &[ArcBytes]) -> StoreResult<Vec<Option<Value>>> {
        info!("MGET {} keys", keys.len());
        match self {
            StorageEngine::InMemory(store) => store.mget(keys),
        }
    }

    pub fn keys(&self, pattern: &str) -> StoreResult<Vec<ArcBytes>> {
        info!("Finding keys with pattern: {:?}", pattern);
        match self {
            StorageEngine::InMemory(store) => store.keys(pattern),
        }
    }

    pub fn rename(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<()> {
        info!("Renaming key: {:?} to {:?}", from, to);
        match self {
            StorageEngine::InMemory(store) => store.rename(from, to),
        }
    }

    pub fn renamenx(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<bool> {
        info!("Renaming key (NX): {:?} to {:?}", from, to);
        match self {
            StorageEngine::InMemory(store) => store.renamenx(from, to),
        }
    }

    pub fn flushdb(&mut self) -> StoreResult<()> {
        info!("Flushing database");
        match self {
            StorageEngine::InMemory(store) => store.flushdb(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{ArcBytes, Value};

    fn key(data: &str) -> ArcBytes {
        ArcBytes::from(data.as_bytes().to_vec())
    }

    #[test]
    fn test_engine_set_and_get() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("foo");
        let v = Value::Str(ArcBytes::from_str("bar"));

        engine.set(k.clone(), v.clone()).unwrap();
        let got = engine.get(k.clone()).unwrap();
        assert_eq!(got, Some(v));
    }

    #[test]
    fn test_engine_get_nonexistent_key() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("not_found");

        let got = engine.get(k).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn test_engine_delete() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("hello");
        let v = Value::Str(ArcBytes::from_str("world"));

        engine.set(k.clone(), v).unwrap();
        engine.del(k.clone()).unwrap();

        let got = engine.get(k.clone()).unwrap();
        assert_eq!(got, None)
    }

    #[test]
    fn test_engine_delete_nonexistent_key() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("ghost");

        // delete should not panic or error
        let result = engine.del(k);
        assert!(result.is_ok());
    }

    #[test]
    fn test_engine_mset() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let entries = vec![
            (key("kin1"), Value::Str(ArcBytes::from_str("dza1"))),
            (key("kin2"), Value::Str(ArcBytes::from_str("dza2"))),
        ];

        engine.mset(entries.clone()).unwrap();

        for (k, v) in entries {
            let got = engine.get(k).unwrap();
            assert_eq!(got, Some(v));
        }
    }

    #[test]
    fn test_engine_mget() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(ArcBytes::from_str("dza1"));
        let v2 = Value::Str(ArcBytes::from_str("dza2"));

        engine.set(k1.clone(), v1.clone()).unwrap();
        engine.set(k2.clone(), v2.clone()).unwrap();

        let got = engine.mget(&[k1, k2]).unwrap();
        assert_eq!(got, vec![Some(v1), Some(v2)]);
    }

    #[test]
    fn test_engine_keys() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        engine
            .set(key("kin1"), Value::Str(ArcBytes::from_str("dza1")))
            .unwrap();
        engine
            .set(key("kin2"), Value::Str(ArcBytes::from_str("dza2")))
            .unwrap();
        engine
            .set(key("dza3"), Value::Str(ArcBytes::from_str("dza3")))
            .unwrap();

        let got = engine.keys("kin").unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn test_engine_rename() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(ArcBytes::from_str("value"));

        engine.set(k1.clone(), v.clone()).unwrap();
        engine.rename(k1.clone(), k2.clone()).unwrap();

        let got = engine.get(k2.clone()).unwrap();
        assert_eq!(got, Some(v));
    }

    #[test]
    fn test_engine_rename_nonexistent_key() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");

        // should return error when renaming non-existent key
        let result = engine.rename(k1.clone(), k2.clone());
        assert!(result.is_err());
    }

    #[test]
    fn test_engine_renamenx() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(ArcBytes::from_str("value"));

        engine.set(k1.clone(), v.clone()).unwrap();
        let result = engine.renamenx(k1.clone(), k2.clone()).unwrap();
        assert!(result);

        // Ensure the old key is gone and new key exists
        let got = engine.get(k2.clone()).unwrap();
        assert_eq!(got, Some(v));

        // Trying to rename again should fail (key already exists)
        let result = engine.renamenx(k1.clone(), k2.clone()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_engine_flushdb() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        engine
            .set(key("kin1"), Value::Str(ArcBytes::from_str("dza1")))
            .unwrap();
        engine
            .set(key("kin2"), Value::Str(ArcBytes::from_str("dza2")))
            .unwrap();

        engine.flushdb().unwrap();

        // Ensure all keys are removed
        let got = engine.keys("*").unwrap();
        assert!(got.is_empty());
    }
}
