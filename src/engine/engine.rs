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
        match self {
            StorageEngine::InMemory(store) => store.set(key, value),
        }
    }

    pub fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>> {
        match self {
            StorageEngine::InMemory(store) => store.get(key),
        }
    }

    pub fn delete(&mut self, key: ArcBytes) -> StoreResult<()> {
        match self {
            StorageEngine::InMemory(store) => store.delete(key),
        }
    }
}
