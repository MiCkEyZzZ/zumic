use crate::{
    database::{ArcBytes, Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct IncrByFloatCommand {
    pub key: String,
    pub increment: f64,
}

impl CommandExecute for IncrByFloatCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Float(current)) => {
                let new_value = current + self.increment;
                store.set(key_bytes, Value::Float(new_value))?;
                Ok(Value::Float(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Float(self.increment))?;
                Ok(Value::Float(self.increment))
            }
        }
    }
}

#[derive(Debug)]
pub struct DecrByFloatCommand {
    pub key: String,
    pub decrement: f64,
}

impl CommandExecute for DecrByFloatCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Float(current)) => {
                let new_value = current - self.decrement;
                store.set(key_bytes, Value::Float(new_value))?;
                Ok(Value::Float(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Float(-self.decrement))?;
                Ok(Value::Float(-self.decrement))
            }
        }
    }
}

#[derive(Debug)]
pub struct SetFloatCommand {
    pub key: String,
    pub value: f64,
}

impl CommandExecute for SetFloatCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = ArcBytes::from_str(&self.key);
        store.set(key_bytes, Value::Float(self.value))?;
        Ok(Value::Float(self.value))
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::memory::InMemoryStore;

    use super::*;

    #[test]
    fn test_incr_by_float() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(ArcBytes::from_str("key1"), Value::Float(10.0))
            .unwrap();

        let cmd = IncrByFloatCommand {
            key: "key1".to_string(),
            increment: 5.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(15.5));
    }

    #[test]
    fn test_decr_by_float() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(ArcBytes::from_str("key1"), Value::Float(10.0))
            .unwrap();

        let cmd = DecrByFloatCommand {
            key: "key1".to_string(),
            decrement: 3.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(6.5));
    }

    #[test]
    fn test_set_float() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let cmd = SetFloatCommand {
            key: "key1".to_string(),
            value: 20.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(20.5));
    }
}
