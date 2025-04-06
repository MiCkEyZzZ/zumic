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
}
