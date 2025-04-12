use crate::{
    database::{ArcBytes, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct IncrCommand {
    pub key: String,
}

impl CommandExecute for IncrCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Int(current)) => {
                let new_value = current + 1;
                store.set(key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Int(1))?;
                Ok(Value::Int(1))
            }
        }
    }
}

#[derive(Debug)]
pub struct IncrByCommand {
    pub key: String,
    pub increment: i64,
}

impl CommandExecute for IncrByCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let keys_bytes = ArcBytes::from_str(&self.key);

        match store.get(keys_bytes.clone())? {
            Some(Value::Int(current)) => {
                let new_value = current + self.increment;
                store.set(keys_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(keys_bytes, Value::Int(self.increment))?;
                Ok(Value::Int(self.increment))
            }
        }
    }
}

#[derive(Debug)]
pub struct DecrCommand {
    pub key: String,
}

impl CommandExecute for DecrCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Int(current)) => {
                let new_value = current - 1;
                store.set(key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Int(-1))?;
                Ok(Value::Int(-1)) // If key doesn't exist, set it to -1
            }
        }
    }
}

#[derive(Debug)]
pub struct DecrByCommand {
    pub key: String,
    pub decrement: i64,
}

impl CommandExecute for DecrByCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Int(current)) => {
                let new_value = current - self.decrement;
                store.set(key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Int(-self.decrement))?;
                Ok(Value::Int(-self.decrement))
            }
        }
    }
}
