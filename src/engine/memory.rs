use std::{io::Result, sync::Arc};

use dashmap::DashMap;

use super::storage::Storage;
use crate::database::types::Value;

pub struct InMemoryStore {
    pub data: Arc<DashMap<String, Value>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
        }
    }
}

impl Storage for InMemoryStore {
    fn set(&mut self, key: String, value: Value) -> Result<()> {
        self.data.insert(key, value);
        Ok(())
    }
    fn get(&mut self, key: String) -> Option<Value> {
        self.data.get(&key).map(|entry| entry.clone())
    }
}
