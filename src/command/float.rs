//! Команды для работы с числами с плавающей точкой (float) в Zumic.
//!
//! Реализует команды INCRBYFLOAT, DECRBYFLOAT, SETFLOAT для изменения и
//! установки float-значений по ключу.
//! Каждая команда реализует трейт [`CommandExecute`].

use crate::{CommandExecute, Sds, StorageEngine, StoreError, Value};

/// Команда INCRBYFLOAT — увеличивает значение float по ключу на заданное число.
///
/// # Поля
/// * `key` — ключ, значение которого увеличивается.
/// * `increment` — на сколько увеличить значение.
#[derive(Debug)]
pub struct IncrByFloatCommand {
    pub key: String,
    pub increment: f64,
}

impl CommandExecute for IncrByFloatCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

        match store.get(&key_bytes)? {
            Some(Value::Float(current)) => {
                let new_value = current + self.increment;
                store.set(&key_bytes, Value::Float(new_value))?;
                Ok(Value::Float(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(&key_bytes, Value::Float(self.increment))?;
                Ok(Value::Float(self.increment))
            }
        }
    }
}

/// Команда DECRBYFLOAT — уменьшает значение float по ключу на заданное число.
///
/// # Поля
/// * `key` — ключ, значение которого уменьшается.
/// * `decrement` — на сколько уменьшить значение.
#[derive(Debug)]
pub struct DecrByFloatCommand {
    pub key: String,
    pub decrement: f64,
}

impl CommandExecute for DecrByFloatCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);

        match store.get(&key_bytes)? {
            Some(Value::Float(current)) => {
                let new_value = current - self.decrement;
                store.set(&key_bytes, Value::Float(new_value))?;
                Ok(Value::Float(new_value))
            }
            Some(_) => Err(StoreError::InvalidType),
            None => {
                store.set(&key_bytes, Value::Float(-self.decrement))?;
                Ok(Value::Float(-self.decrement))
            }
        }
    }
}

/// Команда SETFLOAT — устанавливает значение float по ключу.
///
/// # Поля
/// * `key` — ключ, в который сохраняется значение.
/// * `value` — сохраняемое значение.
#[derive(Debug)]
pub struct SetFloatCommand {
    pub key: String,
    pub value: f64,
}

impl CommandExecute for SetFloatCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let key_bytes = Sds::from_str(&self.key);
        store.set(&key_bytes, Value::Float(self.value))?;
        Ok(Value::Float(self.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    /// Проверяет, что `IncrByFloatCommand` корректно увеличивает значение float.
    /// Исходное значение ключа — 10.0. После увеличения на 5.5 ожидаем 15.5.
    #[test]
    fn test_incr_by_float() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        store
            .set(&Sds::from_str("key1"), Value::Float(10.0))
            .unwrap();

        let cmd = IncrByFloatCommand {
            key: "key1".to_string(),
            increment: 5.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(15.5));
    }

    /// Проверяет, что `DecrByFloatCommand` корректно уменьшает значение float.
    /// Исходное значение ключа — 10.0. После уменьшения на 3.5 ожидаем 6.5.
    #[test]
    fn test_decr_by_float() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        store
            .set(&Sds::from_str("key1"), Value::Float(10.0))
            .unwrap();

        let cmd = DecrByFloatCommand {
            key: "key1".to_string(),
            decrement: 3.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(6.5));
    }

    /// Проверяет, что `SetFloatCommand` устанавливает значение типа float в хранилище.
    /// Устанавливаем значение 20.5 под ключом "key1", затем проверяем результат.
    #[test]
    fn test_set_float() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

        let cmd = SetFloatCommand {
            key: "key1".to_string(),
            value: 20.5,
        };
        let result = cmd.execute(&mut store);
        assert_eq!(result.unwrap(), Value::Float(20.5));
    }
}
