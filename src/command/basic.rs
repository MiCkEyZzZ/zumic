use crate::{
    database::{types::Value, ArcBytes, QuickList},
    engine::engine::StorageEngine,
    error::StoreError,
};

use super::execute::CommandExecute;

#[derive(Debug)]
pub struct SetCommand {
    pub key: String,
    pub value: Value,
}

impl CommandExecute for SetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.set(ArcBytes::from_str(self.key.as_str()), self.value.clone())?;
        Ok(Value::Null)
    }
}

#[derive(Debug)]
pub struct GetCommand {
    pub key: String,
}

impl CommandExecute for GetCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let result = store.get(ArcBytes::from_str(self.key.as_str()));
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
        let deleted = store.del(ArcBytes::from_str(&self.key))?;
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
            .map(|key| ArcBytes::from_str(key))
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
        let exists = store.get(ArcBytes::from_str(&self.key))?.is_some();
        if !exists {
            store.set(ArcBytes::from_str(&self.key), self.value.clone())?;
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
            .map(|(k, v)| (ArcBytes::from_str(k), v.clone()))
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
        let converted_keys: Vec<ArcBytes> =
            self.keys.iter().map(|k| ArcBytes::from_str(k)).collect();

        let values = store.mget(&converted_keys)?;

        let vec: Vec<ArcBytes> = values
            .into_iter()
            .map(|opt| match opt {
                Some(Value::Str(s)) => Ok(s),
                Some(_) => Err(StoreError::WrongType("Неверный тип".to_string())),
                None => Ok(ArcBytes::from_str("")), // пустая строка для None
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
        store.rename(ArcBytes::from_str(&self.from), ArcBytes::from_str(&self.to))?;
        Ok(Value::Str(ArcBytes::from_str("")))
    }
}

#[derive(Debug)]
pub struct RenameNxCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameNxCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let success =
            store.renamenx(ArcBytes::from_str(&self.from), ArcBytes::from_str(&self.to))?;
        Ok(Value::Int(if success { 1 } else { 0 }))
    }
}

#[derive(Debug)]
pub struct FlushDbCommand;

impl CommandExecute for FlushDbCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        store.flushdb()?;
        Ok(Value::Str(ArcBytes::from_str("OK")))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        command::{
            CommandExecute, DelCommand, ExistsCommand, FlushDbCommand, GetCommand, RenameCommand,
            RenameNxCommand, SetNxCommand,
        },
        database::{ArcBytes, Value},
        engine::{engine::StorageEngine, memory::InMemoryStore},
    };

    use super::{MGetCommand, MSetCommand, SetCommand};

    #[test]
    fn test_set_and_get() {
        // Initialize store
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a SetCommand
        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: crate::database::Value::Str(ArcBytes::from_str("test_value")),
        };

        // Execute the set command
        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        // Create a GetCommand
        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        // Execute the get command
        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Check that the value matches
        assert_eq!(
            result.unwrap(),
            Value::Str(ArcBytes::from_str("test_value"))
        );
    }

    #[test]
    fn test_get_non_existent_key() {
        // Initialize store
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Create a GetCommand with non-existent key
        let get_command = GetCommand {
            key: "non_existent_key".to_string(),
        };

        // Execute the get command
        let result = get_command.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Проверка, что возвращается Null для несуществующего ключа
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn test_del_existing_key() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetCommand
        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: crate::database::Value::Str(ArcBytes::from_str("test_value")),
        };

        // Выполнение команды set
        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {:?}", result);

        // Создаем команду DelCommand
        let del_cmd = DelCommand {
            key: "test_key".to_string(),
        };

        // Выполнение команды del
        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        // Проверка, что возвращается 1 (удалено 1 значение)
        assert_eq!(del_result.unwrap(), Value::Int(1));

        // Проверяем, что ключ больше не существует
        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Проверка, что возвращается Null для удалённого ключа
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn test_del_non_existent_key() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду DelCommand с несуществующим ключом
        let del_cmd = DelCommand {
            key: "non_existent_key".to_string(),
        };

        // Выполнение команды del
        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {:?}", del_result);

        // Проверка, что возвращается 0 (ничего не удалено)
        assert_eq!(del_result.unwrap(), Value::Int(0));
    }

    #[test]
    fn test_exists_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Проверяем существование до добавления ключей
        let exists_cmd = ExistsCommand {
            keys: vec!["test_key1".to_string(), "test_key2".to_string()],
        };
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(0)); // Оба ключа отсутствуют

        // Добавляем один из ключей
        let set_cmd = SetCommand {
            key: "test_key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Проверяем снова — один ключ существует
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(1)); // Только один существует

        // Добавляем второй ключ
        let set_cmd2 = SetCommand {
            key: "test_key2".to_string(),
            value: Value::Str(ArcBytes::from_str("another")),
        };
        set_cmd2.execute(&mut store).unwrap();

        // Теперь оба должны существовать
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Int(2)); // Оба существуют
    }

    #[test]
    fn test_exists_empty_keys() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        let exists_cmd = ExistsCommand { keys: vec![] };
        let result = exists_cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(0)); // Пустой список — ноль
    }

    #[test]
    fn test_setnx_key_not_exists() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetNxCommand с новым ключом
        let setnx_cmd = SetNxCommand {
            key: "new_key".to_string(),
            value: Value::Str(ArcBytes::from_str("new_value")),
        };

        // Выполнение команды SETNX
        let result = setnx_cmd.execute(&mut store);

        // Проверка, что команда вернула 1 (ключ был установлен)
        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1));

        // Проверка, что значение действительно установлено
        let get_cmd = GetCommand {
            key: "new_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("new_value"))
        );
    }

    #[test]
    fn test_setnx_key_exists() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetNxCommand с существующим ключом
        let set_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(ArcBytes::from_str("value")),
        };

        // Выполнение команды SETNX для установки значения
        let _ = set_cmd.execute(&mut store);

        // Теперь пробуем выполнить SETNX для этого же ключа
        let setnx_cmd = SetNxCommand {
            key: "existing_key".to_string(),
            value: Value::Str(ArcBytes::from_str("new_value")),
        };

        // Выполнение команды SETNX для уже существующего ключа
        let result = setnx_cmd.execute(&mut store);

        // Проверка, что команда вернула 0 (ключ уже существует)
        assert!(result.is_ok(), "SetNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(0));

        // Проверка, что значение не изменилось
        let get_cmd = GetCommand {
            key: "existing_key".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Str(ArcBytes::from_str("value")));
    }

    #[test]
    fn test_mset() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду MSetCommand
        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(ArcBytes::from_str("value1"))),
                ("key2".to_string(), Value::Str(ArcBytes::from_str("value2"))),
            ],
        };

        // Выполняем команды mset
        let result = mset_cmd.execute(&mut store);
        assert!(result.is_ok(), "MSetCommand failed: {:?}", result);

        // Проверка, что значения были установлены.
        let get_cmd1 = GetCommand {
            key: "key1".to_string(),
        };

        let get_result1 = get_cmd1.execute(&mut store);
        assert!(get_result1.is_ok(), "GetCommand failed: {:?}", get_result1);
        assert_eq!(
            get_result1.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };

        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(
            get_result2.unwrap(),
            Value::Str(ArcBytes::from_str("value2"))
        );
    }

    #[test]
    fn test_mget() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду MSetCommand для нескольких ключей
        let mset_cmd = MSetCommand {
            entries: vec![
                ("key1".to_string(), Value::Str(ArcBytes::from_str("value1"))),
                ("key2".to_string(), Value::Str(ArcBytes::from_str("value2"))),
            ],
        };
        mset_cmd.execute(&mut store).unwrap();

        // Создаем команду MGetCommand
        let mget_cmd = MGetCommand {
            keys: vec!["key1".to_string(), "key2".to_string()],
        };

        // Выполнение команды mget
        let result = mget_cmd.execute(&mut store);
        assert!(result.is_ok(), "MGetCommand failed: {:?}", result);

        // Проверка, что возвращается список с нужными значениями
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

    #[test]
    fn test_rename() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Создаем команду RenameCommand
        let rename_cmd = RenameCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Выполнение команды rename
        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameCommand failed: {:?}", result);

        // Проверка, что ключ был переименован
        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        // Проверка, что старый ключ больше не существует
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

    #[test]
    fn test_renamenx() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Создаем команду RenameNxCommand
        let rename_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Выполнение команды renamenx
        let result = rename_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1)); // Успех

        // Проверка, что новый ключ существует
        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(
            get_result.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );

        // Проверка, что старый ключ больше не существует
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

    #[test]
    fn test_rename_nx_key_not_exists() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду SetCommand
        let set_cmd = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Создаем команду RenameNxCommand
        let rename_nx_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        // Выполнение команды rename (ключа "key2" еще нет)
        let result = rename_nx_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1)); // Переименование прошло успешно

        // Проверка, что старый ключ больше не существует
        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null); // Ключ удален

        // Проверка, что новый ключ существует
        let get_cmd_new = GetCommand {
            key: "key2".to_string(),
        };
        let get_result_new = get_cmd_new.execute(&mut store);
        assert!(
            get_result_new.is_ok(),
            "GetCommand failed: {:?}",
            get_result_new
        );
        assert_eq!(
            get_result_new.unwrap(),
            Value::Str(ArcBytes::from_str("value1"))
        );
    }

    #[test]
    fn test_flushdb() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Добавляем несколько ключей
        let set_cmd1 = SetCommand {
            key: "key1".to_string(),
            value: Value::Str(ArcBytes::from_str("value1")),
        };
        set_cmd1.execute(&mut store).unwrap();

        let set_cmd2 = SetCommand {
            key: "key2".to_string(),
            value: Value::Str(ArcBytes::from_str("value2")),
        };
        set_cmd2.execute(&mut store).unwrap();

        // Создаем команду FlushDbCommand
        let flush_cmd = FlushDbCommand;

        // Выполнение команды flushdb
        let result = flush_cmd.execute(&mut store);
        assert!(result.is_ok(), "FlushDbCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Str(ArcBytes::from_str("OK")));

        // Проверка, что все ключи были удалены
        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {:?}", get_result);
        assert_eq!(get_result.unwrap(), Value::Null); // Ключ "key1" удален

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };
        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {:?}", get_result2);
        assert_eq!(get_result2.unwrap(), Value::Null); // Ключ "key2" удален
    }
}
