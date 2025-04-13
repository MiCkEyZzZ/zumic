use crate::{
    database::{arcbytes::ArcBytes, types::Value},
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

#[cfg(test)]
mod tests {
    use crate::engine::memory::InMemoryStore;

    use super::*;

    /// Test the `INCR` command:
    /// - If the key does not exist, it should be set to 1.
    /// - If the key exists with an integer value, it should be incremented by 1.
    #[test]
    fn test_incr_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let incr_command = IncrCommand {
            key: "counter".to_string(),
        };

        // Test when key doesn't exist (should create and set to 1).
        let result = incr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(1));

        // Test when key exists (should increment to 2).
        let result = incr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(2));
    }

    /// Test the `INCRBY` command:
    /// - If the key does not exist, it should be created with the given increment value.
    /// - If the key exists with an integer value, it should be incremented by the specified amount.
    #[test]
    fn test_incrby_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let incr_by_command = IncrByCommand {
            key: "counter".to_string(),
            increment: 5,
        };

        // Test when key doesn't exist (should set to 5).
        let result = incr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(5));

        // Test when key exists (should increment by 5, total becomes 10).
        let result = incr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(10));
    }

    /// Test the `DECR` command:
    /// - If the key does not exist, it should be created with the value -1.
    /// - If the key exists with an integer value, it should be decremented by 1.
    #[test]
    fn test_decr_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let decr_command = DecrCommand {
            key: "counter".to_string(),
        };

        // Test when key doesn't exist (should set to -3).
        let result = decr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-1));

        // Test when key exists (should decrement to -2).
        let result = decr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-2));
    }

    /// Test the `DECRBY` command:
    /// - If the key does not exist, it should be created with the negative value of the decrement.
    /// - If the key exists with an integer value, it should be decremented by the specified amount.
    #[test]
    fn test_decrby_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let decr_by_command = DecrByCommand {
            key: "counter".to_string(),
            decrement: 3,
        };

        // Test when key doesn't exist (should set to -3).
        let result = decr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-3));

        // Test when key exists (should decrement by 3, total becomes -6).
        let result = decr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-6));
    }

    /// Test invalid type for `INCR` command:
    /// - If the key exists but the value is not an integer, the command should return an error.
    #[test]
    fn test_invalid_type_for_incr() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(
                ArcBytes::from_str("counter"),
                Value::Str(ArcBytes::from_str("string")),
            )
            .unwrap();

        let incr_command = IncrCommand {
            key: "counter".to_string(),
        };

        let result = incr_command.execute(&mut store);
        assert!(result.is_err()); // Should return an InvalidType error
    }

    /// Test invalid type for `DECR` command:
    /// - If the key exists but the value is not an integer, the command should return an error.
    #[test]
    fn test_invalid_type_for_decr() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(
                ArcBytes::from_str("counter"),
                Value::Str(ArcBytes::from_str("string")),
            )
            .unwrap();

        let decr_command = DecrCommand {
            key: "counter".to_string(),
        };

        let result = decr_command.execute(&mut store);
        assert!(result.is_err()); // Should return an InvalidType error
    }
}
