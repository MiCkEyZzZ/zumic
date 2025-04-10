use crate::{
    database::{types::Value, ArcBytes},
    error::StoreResult,
};

pub trait Storage: Send + Sync {
    fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()>;
    fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>>;
    fn del(&self, key: ArcBytes) -> StoreResult<i64>;
    fn mget(&self, keys: &[ArcBytes]) -> StoreResult<Vec<Option<Value>>>;
    fn mset(&mut self, entries: Vec<(ArcBytes, Value)>) -> StoreResult<()>;
    fn keys(&self, pattern: &str) -> StoreResult<Vec<ArcBytes>>;
    fn rename(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<()>;
    fn renamenx(&mut self, from: ArcBytes, to: ArcBytes) -> StoreResult<bool>;
    fn flushdb(&mut self) -> StoreResult<()>;
}
