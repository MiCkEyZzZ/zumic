//! Команды для работы со строками (String) в Zumic.
//!
//! Реализует команды SET, GET, SETNX, MSET, MGET, STRLEN, APPEND, GETRANGE
//! для управления строковыми значениями по ключу.
//! Каждая команда реализует трейт [`CommandExecute`].

use crate::{CommandExecute, QuickList, Sds, StorageEngine, StoreError, Value};

/// Команда SET — устанавливает значение по ключу.
///
/// # Поля
/// * `key` — ключ, по которому сохраняется значение.
/// * `value` — сохраняемое значение.
#[derive(Debug)]
pub struct SetCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        store.set(&Sds::from_str(self.key.as_str()), self.value.clone())?;
        Ok(Value::Null)
    }
}

/// Команда GET — получает значение по ключу.
///
/// # Поля
/// * `key` — ключ, значение которого требуется получить.
#[derive(Debug)]
pub struct GetCommand {
    pub key: String,
}

impl CommandExecute for GetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let result = store.get(&Sds::from_str(self.key.as_str()));
        match result {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(Value::Null),
            Err(e) => Err(e),
        }
    }
}

/// Команда SETNX — устанавливает значение по ключу, только если ключ не
/// существует.
///
/// # Поля
/// * `key` — ключ, по которому сохраняется значение.
/// * `value` — сохраняемое значение.
#[derive(Debug)]
pub struct SetNxCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetNxCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let exists = store.get(&Sds::from_str(&self.key))?.is_some();
        if !exists {
            store.set(&Sds::from_str(&self.key), self.value.clone())?;
            Ok(Value::Int(1))
        } else {
            Ok(Value::Int(0))
        }
    }
}

/// Команда MSET — устанавливает значения по нескольким ключам одновременно.
///
/// # Поля
/// * `entries` — вектор пар (ключ, значение) для установки.
#[derive(Debug)]
pub struct MSetCommand {
    pub entries: Vec<(String, Value)>,
}

impl CommandExecute for MSetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let mut keys: Vec<Sds> = Vec::with_capacity(self.entries.len());
        for (k, _) in &self.entries {
            keys.push(Sds::from_str(k));
        }
        let converted: Vec<(&Sds, Value)> = keys
            .iter()
            .zip(self.entries.iter().map(|(_, v)| v.clone()))
            .collect();

        store.mset(converted)?;
        Ok(Value::Null)
    }
}

/// Команда MGET — получает значения по нескольким ключам одновременно.
///
/// # Поля
/// * `keys` — список ключей для получения значений.
#[derive(Debug)]
pub struct MGetCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for MGetCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        // 1. Сначала переводим все String → Sds и храним их, чтобы ссылки на них были
        //    валидны
        let converted_keys: Vec<Sds> = self.keys.iter().map(|k| Sds::from_str(k)).collect();

        // 2. Собираем Vec<&Sds> из уже существующих Sds
        let key_refs: Vec<&Sds> = converted_keys.iter().collect();

        // 3. Вызываем mget, передавая &[&Sds]
        let values = store.mget(&key_refs)?;

        // 4. Преобразуем Vec<Option<Value>> → Vec<Sds>, обрабатывая None/ошибки
        let vec: Vec<Sds> = values
            .into_iter()
            .map(|opt| match opt {
                Some(Value::Str(s)) => Ok(s),
                Some(_) => Err(StoreError::WrongType("Неверный тип".into())),
                None => Ok(Sds::from_str("")), // пустая строка для None
            })
            .collect::<Result<_, _>>()?;

        // 5. Упаковываем в QuickList
        let mut list = QuickList::new(64);
        for item in vec {
            list.push_back(item);
        }

        Ok(Value::List(list))
    }
}

/// Команда STRLEN — возвращает длину строки по ключу.
///
/// Формат: `STRLEN key`
///
/// # Поля
/// * `key` — ключ строки.
///
/// # Возвращает
/// Длину строки (в байтах) или 0, если ключ не существует.
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

/// Команда APPEND — добавляет данные к строке по ключу.
///
/// Формат: `APPEND key value`
///
/// # Поля
/// * `key` — ключ строки.
/// * `value` — добавляемое значение.
///
/// # Возвращает
/// Новую длину строки после добавления.
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

/// Команда GETRANGE — возвращает подстроку по диапазону индексов.
///
/// Формат: `GETRANGE key start end`
///
/// # Поля
/// * `key` — ключ строки.
/// * `start` — начальный индекс.
/// * `end` — конечный индекс.
///
/// # Возвращает
/// Подстроку в заданном диапазоне или `Null`, если ключ не существует.
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

    // ==================== Тесты для SET/GET ====================

    /// Тестирование команды `SetCommand` и `GetCommand`.
    /// Проверяется, что после установки значения с помощью `SetCommand`
    /// оно может быть корректно получено с помощью `GetCommand`.
    #[test]
    fn test_set_and_get() {
        let mut store = create_store();

        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: Value::Str(Sds::from_str("test_value")),
        };

        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {result:?}");

        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {result:?}");

        assert_eq!(result.unwrap(), Value::Str(Sds::from_str("test_value")));
    }

    /// Тестирование `GetCommand` для несуществующего ключа.
    /// Проверяет, что команда возвращает `Null` для отсутствующих ключей.
    #[test]
    fn test_get_non_existent_key() {
        let mut store = create_store();

        let get_command = GetCommand {
            key: "non_existent_key".to_string(),
        };

        let result = get_command.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {result:?}");

        assert_eq!(result.unwrap(), Value::Null);
    }

    // ==================== Тесты для SETNX ====================

    /// Тестирование `SetNxCommand` для отсутствующего ключа.
    /// Проверяет, что ключ устанавливается и возвращается 1.
    #[test]
    fn test_setnx_key_not_exists() {
        let mut store = create_store();

        let setnx_cmd = SetNxCommand {
            key: "new_key".to_string(),
            value: Value::Str(Sds::from_str("new_value")),
        };

        let result = setnx_cmd.execute(&mut store);

        assert!(result.is_ok(), "SetNxCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "new_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("new_value")));
    }

    /// Тестирование `SetNxCommand` для существующего ключа.
    /// Проверяет, что команда возвращает 0 и не перезаписывает значение.
    #[test]
    fn test_setnx_key_exists() {
        let mut store = create_store();

        let set_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(Sds::from_str("value")),
        };

        let _ = set_cmd.execute(&mut store);

        let setnx_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(Sds::from_str("new_value")),
        };

        let result = setnx_cmd.execute(&mut store);

        assert!(result.is_ok(), "SetNxCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Int(0));

        let get_cmd = GetCommand {
            key: "existing_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value")));
    }

    // ==================== Тесты для MSET/MGET ====================

    /// Тестирование `MSetCommand` для установки нескольких ключей.
    /// Проверяет, что команда корректно устанавливает значения для всех ключей.
    #[test]
    fn test_mset() {
        let mut store = create_store();

        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(Sds::from_str("value1"))),
                ("key2".to_string(), Value::Str(Sds::from_str("value2"))),
            ],
        };

        let result = mset_cmd.execute(&mut store);
        assert!(result.is_ok(), "MSetCommand failed: {result:?}");

        let get_cmd1 = GetCommand {
            key: "key1".to_string(),
        };

        let get_result1 = get_cmd1.execute(&mut store);
        assert!(get_result1.is_ok(), "GetCommand failed: {get_result1:?}");
        assert_eq!(get_result1.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };

        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {get_result2:?}");
        assert_eq!(get_result2.unwrap(), Value::Str(Sds::from_str("value2")));
    }

    /// Тестирование `MGetCommand`.
    /// Проверяет, что после установки значений через `MSetCommand`,
    /// `MGetCommand` корректно возвращает список значений в том же порядке.
    #[test]
    fn test_mget() {
        let mut store = create_store();

        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(Sds::from_str("value1"))),
                ("key2".to_string(), Value::Str(Sds::from_str("value2"))),
            ],
        };
        mset_cmd.execute(&mut store).unwrap();

        let mget_cmd = MGetCommand {
            keys: vec!["key1".to_string(), "key2".to_string()],
        };

        let result = mget_cmd.execute(&mut store);
        assert!(result.is_ok(), "MGetCommand failed: {result:?}");

        let result_list = match result.unwrap() {
            Value::List(list) => list,
            _ => panic!("Expected Value::List, got something else"),
        };

        let values: Vec<String> = result_list
            .into_iter()
            .map(|item| String::from_utf8_lossy(&item).to_string())
            .collect();

        assert_eq!(values, vec!["value1".to_string(), "value2".to_string()]);
    }

    // ==================== Тесты для STRLEN ====================

    /// Тестирует, что команда `StrLenCommand` правильно возвращает длину
    /// существующей строки.
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

    /// Тестирует, что команда `StrLenCommand` возвращает 0 для несуществующего
    /// ключа.
    #[test]
    fn test_str_len_command_non_existing_key() {
        let mut store = create_store();

        let strlen_cmd = StrLenCommand {
            key: "none_existing_key".to_string(),
        };
        let result = strlen_cmd.execute(&mut store).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    // ==================== Тесты для APPEND ====================

    /// Тестирует, что команда `AppendCommand` правильно добавляет данные к
    /// существующему строковому ключу.
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

    /// Тестирует, что команда `AppendCommand` корректно создаёт новый ключ при
    /// добавлении данных к несуществующему ключу.
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

    /// Тестирует, что команда `AppendCommand` возвращает ошибку при попытке
    /// добавления данных к ключу неверного типа.
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

    /// Тестирует, что команда `AppendCommand` корректно обрабатывает добавление
    /// пустой строки, результатом чего будет длина 0.
    #[test]
    fn test_append_empty_string() {
        let mut store = create_store();
        let cmd = AppendCommand {
            key: "empty".to_string(),
            value: "".to_string(),
        };
        assert_eq!(cmd.execute(&mut store).unwrap(), Value::Int(0));
    }

    // ==================== Тесты для GETRANGE ====================

    /// Тестирует, что команда `GetRangeCommand` корректно возвращает подстроку
    /// из сохранённого значения.
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

    /// Тестирует, что команда `GetRangeCommand` возвращает `Null`, если ключ не
    /// существует.
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

    /// Тестирует, что команда `GetRangeCommand` возвращает ошибку, если
    /// значение ключа имеет неверный тип.
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
            // Ожидаем ошибку InvalidType, так как значение для ключа `anton` не
            // является строкой
        } else {
            panic!("Expected InvalidType error, but got a different error");
        }
    }
}
