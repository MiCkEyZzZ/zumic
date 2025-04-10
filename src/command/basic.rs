use crate::{
    database::{types::Value, ArcBytes},
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
        let result = store.get(ArcBytes::from_str(self.key.as_str()))?;
        Ok(result.unwrap_or(Value::Null))
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
    pub key: String,
}

impl CommandExecute for ExistsCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let exists = store.get(ArcBytes::from_str(&self.key))?.is_some();
        Ok(Value::Int(if exists { 1 } else { 0 }))
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

#[cfg(test)]
mod tests {
    use crate::{
        command::{CommandExecute, DelCommand, ExistsCommand, GetCommand, SetNxCommand},
        database::{ArcBytes, Value},
        engine::{engine::StorageEngine, memory::InMemoryStore},
    };

    use super::SetCommand;

    #[test]
    fn test_set_and_get() {
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

        // Создаем команду GetCommand
        let get_cmd = GetCommand {
            key: "test_key".to_string(),
        };

        // Выполнение команды get
        let result = get_cmd.execute(&mut store);
        assert!(result.is_ok(), "GetCommand failed: {:?}", result);

        // Проверка, что значение совпадает
        assert_eq!(
            result.unwrap(),
            Value::Str(ArcBytes::from_str("test_value"))
        );
    }

    #[test]
    fn test_get_non_existent_key() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду GetCommand с несуществующим ключом
        let get_command = GetCommand {
            key: "non_existent_key".to_string(),
        };

        // Выполнение команды get
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
    fn test_exists_key() {
        // Инициализация хранилища
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        // Создаем команду ExistsCommand
        let exists_cmd = ExistsCommand {
            key: "test_key".to_string(),
        };

        // Убедимся, что ключ не существует до его добавления
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok(), "ExistsCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(0)); // Ключ не существует

        // Добавляем ключ в хранилище
        let set_cmd = SetCommand {
            key: "test_key".to_string(),
            value: Value::Str(ArcBytes::from_str("test_value")),
        };
        set_cmd.execute(&mut store).unwrap();

        // Проверяем, что ключ теперь существует
        let result = exists_cmd.execute(&mut store);
        assert!(result.is_ok(), "ExistsCommand failed: {:?}", result);
        assert_eq!(result.unwrap(), Value::Int(1)); // Ключ существует
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
}
