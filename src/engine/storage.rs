use std::io::Result;

use crate::database::types::Value;

pub trait Storage {
    fn set(&mut self, key: String, value: Value) -> Result<()>;
    fn get(&mut self, key: String) -> Option<Value>;
}
