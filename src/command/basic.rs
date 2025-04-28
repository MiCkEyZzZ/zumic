use crate::{
    engine::engine::StorageEngine,
    StoreError, {QuickList, Sds, Value},
};

use super::execute::CommandExecute;

#[derive(Debug)]
pub struct SetCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.set(Sds::from_str(self.key.as_str()), self.value.clone())?;
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct GetCommand {
    pub key: String,
}

impl CommandExecute for GetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let result = store.get(Sds::from_str(self.key.as_str()));
        match result {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(Value::Null),
            Err(e) => Err(StoreError::from(e)),
        }
    }
}

#[derive(Debug)]
pub struct DelCommand {
    pub key: String,
}

impl CommandExecute for DelCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let deleted = store.del(Sds::from_str(&self.key))?;
        Ok(Value::Int(deleted))
    }
}

#[derive(Debug)]
pub struct ExistsCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for ExistsCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let count = self
            .keys
            .iter()
            .map(|key| Sds::from_str(key))
            .filter_map(|key| store.get(key).ok())
            .filter(|value| value.is_some())
            .count();

        Ok(Value::Int(count as i64))
    }
}

#[derive(Debug)]
pub struct SetNxCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetNxCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let exists = store.get(Sds::from_str(&self.key))?.is_some();
        if !exists {
            store.set(Sds::from_str(&self.key), self.value.clone())?;
            Ok(Value::Int(1))
        } else {
            Ok(Value::Int(0))
        }
    }
}

#[derive(Debug)]
pub struct MSetCommand {
    pub entries: Vec<(String, Value)>,
}

impl CommandExecute for MSetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let converted = self
            .entries
            .iter()
            .map(|(k, v)| (Sds::from_str(k), v.clone()))
            .collect();
        store.mset(converted)?;
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct MGetCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for MGetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let converted_keys: Vec<Sds> = self.keys.iter().map(|k| Sds::from_str(k)).collect();

        let values = store.mget(&converted_keys)?;

        let vec: Vec<Sds> = values
            .into_iter()
            .map(|opt| match opt {
                Some(Value::Str(s)) => Ok(s),
                Some(_) => Err(StoreError::WrongType("Неверный тип".to_string())),
                None => Ok(Sds::from_str("")), // пустая строка для None
            })
            .collect::<Result<_, _>>()?;

        let mut list = QuickList::new(64);
        for item in vec {
            list.push_back(item);
        }

        Ok(Value::List(list))
    }
}

#[derive(Debug)]
pub struct RenameCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.rename(Sds::from_str(&self.from), Sds::from_str(&self.to))?;
        Ok(Value::Str(Sds::from_str("")))
    }
}

#[derive(Debug)]
pub struct RenameNxCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameNxCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let success = store.renamenx(Sds::from_str(&self.from), Sds::from_str(&self.to))?;
        Ok(Value::Int(if success { 1 } else { 0 }))
    }
}

#[derive(Debug)]
pub struct FlushDbCommand;

impl CommandExecute for FlushDbCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.flushdb()?;
        Ok(Value::Str(Sds::from_str("OK")))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        command::{
            CommandExecute, DelCommand, ExistsCommand, FlushDbCommand, GetCommand, RenameCommand,
            RenameNxCommand, SetNxCommand,
        },
        database::{types::Value, Sds},
        engine::{engine::StorageEngine, memory::InMemoryStore},
    };

    use super::{MGetCommand, MSetCommand, SetCommand};

    /// Тестирование команды `SetCommand` и `GetCommand`.
    /// Проверяется, что после установки значения с помощью `SetCommand`
    /// оно может быть корректно получено с помощью `GetCommand`.
    #[test]
    fn test_set_and_get() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: Value::Str(Sds::from_str("test_value")),
        };

        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        assert_eq!(result.unwrap(), Value::Str(Sds::from_str("test_value")));
    }

    /// Тестирование `GetCommand` для несуществующего ключа.
    /// Проверяет, что команда возвращает `Null` для отсутствующих ключей.
    #[test]
    fn test_get_non_existent_key() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let get_command = GetCommand {
            key: "non_existent_key".to_string(),
        };

        let result = get_command.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        assert_eq!(result.unwrap(), Value::Null);
    }

    /// Тестирование `DelCommand` для существующего ключа.
    /// Проверяет, что удаление возвращает 1 и ключ действительно удалён.
    #[test]
    fn test_del_existing_key() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: Value::Str(Sds::from_str("test_value")),
        };

        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        let del_cmd = DelCommand {
            key: "test_key".to_string(),
        };

        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        assert_eq!(del_result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        assert_eq!(result.unwrap(), Value::Null);
    }

    /// Тестирование `DelCommand` для несуществующего ключа.
    #[test]
    fn test_del_non_existent_key() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let del_cmd = DelCommand {
            key: "non_existent_key".to_string(),
        };

        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        assert_eq!(del_result.unwrap(), Value::Int(0));
    }

    /// Тестирование `ExistsCommand`.
    /// Проверяет, что команда правильно считает количество существующих ключей.
    #[test]
    fn test_exists_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let exists_cmd = ExistsCommand {
            keys: vec!["test_key1".to_string(), "test_key2".to_string()],
        };
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(0));

        let set_cmd = SetCommand {
            key: "test_key1".to_string(),
            value: Value::Str(Sds::from_str("value")),
        };
        set_cmd.execute(&mut store).unwrap();

        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(1));

        let set_cmd2 = SetCommand {
            key: "test_key2".to_string(),
            value: Value::Str(Sds::from_str("another")),
        };
        set_cmd2.execute(&mut store).unwrap();

        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(2));
    }

    /// Тестирование `ExistsCommand` с пустым списком ключей.
    /// Проверяет, что команда возвращает 0 для пустого списка.
    #[test]
    fn test_exists_empty_keys() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        let exists_cmd = ExistsCommand { keys: vec![] };
        let result = exists_cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(0));
    }

    /// Тестирование `SetNxCommand` для отсутствующего ключа.
    /// Проверяет, что ключ устанавливается и возвращается 1.
    #[test]
    fn test_setnx_key_not_exists() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let setnx_cmd = SetNxCommand {
            key: "new_key".to_string(),
            value: Value::Str(Sds::from_str("new_value")),
        };

        let result = setnx_cmd.execute(&mut store);

        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "new_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("new_value")));
    }

    /// Тестирование `SetNxCommand` для существующего ключа.
    /// Проверяет, что команда возвращает 0 и не перезаписывает значение.
    #[test]
    fn test_setnx_key_exists() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

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

        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(0));

        let get_cmd = GetCommand {
            key: "existing_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value")));
    }

    /// Тестирование `MSetCommand` для установки нескольких ключей.
    /// Проверяет, что команда корректно устанавливает значения для всех ключей.
    #[test]
    fn test_mset() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(Sds::from_str("value1"))),
                ("key2".to_string(), Value::Str(Sds::from_str("value2"))),
            ],
        };

        let result = mset_cmd.execute(&mut store);
        assert!(result.is_ok(), "MSetCommand failed: {:?}", result);

        let get_cmd1 = GetCommand {
            key: "key1".to_string(),
        };

        let get_result1 = get_cmd1.execute(&mut store);
        assert!(get_result1.is_ok(), "GetCommand failed: {:?}", get_result1);
        assert_eq!(get_result1.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };

        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(get_result2.unwrap(), Value::Str(Sds::from_str("value2")));
    }

    /// Тестирование `MGetCommand`.
    /// Проверяет, что после установки значений через `MSetCommand`,
    /// `MGetCommand` корректно возвращает список значений в том же порядке.
    #[test]
    fn test_mget() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

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
        assert!(result.is_ok(), "MGetCommand failed: {:?}", result);

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

    /// Этот тест проверяет, что `RenameCommand` работает как ожидается.
    /// Он переименовывает существующий ключ в новое имя.
    #[test]
    fn test_rename() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(Sds::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        let rename_cmd = RenameCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameCommand failed: {:?}", result);

        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {:?}",
            get_result_old
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when renaming a key to a new key name.
    /// The `RenameNxCommand` only renames the key if the target key does not already exist.
    /// It first adds a key, executes the rename operation, and verifies that the new key exists with the original value,
    /// and that the old key is deleted successfully.
    #[test]
    fn test_renamenx() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(Sds::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        let rename_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {:?}",
            get_result_old
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when renaming a key only if the target key does not already exist.
    /// It first adds a key, attempts to rename it with `RenameNxCommand` (where the target key does not exist),
    /// and verifies that the key is successfully renamed. It then checks that the old key no longer exists and the new key is present.
    #[test]
    fn test_rename_nx_key_not_exists() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(Sds::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        let rename_nx_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        let result = rename_nx_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null);

        let get_cmd_new = GetCommand {
            key: "key2".to_string(),
        };
        let get_result_new = get_cmd_new.execute(&mut store);
        assert!(
            get_result_new.is_ok(),
            "GetCommand failed: {:?}",
            get_result_new
        );
        assert_eq!(get_result_new.unwrap(), Value::Str(Sds::from_str("value1")));
    }

    /// This test ensures that the `FlushDbCommand` properly clears all keys from the database.
    /// It first adds two keys, executes the flush command, and then checks that both keys have been removed.
    #[test]
    fn test_flushdb() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let set_cmd1 = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(Sds::from_str("value1")),
        };
        set_cmd1.execute(&mut store).unwrap();

        let set_cmd2 = SetCommand {
            key: "key2".to_string(),
            value: Value::Str(Sds::from_str("value2")),
        };
        set_cmd2.execute(&mut store).unwrap();

        let flush_cmd = FlushDbCommand;

        let result = flush_cmd.execute(&mut store);
        assert!(result.is_ok(), "FlushDbCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Str(Sds::from_str("OK")));

        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null);

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };
        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(get_result2.unwrap(), Value::Null);
    }
}
