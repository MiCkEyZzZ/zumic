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
}
