use crate::{
    StoreResult, {Sds, Value},
};

/// The `Storage` trait defines the interface for key-value store backends.
/// All methods may return an error and produce a `StoreResult`.
pub trait Storage {
    /// Sets the value for the given key.
    /// Any existing value will be overwritten.
    fn set(&self, key: &Sds, value: Value) -> StoreResult<()>;

    /// Returns the value for the given key, or `None` if the key does not exist.
    fn get(&self, key: &Sds) -> StoreResult<Option<Value>>;

    /// Removes the key from the store.
    /// Returns `true` if the key was removed, or `false` if it didn’t exist.
    fn del(&self, key: &Sds) -> StoreResult<bool>;

    /// Sets multiple key-value pairs in a single operation.
    fn mset(&self, entries: Vec<(&Sds, Value)>) -> StoreResult<()>;

    /// Returns the values for a list of keys.
    /// If a key has no associated value, `None` is returned in its place.
    fn mget(&self, keys: &[&Sds]) -> StoreResult<Vec<Option<Value>>>;

    /// Renames a key to a new name.
    /// Returns an error if the original key does not exist.
    fn rename(&self, from: &Sds, to: &Sds) -> StoreResult<()>;

    /// Renames a key to a new name only if the target key does not already exist.
    /// Returns `true` if the rename was successful, `false` if the destination key already exists.
    fn renamenx(&self, from: &Sds, to: &Sds) -> StoreResult<bool>;

    /// Clears the database by removing all keys.
    fn flushdb(&self) -> StoreResult<()>;
}
