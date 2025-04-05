use super::{memory::InMemoryStore, persistent::PersistentStore};

#[derive(Clone, Debug)]
pub enum StorageType {
    Memory,
    Persistent,
    Clustered,
}

/// Основной движок хранения.
pub enum StorageEngine {
    InMemory(InMemoryStore),
    Persistent(PersistentStore),
}

impl StorageEngine {
    pub fn initialize() {}
    pub fn get_store() {}
}
