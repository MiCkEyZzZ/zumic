use crate::{
    database::{arcbytes::ArcBytes, types::Value},
    error::StoreResult,
};

/// The `Storage` trait defines the interface for key-value storage backends.
/// All methods are fallible and return a `StoreResult`.
pub trait Storage: Send + Sync {
    /// Sets the value for the given key. Overwrites existing values.
    fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()>;

    /// Retrieves the value for the given key, or `None` if the key doesn't exist.
    fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>>;

    /// Deletes a key from the store.
    /// Returns `1` if the key was removed, `0` if it didn't exist.
    fn del(&self, key: ArcBytes) -> StoreResult<i64>;

    /// Sets multiple key-value pairs in a batch operation.
    fn mset(&mut self, entries: Vec<(ArcBytes, Value)>) -> StoreResult<()>;

    /// Gets values for a list of keys.
    /// If a key is missing, `None` is returned in its place.
    fn mget(&self, keys: &[ArcBytes]) -> StoreResult<Vec<Option<Value>>>;

    /// Renames a key to a new key name.
    /// Returns an error if the source key does not exist.
    fn rename(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<()>;

    /// Renames a key to a new name only if the new key doesn't already exist.
    /// Returns `true` if renamed, `false` if the destination exists.
    fn renamenx(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<bool>;

    /// Clears all keys from the database.
    fn flushdb(&mut self) -> StoreResult<()>;
}
