use crate::{
    engine::engine::StorageEngine,
    StoreError, {Sds, Value},
};

use super::CommandExecute;

#[derive(Debug)]
pub struct IncrCommand {
    pub key: String,
}

impl CommandExecute for IncrCommand {
    fn execute(&self, store: &mut StorageEngine) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

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
        let keys_bytes = Sds::from_str(&self.key);

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
        let key_bytes = Sds::from_str(&self.key);

        match store.get(key_bytes.clone())? {
            Some(Value::Int(current)) => {
                let new_value = current - 1;
                store.set(key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(key_bytes, Value::Int(-1))?;
                Ok(Value::Int(-1)) // Если ключ не существует, установите его равным -1
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
        let key_bytes = Sds::from_str(&self.key);

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
    use crate::{database::Sds, engine::memory::InMemoryStore};

    use super::*;

    /// Тест команды `INCR`:
    /// - Если ключ не существует, он должен быть установлен в 1.
    /// - Если ключ существует и его значение — целое число, оно должно быть увеличено на 1.
    #[test]
    fn test_incr_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let incr_command = IncrCommand {
            key: "counter".to_string(),
        };

        // Тест, когда ключ не существует (должен быть установлен в 1).
        let result = incr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(1));

        // Тест, когда ключ существует (должен быть увеличен до 2).
        let result = incr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(2));
    }

    /// Тест команды `INCRBY`:
    /// - Если ключ не существует, он должен быть создан с заданным значением увеличения.
    /// - Если ключ существует и его значение — целое число, оно должно быть увеличено на указанную величину.
    #[test]
    fn test_incrby_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let incr_by_command = IncrByCommand {
            key: "counter".to_string(),
            increment: 5,
        };

        // Тест, когда ключ не существует (должен быть установлен в 5).
        let result = incr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(5));

        // Тест, когда ключ существует (должен быть увеличен на 5, итоговое значение — 10).
        let result = incr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(10));
    }

    /// Тест команды `DECR`:
    /// - Если ключ не существует, он должен быть создан со значением -1.
    /// - Если ключ существует и его значение — целое число, оно должно быть уменьшено на 1.
    #[test]
    fn test_decr_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let decr_command = DecrCommand {
            key: "counter".to_string(),
        };

        // Тест, когда ключ не существует (должен быть установлен в -1).
        let result = decr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-1));

        // Тест, когда ключ существует (должен быть уменьшен до -2).
        let result = decr_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-2));
    }

    /// Тест команды `DECRBY`:
    /// - Если ключ не существует, он должен быть создан с отрицательным значением уменьшения.
    /// - Если ключ существует и его значение — целое число, оно должно быть уменьшено на указанную величину.
    #[test]
    fn test_decrby_command() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());

        let decr_by_command = DecrByCommand {
            key: "counter".to_string(),
            decrement: 3,
        };

        // Тест, когда ключ не существует (должен быть установлен в -3).
        let result = decr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-3));

        // Тест, когда ключ существует (должен быть уменьшен на 3, итоговое значение — -6).
        let result = decr_by_command.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Int(-6));
    }

    /// Тест на неверный тип для команды `INCR`:
    /// - Если ключ существует, но его значение не является целым числом, команда должна вернуть ошибку.
    #[test]
    fn test_invalid_type_for_incr() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(
                Sds::from_str("counter"),
                Value::Str(Sds::from_str("string")),
            )
            .unwrap();

        let incr_command = IncrCommand {
            key: "counter".to_string(),
        };

        let result = incr_command.execute(&mut store);
        assert!(result.is_err()); // Должна быть ошибка InvalidType
    }

    /// Тест на неверный тип для команды `DECR`:
    /// - Если ключ существует, но его значение не является целым числом, команда должна вернуть ошибку.
    #[test]
    fn test_invalid_type_for_decr() {
        let mut store = StorageEngine::InMemory(InMemoryStore::new());
        store
            .set(
                Sds::from_str("counter"),
                Value::Str(Sds::from_str("string")),
            )
            .unwrap();

        let decr_command = DecrCommand {
            key: "counter".to_string(),
        };

        let result = decr_command.execute(&mut store);
        assert!(result.is_err()); // Должна быть ошибка InvalidType
    }
}
