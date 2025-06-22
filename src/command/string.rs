use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

#[derive(Debug)]
pub struct StrLenCommand {
    pub key: String,
}

impl CommandExecute for StrLenCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        if let Some(value) = store.get(&key)? {
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
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        let append_data = self.value.as_bytes();

        match store.get(&key)? {
            Some(Value::Str(ref s)) => {
                let mut result = Vec::with_capacity(s.len() + append_data.len());
                result.extend_from_slice(s);
                result.extend_from_slice(append_data);

                let result = Sds::from_vec(result);
                store.set(&key, Value::Str(result.clone()))?;

                Ok(Value::Int(result.len() as i64))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                let new_value = Sds::from_vec(append_data.to_vec());
                store.set(&key, Value::Str(new_value.clone()))?;
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
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key = Sds::from_str(&self.key);
        if let Some(value) = store.get(&key)? {
            if let Value::Str(ref s) = value {
                // Получаем результат из s.as_str() и обрабатываем возможную ошибку
                let s = s.as_str().map_err(|_| StoreError::InvalidType)?; // Преобразуем ошибку в StoreError

                let len = s.len() as i64;

                // Приведение отрицательных индексов
                let start = if self.start < 0 {
                    len + self.start
                } else {
                    self.start
                };
                let end = if self.end < 0 {
                    len + self.end
                } else {
                    self.end
                };

                // Корректные границы диапазона
                let start = start.max(0).min(len) as usize;
                let end = end.max(start as i64).min(len) as usize;

                let result = &s[start..end];
                return Ok(Value::Str(Sds::from_str(result)));
            } else {
                return Err(StoreError::InvalidType);
            }
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    // Вспомогательная функция для создания нового хранилища в памяти.
    fn create_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    /// Тестирует, что команда `StrLenCommand` правильно возвращает длину существующей строки.
    #[test]
    fn test_str_len_command_existing_key() {
        let mut store = create_store();

        store
            .set(&Sds::from_str("anton"), Value::Str(Sds::from_str("hello")))
            .unwrap();

        let strlen_cmd = StrLenCommand {
            key: "anton".to_string(),
        };
        let result = strlen_cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    /// Тестирует, что команда `AppendCommand` возвращает ошибку при попытке добавления данных
    /// к ключу неверного типа.
    #[test]
    fn test_append_command_invalid_type() {
        let mut store = create_store();
        store.set(&Sds::from_str("test"), Value::Int(42)).unwrap();

        let cmd = AppendCommand {
            key: "test".to_string(),
            value: "oops".to_string(),
        };
        let result = cmd.execute(&mut store);
        assert!(matches!(result, Err(StoreError::InvalidType)));
    }

    /// Тестирует, что команда `AppendCommand` корректно обрабатывает добавление пустой строки,
    /// результатом чего будет длина 0.
    #[test]
    fn test_append_empty_string() {
        let mut store = create_store();
        let cmd = AppendCommand {
            key: "empty".to_string(),
            value: "".to_string(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(0));
    }

    /// Тестирует, что команда `StrLenCommand` возвращает 0 для несуществующего ключа.
    #[test]
    fn test_str_len_command_non_existing_key() {
        let mut store = create_store();

        let strlen_cmd = StrLenCommand {
            key: "none_existing_key".to_string(),
        };
        let result = strlen_cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    /// Тестирует, что команда `AppendCommand` правильно добавляет данные к существующему
    /// строковому ключу.
    #[test]
    fn test_append_command_existing_key() {
        let mut store = create_store();

        store
            .set(&Sds::from_str("anton"), Value::Str(Sds::from_str("hello")))
            .unwrap();

        let command = AppendCommand {
            key: "anton".to_string(),
            value: " world".to_string(),
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Int(11));
    }

    /// Тестирует, что команда `AppendCommand` корректно создаёт новый ключ при добавлении данных
    /// к несуществующему ключу.
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

    /// Тестирует, что команда `GetRangeCommand` корректно возвращает подстроку из сохранённого
    /// значения.
    #[test]
    fn test_get_range_command_existing_key() {
        let mut store = create_store();

        store
            .set(
                &Sds::from_str("anton"),
                Value::Str(Sds::from_str("hello world")),
            )
            .unwrap();

        let command = GetRangeCommand {
            key: "anton".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store).unwrap();

        assert_eq!(result, Value::Str(Sds::from_str("hello")));
    }

    /// Тестирует, что команда `GetRangeCommand` возвращает `Null`, если ключ не существует.
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

    /// Тестирует, что команда `GetRangeCommand` возвращает ошибку, если значение ключа имеет
    /// неверный тип.
    #[test]
    fn test_get_range_command_invalid_type() {
        let mut store = create_store();

        // Добавляем строку с числовым значением в хранилище
        store.set(&Sds::from_str("anton"), Value::Int(42)).unwrap();

        let command = GetRangeCommand {
            key: "anton".to_string(),
            start: 0,
            end: 5,
        };
        let result = command.execute(&mut store);

        // Проверяем, что результат - ошибка
        assert!(result.is_err(), "Expected error but got Ok");

        // Проверяем, что ошибка соответствует InvalidType
        if let Err(StoreError::InvalidType) = result {
            // Ожидаем ошибку InvalidType, так как значение для ключа `anton` не является строкой
        } else {
            panic!("Expected InvalidType error, but got a different error");
        }
    }
}
