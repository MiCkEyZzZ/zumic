use crate::{
    database::{ArcBytes, Value},
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
        let append_value = ArcBytes::from_str(&self.value);

        if let Some(mut existing_value) = store.get(key.clone())? {
            if let Value::Str(ref mut s) = existing_value {
                // Клонируем s для работы с данными и добавляем новые байты
                let mut updated_value = s.to_vec();
                updated_value.extend_from_slice(&append_value.to_vec()); // Добавляем новые байты

                // Сохраняем обновленное значение
                store.set(key, Value::Str(ArcBytes::from_vec(updated_value.clone())))?;
                return Ok(Value::Int(updated_value.len() as i64)); // Возвращаем длину обновленной строки
            } else {
                return Err(StoreError::InvalidType); // Ошибка, если значение не строка
            }
        }

        // Если строки не было, создаем новую строку
        store.set(key, Value::Str(append_value.clone()))?;
        Ok(Value::Int(append_value.len() as i64)) // Возвращаем длину новой строки
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
                let start = self.start as usize;
                let end = self.end as usize;
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

        // Добавляем в хранилище строку с числовым значением
        store
            .set(ArcBytes::from_str("anton"), Value::Int(42))
            .unwrap();

        let command = GetRangeCommand {
            key: "anton".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store);

        // Проверяем, что результат является ошибкой
        assert!(result.is_err(), "Expected error but got Ok");

        // Проверяем, что ошибка соответствует InvalidType
        if let Err(StoreError::InvalidType) = result {
            // Ожидаем ошибку InvalidType, так как значение для ключа "anton" не является строкой
        } else {
            panic!("Expected InvalidType error, but got a different error");
        }
    }
}
