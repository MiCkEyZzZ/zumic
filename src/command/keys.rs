use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

/// Команда DEL — удаляет значение по ключу.
#[derive(Debug)]
pub struct DelCommand {
    pub key: String,
}

impl CommandExecute for DelCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let deleted = store.del(&Sds::from_str(&self.key))?;
        Ok(Value::Bool(deleted))
    }

    fn command_name(&self) -> &'static str {
        "DEL"
    }
}

/// Команда EXISTS — проверяет существование одного или нескольких ключей.
#[derive(Debug)]
pub struct ExistsCommand {
    pub keys: Vec<String>,
}

impl CommandExecute for ExistsCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let count = self
            .keys
            .iter()
            .map(|key| Sds::from_str(key))
            .filter_map(|key| store.get(&key).ok())
            .filter(|value| value.is_some())
            .count();

        Ok(Value::Int(count as i64))
    }

    fn command_name(&self) -> &'static str {
        "EXISTS"
    }
}

/// Команда RENAME — переименовывает существующий ключ.
#[derive(Debug)]
pub struct RenameCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        store.rename(&Sds::from_str(&self.from), &Sds::from_str(&self.to))?;
        Ok(Value::Str(Sds::from_str("")))
    }

    fn command_name(&self) -> &'static str {
        "RENAME"
    }
}

/// Команда RENAMENX — переименовывает ключ, только если новый ключ не
/// существует.
#[derive(Debug)]
pub struct RenameNxCommand {
    pub from: String,
    pub to: String,
}

impl CommandExecute for RenameNxCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let success = store.renamenx(&Sds::from_str(&self.from), &Sds::from_str(&self.to))?;
        Ok(Value::Int(if success { 1 } else { 0 }))
    }

    fn command_name(&self) -> &'static str {
        "RENAMENX"
    }
}

/// Команда FLUSHDB — удаляет все ключи из текущей базы данных.
#[derive(Debug)]
pub struct FlushDbCommand;

impl CommandExecute for FlushDbCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        store.flushdb()?;
        Ok(Value::Str(Sds::from_str("OK")))
    }

    fn command_name(&self) -> &'static str {
        "FLUSHDB"
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GetCommand, InMemoryStore, SetCommand, Value};

    // Вспомогательная функция для создания нового хранилища в памяти.
    fn create_store() -> StorageEngine {
        StorageEngine::Memory(InMemoryStore::new())
    }

    /// Тестирование `DelCommand` для существующего ключа.
    /// Проверяет, что удаление возвращает 1 и ключ действительно удалён.
    #[test]
    fn test_del_existing_key() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: Value::Str(Sds::from_str("test_value")),
        };

        let result = set_cmd.execute(&mut store);
        assert!(result.is_ok(), "SetCommand failed: {result:?}");

        let del_cmd = DelCommand {
            key: "test_key".to_string(),
        };

        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {del_result:?}");

        assert_eq!(del_result.unwrap(), Value::Bool(true));

        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {result:?}");

        assert_eq!(result.unwrap(), Value::Null);
    }

    /// Тестирование `DelCommand` для несуществующего ключа.
    #[test]
    fn test_del_non_existent_key() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

        let del_cmd = DelCommand {
            key: "non_existent_key".to_string(),
        };

        let del_result = del_cmd.execute(&mut store);
        assert!(del_result.is_ok(), "DelCommand failed: {del_result:?}");

        assert_eq!(del_result.unwrap(), Value::Bool(false));
    }

    /// Тестирование `ExistsCommand`.
    /// Проверяет, что команда правильно считает количество существующих ключей.
    #[test]
    fn test_exists_command() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        let exists_cmd = ExistsCommand { keys: vec![] };
        let result = exists_cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(0));
    }

    /// Этот тест проверяет, что `RenameCommand` работает как ожидается.
    /// Он переименовывает существующий ключ в новое имя.
    #[test]
    fn test_rename() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        assert!(result.is_ok(), "RenameCommand failed: {result:?}");

        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {get_result_old:?}"
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when
    /// renaming a key to a new key name. The `RenameNxCommand` only renames
    /// the key if the target key does not already exist. It first adds a
    /// key, executes the rename operation, and verifies that the new key exists
    /// with the original value, and that the old key is deleted
    /// successfully.
    #[test]
    fn test_renamenx() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        assert!(result.is_ok(), "RenameNxCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "key2".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Str(Sds::from_str("value1")));

        let get_cmd_old = GetCommand {
            key: "key1".to_string(),
        };
        let get_result_old = get_cmd_old.execute(&mut store);
        assert!(
            get_result_old.is_ok(),
            "GetCommand failed: {get_result_old:?}"
        );
        assert_eq!(get_result_old.unwrap(), Value::Null);
    }

    /// This test ensures that the `RenameNxCommand` works as expected when
    /// renaming a key only if the target key does not already exist.
    /// It first adds a key, attempts to rename it with `RenameNxCommand` (where
    /// the target key does not exist), and verifies that the key is
    /// successfully renamed. It then checks that the old key no longer exists
    /// and the new key is present.
    #[test]
    fn test_rename_nx_key_not_exists() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        assert!(result.is_ok(), "RenameNxCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Int(1));

        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Null);

        let get_cmd_new = GetCommand {
            key: "key2".to_string(),
        };
        let get_result_new = get_cmd_new.execute(&mut store);
        assert!(
            get_result_new.is_ok(),
            "GetCommand failed: {get_result_new:?}"
        );
        assert_eq!(get_result_new.unwrap(), Value::Str(Sds::from_str("value1")));
    }

    /// Тестирование `RenameNxCommand` когда целевой ключ уже существует.
    /// Проверяет, что команда возвращает 0 и не перезаписывает существующий
    /// ключ.
    #[test]
    fn test_rename_nx_target_exists() {
        let mut store = create_store();

        store
            .set(&Sds::from_str("key1"), Value::Str(Sds::from_str("value1")))
            .unwrap();
        store
            .set(&Sds::from_str("key2"), Value::Str(Sds::from_str("value2")))
            .unwrap();

        let rename_nx_cmd = RenameNxCommand {
            from: "key1".to_string(),
            to: "key2".to_string(),
        };

        let result = rename_nx_cmd.execute(&mut store);
        assert!(result.is_ok(), "RenameNxCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Int(0));

        // Проверяем, что key1 всё ещё существует
        let get1 = store.get(&Sds::from_str("key1"));
        assert_eq!(get1.unwrap(), Some(Value::Str(Sds::from_str("value1"))));

        // Проверяем, что key2 не изменился
        let get2 = store.get(&Sds::from_str("key2"));
        assert_eq!(get2.unwrap(), Some(Value::Str(Sds::from_str("value2"))));
    }

    /// This test ensures that the `FlushDbCommand` properly clears all keys
    /// from the database. It first adds two keys, executes the flush
    /// command, and then checks that both keys have been removed.
    #[test]
    fn test_flushdb() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        assert!(result.is_ok(), "FlushDbCommand failed: {result:?}");
        assert_eq!(result.unwrap(), Value::Str(Sds::from_str("OK")));

        let get_cmd = GetCommand {
            key: "key1".to_string(),
        };
        let get_result = get_cmd.execute(&mut store);
        assert!(get_result.is_ok(), "GetCommand failed: {get_result:?}");
        assert_eq!(get_result.unwrap(), Value::Null);

        let get_cmd2 = GetCommand {
            key: "key2".to_string(),
        };
        let get_result2 = get_cmd2.execute(&mut store);
        assert!(get_result2.is_ok(), "GetCommand failed: {get_result2:?}");
        assert_eq!(get_result2.unwrap(), Value::Null);
    }
}
