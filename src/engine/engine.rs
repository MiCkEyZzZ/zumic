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

    pub fn delete(&mut self, key: ArcBytes) -> StoreResult<()> {
        info!("Deleting key: {:?}", key);
        match self {
            StorageEngine::InMemory(store) => store.delete(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::types::Value;
    use crate::database::ArcBytes;

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
        engine.delete(k.clone()).unwrap();

        let got = engine.get(k.clone()).unwrap();
        assert_eq!(got, None)
    }

    #[test]
    fn test_engine_delete_nonexistent_key() {
        let mut engine = StorageEngine::InMemory(InMemoryStore::new());
        let k = key("ghost");

        // delete should not panic or error
        let result = engine.delete(k);
        assert!(result.is_ok());
    }
}
