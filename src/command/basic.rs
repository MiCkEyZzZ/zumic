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

#[cfg(test)]
mod tests {
    use crate::{
        command::{CommandExecute, GetCommand},
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
}
