use std::io::{self};

use super::{InClusterStore, InMemoryStore, InPersistentStore};
use crate::{
    Sds, Storage, StoreResult, Value, {StorageConfig, StorageType},
};

pub enum StorageEngine {
    Memory(InMemoryStore),
    Cluster(InClusterStore),
    Persistent(InPersistentStore),
}

impl StorageEngine {
    pub fn set(&self, key: &Sds, value: Value) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.set(key, value),
            StorageEngine::Cluster(store) => store.set(key, value),
            StorageEngine::Persistent(store) => store.set(key, value),
        }
    }

    pub fn get(&self, key: &Sds) -> StoreResult<Option<Value>> {
        match self {
            StorageEngine::Memory(store) => store.get(key),
            StorageEngine::Cluster(store) => store.get(key),
            StorageEngine::Persistent(store) => store.get(key),
        }
    }

    pub fn del(&self, key: &Sds) -> StoreResult<bool> {
        match self {
            StorageEngine::Memory(store) => store.del(key),
            StorageEngine::Cluster(store) => store.del(key),
            StorageEngine::Persistent(store) => store.del(key),
        }
    }

    pub fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.mset(entries),
            StorageEngine::Cluster(store) => store.mset(entries),
            StorageEngine::Persistent(store) => store.mset(entries),
        }
    }

    pub fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>> {
        match self {
            StorageEngine::Memory(store) => store.mget(keys),
            StorageEngine::Cluster(store) => store.mget(keys),
            StorageEngine::Persistent(store) => store.mget(keys),
        }
    }

    pub fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.rename(from, to),
            StorageEngine::Cluster(store) => store.rename(from, to),
            StorageEngine::Persistent(store) => store.rename(from, to),
        }
    }

    pub fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool> {
        match self {
            StorageEngine::Memory(store) => store.renamenx(from, to),
            StorageEngine::Cluster(store) => store.renamenx(from, to),
            StorageEngine::Persistent(store) => store.renamenx(from, to),
        }
    }

    pub fn flushdb(&self) -> StoreResult<()> {
        match self {
            StorageEngine::Memory(store) => store.flushdb(),
            StorageEngine::Cluster(store) => store.flushdb(),
            StorageEngine::Persistent(store) => store.flushdb(),
        }
    }

    /// Initialize storage engine based in the passed configuration.
    pub fn initialize(config: &StorageConfig) -> io::Result<Self> {
        match &config.storage_type {
            StorageType::Memory => Ok(Self::Memory(InMemoryStore::new())),
            StorageType::Cluster => todo!("Cluster store initialization"),
            StorageType::Persistent => todo!("Persistent store initialization"),
        }
    }

    /// Gets a reference to a specific storage via the `Storage` common trait.
    pub fn get_store(&self) -> &dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
        }
    }

    pub fn get_store_mut(&mut self) -> &mut dyn Storage {
        match self {
            Self::Memory(store) => store,
            Self::Cluster(store) => store,
            Self::Persistent(store) => store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(data: &str) -> Sds {
        Sds::from(data.as_bytes())
    }

    /// Tests that setting a value and then getting it return the same value.
    #[test]
    fn test_engine_set_and_get() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("kin");
        let v = Value::Str(Sds::from_str("dzadza"));

        engine.set(&k, v.clone()).unwrap();
        let got = engine.get(&k).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Checks that getting a value by a non-existent key return None.
    #[test]
    fn test_engine_get_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("not_found");

        let got = engine.get(&k).unwrap();
        assert_eq!(got, None);
    }

    /// Checks that deleting an existing key removes it from the store.
    #[test]
    fn test_engine_delete() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("hello");
        let v = Value::Str(Sds::from_str("world"));

        engine.set(&k, v).unwrap();
        engine.del(&k).unwrap();

        let got = engine.get(&k).unwrap();
        assert_eq!(got, None)
    }

    /// Checks that deleting a non-existent key doesn't result in an error.
    #[test]
    fn test_engine_delete_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k = key("ghost");

        // Deleting should not cause a panic or error.
        let result = engine.del(&k);
        assert!(result.is_ok());
    }

    /// Tests setting multiple key-value pairs with mset.
    #[test]
    fn test_engine_mset() {
        let engine = StorageEngine::Memory(InMemoryStore::new());

        // Live variables so references are valid until the end of the function.
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));

        // Collect Vec<(&Sds, Value)>
        let entries = vec![(&k1, v1.clone()), (&k2, v2.clone())];
        engine.mset(entries).unwrap();

        // Checking what was supposed to be done
        assert_eq!(engine.get(&k1).unwrap(), Some(v1));
        assert_eq!(engine.get(&k2).unwrap(), Some(v2));
    }

    /// Checks that mget returns values in the correct order for multiple keys.
    #[test]
    fn test_engine_mget() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("kin1");
        let k2 = key("kin2");
        let v1 = Value::Str(Sds::from_str("dza1"));
        let v2 = Value::Str(Sds::from_str("dza2"));

        engine.set(&k1, v1.clone()).unwrap();
        engine.set(&k2, v2.clone()).unwrap();

        let got = engine.mget(&[&k1, &k2]).unwrap();
        assert_eq!(got, vec![Some(v1), Some(v2)]);
    }

    /// Checks that the key is renamed successfully.
    #[test]
    fn test_engine_rename() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));

        engine.set(&k1, v.clone()).unwrap();
        engine.rename(&k1, &k2).unwrap();

        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));
    }

    /// Checks that renaming a non-existent key results in an error.
    #[test]
    fn test_engine_rename_nonexistent_key() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");

        // An error should be returned when attempting to rename a non-existent key.
        let result = engine.rename(&k1, &k2);
        assert!(result.is_err());
    }

    /// Tests the behavior of the renamenx method: renaming is performed only
    /// if the new key is missing.
    #[test]
    fn test_engine_renamenx() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let k1 = key("old_key");
        let k2 = key("new_key");
        let v = Value::Str(Sds::from_str("value"));

        engine.set(&k1, v.clone()).unwrap();
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(result);

        // Check that the old key is deleted and the new one is present.
        let got = engine.get(&k2).unwrap();
        assert_eq!(got, Some(v));

        // Retrying the rename should fail because the new key already exists.
        let result = engine.renamenx(&k1, &k2).unwrap();
        assert!(!result);
    }

    /// Tests that flushdb clears all data from storage.
    #[test]
    fn test_engine_flushdb() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        engine
            .set(&key("a"), Value::Str(Sds::from_str("x")))
            .unwrap();
        engine
            .set(&key("b"), Value::Str(Sds::from_str("y")))
            .unwrap();

        engine.flushdb().unwrap();

        let a = engine.get(&key("a")).unwrap();
        let b = engine.get(&key("b")).unwrap();
        assert_eq!(a, None);
        assert_eq!(b, None);
    }

    /// Tests engine initialization with memory configuration.
    #[test]
    fn test_engine_initialize_memory() {
        let config = StorageConfig {
            storage_type: StorageType::Memory,
        };

        let engine = StorageEngine::initialize(&config);
        assert!(engine.is_ok());
    }

    /// Tests that the get_store method returns a trait object that can be manipulated.
    #[test]
    fn test_engine_get_store() {
        let engine = StorageEngine::Memory(InMemoryStore::new());
        let store = engine.get_store();
        assert!(store.mget(&[]).is_ok());
    }

    /// Tests that get_store_mut returns a mutable trait object that can be manipulated.
    #[test]
    fn test_engine_get_store_mut() {
        let mut engine = StorageEngine::Memory(InMemoryStore::new());
        let store_mut = engine.get_store_mut();
        assert!(store_mut.set(&key("x"), Value::Int(42)).is_ok());

        let got = store_mut.get(&key("x")).unwrap();
        assert_eq!(got, Some(Value::Int(42)));
    }
}
