use crate::{
    database::{arcbytes::ArcBytes, types::Value},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::CommandExecute;

#[derive(Debug)]
pub struct StrLenCommand {
    pub key: String,
}

impl CommandExecute for StrLenCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        if let Some(value) = store.get(key)? {
            if let Value::Str(ref s) = value {
                Ok(Value::Int(s.len() as i64))
            } else {
                Err(StoreError::InvalidType)
            }
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct AppendCommand {
    pub key: String,
    pub value: String,
}

impl CommandExecute for AppendCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        let append_data = self.value.as_bytes();

        match store.get(key.clone())? {
            Some(Value::Str(ref s)) => {
                let mut result = Vec::with_capacity(s.len() + append_data.len());
                result.extend_from_slice(s); // копируем оригинал
                result.extend_from_slice(append_data); // добавляем новое

                let result = ArcBytes::from_vec(result);
                store.set(key, Value::Str(result.clone()))?;

                Ok(Value::Int(result.len() as i64))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let new_value = ArcBytes::from_vec(append_data.to_vec()); // единственная аллокация
                store.set(key, Value::Str(new_value.clone()))?;
                Ok(Value::Int(new_value.len() as i64))
            }
        }
    }
}

#[derive(Debug)]
pub struct GetRangeCommand {
    pub key: String,
    pub start: i64,
    pub end: i64,
}

impl CommandExecute for GetRangeCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key = ArcBytes::from_str(&self.key);
        if let Some(value) = store.get(key)? {
            if let Value::Str(ref s) = value {
                let start = self.start.max(0) as usize;
                let end = self.end.min(s.len() as i64) as usize;
                let start = start.min(end); // Guarantee start <= end
                let sliced = s.slice(start..end);
                return Ok(Value::Str(sliced));
            } else {
                return Err(StoreError::InvalidType);
            }
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::memory::InMemoryStore;

    use super::*;

    fn create_store() -> StorageEngine {
        StorageEngine::InMemory(InMemoryStore::new())
    }

    #[test]
    fn test_str_len_command_existing_key() {
        let mut store = create_store();

        store
            .set(
                ArcBytes::from_str("anton"),
                Value::Str(ArcBytes::from_str("hello")),
            )
            .unwrap();

        let strlen_cmd = StrLenCommand {
            key: "anton".to_string(),
        };
        let result = strlen_cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_append_command_invalid_type() {
        let mut store = create_store();
        store
            .set(ArcBytes::from_str("test"), Value::Int(42))
            .unwrap();

        let cmd = AppendCommand {
            key: "test".to_string(),
            value: "oops".to_string(),
        };
        let result = cmd.execute(&mut store);
        assert!(matches!(result, Err(StoreError::InvalidType)));
    }

    #[test]
    fn test_append_empty_string() {
        let mut store = create_store();
        let cmd = AppendCommand {
            key: "empty".to_string(),
            value: "".to_string(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(0));
    }

    #[test]
    fn test_str_len_command_non_existing_key() {
        let mut store = create_store();

        let strlen_cmd = StrLenCommand {
            key: "none_existing_key".to_string(),
        };
        let result = strlen_cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_append_command_existing_key() {
        let mut store = create_store();

        store
            .set(
                ArcBytes::from_str("anton"),
                Value::Str(ArcBytes::from_str("hello")),
            )
            .unwrap();

        let command = AppendCommand {
            key: "anton".to_string(),
            value: " world".to_string(),
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Int(11));
    }

    #[test]
    fn test_append_command_non_existing_key() {
        let mut store = create_store();

        let command = AppendCommand {
            key: "non_existing_key".to_string(),
            value: "hello".to_string(),
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn test_get_range_command_existing_key() {
        let mut store = create_store();

        store
            .set(
                ArcBytes::from_str("anton"),
                Value::Str(ArcBytes::from_str("hello world")),
            )
            .unwrap();

        let command = GetRangeCommand {
            key: "anton".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Str(ArcBytes::from_str("hello")));
    }

    #[test]
    fn test_get_range_command_non_existing_key() {
        let mut store = create_store();

        let command = GetRangeCommand {
            key: "non_existing_key".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_get_range_command_invalid_type() {
        let mut store = create_store();

        // Add a string with a numeric value to the storage
        store
            .set(ArcBytes::from_str("anton"), Value::Int(42))
            .unwrap();

        let command = GetRangeCommand {
            key: "anton".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store);

        // Check if the result is an error
        assert!(result.is_err(), "Expected error but got Ok");

        // Check that the error matches InvalidType
        if let Err(StoreError::InvalidType) = result {
            // Expecting an InvalidType error because the value for key `anton` is not a string
        } else {
            panic!("Expected InvalidType error, but got a different error");
        }
    }
}
