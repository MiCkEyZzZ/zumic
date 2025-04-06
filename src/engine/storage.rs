use crate::{
    database::{types::Value, ArcBytes},
    error::StoreResult,
};

pub trait Storage: Send + Sync {
    fn set(&mut self, key: ArcBytes, value: Value) -> StoreResult<()>;
    fn get(&mut self, key: ArcBytes) -> StoreResult<Option<Value>>;
}
