use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

/// Команда `INCR key`.
///
/// Увеличивает целочисленное значение по ключу на 1.
/// Если ключ не существует, создаётся со значением 1.
/// Возвращает новое значение или ошибку `InvalidType`
/// если под этим ключом лежит не целое число.
#[derive(Debug)]
pub struct IncrCommand {
    /// Ключ, значение по которому нужно увеличить
    pub key: String,
}

impl CommandExecute for IncrCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

        match store.get(&key_bytes)? {
            Some(Value::Int(current)) => {
                // Существующее целочисленное значение — увеличиваем
                let new_value = current + 1;
                store.set(&key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                // Ключа нет — создаём со значением 1
                store.set(&key_bytes, Value::Int(1))?;
                Ok(Value::Int(1))
            }
        }
    }
}

/// Команда `INCRBY key increment`.
///
/// Увеличивает целочисленное значение по ключу на `increment`.
/// Если ключа нет — создаёт со значением `increment`.
/// Ошибка `InvalidType`, если по этому ключу лежит не целое число.
#[derive(Debug)]
pub struct IncrByCommand {
    /// Ключ, значение по которому увеличиваем
    pub key: String,
    /// Величина прироста
    pub increment: i64,
}

impl CommandExecute for IncrByCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let keys_bytes = Sds::from_str(&self.key);

        match store.get(&keys_bytes)? {
            Some(Value::Int(current)) => {
                // Существующее целочисленное значение — увеличиваем на increment
                let new_value = current + self.increment;
                store.set(&keys_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                // Ключа нет — создаём со значением increment
                store.set(&keys_bytes, Value::Int(self.increment))?;
                Ok(Value::Int(self.increment))
            }
        }
    }
}

/// Команда `DECR key`.
///
/// Уменьшает целочисленное значение под `key` на 1.
/// - Если ключ не существует, создаётся со значением -1.
/// - Если значение под ключом — не `Int`, возвращает `StoreError::InvalidType`.
#[derive(Debug)]
pub struct DecrCommand {
    /// Ключ, значение по которому нужно уменьшить.
    pub key: String,
}

impl CommandExecute for DecrCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

        match store.get(&key_bytes)? {
            Some(Value::Int(current)) => {
                // Существующее целочисленное значение — уменьшаем
                let new_value = current - 1;
                store.set(&key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                // Если ключ не существует, установите его равным -1
                store.set(&key_bytes, Value::Int(-1))?;
                Ok(Value::Int(-1))
            }
        }
    }
}

/// Команда `DECRBY key decrement`.
///
/// Уменьшает целочисленное значение под `key` на `decrement`.
/// - Если ключ не существует, создаётся со значением `-decrement`.
/// - Если значение под ключом — не `Int`, возвращает `StoreError::InvalidType`.
#[derive(Debug)]
pub struct DecrByCommand {
    /// Ключ, значение по которому нужно уменьшить.
    pub key: String,
    /// Величина уменьшения.
    pub decrement: i64,
}

impl CommandExecute for DecrByCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

        match store.get(&key_bytes)? {
            Some(Value::Int(current)) => {
                // Существующее целочисленное значение — уменьшаем на decrement
                let new_value = current - self.decrement;
                store.set(&key_bytes, Value::Int(new_value))?;
                Ok(Value::Int(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                // Ключа нет — создаём со значением -decrement
                store.set(&key_bytes, Value::Int(-self.decrement))?;
                Ok(Value::Int(-self.decrement))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryStore, Sds};

    /// Тест команды `INCR`:
    /// - Если ключ не существует, он должен быть установлен в 1.
    /// - Если ключ существует и его значение — целое число, оно должно быть увеличено на 1.
    #[test]
    fn test_incr_command() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        let mut store = StorageEngine::Memory(InMemoryStore::new());

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
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        store
            .set(
                &Sds::from_str("counter"),
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
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        store
            .set(
                &Sds::from_str("counter"),
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
